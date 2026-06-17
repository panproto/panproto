//! Enriched theories: schema coercions, defaults, mergers, conflict
//! policies, and refinement subsorting.
//!
//! Ported from `panproto_wasm::api::enriched` (see
//! `crates/panproto-wasm/src/api/enriched.rs`): the engine logic is
//! identical, with the WASM `JsError`/`MessagePack` pairing replaced by
//! [`FfiError`] and CBOR via [`crate::canonical`].

use std::collections::HashSet;
use std::sync::Arc;

use panproto_core::gat;
use panproto_core::inst::value::Value;
use panproto_core::schema::{CoercionSpec, Constraint};
use safer_ffi::prelude::*;
use serde::Deserialize;

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// CBOR payload for [`pp_schema_add_merger`]: a merge strategy name and
/// its arguments (mirrors the WASM `MergerSpec` inline struct).
#[derive(Deserialize)]
struct MergerSpec {
    strategy: String,
    #[serde(default)]
    args: Vec<String>,
}

/// CBOR payload for [`pp_schema_add_policy`]: a conflict-resolution
/// policy name (mirrors the WASM `PolicySpec` inline struct).
#[derive(Deserialize)]
struct PolicySpec {
    policy: String,
}

/// Add a coercion between two vertex kinds to a schema.
///
/// `schema_handle` is a [`Resource::Schema`](crate::handle::Resource)
/// handle; `from_kind` and `to_kind` are the UTF-8 source/target vertex
/// kind names; `expr` is a CBOR-encoded `panproto_expr::Expr` coercion
/// expression. On success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle with the
/// coercion installed (as a `CoercionClass::Opaque` coercion with no
/// inverse), keyed by the `(from_kind, to_kind)` pair.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_coercion(
    schema_handle: u32,
    from_kind: c_slice::Ref<'_, u8>,
    to_kind: c_slice::Ref<'_, u8>,
    expr: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;

        let from_kind = utf8(from_kind.as_slice(), "from_kind")?;
        let to_kind = utf8(to_kind.as_slice(), "to_kind")?;

        // `forward` is `panproto_expr::Expr`; its concrete type is
        // inferred from the `CoercionSpec` literal below, so panproto-c
        // need not name `panproto-expr` (it is not a direct dependency).
        let forward = crate::canonical::decode(expr.as_slice())?;
        let coercion_spec = CoercionSpec {
            forward,
            inverse: None,
            class: gat::CoercionClass::Opaque,
        };

        let mut new_schema = schema;
        new_schema.coercions.insert(
            (
                gat::Name::from(from_kind.as_str()),
                gat::Name::from(to_kind.as_str()),
            ),
            coercion_spec,
        );

        *out_handle = handle::alloc(Resource::Schema(Arc::new(new_schema)));
        Ok(PpStatus::Ok)
    })
}

/// Add a default value to a schema vertex.
///
/// `schema_handle` is a schema handle; `vertex_name` is the UTF-8 vertex
/// name; `expr` is a CBOR-encoded `panproto_core::inst::value::Value`.
/// The default is recorded as a `default` constraint annotation on the
/// vertex. On success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_default(
    schema_handle: u32,
    vertex_name: c_slice::Ref<'_, u8>,
    expr: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;
        let vertex_name = utf8(vertex_name.as_slice(), "vertex_name")?;
        let default_value: Value = crate::canonical::decode(expr.as_slice())?;

        let mut new_schema = schema;
        let constraint = Constraint {
            sort: "default".into(),
            value: format!("{default_value:?}"),
        };
        new_schema
            .constraints
            .entry(gat::Name::from(vertex_name.as_str()))
            .or_default()
            .push(constraint);

        *out_handle = handle::alloc(Resource::Schema(Arc::new(new_schema)));
        Ok(PpStatus::Ok)
    })
}

/// Add a merger annotation to a schema vertex.
///
/// `schema_handle` is a schema handle; `vertex_name` is the UTF-8 vertex
/// name; `spec` is a CBOR-encoded `{ strategy, args }` record. The
/// merger is recorded as a `merger` constraint annotation. On success,
/// `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_merger(
    schema_handle: u32,
    vertex_name: c_slice::Ref<'_, u8>,
    spec: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;
        let vertex_name = utf8(vertex_name.as_slice(), "vertex_name")?;
        let merger: MergerSpec = crate::canonical::decode(spec.as_slice())?;

        if !schema.has_vertex(&vertex_name) {
            return Err(FfiError::Operation(format!(
                "vertex {vertex_name:?} not found in schema"
            )));
        }

        let mut new_schema = schema;
        let constraint_value = if merger.args.is_empty() {
            merger.strategy
        } else {
            format!("{}({})", merger.strategy, merger.args.join(", "))
        };
        let constraint = Constraint {
            sort: "merger".into(),
            value: constraint_value,
        };
        new_schema
            .constraints
            .entry(gat::Name::from(vertex_name.as_str()))
            .or_default()
            .push(constraint);

        *out_handle = handle::alloc(Resource::Schema(Arc::new(new_schema)));
        Ok(PpStatus::Ok)
    })
}

/// Add a conflict policy annotation to a schema vertex.
///
/// `schema_handle` is a schema handle; `vertex_name` is the UTF-8 vertex
/// name; `spec` is a CBOR-encoded `{ policy }` record. The policy is
/// recorded as a `conflict_policy` constraint annotation. On success,
/// `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_schema_add_policy(
    schema_handle: u32,
    vertex_name: c_slice::Ref<'_, u8>,
    spec: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;
        let vertex_name = utf8(vertex_name.as_slice(), "vertex_name")?;
        let policy: PolicySpec = crate::canonical::decode(spec.as_slice())?;

        if !schema.has_vertex(&vertex_name) {
            return Err(FfiError::Operation(format!(
                "vertex {vertex_name:?} not found in schema"
            )));
        }

        let mut new_schema = schema;
        let constraint = Constraint {
            sort: "conflict_policy".into(),
            value: policy.policy,
        };
        new_schema
            .constraints
            .entry(gat::Name::from(vertex_name.as_str()))
            .or_default()
            .push(constraint);

        *out_handle = handle::alloc(Resource::Schema(Arc::new(new_schema)));
        Ok(PpStatus::Ok)
    })
}

/// Decide a refinement subsort relationship between two constraint sets.
///
/// `base_sort` is the UTF-8 shared base sort name; `sub_constraints`
/// and `super_constraints` are CBOR-encoded `Vec<(String, String)>`
/// of `(sort, value)` pairs. On success, `out_is_subsort` receives `1`
/// when the sub-refinement refines at least as much as the
/// super-refinement (it carries every constraint the super-refinement
/// does), else `0`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_enriched_refinement_subsort(
    base_sort: c_slice::Ref<'_, u8>,
    sub_constraints: c_slice::Ref<'_, u8>,
    super_constraints: c_slice::Ref<'_, u8>,
    out_is_subsort: &mut u32,
) -> i32 {
    guard(|| {
        // The base sort is the shared carrier; both refinement sets are
        // taken over it. It does not affect the subset decision but is
        // validated as UTF-8 for a well-formed call.
        let _base_sort = utf8(base_sort.as_slice(), "base_sort")?;

        let refined: Vec<(String, String)> = crate::canonical::decode(sub_constraints.as_slice())?;
        let target: Vec<(String, String)> = crate::canonical::decode(super_constraints.as_slice())?;

        let refined_set: HashSet<(&str, &str)> = refined
            .iter()
            .map(|(s, v)| (s.as_str(), v.as_str()))
            .collect();

        let is_subsort = target
            .iter()
            .all(|(s, v)| refined_set.contains(&(s.as_str(), v.as_str())));

        *out_is_subsort = u32::from(is_subsort);
        Ok(PpStatus::Ok)
    })
}

/// Decode a borrowed byte slice as UTF-8, mapping a decode failure to a
/// descriptive [`FfiError::Serialization`].
fn utf8(bytes: &[u8], field: &str) -> Result<String, FfiError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|e| FfiError::Serialization(format!("{field}: invalid UTF-8: {e}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_core::gat::Name;
    use panproto_core::inst::value::Value;
    use panproto_core::schema::{Schema, Vertex};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::encode;

    fn schema_with_vertex(vertex: &str) -> Schema {
        let mut vertices = HashMap::new();
        vertices.insert(
            Name::from(vertex),
            Vertex {
                id: Name::from(vertex),
                kind: "object".into(),
                nsid: None,
            },
        );
        Schema {
            protocol: "enriched-test".into(),
            vertices,
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: vec![],
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn schema_handle(s: &Schema) -> u32 {
        let bytes = encode(s).unwrap();
        let slice: c_slice::Box<u8> = bytes.into_boxed_slice().into();
        let mut handle: u32 = u32::MAX;
        let status = crate::api::schema::pp_schema_from_cbor(slice.as_ref(), &mut handle);
        assert_eq!(status, PpStatus::Ok as i32);
        handle
    }

    fn read_schema(handle: u32) -> Schema {
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            crate::api::schema::pp_schema_to_cbor(handle, &mut out),
            PpStatus::Ok as i32
        );
        let s: Schema = crate::canonical::decode(&out).unwrap();
        pp_buf_free(out);
        s
    }

    fn slice_of(bytes: Vec<u8>) -> c_slice::Box<u8> {
        bytes.into_boxed_slice().into()
    }

    #[test]
    fn add_default_records_constraint() {
        let h = schema_handle(&schema_with_vertex("title"));
        let value_bytes = encode(&Value::Str("hello".into())).unwrap();

        let mut out_h: u32 = u32::MAX;
        let status = pp_schema_add_default(
            h,
            slice_of(b"title".to_vec()).as_ref(),
            slice_of(value_bytes).as_ref(),
            &mut out_h,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let schema = read_schema(out_h);
        let cs = schema.constraints.get(&Name::from("title")).unwrap();
        assert!(cs.iter().any(|c| c.sort.as_str() == "default"));

        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(out_h), PpStatus::Ok as i32);
    }

    #[test]
    fn add_merger_records_constraint() {
        let h = schema_handle(&schema_with_vertex("tags"));
        // `{ strategy, args }` map shape.
        let spec = encode(&serde_json::json!({
            "strategy": "union",
            "args": ["a", "b"]
        }))
        .unwrap();

        let mut out_h: u32 = u32::MAX;
        let status = pp_schema_add_merger(
            h,
            slice_of(b"tags".to_vec()).as_ref(),
            slice_of(spec).as_ref(),
            &mut out_h,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let schema = read_schema(out_h);
        let cs = schema.constraints.get(&Name::from("tags")).unwrap();
        let merger = cs.iter().find(|c| c.sort.as_str() == "merger").unwrap();
        assert_eq!(merger.value, "union(a, b)");

        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(out_h), PpStatus::Ok as i32);
    }

    #[test]
    fn add_merger_on_missing_vertex_errors() {
        let h = schema_handle(&schema_with_vertex("tags"));
        let spec = encode(&serde_json::json!({ "strategy": "union" })).unwrap();

        let mut out_h: u32 = u32::MAX;
        let status = pp_schema_add_merger(
            h,
            slice_of(b"nope".to_vec()).as_ref(),
            slice_of(spec).as_ref(),
            &mut out_h,
        );
        assert_eq!(status, PpStatus::Operation as i32);

        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }

    #[test]
    fn add_policy_records_constraint() {
        let h = schema_handle(&schema_with_vertex("body"));
        let spec = encode(&serde_json::json!({ "policy": "last_write_wins" })).unwrap();

        let mut out_h: u32 = u32::MAX;
        let status = pp_schema_add_policy(
            h,
            slice_of(b"body".to_vec()).as_ref(),
            slice_of(spec).as_ref(),
            &mut out_h,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let schema = read_schema(out_h);
        let cs = schema.constraints.get(&Name::from("body")).unwrap();
        let policy = cs
            .iter()
            .find(|c| c.sort.as_str() == "conflict_policy")
            .unwrap();
        assert_eq!(policy.value, "last_write_wins");

        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(out_h), PpStatus::Ok as i32);
    }

    #[test]
    fn add_coercion_installs_coercion_entry() {
        let h = schema_handle(&schema_with_vertex("int_field"));
        // A trivial coercion expression: the variable `x`. Encode it as
        // the externally-tagged `panproto_expr::Expr::Var` shape:
        // `{ "Var": "x" }`.
        let expr_bytes = encode(&serde_json::json!({ "Var": "x" })).unwrap();

        let mut out_h: u32 = u32::MAX;
        let status = pp_schema_add_coercion(
            h,
            slice_of(b"int".to_vec()).as_ref(),
            slice_of(b"string".to_vec()).as_ref(),
            slice_of(expr_bytes).as_ref(),
            &mut out_h,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let schema = read_schema(out_h);
        assert!(
            schema
                .coercions
                .contains_key(&(Name::from("int"), Name::from("string"))),
            "coercion key missing; keys: {:?}",
            schema.coercions.keys().collect::<Vec<_>>()
        );

        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(out_h), PpStatus::Ok as i32);
    }

    #[test]
    fn refinement_subsort_is_subset_check() {
        // refined = {(positive, true), (max, 10)}; target = {(positive, true)}.
        // refined carries every target constraint, so it IS a subsort.
        let refined: Vec<(String, String)> = vec![
            ("positive".into(), "true".into()),
            ("max".into(), "10".into()),
        ];
        let target: Vec<(String, String)> = vec![("positive".into(), "true".into())];

        let mut out: u32 = u32::MAX;
        let status = pp_enriched_refinement_subsort(
            slice_of(b"int".to_vec()).as_ref(),
            slice_of(encode(&refined).unwrap()).as_ref(),
            slice_of(encode(&target).unwrap()).as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);
        assert_eq!(out, 1);

        // The reverse is NOT a subsort: target carries `max` which
        // refined lacks.
        let mut out2: u32 = u32::MAX;
        let status2 = pp_enriched_refinement_subsort(
            slice_of(b"int".to_vec()).as_ref(),
            slice_of(encode(&target).unwrap()).as_ref(),
            slice_of(encode(&refined).unwrap()).as_ref(),
            &mut out2,
        );
        assert_eq!(status2, PpStatus::Ok as i32);
        assert_eq!(out2, 0);
    }
}
