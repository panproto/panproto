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
                    stages: compiled_ref.stages,
                });
            }
            LensRef::Inline { inline } => {
                let compiled = steps::compile_steps(&inline.steps, body_vertex)?;
                compiled_parts.push(compiled);
            }
        }
    }

    compose_parts(&compiled_parts, spec.mode)
}

/// Combine the compiled parts of a composition according to `mode`.
fn compose_parts(
    compiled_parts: &[steps::CompiledSteps],
    mode: ComposeMode,
) -> Result<steps::CompiledSteps, LensDslError> {
    match mode {
        ComposeMode::Vertical => {
            // Vertical: flatten all chains into a single pipeline.
            let chains: Vec<ProtolensChain> =
                compiled_parts.iter().map(|c| c.chain.clone()).collect();

            let mut all_transforms = std::collections::HashMap::new();
            let mut stages = Vec::new();
            for part in compiled_parts {
                for (k, v) in &part.field_transforms {
                    all_transforms
                        .entry(k.clone())
                        .or_insert_with(Vec::new)
                        .extend(v.clone());
                }
                stages.extend(part.stages.iter().cloned());
            }

            Ok(steps::CompiledSteps {
                chain: combinators::pipeline(chains),
                field_transforms: all_transforms,
                stages,
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
                    stages: Vec::new(),
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
            for part in compiled_parts {
                for (k, v) in &part.field_transforms {
                    all_transforms
                        .entry(k.clone())
                        .or_insert_with(Vec::new)
                        .extend(v.clone());
                }
            }

            let chain = ProtolensChain::new(vec![fused]);
            let stages = vec![steps::CompiledStage {
                chain: chain.clone(),
                field_transforms: all_transforms.clone(),
            }];
            Ok(steps::CompiledSteps {
                chain,
                field_transforms: all_transforms,
                stages,
            })
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::document::{InlineLens, Step};

    const BODY: &str = "record:body";

    fn inline_remove(field: &str) -> LensRef {
        LensRef::Inline {
            inline: InlineLens {
                steps: vec![Step::RemoveField {
                    remove_field: field.to_owned(),
                }],
            },
        }
    }

    #[test]
    fn inline_vertical_compose_concatenates_chains() {
        let spec = ComposeSpec {
            mode: ComposeMode::Vertical,
            lenses: vec![inline_remove("a"), inline_remove("b")],
        };
        let compiled = compile_compose(&spec, BODY, &|_| None).unwrap();
        // Two single-step inline lenses flatten to a two-step pipeline.
        assert_eq!(compiled.chain.steps.len(), 2);
    }

    #[test]
    fn unresolved_named_ref_errors() {
        let spec = ComposeSpec {
            mode: ComposeMode::Vertical,
            lenses: vec![LensRef::Ref {
                r#ref: "does.not.exist".to_owned(),
            }],
        };
        let err = compile_compose(&spec, BODY, &|_| None).unwrap_err();
        match err {
            LensDslError::UnresolvedRef { lens_ref } => assert_eq!(lens_ref, "does.not.exist"),
            other => panic!("expected UnresolvedRef, got {other:?}"),
        }
    }

    #[test]
    fn named_ref_resolves_and_chains_concatenate() {
        // A referenced lens document (one remove step), compiled and
        // handed back by the resolver.
        let referenced_src = r#"{
            "id": "dev.example.drop-a",
            "source": "s",
            "target": "t",
            "steps": [{ "remove_field": "a" }]
        }"#;
        let referenced_doc = crate::eval::eval_json(referenced_src).unwrap();
        let referenced = crate::compile::compile(&referenced_doc, BODY, &|_| None).unwrap();
        let referenced_len = referenced.chain.steps.len();
        assert!(referenced_len > 0, "referenced lens must contribute steps");

        let resolver = |id: &str| -> Option<CompiledLens> {
            (id == "dev.example.drop-a").then(|| referenced.clone())
        };

        // A compose that references the sibling by name, then removes an
        // inline field. The compiled chain must be the concatenation.
        let spec = ComposeSpec {
            mode: ComposeMode::Vertical,
            lenses: vec![
                LensRef::Ref {
                    r#ref: "dev.example.drop-a".to_owned(),
                },
                inline_remove("b"),
            ],
        };
        let compiled = compile_compose(&spec, BODY, &resolver).unwrap();
        assert_eq!(
            compiled.chain.steps.len(),
            referenced_len + 1,
            "named-ref + inline should concatenate their chains"
        );
    }

    #[test]
    fn horizontal_compose_of_empty_parts_is_empty() {
        let spec = ComposeSpec {
            mode: ComposeMode::Horizontal,
            lenses: vec![],
        };
        let compiled = compile_compose(&spec, BODY, &|_| None).unwrap();
        assert!(compiled.chain.steps.is_empty());
    }
}
