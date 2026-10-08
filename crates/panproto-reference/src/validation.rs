//! Whole-instance validation as predicates over an instance, the status
//! aggregation, the budget law and the admission policies.
//!
//! Each check kind is a predicate read off its definition over the graph
//! form of the instance (a table's rows are nodes with their cells as
//! fields and its pairs edges). Domain: constraint sorts whose semantics is
//! one of `MinOutDegree`, `MaxOutDegree`, `Minimum`, `Maximum` (numeric
//! bounds), `MaxLength`/`MinLength` counted in characters or UTF-8 bytes,
//! `RequiredValue`, `Acyclic` or `Annotation`; references with a key-value
//! source of one value per part; endpoint anchors compared by equality.
//! A schema outside that domain gives `None`.

use std::collections::{BTreeMap, BTreeSet};

use crate::exact::DecimalValue;
use crate::fan::{FanViolation, candidates as fan_candidates, violations_at};
use crate::keys;
use crate::lens::ViewRef;
use crate::model::{Graph, Name, NodeShape, SEdge};
use crate::restrict::is_fan_kind;
use crate::schema::{Cardinality, KeyRole, KeyScope, OnUnresolved, RefSource, Schema};
use crate::value::{Presence, Value};

/// Index/null matching uses the same predicate as key semantics.
pub use crate::keys::index_and_null_match;
/// Lexical-space validation uses the declared lexical recognizer.
pub use crate::keys::lexical_space;

/// The semantics a protocol gives a constraint sort (the reference's
/// domain).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Semantics {
    /// No check.
    Annotation,
    /// A maximum length in characters (`true`) or UTF-8 bytes.
    MaxLength(bool),
    /// A minimum length in characters (`true`) or UTF-8 bytes.
    MinLength(bool),
    /// An inclusive numeric lower bound.
    Minimum,
    /// An inclusive numeric upper bound.
    Maximum,
    /// A lower bound on out-degree.
    MinOutDegree,
    /// An upper bound on out-degree.
    MaxOutDegree,
    /// A value is required.
    RequiredValue,
    /// The self-edge named by the value is acyclic.
    Acyclic,
    /// A semantics outside the reference's domain.
    Other,
}

/// A check kind, in its declaration order (which leads the evaluation and
/// report order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckKind {
    /// Every anchor is a schema vertex.
    NodeAnchor,
    /// Every non-fan edge's label is a schema edge.
    EdgeDeclared,
    /// Every non-fan edge's endpoints are anchored at its label's ends.
    EndpointCoherence,
    /// Every fan is well formed.
    FanWellFormed,
    /// Required edges are present.
    RequiredEdge,
    /// Out-degrees lie in their bounds.
    OutDegree,
    /// Positions of an ordered edge are distinct per node.
    PositionDistinct,
    /// Values satisfy their constraints.
    ValueConstraint,
    /// Required values are present.
    RequiredValue,
    /// Keys are unique.
    KeyUnique,
    /// References resolve.
    ReferenceResolves,
    /// Declared self-edges are acyclic.
    Acyclic,
}

/// What a check is about.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SchemaRef {
    /// The whole schema.
    Schema,
    /// A vertex.
    Vertex(Name),
    /// An edge.
    Edge(SEdge),
    /// A required edge of a vertex.
    Required(Name, SEdge),
    /// An ordered edge.
    Ordering(SEdge),
    /// A constraint `(vertex, sort, value)`.
    Constraint(Name, Name, String),
    /// A key `(vertex, key)`.
    Key(Name, Name),
    /// A reference.
    Reference(Name),
}

/// A compiled check.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CheckKey {
    /// Kind.
    pub kind: CheckKind,
    /// Declaration.
    pub decl: SchemaRef,
}

/// Whether a check decides the status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Requirement {
    /// It does.
    Required,
    /// It is reported only.
    Advisory,
}

/// A finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// The anchor is not a vertex.
    UnknownAnchor(Name),
    /// The label is not an edge.
    UndeclaredEdge(SEdge),
    /// An endpoint is anchored elsewhere: `(source side?, expected, found)`.
    IncoherentEndpoint(bool, Name, Name),
    /// An endpoint is no node of the instance: `(source side?, node)`. An
    /// instance of the production shapes cannot hold such an edge; the
    /// reference model can, and reports it here rather than failing.
    DanglingEndpoint(bool, u32),
    /// A fan violation.
    MalformedFan(FanViolation),
    /// A required edge is missing.
    MissingRequiredEdge(SEdge),
    /// An out-degree out of bounds: `(edge, min, max, found, edges)`.
    OutDegree(Option<SEdge>, Option<u32>, Option<u32>, u32, Vec<ViewRef>),
    /// Edges sharing a position.
    DuplicatePosition(u32, Vec<u32>),
    /// A value outside its bound: `(sort, bound, value)`.
    ValueConstraint(Name, String, Value),
    /// A missing value (`None` the own value, else the field).
    MissingValue(Option<String>),
    /// A duplicated key: its columns and values.
    DuplicateKey(Vec<(String, Value)>),
    /// An element a `Within` key leaves outside its scope (advisory).
    OutsideKeyScope(Name, Name),
    /// A reference with no target.
    DanglingReference(Name, Vec<Value>),
    /// A reference with several targets.
    AmbiguousReference(Name, Vec<Value>, Vec<ViewRef>),
    /// A cycle: its strongly connected component.
    Cycle(Vec<ViewRef>),
}

/// A diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The check.
    pub check: CheckKey,
    /// Its requirement.
    pub requirement: Requirement,
    /// The subject.
    pub subject: Option<ViewRef>,
    /// The elements named, ascending.
    pub elements: Vec<ViewRef>,
    /// The finding.
    pub violation: Violation,
}

/// A three-valued status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckStatus {
    /// Every required check passed.
    Passed,
    /// A required check failed.
    Failed,
    /// No required failure, but a required check is undecided.
    Incomplete,
}

impl CheckStatus {
    /// Kleene strong conjunction: `Failed` if either is, else `Incomplete`
    /// if either is, else `Passed`.
    #[must_use]
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Failed, _) | (_, Self::Failed) => Self::Failed,
            (Self::Incomplete, _) | (_, Self::Incomplete) => Self::Incomplete,
            _ => Self::Passed,
        }
    }

    /// The fold of [`Self::and`] with unit `Passed`.
    #[must_use]
    pub fn all(items: impl IntoIterator<Item = Self>) -> Self {
        items.into_iter().fold(Self::Passed, Self::and)
    }
}

/// The status of a report: `Failed` iff a required diagnostic exists, else
/// `Incomplete` iff a required undecided check exists, else `Passed`.
#[must_use]
pub fn status(diagnostics: &[Diagnostic], required_incomplete: usize) -> CheckStatus {
    let failed = diagnostics
        .iter()
        .filter(|d| d.requirement == Requirement::Required)
        .map(|_| CheckStatus::Failed);
    let incomplete = (0..required_incomplete).map(|_| CheckStatus::Incomplete);
    CheckStatus::all(failed.chain(incomplete))
}

/// A validation report: diagnostics and undecided checks in check order,
/// and the status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// Diagnostics, sorted by check, subject and elements.
    pub diagnostics: Vec<Diagnostic>,
    /// Undecided (ill-formed) check instances with their detail.
    pub incomplete: Vec<(CheckKey, Requirement, Option<ViewRef>, String)>,
    /// The status.
    pub status: CheckStatus,
}

fn non_fan(g: &Graph, n: u32) -> bool {
    g.nodes
        .get(&n)
        .is_some_and(|x| x.shape != NodeShape::HyperEdge)
        && !g
            .edges
            .values()
            .any(|e| e.src == n && is_fan_kind(&e.label.kind))
}

/// The value sites of vertex `v` at node `n`.
///
/// Its own value when it is anchored at v, and for each `prop` edge e named c into v with `e.src`
/// its anchor and no out-edge labelled e, its field c. `(None, ..)` is the own value; a site is
/// `None` when absent or null-absent.
#[must_use]
pub fn value_sites(
    schema: &Schema,
    g: &Graph,
    v: &str,
    n: u32,
) -> Vec<(Option<String>, Option<Value>)> {
    let Some(node) = g.nodes.get(&n) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if node.anchor == v {
        let own = match &node.value {
            Some(Presence::Present(x)) => Some(x.clone()),
            _ => None,
        };
        out.push((None, own));
    }
    for e in schema
        .edges
        .iter()
        .filter(|e| e.kind == "prop" && e.tgt == v && e.src == node.anchor)
    {
        let Some(c) = e.name.as_ref() else { continue };
        if g.edges.values().any(|x| x.src == n && x.label == *e) {
            continue;
        }
        out.push((
            Some(c.clone()),
            node.fields.get(c).filter(|x| **x != Value::Null).cloned(),
        ));
    }
    out
}

fn semantics_of<'a>(sem: &'a BTreeMap<Name, Semantics>, sort: &str) -> Option<&'a Semantics> {
    sem.get(sort)
}

fn diag(
    kind: CheckKind,
    decl: SchemaRef,
    subject: Option<ViewRef>,
    mut elements: Vec<ViewRef>,
    violation: Violation,
) -> Diagnostic {
    elements.sort();
    elements.dedup();
    Diagnostic {
        check: CheckKey { kind, decl },
        requirement: Requirement::Required,
        subject,
        elements,
        violation,
    }
}

/// The structural kinds: anchors, edges, endpoints, fans, required edges
/// and positions.
fn structural(schema: &Schema, g: &Graph, out: &mut Vec<Diagnostic>) {
    for (id, n) in g.nodes.iter().filter(|(i, _)| non_fan(g, **i)) {
        if !schema.vertices.contains_key(&n.anchor) {
            out.push(diag(
                CheckKind::NodeAnchor,
                SchemaRef::Vertex(n.anchor.clone()),
                Some(ViewRef::Node(*id)),
                vec![ViewRef::Node(*id)],
                Violation::UnknownAnchor(n.anchor.clone()),
            ));
        }
    }
    for (id, e) in g.edges.iter().filter(|(_, e)| !is_fan_kind(&e.label.kind)) {
        let me = ViewRef::Edge(*id);
        if !schema.edges.contains(&e.label) {
            out.push(diag(
                CheckKind::EdgeDeclared,
                SchemaRef::Edge(e.label.clone()),
                Some(me.clone()),
                vec![me.clone()],
                Violation::UndeclaredEdge(e.label.clone()),
            ));
        }
        for (src_side, node, expected) in
            [(true, e.src, &e.label.src), (false, e.tgt, &e.label.tgt)]
        {
            let Some(found) = g.nodes.get(&node).map(|n| &n.anchor) else {
                out.push(diag(
                    CheckKind::EndpointCoherence,
                    SchemaRef::Edge(e.label.clone()),
                    Some(me.clone()),
                    vec![me.clone()],
                    Violation::DanglingEndpoint(src_side, node),
                ));
                continue;
            };
            if found != expected {
                let v = Violation::IncoherentEndpoint(src_side, expected.clone(), found.clone());
                out.push(diag(
                    CheckKind::EndpointCoherence,
                    SchemaRef::Edge(e.label.clone()),
                    Some(me.clone()),
                    vec![me.clone(), ViewRef::Node(node)],
                    v,
                ));
            }
        }
    }
    for f in fan_candidates(g) {
        let he = g.nodes[&f].anchor.clone();
        for v in violations_at(g, f, Some(schema)) {
            out.push(diag(
                CheckKind::FanWellFormed,
                SchemaRef::Vertex(he.clone()),
                Some(ViewRef::Node(f)),
                vec![ViewRef::Node(f)],
                Violation::MalformedFan(v),
            ));
        }
    }
    for (id, n) in g
        .nodes
        .iter()
        .filter(|(i, n)| non_fan(g, **i) && schema.vertices.contains_key(&n.anchor))
    {
        for req in schema.required.get(&n.anchor).into_iter().flatten() {
            if !g.edges.values().any(|e| e.src == *id && e.label == *req) {
                out.push(diag(
                    CheckKind::RequiredEdge,
                    SchemaRef::Required(n.anchor.clone(), req.clone()),
                    Some(ViewRef::Node(*id)),
                    vec![ViewRef::Node(*id)],
                    Violation::MissingRequiredEdge(req.clone()),
                ));
            }
        }
        for ord in schema.orderings.keys().filter(|o| o.src == n.anchor) {
            let mut by_pos: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for (eid, e) in g
                .edges
                .iter()
                .filter(|(_, e)| e.src == *id && e.label == *ord)
            {
                if let Some(p) = e.position {
                    by_pos.entry(p).or_default().push(*eid);
                }
            }
            for (p, edges) in by_pos.into_iter().filter(|(_, v)| v.len() > 1) {
                let mut elements: Vec<ViewRef> = edges.iter().map(|e| ViewRef::Edge(*e)).collect();
                elements.push(ViewRef::Node(*id));
                out.push(diag(
                    CheckKind::PositionDistinct,
                    SchemaRef::Ordering(ord.clone()),
                    Some(ViewRef::Node(*id)),
                    elements,
                    Violation::DuplicatePosition(p, edges),
                ));
            }
        }
    }
}

/// A value as an exact number (an integer, a big integer, a decimal, or a
/// finite float at its shortest decimal).
fn as_number(v: &Value) -> Option<DecimalValue> {
    match v {
        Value::Int(i) => DecimalValue::parse(&i.to_string()),
        Value::BigInt(b) => Some(crate::exact::ExactInteger::Big(b.clone()).as_decimal()),
        Value::Decimal(crate::exact::Decimal::Finite(d)) => Some(d.clone()),
        Value::Float(bits) => {
            let d = crate::float::shortest(crate::float::Format::B64, *bits)?;
            Some(DecimalValue {
                negative: d.negative,
                digits: d.coefficient,
                exponent: crate::num::Int::from_i64(d.exponent),
            })
        }
        _ => None,
    }
}

fn length(v: &Value, chars: bool) -> Option<u64> {
    let n = match (v, chars) {
        (Value::Str(s) | Value::Token(s), true) => s.chars().count(),
        (Value::Str(s) | Value::Token(s), false) => s.len(),
        (Value::Bytes(b), false) => b.len(),
        (Value::List(items), _) => items.len(),
        _ => return None,
    };
    u64::try_from(n).ok()
}

/// One value constraint at one value: `Some(true)` passes, `Some(false)`
/// fails, `None` undecidable for the value's kind.
fn passes(sem: &Semantics, bound: &str, v: &Value) -> Option<bool> {
    match sem {
        Semantics::MaxLength(chars) => Some(length(v, *chars)? <= bound.parse::<u64>().ok()?),
        Semantics::MinLength(chars) => Some(length(v, *chars)? >= bound.parse::<u64>().ok()?),
        Semantics::Minimum => Some(
            as_number(v)?
                .cmp_numeric(&DecimalValue::parse(bound)?)
                .is_ge(),
        ),
        Semantics::Maximum => Some(
            as_number(v)?
                .cmp_numeric(&DecimalValue::parse(bound)?)
                .is_le(),
        ),
        _ => Some(true),
    }
}

/// The constraint kinds: value bounds, required values and out-degrees.
fn constraints(
    schema: &Schema,
    sem: &BTreeMap<Name, Semantics>,
    g: &Graph,
    out: &mut Vec<Diagnostic>,
    inc: &mut Vec<(CheckKey, Requirement, Option<ViewRef>, String)>,
) {
    for (v, cs) in &schema.constraints {
        for (sort, bound) in cs {
            let Some(s) = semantics_of(sem, sort) else {
                continue;
            };
            let decl = SchemaRef::Constraint(v.clone(), sort.clone(), bound.clone());
            for (id, n) in g
                .nodes
                .iter()
                .filter(|(i, n)| non_fan(g, **i) && schema.vertices.contains_key(&n.anchor))
            {
                let me = ViewRef::Node(*id);
                match s {
                    Semantics::MinOutDegree | Semantics::MaxOutDegree if n.anchor == *v => {
                        let Ok(k) = bound.parse::<u32>() else {
                            inc.push((
                                CheckKey {
                                    kind: CheckKind::OutDegree,
                                    decl: decl.clone(),
                                },
                                Requirement::Required,
                                Some(me.clone()),
                                "ill-formed bound".into(),
                            ));
                            continue;
                        };
                        let counted: Vec<ViewRef> = g
                            .edges
                            .iter()
                            .filter(|(_, e)| e.src == *id && !is_fan_kind(&e.label.kind))
                            .map(|(i, _)| ViewRef::Edge(*i))
                            .collect();
                        let found = u32::try_from(counted.len()).unwrap_or(u32::MAX);
                        let (min, max) = if *s == Semantics::MinOutDegree {
                            (Some(k), None)
                        } else {
                            (None, Some(k))
                        };
                        if min.is_some_and(|lo| found < lo) || max.is_some_and(|hi| found > hi) {
                            let mut elements = counted.clone();
                            elements.push(me.clone());
                            out.push(diag(
                                CheckKind::OutDegree,
                                decl.clone(),
                                Some(me),
                                elements,
                                Violation::OutDegree(None, min, max, found, counted),
                            ));
                        }
                    }
                    Semantics::RequiredValue => {
                        for (field, site) in value_sites(schema, g, v, *id) {
                            if site.is_none() {
                                out.push(diag(
                                    CheckKind::RequiredValue,
                                    decl.clone(),
                                    Some(me.clone()),
                                    vec![me.clone()],
                                    Violation::MissingValue(field),
                                ));
                            }
                        }
                    }
                    Semantics::MaxLength(_)
                    | Semantics::MinLength(_)
                    | Semantics::Minimum
                    | Semantics::Maximum => {
                        for (_, site) in value_sites(schema, g, v, *id) {
                            let Some(x) = site else { continue };
                            match passes(s, bound, &x) {
                                Some(true) => {}
                                Some(false) => out.push(diag(
                                    CheckKind::ValueConstraint,
                                    decl.clone(),
                                    Some(me.clone()),
                                    vec![me.clone()],
                                    Violation::ValueConstraint(sort.clone(), bound.clone(), x),
                                )),
                                None => inc.push((
                                    CheckKey {
                                        kind: CheckKind::ValueConstraint,
                                        decl: decl.clone(),
                                    },
                                    Requirement::Required,
                                    Some(me.clone()),
                                    "not comparable".into(),
                                )),
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Key uniqueness and primary-key required parts.
fn keyed(schema: &Schema, g: &Graph, out: &mut Vec<Diagnostic>) {
    for (v, ks) in &schema.keys {
        for key in ks {
            let decl = SchemaRef::Key(v.clone(), key.id.clone());
            let (buckets, outside) = keys::buckets(g, v, key);
            for (values, elems) in buckets.into_values().filter(|(_, e)| e.len() > 1) {
                let cols: Vec<(String, Value)> = key
                    .parts
                    .iter()
                    .map(|p| match p {
                        crate::schema::KeyPart::Path(path) => path
                            .last()
                            .map(|e| e.column().to_owned())
                            .unwrap_or_default(),
                        crate::schema::KeyPart::Position { within } => within.column().to_owned(),
                    })
                    .zip(values)
                    .collect();
                out.push(diag(
                    CheckKind::KeyUnique,
                    decl.clone(),
                    None,
                    elems.into_iter().map(ViewRef::Node).collect(),
                    Violation::DuplicateKey(cols),
                ));
            }
            if let KeyScope::Within(scope) = &key.scope {
                for n in outside {
                    let mut d = diag(
                        CheckKind::KeyUnique,
                        decl.clone(),
                        Some(ViewRef::Node(n)),
                        vec![ViewRef::Node(n)],
                        Violation::OutsideKeyScope(key.id.clone(), scope.clone()),
                    );
                    d.requirement = Requirement::Advisory;
                    out.push(d);
                }
            }
            if key.role == KeyRole::Primary {
                for (id, _) in g
                    .nodes
                    .iter()
                    .filter(|(i, n)| n.anchor == *v && non_fan(g, **i))
                {
                    for p in &key.parts {
                        if let crate::schema::KeyPart::Path(path) = p
                            && keys::path_value(g, *id, path).is_none()
                        {
                            let field = path.last().map(|e| e.column().to_owned());
                            out.push(diag(
                                CheckKind::RequiredValue,
                                decl.clone(),
                                Some(ViewRef::Node(*id)),
                                vec![ViewRef::Node(*id)],
                                Violation::MissingValue(field),
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// Reference resolution on the syntactic shape: each holder's key value is
/// looked up among the targets' values of the target key.
fn references(schema: &Schema, g: &Graph, out: &mut Vec<Diagnostic>) -> Option<()> {
    for r in schema.references.values() {
        let RefSource::KeyValue { parts } = &r.source else {
            return None;
        };
        if r.cardinality == Cardinality::Many {
            return None;
        }
        let key = schema
            .keys
            .get(&r.target_vertex)?
            .iter()
            .find(|k| Some(&k.id) == r.target_key.as_ref())?;
        let requirement = if r.on_unresolved == OnUnresolved::Error {
            Requirement::Required
        } else {
            Requirement::Advisory
        };
        let decl = SchemaRef::Reference(r.id.clone());
        for (h, _) in g
            .nodes
            .iter()
            .filter(|(i, n)| n.anchor == r.holder && non_fan(g, **i))
        {
            let me = ViewRef::Node(*h);
            let values: Option<Vec<Value>> =
                parts.iter().map(|p| keys::path_value(g, *h, p)).collect();
            let mut push = |elements: Vec<ViewRef>, v: Violation| {
                let mut d = diag(
                    CheckKind::ReferenceResolves,
                    decl.clone(),
                    Some(me.clone()),
                    elements,
                    v,
                );
                d.requirement = requirement;
                out.push(d);
            };
            let Some(values) = values else {
                if !r.nullable {
                    let field = parts
                        .first()
                        .and_then(|p| p.last())
                        .map(|e| e.column().to_owned());
                    push(vec![me.clone()], Violation::MissingValue(field));
                }
                continue;
            };
            let candidates: Vec<ViewRef> = g
                .nodes
                .iter()
                .filter(|(i, n)| {
                    n.anchor == r.target_vertex
                        && non_fan(g, **i)
                        && keys::key_value(g, **i, key).as_ref() == Some(&values)
                })
                .map(|(i, _)| ViewRef::Node(*i))
                .collect();
            match candidates.len() {
                1 => {}
                0 => push(
                    vec![me.clone()],
                    Violation::DanglingReference(r.id.clone(), values),
                ),
                _ => {
                    let mut elements = candidates.clone();
                    elements.push(me.clone());
                    push(
                        elements,
                        Violation::AmbiguousReference(r.id.clone(), values, candidates),
                    );
                }
            }
        }
    }
    Some(())
}

/// Acyclicity of declared self-edges: every strongly connected component
/// of the subgraph of edges labelled `(v, v, name)` with an edge inside it
/// (a self-loop counts) is a cycle.
fn acyclic(schema: &Schema, sem: &BTreeMap<Name, Semantics>, g: &Graph, out: &mut Vec<Diagnostic>) {
    for (v, cs) in &schema.constraints {
        for (sort, name) in cs {
            if semantics_of(sem, sort) != Some(&Semantics::Acyclic) {
                continue;
            }
            let label = SEdge {
                src: v.clone(),
                tgt: v.clone(),
                kind: schema
                    .edges
                    .iter()
                    .find(|e| {
                        e.src == *v && e.tgt == *v && e.name.as_deref() == Some(name.as_str())
                    })
                    .map_or_else(String::new, |e| e.kind.clone()),
                name: Some(name.clone()),
            };
            let edges: Vec<(u32, u32)> = g
                .edges
                .values()
                .filter(|e| e.label == label)
                .map(|e| (e.src, e.tgt))
                .collect();
            let reach = |a: u32, b: u32| {
                let mut seen = BTreeSet::new();
                let mut stack = vec![a];
                while let Some(x) = stack.pop() {
                    for (s, t) in &edges {
                        if *s == x && seen.insert(*t) {
                            stack.push(*t);
                        }
                    }
                }
                seen.contains(&b)
            };
            let nodes: BTreeSet<u32> = edges.iter().flat_map(|(s, t)| [*s, *t]).collect();
            let mut done = BTreeSet::new();
            for n in &nodes {
                if done.contains(n) || !reach(*n, *n) {
                    continue;
                }
                let scc: Vec<ViewRef> = nodes
                    .iter()
                    .filter(|m| reach(*n, **m) && reach(**m, *n))
                    .map(|m| ViewRef::Node(*m))
                    .collect();
                for m in &scc {
                    if let ViewRef::Node(x) = m {
                        done.insert(*x);
                    }
                }
                out.push(diag(
                    CheckKind::Acyclic,
                    SchemaRef::Constraint(v.clone(), sort.clone(), name.clone()),
                    None,
                    scc.clone(),
                    Violation::Cycle(scc),
                ));
            }
        }
    }
}

/// Validate a graph.
///
/// Every kind's diagnostics, sorted by check, subject and elements, the undecided instances, and
/// the status. `None` when a constraint sort or a reference lies outside the reference's domain.
#[must_use]
pub fn validate(schema: &Schema, sem: &BTreeMap<Name, Semantics>, g: &Graph) -> Option<Report> {
    if schema
        .constraints
        .values()
        .flatten()
        .any(|(s, _)| sem.get(s).is_none_or(|x| *x == Semantics::Other))
    {
        return None;
    }
    let mut d = Vec::new();
    let mut inc = Vec::new();
    structural(schema, g, &mut d);
    constraints(schema, sem, g, &mut d, &mut inc);
    keyed(schema, g, &mut d);
    references(schema, g, &mut d)?;
    acyclic(schema, sem, g, &mut d);
    d.sort_by(|a, b| (&a.check, &a.subject, &a.elements).cmp(&(&b.check, &b.subject, &b.elements)));
    let required_incomplete = inc.iter().filter(|x| x.1 == Requirement::Required).count();
    let st = status(&d, required_incomplete);
    Some(Report {
        diagnostics: d,
        incomplete: inc,
        status: st,
    })
}

/// The admission policies over outcome bundles of required check instances.
///
/// `RequirePassed` admits exactly a result with no failed or incomplete bundle; `NoNewFailures`
/// admits iff every failed or incomplete bundle of the result was already open in the baseline with
/// equal bytes, where each instance of a baseline budget cut counts as open with the given virtual
/// incomplete bundle.
#[must_use]
pub fn admits<K: Ord>(
    require_passed: bool,
    baseline: &BTreeMap<K, Vec<u8>>,
    baseline_cut: &BTreeMap<K, Vec<u8>>,
    result: &BTreeMap<K, Vec<u8>>,
) -> bool {
    if require_passed {
        return result.is_empty();
    }
    result
        .iter()
        .all(|(k, bytes)| baseline.get(k).or_else(|| baseline_cut.get(k)) == Some(bytes))
}

/// The validation-side declared key equality check.
#[must_use]
pub fn key_equality(equality: &keys::KeyEquality, left: &Value, right: &Value) -> Option<bool> {
    keys::key_equivalence(equality, left, right)
}

/// Select a variable definition under first-defines or
/// nearest-preceding binding.
#[must_use]
pub fn variable_binding(
    definitions: &[(usize, String, u32)],
    occurrence_position: usize,
    name: &str,
    nearest_preceding: bool,
) -> Option<u32> {
    let matching = definitions
        .iter()
        .filter(|(position, candidate, _)| *position < occurrence_position && candidate == name);
    if nearest_preceding {
        matching
            .max_by_key(|(position, _, _)| *position)
            .map(|x| x.2)
    } else {
        matching
            .min_by_key(|(position, _, _)| *position)
            .map(|x| x.2)
    }
}

/// The declaration-side guard, target-set and sort-key forms are valid
/// exactly when all independently stated predicates pass.
#[must_use]
pub fn key_forms(predicates: &[bool]) -> bool {
    predicates.iter().all(|predicate| *predicate)
}

/// Parsing with an advisory format annotation has the same result as
/// parsing without the annotation.
#[must_use]
pub fn annotated_format_parse<T: Eq>(without: &Option<T>, with: &Option<T>) -> bool {
    without == with
}

/// Advisory format validation is exact: it reports precisely the
/// complement of the declared recognizer.
#[must_use]
pub fn annotated_format_advisory(
    text: &str,
    accepts: impl FnOnce(&str) -> bool,
    reports: bool,
) -> bool {
    reports != accepts(text)
}

/// An asserting format accepts exactly the values admitted by its
/// recognizer.
#[must_use]
pub fn format_assertion_acceptance(
    text: &str,
    accepts: impl FnOnce(&str) -> bool,
    accepted: bool,
) -> bool {
    accepted == accepts(text)
}
