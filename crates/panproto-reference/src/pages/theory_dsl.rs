//! Complete reference operations assigned to `theory-dsl`.
//!
//! A theory document is the reference crate's own JSON value
//! ([`crate::json::Value`]); Nickel evaluation and YAML reading stay outside
//! this model, which starts from the value they produce. The module checks
//! the document grammar, elaborates a theory's parents and imports, types its
//! equations, compiles morphism documents and bundles, and names the document
//! path of every refusal, in the order the `theory-dsl` correctness page
//! states. Signature checking and morphism checking are the `theories` page's
//! reference ([`typecheck_signature`], [`check_morphism`]); everything else is
//! transcribed from the page and shares no code with production, which reads
//! the document through serde types and the `panproto-gat` checker.
//!
//! The model is total on its domain and says so outside it: a document using
//! a dependent sort, a sort kind other than `structural`, a closed sort, an
//! implicit input, a directed equation, a conflict policy, a `case`, `let`
//! or hole term, a sort applied to arguments, or a body other than a theory,
//! a morphism or a bundle of theories and morphisms is answered with
//! [`Outcome::OutsideDomain`].

use std::collections::BTreeMap;

use crate::json::Value;
use crate::pages::theories::theory::{
    Defect, Equation, Implicit, Morphism, MorphismError, Op, OpImage, Site, Sort, SortExpr,
    SortKind, SymKind, Term, Theory, check_morphism, typecheck_signature,
};

pub use crate::dsl_path::{Path, Segment, locate};

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
    /// A theory name, an inline theory or an inline composition.
    TheoryReference,
}

/// Why the grammar refuses a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeProblem {
    /// The value has the wrong JSON type.
    WrongType(Expected),
    /// An object carries a member the grammar does not name.
    UnknownField,
    /// A required member is absent.
    MissingField(String),
    /// A string names no variant admitted at this place.
    UnknownVariant(String),
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
    /// A sort expression or a term is not in the term grammar.
    TermParse,
    /// A parent, import, domain or codomain names no theory.
    TheoryNotFound(String),
    /// An import exposes a name its theory does not declare.
    UnknownExposed(String),
    /// An operation declares both `input` and `inputs`.
    AmbiguousOpInputs,
    /// The elaborated signature is ill formed; the first defect.
    Signature(Defect),
    /// The named equation is ill typed.
    Equation(String),
    /// A morphism map entry names no domain symbol: the map (`sort` or
    /// `op`) and the key.
    UnknownMapKey(String, String),
    /// The morphism check refuses the morphism.
    MorphismCheck(MorphismError),
    /// A bundle defines a theory or a morphism twice.
    Duplicate(String),
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

/// What a document compiles to: theories and morphisms by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Compiled {
    /// Compiled theories.
    pub theories: BTreeMap<String, Theory>,
    /// Compiled morphisms.
    pub morphisms: BTreeMap<String, Morphism>,
}

/// A step of the reference that stops on a refusal or outside the domain.
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
    Str,
    Bool,
    OptStr,
    List(Box<Self>),
    OptList(Box<Self>),
    Map(Box<Self>),
    Record(Vec<F>),
    Tagged(Vec<(&'static str, Vec<F>)>),
    Choice(Vec<&'static str>),
    TheoryRef,
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

fn param() -> G {
    G::Record(vec![
        req("name", G::Str),
        req("sort", G::Str),
        opt("implicit", G::Bool),
    ])
}

fn op_record() -> G {
    G::Record(vec![
        req("name", G::Str),
        opt("input", G::OptStr),
        opt("inputs", G::OptList(Box::new(param()))),
        req("output", G::Str),
    ])
}

fn equation_record() -> G {
    G::Record(vec![
        req("name", G::Str),
        req("lhs", G::Str),
        req("rhs", G::Str),
    ])
}

fn theory_fields() -> Vec<F> {
    let kind = G::Tagged(vec![
        ("structural", vec![]),
        ("val", vec![req("value_kind", G::Str)]),
        (
            "coercion",
            vec![req("from", G::Str), req("to", G::Str), req("class", G::Str)],
        ),
        ("merger", vec![req("value_kind", G::Str)]),
    ]);
    let sort = G::Record(vec![
        req("name", G::Str),
        opt("params", list(param())),
        opt("kind", kind),
        opt("closed", G::OptList(Box::new(G::Str))),
    ]);
    let directed = G::Record(vec![
        req("name", G::Str),
        req("lhs", G::Str),
        req("rhs", G::Str),
        req("impl_expr", G::Str),
        opt("inverse", G::OptStr),
        opt("source_kind", G::OptStr),
        opt("target_kind", G::OptStr),
        opt("coercion_class", G::Str),
    ]);
    let strategy = G::Tagged(vec![
        ("keep_left", vec![]),
        ("keep_right", vec![]),
        ("fail", vec![]),
        ("custom", vec![req("expr", G::Str)]),
    ]);
    let policy = G::Record(vec![
        req("name", G::Str),
        req("value_kind", G::Str),
        req("strategy", strategy),
    ]);
    let import = G::Record(vec![
        req("from", G::Str),
        opt("alias", G::OptStr),
        opt("expose", list(G::Str)),
    ]);
    vec![
        req("theory", G::Str),
        opt("extends", list(G::Str)),
        opt("imports", list(import)),
        opt("sorts", list(sort)),
        opt("ops", list(op_record())),
        opt("equations", list(equation_record())),
        opt("directed_equations", list(directed)),
        opt("policies", list(policy)),
    ]
}

fn morphism_fields() -> Vec<F> {
    vec![
        req("morphism", G::Str),
        req("domain", G::Str),
        req("codomain", G::Str),
        req("sort_map", G::Map(Box::new(G::Str))),
        req("op_map", G::Map(Box::new(G::Str))),
    ]
}

fn composition_fields() -> Vec<F> {
    let identify = G::Record(vec![
        req("kind", G::Choice(vec!["sort", "op"])),
        req("left", G::Str),
        req("right", G::Str),
    ]);
    let step = G::Record(vec![
        req("left", G::Str),
        req("right", G::Str),
        req("shared_sorts", list(G::Str)),
        opt("shared_ops", list(G::Str)),
        opt("mode", G::Choice(vec!["amalgamate", "explicit_identity"])),
        opt("identify", list(identify)),
    ]);
    vec![
        req("result", G::Str),
        opt("bases", list(G::Str)),
        req("steps", list(step)),
    ]
}

fn protocol_fields() -> Vec<F> {
    let rule = G::Record(vec![
        req("edge_kind", G::Str),
        req("src_kind", G::Str),
        req("tgt_kind", G::Str),
    ]);
    vec![
        req("protocol", G::Str),
        req("schema_theory", G::TheoryRef),
        req("instance_theory", G::TheoryRef),
        opt("edge_rules", list(rule)),
    ]
}

/// The fields of the body selected by `key`.
fn body_fields(key: &str) -> Vec<F> {
    match key {
        "theory" => theory_fields(),
        "morphism" => morphism_fields(),
        "compose" => vec![req("compose", G::Record(composition_fields()))],
        "protocol" => protocol_fields(),
        "bundle" => vec![
            req("bundle", G::Str),
            opt("theories", list(G::Record(theory_fields()))),
            opt("morphisms", list(G::Record(morphism_fields()))),
            opt("compositions", list(G::Record(composition_fields()))),
            opt("protocols", list(G::Record(protocol_fields()))),
        ],
        "class" => vec![
            req("class", G::Str),
            req("params", list(G::Str)),
            req("signatures", list(op_record())),
            opt("axioms", list(equation_record())),
        ],
        "instance" => vec![
            req("instance", G::Str),
            req("class", G::Str),
            req("target", G::Str),
            req("bindings", G::Map(Box::new(G::Str))),
        ],
        _ => vec![
            req("inductive", G::Str),
            opt("params", list(param())),
            req(
                "constructors",
                list(G::Record(vec![
                    req("name", G::Str),
                    opt("inputs", list(param())),
                ])),
            ),
        ],
    }
}

/// The body keys in grammar order.
pub const BODY_KEYS: [&str; 8] = [
    "theory",
    "morphism",
    "compose",
    "protocol",
    "bundle",
    "class",
    "instance",
    "inductive",
];

/// The members of an object, the last occurrence of each name kept, in the
/// order of their names.
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

impl std::convert::From<(Path, ShapeProblem)> for Stop {
    fn from((path, problem): (Path, ShapeProblem)) -> Self {
        Self::Refused(Box::new((path, Refusal::Shape(problem))))
    }
}

fn wrong<T>(path: &Path, expected: Expected) -> Shaped<T> {
    Err((path.clone(), ShapeProblem::WrongType(expected)))
}

fn check(value: &Value, g: &G, path: &Path) -> Shaped<()> {
    match g {
        G::Str => match value {
            Value::String(_) => Ok(()),
            _ => wrong(path, Expected::String),
        },
        G::OptStr => match value {
            Value::String(_) | Value::Null => Ok(()),
            _ => wrong(path, Expected::String),
        },
        G::Bool => match value {
            Value::Bool(_) => Ok(()),
            _ => wrong(path, Expected::Boolean),
        },
        G::List(element) => check_list(value, element, path),
        G::OptList(element) => match value {
            Value::Null => Ok(()),
            _ => check_list(value, element, path),
        },
        G::Map(member) => {
            let Some(entries) = members(value) else {
                return wrong(path, Expected::Object);
            };
            for (name, entry) in entries {
                check(entry, member, &path.key(name))?;
            }
            Ok(())
        }
        G::Record(fields) => check_record(value, &[], fields, path),
        G::Tagged(variants) => {
            let Some(entries) = members(value) else {
                return wrong(path, Expected::Object);
            };
            let Some(tag) = entries.get("type") else {
                return Err((path.clone(), ShapeProblem::MissingField("type".to_owned())));
            };
            let Value::String(tag) = tag else {
                return wrong(&path.key("type"), Expected::String);
            };
            let Some((_, fields)) = variants.iter().find(|(name, _)| name == tag) else {
                return Err((path.key("type"), ShapeProblem::UnknownVariant(tag.clone())));
            };
            check_record(value, &[req("type", G::Str)], fields, path)
        }
        G::Choice(choices) => match value {
            Value::String(found) if choices.contains(&found.as_str()) => Ok(()),
            Value::String(found) => {
                Err((path.clone(), ShapeProblem::UnknownVariant(found.clone())))
            }
            _ => wrong(path, Expected::String),
        },
        G::TheoryRef => match value {
            Value::String(_) => Ok(()),
            Value::Object(_) => {
                let has_theory = members(value).is_some_and(|m| m.contains_key("theory"));
                let fields = if has_theory {
                    theory_fields()
                } else {
                    composition_fields()
                };
                check_record(value, &[], &fields, path)
            }
            _ => wrong(path, Expected::TheoryReference),
        },
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
/// The value must be an object with exactly one body key (an `instance`
/// document's `class` member is that body's field); the body key selects the
/// field table, which `id` and `description` lead. Returns the body key.
///
/// # Errors
///
/// The path and problem of the first value refused, in the order the page
/// states: an object's least unknown member name, then each field in table
/// order, a map's members in name order, an array's elements in order.
///
/// Book anchor: `L-theory-dsl-1`.
pub fn check_document(value: &Value) -> Result<&'static str, (Path, ShapeProblem)> {
    let root = Path::root();
    let Some(entries) = members(value) else {
        return wrong(&root, Expected::Object);
    };
    let mut present: Vec<&'static str> = BODY_KEYS
        .iter()
        .copied()
        .filter(|key| entries.contains_key(key))
        .collect();
    if present.contains(&"instance") {
        present.retain(|key| *key != "class");
    }
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
    check_record(
        value,
        &[req("id", G::Str), req("description", G::Str)],
        &body_fields(body),
        &root,
    )?;
    Ok(body)
}

// ─── Reading checked values ─────────────────────────────────────────────

fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    members(value).and_then(|m| m.get(key).copied())
}

fn text(value: &Value, key: &str) -> String {
    match get(value, key) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
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

// ─── Terms and sort expressions ─────────────────────────────────────────

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// An identifier, or `Q.n` for identifiers `Q` and `n`.
fn is_symbol(s: &str) -> bool {
    match s.split_once('.') {
        Some((q, n)) => is_identifier(q) && is_identifier(n),
        None => is_identifier(s),
    }
}

/// Split the text between an application's parentheses into arguments:
/// commas at parenthesis depth zero separate them, and blank text has none.
fn arguments(inner: &str) -> Vec<&str> {
    if inner.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (at, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

/// The shape of a term or sort expression as text.
enum Shape<'a> {
    /// No parenthesis.
    Atom,
    /// `head(inner)`.
    Applied(&'a str, &'a str),
}

/// `head(inner)` split at the first `(` and its matching `)`, which must end
/// the text; `None` when the parentheses do not close or text follows them.
fn application(s: &str) -> Option<Shape<'_>> {
    let Some(open) = s.find('(') else {
        return Some(Shape::Atom);
    };
    let mut depth = 0usize;
    for (at, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let close = open + at;
                    return s[close + 1..]
                        .trim()
                        .is_empty()
                        .then(|| Shape::Applied(s[..open].trim(), &s[open + 1..close]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Definition: the term grammar of an equation side.
///
/// `term ::= ident | symbol '(' (term (',' term)*)? ')'`, with whitespace
/// around tokens, where `symbol` is an identifier or `Q.n`. A term beginning
/// with `case `, `let ` or `?` is outside the domain.
fn parse_term(s: &str) -> Step<Term> {
    let s = s.trim();
    if s.starts_with("case ") || s.starts_with("let ") || s.starts_with('?') {
        return Err(Stop::Outside("case, let and hole terms"));
    }
    match application(s) {
        Some(Shape::Atom) if is_identifier(s) => Ok(Term::Var(s.to_owned())),
        Some(Shape::Applied(head, inner)) if is_symbol(head) => {
            let args = arguments(inner)
                .into_iter()
                .map(parse_term)
                .collect::<Step<Vec<_>>>()?;
            Ok(Term::App(head.to_owned(), args))
        }
        _ => refuse(Path::root(), Refusal::TermParse),
    }
}

/// A sort expression: a symbol, or `symbol()` for the same sort. A sort
/// applied to arguments is outside the domain.
fn parse_sort(s: &str) -> Step<String> {
    let s = s.trim();
    match application(s) {
        Some(Shape::Atom) if is_symbol(s) => Ok(s.to_owned()),
        Some(Shape::Applied(head, inner)) if is_symbol(head) => {
            if inner.trim().is_empty() {
                Ok(head.to_owned())
            } else {
                Err(Stop::Outside("sorts applied to arguments"))
            }
        }
        _ => refuse(Path::root(), Refusal::TermParse),
    }
}

// ─── Theory elaboration ─────────────────────────────────────────────────

/// Where a declaration of the elaborated theory comes from.
#[derive(Clone, Debug)]
enum Origin {
    /// The spec's own declaration at this path.
    Own(Path),
    /// A declaration contributed by the `extends` or `imports` entry at
    /// this path.
    Entry(Path),
}

#[derive(Default)]
struct Union {
    sorts: Vec<(Sort, Origin)>,
    ops: Vec<(Op, Origin)>,
    eqs: Vec<(Equation, Origin)>,
    /// Operations the spec declares with the `input` shorthand.
    shorthand: Vec<String>,
}

impl Union {
    fn include(&mut self, theory: &Theory, entry: &Path) {
        for sort in &theory.sorts {
            if !self.sorts.iter().any(|(s, _)| s == sort) {
                self.sorts
                    .push((sort.clone(), Origin::Entry(entry.clone())));
            }
        }
        for op in &theory.ops {
            if !self.ops.iter().any(|(o, _)| o == op) {
                self.ops.push((op.clone(), Origin::Entry(entry.clone())));
            }
        }
        for eq in &theory.eqs {
            if !self.eqs.iter().any(|(e, _)| e == eq) {
                self.eqs.push((eq.clone(), Origin::Entry(entry.clone())));
            }
        }
    }
}

/// The path of a declaration followed by `suffix` when the declaration is
/// the spec's own, and the path of its `extends` or `imports` entry
/// otherwise.
fn origin_path(from: &Origin, suffix: &Path) -> Path {
    match from {
        Origin::Own(path) => path.join(suffix),
        Origin::Entry(path) => path.clone(),
    }
}

fn suffix(steps: &[&str]) -> Path {
    Path(
        steps
            .iter()
            .map(|step| {
                step.parse::<usize>()
                    .map_or_else(|_| Segment::Key((*step).to_owned()), Segment::Index)
            })
            .collect(),
    )
}

/// The path of the first signature defect.
fn defect_path(union: &Union, defect: &Defect) -> Path {
    let sort_from = |name: &str| union.sorts.iter().find(|(s, _)| s.name == name);
    let op_from = |name: &str| union.ops.iter().find(|(o, _)| o.name == name);
    let site = |site: &Site| match site {
        Site::SortParam(sort, index) => sort_from(sort)
            .map(|(_, from)| origin_path(from, &suffix(&["params", &index.to_string(), "sort"]))),
        Site::OpInput(op, index) => op_from(op).map(|(o, from)| {
            if matches!(from, Origin::Own(_)) && union.shorthand.contains(&o.name) {
                origin_path(from, &suffix(&["input"]))
            } else {
                origin_path(from, &suffix(&["inputs", &index.to_string(), "sort"]))
            }
        }),
        Site::OpOutput(op) => op_from(op).map(|(_, from)| origin_path(from, &suffix(&["output"]))),
    };
    let found = match defect {
        Defect::UndeclaredSort(s, _)
        | Defect::SortArity(s, ..)
        | Defect::ParamIllTyped(s, ..)
        | Defect::UnboundParamVar(s, _)
        | Defect::DuplicateBinder(s, _) => site(s),
        Defect::DuplicateSymbol(kind, name) => {
            let froms: Vec<&Origin> = match kind {
                SymKind::Sort => union
                    .sorts
                    .iter()
                    .filter(|(s, _)| &s.name == name)
                    .map(|(_, f)| f)
                    .collect(),
                SymKind::Op => union
                    .ops
                    .iter()
                    .filter(|(o, _)| &o.name == name)
                    .map(|(_, f)| f)
                    .collect(),
                SymKind::Eq => union
                    .eqs
                    .iter()
                    .filter(|(e, _)| &e.name == name)
                    .map(|(_, f)| f)
                    .collect(),
                SymKind::DirectedEq | SymKind::Policy => Vec::new(),
            };
            froms.get(1).map(|from| origin_path(from, &Path::root()))
        }
    };
    found.unwrap_or_default()
}

/// The names one import brings into scope.
struct ImportScope {
    /// Qualifiers: the imported theory's name and the alias, if any.
    qualifiers: Vec<String>,
    /// Whether a bare name of this import resolves to it: no alias, or the
    /// name is exposed.
    bare: Box<dyn Fn(&str) -> bool>,
    /// The import's sorts and operations, each with its new name.
    sorts: Vec<(String, String)>,
    ops: Vec<(String, String)>,
}

/// Resolve a sort or operation head of the spec's own declarations.
fn resolve(head: &str, own: &[String], scopes: &[ImportScope], ops: bool) -> String {
    let table = |scope: &ImportScope| -> Vec<(String, String)> {
        if ops {
            scope.ops.clone()
        } else {
            scope.sorts.clone()
        }
    };
    if own.iter().any(|n| n == head) {
        return head.to_owned();
    }
    if let Some((q, n)) = head.split_once('.') {
        for scope in scopes {
            if scope.qualifiers.iter().any(|x| x == q)
                && let Some((_, new)) = table(scope).into_iter().find(|(old, _)| old == n)
            {
                return new;
            }
        }
        return head.to_owned();
    }
    for scope in scopes {
        if (scope.bare)(head)
            && let Some((_, new)) = table(scope).into_iter().find(|(old, _)| old == head)
        {
            return new;
        }
    }
    head.to_owned()
}

fn rename_term(term: &Term, map: &dyn Fn(&str) -> String) -> Term {
    match term {
        Term::Var(x) => Term::Var(x.clone()),
        Term::App(op, args) => {
            Term::App(map(op), args.iter().map(|a| rename_term(a, map)).collect())
        }
        other => other.clone(),
    }
}

/// The theory `theory` with each sort and operation name `n` renamed to
/// `rename(n)` everywhere, and each equation name likewise.
fn renamed(theory: &Theory, rename: &dyn Fn(&str) -> String) -> Step<Theory> {
    let sort = |s: &SortExpr| match s {
        SortExpr::Name(n) => Ok(SortExpr::Name(rename(n))),
        SortExpr::App(..) => Err(Stop::Outside("dependent sorts")),
    };
    let mut out = Theory {
        name: theory.name.clone(),
        extends: theory.extends.clone(),
        ..Theory::default()
    };
    for s in &theory.sorts {
        out.sorts.push(Sort {
            name: rename(&s.name),
            params: Vec::new(),
            kind: s.kind,
            constructors: None,
        });
    }
    for o in &theory.ops {
        out.ops.push(Op {
            name: rename(&o.name),
            inputs: o
                .inputs
                .iter()
                .map(|(x, s, i)| Ok((x.clone(), sort(s)?, *i)))
                .collect::<Step<Vec<_>>>()?,
            output: sort(&o.output)?,
        });
    }
    for e in &theory.eqs {
        out.eqs.push(Equation {
            name: rename(&e.name),
            lhs: rename_term(&e.lhs, rename),
            rhs: rename_term(&e.rhs, rename),
        });
    }
    Ok(out)
}

/// Whether `theory` lies in the domain: simple structural open sorts, ops
/// without implicit inputs over simple sorts, first-order equations, no
/// directed equations and no policies.
fn library_in_domain(theory: &Theory) -> Step<()> {
    let first_order = |t: &Term| -> bool {
        fn walk(t: &Term) -> bool {
            match t {
                Term::Var(_) => true,
                Term::App(_, args) => args.iter().all(walk),
                _ => false,
            }
        }
        walk(t)
    };
    let simple =
        theory.sorts.iter().all(|s| {
            s.params.is_empty() && s.kind == SortKind::Structural && s.constructors.is_none()
        }) && theory.ops.iter().all(|o| {
            matches!(o.output, SortExpr::Name(_))
                && o.inputs
                    .iter()
                    .all(|(_, s, i)| matches!(s, SortExpr::Name(_)) && *i == Implicit::No)
        }) && theory
            .eqs
            .iter()
            .all(|e| first_order(&e.lhs) && first_order(&e.rhs))
            && theory.directed_eqs.is_empty()
            && theory.policies.is_empty();
    if simple {
        Ok(())
    } else {
        Err(Stop::Outside("a resolved theory outside the domain"))
    }
}

/// The spec's own declarations outside the domain, checked before anything
/// else so that the reference never answers for a document it cannot model.
fn spec_in_domain(spec: &Value) -> Step<()> {
    for sort in items(spec, "sorts") {
        if !items(sort, "params").is_empty() {
            return Err(Stop::Outside("dependent sorts"));
        }
        if let Some(kind) = get(sort, "kind")
            && text(kind, "type") != "structural"
        {
            return Err(Stop::Outside("sort kinds"));
        }
        if matches!(get(sort, "closed"), Some(Value::Array(_))) {
            return Err(Stop::Outside("closed sorts"));
        }
    }
    for op in items(spec, "ops") {
        if items(op, "inputs")
            .iter()
            .any(|p| matches!(get(p, "implicit"), Some(Value::Bool(true))))
        {
            return Err(Stop::Outside("implicit inputs"));
        }
    }
    if !items(spec, "directed_equations").is_empty() || !items(spec, "policies").is_empty() {
        return Err(Stop::Outside("directed equations and policies"));
    }
    Ok(())
}

/// The parameter name of the `input` shorthand: the first character of the
/// input sort as written, lowercased, when it is an ASCII letter, and `x`
/// otherwise.
fn shorthand_param(input: &str) -> String {
    input
        .chars()
        .next()
        .filter(char::is_ascii_alphabetic)
        .map_or_else(|| "x".to_owned(), |c| c.to_ascii_lowercase().to_string())
}

/// One local operation, parsed, before name resolution: name, inputs
/// `(binder, sort head)`, output head.
type ParsedOp = (String, Vec<(String, String)>, String);

fn parse_op(op: &Value) -> Step<ParsedOp> {
    let name = text(op, "name");
    let output =
        parse_sort(&text(op, "output")).map_err(|s| s.within(&Path::root().key("output")))?;
    let input = opt_text(op, "input");
    let inputs = match get(op, "inputs") {
        Some(Value::Array(list)) => Some(list.as_slice()),
        _ => None,
    };
    let inputs = match (input, inputs) {
        (Some(_), Some(_)) => return refuse(Path::root(), Refusal::AmbiguousOpInputs),
        (Some(input), None) => {
            let sort = parse_sort(&input).map_err(|s| s.within(&Path::root().key("input")))?;
            vec![(shorthand_param(&input), sort)]
        }
        (None, Some(list)) => list
            .iter()
            .enumerate()
            .map(|(index, p)| {
                parse_sort(&text(p, "sort"))
                    .map(|sort| (text(p, "name"), sort))
                    .map_err(|s| s.within(&suffix(&["inputs", &index.to_string(), "sort"])))
            })
            .collect::<Step<Vec<_>>>()?,
        (None, None) => Vec::new(),
    };
    Ok((name, inputs, output))
}

/// Definition: the elaboration of a theory body.
///
/// Parents are resolved and included in `extends` order, then each import in
/// order is resolved, its exposed names checked and its declarations renamed
/// and included; an included declaration equal to one already present is
/// included once. The spec's own sorts, operations and equations follow, in
/// that order, with names resolved as the page states; then the signature is
/// checked and each equation typed in order.
///
/// # Errors
///
/// The refusals of the page, at the path of the element refused, relative to
/// the theory body.
///
/// Book anchor: `L-theory-dsl-2`.
#[must_use]
pub fn elaborate(spec: &Value, resolve_theory: &dyn Fn(&str) -> Option<Theory>) -> Outcome<Theory> {
    outcome(elaborate_step(spec, resolve_theory))
}

fn elaborate_step(spec: &Value, resolve_theory: &dyn Fn(&str) -> Option<Theory>) -> Step<Theory> {
    spec_in_domain(spec)?;
    let root = Path::root();
    let name = text(spec, "theory");
    let extends = strings(spec, "extends");
    let mut union = Union::default();
    for (index, parent) in extends.iter().enumerate() {
        let at = root.key("extends").index(index);
        let Some(theory) = resolve_theory(parent) else {
            return refuse(at, Refusal::TheoryNotFound(parent.clone()));
        };
        library_in_domain(&theory)?;
        union.include(&theory, &at);
    }
    let own_sorts: Vec<String> = items(spec, "sorts")
        .iter()
        .map(|s| text(s, "name"))
        .chain(union.sorts.iter().map(|(s, _)| s.name.clone()))
        .collect();
    let own_ops: Vec<String> = items(spec, "ops")
        .iter()
        .map(|o| text(o, "name"))
        .chain(union.ops.iter().map(|(o, _)| o.name.clone()))
        .collect();

    let mut scopes: Vec<ImportScope> = Vec::new();
    for (index, import) in items(spec, "imports").iter().enumerate() {
        let at = root.key("imports").index(index);
        scopes.push(include_import(import, &at, resolve_theory, &mut union)?);
    }

    let sort_head = |h: &str| resolve(h, &own_sorts, &scopes, false);
    let op_head = |h: &str| resolve(h, &own_ops, &scopes, true);
    for (index, sort) in items(spec, "sorts").iter().enumerate() {
        union.sorts.push((
            Sort {
                name: text(sort, "name"),
                params: Vec::new(),
                kind: SortKind::Structural,
                constructors: None,
            },
            Origin::Own(root.key("sorts").index(index)),
        ));
    }
    for (index, op) in items(spec, "ops").iter().enumerate() {
        let at = root.key("ops").index(index);
        let (op_name, inputs, output) = parse_op(op).map_err(|s| s.within(&at))?;
        if opt_text(op, "input").is_some() {
            union.shorthand.push(op_name.clone());
        }
        union.ops.push((
            Op {
                name: op_name,
                inputs: inputs
                    .into_iter()
                    .map(|(x, s)| (x, SortExpr::Name(sort_head(&s)), Implicit::No))
                    .collect(),
                output: SortExpr::Name(sort_head(&output)),
            },
            Origin::Own(at),
        ));
    }
    for (index, eq) in items(spec, "equations").iter().enumerate() {
        let at = root.key("equations").index(index);
        let lhs = parse_term(&text(eq, "lhs")).map_err(|s| s.within(&at.key("lhs")))?;
        let rhs = parse_term(&text(eq, "rhs")).map_err(|s| s.within(&at.key("rhs")))?;
        union.eqs.push((
            Equation {
                name: text(eq, "name"),
                lhs: rename_term(&lhs, &op_head),
                rhs: rename_term(&rhs, &op_head),
            },
            Origin::Own(at),
        ));
    }

    checked(name, extends, &union)
}

/// Resolve the import at `at`, check its exposed names, include its renamed
/// declarations and return the names it brings into scope.
fn include_import(
    import: &Value,
    at: &Path,
    resolve_theory: &dyn Fn(&str) -> Option<Theory>,
    union: &mut Union,
) -> Step<ImportScope> {
    let from = text(import, "from");
    let Some(theory) = resolve_theory(&from) else {
        return refuse(at.key("from"), Refusal::TheoryNotFound(from));
    };
    library_in_domain(&theory)?;
    let alias = opt_text(import, "alias");
    let expose = strings(import, "expose");
    for (j, exposed) in expose.iter().enumerate() {
        let declared = theory.sorts.iter().any(|s| &s.name == exposed)
            || theory.ops.iter().any(|o| &o.name == exposed)
            || theory.eqs.iter().any(|e| &e.name == exposed);
        if !declared {
            return refuse(
                at.key("expose").index(j),
                Refusal::UnknownExposed(exposed.clone()),
            );
        }
    }
    let prefix = alias.clone().unwrap_or_else(|| from.clone());
    let exposed_list = expose.clone();
    let rename = move |n: &str| {
        if exposed_list.iter().any(|e| e == n) {
            n.to_owned()
        } else {
            format!("{prefix}_{n}")
        }
    };
    union.include(&renamed(&theory, &rename)?, at);
    let mut qualifiers = vec![from];
    qualifiers.extend(alias.clone());
    let unaliased = alias.is_none();
    Ok(ImportScope {
        qualifiers,
        bare: Box::new(move |n| unaliased || expose.iter().any(|e| e == n)),
        sorts: theory
            .sorts
            .iter()
            .map(|s| (s.name.clone(), rename(&s.name)))
            .collect(),
        ops: theory
            .ops
            .iter()
            .map(|o| (o.name.clone(), rename(&o.name)))
            .collect(),
    })
}

/// The elaborated theory, after its signature check and the typing of
/// each equation in order.
fn checked(name: String, extends: Vec<String>, union: &Union) -> Step<Theory> {
    let theory = Theory {
        name,
        extends,
        sorts: union.sorts.iter().map(|(s, _)| s.clone()).collect(),
        ops: union.ops.iter().map(|(o, _)| o.clone()).collect(),
        eqs: union.eqs.iter().map(|(e, _)| e.clone()).collect(),
        directed_eqs: Vec::new(),
        policies: Vec::new(),
    };
    if let Err(defects) = typecheck_signature(&theory) {
        let first = defects[0].clone();
        return refuse(defect_path(union, &first), Refusal::Signature(first));
    }
    for (eq, from) in &union.eqs {
        if !equation_typed(eq, &theory) {
            return refuse(
                origin_path(from, &Path::root()),
                Refusal::Equation(eq.name.clone()),
            );
        }
    }
    Ok(theory)
}

/// Definition: first-order equation typing.
///
/// Every variable takes the input sort of each argument position it fills,
/// and all of them must agree; each application names a declared operation
/// with as many arguments as inputs, each argument of the input's sort; both
/// sides have one sort.
fn equation_typed(eq: &Equation, theory: &Theory) -> bool {
    fn constrain(t: &Term, theory: &Theory, ctx: &mut BTreeMap<String, String>) -> bool {
        let Term::App(op, args) = t else {
            return true;
        };
        let Some(o) = theory.ops.iter().find(|o| &o.name == op) else {
            return false;
        };
        if o.inputs.len() != args.len() {
            return false;
        }
        args.iter().zip(&o.inputs).all(|(a, (_, s, _))| match a {
            Term::Var(x) => {
                *ctx.entry(x.clone()).or_insert_with(|| s.head().to_owned()) == s.head()
            }
            _ => constrain(a, theory, ctx),
        })
    }
    fn sort_of(t: &Term, theory: &Theory, ctx: &BTreeMap<String, String>) -> Option<String> {
        match t {
            Term::Var(x) => ctx.get(x).cloned(),
            Term::App(op, args) => {
                let o = theory.ops.iter().find(|o| &o.name == op)?;
                let all = args.iter().zip(&o.inputs).all(|(a, (_, s, _))| {
                    sort_of(a, theory, ctx).is_some_and(|found| found == s.head())
                });
                (all && args.len() == o.inputs.len()).then(|| o.output.head().to_owned())
            }
            _ => None,
        }
    }
    let mut ctx = BTreeMap::new();
    if !constrain(&eq.lhs, theory, &mut ctx) || !constrain(&eq.rhs, theory, &mut ctx) {
        return false;
    }
    match (
        sort_of(&eq.lhs, theory, &ctx),
        sort_of(&eq.rhs, theory, &ctx),
    ) {
        (Some(l), Some(r)) => l == r,
        _ => false,
    }
}

// ─── Morphisms and bundles ──────────────────────────────────────────────

/// The path, relative to the morphism body, of the entry a morphism check
/// refuses.
fn morphism_refusal_path(error: &MorphismError, m: &Morphism, domain: &Theory) -> Path {
    let root = Path::root();
    let first_sort_to = |target: &str| {
        domain
            .sorts
            .iter()
            .find(|s| m.sorts.get(&s.name).is_some_and(|t| t == target))
            .map(|s| root.key("sort_map").key(&s.name))
    };
    let first_op_to = |target: &str| {
        domain
            .ops
            .iter()
            .find(|o| matches!(m.ops.get(&o.name), Some(OpImage::Op(t)) if t == target))
            .map(|o| root.key("op_map").key(&o.name))
    };
    let found = match error {
        MorphismError::MissingSortMapping(_) => Some(root.key("sort_map")),
        MorphismError::MissingOpMapping(_) => Some(root.key("op_map")),
        MorphismError::SortNotFound(target) => first_sort_to(target),
        MorphismError::OpNotFound(target) => first_op_to(target),
        MorphismError::SortArityMismatch { sort, .. }
        | MorphismError::SortKindMismatch { sort, .. }
        | MorphismError::SortParamMismatch { sort, .. }
        | MorphismError::ClosureMismatch { sort, .. } => Some(root.key("sort_map").key(sort)),
        MorphismError::OpTypeMismatch { op, .. }
        | MorphismError::TermAssignmentUnboundVar { op, .. }
        | MorphismError::TermAssignmentIllTyped { op, .. }
        | MorphismError::TermAssignmentSortMismatch { op, .. } => Some(root.key("op_map").key(op)),
        _ => None,
    };
    found.unwrap_or(root)
}

/// Definition: the compilation of a morphism body.
///
/// The domain and then the codomain are resolved; the least sort-map key and
/// then the least operation-map key naming no domain symbol is refused; the
/// morphism is then checked, a refusal located at the map entry it concerns.
///
/// # Errors
///
/// The refusals of the page at their paths, relative to the morphism body.
///
/// Book anchor: `L-theory-dsl-3`.
#[must_use]
pub fn compile_morphism(
    spec: &Value,
    resolve_theory: &dyn Fn(&str) -> Option<Theory>,
) -> Outcome<Morphism> {
    outcome(morphism_step(spec, resolve_theory))
}

fn morphism_step(spec: &Value, resolve_theory: &dyn Fn(&str) -> Option<Theory>) -> Step<Morphism> {
    let root = Path::root();
    let domain_name = text(spec, "domain");
    let codomain_name = text(spec, "codomain");
    let Some(domain) = resolve_theory(&domain_name) else {
        return refuse(root.key("domain"), Refusal::TheoryNotFound(domain_name));
    };
    let Some(codomain) = resolve_theory(&codomain_name) else {
        return refuse(root.key("codomain"), Refusal::TheoryNotFound(codomain_name));
    };
    let sort_map = string_map(spec, "sort_map");
    let op_map = string_map(spec, "op_map");
    if let Some(key) = sort_map
        .keys()
        .find(|k| !domain.sorts.iter().any(|s| &s.name == *k))
    {
        return refuse(
            root.key("sort_map").key(key),
            Refusal::UnknownMapKey("sort".to_owned(), key.clone()),
        );
    }
    if let Some(key) = op_map
        .keys()
        .find(|k| !domain.ops.iter().any(|o| &o.name == *k))
    {
        return refuse(
            root.key("op_map").key(key),
            Refusal::UnknownMapKey("op".to_owned(), key.clone()),
        );
    }
    let morphism = Morphism {
        name: text(spec, "morphism"),
        domain: domain_name,
        codomain: codomain_name,
        sorts: sort_map,
        ops: op_map
            .into_iter()
            .map(|(k, v)| (k, OpImage::Op(v)))
            .collect(),
    };
    if let Err(error) = check_morphism(&domain, &codomain, &morphism) {
        return refuse(
            morphism_refusal_path(&error, &morphism, &domain),
            Refusal::MorphismCheck(error),
        );
    }
    Ok(morphism)
}

/// Definition: the compilation of a document.
///
/// The grammar is checked first; a theory body is elaborated, a morphism body
/// compiled, and a bundle's theories are elaborated in listed order, each
/// seeing the bundle's earlier theories before the library, then its
/// morphisms in listed order against the bundle's theories and the library.
///
/// # Errors
///
/// The first refusal, at its path from the document root.
///
/// Book anchors: `L-theory-dsl-1` to `L-theory-dsl-4`.
#[must_use]
pub fn compile_document(
    document: &Value,
    library: &dyn Fn(&str) -> Option<Theory>,
) -> Outcome<Compiled> {
    outcome(document_step(document, library))
}

fn document_step(document: &Value, library: &dyn Fn(&str) -> Option<Theory>) -> Step<Compiled> {
    let body = check_document(document)?;
    let root = Path::root();
    let mut out = Compiled::default();
    match body {
        "theory" => {
            let theory = elaborate_step(document, library)?;
            out.theories.insert(theory.name.clone(), theory);
        }
        "morphism" => {
            let morphism = morphism_step(document, library)?;
            out.morphisms.insert(morphism.name.clone(), morphism);
        }
        "bundle" => {
            for (index, spec) in items(document, "theories").iter().enumerate() {
                let at = root.key("theories").index(index);
                let name = text(spec, "theory");
                if out.theories.contains_key(&name) {
                    return refuse(at.key("theory"), Refusal::Duplicate(name));
                }
                let seen = out.theories.clone();
                let combined = |n: &str| seen.get(n).cloned().or_else(|| library(n));
                let theory = elaborate_step(spec, &combined).map_err(|s| s.within(&at))?;
                out.theories.insert(name, theory);
            }
            if !items(document, "compositions").is_empty() {
                return Err(Stop::Outside("compositions"));
            }
            for (index, spec) in items(document, "morphisms").iter().enumerate() {
                let at = root.key("morphisms").index(index);
                let name = text(spec, "morphism");
                if out.morphisms.contains_key(&name) {
                    return refuse(at.key("morphism"), Refusal::Duplicate(name));
                }
                let seen = out.theories.clone();
                let combined = |n: &str| seen.get(n).cloned().or_else(|| library(n));
                let morphism = morphism_step(spec, &combined).map_err(|s| s.within(&at))?;
                out.morphisms.insert(name, morphism);
            }
            if !items(document, "protocols").is_empty() {
                return Err(Stop::Outside("protocols"));
            }
        }
        _ => {
            return Err(Stop::Outside(
                "bodies other than theory, morphism and bundle",
            ));
        }
    }
    Ok(out)
}
