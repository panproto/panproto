//! GAT operations: theory construction, colimit, morphism checking,
//! model migration, free-model construction, and model checking.
//!
//! Ported from `panproto_wasm::api::gat` (see
//! `crates/panproto-wasm/src/api/gat.rs`) to the panproto-c conventions:
//! CBOR via [`crate::canonical`], errors as [`FfiError`], handles via the
//! [`crate::handle`] slab, and panic-safe entry points wrapped in
//! [`crate::panic::guard`].
//!
//! Theories live in the slab as [`Resource::Theory`]; free models live as
//! [`Resource::Model`]. A [`gat::Model`] carries operation
//! interpretations as closures (`Arc<dyn Fn(...)>`), which stay
//! in-process and never cross the boundary as data. The model is still
//! fully evaluable and its carrier is extractable across the boundary:
//! [`pp_gat_eval_in_model`] runs an operation in the model and returns
//! the resulting [`gat::ModelValue`], and [`pp_gat_model_sort_interp`]
//! emits the model's full carrier (its `sort_interp` map). The remaining
//! entry points exchange CBOR payloads.

use std::collections::HashMap;

use panproto_core::gat::{self, FreeModelConfig};
use safer_ffi::prelude::*;
use serde::Deserialize;

use crate::api::helpers::MorphismCheckResult;
use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource, with_three_resources, with_two_resources};
use crate::panic::guard;

/// CBOR config payload for [`pp_gat_free_model`].
///
/// Mirrors the public fields of [`gat::FreeModelConfig`]. Each field is
/// `serde(default)`, so an empty or partial payload falls back to the
/// engine defaults (`max_depth = 3`, `max_terms_per_sort = 1000`).
#[derive(Debug, Deserialize)]
struct FreeModelConfigSpec {
    /// Maximum depth of term generation.
    #[serde(default = "default_max_depth")]
    max_depth: usize,
    /// Maximum number of terms per sort (safety bound).
    #[serde(default = "default_max_terms_per_sort")]
    max_terms_per_sort: usize,
}

fn default_max_depth() -> usize {
    FreeModelConfig::default().max_depth
}

fn default_max_terms_per_sort() -> usize {
    FreeModelConfig::default().max_terms_per_sort
}

impl Default for FreeModelConfigSpec {
    fn default() -> Self {
        let cfg = FreeModelConfig::default();
        Self {
            max_depth: cfg.max_depth,
            max_terms_per_sort: cfg.max_terms_per_sort,
        }
    }
}

impl From<FreeModelConfigSpec> for FreeModelConfig {
    fn from(spec: FreeModelConfigSpec) -> Self {
        Self {
            max_depth: spec.max_depth,
            max_terms_per_sort: spec.max_terms_per_sort,
        }
    }
}

/// Create a GAT theory from a CBOR spec.
///
/// `spec` is a CBOR-encoded [`gat::Theory`]. On success, `out_handle`
/// receives a fresh [`Resource::Theory`] handle and [`PpStatus::Ok`] is
/// returned. On CBOR decode failure, [`PpStatus::Serialization`] is
/// returned and `out_handle` is left untouched.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_create_theory(spec: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let theory: gat::Theory = crate::canonical::decode(spec.as_slice())?;
        *out_handle = handle::alloc(Resource::Theory(Box::new(theory)));
        Ok(PpStatus::Ok)
    })
}

/// Compute the colimit of two theories over a shared base.
///
/// `t1`, `t2`, and `shared` are [`Resource::Theory`] handles. On
/// success, `out_handle` receives a fresh [`Resource::Theory`] handle
/// holding `gat::colimit_by_name(t1, t2, shared)`.
///
/// Returns [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] if a
/// handle is invalid or not a theory, and [`PpStatus::Operation`] if the
/// colimit fails.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_colimit(t1: u32, t2: u32, shared: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let result = with_three_resources(t1, t2, shared, |r1, r2, r3| {
            let th1 = r1.as_theory()?;
            let th2 = r2.as_theory()?;
            let th_shared = r3.as_theory()?;
            gat::colimit_by_name(th1, th2, th_shared)
                .map_err(|e| FfiError::Operation(format!("colimit failed: {e}")))
        })?;
        *out_handle = handle::alloc(Resource::Theory(Box::new(result)));
        Ok(PpStatus::Ok)
    })
}

/// Check the validity of a theory morphism.
///
/// `morphism` is a CBOR-encoded [`gat::TheoryMorphism`]; `domain` and
/// `codomain` are [`Resource::Theory`] handles. On success, `out`
/// receives a CBOR-encoded [`MorphismCheckResult`]. The result itself
/// encodes validity, so the entry point returns [`PpStatus::Ok`] for
/// both a valid and an invalid morphism; only a malformed payload or a
/// bad handle yields a non-`Ok` status. Calls `gat::check_morphism`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_check_morphism(
    morphism: c_slice::Ref<'_, u8>,
    domain: u32,
    codomain: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let morph: gat::TheoryMorphism = crate::canonical::decode(morphism.as_slice())?;

        let result = with_two_resources(domain, codomain, |r1, r2| {
            let dom = r1.as_theory()?;
            let cod = r2.as_theory()?;
            Ok(match gat::check_morphism(&morph, dom, cod) {
                Ok(()) => MorphismCheckResult {
                    valid: true,
                    error: None,
                },
                Err(e) => MorphismCheckResult {
                    valid: false,
                    error: Some(e.to_string()),
                },
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Migrate a model through a theory morphism.
///
/// `model` is a CBOR-encoded sort-interpretation map
/// (`HashMap<String, Vec<ModelValue>>`); `morphism` is a CBOR-encoded
/// [`gat::TheoryMorphism`]. On success, `out` receives the CBOR-encoded
/// reindexed sort interpretations.
///
/// Only sort interpretations cross the boundary. A [`gat::Model`] also
/// carries operation interpretations as closures (`Arc<dyn Fn(...)>`),
/// which cannot be serialized; reindexing those is the host's
/// responsibility once the sorts have moved. The reindex mirrors
/// `panproto_wasm::api::gat::migrate_model`: for each `(domain_sort,
/// codomain_sort)` entry in the morphism's `sort_map`, the codomain
/// sort's carrier is copied to the domain sort name.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_migrate_model(
    model: c_slice::Ref<'_, u8>,
    morphism: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let morph: gat::TheoryMorphism = crate::canonical::decode(morphism.as_slice())
            .map_err(|e| FfiError::Serialization(format!("morphism: {e}")))?;

        // Models contain function pointers and cannot be fully
        // serialized. Only the sort_interp portion crosses the boundary.
        let sort_interp: HashMap<String, Vec<gat::ModelValue>> =
            crate::canonical::decode(model.as_slice())
                .map_err(|e| FfiError::Serialization(format!("model sort_interp: {e}")))?;

        // Reindex sort interpretations according to the morphism's
        // sort_map: each domain sort takes the carrier of the codomain
        // sort it maps to.
        let mut reindexed: HashMap<String, Vec<gat::ModelValue>> = HashMap::new();
        for (domain_sort, codomain_sort) in &morph.sort_map {
            if let Some(values) = sort_interp.get(codomain_sort.as_ref()) {
                reindexed.insert(domain_sort.to_string(), values.clone());
            }
        }

        *out = crate::canonical::encode(&reindexed)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Construct a bounded approximation of the free (initial) model of a
/// theory.
///
/// `theory` is a [`Resource::Theory`] handle; `config` is an optional
/// CBOR-encoded free-model config (`{max_depth, max_terms_per_sort}`); an
/// empty slice selects the engine defaults. On success, `out_handle`
/// receives a fresh [`Resource::Model`] handle holding the constructed
/// model.
///
/// The model is held by handle rather than serialized: a model's
/// operation interpretations are closures (`Arc<dyn Fn(...)>`) that
/// cannot cross the ABI. Calls `gat::free_model`.
///
/// Returns [`PpStatus::Serialization`] on a malformed config payload,
/// [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] for a bad
/// theory handle, and [`PpStatus::Operation`] when free-model
/// construction fails (a cyclic sort dependency or an exceeded term
/// bound).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_free_model(theory: u32, config: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let spec: FreeModelConfigSpec = if config.as_slice().is_empty() {
            FreeModelConfigSpec::default()
        } else {
            crate::canonical::decode(config.as_slice())?
        };
        let cfg: FreeModelConfig = spec.into();

        let model = handle::with_resource(theory, |r| {
            let theory = r.as_theory()?;
            gat::free_model(theory, &cfg)
                .map(|result| result.model)
                .map_err(|e| FfiError::Operation(format!("free_model: {e}")))
        })?;

        *out_handle = handle::alloc(Resource::Model(Box::new(model)));
        Ok(PpStatus::Ok)
    })
}

/// Check a model against a theory, returning equation violations.
///
/// `model` is a [`Resource::Model`] handle; `theory` is a
/// [`Resource::Theory`] handle. On success, `out` receives a
/// CBOR-encoded `Vec<String>` of violation descriptions (the `Debug`
/// rendering of each `gat::EquationViolation`), empty when the model
/// satisfies every equation. A satisfied and a violated model both
/// return [`PpStatus::Ok`]; the verdict lives in the payload.
///
/// Returns [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] for a
/// bad handle, and [`PpStatus::Operation`] when checking itself fails (a
/// missing carrier set, or an assignment count that exceeds the engine
/// bound). Calls `gat::check_model`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_check_model(model: u32, theory: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let violations = with_two_resources(model, theory, |rm, rt| {
            let model = rm.as_model()?;
            let theory = rt.as_theory()?;
            gat::check_model(model, theory)
                .map_err(|e| FfiError::Operation(format!("check_model: {e}")))
        })?;

        let descriptions: Vec<String> = violations.iter().map(|v| format!("{v:?}")).collect();
        *out = crate::canonical::encode(&descriptions)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Evaluate an operation in a model and return the resulting value.
///
/// `model` is a [`Resource::Model`] handle; `op_name` is the UTF-8
/// operation name; `args` is a CBOR-encoded `Vec<ModelValue>` of
/// arguments. On success, `out` receives the CBOR-encoded
/// [`gat::ModelValue`] the operation produced. The operation's
/// interpretation (a closure held in the model) is run in-process; only
/// its inputs and output cross the boundary, so a handle-held model is
/// fully evaluable from the host. Calls [`gat::Model::eval`].
///
/// Returns [`PpStatus::Serialization`] on a malformed argument payload,
/// [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] for a bad
/// model handle, and [`PpStatus::Operation`] when the operation is not in
/// the model or its interpretation fails.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_eval_in_model(
    model: u32,
    op_name: c_slice::Ref<'_, u8>,
    args: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let op = std::str::from_utf8(op_name.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid op name: {e}")))?;
        let arg_values: Vec<gat::ModelValue> = crate::canonical::decode(args.as_slice())
            .map_err(|e| FfiError::Serialization(format!("model args: {e}")))?;

        let value = handle::with_resource(model, |r| {
            let model = r.as_model()?;
            model
                .eval(op, &arg_values)
                .map_err(|e| FfiError::Operation(format!("eval_in_model: {e}")))
        })?;

        *out = crate::canonical::encode(&value)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Emit a model's full carrier: its sort-interpretation map.
///
/// `model` is a [`Resource::Model`] handle. On success, `out` receives
/// the CBOR-encoded `HashMap<String, Vec<ModelValue>>` of the model's
/// `sort_interp`: each sort name mapped to its carrier set. This is the
/// extractable data half of a model (the operation interpretations stay
/// in-process); paired with [`pp_gat_eval_in_model`], a handle-held model
/// is both evaluable and its carrier serializable.
///
/// Returns [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] for a
/// bad model handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_model_sort_interp(model: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_resource(model, |r| {
            let model = r.as_model()?;
            crate::canonical::encode(&model.sort_interp)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Serialize the theory behind a handle to CBOR.
///
/// `theory` is a [`Resource::Theory`] handle. On success, `out` receives
/// the CBOR-encoded [`gat::Theory`] in the same shape
/// [`pp_gat_create_theory`] decodes, so an engine-produced theory (a
/// colimit result, for instance) can be reified by the host and fed back
/// in.
///
/// Returns [`PpStatus::InvalidHandle`] / [`PpStatus::TypeMismatch`] for a
/// bad handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_gat_serialize_theory(theory: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let bytes = handle::with_resource(theory, |r| {
            let theory = r.as_theory()?;
            crate::canonical::encode(theory)
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::gat::{self, Equation, Operation, Sort, Term, Theory, TheoryMorphism};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::{decode, encode};
    use crate::handle::with_resource;

    /// A pointed-set theory `{ S }` with two constants `a, b : S` and an
    /// equation `a = b`. Its free model has a single carrier element and
    /// satisfies the equation.
    fn theory_two_points_collapsed() -> Theory {
        Theory::new(
            "TwoPoints",
            vec![Sort::simple("S")],
            vec![Operation::nullary("a", "S"), Operation::nullary("b", "S")],
            vec![Equation::new(
                "a_eq_b",
                Term::constant("a"),
                Term::constant("b"),
            )],
        )
    }

    /// A two-sort theory `{ A, B }` with a single op `f : A -> B`.
    fn theory_ab() -> Theory {
        Theory::new(
            "ThAB",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            vec![],
        )
    }

    /// A single-sort theory `{ A }` used as the colimit shared base.
    fn theory_a() -> Theory {
        Theory::new("ThA", vec![Sort::simple("A")], vec![], vec![])
    }

    fn alloc_theory(t: Theory) -> u32 {
        handle::alloc(Resource::Theory(Box::new(t)))
    }

    #[test]
    fn create_theory_round_trips_through_ffi() {
        let bytes = encode(&theory_ab()).unwrap();
        let slice: c_slice::Box<u8> = bytes.into_boxed_slice().into();
        let mut h: u32 = u32::MAX;
        assert_eq!(
            pp_gat_create_theory(slice.as_ref(), &mut h),
            PpStatus::Ok as i32
        );
        assert_ne!(h, u32::MAX);

        // The handle round-trips back to the same theory shape.
        let restored = with_resource(h, |r| Ok(r.as_theory()?.clone())).unwrap();
        assert_eq!(&*restored.name, "ThAB");
        assert_eq!(restored.sorts.len(), 2);
        assert_eq!(restored.ops.len(), 1);

        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }

    #[test]
    fn create_theory_rejects_garbage_cbor() {
        let bad: Box<[u8]> = vec![0xFFu8, 0xFE, 0xFD].into_boxed_slice();
        let slice: c_slice::Box<u8> = bad.into();
        let mut h: u32 = u32::MAX;
        assert_eq!(
            pp_gat_create_theory(slice.as_ref(), &mut h),
            PpStatus::Serialization as i32
        );
        assert_eq!(h, u32::MAX);
    }

    #[test]
    fn colimit_pushes_out_over_shared_base() {
        // Colimit of ThAB and ThAB over their shared sort A is an
        // idempotent pushout: the result theory still carries both
        // sorts A and B.
        let h1 = alloc_theory(theory_ab());
        let h2 = alloc_theory(theory_ab());
        let shared = alloc_theory(theory_a());

        let mut out_h: u32 = u32::MAX;
        assert_eq!(
            pp_gat_colimit(h1, h2, shared, &mut out_h),
            PpStatus::Ok as i32
        );
        assert_ne!(out_h, u32::MAX);

        let result = with_resource(out_h, |r| Ok(r.as_theory()?.clone())).unwrap();
        assert!(result.sorts.iter().any(|s| &*s.name == "A"));
        assert!(result.sorts.iter().any(|s| &*s.name == "B"));

        for h in [h1, h2, shared, out_h] {
            assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        }
    }

    #[test]
    fn colimit_rejects_non_theory_handle() {
        use panproto_core::schema::Protocol;

        let proto = handle::alloc(Resource::Protocol(Box::<Protocol>::default()));
        let shared = alloc_theory(theory_a());
        let t = alloc_theory(theory_ab());

        let mut out_h: u32 = u32::MAX;
        assert_eq!(
            pp_gat_colimit(proto, t, shared, &mut out_h),
            PpStatus::TypeMismatch as i32
        );

        for h in [proto, shared, t] {
            assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        }
    }

    #[test]
    fn check_morphism_reports_valid_identity() {
        let theory = theory_ab();
        let morph = TheoryMorphism::identity(&theory);

        let dom = alloc_theory(theory.clone());
        let cod = alloc_theory(theory);

        let m_bytes = encode(&morph).unwrap();
        let m_slice: c_slice::Box<u8> = m_bytes.into_boxed_slice().into();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_check_morphism(m_slice.as_ref(), dom, cod, &mut out),
            PpStatus::Ok as i32
        );

        let result: MorphismCheckResult = decode(&out).unwrap();
        assert!(result.valid, "identity morphism should be valid");
        assert!(result.error.is_none());

        pp_buf_free(out);
        for h in [dom, cod] {
            assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        }
    }

    #[test]
    fn check_morphism_reports_invalid_when_sort_missing() {
        // A morphism that claims to map sort B to a sort the codomain
        // does not have is invalid; the result encodes valid = false
        // (and the entry point still returns Ok).
        let dom_theory = theory_ab();
        let cod_theory = theory_a();

        let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        sort_map.insert(Arc::from("A"), Arc::from("A"));
        sort_map.insert(Arc::from("B"), Arc::from("Nonexistent"));
        let mut op_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        op_map.insert(Arc::from("f"), Arc::from("f"));
        let morph = TheoryMorphism::new("bad", "ThAB", "ThA", sort_map, op_map);

        let dom = alloc_theory(dom_theory);
        let cod = alloc_theory(cod_theory);

        let m_bytes = encode(&morph).unwrap();
        let m_slice: c_slice::Box<u8> = m_bytes.into_boxed_slice().into();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_check_morphism(m_slice.as_ref(), dom, cod, &mut out),
            PpStatus::Ok as i32
        );

        let result: MorphismCheckResult = decode(&out).unwrap();
        assert!(!result.valid, "morphism with dangling sort should fail");
        assert!(result.error.is_some());

        pp_buf_free(out);
        for h in [dom, cod] {
            assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        }
    }

    #[test]
    fn migrate_model_reindexes_sort_interp() {
        // Morphism maps domain sort "A" to codomain sort "X" and "B" to
        // "Y". The reindexed carrier of each domain sort is the codomain
        // carrier of the sort it maps to.
        let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        sort_map.insert(Arc::from("A"), Arc::from("X"));
        sort_map.insert(Arc::from("B"), Arc::from("Y"));
        let morph = TheoryMorphism::new("m", "Dom", "Cod", sort_map, HashMap::new());

        let mut sort_interp: HashMap<String, Vec<gat::ModelValue>> = HashMap::new();
        sort_interp.insert(
            "X".into(),
            vec![gat::ModelValue::Int(1), gat::ModelValue::Int(2)],
        );
        sort_interp.insert("Y".into(), vec![gat::ModelValue::Str("hello".into())]);
        // A codomain sort not referenced by the morphism is dropped.
        sort_interp.insert("Z".into(), vec![gat::ModelValue::Bool(true)]);

        let model_bytes = encode(&sort_interp).unwrap();
        let morph_bytes = encode(&morph).unwrap();
        let model_slice: c_slice::Box<u8> = model_bytes.into_boxed_slice().into();
        let morph_slice: c_slice::Box<u8> = morph_bytes.into_boxed_slice().into();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_migrate_model(model_slice.as_ref(), morph_slice.as_ref(), &mut out),
            PpStatus::Ok as i32
        );

        let reindexed: HashMap<String, Vec<gat::ModelValue>> = decode(&out).unwrap();
        assert_eq!(
            reindexed.get("A"),
            Some(&vec![gat::ModelValue::Int(1), gat::ModelValue::Int(2)])
        );
        assert_eq!(
            reindexed.get("B"),
            Some(&vec![gat::ModelValue::Str("hello".into())])
        );
        // "Z" had no domain preimage, so it is absent from the output.
        assert!(!reindexed.contains_key("Z"));

        pp_buf_free(out);
    }

    #[test]
    fn migrate_model_rejects_garbage_morphism() {
        let sort_interp: HashMap<String, Vec<gat::ModelValue>> = HashMap::new();
        let model_bytes = encode(&sort_interp).unwrap();
        let model_slice: c_slice::Box<u8> = model_bytes.into_boxed_slice().into();
        let bad_morph: Box<[u8]> = vec![0xFFu8, 0xFE, 0xFD].into_boxed_slice();
        let morph_slice: c_slice::Box<u8> = bad_morph.into();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_migrate_model(model_slice.as_ref(), morph_slice.as_ref(), &mut out),
            PpStatus::Serialization as i32
        );
        pp_buf_free(out);
    }

    // ── pp_gat_free_model + pp_gat_check_model ─────────────────────────

    #[test]
    fn free_model_with_default_config_builds_model_handle() {
        let theory_h = alloc_theory(theory_two_points_collapsed());

        // Empty config slice selects the engine defaults.
        let mut model_h: u32 = u32::MAX;
        let empty: c_slice::Box<u8> = Vec::new().into_boxed_slice().into();
        assert_eq!(
            pp_gat_free_model(theory_h, empty.as_ref(), &mut model_h),
            PpStatus::Ok as i32
        );
        assert_ne!(model_h, u32::MAX);

        // The handle resolves to a Model whose single carrier is the
        // collapsed `{a, b}` class.
        let sort_keys = with_resource(model_h, |r| {
            let m = r.as_model()?;
            let carrier = m
                .sort_interp
                .get("S")
                .cloned()
                .ok_or_else(|| FfiError::Operation("no carrier S".to_owned()))?;
            Ok(carrier.len())
        })
        .unwrap();
        assert_eq!(sort_keys, 1, "a = b collapses S to a single class");

        assert_eq!(pp_handle_free(model_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn free_model_honors_explicit_config() {
        let theory_h = alloc_theory(theory_two_points_collapsed());

        let cfg = encode(&serde_json::json!({
            "max_depth": 1,
            "max_terms_per_sort": 100
        }))
        .unwrap();
        let cfg_slice: c_slice::Box<u8> = cfg.into_boxed_slice().into();

        let mut model_h: u32 = u32::MAX;
        assert_eq!(
            pp_gat_free_model(theory_h, cfg_slice.as_ref(), &mut model_h),
            PpStatus::Ok as i32
        );
        assert_ne!(model_h, u32::MAX);

        assert_eq!(pp_handle_free(model_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn free_model_rejects_non_theory_handle() {
        let schema = handle::alloc(Resource::Protocol(
            Box::<panproto_core::schema::Protocol>::default(),
        ));
        let mut model_h: u32 = u32::MAX;
        let empty: c_slice::Box<u8> = Vec::new().into_boxed_slice().into();
        assert_eq!(
            pp_gat_free_model(schema, empty.as_ref(), &mut model_h),
            PpStatus::TypeMismatch as i32
        );
        assert_eq!(pp_handle_free(schema), PpStatus::Ok as i32);
    }

    #[test]
    fn check_model_of_free_model_reports_no_violations() {
        // The free model of a theory satisfies that theory's equations by
        // construction, so check_model returns an empty violation list.
        let theory_h = alloc_theory(theory_two_points_collapsed());

        let mut model_h: u32 = u32::MAX;
        let empty: c_slice::Box<u8> = Vec::new().into_boxed_slice().into();
        assert_eq!(
            pp_gat_free_model(theory_h, empty.as_ref(), &mut model_h),
            PpStatus::Ok as i32
        );

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_check_model(model_h, theory_h, &mut out),
            PpStatus::Ok as i32
        );
        let violations: Vec<String> = decode(&out).unwrap();
        assert!(
            violations.is_empty(),
            "free model should satisfy its theory, got {violations:?}"
        );
        pp_buf_free(out);

        assert_eq!(pp_handle_free(model_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn check_model_rejects_non_model_handle() {
        let theory_h = alloc_theory(theory_two_points_collapsed());
        // A theory handle passed where a model is expected is a mismatch.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_check_model(theory_h, theory_h, &mut out),
            PpStatus::TypeMismatch as i32
        );
        pp_buf_free(out);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    // ── pp_gat_eval_in_model + pp_gat_model_sort_interp ────────────────

    #[test]
    fn eval_in_model_runs_a_free_model_operation() {
        // The free model of the collapsed pointed-set theory interprets
        // its two nullary constants `a` and `b` as the single collapsed
        // carrier element; evaluating either returns a sane ModelValue.
        let theory_h = alloc_theory(theory_two_points_collapsed());
        let mut model_h: u32 = u32::MAX;
        let empty: c_slice::Box<u8> = Vec::new().into_boxed_slice().into();
        assert_eq!(
            pp_gat_free_model(theory_h, empty.as_ref(), &mut model_h),
            PpStatus::Ok as i32
        );

        // Evaluate the nullary constant `a` (no arguments).
        let no_args: Vec<gat::ModelValue> = Vec::new();
        let args_bytes = encode(&no_args).unwrap();
        let args_slice: c_slice::Box<u8> = args_bytes.into_boxed_slice().into();
        let op_bytes: Box<[u8]> = b"a".to_vec().into_boxed_slice();
        let op_slice: c_slice::Box<u8> = op_bytes.into();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_eval_in_model(model_h, op_slice.as_ref(), args_slice.as_ref(), &mut out),
            PpStatus::Ok as i32
        );
        // The result decodes as a structured ModelValue (the carrier
        // element `a` is interpreted as).
        let value: gat::ModelValue = decode(&out).unwrap();
        // A free model interprets a nullary constant as a term value; it
        // is not the absent/null value.
        assert_ne!(value, gat::ModelValue::Null, "eval should yield a value");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(model_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn eval_in_model_rejects_unknown_op() {
        let _ = crate::error::take_last_error();
        let theory_h = alloc_theory(theory_two_points_collapsed());
        let mut model_h: u32 = u32::MAX;
        let empty: c_slice::Box<u8> = Vec::new().into_boxed_slice().into();
        assert_eq!(
            pp_gat_free_model(theory_h, empty.as_ref(), &mut model_h),
            PpStatus::Ok as i32
        );

        let no_args: Vec<gat::ModelValue> = Vec::new();
        let args_bytes = encode(&no_args).unwrap();
        let args_slice: c_slice::Box<u8> = args_bytes.into_boxed_slice().into();
        let op_bytes: Box<[u8]> = b"nonexistent".to_vec().into_boxed_slice();
        let op_slice: c_slice::Box<u8> = op_bytes.into();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_eval_in_model(model_h, op_slice.as_ref(), args_slice.as_ref(), &mut out),
            PpStatus::Operation as i32
        );
        pp_buf_free(out);

        assert_eq!(pp_handle_free(model_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn model_sort_interp_is_non_empty_for_a_free_model() {
        // A free model assigns a carrier set to every sort; the emitted
        // map round-trips and carries the collapsed `S` carrier.
        let theory_h = alloc_theory(theory_two_points_collapsed());
        let mut model_h: u32 = u32::MAX;
        let empty: c_slice::Box<u8> = Vec::new().into_boxed_slice().into();
        assert_eq!(
            pp_gat_free_model(theory_h, empty.as_ref(), &mut model_h),
            PpStatus::Ok as i32
        );

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_model_sort_interp(model_h, &mut out),
            PpStatus::Ok as i32
        );
        let sort_interp: HashMap<String, Vec<gat::ModelValue>> = decode(&out).unwrap();
        assert!(
            !sort_interp.is_empty(),
            "a free model has at least one carrier set"
        );
        let carrier = sort_interp.get("S").expect("carrier for sort S");
        assert_eq!(carrier.len(), 1, "a = b collapses S to a single class");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(model_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn model_sort_interp_rejects_non_model_handle() {
        let theory_h = alloc_theory(theory_two_points_collapsed());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_model_sort_interp(theory_h, &mut out),
            PpStatus::TypeMismatch as i32
        );
        pp_buf_free(out);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    // ── pp_gat_serialize_theory ────────────────────────────────────────

    #[test]
    fn serialize_theory_round_trips_through_create() {
        // Serializing an engine-produced colimit theory and feeding it
        // back through pp_gat_create_theory reconstructs the same shape.
        let h1 = alloc_theory(theory_ab());
        let h2 = alloc_theory(theory_ab());
        let shared = alloc_theory(theory_a());
        let mut colimit_h: u32 = u32::MAX;
        assert_eq!(
            pp_gat_colimit(h1, h2, shared, &mut colimit_h),
            PpStatus::Ok as i32
        );

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_serialize_theory(colimit_h, &mut out),
            PpStatus::Ok as i32
        );
        let serialized = out.to_vec();
        pp_buf_free(out);

        // Reify the serialized theory.
        let serialized_slice: c_slice::Box<u8> = serialized.into_boxed_slice().into();
        let mut reified_h: u32 = u32::MAX;
        assert_eq!(
            pp_gat_create_theory(serialized_slice.as_ref(), &mut reified_h),
            PpStatus::Ok as i32
        );

        let original = with_resource(colimit_h, |r| Ok(r.as_theory()?.clone())).unwrap();
        let reified = with_resource(reified_h, |r| Ok(r.as_theory()?.clone())).unwrap();
        assert_eq!(original.name, reified.name);
        assert_eq!(original.sorts.len(), reified.sorts.len());
        assert_eq!(original.ops.len(), reified.ops.len());

        for h in [h1, h2, shared, colimit_h, reified_h] {
            assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        }
    }

    #[test]
    fn serialize_theory_rejects_non_theory_handle() {
        let proto = handle::alloc(Resource::Protocol(
            Box::<panproto_core::schema::Protocol>::default(),
        ));
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_gat_serialize_theory(proto, &mut out),
            PpStatus::TypeMismatch as i32
        );
        pp_buf_free(out);
        assert_eq!(pp_handle_free(proto), PpStatus::Ok as i32);
    }
}
