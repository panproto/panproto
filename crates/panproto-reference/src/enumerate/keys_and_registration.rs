//! Complete finite scopes for the key and registration page.

use crate::exact::{BigIntValue, parse_decimal};
use crate::keys::{KeyEquality, NumericPrimitive, SqlCollation, SqlPad};
use crate::model::SEdge;
use crate::num::Nat;
use crate::schema::{Key, KeyPart, KeyRole, KeyScope, Schema};
use crate::value::Value;

/// Every O17 structural graph, grouped implicitly by its vertex count.
///
/// For `n = 0, 1, 2, 3`, this enumerates every labelled graph on `v0..vn`
/// with the sole vertex kind `k`, the sole edge kind/name `prop:e`, and at
/// most three edges. The four group sizes are 1, 2, 15, and 130.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    (0..=3).flat_map(structural_schemas).collect()
}

/// The O17 structural graphs at one vertex count.
#[must_use]
pub fn structural_schemas(vertices: usize) -> Vec<Schema> {
    super::schemas(vertices, &["k"], &["e"], 3)
}

/// The two graph-independent O17 key-part atoms.
///
/// `Path([])` precedes the one-edge self path under the reference carrier's
/// derived order. Neither atom depends on whether the structural context
/// contains the named edge, which is what makes the descriptor factor
/// independent of the 148 graph contexts.
#[must_use]
pub fn key_part_atoms() -> [KeyPart; 2] {
    [
        KeyPart::Path(vec![]),
        KeyPart::Path(vec![SEdge::new("v0", "v0", "prop", Some("e"))]),
    ]
}

/// The six nonempty words of length at most two over [`key_part_atoms`].
#[must_use]
pub fn key_part_words() -> Vec<Vec<KeyPart>> {
    super::sequences(&key_part_atoms(), 2)
        .into_iter()
        .filter(|word| !word.is_empty())
        .collect()
}

/// One owner/key descriptor before assigning it to key slot `k0` or `k1`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyDescriptor {
    /// Vertex that owns the key.
    pub owner: String,
    /// The key's one- or two-part word.
    pub parts: Vec<KeyPart>,
    /// Key role.
    pub role: KeyRole,
    /// Key scope.
    pub scope: KeyScope,
}

impl KeyDescriptor {
    /// Materialize this descriptor under the supplied slot id.
    #[must_use]
    pub fn key(&self, id: &str) -> Key {
        Key {
            id: id.to_owned(),
            parts: self.parts.clone(),
            role: self.role,
            scope: self.scope.clone(),
            equality: vec![],
        }
    }
}

/// Add the two fixed declaration slots to one structural graph.
#[must_use]
pub fn schema_with_keys(mut graph: Schema, slots: [Option<&KeyDescriptor>; 2]) -> Schema {
    for (slot, descriptor) in slots.into_iter().enumerate() {
        if let Some(descriptor) = descriptor {
            graph
                .keys
                .entry(descriptor.owner.clone())
                .or_default()
                .push(descriptor.key(&format!("k{slot}")));
        }
    }
    for keys in graph.keys.values_mut() {
        keys.sort_by(|left, right| left.id.cmp(&right.id));
    }
    graph
}

/// Every O17 owner/key descriptor for an `n`-vertex context.
///
/// The product has `n * 6 * 5 * (n + 2) = 30n(n+2)` elements: `n` owners,
/// six part words, five roles, and the document, global, and `n` within
/// scopes.
#[must_use]
pub fn key_descriptors(vertices: usize) -> Vec<KeyDescriptor> {
    let roles = [
        KeyRole::Primary,
        KeyRole::Unique,
        KeyRole::Identifier,
        KeyRole::SortKey,
        KeyRole::Index,
    ];
    let scopes: Vec<_> = [KeyScope::Document, KeyScope::Global]
        .into_iter()
        .chain((0..vertices).map(|index| KeyScope::Within(format!("v{index}"))))
        .collect();
    let parts = key_part_words();
    let mut descriptors = Vec::new();
    for owner in (0..vertices).map(|index| format!("v{index}")) {
        for word in &parts {
            for role in roles {
                for scope in &scopes {
                    descriptors.push(KeyDescriptor {
                        owner: owner.clone(),
                        parts: word.clone(),
                        role,
                        scope: scope.clone(),
                    });
                }
            }
        }
    }
    descriptors
}

/// The changed axis of one directed O17 classification case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeAxis {
    /// `None -> Some(key)`.
    Add,
    /// `Some(key) -> None`.
    Remove,
    /// Change only the role.
    Role,
    /// Change only the scope.
    Scope,
    /// Change only the part word.
    Parts,
}

/// One directed one-axis change before graph and second-key contexts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AxisTransition {
    /// Changed axis.
    pub axis: ChangeAxis,
    /// Key in the source slot.
    pub before: Option<KeyDescriptor>,
    /// Key in the target slot.
    pub after: Option<KeyDescriptor>,
}

/// Every directed O17 one-axis transition for an `n`-vertex context.
///
/// Each descriptor contributes one add, one remove, four role changes,
/// `n+1` scope changes, and five part changes. The resulting exact count is
/// therefore `D_n * (n+12)`.
#[must_use]
pub fn axis_transitions(vertices: usize) -> Vec<AxisTransition> {
    let descriptors = key_descriptors(vertices);
    let roles = [
        KeyRole::Primary,
        KeyRole::Unique,
        KeyRole::Identifier,
        KeyRole::SortKey,
        KeyRole::Index,
    ];
    let scopes: Vec<_> = [KeyScope::Document, KeyScope::Global]
        .into_iter()
        .chain((0..vertices).map(|index| KeyScope::Within(format!("v{index}"))))
        .collect();
    let words = key_part_words();
    let mut transitions = Vec::new();
    for descriptor in descriptors {
        transitions.push(AxisTransition {
            axis: ChangeAxis::Add,
            before: None,
            after: Some(descriptor.clone()),
        });
        transitions.push(AxisTransition {
            axis: ChangeAxis::Remove,
            before: Some(descriptor.clone()),
            after: None,
        });
        transitions.extend(
            roles
                .into_iter()
                .filter(|role| *role != descriptor.role)
                .map(|role| AxisTransition {
                    axis: ChangeAxis::Role,
                    before: Some(descriptor.clone()),
                    after: Some(KeyDescriptor {
                        role,
                        ..descriptor.clone()
                    }),
                }),
        );
        transitions.extend(
            scopes
                .iter()
                .filter(|scope| **scope != descriptor.scope)
                .map(|scope| AxisTransition {
                    axis: ChangeAxis::Scope,
                    before: Some(descriptor.clone()),
                    after: Some(KeyDescriptor {
                        scope: scope.clone(),
                        ..descriptor.clone()
                    }),
                }),
        );
        transitions.extend(
            words
                .iter()
                .filter(|parts| **parts != descriptor.parts)
                .map(|parts| AxisTransition {
                    axis: ChangeAxis::Parts,
                    before: Some(descriptor.clone()),
                    after: Some(KeyDescriptor {
                        parts: parts.clone(),
                        ..descriptor.clone()
                    }),
                }),
        );
    }
    transitions
}

/// Context class of the untouched second key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OtherKeyClass {
    /// The other slot is empty.
    Absent,
    /// The other key has the changed key's owner.
    SameOwner,
    /// The other key has a different owner.
    DifferentOwner,
}

/// One row of the O17 context-factorization certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactorClass {
    /// Number of labelled vertices.
    pub vertices: usize,
    /// Changed key slot, zero or one.
    pub slot: usize,
    /// Changed axis.
    pub axis: ChangeAxis,
    /// Untouched second-key context.
    pub other: OtherKeyClass,
    /// Index of a representative structural graph within the `n` group.
    pub representative_graph: usize,
    /// Index of a representative in [`axis_transitions`].
    pub representative_transition: usize,
    /// Index of the representative untouched descriptor, when present.
    pub representative_other: Option<usize>,
    /// Number of full-carrier classifications represented by this row.
    pub multiplicity: u64,
}

/// The complete context map and multiplicities for O17 classification.
///
/// Rows split by vertex count, changed slot, changed axis, and whether the
/// untouched key is absent, has the same owner, or has a different owner.
/// Those classes exhaust the only possible interaction because key ids are
/// the fixed distinct slots `k0` and `k1`, while classification reads the
/// untouched declaration identically on both sides.
#[must_use]
pub fn factor_classes() -> Vec<FactorClass> {
    let mut classes = Vec::new();
    for vertices in 1..=3 {
        let graphs = structural_graph_count(u64::try_from(vertices).unwrap_or(0));
        let descriptors = key_descriptors(vertices);
        let transitions = axis_transitions(vertices);
        for slot in 0..2 {
            for axis in [
                ChangeAxis::Add,
                ChangeAxis::Remove,
                ChangeAxis::Role,
                ChangeAxis::Scope,
                ChangeAxis::Parts,
            ] {
                let Some(representative_transition) =
                    transitions.iter().position(|change| change.axis == axis)
                else {
                    continue;
                };
                let transition_count = transitions
                    .iter()
                    .filter(|change| change.axis == axis)
                    .count();
                let transition_count = u64::try_from(transition_count).unwrap_or(0);
                let owner = transitions[representative_transition]
                    .before
                    .as_ref()
                    .or(transitions[representative_transition].after.as_ref())
                    .map_or("v0", |descriptor| descriptor.owner.as_str());
                let same = descriptors
                    .iter()
                    .position(|descriptor| descriptor.owner == owner);
                let different = descriptors
                    .iter()
                    .position(|descriptor| descriptor.owner != owner);
                let same_count = descriptors
                    .iter()
                    .filter(|descriptor| descriptor.owner == owner)
                    .count();
                let same_count = u64::try_from(same_count).unwrap_or(0);
                let descriptor_total = u64::try_from(descriptors.len()).unwrap_or(0);
                let different_count = descriptor_total - same_count;
                for (other, representative_other, other_count) in [
                    (OtherKeyClass::Absent, None, 1),
                    (OtherKeyClass::SameOwner, same, same_count),
                    (OtherKeyClass::DifferentOwner, different, different_count),
                ] {
                    if other_count == 0 {
                        continue;
                    }
                    classes.push(FactorClass {
                        vertices,
                        slot,
                        axis,
                        other,
                        representative_graph: 0,
                        representative_transition,
                        representative_other,
                        multiplicity: graphs * transition_count * other_count,
                    });
                }
            }
        }
    }
    classes
}

/// Closed-form structural-graph count for one vertex count.
#[must_use]
pub fn structural_graph_count(vertices: u64) -> u64 {
    (0..=3)
        .map(|edges| super::choose(vertices * vertices, edges))
        .sum()
}

/// Closed-form descriptor count `D_n = 30n(n+2)`.
#[must_use]
pub const fn descriptor_count(vertices: u64) -> u64 {
    30 * vertices * (vertices + 2)
}

/// Closed-form size of the exact two-slot schema carrier.
#[must_use]
pub fn schema_count() -> u64 {
    (0..=3)
        .map(|vertices| {
            let choices = descriptor_count(vertices) + 1;
            structural_graph_count(vertices) * choices * choices
        })
        .sum()
}

/// Closed-form count of directed one-axis classifications.
#[must_use]
pub fn classification_count() -> u64 {
    (1..=3)
        .map(|vertices| {
            let descriptors = descriptor_count(vertices);
            structural_graph_count(vertices) * 2 * descriptors * (descriptors + 1) * (vertices + 12)
        })
        .sum()
}

/// Every subset of size at most two of a supplied labelled set.
#[must_use]
pub fn subsets<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    super::subsets(items, 2)
}

/// The exact 37-value B10/O17 equality carrier, in contract order.
#[must_use]
pub fn equality_values() -> Vec<Value> {
    let mut values = vec![
        Value::Int(-1),
        Value::Int(0),
        Value::Int(1),
        Value::Int(2),
        Value::BigInt(BigIntValue {
            negative: false,
            significand: Nat::from_u64(9_223_372_036_854_775_808),
            zeros: Nat::zero(),
        }),
        Value::BigInt(BigIntValue {
            negative: true,
            significand: Nat::from_u64(9_223_372_036_854_775_809),
            zeros: Nat::zero(),
        }),
    ];
    values.extend(
        [
            0.0_f64,
            -0.0,
            0.5,
            0.1,
            1.0,
            2.0,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::from_bits(0x7ff8_0000_0000_0001),
            f64::from_bits(0xfff8_0000_0000_0002),
        ]
        .into_iter()
        .map(|value| Value::Float(value.to_bits())),
    );
    values.extend(
        [
            "0",
            "-0",
            "-0E+3",
            "1",
            "1.0",
            "1.00",
            "0.5",
            "0.1",
            "2",
            "-Infinity",
            "Infinity",
            "NaN1",
            "-sNaN2",
        ]
        .into_iter()
        .filter_map(parse_decimal)
        .map(Value::Decimal),
    );
    values.extend(
        ["", "a", "A", "a ", "a  ", " a", "é", "É"]
            .into_iter()
            .map(|value| Value::Str(value.to_owned())),
    );
    values
}

/// The twelve computable B10/O17 equality contexts.
///
/// `within-int` and `within-decimal` deliberately carry the same public
/// `Decimal` primitive: XSD places both declarations in one numeric
/// primitive, while O17 retains the two declaration contexts in its census.
#[must_use]
pub fn equality_configurations() -> Vec<(&'static str, KeyEquality)> {
    vec![
        ("representation", KeyEquality::Representation),
        ("numeric", KeyEquality::NumericValue),
        (
            "within-int",
            KeyEquality::NumericValueWithinKind(NumericPrimitive::Decimal),
        ),
        (
            "within-decimal",
            KeyEquality::NumericValueWithinKind(NumericPrimitive::Decimal),
        ),
        (
            "within-float32",
            KeyEquality::NumericValueWithinKind(NumericPrimitive::Float(32)),
        ),
        (
            "within-float64",
            KeyEquality::NumericValueWithinKind(NumericPrimitive::Float(64)),
        ),
        (
            "within-any",
            KeyEquality::NumericValueWithinKind(NumericPrimitive::Any),
        ),
        (
            "sql-binary-no-pad",
            KeyEquality::Sql {
                collation: SqlCollation::Binary,
                pad: SqlPad::NoPad,
            },
        ),
        (
            "sql-binary-pad-space",
            KeyEquality::Sql {
                collation: SqlCollation::Binary,
                pad: SqlPad::PadSpace,
            },
        ),
        (
            "sql-nocase-no-pad",
            KeyEquality::Sql {
                collation: SqlCollation::AsciiCaseInsensitive,
                pad: SqlPad::NoPad,
            },
        ),
        (
            "sql-nocase-pad-space",
            KeyEquality::Sql {
                collation: SqlCollation::AsciiCaseInsensitive,
                pad: SqlPad::PadSpace,
            },
        ),
        ("xml-noncdata", KeyEquality::XmlNonCdata),
    ]
}

/// Every string of length at most four over the O17 XML alphabet.
///
/// The alphabet contains content, U+0020 (the normalization character), and
/// two XML-whitespace characters that this post-attribute-processing kernel
/// must preserve. CR preservation is pinned separately by the differential.
#[must_use]
pub fn xml_strings() -> Vec<String> {
    const ALPHABET: [char; 4] = ['a', ' ', '\t', '\n'];
    let mut strings = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..4 {
        frontier = frontier
            .iter()
            .flat_map(|prefix| {
                ALPHABET.into_iter().map(move |symbol| {
                    let mut text = prefix.clone();
                    text.push(symbol);
                    text
                })
            })
            .collect();
        strings.extend(frontier.iter().cloned());
    }
    strings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o17_factorization_reconciles_both_binding_totals() {
        let graph_counts: Vec<_> = (0..=3)
            .map(|vertices| structural_schemas(vertices).len())
            .collect();
        assert_eq!(graph_counts, [1, 2, 15, 130]);
        assert_eq!(schemas().len(), 148);

        let descriptor_counts: Vec<_> = (1..=3)
            .map(|vertices| key_descriptors(vertices).len())
            .collect();
        assert_eq!(descriptor_counts, [90, 240, 450]);
        for vertices in 1..=3 {
            let expected = descriptor_count(u64::try_from(vertices).unwrap_or(0));
            assert_eq!(
                u64::try_from(key_descriptors(vertices).len()).unwrap_or(0),
                expected
            );
            assert_eq!(
                u64::try_from(axis_transitions(vertices).len()).unwrap_or(0),
                expected * (u64::try_from(vertices).unwrap_or(0) + 12)
            );
        }

        assert_eq!(schema_count(), 27_329_908);
        assert_eq!(classification_count(), 816_223_680);
        assert_eq!(
            factor_classes()
                .iter()
                .map(|class| class.multiplicity)
                .sum::<u64>(),
            classification_count()
        );
    }

    #[test]
    fn every_factor_row_has_a_witness_in_its_declared_context_class() {
        for class in factor_classes() {
            let graphs = structural_schemas(class.vertices);
            let transitions = axis_transitions(class.vertices);
            let descriptors = key_descriptors(class.vertices);
            assert!(class.representative_graph < graphs.len());
            assert!(class.representative_transition < transitions.len());
            assert_eq!(
                transitions[class.representative_transition].axis,
                class.axis
            );

            let changed_owner = transitions[class.representative_transition]
                .before
                .as_ref()
                .or(transitions[class.representative_transition].after.as_ref())
                .map(|descriptor| descriptor.owner.as_str());
            match (class.other, class.representative_other) {
                (OtherKeyClass::Absent, None) => {}
                (OtherKeyClass::SameOwner, Some(index)) => {
                    assert!(index < descriptors.len());
                    assert_eq!(Some(descriptors[index].owner.as_str()), changed_owner);
                }
                (OtherKeyClass::DifferentOwner, Some(index)) => {
                    assert!(index < descriptors.len());
                    assert_ne!(Some(descriptors[index].owner.as_str()), changed_owner);
                }
                _ => panic!("factor row has no representative for its declared context"),
            }
        }
    }

    #[test]
    fn the_two_key_slots_preserve_ids_and_owner_grouping() {
        let descriptors = key_descriptors(2);
        let first = &descriptors[0];
        let second_index = descriptors
            .iter()
            .position(|descriptor| descriptor.owner != first.owner);
        assert!(second_index.is_some());
        let second = &descriptors[second_index.unwrap_or(0)];
        let schema = schema_with_keys(structural_schemas(2).remove(0), [Some(first), Some(second)]);
        let ids: Vec<_> = schema
            .keys
            .values()
            .flatten()
            .map(|key| key.id.as_str())
            .collect();
        assert_eq!(ids, ["k0", "k1"]);
    }
}
