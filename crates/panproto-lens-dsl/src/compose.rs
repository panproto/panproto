//! Lens composition via named references.
//!
//! The `compose` body variant allows lenses to be composed from
//! references to other lens documents (resolved via a callback)
//! or inline step definitions. Supports both vertical (sequential)
//! and horizontal (parallel) composition modes.

use panproto_lens::{ProtolensChain, combinators, protolens_horizontal};

use crate::compile::CompiledLens;
use crate::document::{ComposeMode, ComposeSpec, LensRef};
use crate::error::LensDslError;
use crate::steps;

/// Compile a composition specification.
///
/// Resolves `ref` entries via the `resolver` callback, compiles
/// inline lens definitions, and composes the results.
///
/// # Errors
///
/// Returns [`LensDslError::UnresolvedRef`] if a reference cannot
/// be resolved, or propagates compilation errors from inline lenses.
pub fn compile_compose(
    spec: &ComposeSpec,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<steps::CompiledSteps, LensDslError> {
    let mut compiled_parts: Vec<steps::CompiledSteps> = Vec::new();

    for lens_ref in &spec.lenses {
        match lens_ref {
            LensRef::Ref { r#ref } => {
                let compiled_ref = resolver(r#ref).ok_or_else(|| LensDslError::UnresolvedRef {
                    lens_ref: r#ref.clone(),
                })?;
                compiled_parts.push(steps::CompiledSteps {
                    chain: compiled_ref.chain,
                    field_transforms: compiled_ref.field_transforms,
                });
            }
            LensRef::Inline { inline } => {
                let compiled = steps::compile_steps(&inline.steps, body_vertex)?;
                compiled_parts.push(compiled);
            }
        }
    }

    match spec.mode {
        ComposeMode::Vertical => {
            // Vertical: flatten all chains into a single pipeline.
            let chains: Vec<ProtolensChain> =
                compiled_parts.iter().map(|c| c.chain.clone()).collect();

            let mut all_transforms = std::collections::HashMap::new();
            for part in &compiled_parts {
                for (k, v) in &part.field_transforms {
                    all_transforms
                        .entry(k.clone())
                        .or_insert_with(Vec::new)
                        .extend(v.clone());
                }
            }

            Ok(steps::CompiledSteps {
                chain: combinators::pipeline(chains),
                field_transforms: all_transforms,
            })
        }

        ComposeMode::Horizontal => {
            // Horizontal composition of natural transformations:
            // Given η : F ⟹ G and θ : F' ⟹ G', produce η * θ : F∘F' ⟹ G∘G'.
            //
            // Each ProtolensChain must first be fused into a single Protolens
            // (vertical composition within each chain), then horizontal
            // composition is applied between the fused protolenses.
            if compiled_parts.is_empty() {
                return Ok(steps::CompiledSteps {
                    chain: ProtolensChain::new(vec![]),
                    field_transforms: std::collections::HashMap::new(),
                });
            }

            let mut fused =
                compiled_parts[0]
                    .chain
                    .fuse()
                    .map_err(|e| LensDslError::ExprParse {
                        step_desc: "horizontal_compose[0].fuse".to_owned(),
                        message: format!("{e}"),
                    })?;

            for (i, part) in compiled_parts[1..].iter().enumerate() {
                let other = part.chain.fuse().map_err(|e| LensDslError::ExprParse {
                    step_desc: format!("horizontal_compose[{}].fuse", i + 1),
                    message: format!("{e}"),
                })?;
                fused =
                    protolens_horizontal(&fused, &other).map_err(|e| LensDslError::ExprParse {
                        step_desc: format!("horizontal_compose[{}]", i + 1),
                        message: format!("{e}"),
                    })?;
            }

            let mut all_transforms = std::collections::HashMap::new();
            for part in &compiled_parts {
                for (k, v) in &part.field_transforms {
                    all_transforms
                        .entry(k.clone())
                        .or_insert_with(Vec::new)
                        .extend(v.clone());
                }
            }

            Ok(steps::CompiledSteps {
                chain: ProtolensChain::new(vec![fused]),
                field_transforms: all_transforms,
            })
        }
    }
}
