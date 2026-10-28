//! Same-input production/reference differentials for declared-key kernels.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io;

use panproto_expr::exact::{BigIntValue, DecimalValue, WideInt};
use panproto_inst::key as production;
use panproto_inst::validation::{
    CheckKind, ExternalVerdicts, IncompleteReason, Requirement, ValidationBudget,
    ValidationOptions, ValidationPlan, Violation,
};
use panproto_inst::{
    EdgeId, FieldPresence, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, Node, NodeShape,
    Value, ViewRef,
};
use panproto_reference::enumerate::keys_and_registration as enumerate;
use panproto_reference::exact::Decimal as ReferenceDecimal;
use panproto_reference::model::{
    GEdge as ReferenceEdge, Graph as ReferenceGraph, Node as ReferenceNode,
    SEdge as ReferenceSchemaEdge,
};
use panproto_reference::value::{Presence as ReferencePresence, Value as ReferenceValue};
use panproto_reference::{keys as reference_keys, schema as reference};
use panproto_schema::{
    Edge, Key, KeyEquality, KeyPart, KeyRole, KeyScope, NumericPrimitive, Protocol, Schema,
    SqlCollation, SqlPad, Vertex,
};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn production_value(value: &ReferenceValue) -> Result<Value, io::Error> {
    Ok(match value {
        ReferenceValue::Int(value) => Value::Int(*value),
        ReferenceValue::BigInt(value) => Value::BigInt(BigIntValue {
            negative: value.negative,
            significand: value.significand.to_string(),
            zeros: WideInt::from_sign_and_digits(false, &value.zeros.to_string())
                .ok_or_else(|| invalid("reference big-integer exponent is not canonical"))?,
        }),
        ReferenceValue::Float(bits) => Value::Float(f64::from_bits(*bits)),
        ReferenceValue::Decimal(value) => Value::Decimal(match value {
            ReferenceDecimal::Finite(value) => DecimalValue::new(
                value.negative,
                value.digits.to_string(),
                WideInt::parse_canonical(&value.exponent.to_string())
                    .ok_or_else(|| invalid("reference decimal exponent is not canonical"))?,
            )
            .map_err(|error| invalid(error.to_string()))?,
            ReferenceDecimal::Infinite { negative } => DecimalValue::infinity(*negative),
            ReferenceDecimal::NaN {
                negative,
                signalling,
                payload,
            } => DecimalValue::nan(*negative, *signalling, payload.to_string()),
        }),
        ReferenceValue::Str(value) => Value::Str(value.clone()),
        other => {
            return Err(invalid(format!(
                "value outside the O17 equality carrier: {other:?}"
            )));
        }
    })
}

fn production_equality(equality: &reference_keys::KeyEquality) -> KeyEquality {
    match equality {
        reference_keys::KeyEquality::Representation => KeyEquality::Representation,
        reference_keys::KeyEquality::NumericValue => KeyEquality::NumericValue,
        reference_keys::KeyEquality::NumericValueWithinKind(primitive) => {
            KeyEquality::NumericValueWithinKind(match primitive {
                reference_keys::NumericPrimitive::Decimal => NumericPrimitive::Decimal,
                reference_keys::NumericPrimitive::Float(width) => NumericPrimitive::Float(*width),
                reference_keys::NumericPrimitive::Any => NumericPrimitive::Any,
            })
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

fn production_edge(edge: &ReferenceSchemaEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn production_key(key: &reference::Key) -> Key {
    Key {
        id: key.id.as_str().into(),
        parts: key
            .parts
            .iter()
            .map(|part| match part {
                reference::KeyPart::Path(path) => {
                    KeyPart::Path(path.iter().map(production_edge).collect())
                }
                reference::KeyPart::Position { within } => KeyPart::Position {
                    within: production_edge(within),
                },
            })
            .collect(),
        role: match key.role {
            reference::KeyRole::Primary => KeyRole::Primary,
            reference::KeyRole::Unique => KeyRole::Unique,
            reference::KeyRole::Identifier => KeyRole::Identifier,
            reference::KeyRole::SortKey => KeyRole::SortKey,
            reference::KeyRole::Index => KeyRole::Index,
        },
        scope: match &key.scope {
            reference::KeyScope::Document => KeyScope::Document,
            reference::KeyScope::Global => KeyScope::Global,
            reference::KeyScope::Within(vertex) => KeyScope::Within(vertex.as_str().into()),
        },
        equality: key.equality.iter().map(production_equality).collect(),
    }
}

#[derive(Clone, Default)]
struct GraphPair {
    reference: ReferenceGraph,
    production: GInstance,
}

impl GraphPair {
    fn add_node(
        &mut self,
        id: u32,
        anchor: &str,
        value: Option<ReferencePresence>,
    ) -> Result<(), io::Error> {
        let mut reference = ReferenceNode::new(id, anchor);
        reference.value.clone_from(&value);
        self.reference.nodes.insert(id, reference);

        let mut production = Node::new(id, anchor);
        production.value = value
            .map(|presence| match presence {
                ReferencePresence::Present(value) => {
                    production_value(&value).map(FieldPresence::Present)
                }
                ReferencePresence::Null => Ok(FieldPresence::Null),
                ReferencePresence::Absent => Ok(FieldPresence::Absent),
            })
            .transpose()?;
        self.production = std::mem::take(&mut self.production).with_node(production);
        Ok(())
    }

    fn add_edge(
        &mut self,
        id: u32,
        src: u32,
        tgt: u32,
        label: &ReferenceSchemaEdge,
        position: Option<u32>,
    ) -> Result<(), Box<dyn Error>> {
        self.reference.edges.insert(
            id,
            ReferenceEdge {
                src,
                tgt,
                label: label.clone(),
                position,
            },
        );
        self.production.insert_edge(
            EdgeId(id),
            GEdge::new(src, tgt, production_edge(label)).with_position(position),
        )?;
        Ok(())
    }

    fn add_field(
        &mut self,
        id: u32,
        name: &str,
        value: ReferenceValue,
    ) -> Result<(), Box<dyn Error>> {
        self.reference
            .nodes
            .get_mut(&id)
            .ok_or_else(|| invalid(format!("missing reference node {id}")))?
            .fields
            .insert(name.to_owned(), value.clone());
        self.production
            .node_mut(id)
            .ok_or_else(|| invalid(format!("missing production node {id}")))?
            .extra_fields
            .insert(name.into(), production_value(&value)?);
        Ok(())
    }

    fn mark_fan(&mut self, id: u32) -> Result<(), Box<dyn Error>> {
        self.reference
            .nodes
            .get_mut(&id)
            .ok_or_else(|| invalid(format!("missing reference node {id}")))?
            .shape = panproto_reference::model::NodeShape::HyperEdge;
        self.production
            .node_mut(id)
            .ok_or_else(|| invalid(format!("missing production node {id}")))?
            .shape = NodeShape::HyperEdge;
        Ok(())
    }
}

fn schema_with_key(
    vertices: &[(&str, &str)],
    edges: &[ReferenceSchemaEdge],
    ordered: &[ReferenceSchemaEdge],
    owner: &str,
    key: &reference::Key,
) -> Schema {
    let mut schema = Schema::default();
    for (id, kind) in vertices {
        schema.vertices.insert(
            (*id).into(),
            Vertex {
                id: (*id).into(),
                kind: (*kind).into(),
                nsid: None,
            },
        );
    }
    for reference_edge in edges {
        let edge = production_edge(reference_edge);
        schema.edges.insert(edge.clone(), edge.kind.clone());
        schema
            .outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        schema
            .incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        schema
            .between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge);
    }
    for edge in ordered {
        schema.orderings.insert(production_edge(edge), 0);
    }
    schema
        .declarations
        .insert_key(owner.into(), production_key(key));
    schema
}

fn part_names(key: &reference::Key) -> Vec<String> {
    let label = |edge: &ReferenceSchemaEdge| edge.name.as_deref().unwrap_or(&edge.tgt).to_owned();
    key.parts
        .iter()
        .map(|part| match part {
            reference::KeyPart::Path(path) => path.iter().map(label).collect::<Vec<_>>().join("/"),
            reference::KeyPart::Position { within } => {
                format!("position({})", label(within))
            }
        })
        .collect()
}

fn node_ids(elements: &[ViewRef]) -> Result<Vec<u32>, io::Error> {
    elements
        .iter()
        .map(|element| match element {
            ViewRef::Node(id) => Ok(*id),
            other => Err(invalid(format!(
                "graph key diagnostic named a non-node element: {other:?}"
            ))),
        })
        .collect()
}

/// Compare the independent bucket definition with the public validator.
///
/// Every complete, in-scope tuple in this bounded witness has a twin.  A
/// `DuplicateKey` diagnostic therefore exposes both the tuple read by the
/// production key function and the complete member list of its bucket.
/// NULL tuples and out-of-scope elements are checked through their required
/// absence from those diagnostics and, for the latter, `OutsideKeyScope`.
// Keeping the expected reference index and the observed production index in
// one function makes it harder to accidentally compare different witnesses.
#[allow(clippy::too_many_lines)]
fn assert_validation_key_index(
    pair: &GraphPair,
    schema: &Schema,
    owner: &str,
    key: &reference::Key,
) -> Result<(), Box<dyn Error>> {
    let (reference_buckets, reference_outside) =
        reference_keys::buckets(&pair.reference, owner, key)
            .map_err(|error| invalid(format!("reference bucket refusal: {error:?}")))?;
    let production_declaration = production_key(key);

    let mut expected_duplicates = BTreeMap::new();
    let mut expected_values = BTreeMap::new();
    for (_, (values, members)) in reference_buckets {
        assert!(
            members.len() > 1,
            "the bounded differential must twin every indexed tuple"
        );
        let production_values = values
            .iter()
            .map(production_value)
            .collect::<Result<Vec<_>, _>>()?;
        expected_duplicates.insert(members.clone(), production_values.clone());
        for member in members {
            expected_values.insert(member, production_values.clone());
        }
    }
    let expected_outside = reference_outside.into_iter().collect::<BTreeSet<_>>();

    // This direct call independently specifies each tuple.  The comparison
    // below reconstructs the same map solely from public diagnostics.
    for (id, node) in &pair.reference.nodes {
        if node.anchor != owner {
            continue;
        }
        let direct = reference_keys::key_value(&pair.reference, *id, key)
            .map(|values| {
                values
                    .iter()
                    .map(production_value)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        match (
            direct,
            expected_values.get(id),
            expected_outside.contains(id),
        ) {
            (Some(direct), Some(indexed), false) => {
                assert!(production::tuple_equivalence(
                    &production_declaration,
                    &direct,
                    indexed
                )?);
            }
            (Some(_), None, true) | (None, None, false) => {}
            other => {
                return Err(invalid(format!(
                    "reference key-value/index disagreement for node {id}: {other:?}"
                ))
                .into());
            }
        }
    }

    let plan = ValidationPlan::compile(schema, &Protocol::default(), &ValidationOptions::default());
    let report = plan.validate_graph(
        &pair.production,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    let expected_names = part_names(key);
    let mut actual_duplicates = BTreeMap::new();
    let mut actual_values = BTreeMap::new();
    let mut actual_outside = BTreeSet::new();
    for diagnostic in report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.check.kind == CheckKind::KeyUnique)
    {
        match &diagnostic.violation {
            Violation::DuplicateKey { key, elems } => {
                assert_eq!(diagnostic.requirement, Requirement::Required);
                assert!(diagnostic.subject.is_none());
                let (names, values): (Vec<_>, Vec<_>) = key.iter().cloned().unzip();
                assert_eq!(names, expected_names);
                let members = node_ids(elems)?;
                assert!(
                    actual_duplicates
                        .insert(members.clone(), values.clone())
                        .is_none(),
                    "one diagnostic per key bucket"
                );
                for member in members {
                    assert!(
                        actual_values.insert(member, values.clone()).is_none(),
                        "one key tuple per indexed node"
                    );
                }
            }
            Violation::OutsideKeyScope {
                key: actual_key,
                scope,
            } => {
                assert_eq!(diagnostic.requirement, Requirement::Advisory);
                assert_eq!(actual_key.as_ref(), key.id);
                let reference::KeyScope::Within(expected_scope) = &key.scope else {
                    return Err(invalid("outside-scope diagnostic for an unscoped key").into());
                };
                assert_eq!(scope.as_ref(), expected_scope);
                let subject = diagnostic
                    .subject
                    .as_ref()
                    .ok_or_else(|| invalid("outside-scope diagnostic has no subject"))?;
                let subject_ids = node_ids(std::slice::from_ref(subject))?;
                let [id] = subject_ids.as_slice() else {
                    return Err(invalid("outside-scope diagnostic has the wrong subject").into());
                };
                actual_outside.insert(*id);
            }
            other => {
                return Err(invalid(format!(
                    "unexpected key diagnostic in bounded differential: {other:?}"
                ))
                .into());
            }
        }
    }
    assert!(actual_duplicates.keys().eq(expected_duplicates.keys()));
    for (members, actual) in &actual_duplicates {
        assert!(production::tuple_equivalence(
            &production_declaration,
            actual,
            &expected_duplicates[members]
        )?);
    }
    assert!(actual_values.keys().eq(expected_values.keys()));
    for (member, actual) in &actual_values {
        assert!(production::tuple_equivalence(
            &production_declaration,
            actual,
            &expected_values[member]
        )?);
    }
    assert_eq!(actual_outside, expected_outside);
    Ok(())
}

fn one_part_key(equality: KeyEquality) -> Key {
    Key {
        id: "key".into(),
        parts: vec![KeyPart::Path(Vec::new())],
        role: KeyRole::Unique,
        scope: KeyScope::Document,
        equality: vec![equality],
    }
}

fn path_bucket_witness() -> Result<(GraphPair, Vec<ReferenceSchemaEdge>), Box<dyn Error>> {
    let name = ReferenceSchemaEdge::new("item", "name", "prop", Some("name"));
    let number = ReferenceSchemaEdge::new("item", "number", "prop", Some("number"));
    let mut pair = GraphPair::default();
    for id in [3, 7, 9, 20, 21] {
        pair.add_node(id, "item", None)?;
    }
    let members = [
        (3, "A", ReferenceValue::Int(1)),
        (7, "a", ReferenceValue::Float(1.0_f64.to_bits())),
        (9, "A", ReferenceValue::Int(1)),
    ];
    let mut edge_id = 0_u32;
    for (member, text, numeric) in members {
        let name_node = 100 + member;
        let number_node = 200 + member;
        pair.add_node(
            name_node,
            "name",
            Some(ReferencePresence::Present(ReferenceValue::Str(
                text.to_owned(),
            ))),
        )?;
        pair.add_node(
            number_node,
            "number",
            Some(ReferencePresence::Present(numeric)),
        )?;
        pair.add_edge(edge_id, member, name_node, &name, None)?;
        edge_id += 1;
        pair.add_edge(edge_id, member, number_node, &number, None)?;
        edge_id += 1;
    }
    // Both NULL witnesses are twins, but SQL-style key indexing omits them.
    for member in [20, 21] {
        let name_node = 100 + member;
        let number_node = 200 + member;
        pair.add_node(name_node, "name", Some(ReferencePresence::Null))?;
        pair.add_node(
            number_node,
            "number",
            Some(ReferencePresence::Present(ReferenceValue::Int(1))),
        )?;
        pair.add_edge(edge_id, member, name_node, &name, None)?;
        edge_id += 1;
        pair.add_edge(edge_id, member, number_node, &number, None)?;
        edge_id += 1;
    }
    Ok((pair, vec![name, number]))
}

#[test]
fn validation_reconstructs_path_keys_in_document_and_global_scopes() -> Result<(), Box<dyn Error>> {
    let (pair, edges) = path_bucket_witness()?;
    for scope in [reference::KeyScope::Document, reference::KeyScope::Global] {
        let key = reference::Key {
            id: "path-key".to_owned(),
            // The deliberately nonalphabetic order pins tuple order.
            parts: vec![
                reference::KeyPart::Path(vec![edges[1].clone()]),
                reference::KeyPart::Path(vec![edges[0].clone()]),
            ],
            role: reference::KeyRole::Unique,
            scope,
            equality: vec![
                reference_keys::KeyEquality::NumericValue,
                reference_keys::KeyEquality::Sql {
                    collation: reference_keys::SqlCollation::AsciiCaseInsensitive,
                    pad: reference_keys::SqlPad::NoPad,
                },
            ],
        };
        let schema = schema_with_key(
            &[("item", "object"), ("name", "string"), ("number", "number")],
            &edges,
            &[],
            "item",
            &key,
        );
        assert_validation_key_index(&pair, &schema, "item", &key)?;
    }
    Ok(())
}

#[test]
fn validation_reconstructs_position_buckets() -> Result<(), Box<dyn Error>> {
    let within = ReferenceSchemaEdge::new("list", "item", "prop", Some("items"));
    let mut pair = GraphPair::default();
    for id in [0, 1] {
        pair.add_node(id, "list", None)?;
    }
    for id in [10, 11, 20, 21] {
        pair.add_node(id, "item", None)?;
    }
    // Stored positions order siblings; the key part is their zero-based
    // ordinal, not the stored position itself. Edge ids break equal-position
    // ties and intentionally differ between parents.
    for (edge_id, src, tgt, position) in [
        (40, 0, 10, Some(100)),
        (50, 0, 11, Some(300)),
        (10, 1, 20, Some(100)),
        (30, 1, 21, Some(300)),
    ] {
        pair.add_edge(edge_id, src, tgt, &within, position)?;
    }
    let key = reference::Key {
        id: "position-key".to_owned(),
        parts: vec![reference::KeyPart::Position {
            within: within.clone(),
        }],
        role: reference::KeyRole::Unique,
        scope: reference::KeyScope::Document,
        equality: Vec::new(),
    };
    let schema = schema_with_key(
        &[("list", "object"), ("item", "object")],
        std::slice::from_ref(&within),
        std::slice::from_ref(&within),
        "item",
        &key,
    );
    assert_validation_key_index(&pair, &schema, "item", &key)
}

#[test]
fn validation_identifies_exact_parallel_position_edges() -> Result<(), Box<dyn Error>> {
    let within = ReferenceSchemaEdge::new("list", "item", "prop", Some("items"));
    let mut pair = GraphPair::default();
    for id in [0, 1] {
        pair.add_node(id, "list", None)?;
    }
    for id in [10, 20] {
        pair.add_node(id, "item", None)?;
    }
    // Each item has two parallel incoming edges. Edge-id order selects the
    // later-positioned edge, so target identity alone would return ordinal 0
    // while exact incoming-edge identity returns ordinal 1.
    for (edge_id, src, tgt, position) in [
        (40, 0, 10, Some(300)),
        (50, 0, 10, Some(100)),
        (10, 1, 20, Some(300)),
        (30, 1, 20, Some(100)),
    ] {
        pair.add_edge(edge_id, src, tgt, &within, position)?;
    }
    let key = reference::Key {
        id: "parallel-position-key".to_owned(),
        parts: vec![reference::KeyPart::Position {
            within: within.clone(),
        }],
        role: reference::KeyRole::Unique,
        scope: reference::KeyScope::Document,
        equality: Vec::new(),
    };
    let schema = schema_with_key(
        &[("list", "object"), ("item", "object")],
        std::slice::from_ref(&within),
        std::slice::from_ref(&within),
        "item",
        &key,
    );
    assert_validation_key_index(&pair, &schema, "item", &key)
}

#[test]
fn path_fallback_is_decided_per_last_hop_frontier_node() -> Result<(), Box<dyn Error>> {
    let branch = ReferenceSchemaEdge::new("item", "branch", "prop", Some("branch"));
    let value = ReferenceSchemaEdge::new("branch", "scalar", "prop", Some("value"));
    let mut pair = GraphPair::default();
    for id in [0, 1] {
        pair.add_node(id, "item", None)?;
    }
    for id in [10, 11, 20, 21] {
        pair.add_node(id, "branch", None)?;
    }
    for id in [100, 101] {
        pair.add_node(id, "scalar", None)?;
    }
    pair.add_edge(0, 0, 10, &branch, None)?;
    pair.add_edge(1, 0, 11, &branch, None)?;
    pair.add_edge(2, 1, 20, &branch, None)?;
    pair.add_edge(3, 1, 21, &branch, None)?;
    // The first branch has a matching last edge whose target is NULL. The
    // second branch has no such edge and therefore reads its `value` field.
    pair.add_edge(4, 10, 100, &value, None)?;
    pair.add_edge(5, 20, 101, &value, None)?;
    pair.add_field(11, "value", ReferenceValue::Str("fallback".to_owned()))?;
    pair.add_field(21, "value", ReferenceValue::Str("fallback".to_owned()))?;

    let key = reference::Key {
        id: "branch-fallback".to_owned(),
        parts: vec![reference::KeyPart::Path(vec![
            branch.clone(),
            value.clone(),
        ])],
        role: reference::KeyRole::Unique,
        scope: reference::KeyScope::Document,
        equality: Vec::new(),
    };
    let schema = schema_with_key(
        &[
            ("item", "object"),
            ("branch", "object"),
            ("scalar", "string"),
        ],
        &[branch, value],
        &[],
        "item",
        &key,
    );
    assert_validation_key_index(&pair, &schema, "item", &key)
}

#[test]
fn validation_reconstructs_within_buckets_and_outside_cases() -> Result<(), Box<dyn Error>> {
    let entries = ReferenceSchemaEdge::new("document", "item", "prop", Some("entries"));
    let mut pair = GraphPair::default();
    for id in [0, 1] {
        pair.add_node(id, "document", None)?;
    }
    for id in [10, 11, 20, 21, 30, 31] {
        pair.add_node(
            id,
            "item",
            Some(ReferencePresence::Present(ReferenceValue::Str(
                "same".to_owned(),
            ))),
        )?;
    }
    for (edge_id, src, tgt) in [(0, 0, 10), (1, 0, 11), (2, 1, 20), (3, 1, 21)] {
        pair.add_edge(edge_id, src, tgt, &entries, None)?;
    }
    let key = reference::Key {
        id: "within-key".to_owned(),
        parts: vec![reference::KeyPart::Path(Vec::new())],
        role: reference::KeyRole::Unique,
        scope: reference::KeyScope::Within("document".to_owned()),
        equality: vec![reference_keys::KeyEquality::Representation],
    };
    let schema = schema_with_key(
        &[("document", "object"), ("item", "string")],
        std::slice::from_ref(&entries),
        &[],
        "item",
        &key,
    );
    assert_validation_key_index(&pair, &schema, "item", &key)
}

#[test]
fn within_scope_does_not_traverse_fan_incidence() -> Result<(), Box<dyn Error>> {
    let ordinary = ReferenceSchemaEdge::new("document", "item", "prop", Some("entries"));
    let fan_parent = ReferenceSchemaEdge::new("fan", "document", HYPER_PARENT_KIND, None);
    let fan_child = ReferenceSchemaEdge::new("fan", "item", HYPER_LABEL_KIND, Some("item"));
    let mut pair = GraphPair::default();
    pair.add_node(0, "document", None)?;
    pair.add_node(1, "fan", None)?;
    pair.mark_fan(1)?;
    for id in [10, 11, 12] {
        pair.add_node(
            id,
            "item",
            Some(ReferencePresence::Present(ReferenceValue::Str(
                "same".to_owned(),
            ))),
        )?;
    }
    pair.add_edge(0, 0, 10, &ordinary, None)?;
    pair.add_edge(1, 0, 11, &ordinary, None)?;
    pair.add_edge(2, 1, 0, &fan_parent, None)?;
    pair.add_edge(3, 1, 12, &fan_child, None)?;

    let key = reference::Key {
        id: "ordinary-scope".to_owned(),
        parts: vec![reference::KeyPart::Path(Vec::new())],
        role: reference::KeyRole::Unique,
        scope: reference::KeyScope::Within("document".to_owned()),
        equality: Vec::new(),
    };
    let schema = schema_with_key(
        &[("document", "object"), ("item", "string")],
        std::slice::from_ref(&ordinary),
        &[],
        "item",
        &key,
    );
    assert_validation_key_index(&pair, &schema, "item", &key)
}

#[test]
fn validation_preserves_both_reference_keying_refusals() -> Result<(), Box<dyn Error>> {
    let mut pair = GraphPair::default();
    pair.add_node(
        0,
        "item",
        Some(ReferencePresence::Present(ReferenceValue::Str(
            "text".to_owned(),
        ))),
    )?;

    let named = reference::Key {
        id: "named".to_owned(),
        parts: vec![reference::KeyPart::Path(Vec::new())],
        role: reference::KeyRole::Unique,
        scope: reference::KeyScope::Global,
        equality: vec![reference_keys::KeyEquality::Sql {
            collation: reference_keys::SqlCollation::Named("locale".to_owned()),
            pad: reference_keys::SqlPad::NoPad,
        }],
    };
    assert!(matches!(
        reference_keys::buckets(&pair.reference, "item", &named),
        Err(reference_keys::KeyingError::UnsupportedCollation(name)) if name == "locale"
    ));
    let named_schema = schema_with_key(&[("item", "string")], &[], &[], "item", &named);
    let named_report = ValidationPlan::compile(
        &named_schema,
        &Protocol::default(),
        &ValidationOptions::default(),
    )
    .validate_graph(
        &pair.production,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(named_report.incomplete.iter().any(|incomplete| {
        incomplete.check.kind == CheckKind::KeyUnique
            && incomplete.reason
                == IncompleteReason::UnsupportedCollation {
                    key: "named".into(),
                    collation: "locale".into(),
                }
    }));

    let arity = reference::Key {
        id: "arity".to_owned(),
        parts: vec![
            reference::KeyPart::Path(Vec::new()),
            reference::KeyPart::Path(Vec::new()),
        ],
        role: reference::KeyRole::Unique,
        scope: reference::KeyScope::Global,
        equality: vec![reference_keys::KeyEquality::Representation],
    };
    assert!(matches!(
        reference_keys::buckets(&pair.reference, "item", &arity),
        Err(reference_keys::KeyingError::EqualityArity {
            expected: 2,
            found: 1
        })
    ));
    let arity_schema = schema_with_key(&[("item", "string")], &[], &[], "item", &arity);
    let arity_report = ValidationPlan::compile(
        &arity_schema,
        &Protocol::default(),
        &ValidationOptions::default(),
    )
    .validate_graph(
        &pair.production,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    );
    assert!(arity_report.incomplete.iter().any(|incomplete| {
        incomplete.check.kind == CheckKind::KeyUnique
            && matches!(
                &incomplete.reason,
                IncompleteReason::IllFormedDeclaration { detail }
                    if detail == "key arity on item has 1 equality declarations for 2 parts"
            )
    }));
    Ok(())
}

#[test]
fn equality_carrier_executes_all_pairs_and_triples() -> Result<(), Box<dyn Error>> {
    let reference_values = enumerate::equality_values();
    let production_values = reference_values
        .iter()
        .map(production_value)
        .collect::<Result<Vec<_>, _>>()?;
    let configurations = enumerate::equality_configurations();
    assert_eq!(reference_values.len(), 37);
    assert_eq!(configurations.len(), 12);

    let mut pair_count = 0_usize;
    let mut triple_count = 0_usize;
    for (label, reference_equality) in configurations {
        let production_equality = production_equality(&reference_equality);
        let key = one_part_key(production_equality.clone());
        let production_bytes = production_values
            .iter()
            .map(|value| production::canonical_key_bytes(&key, std::slice::from_ref(value)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut relation = vec![vec![false; production_values.len()]; production_values.len()];
        for (left_index, (reference_left, production_left)) in
            reference_values.iter().zip(&production_values).enumerate()
        {
            for (right_index, (reference_right, production_right)) in
                reference_values.iter().zip(&production_values).enumerate()
            {
                let reference_result = reference_keys::key_equivalence(
                    &reference_equality,
                    reference_left,
                    reference_right,
                )
                .ok_or_else(|| invalid(format!("computable configuration {label} refused")))?;
                let production_result = production::key_equivalence(
                    &production_equality,
                    production_left,
                    production_right,
                )?;
                assert_eq!(
                    production_result, reference_result,
                    "{label}: {left_index}, {right_index}"
                );
                assert_eq!(
                    production_bytes[left_index] == production_bytes[right_index],
                    production_result,
                    "canonical kernel {label}: {left_index}, {right_index}"
                );
                assert_eq!(
                    production::representation_refines(
                        &production_equality,
                        production_left,
                        production_right,
                    )?,
                    reference_keys::representation_refines(
                        &reference_equality,
                        reference_left,
                        reference_right,
                    )
                    .ok_or_else(|| invalid(format!("refinement {label} refused")))?,
                );
                relation[left_index][right_index] = production_result;
                pair_count += 1;
            }
        }
        for left in 0..relation.len() {
            for middle in 0..relation.len() {
                for right in 0..relation.len() {
                    assert!(
                        !(relation[left][middle] && relation[middle][right])
                            || relation[left][right],
                        "transitivity {label}: {left}, {middle}, {right}"
                    );
                    triple_count += 1;
                }
            }
        }
    }
    assert_eq!(pair_count, 16_428);
    assert_eq!(triple_count, 607_836);
    println!("O17 equality carrier: pairs={pair_count}, triples={triple_count}");
    Ok(())
}

#[test]
fn named_collation_has_the_exact_typed_refusal_boundary() -> Result<(), Box<dyn Error>> {
    let reference_equality = reference_keys::KeyEquality::Sql {
        collation: reference_keys::SqlCollation::Named("locale".to_owned()),
        pad: reference_keys::SqlPad::NoPad,
    };
    let production_equality = production_equality(&reference_equality);
    for reference_value in enumerate::equality_values() {
        let production_value = production_value(&reference_value)?;
        let reference_result = reference_keys::key_equivalence(
            &reference_equality,
            &reference_value,
            &reference_value,
        );
        let production_result =
            production::key_equivalence(&production_equality, &production_value, &production_value);
        if matches!(reference_value, ReferenceValue::Str(_)) {
            assert_eq!(reference_result, None);
            assert!(matches!(
                production_result,
                Err(production::KeyingError::UnsupportedCollation(name)) if name == "locale"
            ));
        } else {
            assert_eq!(reference_result, Some(true));
            assert_eq!(production_result, Ok(true));
        }
    }
    Ok(())
}

fn sort_values() -> Result<Vec<(ReferenceValue, Value)>, io::Error> {
    let reference = vec![
        ReferenceValue::Int(0),
        ReferenceValue::Int(1),
        ReferenceValue::Float(1.5_f64.to_bits()),
        ReferenceValue::Str("a".to_owned()),
        ReferenceValue::Str("ab".to_owned()),
        ReferenceValue::Str("b".to_owned()),
        ReferenceValue::Float(f64::NAN.to_bits()),
    ];
    reference
        .into_iter()
        .map(|value| {
            let production = production_value(&value)?;
            Ok((value, production))
        })
        .collect()
}

fn words(alphabet: usize, max_len: usize) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..max_len {
        frontier = frontier
            .iter()
            .flat_map(|prefix| {
                (0..alphabet).map(move |symbol| {
                    let mut word = prefix.clone();
                    word.push(symbol);
                    word
                })
            })
            .collect();
        out.extend(frontier.iter().cloned());
    }
    out
}

#[test]
fn sort_preorder_and_adjacent_descent_match_the_reference() -> Result<(), Box<dyn Error>> {
    let values = sort_values()?;
    for (left_index, (reference_left, production_left)) in values.iter().enumerate() {
        for (right_index, (reference_right, production_right)) in values.iter().enumerate() {
            let expected = reference_keys::sort_order(reference_left, reference_right);
            let actual = production::sort_order(production_left, production_right)?;
            assert_eq!(actual, expected, "sort pair {left_index}, {right_index}");
            if let Some(order) = actual {
                assert_eq!(
                    production::sort_order(production_right, production_left)?,
                    Some(order.reverse())
                );
            }
        }
    }
    for (left_index, (_, left)) in values.iter().enumerate() {
        for (middle_index, (_, middle)) in values.iter().enumerate() {
            for (right_index, (_, right)) in values.iter().enumerate() {
                let (Some(left_middle), Some(middle_right)) = (
                    production::sort_order(left, middle)?,
                    production::sort_order(middle, right)?,
                ) else {
                    continue;
                };
                let left_right = production::sort_order(left, right)?.ok_or_else(|| {
                    invalid(format!(
                        "comparable chain became incomparable: {left_index}, {middle_index}, {right_index}"
                    ))
                })?;
                if !left_middle.is_gt() && !middle_right.is_gt() {
                    assert!(!left_right.is_gt());
                }
            }
        }
    }

    let sequences = words(values.len(), 4);
    assert_eq!(sequences.len(), 2_801);
    for sequence in sequences {
        let reference_sequence = sequence
            .iter()
            .map(|index| values[*index].0.clone())
            .collect::<Vec<_>>();
        let production_sequence = sequence
            .iter()
            .map(|index| values[*index].1.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            production::key_order(&production_sequence)?,
            reference_keys::key_order(&reference_sequence),
            "sequence {sequence:?}"
        );
    }
    Ok(())
}

#[test]
fn xml_kernel_executes_the_exact_o17_carrier() {
    let strings = enumerate::xml_strings();
    assert_eq!(strings.len(), 341);
    let mut pairs = 0_usize;
    for left in &strings {
        let production_left = production::xml_noncdata(left);
        let reference_left = reference_keys::xml_noncdata(left);
        assert_eq!(production_left, reference_left);
        assert_eq!(production::xml_noncdata(&production_left), production_left);
        for right in &strings {
            assert_eq!(
                production_left == production::xml_noncdata(right),
                reference_left == reference_keys::xml_noncdata(right)
            );
            pairs += 1;
        }
    }
    assert_eq!(pairs, 116_281);
    assert_eq!(production::xml_noncdata("\r a \r"), "\r a \r");
    assert_eq!(reference_keys::xml_noncdata("\r a \r"), "\r a \r");
    println!("O17 XML carrier: strings={}, pairs={pairs}", strings.len());
}
