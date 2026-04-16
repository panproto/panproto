//! Lens composition, laws, and protolens chain operations.
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::{
    gat::{self},
    inst::{self, WInstance},
    lens::{self, Stringency},
    mig::{self, Migration},
};

use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

use super::helpers::{
    FactorizationStepInfo, LawCheckResult, ProtolensStepInfo, ProtolensStepSpec,
    build_chain_from_step_spec, default_protocol, extract_migration_owned, lookup_builtin_protocol,
};

/// Map a JS-side stringency string into the engine [`Stringency`].
///
/// Accepts `"strict" | "balanced" | "lenient" | "exploratory"`
/// (case-insensitive) or empty/unset (returns the default).
fn parse_stringency(raw: Option<&str>) -> Result<Option<Stringency>, JsError> {
    let trimmed = raw.map(str::trim).filter(|s| !s.is_empty());
    match trimmed.map(str::to_ascii_lowercase).as_deref() {
        None => Ok(None),
        Some("strict") => Ok(Some(Stringency::Strict)),
        Some("balanced") => Ok(Some(Stringency::Balanced)),
        Some("lenient") => Ok(Some(Stringency::Lenient)),
        Some("exploratory") => Ok(Some(Stringency::Exploratory)),
        Some(other) => Err(JsError::new(&format!(
            "unknown stringency '{other}'; expected strict, balanced, lenient, or exploratory"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Phase 3: Lens & migration enhancements
// ---------------------------------------------------------------------------

/// Auto-generate up to `top_n` ranked candidate lenses with per-step
/// explanations.
///
/// Returns `MessagePack`-encoded JSON array where each element has
/// fields `quality`, `coverage`, `score`, `strategies_used`, and
/// `steps`. Callers typically decode with `msgpack-lite` or a
/// hand-rolled decoder on the JS side.
///
/// # Errors
///
/// Returns `JsError` if schema handles are invalid, no morphism is
/// found, or serialization fails.
#[wasm_bindgen]
#[allow(clippy::needless_pass_by_value)]
pub fn auto_generate_candidates(
    schema1: u32,
    schema2: u32,
    top_n: u32,
    stringency: Option<String>,
) -> Result<Vec<u8>, JsError> {
    let src = slab::with_resource(schema1, |r| Ok(slab::as_schema(r)?.clone()))?;
    let tgt = slab::with_resource(schema2, |r| Ok(slab::as_schema(r)?.clone()))?;
    let protocol =
        lookup_builtin_protocol(&src.protocol).unwrap_or_else(|| default_protocol(&src.protocol));

    let mut config = lens::AutoLensConfig::default();
    if let Some(s) = parse_stringency(stringency.as_deref())? {
        config.stringency = s;
    }
    let candidates = lens::auto_generate_candidates(&src, &tgt, &protocol, &config, top_n as usize)
        .map_err(|e| WasmError::LensConstructionFailed {
            reason: e.to_string(),
        })?;

    let payload: Vec<serde_json::Value> = candidates
        .iter()
        .map(|c| {
            serde_json::json!({
                "quality": c.quality,
                "coverage": c.coverage,
                "score": c.score(),
                "strategies_used": c.strategies_used,
                "steps": c.steps.iter().map(|s| serde_json::json!({
                    "kind": s.kind,
                    "explanation": s.explanation,
                    "confidence": s.confidence,
                    "strategy": s.strategy,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    rmp_serde::to_vec_named(&payload).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Auto-generate a protolens chain between two schemas.
///
/// `stringency` selects which alignment strategies run (one of
/// `"strict" | "balanced" | "lenient" | "exploratory"`; empty/unset
/// uses the default).
///
/// Returns a handle to the `ProtolensChain` resource.
///
/// # Errors
///
/// Returns `JsError` if schema handles are invalid, no morphism is
/// found, or protolens generation fails.
#[wasm_bindgen]
#[allow(clippy::needless_pass_by_value)]
pub fn auto_generate_protolens(
    schema1: u32,
    schema2: u32,
    stringency: Option<String>,
) -> Result<u32, JsError> {
    let src = slab::with_resource(schema1, |r| Ok(slab::as_schema(r)?.clone()))?;
    let tgt = slab::with_resource(schema2, |r| Ok(slab::as_schema(r)?.clone()))?;

    // Extract protocol from schema name and look it up
    let protocol =
        lookup_builtin_protocol(&src.protocol).unwrap_or_else(|| default_protocol(&src.protocol));

    let mut config = lens::AutoLensConfig::default();
    if let Some(s) = parse_stringency(stringency.as_deref())? {
        config.stringency = s;
    }
    let result = lens::auto_generate(&src, &tgt, &protocol, &config).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        result.chain,
    ))))
}

/// Check both `GetPut` and `PutGet` lens laws on a test instance.
///
/// The `instance` bytes are `MessagePack`-encoded `WInstance`.
/// Returns `MessagePack`-encoded result: `{ "holds": bool, "violation": string | null }`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_lens_laws(migration: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;
        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };
        match lens::check_laws(&lens_obj, &instance) {
            Ok(()) => Ok(LawCheckResult {
                holds: true,
                violation: None,
            }),
            Err(e) => Ok(LawCheckResult {
                holds: false,
                violation: Some(e.to_string()),
            }),
        }
    })?;

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Check the `GetPut` lens law on a test instance.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_get_put(migration: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;
        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };
        match lens::check_get_put(&lens_obj, &instance) {
            Ok(()) => Ok(LawCheckResult {
                holds: true,
                violation: None,
            }),
            Err(e) => Ok(LawCheckResult {
                holds: false,
                violation: Some(e.to_string()),
            }),
        }
    })?;

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Check the `PutGet` lens law on a test instance.
///
/// The `instance` bytes are `MessagePack`-encoded `WInstance`.
/// Internally calls get to obtain a view/complement, then verifies `PutGet`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_put_get(migration: u32, instance_bytes: &[u8]) -> Result<Vec<u8>, JsError> {
    let instance: WInstance =
        rmp_serde::from_slice(instance_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_resource(migration, |r| {
        let (compiled, src_schema, tgt_schema) = extract_migration_owned(r)?;
        let lens_obj = lens::Lens {
            compiled,
            src_schema,
            tgt_schema,
        };
        match lens::check_put_get(&lens_obj, &instance) {
            Ok(()) => Ok(LawCheckResult {
                holds: true,
                violation: None,
            }),
            Err(e) => Ok(LawCheckResult {
                holds: false,
                violation: Some(e.to_string()),
            }),
        }
    })?;

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Invert a bijective migration.
///
/// The `mapping` bytes are `MessagePack`-encoded `Migration`.
/// Returns `MessagePack`-encoded `Migration` (the inverse) on success,
/// or a `JsError` if the migration is not bijective.
///
/// # Errors
///
/// Returns `JsError` if the migration is not invertible.
#[wasm_bindgen]
pub fn invert_migration(mapping: &[u8], src: u32, tgt: u32) -> Result<Vec<u8>, JsError> {
    let migration: Migration =
        rmp_serde::from_slice(mapping).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (src_schema, tgt_schema) = slab::with_two_resources(src, tgt, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let inverse =
        mig::invert(&migration, &src_schema, &tgt_schema).map_err(|e| WasmError::InvertFailed {
            reason: e.to_string(),
        })?;

    rmp_serde::to_vec_named(&inverse).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Compose two lenses sequentially.
///
/// Returns a handle to the composed lens.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid or composition fails.
#[wasm_bindgen]
pub fn compose_lenses(l1: u32, l2: u32) -> Result<u32, JsError> {
    let (lens1, lens2) = slab::with_two_resources(l1, l2, |r1, r2| {
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

    let composed = lens::compose(&lens1, &lens2).map_err(|e| WasmError::ComposeFailed {
        reason: e.to_string(),
    })?;

    Ok(slab::alloc(Resource::MigrationWithSchemas {
        compiled: composed.compiled,
        src_schema: std::sync::Arc::new(composed.src_schema),
        tgt_schema: std::sync::Arc::new(composed.tgt_schema),
    }))
}

// ---------------------------------------------------------------------------
// Phase 9: Protolens operations
// ---------------------------------------------------------------------------

/// Instantiate a protolens chain at a specific schema.
///
/// Returns a handle to the resulting compiled lens (stored as
/// `MigrationWithSchemas`).
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or instantiation fails.
#[wasm_bindgen]
pub fn instantiate_protolens(chain: u32, schema: u32) -> Result<u32, JsError> {
    let chain_val = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let schema_val = slab::with_resource(schema, |r| Ok(slab::as_schema(r)?.clone()))?;

    let protocol = lookup_builtin_protocol(&schema_val.protocol)
        .unwrap_or_else(|| default_protocol(&schema_val.protocol));

    let lens_obj = chain_val.instantiate(&schema_val, &protocol).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::MigrationWithSchemas {
        compiled: lens_obj.compiled,
        src_schema: std::sync::Arc::new(lens_obj.src_schema),
        tgt_schema: std::sync::Arc::new(lens_obj.tgt_schema),
    }))
}

/// Get the complement spec for a protolens chain at a schema.
///
/// Returns `MessagePack`-encoded [`ComplementSpec`](panproto_core::lens::ComplementSpec).
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_complement_spec(chain: u32, schema: u32) -> Result<Vec<u8>, JsError> {
    let (chain_val, schema_val) = slab::with_two_resources(chain, schema, |r1, r2| {
        let chain_val = slab::as_protolens_chain(r1)?.clone();
        let schema_val = slab::as_schema(r2)?.clone();
        Ok((chain_val, schema_val))
    })?;

    let protocol = lookup_builtin_protocol(&schema_val.protocol)
        .unwrap_or_else(|| default_protocol(&schema_val.protocol));

    let result = lens::chain_complement_spec(&chain_val, &schema_val, &protocol);

    rmp_serde::to_vec_named(&result).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Build a protolens chain from a diff spec.
///
/// The `diff_bytes` are `MessagePack`-encoded [`DiffSpec`](panproto_core::lens::DiffSpec).
/// Returns a handle to the `ProtolensChain` resource.
///
/// # Errors
///
/// Returns `JsError` if deserialization fails, handles are invalid,
/// or diff-to-protolens conversion fails.
#[wasm_bindgen]
pub fn protolens_from_diff(diff_bytes: &[u8], schema1: u32, schema2: u32) -> Result<u32, JsError> {
    let diff: lens::DiffSpec =
        rmp_serde::from_slice(diff_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (src, tgt) = slab::with_two_resources(schema1, schema2, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let chain = lens::diff_to_protolens(&diff, &src, &tgt).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(chain))))
}

/// Compose two protolens chains.
///
/// Returns a handle to the composed `ProtolensChain`.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid.
#[wasm_bindgen]
pub fn protolens_compose(chain1: u32, chain2: u32) -> Result<u32, JsError> {
    let (c1, c2) = slab::with_two_resources(chain1, chain2, |r1, r2| {
        let ch1 = slab::as_protolens_chain(r1)?;
        let ch2 = slab::as_protolens_chain(r2)?;
        Ok((ch1.clone(), ch2.clone()))
    })?;

    let mut combined_steps = c1.steps;
    combined_steps.extend(c2.steps);

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        lens::ProtolensChain::new(combined_steps),
    ))))
}

/// Serialize a protolens chain to JSON.
///
/// Returns JSON bytes describing each step in the chain (name,
/// source/target endofunctor names, complement type, lossless flag).
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_chain_to_json(chain: u32) -> Result<Vec<u8>, JsError> {
    let steps = slab::with_resource(chain, |r| {
        let chain_val = slab::as_protolens_chain(r)?;
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

    serde_json::to_vec(&steps).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Factorize a theory morphism into elementary endofunctors.
///
/// The `morphism_bytes` are `MessagePack`-encoded [`TheoryMorphism`](panproto_core::gat::TheoryMorphism).
/// `theory1` and `theory2` are handles to the domain and codomain theories.
///
/// Returns `MessagePack`-encoded result with the factorization steps
/// (each step's name and transform description).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails, handles are invalid,
/// or factorization fails.
#[wasm_bindgen]
pub fn factorize_morphism(
    morphism_bytes: &[u8],
    theory1: u32,
    theory2: u32,
) -> Result<Vec<u8>, JsError> {
    let morphism: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let (domain, codomain) = slab::with_two_resources(theory1, theory2, |r1, r2| {
        let t1 = slab::as_theory(r1)?;
        let t2 = slab::as_theory(r2)?;
        Ok((t1.clone(), t2.clone()))
    })?;

    let factorization =
        gat::factorize(&morphism, &domain, &codomain).map_err(|e| WasmError::TheoryError {
            reason: e.to_string(),
        })?;

    let steps: Vec<FactorizationStepInfo> = factorization
        .steps
        .iter()
        .map(|ef| FactorizationStepInfo {
            name: ef.name.to_string(),
            transform: format!("{:?}", ef.transform),
        })
        .collect();

    rmp_serde::to_vec_named(&steps).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Auto-generate a symmetric lens from two schemas.
///
/// Returns a handle to the `SymmetricLens` resource.
///
/// # Errors
///
/// Returns `JsError` if schema handles are invalid or symmetric lens
/// generation fails.
#[wasm_bindgen]
pub fn symmetric_lens_from_schemas(schema1: u32, schema2: u32) -> Result<u32, JsError> {
    let (left, right) = slab::with_two_resources(schema1, schema2, |r1, r2| {
        let s1 = slab::as_schema(r1)?;
        let s2 = slab::as_schema(r2)?;
        Ok((s1.clone(), s2.clone()))
    })?;

    let protocol =
        lookup_builtin_protocol(&left.protocol).unwrap_or_else(|| default_protocol(&left.protocol));
    let config = lens::AutoLensConfig::default();

    let sym =
        lens::SymmetricLens::auto_symmetric(&left, &right, &protocol, &config).map_err(|e| {
            WasmError::LensConstructionFailed {
                reason: e.to_string(),
            }
        })?;

    Ok(slab::alloc(Resource::SymmetricLensHandle(Box::new(sym))))
}

/// Sync data through a symmetric lens.
///
/// The `view` and `complement` bytes are `MessagePack`-encoded
/// [`WInstance`] and [`Complement`](panproto_core::lens::Complement) respectively.
/// `direction` is `0` for left-to-right, `1` for right-to-left.
///
/// Returns `MessagePack`-encoded synced `WInstance`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid, deserialization fails,
/// or synchronization fails.
#[wasm_bindgen]
pub fn symmetric_lens_sync(
    sym_lens: u32,
    view: &[u8],
    complement: &[u8],
    direction: u8,
) -> Result<Vec<u8>, JsError> {
    let view_instance: inst::WInstance =
        rmp_serde::from_slice(view).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("view: {e}"),
        })?;

    let comp: lens::Complement =
        rmp_serde::from_slice(complement).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("complement: {e}"),
        })?;

    let (result_view, _result_complement) = slab::with_resource(sym_lens, |r| {
        let sym = slab::as_symmetric_lens(r)?;
        match direction {
            0 => sym.sync_left_to_right(&view_instance, &comp).map_err(|e| {
                WasmError::LensConstructionFailed {
                    reason: e.to_string(),
                }
            }),
            1 => sym.sync_right_to_left(&view_instance, &comp).map_err(|e| {
                WasmError::LensConstructionFailed {
                    reason: e.to_string(),
                }
            }),
            _ => Err(WasmError::LensConstructionFailed {
                reason: format!("invalid direction: {direction}, expected 0 or 1"),
            }),
        }
    })?;

    rmp_serde::to_vec_named(&result_view).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Apply a single protolens step to a schema.
///
/// The `protolens_bytes` are `MessagePack`-encoded protolens step
/// description with fields `name`, `source`, `target`, and
/// `complement_constructor`.
///
/// Returns a handle to the resulting compiled lens (stored as
/// `MigrationWithSchemas`).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails, the schema handle is
/// invalid, or instantiation fails.
#[wasm_bindgen]
pub fn apply_protolens_step(protolens_bytes: &[u8], schema: u32) -> Result<u32, JsError> {
    // Deserialize a single-step protolens chain spec.
    let chain: lens::ProtolensChain = {
        let step: ProtolensStepSpec = rmp_serde::from_slice(protolens_bytes).map_err(|e| {
            WasmError::DeserializationFailed {
                reason: e.to_string(),
            }
        })?;
        build_chain_from_step_spec(&step)?
    };

    let schema_val = slab::with_resource(schema, |r| Ok(slab::as_schema(r)?.clone()))?;
    let protocol = lookup_builtin_protocol(&schema_val.protocol)
        .unwrap_or_else(|| default_protocol(&schema_val.protocol));

    let lens_obj = chain.instantiate(&schema_val, &protocol).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::MigrationWithSchemas {
        compiled: lens_obj.compiled,
        src_schema: std::sync::Arc::new(lens_obj.src_schema),
        tgt_schema: std::sync::Arc::new(lens_obj.tgt_schema),
    }))
}

/// Compile a lens DSL document (JSON or YAML source) into a
/// `ProtolensChain` resource.
///
/// `source_bytes` is UTF-8 DSL source in the specified `format`:
/// `"json"` or `"yaml"`. Nickel (`"ncl"`) is not supported in the WASM
/// binding because Nickel evaluation requires a filesystem for its
/// contract imports; precompile Nickel → JSON on the host instead.
///
/// `body_vertex` is the parent vertex id under which field-level steps
/// (e.g. `rename_field`, `add_field`) attach — typically the record's
/// `:body` object, such as `"app.bsky.feed.post:body"`.
///
/// Returns a handle to the compiled `ProtolensChain`.
///
/// # Errors
///
/// Returns `JsError` if `format` is unknown, the source fails to parse,
/// or compilation fails (e.g. references an unknown sort or has
/// inconsistent step metadata).
#[wasm_bindgen]
pub fn compile_lens_document(
    source_bytes: &[u8],
    format: &str,
    body_vertex: &str,
) -> Result<u32, JsError> {
    let source =
        std::str::from_utf8(source_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("invalid UTF-8: {e}"),
        })?;

    let doc = match format {
        "json" => panproto_lens_dsl::eval::eval_json(source),
        "yaml" | "yml" => panproto_lens_dsl::eval::eval_yaml(source),
        other => {
            return Err(WasmError::DeserializationFailed {
                reason: format!("unsupported lens DSL format '{other}'; expected 'json' or 'yaml'"),
            }
            .into());
        }
    }
    .map_err(|e| WasmError::DeserializationFailed {
        reason: e.to_string(),
    })?;

    let compiled = panproto_lens_dsl::compile(&doc, body_vertex, &|_| None).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        compiled.chain,
    ))))
}

/// Deserialize a protolens chain from JSON bytes.
///
/// Returns a handle to the `ProtolensChain` resource.
///
/// # Errors
///
/// Returns `JsError` if the JSON is invalid.
#[wasm_bindgen]
pub fn protolens_from_json(json_bytes: &[u8]) -> Result<u32, JsError> {
    let json_str =
        std::str::from_utf8(json_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("invalid UTF-8: {e}"),
        })?;
    let chain = lens::ProtolensChain::from_json(json_str).map_err(|e| {
        WasmError::DeserializationFailed {
            reason: e.to_string(),
        }
    })?;
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(chain))))
}

/// Fuse a protolens chain into a single protolens.
///
/// Composes all steps into a single step with a composite complement.
/// Returns a handle to a new `ProtolensChain` containing the fused step.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or the chain is empty.
#[wasm_bindgen]
pub fn protolens_fuse(chain: u32) -> Result<u32, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let fused = chain_obj
        .fuse()
        .map_err(|e| WasmError::LensConstructionFailed {
            reason: e.to_string(),
        })?;
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        lens::ProtolensChain::new(vec![fused]),
    ))))
}

/// Lift a protolens chain along a theory morphism.
///
/// Given a chain and a `MessagePack`-encoded `TheoryMorphism`, produces
/// a new chain that operates on schemas of the codomain theory.
///
/// # Errors
///
/// Returns `JsError` if the handle is invalid or deserialization fails.
#[wasm_bindgen]
pub fn protolens_lift(chain: u32, morphism_bytes: &[u8]) -> Result<u32, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let morphism: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    let lifted = lens::lift_chain(&chain_obj, &morphism);
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(lifted))))
}

/// Check applicability of a protolens chain against a schema.
///
/// Returns `MessagePack`-encoded JSON: `{ "applicable": bool, "reasons": string[] }`.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_check_applicability(chain: u32, schema: u32) -> Result<Vec<u8>, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;
    let schema_obj = slab::with_resource(schema, |r| Ok(slab::as_schema(r)?.clone()))?;
    let result = chain_obj.check_applicability(&schema_obj);
    let response = match result {
        Ok(()) => serde_json::json!({ "applicable": true, "reasons": Vec::<String>::new() }),
        Err(reasons) => serde_json::json!({ "applicable": false, "reasons": reasons }),
    };
    rmp_serde::to_vec_named(&response).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Apply a protolens chain to a fleet of schemas.
///
/// The `schema_handles` are slab handles to `Schema` resources.
/// Each schema's name is taken from its `protocol` field.
///
/// Returns `MessagePack`-encoded fleet result:
/// `{ "applied": [name, ...], "skipped": [[name, [reasons]], ...] }`.
///
/// # Errors
///
/// Returns `JsError` if any handle is invalid or serialization fails.
#[wasm_bindgen]
pub fn protolens_fleet(chain: u32, schema_handles: &[u32]) -> Result<Vec<u8>, JsError> {
    let chain_obj = slab::with_resource(chain, |r| Ok(slab::as_protolens_chain(r)?.clone()))?;

    // Load all schemas from handles.
    let mut schemas: Vec<(panproto_core::gat::Name, panproto_core::schema::Schema)> = Vec::new();
    for (i, &handle) in schema_handles.iter().enumerate() {
        let schema = slab::with_resource(handle, |r| Ok(slab::as_schema(r)?.clone()))?;
        let name = panproto_core::gat::Name::from(format!("schema_{i}"));
        schemas.push((name, schema));
    }

    // Determine protocol from first schema.
    let protocol = if let Some((_, first)) = schemas.first() {
        lookup_builtin_protocol(&first.protocol)
            .unwrap_or_else(|| default_protocol(&first.protocol))
    } else {
        return rmp_serde::to_vec_named(&serde_json::json!({
            "applied": Vec::<String>::new(),
            "skipped": Vec::<String>::new(),
        }))
        .map_err(|e| JsError::new(&e.to_string()));
    };

    let result = lens::apply_to_fleet(&chain_obj, &schemas, &protocol);

    let applied: Vec<String> = result.applied.iter().map(|(n, _)| n.to_string()).collect();
    let skipped: Vec<(String, Vec<String>)> = result
        .skipped
        .iter()
        .map(|(n, reasons)| (n.to_string(), reasons.clone()))
        .collect();

    let response = serde_json::json!({
        "applied": applied,
        "skipped": skipped,
    });

    rmp_serde::to_vec_named(&response).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

/// Build a protolens chain from a pipeline of step specs.
///
/// Takes `MessagePack`-encoded array of `ProtolensStepSpec` objects.
/// Returns a handle to the composed `ProtolensChain`.
///
/// # Errors
///
/// Returns `JsError` if deserialization or chain construction fails.
#[wasm_bindgen]
pub fn protolens_pipeline(steps_bytes: &[u8]) -> Result<u32, JsError> {
    let specs: Vec<ProtolensStepSpec> =
        rmp_serde::from_slice(steps_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let mut chains = Vec::with_capacity(specs.len());
    for spec in &specs {
        chains.push(build_chain_from_step_spec(spec)?);
    }

    let combined = lens::combinators::pipeline(chains);
    Ok(slab::alloc(Resource::ProtolensChain(Box::new(combined))))
}

/// Auto-generate a protolens with initial morphism hints.
///
/// The `hints_bytes` are `MessagePack`-encoded `HashMap<String, String>`
/// mapping source vertex names to target vertex names. These are used
/// as seed correspondences for the morphism search, enabling alignment
/// across schemas with different NSID namespaces.
///
/// Returns a handle to the generated `ProtolensChain`.
///
/// # Errors
///
/// Returns `JsError` if no morphism is found even with hints.
#[wasm_bindgen]
#[allow(clippy::needless_pass_by_value)]
pub fn auto_generate_protolens_with_hints(
    schema1: u32,
    schema2: u32,
    hints_bytes: &[u8],
    stringency: Option<String>,
) -> Result<u32, JsError> {
    let hints: std::collections::HashMap<String, String> = rmp_serde::from_slice(hints_bytes)
        .map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let s1 = slab::with_resource(schema1, |r| Ok(slab::as_schema(r)?.clone()))?;
    let s2 = slab::with_resource(schema2, |r| Ok(slab::as_schema(r)?.clone()))?;
    let protocol =
        lookup_builtin_protocol(&s1.protocol).unwrap_or_else(|| default_protocol(&s1.protocol));

    let mut initial = std::collections::HashMap::new();
    for (src, tgt) in &hints {
        initial.insert(gat::Name::from(src.as_str()), gat::Name::from(tgt.as_str()));
    }
    let mut config = lens::auto_lens::AutoLensConfig {
        try_overlap: true,
        search_opts: panproto_core::mig::hom_search::SearchOptions {
            initial,
            ..Default::default()
        },
        ..Default::default()
    };
    if let Some(s) = parse_stringency(stringency.as_deref())? {
        config.stringency = s;
    }

    let result = lens::auto_lens::auto_generate(&s1, &s2, &protocol, &config).map_err(|e| {
        WasmError::LensConstructionFailed {
            reason: e.to_string(),
        }
    })?;

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        result.chain,
    ))))
}

/// Auto-generate a protolens chain with a full hint specification.
///
/// Accepts `MessagePack`-encoded [`panproto_lens_dsl::HintSpec`]:
/// `{ anchors: { src: tgt, ... }, constraints: [...] }`.
///
/// Runs forward-chaining anchor derivation and constrained morphism search.
///
/// Returns a handle to the generated [`ProtolensChain`].
#[wasm_bindgen]
pub fn auto_generate_protolens_with_hint_spec(
    schema1: u32,
    schema2: u32,
    hint_spec_bytes: &[u8],
) -> Result<u32, JsError> {
    let hint_spec: panproto_lens_dsl::HintSpec =
        rmp_serde::from_slice(hint_spec_bytes).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let s1 = slab::with_resource(schema1, |r| Ok(slab::as_schema(r)?.clone()))?;
    let s2 = slab::with_resource(schema2, |r| Ok(slab::as_schema(r)?.clone()))?;
    let protocol =
        lookup_builtin_protocol(&s1.protocol).unwrap_or_else(|| default_protocol(&s1.protocol));

    let parts = lens::hint::HintParts {
        anchors: hint_spec.anchors.clone(),
        scope_pairs: hint_spec.scope_pairs(),
        excluded_targets: hint_spec.excluded_target_names(),
        excluded_sources: hint_spec.excluded_source_names(),
        scoring_weights: hint_spec.scoring_weights(),
        name_similarity_threshold: hint_spec.name_similarity_threshold(),
    };
    let (derived, domain_constraints) = lens::hint::resolve_hints(&parts, &s1, &s2);

    let mut config = lens::auto_lens::AutoLensConfig {
        try_overlap: true,
        ..Default::default()
    };
    if let Some(s) = hint_spec.stringency {
        config.stringency = match s {
            panproto_lens_dsl::HintStringency::Strict => Stringency::Strict,
            panproto_lens_dsl::HintStringency::Balanced => Stringency::Balanced,
            panproto_lens_dsl::HintStringency::Lenient => Stringency::Lenient,
            panproto_lens_dsl::HintStringency::Exploratory => Stringency::Exploratory,
        };
    }
    for cluster in &hint_spec.alias_clusters {
        config.alias_dict.add_cluster(cluster);
    }

    let result = lens::auto_lens::auto_generate_with_hints(
        &s1,
        &s2,
        &protocol,
        &config,
        &derived,
        &domain_constraints,
        None,
    )
    .map_err(|e| WasmError::LensConstructionFailed {
        reason: e.to_string(),
    })?;

    Ok(slab::alloc(Resource::ProtolensChain(Box::new(
        result.chain,
    ))))
}
