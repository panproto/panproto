//! Unified compilation dispatcher.
//!
//! Takes a [`LensDocument`] and produces a [`CompiledLens`] by
//! dispatching to the appropriate body-variant compiler (steps,
//! rules, compose, or auto).

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::FieldTransform;
use panproto_lens::ProtolensChain;

use crate::document::{AutoSpec, LensDocument};
use crate::error::LensDslError;

/// The compiled output of a lens document.
///
/// Contains both the schema-level [`ProtolensChain`] and the
/// value-level [`FieldTransform`]s, along with metadata from
/// the original document.
///
/// When the `auto` body variant is used, `chain` is empty and
/// `auto_spec` is `Some`. The caller must invoke
/// `panproto_lens::auto_lens::auto_generate` with the spec
/// parameters and actual schema/protocol context.
#[derive(Debug, Clone)]
pub struct CompiledLens {
    /// The lens document ID.
    pub id: String,
    /// Source schema NSID.
    pub source: String,
    /// Target schema NSID.
    pub target: String,
    /// The compiled protolens chain (schema-level transforms).
    pub chain: ProtolensChain,
    /// Value-level field transforms, keyed by parent vertex.
    pub field_transforms: HashMap<Name, Vec<FieldTransform>>,
    /// Protocol-specific extension metadata (opaque).
    pub extensions: HashMap<String, serde_json::Value>,
    /// Auto-generation spec, if the `auto` body variant was used.
    /// When present, `chain` is empty and the caller must invoke
    /// auto-generation with actual schema/protocol context.
    pub auto_spec: Option<AutoSpec>,
}

/// Compile a [`LensDocument`] into a [`CompiledLens`].
///
/// The `body_vertex` parameter specifies the parent vertex ID under
/// which fields are added/removed (e.g., `"record:body"` for `ATProto`).
///
/// The `resolver` callback is used to resolve lens references in
/// `compose` bodies. It receives a lens ID and should return the
/// already-compiled lens, or `None` if not found.
///
/// # Errors
///
/// Returns [`LensDslError::NoBody`] if no body variant is present,
/// [`LensDslError::MultipleBodies`] if more than one is present,
/// or propagates errors from the body-specific compiler.
pub fn compile(
    doc: &LensDocument,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<CompiledLens, LensDslError> {
    // Validate exactly one body variant.
    let mut present = Vec::new();
    if doc.steps.is_some() {
        present.push("steps");
    }
    if doc.rules.is_some() {
        present.push("rules");
    }
    if doc.compose.is_some() {
        present.push("compose");
    }
    if doc.auto.is_some() {
        present.push("auto");
    }

    if present.is_empty() {
        return Err(LensDslError::NoBody { id: doc.id.clone() });
    }

    if present.len() > 1 {
        return Err(LensDslError::MultipleBodies {
            id: doc.id.clone(),
            variants: present.join(", "),
        });
    }

    let compiled = if let Some(steps) = &doc.steps {
        crate::steps::compile_steps(steps, body_vertex)?
    } else if let Some(rules) = &doc.rules {
        crate::rules::compile_rules(rules, doc.passthrough, body_vertex)?
    } else if let Some(compose) = &doc.compose {
        crate::compose::compile_compose(compose, body_vertex, resolver)?
    } else if doc.auto.is_some() {
        // Auto-generation requires schema and protocol context that
        // the DSL compiler does not have. Return an empty chain; the
        // caller is expected to use panproto_lens::auto_lens::auto_generate
        // directly with the AutoSpec parameters.
        crate::steps::CompiledSteps {
            chain: ProtolensChain::new(vec![]),
            field_transforms: HashMap::new(),
        }
    } else {
        unreachable!("validated above");
    };

    Ok(CompiledLens {
        id: doc.id.clone(),
        source: doc.source.clone(),
        target: doc.target.clone(),
        chain: compiled.chain,
        field_transforms: compiled.field_transforms,
        extensions: doc.extensions.clone(),
        auto_spec: doc.auto.clone(),
    })
}
