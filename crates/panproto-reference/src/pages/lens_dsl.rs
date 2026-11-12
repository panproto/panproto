//! Complete reference operations assigned to `lens-dsl`.
//!
//! A lens document is the reference crate's own JSON value
//! ([`crate::json::Value`]); Nickel evaluation and YAML reading stay outside
//! this model, which starts from the value they produce. The module checks
//! the document grammar, elaborates each schema-free body into the ordered
//! stages the `lens-dsl` correctness page defines, composes parts vertically
//! and horizontally, resolves named references, and names the document path
//! of every refusal in the order the page states. It is transcribed from the
//! page and shares no code with production, which reads the document through
//! serde types and builds `panproto-lens` protolenses.
//!
//! Two judgements belong to other components and are parameters here
//! ([`Oracles`]): whether a string is an expression of the expression
//! language, and whether a declared coercion class survives the sampled
//! round-trip laws. An `auto` or `from_diff` body is inside the domain of the
//! schema-free compilation, which refuses it; its generation from schemas is
//! not modelled. A referenced document that the grammar refuses is answered
//! with [`Outcome::OutsideDomain`], since production resolves references
//! against documents it has already read.

use std::collections::BTreeMap;

use crate::dsl_path::Path;
use crate::json::Value;

// ─── Outcomes and refusals ──────────────────────────────────────────────

/// What a refused value was expected to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    /// An object.
    Object,
    /// An array.
    Array,
    /// A string.
    String,
    /// A boolean.
    Boolean,
    /// A number.
    Number,
    /// A whole number from zero to `2^32 - 1`.
    Natural,
    /// A string or an object.
    StringOrObject,
}

/// Why the grammar refuses a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeProblem {
    /// The value has the wrong JSON type.
    WrongType(Expected),
    /// An object carries a member the grammar does not name.
    UnknownField,
    /// A required member is absent; the path is the object's.
    MissingField(String),
    /// A string, or a member of a variant object, names no variant.
    UnknownVariant(String),
    /// A variant object has no member.
    NoVariant,
    /// A variant object names several variants, in grammar order.
    SeveralVariants(Vec<String>),
    /// The document carries no body key.
    NoBody,
    /// The document carries several body keys, in grammar order.
    SeveralBodies(Vec<String>),
}

/// Why a document is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The grammar refuses a value.
    Shape(ShapeProblem),
    /// A string is not an expression of the expression language.
    ExprParse,
    /// An equation term is not in the term grammar.
    TermParse,
    /// A default holds a whole number past `i64`, or contradicts its kind.
    DefaultValue,
    /// A rule's name template is malformed.
    RuleCompile,
    /// A part of a horizontal composition compiles to the empty chain.
    EmptyHorizontalPart,
    /// A symmetric leg carries a value-level step.
    ValueStepInSymmetricLeg,
    /// A reference names no document.
    UnresolvedRef(String),
    /// A reference reopens a document along the chain of references: the
    /// chain, the reopened identifier last.
    RefCycle(Vec<String>),
    /// A referenced document is refused, at this path of it.
    InRef(String, Box<(Path, Self)>),
    /// A directed composition references a document with a symmetric body.
    SymmetricRef(String),
    /// An `auto` or `from_diff` body needs schemas to compile.
    AutoRequiresSchemas,
    /// `invertible: true` on a lens holding a lossy element.
    NotInvertible,
    /// A declared coercion class fails its sampled round-trip laws.
    CoercionNotHonest,
}

/// The result of the reference on one document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<T> {
    /// The document is accepted with this result.
    Accepted(T),
    /// The document is refused at this path.
    Refused(Path, Refusal),
    /// The document uses a construction outside the reference's domain.
    OutsideDomain(&'static str),
}

/// A computation of the reference that stops on a refusal or outside the
/// domain; refusal paths are relative to the value it was given.
type Step<T> = Result<T, Stop>;

#[derive(Debug)]
enum Stop {
    Refused(Box<(Path, Refusal)>),
    Outside(&'static str),
}

impl Stop {
    fn within(self, prefix: &Path) -> Self {
        match self {
            Self::Refused(refused) => {
                let (path, refusal) = *refused;
                Self::Refused(Box::new((prefix.join(&path), refusal)))
            }
            outside @ Self::Outside(_) => outside,
        }
    }
}

fn refuse<T>(path: Path, refusal: Refusal) -> Step<T> {
    Err(Stop::Refused(Box::new((path, refusal))))
}

fn located<T>(step: Step<T>, prefix: &Path) -> Step<T> {
    step.map_err(|stop| stop.within(prefix))
}

fn outcome<T>(step: Step<T>) -> Outcome<T> {
    match step {
        Ok(value) => Outcome::Accepted(value),
        Err(Stop::Refused(refused)) => Outcome::Refused(refused.0, refused.1),
        Err(Stop::Outside(why)) => Outcome::OutsideDomain(why),
    }
}

// ─── The document grammar ───────────────────────────────────────────────

/// The grammar of one value.
enum G {
    Any,
    Str,
    Bool,
    Number,
    Natural,
    Nullable(Box<Self>),
    List(Box<Self>),
    Map(Box<Self>),
    Record(Vec<F>),
    Tagged(&'static str, Vec<(&'static str, Vec<F>)>),
    Keyed(Vec<(&'static str, Self)>),
    Choice(&'static [&'static str]),
    StrOr(Box<Self>),
    /// A step list; built when it is checked, since a `scoped` step holds one.
    Steps,
}

/// A record field: name, whether required, grammar.
struct F(&'static str, bool, G);

const fn req(name: &'static str, g: G) -> F {
    F(name, true, g)
}

const fn opt(name: &'static str, g: G) -> F {
    F(name, false, g)
}

fn list(g: G) -> G {
    G::List(Box::new(g))
}

fn map(g: G) -> G {
    G::Map(Box::new(g))
}

/// The value kinds a coercion or a directed equation may name.
pub const VALUE_KINDS: &[&str] = &[
    "boolean", "bool", "integer", "int", "float", "number", "string", "str", "bytes", "token",
    "null", "any",
];

const CLASSES: &[&str] = &["iso", "retraction", "projection", "opaque"];

fn rename() -> G {
    G::Record(vec![req("old", G::Str), req("new", G::Str)])
}

fn value_level(subject: &'static str) -> G {
    G::Record(vec![
        req(subject, G::Str),
        req("expr", G::Str),
        opt("inverse", G::Str),
        opt("coercion", G::Choice(CLASSES)),
    ])
}

/// The step variants, in grammar order.
pub const STEP_VARIANTS: [&str; 19] = [
    "remove_field",
    "rename_field",
    "add_field",
    "apply_expr",
    "compute_field",
    "hoist_field",
    "nest_field",
    "scoped",
    "pullback",
    "coerce_sort",
    "merge_sorts",
    "add_sort",
    "drop_sort",
    "rename_sort",
    "add_op",
    "drop_op",
    "rename_op",
    "add_equation",
    "drop_equation",
];

fn step_variant(name: &str) -> G {
    match name {
        "remove_field" | "drop_sort" | "drop_op" | "drop_equation" => G::Str,
        "rename_field" | "rename_sort" | "rename_op" => rename(),
        "add_field" => G::Record(vec![
            req("name", G::Str),
            req("kind", G::Str),
            opt("default", G::Any),
            opt("expr", G::Str),
        ]),
        "apply_expr" => value_level("field"),
        "compute_field" => value_level("target"),
        "hoist_field" => G::Record(vec![
            req("parent", G::Str),
            req("intermediate", G::Str),
            req("child", G::Str),
        ]),
        "nest_field" => G::Record(vec![
            req("parent", G::Str),
            req("child", G::Str),
            req("intermediate", G::Str),
            req("intermediate_kind", G::Str),
            req("edge_kind", G::Str),
            opt("old_edge_name", G::Str),
            opt("parent_to_intermediate", G::Str),
            opt("intermediate_to_child", G::Str),
        ]),
        "scoped" => G::Record(vec![req("focus", G::Str), req("inner", G::Steps)]),
        "pullback" => G::Record(vec![
            req("name", G::Str),
            req("domain", G::Str),
            req("codomain", G::Str),
            opt("sort_map", map(G::Str)),
            opt("op_map", map(G::Str)),
        ]),
        "coerce_sort" => G::Record(vec![
            req("sort", G::Str),
            opt("source_kind", G::Choice(VALUE_KINDS)),
            req("target_kind", G::Choice(VALUE_KINDS)),
            req("expr", G::Str),
            opt("inverse", G::Str),
            req("coercion", G::Choice(CLASSES)),
        ]),
        "merge_sorts" => G::Record(vec![
            req("sort_a", G::Str),
            req("sort_b", G::Str),
            req("merged", G::Str),
            req("expr", G::Str),
        ]),
        "add_sort" => G::Record(vec![
            req("name", G::Str),
            req("kind", G::Str),
            opt("default", G::Any),
        ]),
        "add_op" => G::Record(vec![
            req("name", G::Str),
            req("src", G::Str),
            req("tgt", G::Str),
            req("kind", G::Str),
        ]),
        _ => G::Record(vec![
            req("name", G::Str),
            req("lhs", G::Str),
            req("rhs", G::Str),
        ]),
    }
}

fn step_grammar() -> G {
    G::Keyed(
        STEP_VARIANTS
            .iter()
            .map(|name| (*name, step_variant(name)))
            .collect(),
    )
}

fn rule_grammar() -> G {
    let template = G::Record(vec![req("template", G::Str)]);
    let number = || vec![req("value", G::Number)];
    let text = || vec![req("value", G::Str)];
    let attr_op = G::Tagged(
        "op",
        vec![
            ("add", number()),
            ("subtract", number()),
            ("multiply", number()),
            ("prefix", text()),
            ("suffix", text()),
            ("negate", vec![]),
            ("to-string", vec![]),
            ("to-number", vec![]),
            ("to-boolean", vec![]),
        ],
    );
    let replacement = G::Record(vec![
        opt("name", G::StrOr(Box::new(template))),
        opt("rename_attrs", map(G::Str)),
        opt("add_attrs", map(G::Any)),
        opt("drop_attrs", list(G::Str)),
        opt("keep_attrs", list(G::Str)),
        opt("map_attr_value", map(attr_op)),
    ]);
    G::Record(vec![
        req("pattern", G::Record(vec![req("name", G::Str)])),
        opt("replace", G::Nullable(Box::new(replacement))),
    ])
}

fn compose_grammar() -> G {
    let lens_ref = G::Keyed(vec![
        ("ref", G::Str),
        ("inline", G::Record(vec![req("steps", G::Steps)])),
    ]);
    G::Record(vec![
        req("mode", G::Choice(&["vertical", "horizontal"])),
        req("lenses", list(lens_ref)),
    ])
}

fn auto_grammar() -> G {
    let predicate = G::Tagged(
        "kind",
        vec![
            ("same_edge_name", vec![]),
            ("similar_name", vec![req("threshold", G::Number)]),
            ("same_kind", vec![]),
        ],
    );
    let constraint = G::Tagged(
        "type",
        vec![
            ("scope", vec![req("under", G::Str), req("targets", G::Str)]),
            ("exclude_targets", vec![req("vertices", list(G::Str))]),
            ("exclude_sources", vec![req("vertices", list(G::Str))]),
            (
                "prefer",
                vec![req("predicate", predicate), req("weight", G::Number)],
            ),
        ],
    );
    let hints = G::Record(vec![
        opt("anchors", map(G::Str)),
        opt("constraints", list(constraint)),
        opt(
            "stringency",
            G::Choice(&["strict", "balanced", "lenient", "exploratory"]),
        ),
        opt("alias_clusters", list(list(G::Str))),
    ]);
    G::Record(vec![
        opt("quality_threshold", G::Number),
        opt("enable_overlap", G::Bool),
        opt("max_results", G::Natural),
        opt("hints", hints),
    ])
}

fn directed_grammar() -> G {
    G::Record(vec![
        req("name", G::Str),
        req("lhs", G::Str),
        req("rhs", G::Str),
        req("impl", G::Str),
        opt("inverse", G::Str),
        opt("source_kind", G::Choice(VALUE_KINDS)),
        opt("target_kind", G::Choice(VALUE_KINDS)),
        opt("coercion", G::Choice(CLASSES)),
    ])
}

/// The body keys in grammar order.
pub const BODY_KEYS: [&str; 6] = [
    "steps",
    "rules",
    "compose",
    "auto",
    "from_diff",
    "symmetric",
];

/// The field table of the body selected by `key`, the body member first.
fn body_fields(key: &str) -> Vec<F> {
    let body = match key {
        "steps" => G::Steps,
        "rules" => list(rule_grammar()),
        "compose" => compose_grammar(),
        "auto" => auto_grammar(),
        "from_diff" => G::Record(vec![]),
        _ => G::Record(vec![
            req("left", G::Steps),
            req("right", G::Steps),
            opt("focus", G::Str),
        ]),
    };
    let mut fields = vec![req(BODY_KEYS[body_index(key)], body)];
    if key == "rules" {
        fields.push(opt("passthrough", G::Choice(&["keep", "drop"])));
    }
    if key != "symmetric" {
        fields.push(opt("directed_equations", list(directed_grammar())));
    }
    fields.push(opt("invertible", G::Bool));
    fields.push(opt("extensions", map(G::Any)));
    fields
}

fn body_index(key: &str) -> usize {
    BODY_KEYS.iter().position(|k| *k == key).unwrap_or(0)
}

/// The members of an object, the last occurrence of each name kept, in the
/// order of their names' bytes.
fn members(value: &Value) -> Option<BTreeMap<&str, &Value>> {
    match value {
        Value::Object(entries) => Some(
            entries
                .iter()
                .map(|(name, member)| (name.as_str(), member))
                .collect(),
        ),
        _ => None,
    }
}

/// A grammar check that stops at the first refused value.
type Shaped<T> = Result<T, (Path, ShapeProblem)>;

impl From<(Path, ShapeProblem)> for Stop {
    fn from((path, problem): (Path, ShapeProblem)) -> Self {
        Self::Refused(Box::new((path, Refusal::Shape(problem))))
    }
}

fn wrong<T>(path: &Path, expected: Expected) -> Shaped<T> {
    Err((path.clone(), ShapeProblem::WrongType(expected)))
}

fn is_natural(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit()) && text.parse::<u32>().is_ok()
}

fn check(value: &Value, g: &G, path: &Path) -> Shaped<()> {
    match (g, value) {
        (G::Any, _)
        | (G::Str, Value::String(_))
        | (G::Bool, Value::Bool(_))
        | (G::Number, Value::Number(_))
        | (G::Nullable(_), Value::Null)
        | (G::StrOr(_), Value::String(_)) => Ok(()),
        (G::Natural, Value::Number(text)) if is_natural(text) => Ok(()),
        (G::Str, _) => wrong(path, Expected::String),
        (G::Bool, _) => wrong(path, Expected::Boolean),
        (G::Number, _) => wrong(path, Expected::Number),
        (G::Natural, _) => wrong(path, Expected::Natural),
        (G::Nullable(inner), _) => check(value, inner, path),
        (G::List(element), _) => check_list(value, element, path),
        (G::Steps, _) => check_list(value, &step_grammar(), path),
        (G::Map(member), _) => {
            let Some(entries) = members(value) else {
                return wrong(path, Expected::Object);
            };
            for (name, entry) in entries {
                check(entry, member, &path.key(name))?;
            }
            Ok(())
        }
        (G::Record(fields), _) => check_record(value, &[], fields, path),
        (G::Tagged(tag, variants), _) => check_tagged(value, tag, variants, path),
        (G::Keyed(variants), _) => check_keyed(value, variants, path),
        (G::Choice(choices), Value::String(found)) if choices.contains(&found.as_str()) => Ok(()),
        (G::Choice(_), Value::String(found)) => {
            Err((path.clone(), ShapeProblem::UnknownVariant(found.clone())))
        }
        (G::Choice(_), _) => wrong(path, Expected::String),
        (G::StrOr(inner), Value::Object(_)) => check(value, inner, path),
        (G::StrOr(_), _) => wrong(path, Expected::StringOrObject),
    }
}

fn check_list(value: &Value, element: &G, path: &Path) -> Shaped<()> {
    let Value::Array(items) = value else {
        return wrong(path, Expected::Array);
    };
    for (index, item) in items.iter().enumerate() {
        check(item, element, &path.index(index))?;
    }
    Ok(())
}

fn check_tagged(
    value: &Value,
    tag: &'static str,
    variants: &[(&'static str, Vec<F>)],
    path: &Path,
) -> Shaped<()> {
    let Some(entries) = members(value) else {
        return wrong(path, Expected::Object);
    };
    let Some(found) = entries.get(tag) else {
        return Err((path.clone(), ShapeProblem::MissingField(tag.to_owned())));
    };
    let Value::String(found) = found else {
        return wrong(&path.key(tag), Expected::String);
    };
    let Some((_, fields)) = variants.iter().find(|(name, _)| name == found) else {
        return Err((path.key(tag), ShapeProblem::UnknownVariant(found.clone())));
    };
    check_record(value, &[req(tag, G::Str)], fields, path)
}

fn check_keyed(value: &Value, variants: &[(&'static str, G)], path: &Path) -> Shaped<()> {
    let Some(entries) = members(value) else {
        return wrong(path, Expected::Object);
    };
    if let Some(unknown) = entries
        .keys()
        .find(|name| !variants.iter().any(|(v, _)| v == *name))
    {
        return Err((
            path.key(unknown),
            ShapeProblem::UnknownVariant((*unknown).to_owned()),
        ));
    }
    let present: Vec<&(&str, G)> = variants
        .iter()
        .filter(|(name, _)| entries.contains_key(name))
        .collect();
    match present.as_slice() {
        [] => Err((path.clone(), ShapeProblem::NoVariant)),
        [(name, g)] => entries
            .get(name)
            .map_or(Ok(()), |member| check(member, g, &path.key(name))),
        several => Err((
            path.clone(),
            ShapeProblem::SeveralVariants(several.iter().map(|(n, _)| (*n).to_owned()).collect()),
        )),
    }
}

fn check_record(value: &Value, leading: &[F], fields: &[F], path: &Path) -> Shaped<()> {
    let Some(entries) = members(value) else {
        return wrong(path, Expected::Object);
    };
    let named = |name: &str| leading.iter().chain(fields).any(|f| f.0 == name);
    if let Some(unknown) = entries.keys().find(|name| !named(name)) {
        return Err((path.key(unknown), ShapeProblem::UnknownField));
    }
    for F(name, required, g) in leading.iter().chain(fields) {
        match entries.get(name) {
            Some(member) => check(member, g, &path.key(name))?,
            None if *required => {
                return Err((path.clone(), ShapeProblem::MissingField((*name).to_owned())));
            }
            None => {}
        }
    }
    Ok(())
}

/// Definition: the document grammar.
///
/// The value must be an object with exactly one body key; the body key
/// selects the field table, which `id`, `description`, `source` and
/// `target` lead. Returns the body key.
///
/// # Errors
///
/// The path and problem of the first value refused, in the order the page
/// states: the root's type, the body count, then an object's least unknown
/// member name, each field in table order, a map's members in name order,
/// an array's elements in order; a variant object's least member naming no
/// variant, then its variant count.
///
/// Book anchor: `L-lens-dsl-1`.
pub fn check_document(value: &Value) -> Result<&'static str, (Path, ShapeProblem)> {
    let root = Path::root();
    let Some(entries) = members(value) else {
        return wrong(&root, Expected::Object);
    };
    let present: Vec<&'static str> = BODY_KEYS
        .iter()
        .copied()
        .filter(|key| entries.contains_key(key))
        .collect();
    let body = match present.as_slice() {
        [] => return Err((root, ShapeProblem::NoBody)),
        [one] => *one,
        several => {
            return Err((
                root,
                ShapeProblem::SeveralBodies(several.iter().map(|k| (*k).to_owned()).collect()),
            ));
        }
    };
    let header = [
        req("id", G::Str),
        opt("description", G::Str),
        req("source", G::Str),
        req("target", G::Str),
    ];
    check_record(value, &header, &body_fields(body), &root)?;
    Ok(body)
}

// ─── Reading checked values ─────────────────────────────────────────────

fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    members(value).and_then(|m| m.get(key).copied())
}

fn text(value: &Value, key: &str) -> String {
    opt_text(value, key).unwrap_or_default()
}

fn opt_text(value: &Value, key: &str) -> Option<String> {
    match get(value, key) {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    match get(value, key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    items(value, key)
        .iter()
        .filter_map(|item| match item {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn string_map(value: &Value, key: &str) -> BTreeMap<String, String> {
    get(value, key)
        .and_then(members)
        .map(|entries| {
            entries
                .into_iter()
                .filter_map(|(k, v)| match v {
                    Value::String(s) => Some((k.to_owned(), s.clone())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The single member of a variant object: its name and value.
fn variant(value: &Value) -> Option<(&str, &Value)> {
    members(value).and_then(|m| m.into_iter().next())
}

// ─── Numbers, defaults and kinds ────────────────────────────────────────

/// A JSON number as a reader of 64-bit numbers takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    /// A whole number that `i64` holds.
    Int(i64),
    /// A whole number past `i64::MAX` that `u64` holds.
    Wide,
    /// Any other number, as the nearest double.
    Float(f64),
}

/// Definition: how a JSON number is read.
///
/// A number without fraction or exponent is a whole number: a positive one
/// is `Int` when `i64` holds it and `Wide` when only `u64` does; a negative
/// one is `Int` when its magnitude is from 1 to `2^63`; `-0` and any whole
/// number past those bounds are read as the nearest double, as is every
/// number with a fraction or an exponent.
#[must_use]
pub fn number(text: &str) -> Number {
    let float = || Number::Float(text.parse::<f64>().unwrap_or(f64::NAN));
    if text.contains(['.', 'e', 'E']) {
        return float();
    }
    if let Some(magnitude) = text.strip_prefix('-') {
        return match magnitude.parse::<u64>() {
            Ok(m) if m > 0 && m <= 1 << 63 => {
                Number::Int(i64::try_from(-i128::from(m)).unwrap_or(i64::MIN))
            }
            _ => float(),
        };
    }
    match text.parse::<u64>() {
        Ok(m) => i64::try_from(m).map_or(Number::Wide, Number::Int),
        Err(_) => float(),
    }
}

/// A default value carried into the instance algebra.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Datum {
    /// `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A double, by its bits.
    Float(u64),
    /// A string.
    Str(String),
    /// An array.
    List(Vec<Self>),
    /// An object, the last occurrence of each member kept.
    Record(BTreeMap<String, Self>),
}

/// The datum a JSON value carries; `None` when a whole number past `i64`
/// occurs in it.
fn datum(value: &Value) -> Option<Datum> {
    Some(match value {
        Value::Null => Datum::Null,
        Value::Bool(b) => Datum::Bool(*b),
        Value::String(s) => Datum::Str(s.clone()),
        Value::Number(n) => match number(n) {
            Number::Int(i) => Datum::Int(i),
            Number::Wide => return None,
            Number::Float(f) => Datum::Float(f.to_bits()),
        },
        Value::Array(items) => Datum::List(items.iter().map(datum).collect::<Option<_>>()?),
        Value::Object(_) => Datum::Record(
            members(value)
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| Some((k.to_owned(), datum(v)?)))
                .collect::<Option<_>>()?,
        ),
    })
}

/// A value kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Booleans (`boolean`, `bool`).
    Bool,
    /// Whole numbers (`integer`, `int`).
    Int,
    /// Doubles (`float`, `number`).
    Float,
    /// Strings (`string`, `str`).
    Str,
    /// Bytes.
    Bytes,
    /// Tokens.
    Token,
    /// Null.
    Null,
    /// Any other name.
    Any,
}

/// The kind a name denotes.
#[must_use]
pub fn kind(name: &str) -> Kind {
    match name {
        "boolean" | "bool" => Kind::Bool,
        "integer" | "int" => Kind::Int,
        "float" | "number" => Kind::Float,
        "string" | "str" => Kind::Str,
        "bytes" => Kind::Bytes,
        "token" => Kind::Token,
        "null" => Kind::Null,
        _ => Kind::Any,
    }
}

/// Whether `datum` is a default of the kind named `name`: `null` always is,
/// a whole number is a double default, and an unknown kind constrains
/// nothing.
fn default_has_kind(datum: &Datum, name: &str) -> bool {
    if *datum == Datum::Null {
        return true;
    }
    match kind(name) {
        Kind::Bool => matches!(datum, Datum::Bool(_)),
        Kind::Int => matches!(datum, Datum::Int(_)),
        Kind::Float => matches!(datum, Datum::Float(_) | Datum::Int(_)),
        Kind::Str | Kind::Bytes | Kind::Token => matches!(datum, Datum::Str(_)),
        Kind::Null => false,
        Kind::Any => true,
    }
}

/// The default a step's `default` member names: refused at that member when
/// it holds a whole number past `i64` or contradicts `kind_name`.
fn default_of(spec: &Value, kind_name: &str) -> Step<Datum> {
    let here = Path::root().key("default");
    let Some(datum) = datum(get(spec, "default").unwrap_or(&Value::Null)) else {
        return refuse(here, Refusal::DefaultValue);
    };
    if default_has_kind(&datum, kind_name) {
        Ok(datum)
    } else {
        refuse(here, Refusal::DefaultValue)
    }
}

/// A coercion class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Lossless.
    Iso,
    /// A retraction.
    Retraction,
    /// A projection.
    Projection,
    /// Opaque.
    Opaque,
}

fn class(name: Option<&str>, absent: Class) -> Class {
    match name {
        Some("iso") => Class::Iso,
        Some("retraction") => Class::Retraction,
        Some("projection") => Class::Projection,
        Some("opaque") => Class::Opaque,
        _ => absent,
    }
}

// ─── Terms and expressions ──────────────────────────────────────────────

/// A first-order term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    /// A variable.
    Var(String),
    /// An operation applied to arguments.
    App(String, Vec<Self>),
}

struct TermReader {
    chars: Vec<char>,
    at: usize,
}

impl TermReader {
    fn blank(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    fn ident(&mut self) -> Option<String> {
        let start = self.at;
        match self.chars.get(self.at) {
            Some(c) if c.is_ascii_alphabetic() || *c == '_' => self.at += 1,
            _ => return None,
        }
        while self
            .chars
            .get(self.at)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
        {
            self.at += 1;
        }
        Some(self.chars[start..self.at].iter().collect())
    }

    fn eat(&mut self, c: char) -> bool {
        self.blank();
        let found = self.chars.get(self.at) == Some(&c);
        if found {
            self.at += 1;
        }
        found
    }

    fn term(&mut self) -> Option<Term> {
        self.blank();
        let name = self.ident()?;
        if !self.eat('(') {
            return Some(Term::Var(name));
        }
        let mut args = Vec::new();
        if self.eat(')') {
            return Some(Term::App(name, args));
        }
        loop {
            args.push(self.term()?);
            if self.eat(')') {
                return Some(Term::App(name, args));
            }
            if !self.eat(',') {
                return None;
            }
        }
    }
}

/// Definition: the term grammar.
///
/// `term ::= ident | ident '(' (term (',' term)*)? ')'` with
/// `ident ::= [A-Za-z_][A-Za-z0-9_]*`, white space allowed between tokens.
/// `None` when `text` is not one term.
#[must_use]
pub fn parse_term(text: &str) -> Option<Term> {
    let mut reader = TermReader {
        chars: text.chars().collect(),
        at: 0,
    };
    let term = reader.term()?;
    reader.blank();
    (reader.at == reader.chars.len()).then_some(term)
}

fn term_at(spec: &Value, key: &str) -> Step<Term> {
    parse_term(&text(spec, key))
        .map_or_else(|| refuse(Path::root().key(key), Refusal::TermParse), Ok)
}

/// Whether a string is an expression of the expression language.
pub type ExpressionCheck<'a> = &'a dyn Fn(&str) -> bool;

/// Whether a `coerce_sort` declaration survives its class's sampled laws:
/// the forward and inverse expressions, the class and the source kind.
pub type CoercionCheck<'a> = &'a dyn Fn(&str, Option<&str>, Class, Kind) -> bool;

/// Whether a directed equation survives its class's sampled laws.
pub type DirectedCheck<'a> = &'a dyn Fn(&Elementary) -> bool;

/// The judgements other components make, taken as parameters.
pub struct Oracles<'a> {
    /// The expression grammar.
    pub expression: ExpressionCheck<'a>,
    /// The coercion honesty check of a `coerce_sort` step.
    pub coercion: CoercionCheck<'a>,
    /// The coercion honesty check of a directed equation.
    pub directed: DirectedCheck<'a>,
}

fn expr_at(spec: &Value, key: &str, oracles: &Oracles<'_>) -> Step<String> {
    let found = text(spec, key);
    if (oracles.expression)(&found) {
        Ok(found)
    } else {
        refuse(Path::root().key(key), Refusal::ExprParse)
    }
}

fn opt_expr_at(spec: &Value, key: &str, oracles: &Oracles<'_>) -> Step<Option<String>> {
    match get(spec, key) {
        Some(_) => expr_at(spec, key, oracles).map(Some),
        None => Ok(None),
    }
}

/// A segment of a rule's name template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplatePart {
    /// Literal text.
    Lit(String),
    /// An interpolated variable, rendered as a string.
    Var(String),
}

/// The operand of a numeric attribute operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    /// A whole number.
    Int(i64),
    /// A double, by its bits.
    Float(u64),
}

/// An attribute-value operation of a rule's `map_attr_value`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttrOp {
    /// `field + n`.
    Add(Operand),
    /// `field - n`.
    Subtract(Operand),
    /// `field * n`.
    Multiply(Operand),
    /// `s ++ field`.
    Prefix(String),
    /// `field ++ s`.
    Suffix(String),
    /// `not field`.
    Negate,
    /// The field rendered as a string.
    ToString,
    /// The field read as a whole number.
    ToNumber,
    /// The field's truth: a non-empty string, a non-zero number, else
    /// itself.
    ToBoolean,
}

/// An expression a compiled transform carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprSpec {
    /// Expression-language text, as the document wrote it.
    Text(String),
    /// A name template: the concatenation of its parts, left to right; the
    /// empty string when there are none.
    Template(Vec<TemplatePart>),
    /// An attribute-value operation on the named field.
    AttrOp(String, AttrOp),
}

// ─── The compiled lens ──────────────────────────────────────────────────

/// A value-level transform at an anchor vertex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldTransform {
    /// Rewrite the field `key` in place.
    Apply {
        /// The field.
        key: String,
        /// The forward expression.
        expr: ExprSpec,
        /// The inverse expression.
        inverse: Option<ExprSpec>,
        /// The declared class.
        class: Class,
    },
    /// Compute the field `target`.
    Compute {
        /// The field.
        target: String,
        /// The forward expression.
        expr: ExprSpec,
        /// The inverse expression.
        inverse: Option<ExprSpec>,
        /// The declared class.
        class: Class,
    },
    /// Keep only these fields.
    Keep(Vec<String>),
}

impl FieldTransform {
    /// Whether the transform loses nothing: an `Iso` class.
    #[must_use]
    pub const fn lossless(&self) -> bool {
        matches!(
            self,
            Self::Apply {
                class: Class::Iso,
                ..
            } | Self::Compute {
                class: Class::Iso,
                ..
            }
        )
    }
}

/// One elementary protolens: a schema transform with the residue it keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Elementary {
    /// Add a sort of a vertex kind, with the default its values take.
    AddSort {
        /// The sort.
        sort: String,
        /// The vertex kind.
        kind: String,
        /// The default, absent for a structural vertex.
        default: Option<Datum>,
    },
    /// Add an edge.
    AddEdge {
        /// Source vertex.
        src: String,
        /// Target vertex.
        tgt: String,
        /// Label.
        name: String,
        /// Edge kind.
        kind: String,
    },
    /// Drop the edges `src -> tgt` with this label (any label when absent).
    DropEdge {
        /// Source vertex.
        src: String,
        /// Target vertex.
        tgt: String,
        /// Label.
        name: Option<String>,
    },
    /// Drop a sort.
    DropSort(String),
    /// Rename a sort.
    RenameSort {
        /// Old name.
        old: String,
        /// New name.
        new: String,
    },
    /// Add a unary operation `src -> tgt` of an edge kind.
    AddOp {
        /// Operation name.
        name: String,
        /// Input sort.
        src: String,
        /// Output sort.
        tgt: String,
        /// Edge kind.
        kind: String,
    },
    /// Drop an operation.
    DropOp(String),
    /// Rename an operation.
    RenameOp {
        /// Old name.
        old: String,
        /// New name.
        new: String,
    },
    /// Relabel the edges `src -> tgt` labelled `old`.
    RenameEdgeName {
        /// Source vertex.
        src: String,
        /// Target vertex.
        tgt: String,
        /// Old label.
        old: String,
        /// New label.
        new: String,
    },
    /// Add an equation.
    AddEquation {
        /// Name.
        name: String,
        /// Left side.
        lhs: Term,
        /// Right side.
        rhs: Term,
    },
    /// Drop an equation.
    DropEquation(String),
    /// Pull back along a theory morphism.
    Pullback {
        /// Morphism name.
        name: String,
        /// Domain theory.
        domain: String,
        /// Codomain theory.
        codomain: String,
        /// Sort images.
        sort_map: BTreeMap<String, String>,
        /// Operation images.
        op_map: BTreeMap<String, String>,
    },
    /// Coerce a sort's values to a kind.
    CoerceSort {
        /// The sort.
        sort: String,
        /// The target kind.
        target_kind: Kind,
        /// Forward expression.
        expr: String,
        /// Inverse expression.
        inverse: Option<String>,
        /// Declared class.
        class: Class,
    },
    /// Merge two sorts into one.
    MergeSorts {
        /// First sort.
        sort_a: String,
        /// Second sort.
        sort_b: String,
        /// Merged sort.
        merged: String,
        /// Merging expression.
        expr: String,
    },
    /// An oriented rewrite with its implementation.
    DirectedEq {
        /// Name.
        name: String,
        /// Left side.
        lhs: Term,
        /// Right side.
        rhs: Term,
        /// Forward implementation.
        implementation: String,
        /// Inverse implementation.
        inverse: Option<String>,
        /// Source kind.
        source_kind: Option<Kind>,
        /// Target kind.
        target_kind: Option<Kind>,
        /// Declared class.
        class: Class,
    },
}

impl Elementary {
    /// Whether the protolens keeps no residue: a rename, an equation added
    /// or dropped, a pullback, an `Iso` coercion, or a directed equation
    /// with an inverse.
    #[must_use]
    pub const fn lossless(&self) -> bool {
        match self {
            Self::RenameSort { .. }
            | Self::RenameOp { .. }
            | Self::RenameEdgeName { .. }
            | Self::AddEquation { .. }
            | Self::DropEquation(_)
            | Self::Pullback { .. } => true,
            Self::CoerceSort { class, .. } => matches!(class, Class::Iso),
            Self::DirectedEq { inverse, .. } => inverse.is_some(),
            Self::AddSort { .. }
            | Self::AddEdge { .. }
            | Self::DropEdge { .. }
            | Self::DropSort(_)
            | Self::AddOp { .. }
            | Self::DropOp(_)
            | Self::MergeSorts { .. } => false,
        }
    }
}

/// A protolens of a compiled chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proto {
    /// One elementary protolens.
    Elementary(Elementary),
    /// A protolens applied at every element of the focus vertex.
    Scoped {
        /// The focus vertex.
        focus: String,
        /// The protolens applied there.
        inner: Box<Self>,
    },
    /// The fusion of a chain of two or more protolenses into one.
    Fused(Vec<Self>),
    /// The horizontal composite `η * θ`.
    Horizontal(Box<Self>, Box<Self>),
}

impl Proto {
    /// Whether the protolens keeps no residue: an elementary one by
    /// [`Elementary::lossless`], a scoped one when its inner one is, a
    /// fusion or a horizontal composite when every part is.
    #[must_use]
    pub fn lossless(&self) -> bool {
        match self {
            Self::Elementary(e) => e.lossless(),
            Self::Scoped { inner, .. } => inner.lossless(),
            Self::Fused(chain) => chain.iter().all(Self::lossless),
            Self::Horizontal(eta, theta) => eta.lossless() && theta.lossless(),
        }
    }
}

/// The transforms of a stage, by anchor vertex, in order.
pub type Transforms = BTreeMap<String, Vec<FieldTransform>>;

/// One stage of a compiled lens: a chain and the value-level transforms
/// the same concrete migration runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stage {
    /// The protolens chain.
    pub chain: Vec<Proto>,
    /// The value-level transforms.
    pub transforms: Transforms,
}

/// A compiled lens document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompiledLens {
    /// The ordered stages (a symmetric body's left leg).
    pub stages: Vec<Stage>,
    /// A symmetric body's left and right chains.
    pub symmetric: Option<(Vec<Proto>, Vec<Proto>)>,
}

/// The chain of a stage sequence: its stages' chains in order.
#[must_use]
pub fn chain_of(stages: &[Stage]) -> Vec<Proto> {
    stages
        .iter()
        .flat_map(|s| s.chain.iter().cloned())
        .collect()
}

/// The transforms of a stage sequence, gathered by anchor in stage order.
#[must_use]
pub fn transforms_of(stages: &[Stage]) -> Transforms {
    let mut out = Transforms::new();
    for stage in stages {
        for (anchor, entries) in &stage.transforms {
            out.entry(anchor.clone())
                .or_default()
                .extend(entries.iter().cloned());
        }
    }
    out
}

fn lossless(stages: &[Stage]) -> bool {
    stages.iter().all(|stage| {
        stage.chain.iter().all(Proto::lossless)
            && stage
                .transforms
                .values()
                .flatten()
                .all(FieldTransform::lossless)
    })
}

fn chain_stage(chain: Vec<Elementary>) -> Stage {
    Stage {
        chain: chain.into_iter().map(Proto::Elementary).collect(),
        transforms: Transforms::new(),
    }
}

fn transform_stage(anchor: &str, transform: FieldTransform) -> Stage {
    Stage {
        chain: Vec::new(),
        transforms: Transforms::from([(anchor.to_owned(), vec![transform])]),
    }
}

// ─── Steps ──────────────────────────────────────────────────────────────

fn field(body: &str, name: &str) -> String {
    format!("{body}.{name}")
}

fn remove_field(body: &str, name: &str) -> Stage {
    chain_stage(vec![Elementary::DropSort(field(body, name))])
}

fn rename_field(body: &str, old: &str, new: &str) -> Stage {
    chain_stage(vec![Elementary::RenameEdgeName {
        src: body.to_owned(),
        tgt: field(body, old),
        old: old.to_owned(),
        new: new.to_owned(),
    }])
}

fn add_field(body: &str, name: &str, kind_name: &str, default: Datum) -> Stage {
    chain_stage(vec![
        Elementary::AddSort {
            sort: field(body, name),
            kind: kind_name.to_owned(),
            default: Some(default),
        },
        Elementary::AddEdge {
            src: body.to_owned(),
            tgt: field(body, name),
            name: name.to_owned(),
            kind: "prop".to_owned(),
        },
    ])
}

/// A value-level step's transform: `apply_expr` or `compute_field`.
fn value_step(spec: &Value, apply: bool, body: &str, oracles: &Oracles<'_>) -> Step<Stage> {
    let expr = ExprSpec::Text(expr_at(spec, "expr", oracles)?);
    let inverse = opt_expr_at(spec, "inverse", oracles)?.map(ExprSpec::Text);
    let class = class(opt_text(spec, "coercion").as_deref(), Class::Projection);
    let transform = if apply {
        FieldTransform::Apply {
            key: text(spec, "field"),
            expr,
            inverse,
            class,
        }
    } else {
        FieldTransform::Compute {
            target: text(spec, "target"),
            expr,
            inverse,
            class,
        }
    };
    Ok(transform_stage(body, transform))
}

fn add_field_step(spec: &Value, body: &str, oracles: &Oracles<'_>) -> Step<Stage> {
    let name = text(spec, "name");
    let kind_name = text(spec, "kind");
    let default = default_of(spec, &kind_name)?;
    let expr = opt_expr_at(spec, "expr", oracles)?;
    let mut stage = add_field(body, &name, &kind_name, default);
    if let Some(expr) = expr {
        stage.transforms.insert(
            body.to_owned(),
            vec![FieldTransform::Compute {
                target: name,
                expr: ExprSpec::Text(expr),
                inverse: None,
                class: Class::Projection,
            }],
        );
    }
    Ok(stage)
}

fn hoist_field(spec: &Value) -> Stage {
    let child = text(spec, "child");
    chain_stage(vec![
        Elementary::AddOp {
            name: child.clone(),
            src: text(spec, "parent"),
            tgt: child.clone(),
            kind: child,
        },
        Elementary::DropSort(text(spec, "intermediate")),
    ])
}

fn nest_field(spec: &Value) -> Stage {
    let nonempty = |key: &str, otherwise: String| {
        opt_text(spec, key)
            .filter(|s| !s.is_empty())
            .unwrap_or(otherwise)
    };
    let parent = text(spec, "parent");
    let child = text(spec, "child");
    let intermediate = text(spec, "intermediate");
    let edge_kind = text(spec, "edge_kind");
    chain_stage(vec![
        Elementary::AddSort {
            sort: intermediate.clone(),
            kind: text(spec, "intermediate_kind"),
            default: None,
        },
        Elementary::AddEdge {
            src: parent.clone(),
            tgt: intermediate.clone(),
            name: nonempty("parent_to_intermediate", intermediate.clone()),
            kind: edge_kind.clone(),
        },
        Elementary::AddEdge {
            src: intermediate.clone(),
            tgt: child.clone(),
            name: nonempty("intermediate_to_child", child.clone()),
            kind: edge_kind,
        },
        Elementary::DropEdge {
            src: parent,
            tgt: child,
            name: opt_text(spec, "old_edge_name").filter(|s| !s.is_empty()),
        },
    ])
}

fn coerce_sort(spec: &Value, oracles: &Oracles<'_>) -> Step<Elementary> {
    let expr = expr_at(spec, "expr", oracles)?;
    let inverse = opt_expr_at(spec, "inverse", oracles)?;
    let declared = class(opt_text(spec, "coercion").as_deref(), Class::Iso);
    let source_kind = opt_text(spec, "source_kind").map_or(Kind::Any, |k| kind(&k));
    if !(oracles.coercion)(&expr, inverse.as_deref(), declared, source_kind) {
        return refuse(Path::root(), Refusal::CoercionNotHonest);
    }
    Ok(Elementary::CoerceSort {
        sort: text(spec, "sort"),
        target_kind: kind(&text(spec, "target_kind")),
        expr,
        inverse,
        class: declared,
    })
}

/// The elementary protolenses of a step that compiles to a chain alone.
fn theory_step(name: &str, spec: &Value, oracles: &Oracles<'_>) -> Step<Vec<Elementary>> {
    let str_of = || match spec {
        Value::String(s) => s.clone(),
        _ => String::new(),
    };
    let renamed = || (text(spec, "old"), text(spec, "new"));
    Ok(vec![match name {
        "pullback" => Elementary::Pullback {
            name: text(spec, "name"),
            domain: text(spec, "domain"),
            codomain: text(spec, "codomain"),
            sort_map: string_map(spec, "sort_map"),
            op_map: string_map(spec, "op_map"),
        },
        "coerce_sort" => coerce_sort(spec, oracles)?,
        "merge_sorts" => {
            let expr = expr_at(spec, "expr", oracles)?;
            Elementary::MergeSorts {
                sort_a: text(spec, "sort_a"),
                sort_b: text(spec, "sort_b"),
                merged: text(spec, "merged"),
                expr,
            }
        }
        "add_sort" => {
            let kind_name = text(spec, "kind");
            let default = default_of(spec, &kind_name)?;
            Elementary::AddSort {
                sort: text(spec, "name"),
                kind: kind_name,
                default: Some(default),
            }
        }
        "drop_sort" => Elementary::DropSort(str_of()),
        "rename_sort" => {
            let (old, new) = renamed();
            Elementary::RenameSort { old, new }
        }
        "add_op" => Elementary::AddOp {
            name: text(spec, "name"),
            src: text(spec, "src"),
            tgt: text(spec, "tgt"),
            kind: text(spec, "kind"),
        },
        "drop_op" => Elementary::DropOp(str_of()),
        "rename_op" => {
            let (old, new) = renamed();
            Elementary::RenameOp { old, new }
        }
        "add_equation" => {
            let lhs = term_at(spec, "lhs")?;
            let rhs = term_at(spec, "rhs")?;
            Elementary::AddEquation {
                name: text(spec, "name"),
                lhs,
                rhs,
            }
        }
        _ => Elementary::DropEquation(str_of()),
    }])
}

/// Fuse a non-empty chain into one protolens: a single protolens is
/// itself, a longer chain its fusion. Every protolens a document compiles
/// to has the identity as its source (a horizontal composite's is `Id ∘ Id`),
/// so adjacent protolenses always compose and fusion never refuses.
fn fuse(mut chain: Vec<Proto>) -> Proto {
    if chain.len() == 1 {
        chain.remove(0)
    } else {
        Proto::Fused(chain)
    }
}

fn scoped(spec: &Value, oracles: &Oracles<'_>) -> Step<Vec<Stage>> {
    let focus = text(spec, "focus");
    let mut stages = Vec::new();
    for (k, inner) in items(spec, "inner").iter().enumerate() {
        let here = Path::root().key("inner").index(k);
        for stage in located(compile_step(inner, &focus, oracles), &here)? {
            let chain = if stage.chain.is_empty() {
                Vec::new()
            } else {
                vec![Proto::Scoped {
                    focus: focus.clone(),
                    inner: Box::new(fuse(stage.chain)),
                }]
            };
            stages.push(Stage {
                chain,
                transforms: stage.transforms,
            });
        }
    }
    Ok(stages)
}

/// Definition: the compilation of one step at a body vertex.
///
/// A field step acts at `body`: `remove_field n` drops the sort `body.n`,
/// `rename_field` relabels the edge from `body` to `body.old`, `add_field`
/// adds the sort `body.n` with its default and a `prop` edge labelled `n`
/// (and a projection computing `n` when it has an `expr`), and `apply_expr`
/// and `compute_field` are value-level transforms at `body`. `hoist_field`
/// adds the operation `child : parent -> child` and drops the intermediate;
/// `nest_field` adds the structural intermediate, the two edges through it
/// and drops the direct edge. `scoped` compiles each inner step at the
/// focus and maps each of its stages' fused chains over the focus. Every
/// other step is its one elementary protolens. Each step gives one stage,
/// `scoped` one per inner stage.
///
/// # Errors
///
/// Refusals located relative to the step object, under its variant name:
/// an expression at `expr` or `inverse`, a default at `default`, a term at
/// `lhs` or `rhs`, a coercion honesty failure at the variant, and an inner
/// step's refusal at `inner/k`.
///
/// Book anchor: `L-lens-dsl-2`.
pub fn step_stages(step: &Value, body: &str, oracles: &Oracles<'_>) -> Outcome<Vec<Stage>> {
    outcome(compile_step(step, body, oracles))
}

fn compile_step(step: &Value, body: &str, oracles: &Oracles<'_>) -> Step<Vec<Stage>> {
    let Some((name, spec)) = variant(step) else {
        return Ok(Vec::new());
    };
    let here = Path::root().key(name);
    let stage = match name {
        "remove_field" => match spec {
            Value::String(n) => remove_field(body, n),
            _ => Stage::default(),
        },
        "rename_field" => rename_field(body, &text(spec, "old"), &text(spec, "new")),
        "add_field" => located(add_field_step(spec, body, oracles), &here)?,
        "apply_expr" => located(value_step(spec, true, body, oracles), &here)?,
        "compute_field" => located(value_step(spec, false, body, oracles), &here)?,
        "hoist_field" => hoist_field(spec),
        "nest_field" => nest_field(spec),
        "scoped" => return located(scoped(spec, oracles), &here),
        _ => chain_stage(located(theory_step(name, spec, oracles), &here)?),
    };
    Ok(vec![stage])
}

fn compile_steps(steps: &[Value], body: &str, oracles: &Oracles<'_>) -> Step<Vec<Stage>> {
    let mut stages = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        stages.extend(located(
            compile_step(step, body, oracles),
            &Path::root().index(i),
        )?);
    }
    Ok(stages)
}

// ─── Rules ──────────────────────────────────────────────────────────────

/// Read a name template into its parts.
///
/// # Errors
///
/// `None` for an interpolation that is never closed or names no variable.
fn template(text: &str) -> Option<Vec<TemplatePart>> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            parts.push(TemplatePart::Lit(rest[..open].to_owned()));
        }
        let close = open + rest[open..].find('}')?;
        let var = rest[open + 1..close].trim();
        if var.is_empty() {
            return None;
        }
        parts.push(TemplatePart::Var(var.to_owned()));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        parts.push(TemplatePart::Lit(rest.to_owned()));
    }
    Some(parts)
}

fn operand(value: Option<&Value>) -> Operand {
    match value {
        Some(Value::Number(n)) => match number(n) {
            Number::Int(i) => Operand::Int(i),
            Number::Wide => Operand::Float(n.parse::<f64>().unwrap_or(f64::NAN).to_bits()),
            Number::Float(f) => Operand::Float(f.to_bits()),
        },
        _ => Operand::Int(0),
    }
}

fn attr_op(spec: &Value) -> AttrOp {
    let value = get(spec, "value");
    let string = || match value {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    match text(spec, "op").as_str() {
        "add" => AttrOp::Add(operand(value)),
        "subtract" => AttrOp::Subtract(operand(value)),
        "multiply" => AttrOp::Multiply(operand(value)),
        "prefix" => AttrOp::Prefix(string()),
        "suffix" => AttrOp::Suffix(string()),
        "negate" => AttrOp::Negate,
        "to-string" => AttrOp::ToString,
        "to-number" => AttrOp::ToNumber,
        _ => AttrOp::ToBoolean,
    }
}

/// The kind an `add_attrs` value gives its field.
fn attr_kind(value: &Value) -> &'static str {
    match value {
        Value::Number(n) if matches!(number(n), Number::Int(_)) => "integer",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Null | Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The stages one rule expands to, each refusal located at the rule member
/// it came from.
fn rule_stages(rule: &Value, body: &str) -> Step<Vec<Stage>> {
    let pattern = text(get(rule, "pattern").unwrap_or(&Value::Null), "name");
    let replacement = match get(rule, "replace") {
        None | Some(Value::Null) => {
            return Ok(vec![chain_stage(vec![Elementary::DropSort(pattern)])]);
        }
        Some(replacement) => replacement,
    };
    let replace = Path::root().key("replace");
    let mut stages = Vec::new();
    match get(replacement, "name") {
        Some(Value::String(new)) if *new != pattern => {
            stages.push(chain_stage(vec![Elementary::RenameSort {
                old: pattern,
                new: new.clone(),
            }]));
        }
        Some(spec @ Value::Object(_)) => {
            let Some(parts) = template(&text(spec, "template")) else {
                return refuse(replace.key("name"), Refusal::RuleCompile);
            };
            stages.push(transform_stage(
                body,
                FieldTransform::Compute {
                    target: "name".to_owned(),
                    expr: ExprSpec::Template(parts),
                    inverse: None,
                    class: Class::Projection,
                },
            ));
        }
        _ => {}
    }
    for (old, new) in string_map(replacement, "rename_attrs") {
        stages.push(rename_field(body, &old, &new));
    }
    for attr in strings(replacement, "drop_attrs") {
        stages.push(remove_field(body, &attr));
    }
    for (attr, value) in get(replacement, "add_attrs")
        .and_then(members)
        .unwrap_or_default()
    {
        let kind_name = attr_kind(value);
        let Some(default) = datum(value).filter(|d| default_has_kind(d, kind_name)) else {
            return refuse(replace.key("add_attrs").key(attr), Refusal::DefaultValue);
        };
        stages.push(add_field(body, attr, kind_name, default));
    }
    for (attr, spec) in get(replacement, "map_attr_value")
        .and_then(members)
        .unwrap_or_default()
    {
        stages.push(transform_stage(
            body,
            FieldTransform::Apply {
                key: attr.to_owned(),
                expr: ExprSpec::AttrOp(attr.to_owned(), attr_op(spec)),
                inverse: None,
                class: Class::Projection,
            },
        ));
    }
    Ok(stages)
}

/// Definition: the compilation of a `rules` body.
///
/// Each rule expands, in order, to a `rename_sort` from the pattern name to
/// a literal new name that differs from it (a projection computing `name`
/// for a template), one `rename_field` per `rename_attrs` entry, one
/// `remove_field` per `drop_attrs` entry, one `add_field` per `add_attrs`
/// entry (its kind read off its value) and one `apply_expr` per
/// `map_attr_value` entry, each map in key order; a rule without a
/// replacement is a `drop_sort` of the pattern name. After every rule come
/// one stage keeping every rule's `keep_attrs`, and under `passthrough:
/// drop` one keeping the name each replacing rule keeps, each only when
/// non-empty.
///
/// # Errors
///
/// A malformed template at `i/replace/name`, and a default the instance
/// algebra cannot carry at `i/replace/add_attrs/<key>`, relative to the
/// rule list.
///
/// Book anchor: `L-lens-dsl-3`.
pub fn rules_stages(rules: &[Value], drop: bool, body: &str) -> Outcome<Vec<Stage>> {
    outcome(compile_rules(rules, drop, body))
}

fn compile_rules(rules: &[Value], drop: bool, body: &str) -> Step<Vec<Stage>> {
    let mut stages = Vec::new();
    for (i, rule) in rules.iter().enumerate() {
        stages.extend(located(rule_stages(rule, body), &Path::root().index(i))?);
    }
    let replacing = || {
        rules
            .iter()
            .filter(|rule| !matches!(get(rule, "replace"), None | Some(Value::Null)))
    };
    let keep: Vec<String> = replacing()
        .flat_map(|rule| strings(get(rule, "replace").unwrap_or(&Value::Null), "keep_attrs"))
        .collect();
    if !keep.is_empty() {
        stages.push(transform_stage(body, FieldTransform::Keep(keep)));
    }
    if drop {
        let kept: Vec<String> = replacing()
            .map(
                |rule| match get(get(rule, "replace").unwrap_or(&Value::Null), "name") {
                    Some(Value::String(new)) => new.clone(),
                    _ => text(get(rule, "pattern").unwrap_or(&Value::Null), "name"),
                },
            )
            .collect();
        if !kept.is_empty() {
            stages.push(transform_stage(body, FieldTransform::Keep(kept)));
        }
    }
    Ok(stages)
}

// ─── Composition ────────────────────────────────────────────────────────

/// Definition: vertical composition, the concatenation of the parts'
/// stages in order.
///
/// Book anchor: `L-lens-dsl-4`.
#[must_use]
pub fn vertical(parts: Vec<Vec<Stage>>) -> Vec<Stage> {
    parts.into_iter().flatten().collect()
}

/// Definition: horizontal composition.
///
/// Zero parts give no stage. Otherwise each part's chain is fused, and the
/// fused protolenses are folded from the left with `η * θ`; the result is
/// one stage carrying every part's transforms, gathered by anchor in part
/// order.
///
/// # Errors
///
/// The index of the first part whose chain is empty, with
/// [`Refusal::EmptyHorizontalPart`].
///
/// Book anchor: `L-lens-dsl-5`.
pub fn horizontal(parts: Vec<Vec<Stage>>) -> Result<Vec<Stage>, (usize, Refusal)> {
    let mut acc: Option<Proto> = None;
    let mut transforms = Transforms::new();
    for (i, part) in parts.into_iter().enumerate() {
        let chain = chain_of(&part);
        if chain.is_empty() {
            return Err((i, Refusal::EmptyHorizontalPart));
        }
        let fused = fuse(chain);
        acc = Some(match acc {
            None => fused,
            Some(eta) => Proto::Horizontal(Box::new(eta), Box::new(fused)),
        });
        for (anchor, entries) in transforms_of(&part) {
            transforms.entry(anchor).or_default().extend(entries);
        }
    }
    Ok(acc
        .map(|fused| {
            vec![Stage {
                chain: vec![fused],
                transforms,
            }]
        })
        .unwrap_or_default())
}

/// What compilation reads beyond the document itself.
pub struct Context<'a> {
    /// The body vertex field steps act at.
    pub body: &'a str,
    /// The judgements of other components.
    pub oracles: &'a Oracles<'a>,
    /// The documents references resolve against, by identifier.
    pub documents: &'a dyn Fn(&str) -> Option<Value>,
}

/// The part a reference names, relative to the reference.
fn resolve(id: &str, context: &Context<'_>, open: &[String]) -> Step<Vec<Stage>> {
    if open.iter().any(|o| o == id) {
        let mut cycle = open.to_vec();
        cycle.push(id.to_owned());
        return refuse(Path::root(), Refusal::RefCycle(cycle));
    }
    let Some(document) = (context.documents)(id) else {
        return refuse(Path::root(), Refusal::UnresolvedRef(id.to_owned()));
    };
    if check_document(&document).is_err() {
        return Err(Stop::Outside(
            "a referenced document the grammar refuses: production resolves references against read documents",
        ));
    }
    let mut chain = open.to_vec();
    chain.push(id.to_owned());
    match compile_lens(&document, context, &chain) {
        Ok(compiled) if compiled.symmetric.is_some() => {
            refuse(Path::root(), Refusal::SymmetricRef(id.to_owned()))
        }
        Ok(compiled) => Ok(compiled.stages),
        Err(Stop::Refused(inner)) => refuse(Path::root(), Refusal::InRef(id.to_owned(), inner)),
        Err(outside) => Err(outside),
    }
}

fn compose(spec: &Value, context: &Context<'_>, open: &[String]) -> Step<Vec<Stage>> {
    let mut parts = Vec::new();
    for (i, entry) in items(spec, "lenses").iter().enumerate() {
        let here = Path::root().key("lenses").index(i);
        parts.push(match variant(entry) {
            Some(("ref", Value::String(id))) => {
                located(resolve(id, context, open), &here.key("ref"))?
            }
            Some((_, inline)) => located(
                compile_steps(items(inline, "steps"), context.body, context.oracles),
                &here.key("inline").key("steps"),
            )?,
            None => Vec::new(),
        });
    }
    if text(spec, "mode") == "horizontal" {
        horizontal(parts).or_else(|(i, r)| refuse(Path::root().key("lenses").index(i), r))
    } else {
        Ok(vertical(parts))
    }
}

fn symmetric(spec: &Value, context: &Context<'_>) -> Step<CompiledLens> {
    let focus = opt_text(spec, "focus")
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| context.body.to_owned());
    let leg = |name: &str| -> Step<Vec<Stage>> {
        let mut stages = Vec::new();
        for (i, step) in items(spec, name).iter().enumerate() {
            let here = Path::root().key(name).index(i);
            let part = located(compile_step(step, &focus, context.oracles), &here)?;
            if part.iter().any(|s| !s.transforms.is_empty()) {
                return refuse(here, Refusal::ValueStepInSymmetricLeg);
            }
            stages.extend(part);
        }
        Ok(stages)
    };
    let left = leg("left")?;
    let right = leg("right")?;
    Ok(CompiledLens {
        symmetric: Some((chain_of(&left), chain_of(&right))),
        stages: left,
    })
}

fn directed_equation(spec: &Value, oracles: &Oracles<'_>) -> Step<Elementary> {
    let implementation = expr_at(spec, "impl", oracles)?;
    let inverse = opt_expr_at(spec, "inverse", oracles)?;
    let lhs = term_at(spec, "lhs")?;
    let rhs = term_at(spec, "rhs")?;
    let equation = Elementary::DirectedEq {
        name: text(spec, "name"),
        lhs,
        rhs,
        implementation,
        inverse,
        source_kind: opt_text(spec, "source_kind").map(|k| kind(&k)),
        target_kind: opt_text(spec, "target_kind").map(|k| kind(&k)),
        class: class(opt_text(spec, "coercion").as_deref(), Class::Iso),
    };
    if (oracles.directed)(&equation) {
        Ok(equation)
    } else {
        refuse(Path::root(), Refusal::CoercionNotHonest)
    }
}

/// Compile a document the grammar admits; `open` is the chain of
/// documents being compiled, this one last.
fn compile_lens(document: &Value, context: &Context<'_>, open: &[String]) -> Step<CompiledLens> {
    let Ok(body) = check_document(document) else {
        return Err(Stop::Outside("a document the grammar refuses"));
    };
    let member = get(document, body).unwrap_or(&Value::Null);
    let at_body = Path::root().key(body);
    let mut compiled = match body {
        "steps" => CompiledLens {
            stages: located(
                compile_steps(items(document, "steps"), context.body, context.oracles),
                &at_body,
            )?,
            symmetric: None,
        },
        "rules" => CompiledLens {
            stages: located(
                compile_rules(
                    items(document, "rules"),
                    opt_text(document, "passthrough").as_deref() == Some("drop"),
                    context.body,
                ),
                &at_body,
            )?,
            symmetric: None,
        },
        "compose" => CompiledLens {
            stages: located(compose(member, context, open), &at_body)?,
            symmetric: None,
        },
        "symmetric" => located(symmetric(member, context), &at_body)?,
        _ => return refuse(at_body, Refusal::AutoRequiresSchemas),
    };
    let equations = items(document, "directed_equations");
    if !equations.is_empty() {
        let mut chain = Vec::new();
        for (i, spec) in equations.iter().enumerate() {
            let here = Path::root().key("directed_equations").index(i);
            chain.push(located(directed_equation(spec, context.oracles), &here)?);
        }
        compiled.stages.push(chain_stage(chain));
    }
    let right_lossless = compiled
        .symmetric
        .as_ref()
        .is_none_or(|(_, right)| right.iter().all(Proto::lossless));
    if matches!(get(document, "invertible"), Some(Value::Bool(true)))
        && !(lossless(&compiled.stages) && right_lossless)
    {
        return refuse(Path::root().key("invertible"), Refusal::NotInvertible);
    }
    Ok(compiled)
}

/// Definition: the schema-free compilation of a lens document.
///
/// The grammar is checked first ([`check_document`]); then the body is
/// compiled: a step list by [`step_stages`] step after step, a rule list as
/// [`rules_stages`] states, a composition from its parts (a reference
/// resolved through `context.documents`, transitively, an inline step list
/// compiled at the body vertex) by [`vertical`] or [`horizontal`], and a
/// symmetric body's two legs at its focus (the body vertex when empty); an
/// `auto` or `from_diff` body is refused. Each directed equation then
/// becomes one protolens of a final stage, and `invertible: true` is
/// checked last.
///
/// # Errors
///
/// The refusal and its document path, in that order: grammar problems;
/// body refusals under the body key (a reference's at
/// `compose/lenses/i/ref`, a horizontal part's at `compose/lenses/i`, a leg
/// step's at `symmetric/left/i`); a directed equation's under
/// `directed_equations/i`; `NotInvertible` at `invertible`.
///
/// Book anchors: `L-lens-dsl-2`, `L-lens-dsl-4`, `L-lens-dsl-5`,
/// `L-lens-dsl-6`.
pub fn compile_document(document: &Value, context: &Context<'_>) -> Outcome<CompiledLens> {
    if let Err(refused) = check_document(document) {
        return outcome(Err(refused.into()));
    }
    let open = vec![text(document, "id")];
    outcome(compile_lens(document, context, &open))
}
