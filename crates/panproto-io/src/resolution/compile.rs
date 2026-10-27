//! Compilation of a provider's resolution plan.
//!
//! [`compile`] obtains the provider's [`ResolutionSpec`], checks it against
//! the protocol and the syntactic schema, merges the schema's reference
//! declarations with the provider's overlay and fixes, per reference, the
//! key it resolves through and the form its occurrences take. Every refusal
//! is a typed [`ResolveError`].

use std::collections::BTreeMap;
use std::sync::Arc;

use panproto_gat::Name;
use panproto_inst::{ResolutionContextKind, ResolutionKind, ResolutionSpec};
use panproto_schema::{
    Edge, InstanceShapeKind, Key, KeyRefError, KeyScope, OffsetUnit, Protocol, RefSource,
    RefTextSource, Reference, ResolveMode, Schema, SpanEnd, ValidationError, VariableBinding,
};

use super::ResolutionProvider;
use super::error::{ResolutionFeature, ResolveError};

/// How the occurrences of one reference are read and resolved, with the
/// source paths of its declaration.
#[derive(Clone, Debug)]
pub enum RefForm {
    /// Key values held by the holder, looked up in the index of `key`.
    KeyValue {
        /// The key the reference resolves through.
        key: Key,
        /// One path per key part.
        parts: Vec<Vec<Edge>>,
    },
    /// A variable bound by the identifier key `key`. Under self-binding
    /// (holder and target are one vertex and the field is the binder key's
    /// path), the first occurrence of a name in its scope in document order
    /// is its definition and later ones refer to it; otherwise every
    /// occurrence looks its name up in the binder key's index.
    Variable {
        /// The binder key.
        key: Key,
        /// The field holding the name.
        field: Vec<Edge>,
        /// Whether the holder binds its own variable.
        self_binding: bool,
        /// Which matching preceding definition binds an occurrence.
        binding: VariableBinding,
    },
    /// A path the provider resolves.
    Path {
        /// The field holding the path.
        field: Vec<Edge>,
    },
    /// A standoff span: looked up in the index of `key` by its declared
    /// offsets, or, without a key, materialized as one span node per
    /// distinct field-qualified text identity and byte interval.
    Standoff {
        /// The target key, when spans resolve to keyed elements.
        key: Option<Key>,
        /// Where the text is.
        text: RefTextSource,
        /// The start offset's path.
        start: Vec<Edge>,
        /// The end offset's path.
        end: Vec<Edge>,
        /// The unit the offsets count.
        unit: OffsetUnit,
        /// Whether the end offset is inclusive.
        end_inclusive: bool,
        /// Whether the second written coordinate is an offset or a length.
        end_form: SpanEnd,
        /// The origin added to written coordinates.
        origin: u64,
    },
}

/// One reference with everything resolution needs to evaluate it.
#[derive(Clone, Debug)]
pub struct CompiledRef {
    /// The merged declaration.
    pub reference: Reference,
    /// Its form.
    pub form: RefForm,
}

/// A compiled plan: the merged references and their compiled forms.
pub struct Compiled {
    /// The canonical protocol name.
    pub protocol_name: String,
    /// The provider's spec.
    pub spec: ResolutionSpec,
    /// Schema and protocol references merged by id.
    pub references: BTreeMap<Name, Reference>,
    /// Each reference's form, by id.
    pub forms: BTreeMap<Name, CompiledRef>,
}

/// Compile `provider`'s plan for `protocol` and the syntactic schema.
///
/// # Errors
///
/// In this order: [`ResolveError::ProviderUnavailable`] when `protocol` is
/// not the provider's protocol; the provider's own spec error;
/// [`ResolveError::ProviderFailure`] with code `spec_provider` or
/// `spec_kind` when the spec names another provider or kind;
/// [`ResolveError::UnsupportedKind`] when the kind's target shape is not
/// the protocol's semantic shape; [`ResolveError::UnsupportedFeature`] with
/// [`ResolutionFeature::Assembly`] when an assembly plan is present for a
/// resolving kind or absent for an assembly kind;
/// [`ResolveError::ReferenceConflict`] for an unequal duplicate reference;
/// [`ResolveError::Schema`] for a malformed declaration (the semantic
/// schema of a resolving kind must have the syntactic edge set, every
/// reference of a resolving kind needs an edge, a structural edge must be
/// a syntactic edge, and a key must have the arity its form reads);
/// [`ResolveError::UnsupportedFeature`] with
/// [`ResolutionFeature::ReferenceWriter`] for a redirecting reference over
/// a table or a reference edge without a writer;
/// [`ResolveError::ReferenceConflict`] for a writer that names no
/// reference with its edge; and [`ResolveError::UnsupportedFeature`] with
/// [`ResolutionFeature::ExternalText`] or
/// [`ResolutionFeature::TokenBoundaries`] when the spec does not require a
/// service a declaration needs.
pub fn compile(
    protocol: &Protocol,
    syntactic: &Schema,
    provider: &Arc<dyn ResolutionProvider>,
) -> Result<Compiled, ResolveError> {
    let protocol_name = panproto_protocols::semantics::canonical_name(&protocol.name);
    if provider.protocol() != protocol {
        return Err(ResolveError::ProviderUnavailable {
            protocol: protocol_name,
        });
    }
    let spec = provider.spec(protocol, syntactic)?;
    check_spec(protocol, provider.as_ref(), &spec)?;
    let references = merge_references(syntactic, &spec)?;
    let mut merged = syntactic.clone();
    merged.declarations.references.clone_from(&references);
    if let Some(error) = panproto_schema::validate(&merged, protocol)
        .into_iter()
        .find_map(|error| {
            if let ValidationError::KeyRef(error) = error {
                Some(error)
            } else {
                None
            }
        })
    {
        return Err(ResolveError::Schema(error));
    }
    let resolving = spec.kind.semantic_shape() == InstanceShapeKind::Graph;
    if resolving {
        check_edge_sets(syntactic, &spec.semantic_schema)?;
        if let Some(edge) = spec
            .structural_edges
            .iter()
            .find(|edge| !syntactic.edges.contains_key(*edge))
        {
            return Err(ResolveError::Schema(KeyRefError::PathDoesNotResolve {
                owner: edge.src.clone(),
                path: vec![edge.clone()],
            }));
        }
    }
    let mut forms = BTreeMap::new();
    for (name, reference) in &references {
        if resolving && reference.edge.is_none() {
            return Err(ResolveError::Schema(KeyRefError::EdgeRequired {
                reference: name.clone(),
            }));
        }
        if spec.kind == ResolutionKind::TableToGraph && reference.mode == ResolveMode::Redirect {
            return Err(ResolveError::UnsupportedFeature {
                provider: Some(spec.provider),
                feature: ResolutionFeature::ReferenceWriter,
            });
        }
        forms.insert(
            name.clone(),
            compile_reference(syntactic, reference, Some(&spec.provider))?,
        );
    }
    check_writers(&spec, &references, resolving)?;
    check_context(&spec, &references)?;
    Ok(Compiled {
        protocol_name,
        spec,
        references,
        forms,
    })
}

/// Refuse a spec of another provider or kind, a kind whose target is not
/// the protocol's semantic shape, and a misplaced assembly plan.
fn check_spec(
    protocol: &Protocol,
    provider: &dyn ResolutionProvider,
    spec: &ResolutionSpec,
) -> Result<(), ResolveError> {
    let id = provider.id();
    if spec.provider != id {
        return Err(ResolveError::ProviderFailure {
            provider: id,
            code: "spec_provider".into(),
            at: None,
        });
    }
    if spec.kind != provider.kind() {
        return Err(ResolveError::ProviderFailure {
            provider: id,
            code: "spec_kind".into(),
            at: None,
        });
    }
    if spec.kind.semantic_shape() != protocol.semantic_shape {
        return Err(ResolveError::UnsupportedKind {
            syntactic: spec.kind.syntactic_shape(),
            semantic: protocol.semantic_shape,
        });
    }
    let assembly_kind = matches!(
        spec.kind,
        ResolutionKind::TreeToTable | ResolutionKind::TableToTree
    );
    if assembly_kind == spec.assembly.is_some() {
        Ok(())
    } else {
        Err(ResolveError::UnsupportedFeature {
            provider: Some(id),
            feature: ResolutionFeature::Assembly(spec.kind),
        })
    }
}

/// The schema's references merged with the provider's overlay by id.
fn merge_references(
    syntactic: &Schema,
    spec: &ResolutionSpec,
) -> Result<BTreeMap<Name, Reference>, ResolveError> {
    let mut references = syntactic.declarations.references.clone();
    for (name, reference) in &spec.protocol_references {
        match references.get(name) {
            Some(existing) if existing != reference => {
                return Err(ResolveError::ReferenceConflict {
                    reference: name.clone(),
                });
            }
            Some(_) => {}
            None => {
                references.insert(name.clone(), reference.clone());
            }
        }
    }
    Ok(references)
}

/// Refuse a writer naming no reference with its edge, and, for a resolving
/// kind, a reference edge without a writer.
fn check_writers(
    spec: &ResolutionSpec,
    references: &BTreeMap<Name, Reference>,
    resolving: bool,
) -> Result<(), ResolveError> {
    for (edge, writer) in &spec.reference_writers {
        let writes = references
            .get(writer)
            .is_some_and(|reference| reference.edge.as_ref() == Some(edge));
        if !writes {
            return Err(ResolveError::ReferenceConflict {
                reference: writer.clone(),
            });
        }
    }
    let unwritten = references.values().any(|reference| {
        reference
            .edge
            .as_ref()
            .is_some_and(|edge| !spec.reference_writers.contains_key(edge))
    });
    if resolving && unwritten {
        return Err(ResolveError::UnsupportedFeature {
            provider: Some(spec.provider.clone()),
            feature: ResolutionFeature::ReferenceWriter,
        });
    }
    Ok(())
}

/// Refuse a spec that does not require a service some declaration needs.
fn check_context(
    spec: &ResolutionSpec,
    references: &BTreeMap<Name, Reference>,
) -> Result<(), ResolveError> {
    for (needed, feature) in [
        (
            ResolutionContextKind::ExternalText,
            ResolutionFeature::ExternalText,
        ),
        (
            ResolutionContextKind::TokenBoundaries,
            ResolutionFeature::TokenBoundaries,
        ),
    ] {
        let declared = references
            .values()
            .any(|reference| needs(reference, needed));
        if declared && !spec.context_requirements.contains(&needed) {
            return Err(ResolveError::UnsupportedFeature {
                provider: Some(spec.provider.clone()),
                feature,
            });
        }
    }
    Ok(())
}

/// Whether a reference needs a context service.
const fn needs(reference: &Reference, service: ResolutionContextKind) -> bool {
    matches!(
        (&reference.source, service),
        (
            RefSource::Standoff {
                text: RefTextSource::External(_),
                ..
            },
            ResolutionContextKind::ExternalText,
        ) | (
            RefSource::Standoff {
                unit: OffsetUnit::Token,
                ..
            },
            ResolutionContextKind::TokenBoundaries,
        )
    )
}

/// Refuse a semantic schema whose edge set differs from the syntactic one,
/// naming the least edge in their symmetric difference.
fn check_edge_sets(syntactic: &Schema, semantic: &Schema) -> Result<(), ResolveError> {
    let differing = syntactic
        .edges
        .keys()
        .filter(|edge| !semantic.edges.contains_key(*edge))
        .chain(
            semantic
                .edges
                .keys()
                .filter(|edge| !syntactic.edges.contains_key(*edge)),
        )
        .min();
    differing.map_or(Ok(()), |edge| {
        Err(ResolveError::Schema(KeyRefError::PathDoesNotResolve {
            owner: edge.src.clone(),
            path: vec![edge.clone()],
        }))
    })
}

/// Fix one reference's key and form.
fn compile_reference(
    schema: &Schema,
    reference: &Reference,
    provider: Option<&panproto_inst::ResolutionProviderId>,
) -> Result<CompiledRef, ResolveError> {
    let declarations = &schema.declarations;
    check_supported_reference(schema, reference, provider)?;
    let arity = |expected: usize, found: usize| {
        ResolveError::Schema(KeyRefError::ArityMismatch {
            reference: reference.id.clone(),
            expected,
            found,
        })
    };
    let named = |id: &Name| {
        declarations
            .key(&reference.target_vertex, id)
            .cloned()
            .ok_or_else(|| {
                ResolveError::Schema(KeyRefError::UnknownTargetKey {
                    reference: reference.id.clone(),
                    key: id.clone(),
                })
            })
    };
    let form = match &reference.source {
        RefSource::KeyValue { parts } => {
            let key = match declarations.resolving_key(reference) {
                Some(key) => key.clone(),
                None if parts.is_empty() => empty_key(),
                None => return Err(arity(0, parts.len())),
            };
            RefForm::KeyValue {
                key,
                parts: parts.clone(),
            }
        }
        RefSource::Variable {
            field,
            binder_key,
            binding,
        } => {
            let key = named(binder_key)?;
            if key.parts.len() != 1 {
                return Err(arity(key.parts.len(), 1));
            }
            let self_binding = reference.holder == reference.target_vertex
                && matches!(&key.parts[0], panproto_schema::KeyPart::Path(path) if path == field);
            RefForm::Variable {
                key,
                field: field.clone(),
                self_binding,
                binding: *binding,
            }
        }
        RefSource::Path { field, .. } => RefForm::Path {
            field: field.clone(),
        },
        RefSource::Standoff {
            text,
            start,
            end,
            unit,
            end_inclusive,
            end_form,
            origin,
            ..
        } => {
            let key = match &reference.target_key {
                None => None,
                Some(id) => {
                    let key = named(id)?;
                    if key.parts.len() != 2 {
                        return Err(arity(key.parts.len(), 2));
                    }
                    Some(key)
                }
            };
            RefForm::Standoff {
                key,
                text: text.clone(),
                start: start.clone(),
                end: end.clone(),
                unit: unit.clone(),
                end_inclusive: *end_inclusive,
                end_form: *end_form,
                origin: *origin,
            }
        }
        RefSource::Inline { .. } => unreachable!("inline references are refused above"),
    };
    Ok(CompiledRef {
        reference: reference.clone(),
        form,
    })
}

fn check_supported_reference(
    schema: &Schema,
    reference: &Reference,
    provider: Option<&panproto_inst::ResolutionProviderId>,
) -> Result<(), ResolveError> {
    let declarations = &schema.declarations;
    let unsupported = |feature| ResolveError::UnsupportedFeature {
        provider: provider.cloned(),
        feature,
    };
    if reference.null_vertex.is_some()
        || reference.scope_via.is_some()
        || declarations.target_set(&reference.target_vertex).is_some()
        || matches!(reference.source, RefSource::Inline { .. })
    {
        return Err(unsupported(ResolutionFeature::ResolutionState));
    }
    match &reference.source {
        RefSource::Standoff {
            text, fragments, ..
        } => {
            if fragments.is_some() {
                return Err(unsupported(ResolutionFeature::ResolutionState));
            }
            match text {
                RefTextSource::Referenced { .. } | RefTextSource::Span { .. } => {
                    return Err(unsupported(ResolutionFeature::ResolutionState));
                }
                RefTextSource::ExternalUri { .. } => {
                    return Err(unsupported(ResolutionFeature::ExternalText));
                }
                RefTextSource::Local(_) | RefTextSource::External(_) => {}
            }
        }
        RefSource::KeyValue { .. }
        | RefSource::Path { .. }
        | RefSource::Variable { .. }
        | RefSource::Inline { .. } => {}
    }
    Ok(())
}

/// The key of a reference whose holder names its target by no parts: every
/// element of the target vertex has the empty key value.
fn empty_key() -> Key {
    Key {
        id: Name::from(""),
        parts: Vec::new(),
        role: panproto_schema::KeyRole::Unique,
        scope: KeyScope::Document,
        equality: vec![],
    }
}

#[cfg(test)]
mod tests {
    use panproto_schema::{
        Cardinality, KeyPart, KeyRole, OnUnresolved, PathSyntax, SchemaDeclarations,
    };

    use super::*;

    fn path(name: &str) -> Edge {
        Edge {
            src: "holder".into(),
            tgt: "scalar".into(),
            kind: "property".into(),
            name: Some(name.into()),
        }
    }

    fn reference(
        holder: &str,
        target: &str,
        source: RefSource,
        target_key: Option<&str>,
    ) -> Reference {
        Reference {
            id: "reference".into(),
            holder: holder.into(),
            source,
            target_vertex: target.into(),
            target_key: target_key.map(Into::into),
            edge: None,
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: Default::default(),
            null_vertex: None,
            scope_via: None,
        }
    }

    fn key(id: &str, parts: Vec<KeyPart>) -> Key {
        Key {
            id: id.into(),
            parts,
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: vec![],
        }
    }

    fn schema_with_key(vertex: &str, key: Key) -> Schema {
        let mut declarations = SchemaDeclarations::default();
        declarations.insert_key(vertex.into(), key);
        Schema {
            declarations,
            ..Schema::default()
        }
    }

    #[test]
    fn only_an_empty_key_value_source_may_omit_its_target_key() -> Result<(), ResolveError> {
        let schema = Schema::default();
        let empty = reference(
            "holder",
            "target",
            RefSource::KeyValue { parts: Vec::new() },
            None,
        );
        let compiled = compile_reference(&schema, &empty, None)?;
        assert!(matches!(
            compiled.form,
            RefForm::KeyValue { key, parts }
                if key.parts.is_empty()
                    && key.role == KeyRole::Unique
                    && key.scope == KeyScope::Document
                    && parts.is_empty()
        ));

        let nonempty = reference(
            "holder",
            "target",
            RefSource::KeyValue {
                parts: vec![vec![path("reference")]],
            },
            None,
        );
        assert!(matches!(
            compile_reference(&schema, &nonempty, None),
            Err(ResolveError::Schema(KeyRefError::ArityMismatch {
                expected: 0,
                found: 1,
                ..
            }))
        ));
        Ok(())
    }

    #[test]
    fn variable_self_binding_needs_both_the_same_vertex_and_the_binder_path()
    -> Result<(), ResolveError> {
        let binder = path("binder");
        let other = path("other");
        let schema = schema_with_key(
            "target",
            key("binder", vec![KeyPart::Path(vec![binder.clone()])]),
        );
        for (holder, field, expected) in [
            ("target", vec![binder.clone()], true),
            ("target", vec![other], false),
            ("another_holder", vec![binder], false),
        ] {
            let variable = reference(
                holder,
                "target",
                RefSource::Variable {
                    field,
                    binder_key: "binder".into(),
                    binding: Default::default(),
                },
                None,
            );
            let compiled = compile_reference(&schema, &variable, None)?;
            assert!(matches!(
                compiled.form,
                RefForm::Variable { self_binding, .. } if self_binding == expected
            ));
        }
        Ok(())
    }

    #[test]
    fn a_keyed_standoff_reference_needs_exactly_two_key_parts() -> Result<(), ResolveError> {
        let schema = schema_with_key(
            "span",
            key(
                "interval",
                vec![
                    KeyPart::Path(vec![path("start")]),
                    KeyPart::Path(vec![path("end")]),
                ],
            ),
        );
        let standoff = reference(
            "holder",
            "span",
            RefSource::Standoff {
                text: RefTextSource::Local(vec![path("text")]),
                start: vec![path("start")],
                end: vec![path("end")],
                unit: OffsetUnit::Byte,
                end_inclusive: false,
                end_form: Default::default(),
                origin: 0,
                fragments: None,
            },
            Some("interval"),
        );
        let compiled = compile_reference(&schema, &standoff, None)?;
        assert!(matches!(
            compiled.form,
            RefForm::Standoff { key: Some(key), .. } if key.parts.len() == 2
        ));

        let path_reference = reference(
            "holder",
            "target",
            RefSource::Path {
                field: vec![path("path")],
                syntax: PathSyntax::JsonPointer,
            },
            None,
        );
        assert!(matches!(
            compile_reference(&Schema::default(), &path_reference, None)?.form,
            RefForm::Path { .. }
        ));
        Ok(())
    }
}
