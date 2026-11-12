//! Lens composition via named references.
//!
//! The `compose` body variant allows lenses to be composed from
//! references to other lens documents (resolved via a callback)
//! or inline step definitions. Supports both vertical (sequential)
//! and horizontal (parallel) composition modes.
//!
//! Vertical composition concatenates the parts' stages in order, so
//! composing compiled parts equals compiling the concatenation of their
//! steps. Horizontal composition fuses each part's chain into one
//! protolens and folds the fused protolenses from the left with
//! [`protolens_horizontal`]; the result is one stage carrying every
//! part's value-level transforms in part order.

use panproto_lens::{ProtolensChain, protolens_horizontal};

use crate::compile::CompiledLens;
use crate::document::{ComposeMode, ComposeSpec, LensRef};
use crate::error::LensDslError;
use crate::path::DocumentPath;
use crate::steps;

/// A resolver for named references: `None` when the name resolves to
/// nothing, else the referenced lens compiled or its refusal.
pub type Resolve<'a> = dyn Fn(&str) -> Option<Result<CompiledLens, LensDslError>> + 'a;

/// Compile a composition specification.
///
/// Resolves `ref` entries via the `resolver` callback, compiles
/// inline lens definitions, and composes the results.
///
/// # Errors
///
/// As [`compile_compose_with`].
pub fn compile_compose(
    spec: &ComposeSpec,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<steps::CompiledSteps, LensDslError> {
    compile_compose_with(spec, body_vertex, &|id: &str| resolver(id).map(Ok))
}

/// Compile a composition specification, the resolver reporting a
/// referenced lens's refusal.
///
/// # Errors
///
/// Refusals located relative to the `compose` object:
/// [`LensDslError::UnresolvedRef`],
/// [`LensDslError::SymmetricRefInDirectionalCompose`] and the resolver's
/// own refusal at `/lenses/i/ref`; an inline lens's refusal at
/// `/lenses/i/inline/steps/...`; and, for a horizontal composition,
/// [`LensDslError::EmptyHorizontalPart`] or [`LensDslError::Fuse`] at
/// `/lenses/i`.
pub fn compile_compose_with(
    spec: &ComposeSpec,
    body_vertex: &str,
    resolver: &Resolve<'_>,
) -> Result<steps::CompiledSteps, LensDslError> {
    let mut parts: Vec<steps::CompiledSteps> = Vec::with_capacity(spec.lenses.len());
    for (i, lens_ref) in spec.lenses.iter().enumerate() {
        let here = DocumentPath::root().key("lenses").index(i);
        match lens_ref {
            LensRef::Ref { r#ref } => {
                let at_ref = here.key("ref");
                let compiled_ref = resolver(r#ref)
                    .ok_or_else(|| LensDslError::UnresolvedRef {
                        lens_ref: r#ref.clone(),
                    })
                    .and_then(|resolved| resolved)
                    .map_err(|e| e.within(&at_ref))?;
                if compiled_ref.symmetric.is_some() {
                    return Err(LensDslError::SymmetricRefInDirectionalCompose {
                        lens_ref: r#ref.clone(),
                    }
                    .within(&at_ref));
                }
                parts.push(steps::CompiledSteps {
                    chain: compiled_ref.chain,
                    field_transforms: compiled_ref.field_transforms,
                    stages: compiled_ref.stages,
                });
            }
            LensRef::Inline { inline } => {
                parts.push(
                    steps::compile_steps(&inline.steps, body_vertex)
                        .map_err(|e| e.within(&here.key("inline").key("steps")))?,
                );
            }
        }
    }
    match spec.mode {
        ComposeMode::Vertical => Ok(steps::concat(parts)),
        ComposeMode::Horizontal => horizontal(parts),
    }
}

/// Compose parts horizontally: zero parts give the empty composite, and
/// otherwise the fused parts are folded from the left.
fn horizontal(parts: Vec<steps::CompiledSteps>) -> Result<steps::CompiledSteps, LensDslError> {
    let mut fused = None;
    let mut field_transforms: std::collections::HashMap<_, Vec<_>> =
        std::collections::HashMap::new();
    for (i, part) in parts.into_iter().enumerate() {
        let here = DocumentPath::root().key("lenses").index(i);
        if part.chain.steps.is_empty() {
            return Err(LensDslError::EmptyHorizontalPart.within(&here));
        }
        let this = part
            .chain
            .fuse()
            .map_err(|e| LensDslError::Fuse { error: Box::new(e) }.within(&here))?;
        fused = Some(match fused {
            None => this,
            Some(acc) => protolens_horizontal(&acc, &this)
                .map_err(|e| LensDslError::Fuse { error: Box::new(e) }.within(&here))?,
        });
        for stage in part.stages {
            for (anchor, entries) in stage.field_transforms {
                field_transforms.entry(anchor).or_default().extend(entries);
            }
        }
    }
    let Some(fused) = fused else {
        return Ok(steps::concat(Vec::new()));
    };
    let chain = ProtolensChain::new(vec![fused]);
    Ok(steps::CompiledSteps {
        chain: chain.clone(),
        field_transforms: field_transforms.clone(),
        stages: vec![steps::CompiledStage {
            chain,
            field_transforms,
        }],
    })
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
            LensDslError::At { path, error } => {
                assert_eq!(path.to_string(), "/lenses/0/ref");
                assert!(
                    matches!(*error, LensDslError::UnresolvedRef { ref lens_ref } if lens_ref == "does.not.exist"),
                    "{error:?}"
                );
            }
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
    fn named_symmetric_ref_is_rejected_by_directional_compose() {
        let referenced_src = r#"{
            "id": "dev.example.symmetric",
            "source": "left",
            "target": "right",
            "symmetric": {
                "left": [{ "remove_field": "left-only" }],
                "right": [{ "remove_field": "right-only" }]
            }
        }"#;
        let referenced_doc = crate::eval::eval_json(referenced_src).unwrap();
        let referenced = crate::compile::compile(&referenced_doc, BODY, &|_| None).unwrap();
        assert!(
            referenced.symmetric.is_some(),
            "the referenced fixture must retain both symmetric legs"
        );

        let spec = ComposeSpec {
            mode: ComposeMode::Vertical,
            lenses: vec![LensRef::Ref {
                r#ref: "dev.example.symmetric".to_owned(),
            }],
        };
        // A directed compose cannot consume a symmetric reference.
        let error = compile_compose(&spec, BODY, &|id| {
            (id == "dev.example.symmetric").then(|| referenced.clone())
        })
        .unwrap_err();

        assert!(
            matches!(
                error.unlocated(),
                LensDslError::SymmetricRefInDirectionalCompose { lens_ref }
                    if lens_ref == "dev.example.symmetric"
            ),
            "the rejection must identify the referenced symmetric document"
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
