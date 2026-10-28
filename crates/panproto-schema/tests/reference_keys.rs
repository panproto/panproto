//! Exact O17 declaration-carrier differential for schema canonical bytes.

#![allow(clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_reference::enumerate::keys_and_registration as enumerate;
use panproto_reference::{keys as reference_keys, schema as reference};
use panproto_schema::{
    Cardinality, Edge, Key, KeyEquality, KeyPart, KeyRefError, KeyRole, KeyScope, NullMatch,
    NumericPrimitive, OffsetUnit, OnUnresolved, PathSyntax, Protocol, RefGuard, RefSource,
    RefTextSource, Reference, ResolveMode, Schema, ScopeVia, SpanEnd, SqlCollation, SqlPad,
    TargetSet, UriBase, UriJoin, ValidationError, VariableBinding, Vertex, canonical_bytes,
};

fn edge(edge: &panproto_reference::model::SEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn key_part(part: &reference::KeyPart) -> KeyPart {
    match part {
        reference::KeyPart::Path(path) => KeyPart::Path(path.iter().map(edge).collect()),
        reference::KeyPart::Position { within } => KeyPart::Position {
            within: edge(within),
        },
    }
}

const fn numeric_primitive(primitive: reference_keys::NumericPrimitive) -> NumericPrimitive {
    match primitive {
        reference_keys::NumericPrimitive::Decimal => NumericPrimitive::Decimal,
        reference_keys::NumericPrimitive::Float(width) => NumericPrimitive::Float(width),
        reference_keys::NumericPrimitive::Any => NumericPrimitive::Any,
    }
}

fn key_equality(equality: &reference_keys::KeyEquality) -> KeyEquality {
    match equality {
        reference_keys::KeyEquality::Representation => KeyEquality::Representation,
        reference_keys::KeyEquality::NumericValue => KeyEquality::NumericValue,
        reference_keys::KeyEquality::NumericValueWithinKind(primitive) => {
            KeyEquality::NumericValueWithinKind(numeric_primitive(*primitive))
        }
        reference_keys::KeyEquality::Sql { collation, pad } => KeyEquality::Sql {
            collation: match collation {
                reference_keys::SqlCollation::Binary => SqlCollation::Binary,
                reference_keys::SqlCollation::AsciiCaseInsensitive => {
                    SqlCollation::AsciiCaseInsensitive
                }
                reference_keys::SqlCollation::Named(name) => {
                    SqlCollation::Named(name.as_str().into())
                }
            },
            pad: match pad {
                reference_keys::SqlPad::NoPad => SqlPad::NoPad,
                reference_keys::SqlPad::PadSpace => SqlPad::PadSpace,
            },
        },
        reference_keys::KeyEquality::XmlNonCdata => KeyEquality::XmlNonCdata,
    }
}

const fn key_role(role: reference::KeyRole) -> KeyRole {
    match role {
        reference::KeyRole::Primary => KeyRole::Primary,
        reference::KeyRole::Unique => KeyRole::Unique,
        reference::KeyRole::Identifier => KeyRole::Identifier,
        reference::KeyRole::SortKey => KeyRole::SortKey,
        reference::KeyRole::Index => KeyRole::Index,
    }
}

fn key_scope(scope: &reference::KeyScope) -> KeyScope {
    match scope {
        reference::KeyScope::Document => KeyScope::Document,
        reference::KeyScope::Global => KeyScope::Global,
        reference::KeyScope::Within(vertex) => KeyScope::Within(vertex.as_str().into()),
    }
}

fn key(key: &reference::Key) -> Key {
    Key {
        id: key.id.as_str().into(),
        parts: key.parts.iter().map(key_part).collect(),
        role: key_role(key.role),
        scope: key_scope(&key.scope),
        equality: key.equality.iter().map(key_equality).collect(),
    }
}

const fn uri_join(join: reference::UriJoin) -> UriJoin {
    match join {
        reference::UriJoin::Rfc3986 => UriJoin::Rfc3986,
        reference::UriJoin::FhirServiceBase => UriJoin::FhirServiceBase,
    }
}

fn uri_base(base: &reference::UriBase) -> UriBase {
    match base {
        reference::UriBase::Absolute => UriBase::Absolute,
        reference::UriBase::Field(path) => UriBase::Field(path.iter().map(edge).collect()),
        reference::UriBase::Positional { base, prefix } => UriBase::Positional {
            base: base.as_deref().map(Into::into),
            prefix: prefix.as_deref().map(Into::into),
        },
        reference::UriBase::Ancestor {
            ancestor,
            field,
            join,
        } => UriBase::Ancestor {
            ancestor: ancestor.as_str().into(),
            field: field.iter().map(edge).collect(),
            join: uri_join(*join),
        },
        reference::UriBase::Inherited { field } => UriBase::Inherited {
            field: field.iter().map(edge).collect(),
        },
    }
}

fn path_syntax(syntax: &reference::PathSyntax) -> PathSyntax {
    match syntax {
        reference::PathSyntax::JsonPointer => PathSyntax::JsonPointer,
        reference::PathSyntax::XPath => PathSyntax::XPath,
        reference::PathSyntax::Uri { base } => PathSyntax::Uri {
            base: uri_base(base),
        },
        reference::PathSyntax::IdRef => PathSyntax::IdRef,
    }
}

fn text_source(source: &reference::RefTextSource) -> RefTextSource {
    match source {
        reference::RefTextSource::Local(path) => {
            RefTextSource::Local(path.iter().map(edge).collect())
        }
        reference::RefTextSource::External(path) => {
            RefTextSource::External(path.iter().map(edge).collect())
        }
        reference::RefTextSource::Referenced {
            reference,
            text,
            external,
        } => RefTextSource::Referenced {
            reference: reference.as_str().into(),
            text: text.iter().map(edge).collect(),
            external: external
                .as_ref()
                .map(|path| path.iter().map(edge).collect()),
        },
        reference::RefTextSource::Span { reference } => RefTextSource::Span {
            reference: reference.as_str().into(),
        },
        reference::RefTextSource::ExternalUri { field, base } => RefTextSource::ExternalUri {
            field: field.iter().map(edge).collect(),
            base: uri_base(base),
        },
    }
}

fn reference_value(value: &panproto_reference::value::Value) -> panproto_expr::Value {
    match value {
        panproto_reference::value::Value::Bool(value) => panproto_expr::Value::Bool(*value),
        panproto_reference::value::Value::Int(value) => panproto_expr::Value::Int(*value),
        panproto_reference::value::Value::Float(bits) => {
            panproto_expr::Value::Float(f64::from_bits(*bits))
        }
        panproto_reference::value::Value::Str(value) => panproto_expr::Value::Str(value.clone()),
        panproto_reference::value::Value::Bytes(value) => {
            panproto_expr::Value::Bytes(value.clone())
        }
        panproto_reference::value::Value::CidLink(value) => {
            panproto_expr::Value::CidLink(value.clone())
        }
        panproto_reference::value::Value::Blob { ref_, mime, size } => panproto_expr::Value::Blob {
            ref_: ref_.clone(),
            mime: mime.clone(),
            size: *size,
        },
        panproto_reference::value::Value::Token(value) => {
            panproto_expr::Value::Token(value.clone())
        }
        panproto_reference::value::Value::Null => panproto_expr::Value::Null,
        panproto_reference::value::Value::Opaque { .. }
        | panproto_reference::value::Value::Unknown(_)
        | panproto_reference::value::Value::List(_)
        | panproto_reference::value::Value::LabeledNull(_)
        | panproto_reference::value::Value::Decimal(_)
        | panproto_reference::value::Value::BigInt(_)
        | panproto_reference::value::Value::Temporal(_) => {
            panic!("value outside the bounded declaration carrier: {value:?}")
        }
    }
}

fn reference_source(source: &reference::RefSource) -> RefSource {
    match source {
        reference::RefSource::KeyValue { parts } => RefSource::KeyValue {
            parts: parts
                .iter()
                .map(|path| path.iter().map(edge).collect())
                .collect(),
        },
        reference::RefSource::Path { field, syntax } => RefSource::Path {
            field: field.iter().map(edge).collect(),
            syntax: path_syntax(syntax),
        },
        reference::RefSource::Variable {
            field,
            binder_key,
            binding,
        } => RefSource::Variable {
            field: field.iter().map(edge).collect(),
            binder_key: binder_key.as_str().into(),
            binding: match binding {
                reference::VariableBinding::FirstDefines => VariableBinding::FirstDefines,
                reference::VariableBinding::NearestPreceding => VariableBinding::NearestPreceding,
            },
        },
        reference::RefSource::Standoff {
            text,
            start,
            end,
            unit,
            end_inclusive,
            end_form,
            origin,
            fragments,
        } => RefSource::Standoff {
            text: text_source(text),
            start: start.iter().map(edge).collect(),
            end: end.iter().map(edge).collect(),
            unit: match unit {
                reference::OffsetUnit::Byte => OffsetUnit::Byte,
                reference::OffsetUnit::Utf8Char => OffsetUnit::Utf8Char,
                reference::OffsetUnit::Utf16 => OffsetUnit::Utf16,
                reference::OffsetUnit::Token => OffsetUnit::Token,
            },
            end_inclusive: *end_inclusive,
            end_form: match end_form {
                reference::SpanEnd::Offset => SpanEnd::Offset,
                reference::SpanEnd::Length => SpanEnd::Length,
            },
            origin: *origin,
            fragments: fragments.as_ref().map(edge),
        },
        reference::RefSource::Inline { field } => RefSource::Inline {
            field: field.iter().map(edge).collect(),
        },
    }
}

fn reference(reference: &reference::Reference) -> Reference {
    Reference {
        id: reference.id.as_str().into(),
        holder: reference.holder.as_str().into(),
        source: reference_source(&reference.source),
        target_vertex: reference.target_vertex.as_str().into(),
        target_key: reference.target_key.as_deref().map(Into::into),
        edge: reference.edge.as_ref().map(edge),
        cardinality: match reference.cardinality {
            reference::Cardinality::One => Cardinality::One,
            reference::Cardinality::Many => Cardinality::Many,
        },
        nullable: reference.nullable,
        on_unresolved: match reference.on_unresolved {
            reference::OnUnresolved::Error => OnUnresolved::Error,
            reference::OnUnresolved::Complement => OnUnresolved::Complement,
        },
        mode: match reference.mode {
            reference::ResolveMode::AddEdge => ResolveMode::AddEdge,
            reference::ResolveMode::Redirect => ResolveMode::Redirect,
        },
        guard: reference.guard.as_ref().map(|guard| RefGuard {
            field: guard.field.iter().map(edge).collect(),
            equals: reference_value(&guard.equals),
        }),
        null_match: match reference.null_match {
            reference::NullMatch::Simple => NullMatch::Simple,
            reference::NullMatch::Full => NullMatch::Full,
        },
        null_vertex: reference.null_vertex.as_deref().map(Into::into),
        scope_via: reference.scope_via.as_ref().map(|via| ScopeVia {
            ancestor: via.ancestor.as_str().into(),
            hops: via.hops.iter().map(|hop| hop.as_str().into()).collect(),
        }),
    }
}

fn production_schema(schema: &reference::Schema) -> Schema {
    let vertices: HashMap<_, _> = schema
        .vertices
        .iter()
        .map(|(id, kind)| {
            let id: Name = id.as_str().into();
            (
                id.clone(),
                Vertex {
                    id,
                    kind: kind.as_str().into(),
                    nsid: None,
                },
            )
        })
        .collect();
    let edges: HashMap<_, _> = schema
        .edges
        .iter()
        .map(|reference_edge| {
            let production_edge = edge(reference_edge);
            (production_edge, reference_edge.kind.as_str().into())
        })
        .collect();
    let mut production = Schema {
        protocol: "o17-keys".into(),
        vertices,
        edges,
        entries: schema
            .entries
            .iter()
            .map(|entry| entry.as_str().into())
            .collect(),
        orderings: schema
            .orderings
            .iter()
            .map(|(reference_edge, position)| (edge(reference_edge), *position))
            .collect(),
        ..Schema::default()
    };
    production.declarations.keys = schema
        .keys
        .iter()
        .map(|(vertex, keys)| {
            (
                vertex.as_str().into(),
                keys.iter().map(key).collect::<Vec<_>>(),
            )
        })
        .collect();
    production.declarations.references = schema
        .references
        .iter()
        .map(|(id, declaration)| (id.as_str().into(), reference(declaration)))
        .collect();
    production.declarations.target_sets = schema
        .target_sets
        .iter()
        .map(|(id, set)| {
            (
                id.as_str().into(),
                TargetSet {
                    members: set
                        .members
                        .iter()
                        .map(|(vertex, key)| (vertex.as_str().into(), key.as_str().into()))
                        .collect(),
                    scope: key_scope(&set.scope),
                },
            )
        })
        .collect();
    production
}

fn paired_bytes(schema: &reference::Schema) -> (Vec<u8>, Vec<u8>) {
    (
        reference::canonical_bytes(schema),
        canonical_bytes(&production_schema(schema)),
    )
}

fn paired_declaration_bytes(schema: &reference::Schema) -> (Vec<u8>, Vec<u8>) {
    const EMPTY_DECLARATIONS: [u8; 16] = [0; 16];

    let mut without_declarations = schema.clone();
    without_declarations.keys.clear();
    without_declarations.references.clear();
    without_declarations.target_sets.clear();

    let (reference_bytes, production_bytes) = paired_bytes(schema);
    let (reference_base, production_base) = paired_bytes(&without_declarations);
    assert!(reference_base.ends_with(&EMPTY_DECLARATIONS));
    assert!(production_base.ends_with(&EMPTY_DECLARATIONS));

    let reference_offset = reference_base.len() - EMPTY_DECLARATIONS.len();
    let production_offset = production_base.len() - EMPTY_DECLARATIONS.len();
    assert_eq!(
        &reference_bytes[..reference_offset],
        &reference_base[..reference_offset]
    );
    assert_eq!(
        &production_bytes[..production_offset],
        &production_base[..production_offset]
    );
    (
        reference_bytes[reference_offset..].to_vec(),
        production_bytes[production_offset..].to_vec(),
    )
}

fn contextual_schemas(
    graph: &reference::Schema,
    slot: usize,
    transition: &enumerate::AxisTransition,
    other: Option<&enumerate::KeyDescriptor>,
) -> (reference::Schema, reference::Schema) {
    let other_slot = 1 - slot;
    let mut before = [None, None];
    let mut after = [None, None];
    before[slot] = transition.before.as_ref();
    after[slot] = transition.after.as_ref();
    before[other_slot] = other;
    after[other_slot] = other;
    (
        enumerate::schema_with_keys(graph.clone(), before),
        enumerate::schema_with_keys(graph.clone(), after),
    )
}

fn reference_edge(src: &str, tgt: &str, name: &str) -> panproto_reference::model::SEdge {
    panproto_reference::model::SEdge::new(src, tgt, "prop", Some(name))
}

fn canonical_base() -> reference::Schema {
    let field = reference_edge("holder", "scalar", "field");
    let target = reference_edge("holder", "target", "resolved");
    let mut schema = reference::Schema::default()
        .vertex("holder", "object")
        .vertex("target", "object")
        .vertex("scalar", "scalar")
        .edge("holder", "scalar", "prop", Some("field"))
        .edge("holder", "target", "prop", Some("resolved"));
    schema.keys.insert(
        "target".to_owned(),
        vec![reference::Key {
            id: "id".to_owned(),
            parts: vec![reference::KeyPart::Path(vec![])],
            role: reference::KeyRole::Primary,
            scope: reference::KeyScope::Document,
            equality: vec![],
        }],
    );
    schema.references.insert(
        "r".to_owned(),
        reference::Reference {
            id: "r".to_owned(),
            holder: "holder".to_owned(),
            source: reference::RefSource::KeyValue {
                parts: vec![vec![field]],
            },
            target_vertex: "target".to_owned(),
            target_key: Some("id".to_owned()),
            edge: Some(target),
            cardinality: reference::Cardinality::One,
            nullable: false,
            on_unresolved: reference::OnUnresolved::Error,
            mode: reference::ResolveMode::AddEdge,
            guard: None,
            null_match: reference::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    schema
}

fn equality_fixture(equality: Vec<reference_keys::KeyEquality>) -> reference::Schema {
    let mut schema = reference::Schema::default().vertex("target", "object");
    schema.keys.insert(
        "target".to_owned(),
        vec![reference::Key {
            id: "id".to_owned(),
            parts: (0..equality.len().max(1))
                .map(|_| reference::KeyPart::Path(vec![]))
                .collect(),
            role: reference::KeyRole::Primary,
            scope: reference::KeyScope::Document,
            equality,
        }],
    );
    schema
}

fn with_source(source: reference::RefSource) -> reference::Schema {
    let mut schema = canonical_base();
    schema
        .references
        .get_mut("r")
        .expect("reference fixture")
        .source = source;
    schema
}

#[test]
#[allow(clippy::too_many_lines)]
fn extended_declaration_bytes_match_reference_on_every_sum_variant() {
    use reference_keys::{KeyEquality as Eq, NumericPrimitive as Num, SqlCollation, SqlPad};

    let field = || reference_edge("holder", "scalar", "field");
    let fragment = || reference_edge("holder", "scalar", "fragment");
    let equalities = vec![
        vec![],
        vec![Eq::Representation],
        vec![Eq::NumericValue],
        vec![Eq::NumericValueWithinKind(Num::Decimal)],
        vec![Eq::NumericValueWithinKind(Num::Float(32))],
        vec![Eq::NumericValueWithinKind(Num::Any)],
        vec![Eq::Sql {
            collation: SqlCollation::Binary,
            pad: SqlPad::NoPad,
        }],
        vec![Eq::Sql {
            collation: SqlCollation::AsciiCaseInsensitive,
            pad: SqlPad::PadSpace,
        }],
        vec![Eq::Sql {
            collation: SqlCollation::Named("named".to_owned()),
            pad: SqlPad::NoPad,
        }],
        vec![Eq::XmlNonCdata],
    ];
    let mut fixtures: Vec<_> = equalities.into_iter().map(equality_fixture).collect();

    let mut position_key = canonical_base();
    let carrier = reference_edge("holder", "target", "resolved");
    position_key.orderings.insert(carrier.clone(), 0);
    position_key.keys.get_mut("target").expect("target keys")[0].parts =
        vec![reference::KeyPart::Position { within: carrier }];
    fixtures.push(position_key);

    let path_sources = [
        reference::PathSyntax::JsonPointer,
        reference::PathSyntax::XPath,
        reference::PathSyntax::IdRef,
        reference::PathSyntax::Uri {
            base: reference::UriBase::Absolute,
        },
        reference::PathSyntax::Uri {
            base: reference::UriBase::Field(vec![field()]),
        },
        reference::PathSyntax::Uri {
            base: reference::UriBase::Positional {
                base: Some("holder".to_owned()),
                prefix: Some("target".to_owned()),
            },
        },
        reference::PathSyntax::Uri {
            base: reference::UriBase::Ancestor {
                ancestor: "holder".to_owned(),
                field: vec![field()],
                join: reference::UriJoin::Rfc3986,
            },
        },
        reference::PathSyntax::Uri {
            base: reference::UriBase::Ancestor {
                ancestor: "holder".to_owned(),
                field: vec![field()],
                join: reference::UriJoin::FhirServiceBase,
            },
        },
        reference::PathSyntax::Uri {
            base: reference::UriBase::Inherited {
                field: vec![field()],
            },
        },
    ];
    fixtures.extend(path_sources.into_iter().map(|syntax| {
        with_source(reference::RefSource::Path {
            field: vec![field()],
            syntax,
        })
    }));
    fixtures.extend(
        [
            reference::VariableBinding::FirstDefines,
            reference::VariableBinding::NearestPreceding,
        ]
        .into_iter()
        .map(|binding| {
            with_source(reference::RefSource::Variable {
                field: vec![field()],
                binder_key: "id".to_owned(),
                binding,
            })
        }),
    );

    let text_sources = [
        reference::RefTextSource::Local(vec![field()]),
        reference::RefTextSource::External(vec![field()]),
        reference::RefTextSource::Referenced {
            reference: "r".to_owned(),
            text: vec![field()],
            external: Some(vec![field()]),
        },
        reference::RefTextSource::Span {
            reference: "r".to_owned(),
        },
        reference::RefTextSource::ExternalUri {
            field: vec![field()],
            base: reference::UriBase::Inherited {
                field: vec![field()],
            },
        },
    ];
    fixtures.extend(text_sources.into_iter().enumerate().map(|(index, text)| {
        with_source(reference::RefSource::Standoff {
            text,
            start: vec![field()],
            end: vec![field()],
            unit: [
                reference::OffsetUnit::Byte,
                reference::OffsetUnit::Utf8Char,
                reference::OffsetUnit::Utf16,
                reference::OffsetUnit::Token,
            ][index.min(3)],
            end_inclusive: index == 1,
            end_form: if index == 2 {
                reference::SpanEnd::Length
            } else {
                reference::SpanEnd::Offset
            },
            origin: u64::try_from(index).unwrap_or_default(),
            fragments: (index == 4).then(fragment),
        })
    }));
    fixtures.push(with_source(reference::RefSource::Inline {
        field: vec![field()],
    }));

    let mut target_set = canonical_base();
    target_set.target_sets.insert(
        "targets".to_owned(),
        reference::TargetSet {
            members: vec![("target".to_owned(), "id".to_owned())],
            scope: reference::KeyScope::Document,
        },
    );
    fixtures.push(target_set);

    for axis in 0..8 {
        let mut schema = canonical_base();
        let declaration = schema.references.get_mut("r").expect("reference fixture");
        match axis {
            0 => declaration.cardinality = reference::Cardinality::Many,
            1 => declaration.nullable = true,
            2 => declaration.on_unresolved = reference::OnUnresolved::Complement,
            3 => declaration.mode = reference::ResolveMode::Redirect,
            4 => {
                declaration.guard = Some(reference::RefGuard {
                    field: vec![field()],
                    equals: panproto_reference::value::Value::Str("guard".to_owned()),
                });
            }
            5 => declaration.null_match = reference::NullMatch::Full,
            6 => declaration.null_vertex = Some("scalar".to_owned()),
            7 => {
                declaration.scope_via = Some(reference::ScopeVia {
                    ancestor: "holder".to_owned(),
                    hops: vec!["r".to_owned()],
                });
            }
            _ => unreachable!(),
        }
        fixtures.push(schema);
    }

    assert_eq!(fixtures.len(), 37);
    let mut reference_encodings = BTreeSet::new();
    let mut production_encodings = BTreeSet::new();
    for schema in &fixtures {
        let (reference_bytes, production_bytes) = paired_declaration_bytes(schema);
        assert_eq!(production_bytes, reference_bytes);
        assert!(reference_encodings.insert(reference_bytes));
        assert!(production_encodings.insert(production_bytes));
    }
    assert_eq!(reference_encodings.len(), fixtures.len());
    assert_eq!(production_encodings.len(), fixtures.len());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeclarationClass {
    EqualityArity,
    DuplicateKeyId,
    TargetSetNameClash,
    EmptyTargetSet,
    TargetSetMemberMismatch,
    SortKeyNotOrdered,
    KeyNotResolvable,
    UnknownTarget,
    ReferenceArity,
    InlineRequiresRedirect,
    InlineRequiresTargetSet,
    LengthInclusive,
    InvalidFragments,
    DuplicateGuard,
    EdgeMismatch,
    TargetSetHasKey,
    VariableBinderInvalid,
    BinderModeConflict,
    TextReferenceInvalid,
    ScopeViaInvalid,
}

fn reference_classes(schema: &reference::Schema) -> Vec<DeclarationClass> {
    reference::validate_declarations(schema)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|error| match error {
            reference::DeclarationError::EqualityArity { .. } => DeclarationClass::EqualityArity,
            reference::DeclarationError::DuplicateKeyId { .. } => DeclarationClass::DuplicateKeyId,
            reference::DeclarationError::TargetSetNameClash { .. } => {
                DeclarationClass::TargetSetNameClash
            }
            reference::DeclarationError::EmptyTargetSet { .. } => DeclarationClass::EmptyTargetSet,
            reference::DeclarationError::TargetSetMemberMismatch { .. } => {
                DeclarationClass::TargetSetMemberMismatch
            }
            reference::DeclarationError::SortKeyNotOrdered { .. } => {
                DeclarationClass::SortKeyNotOrdered
            }
            reference::DeclarationError::KeyNotResolvable { .. } => {
                DeclarationClass::KeyNotResolvable
            }
            reference::DeclarationError::UnknownTarget { .. } => DeclarationClass::UnknownTarget,
            reference::DeclarationError::ReferenceArity { .. } => DeclarationClass::ReferenceArity,
            reference::DeclarationError::InlineRequiresRedirect { .. } => {
                DeclarationClass::InlineRequiresRedirect
            }
            reference::DeclarationError::InlineRequiresTargetSet { .. } => {
                DeclarationClass::InlineRequiresTargetSet
            }
            reference::DeclarationError::LengthInclusive { .. } => {
                DeclarationClass::LengthInclusive
            }
            reference::DeclarationError::InvalidFragments { .. } => {
                DeclarationClass::InvalidFragments
            }
            reference::DeclarationError::DuplicateGuard { .. } => DeclarationClass::DuplicateGuard,
            reference::DeclarationError::EdgeMismatch { .. } => DeclarationClass::EdgeMismatch,
            reference::DeclarationError::TargetSetHasKey { .. } => {
                DeclarationClass::TargetSetHasKey
            }
            reference::DeclarationError::VariableBinderInvalid { .. } => {
                DeclarationClass::VariableBinderInvalid
            }
            reference::DeclarationError::BinderModeConflict { .. } => {
                DeclarationClass::BinderModeConflict
            }
            reference::DeclarationError::TextReferenceInvalid { .. } => {
                DeclarationClass::TextReferenceInvalid
            }
            reference::DeclarationError::ScopeViaInvalid { .. } => {
                DeclarationClass::ScopeViaInvalid
            }
        })
        .collect()
}

fn production_classes(schema: &reference::Schema) -> (Vec<DeclarationClass>, Vec<KeyRefError>) {
    let mut common = Vec::new();
    let mut structural = Vec::new();
    for finding in panproto_schema::validate(&production_schema(schema), &Protocol::default()) {
        let ValidationError::KeyRef(error) = finding else {
            continue;
        };
        let class = match error {
            KeyRefError::EqualityArity { .. } => Some(DeclarationClass::EqualityArity),
            KeyRefError::DuplicateKeyId { .. } => Some(DeclarationClass::DuplicateKeyId),
            KeyRefError::TargetSetNameClash { .. } => Some(DeclarationClass::TargetSetNameClash),
            KeyRefError::EmptyTargetSet { .. } => Some(DeclarationClass::EmptyTargetSet),
            KeyRefError::TargetSetMemberMismatch { .. } => {
                Some(DeclarationClass::TargetSetMemberMismatch)
            }
            KeyRefError::SortKeyNotOrdered { .. } => Some(DeclarationClass::SortKeyNotOrdered),
            KeyRefError::KeyNotResolvable { .. } => Some(DeclarationClass::KeyNotResolvable),
            KeyRefError::UnknownTarget { .. } | KeyRefError::UnknownTargetKey { .. } => {
                Some(DeclarationClass::UnknownTarget)
            }
            KeyRefError::ArityMismatch { .. } => Some(DeclarationClass::ReferenceArity),
            KeyRefError::InlineRequiresRedirect { .. } => {
                Some(DeclarationClass::InlineRequiresRedirect)
            }
            KeyRefError::InlineRequiresTargetSet { .. } => {
                Some(DeclarationClass::InlineRequiresTargetSet)
            }
            KeyRefError::LengthInclusive { .. } => Some(DeclarationClass::LengthInclusive),
            KeyRefError::InvalidFragments { .. } => Some(DeclarationClass::InvalidFragments),
            KeyRefError::DuplicateGuard { .. } => Some(DeclarationClass::DuplicateGuard),
            KeyRefError::EdgeMismatch { .. } => Some(DeclarationClass::EdgeMismatch),
            KeyRefError::TargetSetHasKey { .. } => Some(DeclarationClass::TargetSetHasKey),
            KeyRefError::BinderNotIdentifier { .. } => {
                Some(DeclarationClass::VariableBinderInvalid)
            }
            KeyRefError::BinderModeConflict { .. } => Some(DeclarationClass::BinderModeConflict),
            KeyRefError::TextReferenceInvalid { .. } => {
                Some(DeclarationClass::TextReferenceInvalid)
            }
            KeyRefError::ScopeViaInvalid { .. } => Some(DeclarationClass::ScopeViaInvalid),
            error @ (KeyRefError::UnknownVertex { .. }
            | KeyRefError::PathDoesNotResolve { .. }
            | KeyRefError::PositionNotOrdered { .. }
            | KeyRefError::EdgeRequired { .. }
            | KeyRefError::WithinNotAncestor { .. }) => {
                structural.push(error);
                None
            }
        };
        if let Some(class) = class {
            common.push(class);
        }
    }
    (common, structural)
}

fn target_set_schema() -> reference::Schema {
    let mut schema = canonical_base();
    schema.target_sets.insert(
        "targets".to_owned(),
        reference::TargetSet {
            members: vec![("target".to_owned(), "id".to_owned())],
            scope: reference::KeyScope::Document,
        },
    );
    let declaration = schema.references.get_mut("r").expect("reference fixture");
    "targets".clone_into(&mut declaration.target_vertex);
    declaration.target_key = None;
    declaration.edge = None;
    schema
}

fn target_set_predicate_base() -> reference::Schema {
    let mut schema = reference::Schema::default()
        .vertex("a", "object")
        .vertex("b", "object")
        .vertex("c", "object")
        .vertex("scope", "object")
        .edge("scope", "c", "prop", Some("children"));
    let key = |id: &str,
               parts: usize,
               equality: Vec<reference_keys::KeyEquality>,
               scope: reference::KeyScope| reference::Key {
        id: id.to_owned(),
        parts: vec![reference::KeyPart::Path(Vec::new()); parts],
        role: reference::KeyRole::Identifier,
        scope,
        equality,
    };
    schema.keys.insert(
        "a".to_owned(),
        vec![
            key(
                "id",
                1,
                vec![reference_keys::KeyEquality::NumericValue],
                reference::KeyScope::Document,
            ),
            key("implicit", 1, Vec::new(), reference::KeyScope::Document),
        ],
    );
    schema.keys.insert(
        "b".to_owned(),
        vec![
            key(
                "id",
                1,
                vec![reference_keys::KeyEquality::NumericValue],
                reference::KeyScope::Document,
            ),
            key(
                "two",
                2,
                vec![reference_keys::KeyEquality::NumericValue; 2],
                reference::KeyScope::Document,
            ),
            key(
                "representation",
                1,
                vec![reference_keys::KeyEquality::Representation],
                reference::KeyScope::Document,
            ),
            key(
                "explicit",
                1,
                vec![reference_keys::KeyEquality::Representation],
                reference::KeyScope::Document,
            ),
        ],
    );
    schema.keys.insert(
        "c".to_owned(),
        vec![key(
            "scoped",
            1,
            vec![reference_keys::KeyEquality::NumericValue],
            reference::KeyScope::Within("scope".to_owned()),
        )],
    );
    schema
}

type ReferenceTargetSignature = (String, String, usize, Vec<reference_keys::KeyEquality>);

fn reference_target_signatures(
    schema: &reference::Schema,
    members: &[(String, String)],
) -> Option<Vec<ReferenceTargetSignature>> {
    members
        .iter()
        .map(|(vertex, key_id)| {
            let key = schema
                .keys
                .get(vertex)?
                .iter()
                .find(|key| key.id == *key_id)?;
            let equality = if key.equality.is_empty() {
                vec![reference_keys::KeyEquality::Representation; key.parts.len()]
            } else {
                key.equality.clone()
            };
            Some((vertex.clone(), key_id.clone(), key.parts.len(), equality))
        })
        .collect()
}

#[test]
fn target_sets_match_on_the_complete_local_predicate() {
    let cases = [
        (
            "compatible",
            vec![
                ("a".to_owned(), "id".to_owned()),
                ("b".to_owned(), "id".to_owned()),
            ],
            Some(true),
            Vec::new(),
        ),
        (
            "empty",
            Vec::new(),
            Some(false),
            vec![DeclarationClass::EmptyTargetSet],
        ),
        (
            "duplicate",
            vec![
                ("a".to_owned(), "id".to_owned()),
                ("a".to_owned(), "id".to_owned()),
            ],
            Some(false),
            vec![DeclarationClass::TargetSetMemberMismatch],
        ),
        (
            "arity mismatch",
            vec![
                ("a".to_owned(), "id".to_owned()),
                ("b".to_owned(), "two".to_owned()),
            ],
            Some(false),
            vec![DeclarationClass::TargetSetMemberMismatch],
        ),
        (
            "equality mismatch",
            vec![
                ("a".to_owned(), "id".to_owned()),
                ("b".to_owned(), "representation".to_owned()),
            ],
            Some(false),
            vec![DeclarationClass::TargetSetMemberMismatch],
        ),
        (
            "implicit and explicit representation",
            vec![
                ("a".to_owned(), "implicit".to_owned()),
                ("b".to_owned(), "explicit".to_owned()),
            ],
            Some(true),
            Vec::new(),
        ),
        (
            "absent key",
            vec![("missing".to_owned(), "id".to_owned())],
            None,
            vec![DeclarationClass::TargetSetMemberMismatch],
        ),
        (
            "scope mismatch",
            vec![
                ("a".to_owned(), "id".to_owned()),
                ("c".to_owned(), "scoped".to_owned()),
            ],
            Some(true),
            vec![DeclarationClass::TargetSetMemberMismatch],
        ),
        (
            "invalid first member does not anchor the signature",
            vec![
                ("c".to_owned(), "scoped".to_owned()),
                ("b".to_owned(), "two".to_owned()),
            ],
            Some(false),
            vec![DeclarationClass::TargetSetMemberMismatch],
        ),
    ];

    for (name, members, local_expected, declaration_expected) in cases {
        let mut schema = target_set_predicate_base();
        let signatures = reference_target_signatures(&schema, &members);
        assert_eq!(
            signatures.as_deref().map(reference_keys::target_set),
            local_expected,
            "independent local predicate: {name}"
        );
        schema.target_sets.insert(
            "targets".to_owned(),
            reference::TargetSet {
                members,
                scope: reference::KeyScope::Document,
            },
        );
        assert_eq!(
            reference_classes(&schema),
            declaration_expected,
            "independent declaration admission: {name}"
        );
        let (production, structural) = production_classes(&schema);
        assert!(
            structural.is_empty(),
            "production structural spill: {name}: {structural:?}"
        );
        assert_eq!(
            production, declaration_expected,
            "production declaration admission: {name}"
        );
    }
}

#[test]
fn target_set_references_check_every_member_key_role() {
    let cases = [
        (reference::KeyRole::Primary, Vec::new()),
        (reference::KeyRole::Unique, Vec::new()),
        (reference::KeyRole::Identifier, Vec::new()),
        (
            reference::KeyRole::SortKey,
            vec![
                DeclarationClass::SortKeyNotOrdered,
                DeclarationClass::KeyNotResolvable,
            ],
        ),
        (
            reference::KeyRole::Index,
            vec![DeclarationClass::KeyNotResolvable],
        ),
    ];
    for (role, expected) in cases {
        let mut schema = target_set_schema();
        schema.keys.get_mut("target").expect("target keys")[0].role = role;
        assert_eq!(reference_classes(&schema), expected, "reference: {role:?}");
        let (production, structural) = production_classes(&schema);
        assert!(
            structural.is_empty(),
            "production structural spill for {role:?}: {structural:?}"
        );
        assert_eq!(production, expected, "production: {role:?}");
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn declaration_validation_matches_reference_on_every_common_refusal() {
    let mut cases = vec![("valid", canonical_base(), vec![])];

    let mut implicit_primary = canonical_base();
    let mut later_primary = implicit_primary.keys["target"][0].clone();
    later_primary.id = "later-primary".to_owned();
    later_primary
        .parts
        .push(reference::KeyPart::Path(Vec::new()));
    implicit_primary
        .keys
        .get_mut("target")
        .expect("target keys")
        .push(later_primary);
    implicit_primary
        .references
        .get_mut("r")
        .expect("reference fixture")
        .target_key = None;
    cases.push((
        "implicit target selects the first primary key",
        implicit_primary,
        vec![],
    ));

    let mut duplicate = canonical_base();
    let key = duplicate.keys["target"][0].clone();
    duplicate
        .keys
        .get_mut("target")
        .expect("target keys")
        .push(key);
    cases.push((
        "duplicate key",
        duplicate,
        vec![DeclarationClass::DuplicateKeyId],
    ));

    let mut triple_duplicate = canonical_base();
    let key = triple_duplicate.keys["target"][0].clone();
    triple_duplicate
        .keys
        .get_mut("target")
        .expect("target keys")
        .extend([key.clone(), key]);
    cases.push((
        "three declarations sharing one key id",
        triple_duplicate,
        vec![DeclarationClass::DuplicateKeyId],
    ));

    let mut equality = canonical_base();
    equality.keys.get_mut("target").expect("target keys")[0].equality =
        vec![reference_keys::KeyEquality::Representation; 2];
    cases.push((
        "equality arity",
        equality,
        vec![DeclarationClass::EqualityArity],
    ));

    let mut clash = canonical_base();
    clash.references.clear();
    clash.target_sets.insert(
        "target".to_owned(),
        reference::TargetSet {
            members: vec![("target".to_owned(), "id".to_owned())],
            scope: reference::KeyScope::Document,
        },
    );
    cases.push((
        "target-set name clash",
        clash,
        vec![DeclarationClass::TargetSetNameClash],
    ));

    let mut empty = canonical_base();
    empty.references.clear();
    empty.target_sets.insert(
        "targets".to_owned(),
        reference::TargetSet {
            members: vec![],
            scope: reference::KeyScope::Document,
        },
    );
    cases.push((
        "empty target set",
        empty,
        vec![DeclarationClass::EmptyTargetSet],
    ));

    let mut member = canonical_base();
    member.references.clear();
    member.target_sets.insert(
        "targets".to_owned(),
        reference::TargetSet {
            members: vec![("target".to_owned(), "missing".to_owned())],
            scope: reference::KeyScope::Document,
        },
    );
    cases.push((
        "target-set member mismatch",
        member,
        vec![DeclarationClass::TargetSetMemberMismatch],
    ));

    let mut unresolved_set = target_set_schema();
    unresolved_set
        .target_sets
        .get_mut("targets")
        .expect("target set")
        .members = vec![("target".to_owned(), "missing".to_owned())];
    cases.push((
        "target set with only a missing member",
        unresolved_set,
        vec![
            DeclarationClass::TargetSetMemberMismatch,
            DeclarationClass::UnknownTarget,
        ],
    ));

    let mut sort = canonical_base();
    sort.references.clear();
    sort.keys.get_mut("target").expect("target keys")[0].role = reference::KeyRole::SortKey;
    cases.push((
        "sort key not ordered",
        sort,
        vec![DeclarationClass::SortKeyNotOrdered],
    ));

    let mut non_resolvable = canonical_base();
    non_resolvable.keys.get_mut("target").expect("target keys")[0].role = reference::KeyRole::Index;
    cases.push((
        "key not resolvable",
        non_resolvable,
        vec![DeclarationClass::KeyNotResolvable],
    ));

    let mut non_resolvable_member = target_set_schema();
    non_resolvable_member
        .vertices
        .insert("indexed".to_owned(), "object".to_owned());
    non_resolvable_member.keys.insert(
        "indexed".to_owned(),
        vec![reference::Key {
            id: "lookup".to_owned(),
            parts: vec![reference::KeyPart::Path(Vec::new())],
            role: reference::KeyRole::Index,
            scope: reference::KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    non_resolvable_member
        .target_sets
        .get_mut("targets")
        .expect("target set")
        .members
        .push(("indexed".to_owned(), "lookup".to_owned()));
    cases.push((
        "non-resolvable target-set member",
        non_resolvable_member,
        vec![DeclarationClass::KeyNotResolvable],
    ));

    let mut unknown = canonical_base();
    unknown
        .references
        .get_mut("r")
        .expect("reference fixture")
        .target_key = Some("missing".to_owned());
    cases.push((
        "unknown target",
        unknown,
        vec![DeclarationClass::UnknownTarget],
    ));

    let mut arity = canonical_base();
    arity
        .references
        .get_mut("r")
        .expect("reference fixture")
        .source = reference::RefSource::KeyValue { parts: vec![] };
    cases.push((
        "reference arity",
        arity,
        vec![DeclarationClass::ReferenceArity],
    ));

    let mut inline_redirect = target_set_schema();
    inline_redirect
        .references
        .get_mut("r")
        .expect("reference fixture")
        .source = reference::RefSource::Inline {
        field: vec![reference_edge("holder", "scalar", "field")],
    };
    cases.push((
        "inline redirect",
        inline_redirect,
        vec![DeclarationClass::InlineRequiresRedirect],
    ));

    let mut inline_set = canonical_base();
    let declaration = inline_set
        .references
        .get_mut("r")
        .expect("reference fixture");
    declaration.source = reference::RefSource::Inline {
        field: vec![reference_edge("holder", "scalar", "field")],
    };
    declaration.mode = reference::ResolveMode::Redirect;
    cases.push((
        "inline target set",
        inline_set,
        vec![DeclarationClass::InlineRequiresTargetSet],
    ));

    let standoff = |end_form, end_inclusive, fragments| reference::RefSource::Standoff {
        text: reference::RefTextSource::Local(vec![reference_edge("holder", "scalar", "field")]),
        start: vec![reference_edge("holder", "scalar", "field")],
        end: vec![reference_edge("holder", "scalar", "field")],
        unit: reference::OffsetUnit::Utf8Char,
        end_inclusive,
        end_form,
        origin: 0,
        fragments,
    };
    let mut length = canonical_base();
    length
        .references
        .get_mut("r")
        .expect("reference fixture")
        .source = standoff(reference::SpanEnd::Length, true, None);
    cases.push((
        "inclusive length",
        length,
        vec![DeclarationClass::LengthInclusive],
    ));

    let mut fragments = canonical_base();
    fragments
        .references
        .get_mut("r")
        .expect("reference fixture")
        .source = standoff(
        reference::SpanEnd::Offset,
        false,
        Some(reference_edge("holder", "scalar", "field")),
    );
    cases.push((
        "invalid fragments",
        fragments,
        vec![DeclarationClass::InvalidFragments],
    ));

    let mut guards = canonical_base();
    let mut second = guards.references["r"].clone();
    second.id = "r2".to_owned();
    for declaration in guards
        .references
        .values_mut()
        .chain(std::iter::once(&mut second))
    {
        declaration.guard = Some(reference::RefGuard {
            field: vec![reference_edge("holder", "scalar", "field")],
            equals: panproto_reference::value::Value::Str("same".to_owned()),
        });
    }
    guards.references.insert(second.id.clone(), second);
    cases.push((
        "duplicate guard",
        guards,
        vec![DeclarationClass::DuplicateGuard],
    ));

    let mut edge_mismatch = canonical_base();
    edge_mismatch
        .references
        .get_mut("r")
        .expect("reference fixture")
        .edge = Some(reference_edge("holder", "scalar", "field"));
    cases.push((
        "edge mismatch",
        edge_mismatch,
        vec![DeclarationClass::EdgeMismatch],
    ));

    let mut wrong_holder = target_set_schema();
    wrong_holder
        .vertices
        .insert("other-holder".to_owned(), "object".to_owned());
    wrong_holder
        .edges
        .insert(reference_edge("other-holder", "target", "resolved"));
    wrong_holder
        .references
        .get_mut("r")
        .expect("reference fixture")
        .edge = Some(reference_edge("other-holder", "targets", "resolved"));
    cases.push((
        "target-set virtual edge has wrong holder",
        wrong_holder,
        vec![DeclarationClass::EdgeMismatch],
    ));

    let mut redundant = target_set_schema();
    redundant
        .references
        .get_mut("r")
        .expect("reference fixture")
        .target_key = Some("id".to_owned());
    cases.push((
        "target set carries key",
        redundant,
        vec![DeclarationClass::TargetSetHasKey],
    ));

    let mut binder = canonical_base();
    binder
        .references
        .get_mut("r")
        .expect("reference fixture")
        .source = reference::RefSource::Variable {
        field: vec![reference_edge("holder", "scalar", "field")],
        binder_key: "id".to_owned(),
        binding: reference::VariableBinding::FirstDefines,
    };
    cases.push((
        "variable binder",
        binder,
        vec![DeclarationClass::VariableBinderInvalid],
    ));

    let mut binder_modes = canonical_base();
    binder_modes.keys.get_mut("target").expect("target keys")[0].role =
        reference::KeyRole::Identifier;
    let first = binder_modes
        .references
        .get_mut("r")
        .expect("reference fixture");
    first.source = reference::RefSource::Variable {
        field: vec![reference_edge("holder", "scalar", "field")],
        binder_key: "id".to_owned(),
        binding: reference::VariableBinding::FirstDefines,
    };
    let mut nearest = first.clone();
    nearest.id = "r2".to_owned();
    nearest.source = reference::RefSource::Variable {
        field: vec![reference_edge("holder", "scalar", "field")],
        binder_key: "id".to_owned(),
        binding: reference::VariableBinding::NearestPreceding,
    };
    binder_modes.references.insert(nearest.id.clone(), nearest);
    cases.push((
        "binder mode conflict",
        binder_modes,
        vec![DeclarationClass::BinderModeConflict],
    ));

    let mut text = canonical_base();
    text.references
        .get_mut("r")
        .expect("reference fixture")
        .source = reference::RefSource::Standoff {
        text: reference::RefTextSource::Span {
            reference: "missing".to_owned(),
        },
        start: vec![reference_edge("holder", "scalar", "field")],
        end: vec![reference_edge("holder", "scalar", "field")],
        unit: reference::OffsetUnit::Utf8Char,
        end_inclusive: false,
        end_form: reference::SpanEnd::Offset,
        origin: 0,
        fragments: None,
    };
    cases.push((
        "text reference",
        text,
        vec![DeclarationClass::TextReferenceInvalid],
    ));

    let mut reachable_cycle = canonical_base();
    let template = reachable_cycle
        .references
        .remove("r")
        .expect("reference fixture");
    let field = reference_edge("holder", "scalar", "field");
    for (id, next) in [("a", "b"), ("b", "a"), ("c", "a")] {
        let mut declaration = template.clone();
        declaration.id = id.to_owned();
        declaration.source = reference::RefSource::Standoff {
            text: reference::RefTextSource::Span {
                reference: next.to_owned(),
            },
            start: vec![field.clone()],
            end: vec![field.clone()],
            unit: reference::OffsetUnit::Utf8Char,
            end_inclusive: false,
            end_form: reference::SpanEnd::Offset,
            origin: 0,
            fragments: None,
        };
        reachable_cycle
            .references
            .insert(id.to_owned(), declaration);
    }
    cases.push((
        "text reference reaches a repeated-node cycle",
        reachable_cycle,
        vec![
            DeclarationClass::TextReferenceInvalid,
            DeclarationClass::TextReferenceInvalid,
            DeclarationClass::TextReferenceInvalid,
        ],
    ));

    let mut scope = canonical_base();
    scope
        .references
        .get_mut("r")
        .expect("reference fixture")
        .scope_via = Some(reference::ScopeVia {
        ancestor: "holder".to_owned(),
        hops: vec![],
    });
    cases.push(("scope via", scope, vec![DeclarationClass::ScopeViaInvalid]));

    assert_eq!(cases.len(), 27);
    for (name, schema, expected) in cases {
        assert_eq!(reference_classes(&schema), expected, "reference: {name}");
        let (production, structural) = production_classes(&schema);
        assert!(
            structural.is_empty(),
            "production structural spill: {name}: {structural:?}"
        );
        assert_eq!(production, expected, "production: {name}");
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn canonical_declaration_factorization_reconciles_the_exact_carrier() {
    assert_eq!(enumerate::schema_count(), 27_329_908);
    assert_eq!(enumerate::classification_count(), 816_223_680);

    let mut structural_reference = BTreeSet::new();
    let mut structural_production = BTreeSet::new();
    for graph in enumerate::schemas() {
        let (reference_bytes, production_bytes) = paired_bytes(&graph);
        assert!(structural_reference.insert(reference_bytes));
        assert!(structural_production.insert(production_bytes));
    }
    assert_eq!(structural_reference.len(), 148);
    assert_eq!(structural_production.len(), 148);

    for vertices in 1..=3 {
        let graphs = enumerate::structural_schemas(vertices);
        let descriptors = enumerate::key_descriptors(vertices);
        let transitions = enumerate::axis_transitions(vertices);
        let graph = &graphs[0];

        let mut reference_singletons = BTreeSet::new();
        let mut production_singletons = BTreeSet::new();
        for descriptor in &descriptors {
            for slot in 0..2 {
                let mut slots = [None, None];
                slots[slot] = Some(descriptor);
                let schema = enumerate::schema_with_keys(graph.clone(), slots);
                let (reference_bytes, production_bytes) = paired_bytes(&schema);
                assert!(reference_singletons.insert(reference_bytes));
                assert!(production_singletons.insert(production_bytes));
            }
        }
        assert_eq!(reference_singletons.len(), descriptors.len() * 2);
        assert_eq!(production_singletons.len(), descriptors.len() * 2);

        for transition in &transitions {
            for slot in 0..2 {
                let (before, after) = contextual_schemas(graph, slot, transition, None);
                let (before_reference, before_production) = paired_bytes(&before);
                let (after_reference, after_production) = paired_bytes(&after);
                assert_ne!(before_reference, after_reference);
                assert_ne!(before_production, after_production);
            }
        }
    }

    let classes = enumerate::factor_classes();
    let mut represented = 0_u64;
    for class in &classes {
        let graphs = enumerate::structural_schemas(class.vertices);
        let transitions = enumerate::axis_transitions(class.vertices);
        let descriptors = enumerate::key_descriptors(class.vertices);
        let graph = &graphs[class.representative_graph];
        let transition = &transitions[class.representative_transition];
        let other = class.representative_other.map(|index| &descriptors[index]);
        let (before, after) = contextual_schemas(graph, class.slot, transition, other);
        let (before_reference, before_production) = paired_bytes(&before);
        let (after_reference, after_production) = paired_bytes(&after);
        assert_ne!(before_reference, after_reference);
        assert_ne!(before_production, after_production);
        represented += class.multiplicity;
    }
    assert_eq!(represented, 816_223_680);

    println!(
        "O17 keys certificate: 148 structural graphs; descriptors [90, 240, 450]; 27,329,908 schemas; 816,223,680 directed classifications"
    );
}
