//! Key values of graph elements and the key buckets of a scope.
//!
//! The value of a key path `[e1, ..., ek]` at an element is read along its
//! out-edges: each hop follows the out-edges carrying that label, in edge
//! order, and the value is the first non-null own value met at the last
//! hop; an element with no out-edge for the last hop reads its field named
//! by that edge (the field fallback). A null or absent part is SQL's NULL:
//! the element then has no key value and joins no bucket. A `Within(v)` key
//! is scoped by the nearest ancestor anchored at v (walking back one edge at
//! a time, the least of the nearest); an element with no such ancestor is
//! outside the key.

use std::collections::{BTreeMap, BTreeSet};

use crate::exact::{Decimal, ExactInteger};
use crate::float::{FloatValue, Format, decode};
use crate::model::{Graph, NodeShape, SEdge};
use crate::num::{Int, Rat};
use crate::schema::{Key, KeyPart, KeyScope};
use crate::value::{Presence, Value};

/// Equality declared for one key part.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeyEquality {
    /// Canonical representation equality.
    Representation,
    /// Exact mathematical equality across numeric variants.
    NumericValue,
    /// Exact numeric equality, but only within one declared primitive.
    NumericValueWithinKind(NumericPrimitive),
    /// SQL numeric and text equality.
    Sql {
        /// Text collation.
        collation: SqlCollation,
        /// Text padding rule.
        pad: SqlPad,
    },
    /// XML non-CDATA whitespace normalization, then representation.
    XmlNonCdata,
}

/// Primitive used by `NumericValueWithinKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericPrimitive {
    /// The integer/decimal primitive.
    Decimal,
    /// A binary-float primitive of this width.
    Float(u8),
    /// Select the primitive from the value variant.
    Any,
}

/// SQL collation whose exact kernel the reference can compute.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SqlCollation {
    /// Unicode scalar-value order.
    Binary,
    /// Fold ASCII A through Z only.
    AsciiCaseInsensitive,
    /// An implementation-defined collation; text comparison is undefined.
    Named(String),
}

/// SQL treatment of trailing spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SqlPad {
    /// Keep trailing spaces.
    NoPad,
    /// Ignore trailing U+0020.
    PadSpace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NumericKey {
    NegativeInfinity,
    Finite(Rat),
    PositiveInfinity,
    NaN,
}

fn numeric_key(value: &Value) -> Option<NumericKey> {
    match value {
        Value::Int(integer) => Some(NumericKey::Finite(Rat::from_int(Int::from_i64(*integer)))),
        Value::BigInt(integer) => ExactInteger::Big(integer.clone())
            .value()
            .map(NumericKey::Finite),
        Value::Float(bits) => Some(match decode(Format::B64, *bits) {
            FloatValue::Finite { value, .. } => NumericKey::Finite(value),
            FloatValue::Infinite { negative: true } => NumericKey::NegativeInfinity,
            FloatValue::Infinite { negative: false } => NumericKey::PositiveInfinity,
            FloatValue::NaN { .. } => NumericKey::NaN,
        }),
        Value::Decimal(Decimal::Finite(decimal)) => decimal.exponent.to_i64().map(|exponent| {
            NumericKey::Finite(
                Rat::from_int(Int::new(decimal.negative, decimal.digits.clone()))
                    .mul(&Rat::pow10(exponent)),
            )
        }),
        Value::Decimal(Decimal::Infinite { negative: true }) => Some(NumericKey::NegativeInfinity),
        Value::Decimal(Decimal::Infinite { negative: false }) => Some(NumericKey::PositiveInfinity),
        Value::Decimal(Decimal::NaN { .. }) => Some(NumericKey::NaN),
        _ => None,
    }
}

const fn primitive(value: &Value) -> Option<NumericPrimitive> {
    match value {
        Value::Int(_) | Value::BigInt(_) | Value::Decimal(_) => Some(NumericPrimitive::Decimal),
        Value::Float(_) => Some(NumericPrimitive::Float(64)),
        _ => None,
    }
}

fn sql_text(text: &str, collation: &SqlCollation, pad: SqlPad) -> Option<String> {
    if matches!(collation, SqlCollation::Named(_)) {
        return None;
    }
    let text = if pad == SqlPad::PadSpace {
        text.trim_end_matches(' ')
    } else {
        text
    };
    Some(match collation {
        SqlCollation::Binary => text.to_owned(),
        SqlCollation::AsciiCaseInsensitive => text.to_ascii_lowercase(),
        SqlCollation::Named(_) => return None,
    })
}

/// Normalize XML non-CDATA whitespace: replace XML whitespace by U+0020,
/// collapse runs, and trim both ends.
#[must_use]
pub fn xml_noncdata(text: &str) -> String {
    text.split([' ', '\t', '\r', '\n'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Equality of two part values under a declaration.
///
/// `None` is the required refusal for text under a named SQL collation.
#[must_use]
pub fn key_equivalence(equality: &KeyEquality, left: &Value, right: &Value) -> Option<bool> {
    match equality {
        KeyEquality::Representation => crate::value::representation(left, right),
        KeyEquality::NumericValue => match (numeric_key(left), numeric_key(right)) {
            (Some(a), Some(b)) => Some(a == b),
            _ => crate::value::representation(left, right),
        },
        KeyEquality::NumericValueWithinKind(declared) => {
            match (numeric_key(left), numeric_key(right)) {
                (Some(a), Some(b)) => {
                    let left_primitive = if *declared == NumericPrimitive::Any {
                        primitive(left)
                    } else {
                        Some(*declared)
                    };
                    let right_primitive = if *declared == NumericPrimitive::Any {
                        primitive(right)
                    } else {
                        Some(*declared)
                    };
                    Some(left_primitive == right_primitive && a == b)
                }
                _ => crate::value::representation(left, right),
            }
        }
        KeyEquality::Sql { collation, pad } => match (numeric_key(left), numeric_key(right)) {
            (Some(a), Some(b)) => Some(a == b),
            (None, None) => match (left, right) {
                (Value::Str(a), Value::Str(b)) => {
                    Some(sql_text(a, collation, *pad)? == sql_text(b, collation, *pad)?)
                }
                _ => crate::value::representation(left, right),
            },
            _ => Some(false),
        },
        KeyEquality::XmlNonCdata => match (left, right) {
            (Value::Str(a), Value::Str(b)) => Some(xml_noncdata(a) == xml_noncdata(b)),
            _ => crate::value::representation(left, right),
        },
    }
}

/// Representation equality refines every computable declared equality.
#[must_use]
pub fn representation_refines(equality: &KeyEquality, left: &Value, right: &Value) -> Option<bool> {
    let represented = crate::value::representation(left, right)?;
    Some(!represented || key_equivalence(equality, left, right)?)
}

/// Compare values in the sort-key order.
#[must_use]
pub fn sort_order(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    match (numeric_key(left), numeric_key(right)) {
        (Some(NumericKey::Finite(a)), Some(NumericKey::Finite(b))) => Some(a.cmp(&b)),
        (None, None) => match (left, right) {
            (Value::Str(a), Value::Str(b)) => Some(a.as_bytes().cmp(b.as_bytes())),
            (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
            _ => None,
        },
        _ => None,
    }
}

/// Whether a sequence has no adjacent sort-key descent.
#[must_use]
pub fn key_order(values: &[Value]) -> Option<bool> {
    values
        .windows(2)
        .map(|pair| sort_order(&pair[0], &pair[1]).map(|order| !order.is_gt()))
        .try_fold(true, |all, ordered| ordered.map(|one| all && one))
}

/// Validate a target-set declaration expressed as `(vertex, key, arity,
/// equality)` members.
#[must_use]
pub fn target_set(members: &[(String, String, usize, Vec<KeyEquality>)]) -> bool {
    let distinct = members
        .iter()
        .map(|(vertex, key, _, _)| (vertex, key))
        .collect::<BTreeSet<_>>()
        .len()
        == members.len();
    let compatible = members.first().is_none_or(|(_, _, arity, equality)| {
        members.iter().all(|(_, _, other_arity, other_equality)| {
            arity == other_arity && equality == other_equality
        })
    });
    distinct && compatible
}

/// Index matching with an explicit null-match switch.
#[must_use]
pub const fn index_and_null_match(
    index: Option<u64>,
    target: Option<u64>,
    null_matches: bool,
) -> bool {
    match (index, target) {
        (Some(left), Some(right)) => left == right,
        (None, None) => null_matches,
        _ => false,
    }
}

/// Whether a lexeme belongs to a declared lexical space.
#[must_use]
pub fn lexical_space(text: &str, accepts: impl FnOnce(&str) -> bool) -> bool {
    accepts(text)
}

/// A format annotation does not assert validity by itself.
#[must_use]
pub const fn annotated_format(annotation: Option<&str>) -> Option<&str> {
    annotation
}

fn own(g: &Graph, n: u32) -> Option<Value> {
    match g.nodes.get(&n)?.value.as_ref()? {
        Presence::Present(v) if *v != Value::Null => Some(v.clone()),
        _ => None,
    }
}

/// The value of a key path at node `n`, or `None` (NULL).
#[must_use]
pub fn path_value(g: &Graph, n: u32, path: &[SEdge]) -> Option<Value> {
    let (last, hops) = path.split_last()?;
    let mut frontier = vec![n];
    for hop in hops {
        frontier = frontier
            .iter()
            .flat_map(|x| {
                g.edges
                    .values()
                    .filter(move |e| e.src == *x && e.label == *hop)
                    .map(|e| e.tgt)
            })
            .collect();
    }
    frontier.into_iter().find_map(|x| {
        let targets: Vec<u32> = g
            .edges
            .values()
            .filter(|e| e.src == x && e.label == *last)
            .map(|e| e.tgt)
            .collect();
        if targets.is_empty() {
            g.nodes
                .get(&x)?
                .fields
                .get(last.column())
                .filter(|v| **v != Value::Null)
                .cloned()
        } else {
            targets.into_iter().find_map(|t| own(g, t))
        }
    })
}

/// The key value of `n` under `key`.
///
/// One value per part, or `None` when a part is NULL. A position part is the index of `n` among the
/// targets of its ordered edge out of `n`'s parent, by position then edge id.
#[must_use]
pub fn key_value(g: &Graph, n: u32, key: &Key) -> Option<Vec<Value>> {
    key.parts
        .iter()
        .map(|p| match p {
            KeyPart::Path(path) => path_value(g, n, path),
            KeyPart::Position { within } => {
                let parent = g
                    .edges
                    .values()
                    .find(|e| e.tgt == n && e.label == *within)?
                    .src;
                let mut sibs: Vec<(Option<u32>, u32, u32)> = g
                    .edges
                    .iter()
                    .filter(|(_, e)| e.src == parent && e.label == *within)
                    .map(|(i, e)| (e.position, *i, e.tgt))
                    .collect();
                sibs.sort();
                sibs.iter()
                    .position(|s| s.2 == n)
                    .and_then(|i| i64::try_from(i).ok())
                    .map(Value::Int)
            }
        })
        .collect()
}

/// The nearest ancestors of `n` anchored at `v` (walking back one edge at a
/// time over non-fan nodes); the least of them, or `None`.
#[must_use]
pub fn nearest_ancestor(g: &Graph, n: u32, v: &str) -> Option<u32> {
    let mut seen = BTreeSet::from([n]);
    let mut level = vec![n];
    while !level.is_empty() {
        let mut next: Vec<u32> = level
            .iter()
            .flat_map(|x| g.edges.values().filter(move |e| e.tgt == *x).map(|e| e.src))
            .filter(|p| seen.insert(*p))
            .collect();
        next.sort_unstable();
        if let Some(a) = next.iter().find(|x| {
            g.nodes
                .get(x)
                .is_some_and(|m| m.anchor == v && m.shape != NodeShape::HyperEdge)
        }) {
            return Some(*a);
        }
        level = next;
    }
    None
}

/// Key buckets: (scope, canonical key bytes) to (key value, members).
pub type Buckets = BTreeMap<(Option<u32>, Vec<u8>), (Vec<Value>, Vec<u32>)>;

/// The buckets of a key over the non-fan nodes anchored at `vertex`.
///
/// Each bucket key is the scope (the nearest ancestor for `Within`, else none) and the canonical
/// bytes of the key value; elements with a NULL part are left out, and so (separately returned) are
/// `Within` elements with no ancestor.
#[must_use]
pub fn buckets(g: &Graph, vertex: &str, key: &Key) -> (Buckets, Vec<u32>) {
    let mut out = Buckets::new();
    let mut outside = Vec::new();
    for (id, n) in &g.nodes {
        if n.anchor != vertex || n.shape == NodeShape::HyperEdge {
            continue;
        }
        let Some(values) = key_value(g, *id, key) else {
            continue;
        };
        let scope = match &key.scope {
            KeyScope::Within(v) => {
                let Some(a) = nearest_ancestor(g, *id, v) else {
                    outside.push(*id);
                    continue;
                };
                Some(a)
            }
            KeyScope::Global | KeyScope::Document => None,
        };
        let bytes: Vec<u8> = values
            .iter()
            .flat_map(|v| crate::canonical::value_bytes(v).unwrap_or_default())
            .collect();
        out.entry((scope, bytes))
            .or_insert_with(|| (values, Vec::new()))
            .1
            .push(*id);
    }
    (out, outside)
}
