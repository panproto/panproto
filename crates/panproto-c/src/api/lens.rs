//! Lens auto-generation, law checking, get/put, composition, and the
//! full protolens chain surface (instantiate, complement spec, diff,
//! compose, JSON I/O, fuse, symmetric lenses, DSL compilation).
//!
//! Ported from `panproto_wasm::api::lens` (see
//! `crates/panproto-wasm/src/api/lens.rs`), with the WASM `WasmError` /
//! `JsError` pair replaced by [`FfiError`], `rmp_serde` replaced by
//! [`crate::canonical`] (CBOR via `ciborium`), and the WASM slab
//! replaced by [`crate::handle`]. Schemas, protolens chains, symmetric
//! lenses, and compiled lenses cross the boundary as slab handles; test
//! instances, complements, diff specs, and complement specs cross as
//! CBOR payloads.

use std::sync::Arc;

use ciborium::value::Value as CborValue;
use panproto_core::{
    inst::WInstance,
    lens::{self, Stringency},
};
use safer_ffi::prelude::*;

use crate::api::helpers::{
    LawCheckResult, ProtolensStepInfo, extract_migration_owned, protocol_for_schema,
};
use crate::canonical;
use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Parse a UTF-8 stringency tier name into the engine [`Stringency`].
///
/// Accepts `strict` / `balanced` / `lenient` / `exploratory`
/// (case-insensitive) or an empty slice (the engine default, `None`).
///
/// # Errors
///
/// Returns [`FfiError::Operation`] for an unrecognized tier name or
/// non-UTF-8 bytes.
fn parse_stringency(raw: &[u8]) -> Result<Option<Stringency>, FfiError> {
    let text = std::str::from_utf8(raw)
        .map_err(|e| FfiError::Operation(format!("invalid stringency UTF-8: {e}")))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "strict" => Ok(Some(Stringency::Strict)),
        "balanced" => Ok(Some(Stringency::Balanced)),
        "lenient" => Ok(Some(Stringency::Lenient)),
        "exploratory" => Ok(Some(Stringency::Exploratory)),
        other => Err(FfiError::Operation(format!(
            "unknown stringency '{other}'; expected strict, balanced, lenient, or exploratory"
        ))),
    }
}

/// Build the CBOR payload returned by [`pp_lens_get_record`]: a two-key
/// map `{ "view": bytes, "complement": bytes }` whose values are CBOR
/// byte strings each holding a self-contained CBOR item (the projected
/// `WInstance` and the `Complement`).
///
/// Framing the inner items as byte strings (rather than nested maps)
/// lets the host decode the outer map and then run its existing
/// whole-blob `decodeInstance` / `decodeComplement` codecs on each
/// field, mirroring how the Haskell `lensGet` reads them back.
///
/// # Errors
///
/// Returns [`FfiError::Serialization`] if either inner item fails to
/// encode.
fn encode_get_record(view: &WInstance, complement: &lens::Complement) -> Result<Vec<u8>, FfiError> {
    let view_bytes = canonical::encode(view)?;
    let comp_bytes = encode_complement_for_host(complement)?;
    let payload = CborValue::Map(vec![
        (
            CborValue::Text("view".to_owned()),
            CborValue::Bytes(view_bytes),
        ),
        (
            CborValue::Text("complement".to_owned()),
            CborValue::Bytes(comp_bytes),
        ),
    ]);
    canonical::encode(&payload)
}

/// The [`lens::Complement`] fields keyed by a `(u32, u32)` tuple. `serde`
/// (via `ciborium`) serializes a tuple-keyed `HashMap` as a CBOR *map*
/// (array key, value), but the Haskell `Panproto.Instance` complement
/// codec models these two fields as CBOR *lists of `[ [k0, k1], edge ]`
/// pairs* (its `decodePairMap` / `encodePairMap`). The other map fields
/// are `u32`-keyed and agree (both sides use a CBOR map with integer
/// keys), so only these two need reshaping at the boundary.
const COMPLEMENT_PAIR_MAP_FIELDS: [&str; 2] = ["contraction_choices", "arc_edges"];

/// Encode a [`lens::Complement`] in the CBOR shape the Haskell host
/// decodes: `ciborium`'s default serialization with the tuple-keyed
/// [`COMPLEMENT_PAIR_MAP_FIELDS`] rewritten from CBOR maps into
/// lists-of-pairs.
///
/// # Errors
///
/// Returns [`FfiError::Serialization`] if (de)serialization fails.
fn encode_complement_for_host(complement: &lens::Complement) -> Result<Vec<u8>, FfiError> {
    let bytes = canonical::encode(complement)?;
    let mut value: CborValue = canonical::decode(&bytes)?;
    reshape_complement_pair_maps(&mut value, /* to_list */ true);
    canonical::encode(&value)
}

/// Decode a [`lens::Complement`] from the CBOR shape the Haskell host
/// encodes: the inverse of [`encode_complement_for_host`], rewriting the
/// tuple-keyed [`COMPLEMENT_PAIR_MAP_FIELDS`] from lists-of-pairs back
/// into the CBOR maps `ciborium`'s `Deserialize` expects.
///
/// Tolerates input that is already in the map shape (a complement
/// produced by this crate's own `ciborium` encode), so it accepts both
/// conventions.
///
/// # Errors
///
/// Returns [`FfiError::Serialization`] if (de)serialization fails.
fn decode_complement_from_host(bytes: &[u8]) -> Result<lens::Complement, FfiError> {
    let mut value: CborValue = canonical::decode(bytes)?;
    reshape_complement_pair_maps(&mut value, /* to_list */ false);
    let normalized = canonical::encode(&value)?;
    canonical::decode(&normalized)
}

/// Rewrite the [`COMPLEMENT_PAIR_MAP_FIELDS`] of a CBOR-`Map` complement
/// between the `ciborium` map shape and the host list-of-pairs shape.
///
/// When `to_list` is true, each tuple-keyed map `{ [k0,k1]: edge, … }`
/// becomes a list `[ [[k0,k1], edge], … ]`; when false, the inverse.
/// Idempotent against already-correct shapes: a field already in the
/// target shape is left untouched.
fn reshape_complement_pair_maps(value: &mut CborValue, to_list: bool) {
    let CborValue::Map(entries) = value else {
        return;
    };
    for (key, field) in entries.iter_mut() {
        let CborValue::Text(name) = key else { continue };
        if !COMPLEMENT_PAIR_MAP_FIELDS.contains(&name.as_str()) {
            continue;
        }
        if to_list {
            if let CborValue::Map(pairs) = field {
                let list = pairs
                    .drain(..)
                    .map(|(k, v)| CborValue::Array(vec![k, v]))
                    .collect();
                *field = CborValue::Array(list);
            }
        } else if let CborValue::Array(items) = field {
            let pairs = items
                .drain(..)
                .filter_map(|item| match item {
                    CborValue::Array(kv) if kv.len() == 2 => {
                        let mut it = kv.into_iter();
                        match (it.next(), it.next()) {
                            (Some(k), Some(v)) => Some((k, v)),
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .collect();
            *field = CborValue::Map(pairs);
        }
    }
}

/// Auto-generate a protolens chain between two schemas.
///
/// `schema1` and `schema2` are
/// [`Resource::Schema`](crate::handle::Resource) handles; `stringency`
/// is the UTF-8 tier name (`strict`/`balanced`/`lenient`/`exploratory`,
/// empty for default). On success, `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle. Calls
/// `lens::auto_generate`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_auto_generate_protolens(
    schema1: u32,
    schema2: u32,
    stringency: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let tier = parse_stringency(stringency.as_slice())?;
        let (src, tgt) = handle::with_two_resources(schema1, schema2, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;
        let protocol = protocol_for_schema(&src);

        let mut config = lens::AutoLensConfig::default();
        if let Some(s) = tier {
            config.stringency = s;
        }
        let result = lens::auto_generate(&src, &tgt, &protocol, &config)
            .map_err(|e| FfiError::Operation(format!("auto_generate: {e}")))?;

        *out_handle = handle::alloc(Resource::ProtolensChain(Box::new(result.chain)));
        Ok(PpStatus::Ok)
    })
}

/// Auto-generate up to `top_n` ranked candidate lenses.
///
/// `schema1` and `schema2` are schema handles; `stringency` is the
/// UTF-8 tier name. On success, `out` receives a CBOR-encoded
/// `{ candidates, coerce_proposals }` record. Calls
/// `lens::auto_generate_candidates`.
///
/// Each candidate entry carries its instantiable `chain`: the candidate's
/// `ProtolensChain` serialized in the same JSON shape
/// `ProtolensChain::to_json` emits, so the host can feed it back through
/// `pp_protolens_from_json` and `pp_protolens_instantiate` to obtain a
/// runnable lens. The score, coverage, quality, strategies, and per-step
/// explanations travel alongside it.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_auto_generate_candidates(
    schema1: u32,
    schema2: u32,
    top_n: u32,
    stringency: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let tier = parse_stringency(stringency.as_slice())?;
        let (src, tgt) = handle::with_two_resources(schema1, schema2, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;
        let protocol = protocol_for_schema(&src);

        let mut config = lens::AutoLensConfig::default();
        if let Some(s) = tier {
            config.stringency = s;
        }
        let candidates =
            lens::auto_generate_candidates(&src, &tgt, &protocol, &config, top_n as usize)
                .map_err(|e| FfiError::Operation(format!("auto_generate_candidates: {e}")))?;

        // Exploratory-tier coerce proposals are a property of the run,
        // not of any individual candidate; surface them by running a
        // single alignment at the same config. If candidates were found
        // but that alignment errors, the engine is inconsistent, so the
        // error is surfaced rather than swallowed.
        let coerce_proposals = if candidates.is_empty() {
            Vec::new()
        } else {
            let result = lens::auto_generate(&src, &tgt, &protocol, &config).map_err(|e| {
                FfiError::Operation(format!(
                    "candidates were found but coerce-proposal alignment failed: {e}"
                ))
            })?;
            result
                .coerce_proposals
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "src": p.anchor.src.as_str(),
                        "tgt": p.anchor.tgt.as_str(),
                        "witness_name": p.witness_name,
                        "witness_class": p.witness_class,
                        "confidence": p.anchor.confidence,
                        "explanation": p.anchor.explanation,
                    })
                })
                .collect::<Vec<_>>()
        };

        let candidates_payload: Vec<serde_json::Value> = candidates
            .iter()
            .map(|c| {
                // Embed each candidate's chain in the same serde shape
                // `ProtolensChain::to_json` emits, so the host can
                // round-trip it through `pp_protolens_from_json`.
                let chain = serde_json::to_value(&c.chain).map_err(|e| {
                    FfiError::Serialization(format!("candidate chain serialize: {e}"))
                })?;
                Ok(serde_json::json!({
                    "quality": c.quality,
                    "coverage": c.coverage,
                    "score": c.score(),
                    "strategies_used": c.strategies_used,
                    "chain": chain,
                    "steps": c.steps.iter().map(|s| serde_json::json!({
                        "kind": s.kind,
                        "explanation": s.explanation,
                        "confidence": s.confidence,
                        "strategy": s.strategy,
                    })).collect::<Vec<_>>(),
                }))
            })
            .collect::<Result<Vec<_>, FfiError>>()?;

        let wrapper = serde_json::json!({
            "candidates": candidates_payload,
            "coerce_proposals": coerce_proposals,
        });

        *out = canonical::encode(&wrapper)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Build a [`lens::Lens`] from a migration/lens handle.
///
/// Clones the compiled migration and its source/target schemas out of
/// the resource (synthesizing minimal schemas for a bare migration).
fn lens_from_handle(migration: u32) -> Result<lens::Lens, FfiError> {
    handle::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;
        Ok(lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        })
    })
}

/// Encode a law-check outcome as a [`LawCheckResult`].
fn law_result(outcome: Result<(), lens::LawViolation>) -> LawCheckResult {
    match outcome {
        Ok(()) => LawCheckResult {
            holds: true,
            violation: None,
        },
        Err(e) => LawCheckResult {
            holds: false,
            violation: Some(e.to_string()),
        },
    }
}

/// Check both `GetPut` and `PutGet` lens laws on a test instance.
///
/// `migration` is a migration/lens handle; `instance` is a CBOR-encoded
/// `WInstance`. On success, `out` receives a CBOR-encoded
/// [`LawCheckResult`]. Calls `lens::check_laws`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_check_laws(
    migration: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let inst: WInstance = canonical::decode(instance.as_slice())?;
        let lens_obj = lens_from_handle(migration)?;
        let result = law_result(lens::check_laws(&lens_obj, &inst));
        *out = canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Check the `GetPut` lens law on a test instance.
///
/// Arguments and payload match [`pp_lens_check_laws`]. Calls
/// `lens::check_get_put`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_check_get_put(
    migration: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let inst: WInstance = canonical::decode(instance.as_slice())?;
        let lens_obj = lens_from_handle(migration)?;
        let result = law_result(lens::check_get_put(&lens_obj, &inst));
        *out = canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Check the `PutGet` lens law on a test instance.
///
/// Arguments and payload match [`pp_lens_check_laws`]. Calls
/// `lens::check_put_get`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_check_put_get(
    migration: u32,
    instance: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let inst: WInstance = canonical::decode(instance.as_slice())?;
        let lens_obj = lens_from_handle(migration)?;
        let result = law_result(lens::check_put_get(&lens_obj, &inst));
        *out = canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Bidirectional get: extract a view and complement from a record.
///
/// `migration` is a migration/lens handle; `record` is a CBOR-encoded
/// `WInstance`. On success, `out` receives a CBOR-encoded
/// `{ view: WInstance, complement: Complement }`. Calls `lens::get`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_get_record(
    migration: u32,
    record: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let source: WInstance = canonical::decode(record.as_slice())?;
        let lens_obj = lens_from_handle(migration)?;
        let (view, complement) =
            lens::get(&lens_obj, &source).map_err(|e| FfiError::Operation(format!("get: {e}")))?;
        *out = encode_get_record(&view, &complement)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Bidirectional put: restore a record from a view and complement.
///
/// `migration` is a migration/lens handle; `view` and `complement` are
/// CBOR-encoded `WInstance` and `Complement`. On success, `out`
/// receives the CBOR-encoded restored `WInstance`. Calls `lens::put`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_put_record(
    migration: u32,
    view: c_slice::Ref<'_, u8>,
    complement: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let view_inst: WInstance = canonical::decode(view.as_slice())?;
        let comp = decode_complement_from_host(complement.as_slice())?;
        let lens_obj = lens_from_handle(migration)?;
        let restored = lens::put(&lens_obj, &view_inst, &comp)
            .map_err(|e| FfiError::Operation(format!("put: {e}")))?;
        *out = canonical::encode(&restored)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Compose two lenses sequentially.
///
/// `l1` and `l2` are migration/lens handles. On success, `out_handle`
/// receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle.
/// Calls `lens::compose`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_compose(l1: u32, l2: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let (lens1, lens2) = handle::with_two_resources(l1, l2, |r1, r2| {
            let (c1, s1_src, s1_tgt) = extract_migration_owned(r1)?;
            let (c2, s2_src, s2_tgt) = extract_migration_owned(r2)?;
            Ok((
                lens::Lens {
                    compiled: c1,
                    src_schema: s1_src,
                    tgt_schema: s1_tgt,
                },
                lens::Lens {
                    compiled: c2,
                    src_schema: s2_src,
                    tgt_schema: s2_tgt,
                },
            ))
        })?;

        let composed = lens::compose(&lens1, &lens2)
            .map_err(|e| FfiError::Operation(format!("compose: {e}")))?;

        *out_handle = handle::alloc(Resource::MigrationWithSchemas {
            compiled: Box::new(composed.compiled),
            src_schema: Arc::new(composed.src_schema),
            tgt_schema: Arc::new(composed.tgt_schema),
        });
        Ok(PpStatus::Ok)
    })
}

/// Instantiate a protolens chain at a specific schema.
///
/// `chain` is a [`Resource::ProtolensChain`](crate::handle::Resource)
/// handle; `schema` is a [`Resource::Schema`](crate::handle::Resource)
/// handle. On success, `out_handle` receives a fresh
/// [`Resource::MigrationWithSchemas`](crate::handle::Resource) handle.
/// Calls `ProtolensChain::instantiate`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_instantiate(chain: u32, schema: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let (chain_val, schema_val) = handle::with_two_resources(chain, schema, |r1, r2| {
            Ok((r1.as_protolens_chain()?.clone(), r2.as_schema()?.clone()))
        })?;
        let protocol = protocol_for_schema(&schema_val);

        let lens_obj = chain_val
            .instantiate(&schema_val, &protocol)
            .map_err(|e| FfiError::Operation(format!("instantiate: {e}")))?;

        *out_handle = handle::alloc(Resource::MigrationWithSchemas {
            compiled: Box::new(lens_obj.compiled),
            src_schema: Arc::new(lens_obj.src_schema),
            tgt_schema: Arc::new(lens_obj.tgt_schema),
        });
        Ok(PpStatus::Ok)
    })
}

/// Get the complement spec for a protolens chain at a schema.
///
/// `chain` is a protolens chain handle; `schema` is a schema handle. On
/// success, `out` receives a CBOR-encoded
/// `panproto_core::lens::ComplementSpec`. Calls
/// `lens::chain_complement_spec`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_complement_spec(chain: u32, schema: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let (chain_val, schema_val) = handle::with_two_resources(chain, schema, |r1, r2| {
            Ok((r1.as_protolens_chain()?.clone(), r2.as_schema()?.clone()))
        })?;
        let protocol = protocol_for_schema(&schema_val);

        let spec = lens::chain_complement_spec(&chain_val, &schema_val, &protocol);
        *out = canonical::encode(&spec)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Build a protolens chain from a diff spec.
///
/// `diff` is a CBOR-encoded `panproto_core::lens::DiffSpec`; `schema1`
/// and `schema2` are schema handles. On success, `out_handle` receives
/// a fresh [`Resource::ProtolensChain`](crate::handle::Resource)
/// handle. Calls `lens::diff_to_protolens`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_from_diff(
    diff: c_slice::Ref<'_, u8>,
    schema1: u32,
    schema2: u32,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let diff_spec: lens::DiffSpec = canonical::decode(diff.as_slice())?;
        let (src, tgt) = handle::with_two_resources(schema1, schema2, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;

        let chain = lens::diff_to_protolens(&diff_spec, &src, &tgt)
            .map_err(|e| FfiError::Operation(format!("diff_to_protolens: {e}")))?;

        *out_handle = handle::alloc(Resource::ProtolensChain(Box::new(chain)));
        Ok(PpStatus::Ok)
    })
}

/// Compose two protolens chains.
///
/// `chain1` and `chain2` are protolens chain handles. On success,
/// `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle holding
/// the concatenated steps.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_compose(chain1: u32, chain2: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let (c1, c2) = handle::with_two_resources(chain1, chain2, |r1, r2| {
            Ok((
                r1.as_protolens_chain()?.clone(),
                r2.as_protolens_chain()?.clone(),
            ))
        })?;

        let mut combined_steps = c1.steps;
        combined_steps.extend(c2.steps);

        *out_handle = handle::alloc(Resource::ProtolensChain(Box::new(
            lens::ProtolensChain::new(combined_steps),
        )));
        Ok(PpStatus::Ok)
    })
}

/// Serialize a protolens chain to JSON.
///
/// `chain` is a protolens chain handle. On success, `out` receives JSON
/// bytes describing each step (name, endofunctors, lossless flag) per
/// [`ProtolensStepInfo`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_chain_to_json(chain: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let steps = handle::with_resource(chain, |r| {
            let chain_val = r.as_protolens_chain()?;
            Ok(chain_val
                .steps
                .iter()
                .map(|step| ProtolensStepInfo {
                    name: step.name.to_string(),
                    source_endofunctor: step.source.name.to_string(),
                    target_endofunctor: step.target.name.to_string(),
                    lossless: step.is_lossless(),
                })
                .collect::<Vec<_>>())
        })?;

        let bytes =
            serde_json::to_vec(&steps).map_err(|e| FfiError::Serialization(e.to_string()))?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Deserialize a protolens chain from JSON.
///
/// `json` is raw JSON bytes. On success, `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle. Calls
/// `ProtolensChain::from_json`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_from_json(json: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let json_str = std::str::from_utf8(json.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid JSON UTF-8: {e}")))?;
        let chain = lens::ProtolensChain::from_json(json_str)
            .map_err(|e| FfiError::Operation(format!("from_json: {e}")))?;
        *out_handle = handle::alloc(Resource::ProtolensChain(Box::new(chain)));
        Ok(PpStatus::Ok)
    })
}

/// Fuse a protolens chain into a single composite step.
///
/// `chain` is a protolens chain handle. On success, `out_handle`
/// receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle holding
/// the fused step. Calls `ProtolensChain::fuse`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_protolens_fuse(chain: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let chain_obj = handle::with_resource(chain, |r| Ok(r.as_protolens_chain()?.clone()))?;
        let fused = chain_obj
            .fuse()
            .map_err(|e| FfiError::Operation(format!("fuse: {e}")))?;
        *out_handle = handle::alloc(Resource::ProtolensChain(Box::new(
            lens::ProtolensChain::new(vec![fused]),
        )));
        Ok(PpStatus::Ok)
    })
}

/// Auto-generate a symmetric lens from two schemas.
///
/// `schema1` and `schema2` are
/// [`Resource::Schema`](crate::handle::Resource) handles. On success,
/// `out_handle` receives a fresh
/// [`Resource::SymmetricLensHandle`](crate::handle::Resource) handle.
/// Calls `SymmetricLens::auto_symmetric`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_symmetric_from_schemas(schema1: u32, schema2: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let (left, right) = handle::with_two_resources(schema1, schema2, |r1, r2| {
            Ok((r1.as_schema()?.clone(), r2.as_schema()?.clone()))
        })?;
        let protocol = protocol_for_schema(&left);
        let config = lens::AutoLensConfig::default();

        let sym = lens::SymmetricLens::auto_symmetric(&left, &right, &protocol, &config)
            .map_err(|e| FfiError::Operation(format!("auto_symmetric: {e}")))?;

        *out_handle = handle::alloc(Resource::SymmetricLensHandle(Box::new(sym)));
        Ok(PpStatus::Ok)
    })
}

/// Sync data through a symmetric lens.
///
/// `sym_lens` is a symmetric-lens handle; `view` and `complement` are
/// CBOR-encoded `WInstance` and `Complement`; `direction` is `0`
/// (left-to-right) or `1` (right-to-left). On success, `out` receives
/// the CBOR-encoded synced `WInstance`. Calls
/// `SymmetricLens::sync_left_to_right` / `sync_right_to_left`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_symmetric_sync(
    sym_lens: u32,
    view: c_slice::Ref<'_, u8>,
    complement: c_slice::Ref<'_, u8>,
    direction: u8,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let view_instance: WInstance = canonical::decode(view.as_slice())?;
        let comp = decode_complement_from_host(complement.as_slice())?;

        let (result_view, _result_complement) = handle::with_resource(sym_lens, |r| {
            let sym = r.as_symmetric_lens()?;
            match direction {
                0 => sym
                    .sync_left_to_right(&view_instance, &comp)
                    .map_err(|e| FfiError::Operation(format!("sync_left_to_right: {e}"))),
                1 => sym
                    .sync_right_to_left(&view_instance, &comp)
                    .map_err(|e| FfiError::Operation(format!("sync_right_to_left: {e}"))),
                other => Err(FfiError::Operation(format!(
                    "invalid direction: {other}, expected 0 or 1"
                ))),
            }
        })?;

        *out = canonical::encode(&result_view)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Compile a lens DSL document into a protolens chain.
///
/// `source` is UTF-8 DSL source; `format` is the UTF-8 format name
/// (`json` or `yaml`); `body_vertex` is the UTF-8 parent vertex id for
/// field-level steps. On success, `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle. Calls
/// `panproto_core::lens_dsl::{eval, compile}`.
///
/// Nickel (`ncl`) is intentionally unsupported here, matching the WASM
/// boundary: Nickel evaluation requires a filesystem for its contract
/// imports, so callers precompile Nickel to JSON on the host.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_compile_document(
    source: c_slice::Ref<'_, u8>,
    format: c_slice::Ref<'_, u8>,
    body_vertex: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let source_str = std::str::from_utf8(source.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid source UTF-8: {e}")))?;
        let format_str = std::str::from_utf8(format.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid format UTF-8: {e}")))?;
        let body = std::str::from_utf8(body_vertex.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid body_vertex UTF-8: {e}")))?;

        let doc = match format_str {
            "json" => panproto_core::lens_dsl::eval::eval_json(source_str),
            "yaml" | "yml" => panproto_core::lens_dsl::eval::eval_yaml(source_str),
            other => {
                return Err(FfiError::Operation(format!(
                    "unsupported lens DSL format '{other}'; expected 'json' or 'yaml'"
                )));
            }
        }
        .map_err(|e| FfiError::Operation(format!("lens DSL eval: {e}")))?;

        let compiled = panproto_core::lens_dsl::compile(&doc, body, &|_| None)
            .map_err(|e| FfiError::Operation(format!("lens DSL compile: {e}")))?;

        *out_handle = handle::alloc(Resource::ProtolensChain(Box::new(compiled.chain)));
        Ok(PpStatus::Ok)
    })
}

/// Compile a lens DSL document, resolving `compose` named references
/// against a bundle of sibling documents.
///
/// `source`, `format`, and `body_vertex` match
/// [`pp_lens_compile_document`]. `refs` is a CBOR-encoded
/// `map<string, string>` from each referenced lens `id` to its document
/// source (in the same `format`); a `compose` body's `ref` entries are
/// resolved against this map. On success, `out_handle` receives a fresh
/// [`Resource::ProtolensChain`](crate::handle::Resource) handle. Calls
/// `panproto_core::lens_dsl::compile_with_refs`.
///
/// Nickel (`ncl`) is intentionally unsupported, matching
/// [`pp_lens_compile_document`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_lens_compile_document_with_refs(
    source: c_slice::Ref<'_, u8>,
    format: c_slice::Ref<'_, u8>,
    body_vertex: c_slice::Ref<'_, u8>,
    refs: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let format_str = std::str::from_utf8(format.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid format UTF-8: {e}")))?;
        let body = std::str::from_utf8(body_vertex.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid body_vertex UTF-8: {e}")))?;

        let parse = |text: &str| -> Result<panproto_core::lens_dsl::LensDocument, FfiError> {
            match format_str {
                "json" => panproto_core::lens_dsl::eval::eval_json(text),
                "yaml" | "yml" => panproto_core::lens_dsl::eval::eval_yaml(text),
                other => {
                    return Err(FfiError::Operation(format!(
                        "unsupported lens DSL format '{other}'; expected 'json' or 'yaml'"
                    )));
                }
            }
            .map_err(|e| FfiError::Operation(format!("lens DSL eval: {e}")))
        };

        let source_str = std::str::from_utf8(source.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid source UTF-8: {e}")))?;
        let doc = parse(source_str)?;

        let ref_sources: std::collections::HashMap<String, String> =
            canonical::decode(refs.as_slice())?;
        let mut docs_by_id = std::collections::HashMap::new();
        for (id, ref_source) in &ref_sources {
            docs_by_id.insert(id.clone(), parse(ref_source)?);
        }

        let compiled = panproto_core::lens_dsl::compile_with_refs(&doc, body, &docs_by_id)
            .map_err(|e| FfiError::Operation(format!("lens DSL compile: {e}")))?;

        *out_handle = handle::alloc(Resource::ProtolensChain(Box::new(compiled.chain)));
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use panproto_core::schema::{Schema, SchemaBuilder};

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};

    fn slice(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// Decode the `{ "view": bytes, "complement": bytes }` payload
    /// [`encode_get_record`] produces, returning the inner view CBOR and
    /// the inner complement CBOR (the latter still in the host
    /// list-of-pairs shape, the form `put_record` expects back).
    fn split_get_record(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let payload: ciborium::value::Value = canonical::decode(bytes).unwrap();
        let map = payload.as_map().expect("get_record payload is a map");
        let mut view_bytes = None;
        let mut comp_bytes = None;
        for (k, v) in map {
            match k.as_text() {
                Some("view") => view_bytes = Some(v.as_bytes().expect("view is bytes").clone()),
                Some("complement") => {
                    comp_bytes = Some(v.as_bytes().expect("complement is bytes").clone());
                }
                _ => {}
            }
        }
        (
            view_bytes.expect("view present"),
            comp_bytes.expect("complement present"),
        )
    }

    /// A `post` record carrying `text` and `subtitle` string properties,
    /// the source of a drop-bearing morphism. The FFI auto-generation
    /// path derives its protocol from the schema's `protocol` field
    /// (here `"test"`, which falls back to
    /// [`default_protocol`](crate::api::helpers::default_protocol), whose
    /// object kinds are `object` / `string` / `record`), so the fixtures
    /// use only `record` and `string` vertex kinds.
    fn source_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("post.text", "string", None::<&str>)
            .unwrap()
            .vertex("post.subtitle", "string", None::<&str>)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.subtitle", "prop", Some("subtitle"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// The target schema: a `post` record with only the `text` property,
    /// so the morphism from [`source_schema`] drops `subtitle`.
    fn target_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("test");
        SchemaBuilder::new(&proto)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("post.text", "string", None::<&str>)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn schema_handle(s: &Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s.clone())))
    }

    /// Auto-generate a chain between two schema handles, returning its
    /// chain handle.
    fn auto_chain(src_h: u32, tgt_h: u32) -> u32 {
        let mut out: u32 = u32::MAX;
        let status =
            pp_lens_auto_generate_protolens(src_h, tgt_h, slice(b"balanced").as_ref(), &mut out);
        assert_eq!(
            status,
            PpStatus::Ok as i32,
            "auto_generate_protolens failed"
        );
        out
    }

    #[test]
    fn auto_generate_emits_step_summary_json() {
        let src_h = schema_handle(&source_schema());
        let tgt_h = schema_handle(&target_schema());
        let chain_h = auto_chain(src_h, tgt_h);

        // chain_to_json emits a JSON array of ProtolensStepInfo summary
        // objects (name / source_endofunctor / target_endofunctor /
        // lossless), distinct from the full serde ProtolensChain shape
        // pp_protolens_from_json parses. Verify the summary shape.
        let mut json_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_protolens_chain_to_json(chain_h, &mut json_out),
            PpStatus::Ok as i32
        );
        let value: serde_json::Value = serde_json::from_slice(&json_out).unwrap();
        let arr = value.as_array().expect("expected JSON array");
        // The chain may be empty (the morphism is realized without
        // elementary steps); when steps are present each carries the
        // ProtolensStepInfo summary keys.
        for step in arr {
            assert!(step.get("name").is_some(), "step missing name: {step:?}");
            assert!(
                step.get("source_endofunctor").is_some(),
                "step missing source_endofunctor: {step:?}"
            );
            assert!(
                step.get("lossless").is_some(),
                "step missing lossless: {step:?}"
            );
        }
        pp_buf_free(json_out);

        assert_eq!(pp_handle_free(chain_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn instantiate_get_put_round_trip() {
        use crate::api::instance::pp_inst_json_to_instance;

        let src_h = schema_handle(&source_schema());
        let tgt_h = schema_handle(&target_schema());
        let chain_h = auto_chain(src_h, tgt_h);

        // Instantiate the chain at the source schema, producing a lens.
        let mut lens_h: u32 = u32::MAX;
        assert_eq!(
            pp_protolens_instantiate(chain_h, src_h, &mut lens_h),
            PpStatus::Ok as i32
        );

        // Parse a small source instance.
        let json = br#"{"text": "hello", "subtitle": "world"}"#;
        let mut inst_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_inst_json_to_instance(
                src_h,
                slice(json).as_ref(),
                slice(b"post").as_ref(),
                &mut inst_out
            ),
            PpStatus::Ok as i32
        );
        let source_cbor = inst_out.to_vec();
        pp_buf_free(inst_out);

        // get: project the source through the lens.
        let mut get_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_lens_get_record(lens_h, slice(&source_cbor).as_ref(), &mut get_out),
            PpStatus::Ok as i32,
            "get_record failed"
        );
        // Split the { view, complement } payload. The view is a plain
        // WInstance CBOR; the complement is in the host list-of-pairs
        // shape, which is exactly what put_record expects back.
        let (view_cbor, comp_cbor) = split_get_record(&get_out);
        pp_buf_free(get_out);

        // put: reconstruct the source from the unchanged view and
        // complement. GetPut law: put(get(s)) == s.
        let mut put_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_lens_put_record(
                lens_h,
                slice(&view_cbor).as_ref(),
                slice(&comp_cbor).as_ref(),
                &mut put_out
            ),
            PpStatus::Ok as i32,
            "put_record failed"
        );
        let restored: WInstance = canonical::decode(&put_out).unwrap();
        pp_buf_free(put_out);

        let original: WInstance = canonical::decode(&source_cbor).unwrap();
        assert_eq!(
            restored.node_count(),
            original.node_count(),
            "get-put round trip lost nodes"
        );

        // check_get_put on the original instance should hold.
        let mut law_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_lens_check_get_put(lens_h, slice(&source_cbor).as_ref(), &mut law_out),
            PpStatus::Ok as i32
        );
        let law: LawCheckResult = canonical::decode(&law_out).unwrap();
        assert!(law.holds, "GetPut violation: {:?}", law.violation);
        pp_buf_free(law_out);

        assert_eq!(pp_handle_free(lens_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(chain_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    /// Build a deterministically non-empty two-step chain: drop the
    /// `subtitle` and `byline` sorts. Independent of auto-generation,
    /// which may legitimately realize a morphism with an empty chain.
    fn drop_chain_handle() -> u32 {
        use panproto_core::gat::Name;
        use panproto_core::lens::ProtolensChain;
        use panproto_core::lens::protolens::elementary::drop_sort;
        let chain = ProtolensChain::new(vec![
            drop_sort(Name::from("post.subtitle")),
            drop_sort(Name::from("post.byline")),
        ]);
        handle::alloc(Resource::ProtolensChain(Box::new(chain)))
    }

    #[test]
    fn complement_spec_and_fuse() {
        let src_h = schema_handle(&source_schema());
        let chain_h = drop_chain_handle();

        // complement_spec yields a CBOR ComplementSpec.
        let mut spec_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_protolens_complement_spec(chain_h, src_h, &mut spec_out),
            PpStatus::Ok as i32
        );
        let spec: lens::ComplementSpec = canonical::decode(&spec_out).unwrap();
        let _ = spec.kind;
        pp_buf_free(spec_out);

        // fuse collapses the two-step chain to a single step.
        let mut fused_h: u32 = u32::MAX;
        assert_eq!(
            pp_protolens_fuse(chain_h, &mut fused_h),
            PpStatus::Ok as i32
        );
        let step_count =
            handle::with_resource(fused_h, |r| Ok(r.as_protolens_chain()?.steps.len())).unwrap();
        assert_eq!(step_count, 1, "fuse should yield a single step");

        assert_eq!(pp_handle_free(fused_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(chain_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
    }

    #[test]
    fn compose_chain_concatenates_steps() {
        let chain_h = drop_chain_handle();
        let len1 =
            handle::with_resource(chain_h, |r| Ok(r.as_protolens_chain()?.steps.len())).unwrap();

        let chain_h2 = drop_chain_handle();
        let len2 =
            handle::with_resource(chain_h2, |r| Ok(r.as_protolens_chain()?.steps.len())).unwrap();

        // Composition concatenates the step lists.
        let mut composed_h: u32 = u32::MAX;
        assert_eq!(
            pp_protolens_compose(chain_h, chain_h2, &mut composed_h),
            PpStatus::Ok as i32
        );
        let composed_len =
            handle::with_resource(composed_h, |r| Ok(r.as_protolens_chain()?.steps.len())).unwrap();
        assert_eq!(composed_len, len1 + len2);
        assert_eq!(
            composed_len, 4,
            "two two-step chains should compose to four"
        );

        assert_eq!(pp_handle_free(composed_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(chain_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(chain_h2), PpStatus::Ok as i32);
    }

    #[test]
    fn auto_generate_candidates_each_carries_a_chain() {
        // Each ranked candidate must carry an instantiable `chain` (the
        // serialized ProtolensChain) alongside its score, so the host can
        // run any candidate, not just the top one.
        let src_h = schema_handle(&source_schema());
        let tgt_h = schema_handle(&target_schema());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        // Exploratory tier surfaces spans as multiple candidates.
        let status = pp_lens_auto_generate_candidates(
            src_h,
            tgt_h,
            5,
            slice(b"exploratory").as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let value: serde_json::Value = canonical::decode(&out).unwrap();
        pp_buf_free(out);
        let candidates = value
            .get("candidates")
            .and_then(serde_json::Value::as_array)
            .expect("candidates array present");
        assert!(
            !candidates.is_empty(),
            "expected at least one candidate, got {value:?}"
        );

        // Every candidate carries a `chain` whose JSON round-trips
        // through pp_protolens_from_json (the engine's own chain codec).
        for cand in candidates {
            let chain = cand.get("chain").expect("candidate carries a chain");
            assert!(cand.get("score").is_some(), "candidate carries a score");
            let chain_json = serde_json::to_string(chain).unwrap();
            let mut chain_h: u32 = u32::MAX;
            assert_eq!(
                pp_protolens_from_json(slice(chain_json.as_bytes()).as_ref(), &mut chain_h),
                PpStatus::Ok as i32,
                "candidate chain {chain_json} should parse via from_json"
            );
            // And it instantiates against the source schema, yielding a
            // runnable lens handle.
            let mut lens_h: u32 = u32::MAX;
            assert_eq!(
                pp_protolens_instantiate(chain_h, src_h, &mut lens_h),
                PpStatus::Ok as i32,
                "candidate chain should instantiate at the source schema"
            );
            assert_eq!(pp_handle_free(lens_h), PpStatus::Ok as i32);
            assert_eq!(pp_handle_free(chain_h), PpStatus::Ok as i32);
        }

        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    #[test]
    fn unknown_stringency_is_rejected() {
        let src_h = schema_handle(&source_schema());
        let tgt_h = schema_handle(&target_schema());
        let mut out: u32 = u32::MAX;
        let status =
            pp_lens_auto_generate_protolens(src_h, tgt_h, slice(b"loose").as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }

    /// The host-shape complement codec must round-trip: encoding a
    /// complement for the host (tuple-keyed maps as lists-of-pairs) and
    /// decoding it back yields an equivalent complement. Exercises a
    /// complement with populated `contraction_choices` and `arc_edges`,
    /// the two fields whose CBOR shape differs from `ciborium`'s default.
    #[test]
    fn complement_host_shape_round_trips() {
        use panproto_core::gat::Name;
        use panproto_core::schema::Edge;

        let edge = Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: Some(Name::from("x")),
        };
        let mut comp = lens::Complement::empty();
        comp.contraction_choices.insert((1, 2), edge.clone());
        comp.arc_edges.insert((3, 4), edge.clone());
        comp.original_parent.insert(2, 1);
        comp.source_fingerprint = 99;

        let host_bytes = encode_complement_for_host(&comp).unwrap();

        // The host-shaped bytes must decode under the Haskell convention:
        // the tuple-keyed fields are CBOR lists, not maps.
        let host_value: ciborium::value::Value = canonical::decode(&host_bytes).unwrap();
        for (k, v) in host_value.as_map().unwrap() {
            if matches!(k.as_text(), Some("contraction_choices" | "arc_edges")) {
                assert!(
                    v.is_array(),
                    "tuple-keyed field {k:?} must be a list for the host"
                );
            }
        }

        // And the inverse recovers the complement.
        let restored = decode_complement_from_host(&host_bytes).unwrap();
        assert_eq!(restored.contraction_choices.get(&(1, 2)), Some(&edge));
        assert_eq!(restored.arc_edges.get(&(3, 4)), Some(&edge));
        assert_eq!(restored.original_parent.get(&2), Some(&1));
        assert_eq!(restored.source_fingerprint, 99);

        // decode_complement_from_host also tolerates the ciborium map
        // shape (a complement this crate encoded directly), so both
        // conventions decode.
        let map_bytes = canonical::encode(&comp).unwrap();
        let from_map = decode_complement_from_host(&map_bytes).unwrap();
        assert_eq!(from_map.contraction_choices.get(&(1, 2)), Some(&edge));
    }

    #[test]
    fn parse_stringency_accepts_tiers() {
        assert!(matches!(parse_stringency(b""), Ok(None)));
        assert!(matches!(
            parse_stringency(b"Strict"),
            Ok(Some(Stringency::Strict))
        ));
        assert!(matches!(
            parse_stringency(b"EXPLORATORY"),
            Ok(Some(Stringency::Exploratory))
        ));
        assert!(parse_stringency(b"nonsense").is_err());
    }

    /// `pp_protolens_from_json` parses the full serde `ProtolensChain`
    /// shape that `ProtolensChain::to_json` emits (the endofunctor
    /// structure plus complement constructors), which is distinct from
    /// the lightweight step-summary array `pp_protolens_chain_to_json`
    /// produces. Feed the engine its own serialized chain and confirm
    /// the step count round-trips.
    #[test]
    fn from_json_parses_full_serde_chain() {
        let src_h = schema_handle(&source_schema());
        let tgt_h = schema_handle(&target_schema());
        let chain_h = auto_chain(src_h, tgt_h);

        // Serialize the chain through the engine's full serde codec.
        let json = handle::with_resource(chain_h, |r| {
            let chain = r.as_protolens_chain()?;
            chain
                .to_json()
                .map_err(|e| FfiError::Serialization(e.to_string()))
        })
        .unwrap();
        let expected_len =
            handle::with_resource(chain_h, |r| Ok(r.as_protolens_chain()?.steps.len())).unwrap();

        let mut rebuilt_h: u32 = u32::MAX;
        assert_eq!(
            pp_protolens_from_json(slice(json.as_bytes()).as_ref(), &mut rebuilt_h),
            PpStatus::Ok as i32,
            "from_json rejected the engine's own ProtolensChain JSON"
        );
        let rebuilt_len =
            handle::with_resource(rebuilt_h, |r| Ok(r.as_protolens_chain()?.steps.len())).unwrap();
        assert_eq!(rebuilt_len, expected_len, "from_json lost steps");

        assert_eq!(pp_handle_free(rebuilt_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(chain_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(src_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(tgt_h), PpStatus::Ok as i32);
    }
}
