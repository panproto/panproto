//! GAT operations: theory construction, colimit, morphism checking,
//! model migration.
//!
//! Ported from `panproto_wasm::api::gat` (see
//! `crates/panproto-wasm/src/api/gat.rs`) to the panproto-c conventions:
//! CBOR via [`crate::canonical`], errors as [`FfiError`], handles via the
//! [`crate::handle`] slab, and panic-safe entry points wrapped in
//! [`crate::panic::guard`].
//!
//! Theories live in the slab as [`Resource::Theory`]; the other three
//! entry points exchange CBOR payloads. Model migration only moves the
//! sort-interpretation map: operation interpretations are closures
//! (`Arc<dyn Fn(...)>`) that cannot cross the ABI, so they are neither
//! serialized nor reconstructed here.

use std::collections::HashMap;

use panproto_core::gat::{self};
use safer_ffi::prelude::*;

use crate::api::helpers::MorphismCheckResult;
use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource, with_three_resources, with_two_resources};
use crate::panic::guard;

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::gat::{self, Operation, Sort, Theory, TheoryMorphism};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::{decode, encode};
    use crate::handle::with_resource;

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
}
