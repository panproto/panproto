//! Complete reference operations assigned to `keys-and-registration`.

pub mod keys {
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

    use crate::canonical::Writer;
    use crate::exact::{Decimal, DecimalValue, ExactInteger};
    use crate::float::{FloatValue, Format, decode};
    use crate::model::{Graph, NodeShape, SEdge};
    use crate::num::{Int, Nat, Rat};
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

    static REPRESENTATION_EQUALITY: KeyEquality = KeyEquality::Representation;

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

    #[derive(Clone, Debug)]
    enum NumericKey {
        NegativeInfinity,
        Finite(DecimalValue),
        PositiveInfinity,
        NaN,
    }

    impl PartialEq for NumericKey {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (Self::NegativeInfinity, Self::NegativeInfinity)
                | (Self::PositiveInfinity, Self::PositiveInfinity)
                | (Self::NaN, Self::NaN) => true,
                (Self::Finite(left), Self::Finite(right)) => left.cmp_numeric(right).is_eq(),
                _ => false,
            }
        }
    }

    impl Eq for NumericKey {}

    impl PartialOrd for NumericKey {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for NumericKey {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            use NumericKey::{Finite, NaN, NegativeInfinity, PositiveInfinity};
            match (self, other) {
                (NegativeInfinity, NegativeInfinity)
                | (PositiveInfinity, PositiveInfinity)
                | (NaN, NaN) => std::cmp::Ordering::Equal,
                (NegativeInfinity, _) | (_, NaN) => std::cmp::Ordering::Less,
                (_, NegativeInfinity) | (NaN | PositiveInfinity, _) => std::cmp::Ordering::Greater,
                (_, PositiveInfinity) => std::cmp::Ordering::Less,
                (Finite(left), Finite(right)) => left.cmp_numeric(right),
            }
        }
    }

    fn normalize_decimal(mut value: DecimalValue) -> DecimalValue {
        if value.is_zero() {
            value.negative = false;
            value.exponent = Int::zero();
            return value;
        }
        let ten = Nat::from_u64(10);
        while value.digits.digit(0) == 0 {
            let (digits, remainder) = value.digits.div_rem(&ten).unwrap_or_default();
            debug_assert!(remainder.is_zero());
            value.digits = digits;
            value.exponent = value.exponent.add(&Int::from_i64(1));
        }
        value
    }

    fn rational_decimal(value: &Rat) -> Option<DecimalValue> {
        let two = Nat::from_u64(2);
        let mut denominator = value.den().clone();
        let mut power = 0u32;
        while denominator != Nat::one() {
            let (quotient, remainder) = denominator.div_rem(&two)?;
            if !remainder.is_zero() {
                return None;
            }
            denominator = quotient;
            power = power.checked_add(1)?;
        }
        let digits = value
            .num()
            .magnitude()
            .mul(&Nat::pow(&Nat::from_u64(5), power));
        Some(normalize_decimal(DecimalValue {
            negative: value.num().is_negative(),
            digits,
            exponent: Int::new(true, Nat::from_u64(u64::from(power))),
        }))
    }

    fn numeric_key(value: &Value) -> Option<NumericKey> {
        match value {
            Value::Int(integer) => Some(NumericKey::Finite(normalize_decimal(
                ExactInteger::Int(*integer).as_decimal(),
            ))),
            Value::BigInt(integer) => Some(NumericKey::Finite(normalize_decimal(
                ExactInteger::Big(integer.clone()).as_decimal(),
            ))),
            Value::Float(bits) => Some(match decode(Format::B64, *bits) {
                FloatValue::Finite { value, .. } => NumericKey::Finite(rational_decimal(&value)?),
                FloatValue::Infinite { negative: true } => NumericKey::NegativeInfinity,
                FloatValue::Infinite { negative: false } => NumericKey::PositiveInfinity,
                FloatValue::NaN { .. } => NumericKey::NaN,
            }),
            Value::Decimal(Decimal::Finite(decimal)) => {
                Some(NumericKey::Finite(normalize_decimal(decimal.clone())))
            }
            Value::Decimal(Decimal::Infinite { negative: true }) => {
                Some(NumericKey::NegativeInfinity)
            }
            Value::Decimal(Decimal::Infinite { negative: false }) => {
                Some(NumericKey::PositiveInfinity)
            }
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

    fn write_numeric_key(writer: &mut Writer, key: &NumericKey) {
        match key {
            NumericKey::NegativeInfinity => writer.tag(0),
            NumericKey::Finite(value) => {
                writer.tag(1);
                writer.tag(u8::from(value.negative));
                writer.str(&value.digits.to_string());
                writer.tag(u8::from(value.exponent.is_negative()));
                writer.str(&value.exponent.magnitude().to_string());
            }
            NumericKey::PositiveInfinity => writer.tag(2),
            NumericKey::NaN => writer.tag(3),
        }
    }

    fn write_representation(writer: &mut Writer, value: &Value) {
        let bytes = crate::canonical::value_bytes(value).unwrap_or_default();
        writer.raw(&bytes);
    }

    fn write_primitive(writer: &mut Writer, primitive: NumericPrimitive) {
        match primitive {
            NumericPrimitive::Decimal => writer.tag(0),
            NumericPrimitive::Float(width) => {
                writer.tag(1);
                writer.tag(width);
            }
            NumericPrimitive::Any => writer.tag(2),
        }
    }

    /// A declared key function is undefined only for text under a named SQL
    /// collation, whose comparison belongs to the database implementation.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum KeyingError {
        /// Text under an implementation-defined collation.
        UnsupportedCollation(String),
        /// A non-empty equality vector did not have one entry per part.
        EqualityArity {
            /// Number of key parts.
            expected: usize,
            /// Number of equality declarations.
            found: usize,
        },
    }

    fn equality_key(equality: &KeyEquality, value: &Value) -> Result<Vec<u8>, KeyingError> {
        let mut writer = Writer::default();
        match equality {
            KeyEquality::Representation => {
                writer.tag(0);
                write_representation(&mut writer, value);
            }
            KeyEquality::NumericValue => {
                writer.tag(1);
                if let Some(key) = numeric_key(value) {
                    writer.tag(0);
                    write_numeric_key(&mut writer, &key);
                } else {
                    writer.tag(1);
                    write_representation(&mut writer, value);
                }
            }
            KeyEquality::NumericValueWithinKind(declared) => {
                writer.tag(2);
                let selected = if *declared == NumericPrimitive::Any {
                    primitive(value).unwrap_or(NumericPrimitive::Any)
                } else {
                    *declared
                };
                write_primitive(&mut writer, selected);
                if let Some(key) = numeric_key(value) {
                    writer.tag(0);
                    write_numeric_key(&mut writer, &key);
                } else {
                    writer.tag(1);
                    write_representation(&mut writer, value);
                }
            }
            KeyEquality::Sql { collation, pad } => {
                writer.tag(3);
                if let Some(key) = numeric_key(value) {
                    writer.tag(0);
                    write_numeric_key(&mut writer, &key);
                } else if let Value::Str(text) = value {
                    writer.tag(1);
                    let text = sql_text(text, collation, *pad).ok_or_else(|| match collation {
                        SqlCollation::Named(name) => {
                            KeyingError::UnsupportedCollation(name.clone())
                        }
                        SqlCollation::Binary | SqlCollation::AsciiCaseInsensitive => unreachable!(),
                    })?;
                    writer.str(&text);
                } else {
                    writer.tag(2);
                    write_representation(&mut writer, value);
                }
            }
            KeyEquality::XmlNonCdata => {
                writer.tag(4);
                if let Value::Str(text) = value {
                    writer.tag(0);
                    writer.str(&xml_noncdata(text));
                } else {
                    writer.tag(1);
                    write_representation(&mut writer, value);
                }
            }
        }
        Ok(writer.bytes)
    }

    fn equalities(key: &Key) -> Result<Vec<&KeyEquality>, KeyingError> {
        if key.equality.is_empty() {
            return Ok((0..key.parts.len())
                .map(|_| &REPRESENTATION_EQUALITY)
                .collect());
        }
        if key.equality.len() != key.parts.len() {
            return Err(KeyingError::EqualityArity {
                expected: key.parts.len(),
                found: key.equality.len(),
            });
        }
        Ok(key.equality.iter().collect())
    }

    /// Definition: Compare two complete key tuples under the target key's
    /// declared per-part equality.
    ///
    /// # Errors
    ///
    /// [`KeyingError`] when the equality vector has the wrong arity or names
    /// a collation this independent reference does not evaluate.
    ///
    /// Book anchors: `L-keys-and-registration-8`, `L-resolution-14`.
    pub fn tuple_equivalence(
        key: &Key,
        left: &[Value],
        right: &[Value],
    ) -> Result<bool, KeyingError> {
        if left.len() != key.parts.len() || right.len() != key.parts.len() {
            return Ok(false);
        }
        for ((equality, left), right) in equalities(key)?.into_iter().zip(left).zip(right) {
            if !key_equivalence(equality, left, right).ok_or_else(|| match equality {
                KeyEquality::Sql {
                    collation: SqlCollation::Named(name),
                    ..
                } => KeyingError::UnsupportedCollation(name.clone()),
                _ => unreachable!(),
            })? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn tuple_key(key: &Key, values: &[Value]) -> Result<Vec<u8>, KeyingError> {
        if values.len() != key.parts.len() {
            return Err(KeyingError::EqualityArity {
                expected: key.parts.len(),
                found: values.len(),
            });
        }
        let mut writer = Writer::default();
        writer.len(values.len());
        for (equality, value) in equalities(key)?.into_iter().zip(values) {
            let part = equality_key(equality, value)?;
            writer.len(part.len());
            writer.raw(&part);
        }
        Ok(writer.bytes)
    }

    /// Definition: Normalize XML non-CDATA whitespace: replace XML whitespace by U+0020,
    /// collapse runs, and trim both ends.
    ///
    /// Book anchors: `L-keys-and-registration-14`.
    #[must_use]
    pub fn xml_noncdata(text: &str) -> String {
        text.split([' ', '\t', '\r', '\n'])
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Definition: Equality of two part values under a declaration.
    ///
    /// `None` is the required refusal for text under a named SQL collation.
    ///
    /// Book anchors: `L-keys-and-registration-8`.
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

    /// Definition: Representation equality refines every computable declared equality.
    ///
    /// Book anchors: `L-keys-and-registration-9`.
    #[must_use]
    pub fn representation_refines(
        equality: &KeyEquality,
        left: &Value,
        right: &Value,
    ) -> Option<bool> {
        let represented = crate::value::representation(left, right)?;
        Some(!represented || key_equivalence(equality, left, right)?)
    }

    /// Definition: Compare values in the sort-key order.
    ///
    /// Book anchors: `L-keys-and-registration-11`.
    #[must_use]
    pub fn sort_order(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
        match (numeric_key(left), numeric_key(right)) {
            (Some(NumericKey::NaN), _) | (_, Some(NumericKey::NaN)) => None,
            (Some(NumericKey::NegativeInfinity), Some(NumericKey::NegativeInfinity))
            | (Some(NumericKey::PositiveInfinity), Some(NumericKey::PositiveInfinity)) => {
                Some(std::cmp::Ordering::Equal)
            }
            (Some(NumericKey::NegativeInfinity), Some(_))
            | (Some(NumericKey::Finite(_)), Some(NumericKey::PositiveInfinity)) => {
                Some(std::cmp::Ordering::Less)
            }
            (Some(_), Some(NumericKey::NegativeInfinity))
            | (Some(NumericKey::PositiveInfinity), Some(NumericKey::Finite(_))) => {
                Some(std::cmp::Ordering::Greater)
            }
            (Some(NumericKey::Finite(a)), Some(NumericKey::Finite(b))) => Some(a.cmp_numeric(&b)),
            (None, None) => match (left, right) {
                (Value::Str(a), Value::Str(b)) => Some(a.as_bytes().cmp(b.as_bytes())),
                (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Definition: Whether a sequence has no adjacent sort-key descent.
    ///
    /// Book anchors: `L-keys-and-registration-12`.
    #[must_use]
    pub fn key_order(values: &[Value]) -> Option<bool> {
        values
            .windows(2)
            .map(|pair| sort_order(&pair[0], &pair[1]).map(|order| !order.is_gt()))
            .try_fold(true, |all, ordered| ordered.map(|one| all && one))
    }

    /// Definition: Validate a target-set declaration expressed as `(vertex, key, arity,
    /// equality)` members.
    ///
    /// Book anchors: `L-keys-and-registration-13`.
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

    /// Definition: Index matching with an explicit null-match switch.
    ///
    /// Book anchors: `L-keys-and-registration-16`.
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

    /// Definition: Whether a lexeme belongs to a declared lexical space.
    ///
    /// Book anchors: `L-keys-and-registration-17`.
    #[must_use]
    pub fn lexical_space(text: &str, accepts: impl FnOnce(&str) -> bool) -> bool {
        accepts(text)
    }

    /// Definition: A format annotation does not assert validity by itself.
    ///
    /// Book anchors: `L-keys-and-registration-18`.
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

    /// Definition: The key value of `n` under `key`.
    ///
    /// One value per part, or `None` when a part is NULL. A position part is the index of `n` among the
    /// targets of its ordered edge out of `n`'s parent, by position then edge id.
    ///
    /// Book anchors: `L-keys-and-registration-1`.
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
                        .find(|e| {
                            e.tgt == n && e.label == *within && ordinary_arc(g, e.src, e.tgt, e)
                        })?
                        .src;
                    let mut sibs: Vec<(Option<u32>, u32, u32)> = g
                        .edges
                        .iter()
                        .filter(|(_, e)| {
                            e.src == parent
                                && e.label == *within
                                && ordinary_arc(g, e.src, e.tgt, e)
                        })
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
                .flat_map(|x| {
                    g.edges
                        .values()
                        .filter(move |e| e.tgt == *x && ordinary_arc(g, e.src, e.tgt, e))
                        .map(|e| e.src)
                })
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

    /// Whether an instance edge is an ordinary-node arc rather than fan
    /// incidence vocabulary.
    fn ordinary_arc(g: &Graph, src: u32, tgt: u32, edge: &crate::model::GEdge) -> bool {
        !crate::restrict::is_fan_kind(&edge.label.kind)
            && [src, tgt].into_iter().all(|id| {
                g.nodes
                    .get(&id)
                    .is_some_and(|node| node.shape != NodeShape::HyperEdge)
            })
    }

    /// Key buckets: (scope, canonical key bytes) to (key value, members).
    pub type Buckets = BTreeMap<(Option<u32>, Vec<u8>), (Vec<Value>, Vec<u32>)>;

    /// Definition: The buckets of a key over the non-fan nodes anchored at `vertex`.
    ///
    /// Each bucket key is the scope (the nearest ancestor for `Within`, else none) and the canonical
    /// bytes of the key value; elements with a NULL part are left out, and so (separately returned) are
    /// `Within` elements with no ancestor.
    ///
    /// # Errors
    ///
    /// [`KeyingError`] when the equality vector has the wrong arity or names
    /// a collation this independent reference does not evaluate.
    ///
    /// Book anchors: `L-keys-and-registration-2`.
    pub fn buckets(g: &Graph, vertex: &str, key: &Key) -> Result<(Buckets, Vec<u32>), KeyingError> {
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
            let bytes = tuple_key(key, &values)?;
            out.entry((scope, bytes))
                .or_insert_with(|| (values, Vec::new()))
                .1
                .push(*id);
        }
        Ok((out, outside))
    }
}

pub mod schema {
    //! The schema model: vertices with kinds, schema edges, hyper-edges,
    //! constraints, required and ordered edges, entries, and the key and
    //! reference declarations.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::canonical::Writer;
    use crate::keys::KeyEquality;
    use crate::model::{Name, SEdge};
    use crate::theory::{
        CompositionSpec, Defect as TheoryDefect, RecomposeError, Theory,
        canonical_composition_spec_bytes, canonical_theory_bytes, recompose, typecheck_signature,
    };
    use crate::value::Value;

    /// A hyper-edge: a kind, a signature of labels to vertices, and the label
    /// naming its parent vertex.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct HyperEdge {
        /// Kind.
        pub kind: Name,
        /// Label to vertex.
        pub signature: BTreeMap<Name, Name>,
        /// The parent label.
        pub parent_label: Name,
    }

    /// A part of a key: a path of edges from the keyed vertex to a scalar, or
    /// the element's position among the targets of an ordered edge.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum KeyPart {
        /// A path of edges.
        Path(Vec<SEdge>),
        /// The index among the targets of `within`.
        Position {
            /// The ordered edge.
            within: SEdge,
        },
    }

    /// The role of a key.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum KeyRole {
        /// A primary key (its parts are required).
        Primary,
        /// A unique key.
        Unique,
        /// An identifier.
        Identifier,
        /// A stable-sort key; it imposes ordering but not uniqueness.
        SortKey,
        /// An index; it imposes neither uniqueness nor resolution.
        Index,
    }

    /// The uniqueness domain of a key.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum KeyScope {
        /// The document (the whole instance for a single-document instance).
        Document,
        /// The whole instance.
        Global,
        /// Elements under the same nearest ancestor anchored at the vertex.
        Within(Name),
    }

    /// A key declaration.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Key {
        /// Id, unique per vertex.
        pub id: Name,
        /// Parts in key order.
        pub parts: Vec<KeyPart>,
        /// Role.
        pub role: KeyRole,
        /// Scope.
        pub scope: KeyScope,
        /// Per-part equality. Empty means representation equality on every
        /// part and preserves the legacy byte layout.
        pub equality: Vec<KeyEquality>,
    }

    /// How a URI reference obtains its base.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum UriBase {
        /// The written reference must be absolute.
        Absolute,
        /// Read a base from a holder field.
        Field(Vec<SEdge>),
        /// Fold base and prefix directives before the holder.
        Positional {
            /// Vertex carrying a base directive.
            base: Option<Name>,
            /// Vertex carrying a prefix directive.
            prefix: Option<Name>,
        },
        /// Read a base on the nearest ancestor of this type.
        Ancestor {
            /// Ancestor vertex.
            ancestor: Name,
            /// Base-valued field.
            field: Vec<SEdge>,
            /// Joining rule.
            join: UriJoin,
        },
        /// Fold this field over the ancestor-or-self chain.
        Inherited {
            /// Base-valued field.
            field: Vec<SEdge>,
        },
    }

    /// Joining rule for ancestor-provided URI bases.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum UriJoin {
        /// RFC 3986 section 5.2.
        Rfc3986,
        /// FHIR REST service-base joining.
        FhirServiceBase,
    }

    /// Syntax of a path-valued reference.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum PathSyntax {
        /// RFC 6901 JSON Pointer.
        JsonPointer,
        /// `XPath`.
        XPath,
        /// URI/IRI reference with an explicit base source.
        Uri {
            /// Source of the base environment.
            base: UriBase,
        },
        /// XML IDREF.
        IdRef,
    }

    /// Variable-definition selection.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum VariableBinding {
        /// The first matching definition binds every occurrence.
        FirstDefines,
        /// The closest matching preceding definition binds the occurrence.
        NearestPreceding,
    }

    /// Text addressed by a standoff selector.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum RefTextSource {
        /// Text at a local field.
        Local(Vec<SEdge>),
        /// Bytes loaded under a key read at this field.
        External(Vec<SEdge>),
        /// Text reached through another resolved reference.
        Referenced {
            /// Reference declaration.
            reference: Name,
            /// Text path from its target.
            text: Vec<SEdge>,
            /// Optional external-text key path used when `text` is absent.
            external: Option<Vec<SEdge>>,
        },
        /// Text is the substring selected by another standoff reference.
        Span {
            /// Outer standoff reference.
            reference: Name,
        },
        /// External text named by a URI field.
        ExternalUri {
            /// URI-valued field.
            field: Vec<SEdge>,
            /// Source of the URI base.
            base: UriBase,
        },
    }

    /// Unit in which a standoff offset is written.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum OffsetUnit {
        /// UTF-8 bytes.
        Byte,
        /// Unicode scalar values.
        Utf8Char,
        /// UTF-16 code units.
        Utf16,
        /// Route-defined tokens.
        Token,
    }

    /// Interpretation of the written end coordinate.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum SpanEnd {
        /// An absolute end offset.
        Offset,
        /// A length added to the start.
        Length,
    }

    /// A data-dependent reference-selection guard.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct RefGuard {
        /// Guard-valued field.
        pub field: Vec<SEdge>,
        /// Required representation-equal value.
        pub equals: Value,
    }

    /// Null matching rule for composite foreign keys.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum NullMatch {
        /// Any null part makes the tuple absent.
        Simple,
        /// Two all-null tuples match.
        Full,
    }

    /// Optional scope traversal declared by a reference.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct ScopeVia {
        /// Vertex anchoring the holder's nearest syntactic scope ancestor.
        pub ancestor: Name,
        /// Declared single-valued references followed from that ancestor to
        /// the target key's `Within` owner.
        pub hops: Vec<Name>,
    }

    /// Where a reference reads its key.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum RefSource {
        /// Key values at these paths, aligned with the target key's parts.
        KeyValue {
            /// One path per target key part.
            parts: Vec<Vec<SEdge>>,
        },
        /// A path expression (resolved by a provider).
        Path {
            /// The field holding it.
            field: Vec<SEdge>,
            /// Path syntax and URI environment, when applicable.
            syntax: PathSyntax,
        },
        /// A variable name bound by its first occurrence.
        Variable {
            /// The field holding the name.
            field: Vec<SEdge>,
            /// The identifier key binding it.
            binder_key: Name,
            /// Definition-selection rule.
            binding: VariableBinding,
        },
        /// A standoff interval into text.
        Standoff {
            /// Text source.
            text: RefTextSource,
            /// Start coordinate field.
            start: Vec<SEdge>,
            /// End or length coordinate field.
            end: Vec<SEdge>,
            /// Coordinate unit.
            unit: OffsetUnit,
            /// Whether an offset end includes the indexed unit.
            end_inclusive: bool,
            /// Offset or length form.
            end_form: SpanEnd,
            /// Written coordinate origin.
            origin: u64,
            /// Optional ordered fragment edge.
            fragments: Option<SEdge>,
        },
        /// An identified occurrence reached inline from its holder.
        Inline {
            /// Field whose target is the occurrence.
            field: Vec<SEdge>,
        },
    }

    /// Cardinality of a reference.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum Cardinality {
        /// One target.
        One,
        /// A sequence of targets.
        Many,
    }

    /// What happens to an unresolved reference.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum OnUnresolved {
        /// It is an error (unless lenient).
        Error,
        /// It is recorded in the complement.
        Complement,
    }

    /// How a resolved reference is represented.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum ResolveMode {
        /// Keep the holder's field and add an edge.
        AddEdge,
        /// Remove the occurrence node and point its arc at the target.
        Redirect,
    }

    /// A reference declaration.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Reference {
        /// Id.
        pub id: Name,
        /// The holder vertex.
        pub holder: Name,
        /// The source of the key.
        pub source: RefSource,
        /// The target vertex.
        pub target_vertex: Name,
        /// The target key.
        pub target_key: Option<Name>,
        /// The resolved edge.
        pub edge: Option<SEdge>,
        /// Cardinality.
        pub cardinality: Cardinality,
        /// Whether no target is allowed.
        pub nullable: bool,
        /// Unresolved policy.
        pub on_unresolved: OnUnresolved,
        /// Mode.
        pub mode: ResolveMode,
        /// Optional target-selection guard.
        pub guard: Option<RefGuard>,
        /// Composite-null matching rule.
        pub null_match: NullMatch,
        /// Optional null sentinel vertex.
        pub null_vertex: Option<Name>,
        /// Optional path used to determine the scope bucket.
        pub scope_via: Option<ScopeVia>,
    }

    /// A key whose members range over several vertex types.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct TargetSet {
        /// `(vertex, key)` members.
        pub members: Vec<(Name, Name)>,
        /// Shared scope of the member keys.
        pub scope: KeyScope,
    }

    /// A schema.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct Schema {
        /// Vertex id to kind.
        pub vertices: BTreeMap<Name, Name>,
        /// Schema edges.
        pub edges: BTreeSet<SEdge>,
        /// Hyper-edges by id.
        pub hyper_edges: BTreeMap<Name, HyperEdge>,
        /// Constraints per vertex: `(sort, value)`.
        pub constraints: BTreeMap<Name, Vec<(Name, String)>>,
        /// Required edges per vertex.
        pub required: BTreeMap<Name, Vec<SEdge>>,
        /// Ordered edges with their position.
        pub orderings: BTreeMap<SEdge, u32>,
        /// Entry vertices, in order.
        pub entries: Vec<Name>,
        /// Keys per vertex, sorted by id.
        pub keys: BTreeMap<Name, Vec<Key>>,
        /// References by id.
        pub references: BTreeMap<Name, Reference>,
        /// Cross-vertex target sets by id.
        pub target_sets: BTreeMap<Name, TargetSet>,
    }

    impl Schema {
        /// Add a vertex of `kind`.
        #[must_use]
        pub fn vertex(mut self, id: &str, kind: &str) -> Self {
            self.vertices.insert(id.to_owned(), kind.to_owned());
            self
        }

        /// Add an edge.
        #[must_use]
        pub fn edge(mut self, src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Self {
            self.edges.insert(SEdge::new(src, tgt, kind, name));
            self
        }

        /// The edges from `a` to `b`.
        #[must_use]
        pub fn edges_between(&self, a: &str, b: &str) -> Vec<&SEdge> {
            self.edges
                .iter()
                .filter(|e| e.src == a && e.tgt == b)
                .collect()
        }
    }

    /// A declaration-level schema refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum DeclarationError {
        /// A key's non-empty equality vector has the wrong arity.
        EqualityArity {
            /// Vertex carrying the key.
            vertex: Name,
            /// Key id.
            key: Name,
            /// Number of parts.
            expected: usize,
            /// Number of equality entries.
            found: usize,
        },
        /// A key id occurs twice on one vertex.
        DuplicateKeyId {
            /// Vertex carrying the duplicate declaration.
            vertex: Name,
            /// Repeated key id.
            key: Name,
        },
        /// A target-set id is also a vertex id.
        TargetSetNameClash {
            /// Target-set id that also names a vertex.
            set: Name,
        },
        /// A target-set member is absent, repeated, or differs in arity,
        /// equality, or scope from the other members.
        TargetSetMemberMismatch {
            /// Target-set id.
            set: Name,
            /// Member vertex.
            vertex: Name,
            /// Member key.
            key: Name,
        },
        /// A sort key is not the key of targets of one ordered container edge.
        SortKeyNotOrdered {
            /// Sort-key id.
            key: Name,
            /// Vertex carrying the sort key.
            vertex: Name,
        },
        /// A non-resolvable key role was used by a reference.
        KeyNotResolvable {
            /// Reference that names the key.
            reference: Name,
            /// Non-resolvable key id.
            key: Name,
        },
        /// A target key or target set is absent.
        UnknownTarget {
            /// Reference whose target is absent.
            reference: Name,
        },
        /// The source tuple and target key have different arities.
        ReferenceArity {
            /// Reference whose tuple has the wrong arity.
            reference: Name,
            /// Target-key arity.
            expected: usize,
            /// Source tuple arity.
            found: usize,
        },
        /// Inline identification must redirect its occurrence.
        InlineRequiresRedirect {
            /// Inline reference declared with another mode.
            reference: Name,
        },
        /// Inline identification must range over a declared target set.
        InlineRequiresTargetSet {
            /// Inline reference naming a single vertex.
            reference: Name,
        },
        /// A length cannot also have an inclusive end.
        LengthInclusive {
            /// Standoff reference with both forms selected.
            reference: Name,
        },
        /// A fragmented reference must be `Many` and use an ordered edge from
        /// its holder.
        InvalidFragments {
            /// Fragmented reference with an invalid carrier.
            reference: Name,
        },
        /// Two references in one guard group select the same value.
        DuplicateGuard {
            /// References whose guard values collide.
            references: Vec<Name>,
        },
        /// A reference edge does not typecheck against its target declaration.
        EdgeMismatch {
            /// Reference with the bad edge.
            reference: Name,
        },
        /// A target-set reference carries a per-vertex target key.
        TargetSetHasKey {
            /// Reference with the redundant key.
            reference: Name,
        },
        /// A variable binder is absent or is not an identifier key.
        VariableBinderInvalid {
            /// Variable reference.
            reference: Name,
            /// Binder key id.
            key: Name,
        },
        /// One binder key is used by incompatible binding modes.
        BinderModeConflict {
            /// Binder key id.
            key: Name,
            /// References using conflicting modes.
            references: Vec<Name>,
        },
        /// A referenced/span text declaration is ill-typed or cyclic.
        TextReferenceInvalid {
            /// Standoff reference.
            reference: Name,
            /// Referenced text-producing reference.
            text_reference: Name,
        },
        /// A scope-via chain is ill-typed or cyclic.
        ScopeViaInvalid {
            /// Reference carrying the chain.
            reference: Name,
        },
    }

    fn key_equalities(key: &Key) -> Option<Vec<KeyEquality>> {
        if key.equality.is_empty() {
            Some(vec![KeyEquality::Representation; key.parts.len()])
        } else if key.equality.len() == key.parts.len() {
            Some(key.equality.clone())
        } else {
            None
        }
    }

    fn target_members<'a>(
        schema: &'a Schema,
        reference: &'a Reference,
    ) -> Vec<(&'a Name, &'a Key)> {
        if let Some(set) = schema.target_sets.get(&reference.target_vertex) {
            return set
                .members
                .iter()
                .filter_map(|(vertex, id)| {
                    schema.keys.get(vertex).and_then(|keys| {
                        keys.iter()
                            .find(|key| key.id == *id)
                            .map(|key| (vertex, key))
                    })
                })
                .collect();
        }
        let key_id = match &reference.source {
            RefSource::Variable { binder_key, .. } => Some(binder_key),
            _ => reference.target_key.as_ref(),
        };
        schema
            .keys
            .get(&reference.target_vertex)
            .into_iter()
            .flatten()
            .filter(|key| Some(&key.id) == key_id)
            .map(|key| (&reference.target_vertex, key))
            .collect()
    }

    /// Definition: Validate the schema-v2 declaration carrier, including key
    /// equality, sort-key, target-set, guarded, inline, and fragmented-source
    /// side conditions.
    ///
    /// Book anchors: `L-keys-and-registration-13`, `L-keys-and-registration-15`.
    /// # Errors
    ///
    /// Every declaration defect, in deterministic declaration order.
    #[allow(clippy::too_many_lines)]
    pub fn validate_declarations(schema: &Schema) -> Result<(), Vec<DeclarationError>> {
        let mut errors = Vec::new();
        for (vertex, keys) in &schema.keys {
            let mut ids = BTreeSet::new();
            for key in keys {
                if !ids.insert(&key.id) {
                    errors.push(DeclarationError::DuplicateKeyId {
                        vertex: vertex.clone(),
                        key: key.id.clone(),
                    });
                }
                if !key.equality.is_empty() && key.equality.len() != key.parts.len() {
                    errors.push(DeclarationError::EqualityArity {
                        vertex: vertex.clone(),
                        key: key.id.clone(),
                        expected: key.parts.len(),
                        found: key.equality.len(),
                    });
                }
                if key.role == KeyRole::SortKey {
                    let ordered = match &key.scope {
                        KeyScope::Within(container) => {
                            let carriers: Vec<_> = schema
                                .edges
                                .iter()
                                .filter(|edge| edge.src == *container && edge.tgt == *vertex)
                                .collect();
                            carriers.len() == 1 && schema.orderings.contains_key(carriers[0])
                        }
                        KeyScope::Document | KeyScope::Global => false,
                    };
                    if !ordered {
                        errors.push(DeclarationError::SortKeyNotOrdered {
                            key: key.id.clone(),
                            vertex: vertex.clone(),
                        });
                    }
                }
            }
        }
        for (id, set) in &schema.target_sets {
            if schema.vertices.contains_key(id) {
                errors.push(DeclarationError::TargetSetNameClash { set: id.clone() });
            }
            let mut members = BTreeSet::new();
            let mut signature: Option<(usize, Vec<KeyEquality>)> = None;
            for (vertex, key_id) in &set.members {
                let member = schema
                    .keys
                    .get(vertex)
                    .and_then(|keys| keys.iter().find(|key| key.id == *key_id));
                let valid = member.is_some_and(|key| {
                    let equality = key_equalities(key);
                    let this = equality.map(|equality| (key.parts.len(), equality));
                    let agrees = signature
                        .as_ref()
                        .is_none_or(|expected| this.as_ref() == Some(expected));
                    if signature.is_none() {
                        signature = this;
                    }
                    members.insert((vertex, key_id)) && key.scope == set.scope && agrees
                });
                if !valid {
                    errors.push(DeclarationError::TargetSetMemberMismatch {
                        set: id.clone(),
                        vertex: vertex.clone(),
                        key: key_id.clone(),
                    });
                }
            }
        }
        let mut guards: Vec<(&str, &RefSource, &[SEdge], &str, &Value)> = Vec::new();
        let mut binder_modes: BTreeMap<(&str, &str), Vec<(&str, VariableBinding)>> =
            BTreeMap::new();
        for reference in schema.references.values() {
            let targets = target_members(schema, reference);
            if targets.is_empty() {
                errors.push(DeclarationError::UnknownTarget {
                    reference: reference.id.clone(),
                });
                continue;
            }
            if targets
                .iter()
                .any(|(_, key)| matches!(key.role, KeyRole::SortKey | KeyRole::Index))
            {
                errors.push(DeclarationError::KeyNotResolvable {
                    reference: reference.id.clone(),
                    key: targets[0].1.id.clone(),
                });
            }
            let expected = targets[0].1.parts.len();
            if let Some(set) = schema.target_sets.get(&reference.target_vertex) {
                if reference.target_key.is_some() {
                    errors.push(DeclarationError::TargetSetHasKey {
                        reference: reference.id.clone(),
                    });
                }
                if let Some(edge) = &reference.edge {
                    let valid = edge.tgt == reference.target_vertex
                        && set.members.iter().all(|(vertex, _)| {
                            let mut member = edge.clone();
                            member.tgt.clone_from(vertex);
                            schema.edges.contains(&member)
                        });
                    if !valid {
                        errors.push(DeclarationError::EdgeMismatch {
                            reference: reference.id.clone(),
                        });
                    }
                }
            } else if reference.edge.as_ref().is_some_and(|edge| {
                edge.src != reference.holder
                    || edge.tgt != reference.target_vertex
                    || !schema.edges.contains(edge)
            }) {
                errors.push(DeclarationError::EdgeMismatch {
                    reference: reference.id.clone(),
                });
            }
            if let RefSource::KeyValue { parts } = &reference.source
                && parts.len() != expected
            {
                errors.push(DeclarationError::ReferenceArity {
                    reference: reference.id.clone(),
                    expected,
                    found: parts.len(),
                });
            }
            if matches!(reference.source, RefSource::Inline { .. })
                && reference.mode != ResolveMode::Redirect
            {
                errors.push(DeclarationError::InlineRequiresRedirect {
                    reference: reference.id.clone(),
                });
            }
            if matches!(reference.source, RefSource::Inline { .. })
                && !schema.target_sets.contains_key(&reference.target_vertex)
            {
                errors.push(DeclarationError::InlineRequiresTargetSet {
                    reference: reference.id.clone(),
                });
            }
            if let RefSource::Variable {
                binder_key,
                binding,
                ..
            } = &reference.source
            {
                if targets
                    .first()
                    .is_none_or(|(_, key)| key.id != *binder_key || key.role != KeyRole::Identifier)
                {
                    errors.push(DeclarationError::VariableBinderInvalid {
                        reference: reference.id.clone(),
                        key: binder_key.clone(),
                    });
                }
                binder_modes
                    .entry((reference.target_vertex.as_str(), binder_key.as_str()))
                    .or_default()
                    .push((reference.id.as_str(), *binding));
            }
            if let RefSource::Standoff {
                end_inclusive,
                end_form,
                fragments,
                ..
            } = &reference.source
            {
                if *end_form == SpanEnd::Length && *end_inclusive {
                    errors.push(DeclarationError::LengthInclusive {
                        reference: reference.id.clone(),
                    });
                }
                if let Some(edge) = fragments
                    && (reference.cardinality != Cardinality::Many
                        || edge.src != reference.holder
                        || !schema.orderings.contains_key(edge))
                {
                    errors.push(DeclarationError::InvalidFragments {
                        reference: reference.id.clone(),
                    });
                }
            }
            if let Some(guard) = &reference.guard {
                guards.push((
                    reference.holder.as_str(),
                    &reference.source,
                    &guard.field,
                    reference.id.as_str(),
                    &guard.equals,
                ));
            }
            validate_text_reference(schema, reference, &mut errors);
            validate_scope_via(schema, reference, &mut errors);
        }
        for (vertex_key, modes) in binder_modes {
            let distinct: BTreeSet<_> = modes.iter().map(|(_, mode)| *mode).collect();
            let targeted_elsewhere = schema.references.values().any(|reference| {
                let directly_targets = reference.target_vertex == vertex_key.0
                    && reference.target_key.as_deref() == Some(vertex_key.1);
                let targets_through_set = schema
                    .target_sets
                    .get(&reference.target_vertex)
                    .is_some_and(|set| {
                        set.members
                            .iter()
                            .any(|(vertex, key)| vertex == vertex_key.0 && key == vertex_key.1)
                    });
                (directly_targets || targets_through_set)
                    && !matches!(
                        &reference.source,
                        RefSource::Variable { binder_key, .. } if binder_key == vertex_key.1
                    )
            });
            if distinct.len() > 1 || targeted_elsewhere {
                errors.push(DeclarationError::BinderModeConflict {
                    key: vertex_key.1.to_owned(),
                    references: modes.into_iter().map(|(id, _)| id.to_owned()).collect(),
                });
            }
        }
        for (index, (holder, source, field, _, _)) in guards.iter().enumerate() {
            if guards[..index].iter().any(
                |(candidate_holder, candidate_source, candidate_field, _, _)| {
                    candidate_holder == holder
                        && *candidate_source == *source
                        && *candidate_field == *field
                },
            ) {
                continue;
            }
            let group: Vec<_> = guards
                .iter()
                .filter(
                    |(candidate_holder, candidate_source, candidate_field, _, _)| {
                        candidate_holder == holder
                            && *candidate_source == *source
                            && *candidate_field == *field
                    },
                )
                .map(|(_, _, _, id, value)| (*id, *value))
                .collect();
            let duplicates: Vec<Name> = group
                .iter()
                .filter(|(_, value)| {
                    group
                        .iter()
                        .filter(|(_, other)| {
                            crate::value::representation(value, other) == Some(true)
                        })
                        .count()
                        > 1
                })
                .map(|(id, _)| (*id).to_owned())
                .collect();
            if !duplicates.is_empty() {
                errors.push(DeclarationError::DuplicateGuard {
                    references: duplicates,
                });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_text_reference(
        schema: &Schema,
        reference: &Reference,
        errors: &mut Vec<DeclarationError>,
    ) {
        let RefSource::Standoff { text, .. } = &reference.source else {
            return;
        };
        let named = match text {
            RefTextSource::Referenced {
                reference: named, ..
            }
            | RefTextSource::Span { reference: named } => named,
            RefTextSource::Local(_)
            | RefTextSource::External(_)
            | RefTextSource::ExternalUri { .. } => return,
        };
        let valid = schema.references.get(named).is_some_and(|other| {
            if other.holder != reference.holder || other.cardinality != Cardinality::One {
                return false;
            }
            match text {
                RefTextSource::Referenced { .. } => matches!(
                    other.source,
                    RefSource::KeyValue { .. }
                        | RefSource::Path { .. }
                        | RefSource::Variable { .. }
                ),
                RefTextSource::Span { .. } => {
                    matches!(
                        other.source,
                        RefSource::Standoff {
                            fragments: None,
                            ..
                        }
                    )
                }
                RefTextSource::Local(_)
                | RefTextSource::External(_)
                | RefTextSource::ExternalUri { .. } => true,
            }
        }) && !text_chain_reaches(schema, named, &reference.id, &mut BTreeSet::new());
        if !valid {
            errors.push(DeclarationError::TextReferenceInvalid {
                reference: reference.id.clone(),
                text_reference: named.clone(),
            });
        }
    }

    fn text_chain_reaches(
        schema: &Schema,
        at: &str,
        target: &str,
        seen: &mut BTreeSet<Name>,
    ) -> bool {
        if at == target {
            return true;
        }
        if !seen.insert(at.to_owned()) {
            return false;
        }
        let Some(RefSource::Standoff { text, .. }) =
            schema.references.get(at).map(|reference| &reference.source)
        else {
            return false;
        };
        match text {
            RefTextSource::Referenced { reference, .. } | RefTextSource::Span { reference } => {
                text_chain_reaches(schema, reference, target, seen)
            }
            RefTextSource::Local(_)
            | RefTextSource::External(_)
            | RefTextSource::ExternalUri { .. } => false,
        }
    }

    fn validate_scope_via(
        schema: &Schema,
        reference: &Reference,
        errors: &mut Vec<DeclarationError>,
    ) {
        let Some(via) = &reference.scope_via else {
            return;
        };
        let expected_scope = schema
            .target_sets
            .get(&reference.target_vertex)
            .map(|set| &set.scope)
            .or_else(|| {
                target_members(schema, reference)
                    .first()
                    .map(|(_, key)| &key.scope)
            });
        let KeyScope::Within(scope_vertex) = expected_scope.unwrap_or(&KeyScope::Document) else {
            errors.push(DeclarationError::ScopeViaInvalid {
                reference: reference.id.clone(),
            });
            return;
        };
        let mut vertex = via.ancestor.as_str();
        let mut seen = BTreeSet::new();
        let mut valid = !via.hops.is_empty();
        for hop in &via.hops {
            let Some(declaration) = schema.references.get(hop) else {
                valid = false;
                break;
            };
            valid &= seen.insert(hop)
                && declaration.holder == vertex
                && declaration.cardinality == Cardinality::One;
            vertex = &declaration.target_vertex;
        }
        valid &= vertex == scope_vertex;
        if !valid {
            errors.push(DeclarationError::ScopeViaInvalid {
                reference: reference.id.clone(),
            });
        }
    }

    fn write_edges(writer: &mut Writer, edges: impl ExactSizeIterator<Item = SEdge>) {
        writer.len(edges.len());
        for edge in edges {
            writer.edge(&edge);
        }
    }

    fn write_key(writer: &mut Writer, key: &Key) {
        writer.str(&key.id);
        writer.len(key.parts.len());
        for part in &key.parts {
            match part {
                KeyPart::Path(path) => {
                    writer.tag(0);
                    write_edges(writer, path.iter().cloned());
                }
                KeyPart::Position { within } => {
                    writer.tag(1);
                    writer.edge(within);
                }
            }
        }
        let role = match key.role {
            KeyRole::Primary => 0,
            KeyRole::Unique => 1,
            KeyRole::Identifier => 2,
            KeyRole::SortKey => 3,
            KeyRole::Index => 4,
        };
        writer.tag(if key.equality.is_empty() {
            role
        } else {
            role | 0x80
        });
        match &key.scope {
            KeyScope::Document => writer.tag(0),
            KeyScope::Global => writer.tag(1),
            KeyScope::Within(vertex) => {
                writer.tag(2);
                writer.str(vertex);
            }
        }
        if !key.equality.is_empty() {
            writer.len(key.equality.len());
            for equality in &key.equality {
                write_key_equality(writer, equality);
            }
        }
    }

    fn write_key_equality(writer: &mut Writer, equality: &KeyEquality) {
        use crate::keys::{NumericPrimitive, SqlCollation, SqlPad};
        match equality {
            KeyEquality::Representation => writer.tag(0),
            KeyEquality::NumericValue => writer.tag(1),
            KeyEquality::NumericValueWithinKind(primitive) => {
                writer.tag(2);
                match primitive {
                    NumericPrimitive::Decimal => writer.tag(0),
                    NumericPrimitive::Float(width) => {
                        writer.tag(1);
                        writer.tag(*width);
                    }
                    NumericPrimitive::Any => writer.tag(2),
                }
            }
            KeyEquality::Sql { collation, pad } => {
                writer.tag(3);
                match collation {
                    SqlCollation::Binary => writer.tag(0),
                    SqlCollation::AsciiCaseInsensitive => writer.tag(1),
                    SqlCollation::Named(name) => {
                        writer.tag(2);
                        writer.str(name);
                    }
                }
                writer.tag(match pad {
                    SqlPad::NoPad => 0,
                    SqlPad::PadSpace => 1,
                });
            }
            KeyEquality::XmlNonCdata => writer.tag(4),
        }
    }

    fn write_uri_base(writer: &mut Writer, base: &UriBase) {
        match base {
            UriBase::Absolute => writer.tag(0),
            UriBase::Field(field) => {
                writer.tag(1);
                write_edges(writer, field.iter().cloned());
            }
            UriBase::Positional { base, prefix } => {
                writer.tag(2);
                writer.opt_name(base.as_deref());
                writer.opt_name(prefix.as_deref());
            }
            UriBase::Ancestor {
                ancestor,
                field,
                join,
            } => {
                writer.tag(3);
                writer.str(ancestor);
                write_edges(writer, field.iter().cloned());
                writer.tag(match join {
                    UriJoin::Rfc3986 => 0,
                    UriJoin::FhirServiceBase => 1,
                });
            }
            UriBase::Inherited { field } => {
                writer.tag(4);
                write_edges(writer, field.iter().cloned());
            }
        }
    }

    fn write_text_source(writer: &mut Writer, source: &RefTextSource) {
        match source {
            RefTextSource::Local(field) => {
                writer.tag(0);
                write_edges(writer, field.iter().cloned());
            }
            RefTextSource::External(field) => {
                writer.tag(1);
                write_edges(writer, field.iter().cloned());
            }
            RefTextSource::Referenced {
                reference,
                text,
                external,
            } => {
                writer.tag(2);
                writer.str(reference);
                write_edges(writer, text.iter().cloned());
                match external {
                    Some(path) => {
                        writer.tag(1);
                        write_edges(writer, path.iter().cloned());
                    }
                    None => writer.tag(0),
                }
            }
            RefTextSource::Span { reference } => {
                writer.tag(3);
                writer.str(reference);
            }
            RefTextSource::ExternalUri { field, base } => {
                writer.tag(4);
                write_edges(writer, field.iter().cloned());
                write_uri_base(writer, base);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn write_reference(writer: &mut Writer, reference: &Reference) {
        writer.str(&reference.id);
        writer.str(&reference.holder);
        match &reference.source {
            RefSource::KeyValue { parts } => {
                writer.tag(0);
                writer.len(parts.len());
                for path in parts {
                    write_edges(writer, path.iter().cloned());
                }
            }
            RefSource::Path { field, syntax } => {
                writer.tag(1);
                write_edges(writer, field.iter().cloned());
                match syntax {
                    PathSyntax::JsonPointer => writer.tag(0),
                    PathSyntax::XPath => writer.tag(1),
                    PathSyntax::Uri { base } => {
                        writer.tag(2);
                        write_uri_base(writer, base);
                    }
                    PathSyntax::IdRef => writer.tag(3),
                }
            }
            RefSource::Variable {
                field,
                binder_key,
                binding,
            } => {
                writer.tag(2);
                write_edges(writer, field.iter().cloned());
                writer.str(binder_key);
                writer.tag(match binding {
                    VariableBinding::FirstDefines => 0,
                    VariableBinding::NearestPreceding => 1,
                });
            }
            RefSource::Standoff {
                text,
                start,
                end,
                unit,
                end_inclusive,
                end_form,
                origin,
                fragments,
            } => {
                writer.tag(3);
                write_text_source(writer, text);
                write_edges(writer, start.iter().cloned());
                write_edges(writer, end.iter().cloned());
                writer.tag(match unit {
                    OffsetUnit::Byte => 0,
                    OffsetUnit::Utf8Char => 1,
                    OffsetUnit::Utf16 => 2,
                    OffsetUnit::Token => 3,
                });
                writer.tag(u8::from(*end_inclusive));
                writer.tag(match end_form {
                    SpanEnd::Offset => 0,
                    SpanEnd::Length => 1,
                });
                writer.u64(*origin);
                match fragments {
                    None => writer.tag(0),
                    Some(edge) => {
                        writer.tag(1);
                        writer.edge(edge);
                    }
                }
            }
            RefSource::Inline { field } => {
                writer.tag(4);
                write_edges(writer, field.iter().cloned());
            }
        }
        writer.str(&reference.target_vertex);
        writer.opt_name(reference.target_key.as_deref());
        match &reference.edge {
            None => writer.tag(0),
            Some(edge) => {
                writer.tag(1);
                writer.edge(edge);
            }
        }
        writer.tag(match reference.cardinality {
            Cardinality::One => 0,
            Cardinality::Many => 1,
        });
        writer.tag(u8::from(reference.nullable));
        writer.tag(match reference.on_unresolved {
            OnUnresolved::Error => 0,
            OnUnresolved::Complement => 1,
        });
        writer.tag(match reference.mode {
            ResolveMode::AddEdge => 0,
            ResolveMode::Redirect => 1,
        });
        let has_extensions = reference.guard.is_some()
            || reference.null_match != NullMatch::Simple
            || reference.null_vertex.is_some()
            || reference.scope_via.is_some();
        if has_extensions {
            writer.tag(0xff);
            match &reference.guard {
                None => writer.tag(0),
                Some(guard) => {
                    writer.tag(1);
                    write_edges(writer, guard.field.iter().cloned());
                    writer.raw(&crate::canonical::value_bytes(&guard.equals).unwrap_or_default());
                }
            }
            writer.tag(match reference.null_match {
                NullMatch::Simple => 0,
                NullMatch::Full => 1,
            });
            writer.opt_name(reference.null_vertex.as_deref());
            match &reference.scope_via {
                None => writer.tag(0),
                Some(via) => {
                    writer.tag(1);
                    writer.str(&via.ancestor);
                    writer.len(via.hops.len());
                    for hop in &via.hops {
                        writer.str(hop);
                    }
                }
            }
        }
    }

    /// Definition: Canonical bytes for the finite reference schema.
    ///
    /// The layout writes every field in declaration order, every ordered map
    /// or set in key order, every sequence behind a `u64` length, and every sum
    /// behind a one-byte discriminant. Strings and edges use [`Writer`]'s
    /// canonical primitive layout.
    ///
    /// Book anchors: `L-keys-and-registration-3`, `L-keys-and-registration-10`, `L-keys-and-registration-15`.
    #[must_use]
    pub fn canonical_bytes(schema: &Schema) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.len(schema.vertices.len());
        for (vertex, kind) in &schema.vertices {
            writer.str(vertex);
            writer.str(kind);
        }
        write_edges(&mut writer, schema.edges.iter().cloned());
        writer.len(schema.hyper_edges.len());
        for (id, hyper) in &schema.hyper_edges {
            writer.str(id);
            writer.str(&hyper.kind);
            writer.len(hyper.signature.len());
            for (label, vertex) in &hyper.signature {
                writer.str(label);
                writer.str(vertex);
            }
            writer.str(&hyper.parent_label);
        }
        writer.len(schema.constraints.len());
        for (vertex, constraints) in &schema.constraints {
            writer.str(vertex);
            writer.len(constraints.len());
            for (sort, value) in constraints {
                writer.str(sort);
                writer.str(value);
            }
        }
        writer.len(schema.required.len());
        for (vertex, required) in &schema.required {
            writer.str(vertex);
            write_edges(&mut writer, required.iter().cloned());
        }
        writer.len(schema.orderings.len());
        for (edge, position) in &schema.orderings {
            writer.edge(edge);
            writer.u32(*position);
        }
        writer.len(schema.entries.len());
        for entry in &schema.entries {
            writer.str(entry);
        }
        writer.len(schema.keys.len());
        for (vertex, keys) in &schema.keys {
            writer.str(vertex);
            writer.len(keys.len());
            for key in keys {
                write_key(&mut writer, key);
            }
        }
        writer.len(schema.references.len());
        for (id, reference) in &schema.references {
            writer.str(id);
            write_reference(&mut writer, reference);
        }
        if !schema.target_sets.is_empty() {
            writer.tag(0xff);
            writer.len(schema.target_sets.len());
            for (id, set) in &schema.target_sets {
                writer.str(id);
                writer.len(set.members.len());
                for (vertex, key) in &set.members {
                    writer.str(vertex);
                    writer.str(key);
                }
                match &set.scope {
                    KeyScope::Document => writer.tag(0),
                    KeyScope::Global => writer.tag(1),
                    KeyScope::Within(vertex) => {
                        writer.tag(2);
                        writer.str(vertex);
                    }
                }
            }
        }
        writer.bytes
    }

    /// Compatibility of the finite reference schema under a change.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Change {
        /// No semantic declaration changed.
        Identical,
        /// Existing instances remain valid, though the inverse may not hold.
        BackwardCompatible,
        /// Some previously valid instance may be rejected.
        Breaking,
    }

    fn key_is_tightened(old: &Key, new: &Key) -> bool {
        let role_rank = |role: &KeyRole| match role {
            KeyRole::Primary => 2,
            KeyRole::Unique | KeyRole::Identifier => 1,
            KeyRole::SortKey | KeyRole::Index => 0,
        };
        let scope_rank = |scope: &KeyScope| match scope {
            KeyScope::Global => 3,
            KeyScope::Document => 2,
            KeyScope::Within(_) => 1,
        };
        old.parts != new.parts
            || old.equality != new.equality
            || role_rank(&new.role) > role_rank(&old.role)
            || (role_rank(&new.role) == role_rank(&old.role) && new.role != old.role)
            || scope_rank(&new.scope) > scope_rank(&old.scope)
            || (scope_rank(&new.scope) == scope_rank(&old.scope) && new.scope != old.scope)
    }

    fn reference_is_tightened(old: &Reference, new: &Reference) -> bool {
        (old.cardinality == Cardinality::Many && new.cardinality == Cardinality::One)
            || (old.nullable && !new.nullable)
            || (old.on_unresolved == OnUnresolved::Complement
                && new.on_unresolved == OnUnresolved::Error)
            || old.holder != new.holder
            || old.source != new.source
            || old.target_vertex != new.target_vertex
            || old.target_key != new.target_key
            || old.edge != new.edge
            || old.mode != new.mode
            || old.id != new.id
            || old.guard != new.guard
            || old.null_match != new.null_match
            || old.null_vertex != new.null_vertex
            || old.scope_via != new.scope_via
    }

    fn keyed_keys(schema: &Schema) -> BTreeMap<(&str, &str), &Key> {
        schema
            .keys
            .iter()
            .flat_map(|(vertex, keys)| {
                keys.iter()
                    .map(move |key| ((vertex.as_str(), key.id.as_str()), key))
            })
            .collect()
    }

    /// Definition: Classify key and reference changes by backward compatibility.
    ///
    /// Adding a key on an existing vertex is breaking and removing one is a
    /// relaxation. A changed key is breaking exactly when it adds or changes
    /// parts, strengthens its role, widens its scope, or moves between two
    /// distinct `Within` scopes. Reference changes follow the corresponding
    /// cardinality, nullability and unresolved-value rules. Changes elsewhere
    /// in this finite carrier fail closed as breaking.
    ///
    /// Book anchors: `L-keys-and-registration-4`.
    #[must_use]
    pub fn classify_change(before: &Schema, after: &Schema) -> Change {
        if before == after {
            return Change::Identical;
        }
        let before_keys = keyed_keys(before);
        let after_keys = keyed_keys(after);
        let added_key_breaks = after_keys
            .keys()
            .any(|key| !before_keys.contains_key(key) && before.vertices.contains_key(key.0));
        let changed_key_breaks = before_keys.iter().any(|(key, old)| {
            after_keys
                .get(key)
                .is_some_and(|new| key_is_tightened(old, new))
        });
        let added_reference_breaks = after.references.iter().any(|(id, reference)| {
            !before.references.contains_key(id)
                && before.vertices.contains_key(&reference.holder)
                && (!reference.nullable || reference.on_unresolved == OnUnresolved::Error)
        });
        let changed_reference_breaks = before.references.iter().any(|(id, old)| {
            after
                .references
                .get(id)
                .is_some_and(|new| reference_is_tightened(old, new))
        });
        let declarations_only = before.vertices == after.vertices
            && before.edges == after.edges
            && before.hyper_edges == after.hyper_edges
            && before.constraints == after.constraints
            && before.required == after.required
            && before.orderings == after.orderings
            && before.entries == after.entries;
        let added_target_set_breaks = after.target_sets.iter().any(|(id, set)| {
            before.target_sets.get(id).is_none_or(|old| {
                old.scope != set.scope
                    || set
                        .members
                        .iter()
                        .any(|member| !old.members.contains(member))
            })
        });
        if added_key_breaks
            || changed_key_breaks
            || added_reference_breaks
            || changed_reference_breaks
            || added_target_set_breaks
            || !declarations_only
        {
            Change::Breaking
        } else {
            Change::BackwardCompatible
        }
    }

    /// The schema and the two closed theories/recipes registered for one
    /// protocol.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Registration {
        /// Finite schema declaration carrier.
        pub schema: Schema,
        /// Closed schema theory.
        pub schema_theory: Theory,
        /// Closed instance theory for the protocol's shape.
        pub instance_theory: Theory,
        /// Replayable schema-theory recipe.
        pub schema_composition: CompositionSpec,
        /// Replayable instance-theory recipe.
        pub instance_composition: CompositionSpec,
    }

    /// A typed registration refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RegistrationConflict {
        /// Schema-v2 declarations are not well formed.
        InvalidDeclarations(Vec<DeclarationError>),
        /// A registered theory is open or ill typed.
        InvalidTheory {
            /// Whether this is the instance theory.
            instance: bool,
            /// Exact signature defects.
            defects: Vec<TheoryDefect>,
        },
        /// A composition recipe cannot be replayed.
        Replay {
            /// Whether this is the instance recipe.
            instance: bool,
            /// Replay refusal.
            error: RecomposeError,
        },
        /// A recipe replayed to bytes different from its declared result.
        ReplayMismatch {
            /// Whether this is the instance recipe.
            instance: bool,
        },
        /// The name already denotes different canonical registration bytes.
        NameConflict {
            /// The already-registered name.
            name: Name,
        },
    }

    fn registration_bytes(registration: &Registration) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.raw(&canonical_bytes(&registration.schema));
        writer.raw(&canonical_theory_bytes(&registration.schema_theory));
        writer.raw(&canonical_theory_bytes(&registration.instance_theory));
        writer.raw(&canonical_composition_spec_bytes(
            &registration.schema_composition,
        ));
        writer.raw(&canonical_composition_spec_bytes(
            &registration.instance_composition,
        ));
        writer.bytes
    }

    /// Definition: register one complete Q.6--Q.8 protocol carrier.
    ///
    /// Both theories are closed and well typed, both versioned recipes replay
    /// to the declared canonical theory bytes, and equal-name replay is
    /// accepted exactly when the complete canonical registration bytes agree.
    ///
    /// # Errors
    ///
    /// The first [`RegistrationConflict`] in declaration, closed-theory,
    /// replay and same-name order.
    ///
    /// Book anchors: `L-keys-and-registration-5`, `L-keys-and-registration-6`, `L-keys-and-registration-7`.
    #[allow(clippy::result_large_err)]
    pub fn register(
        registry: &mut BTreeMap<Name, Registration>,
        theory_registry: &BTreeMap<Name, Theory>,
        name: &str,
        registration: Registration,
    ) -> Result<(), RegistrationConflict> {
        validate_declarations(&registration.schema)
            .map_err(RegistrationConflict::InvalidDeclarations)?;
        for (instance, theory) in [
            (false, &registration.schema_theory),
            (true, &registration.instance_theory),
        ] {
            if let Err(defects) = typecheck_signature(theory) {
                return Err(RegistrationConflict::InvalidTheory { instance, defects });
            }
        }
        for (instance, recipe, declared) in [
            (
                false,
                &registration.schema_composition,
                &registration.schema_theory,
            ),
            (
                true,
                &registration.instance_composition,
                &registration.instance_theory,
            ),
        ] {
            let replayed = recompose(recipe, theory_registry)
                .map_err(|error| RegistrationConflict::Replay { instance, error })?;
            if canonical_theory_bytes(&replayed) != canonical_theory_bytes(declared) {
                return Err(RegistrationConflict::ReplayMismatch { instance });
            }
        }
        if let Some(existing) = registry.get(name) {
            return if registration_bytes(existing) == registration_bytes(&registration) {
                Ok(())
            } else {
                Err(RegistrationConflict::NameConflict {
                    name: name.to_owned(),
                })
            };
        }
        registry.insert(name.to_owned(), registration);
        Ok(())
    }
}

pub use self::keys::{
    annotated_format, buckets, index_and_null_match, key_equivalence, key_order, key_value,
    lexical_space, nearest_ancestor, path_value, representation_refines, sort_order, target_set,
    tuple_equivalence, xml_noncdata,
};
pub use self::schema::{
    Cardinality, Key, KeyPart, KeyRole, KeyScope, NullMatch, OffsetUnit, OnUnresolved, PathSyntax,
    RefGuard, RefSource, RefTextSource, Reference, ResolveMode, Schema, ScopeVia, SpanEnd,
    TargetSet, UriBase, UriJoin, VariableBinding, canonical_bytes, classify_change, register,
    validate_declarations,
};
