//! GAT operations: theories, morphisms, model migration.
//!
//! Split out of the monolithic api.rs into a domain module.

use panproto_core::gat::{self};
use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use crate::error::WasmError;
use crate::slab::{self, Resource};

use super::helpers::MorphismCheckResult;

// ---------------------------------------------------------------------------
// Phase 5: GAT operations
// ---------------------------------------------------------------------------

/// Create a theory from a `MessagePack` spec. Returns handle.
///
/// The `spec` bytes are `MessagePack`-encoded [`Theory`](panproto_core::gat::Theory).
///
/// # Errors
///
/// Returns `JsError` if deserialization fails.
#[wasm_bindgen]
pub fn create_theory(spec: &[u8]) -> Result<u32, JsError> {
    let theory: gat::Theory =
        rmp_serde::from_slice(spec).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;
    Ok(slab::alloc(Resource::Theory(Box::new(theory))))
}

/// Compute colimit of two theories over a shared base. Returns handle.
///
/// # Errors
///
/// Returns `JsError` if any handle is invalid or the colimit fails.
#[wasm_bindgen]
pub fn colimit_theories(t1: u32, t2: u32, shared: u32) -> Result<u32, JsError> {
    let result = slab::with_three_resources(t1, t2, shared, |r1, r2, r3| {
        let th1 = slab::as_theory(r1)?;
        let th2 = slab::as_theory(r2)?;
        let th_shared = slab::as_theory(r3)?;
        gat::colimit_by_name(th1, th2, th_shared).map_err(|e| WasmError::ColimitFailed {
            reason: e.to_string(),
        })
    })?;
    Ok(slab::alloc(Resource::Theory(Box::new(result))))
}

/// Check morphism validity. Returns `MessagePack` result.
///
/// The `morphism` bytes are `MessagePack`-encoded `TheoryMorphism`.
/// Returns `MessagePack`-encoded result: `{ "valid": bool, "error": string | null }`.
///
/// # Errors
///
/// Returns `JsError` if handles are invalid or deserialization fails.
#[wasm_bindgen]
pub fn check_morphism(morphism: &[u8], domain: u32, codomain: u32) -> Result<Vec<u8>, JsError> {
    let morph: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism).map_err(|e| WasmError::DeserializationFailed {
            reason: e.to_string(),
        })?;

    let result = slab::with_two_resources(domain, codomain, |r1, r2| {
        let dom = slab::as_theory(r1)?;
        let cod = slab::as_theory(r2)?;
        match gat::check_morphism(&morph, dom, cod) {
            Ok(()) => Ok(MorphismCheckResult {
                valid: true,
                error: None,
            }),
            Err(e) => Ok(MorphismCheckResult {
                valid: false,
                error: Some(e.to_string()),
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

/// Migrate a model through a morphism. Returns `MessagePack` model.
///
/// The `model` and `morphism` bytes are `MessagePack`-encoded
/// `Model` and `TheoryMorphism` respectively.
///
/// Note: Only the sort interpretations can be serialized; operation
/// interpretations (functions) cannot cross the WASM boundary. This
/// returns a `MessagePack` result containing the reindexed sort
/// interpretations.
///
/// # Errors
///
/// Returns `JsError` if deserialization or migration fails.
#[wasm_bindgen]
pub fn migrate_model(model: &[u8], morphism: &[u8]) -> Result<Vec<u8>, JsError> {
    let morph: gat::TheoryMorphism =
        rmp_serde::from_slice(morphism).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("morphism: {e}"),
        })?;

    // Models contain function pointers and cannot be fully serialized.
    // We serialize only the sort_interp portion and reindex it.
    let sort_interp: HashMap<String, Vec<gat::ModelValue>> =
        rmp_serde::from_slice(model).map_err(|e| WasmError::DeserializationFailed {
            reason: format!("model sort_interp: {e}"),
        })?;

    // Reindex sort interpretations according to the morphism's sort_map.
    let mut reindexed: HashMap<String, Vec<gat::ModelValue>> = HashMap::new();
    for (domain_sort, codomain_sort) in &morph.sort_map {
        if let Some(values) = sort_interp.get(codomain_sort.as_ref()) {
            reindexed.insert(domain_sort.to_string(), values.clone());
        }
    }

    rmp_serde::to_vec_named(&reindexed).map_err(|e| -> JsError {
        WasmError::SerializationFailed {
            reason: e.to_string(),
        }
        .into()
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn create_theory_from_minimal_spec() {
        let theory = panproto_core::gat::Theory::new("SmokeTheory", vec![], vec![], vec![]);
        let spec = rmp_serde::to_vec_named(&theory).unwrap();
        let handle = create_theory(&spec).unwrap();
        assert_ne!(handle, u32::MAX, "theory handle should be allocated");
    }
}
