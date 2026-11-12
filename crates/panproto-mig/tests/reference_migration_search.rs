//! Production migration search against the migration-search reference.
//!
//! Every ordered pair of schemas of the bounded scope is searched for total
//! morphisms and for spans under each request, and every answer is compared
//! with the reference: each returned morphism is checked against the two
//! schemas alone, its cost is recomputed term by term in exact arithmetic,
//! and the optimum is read off an enumeration of every assignment. The same
//! answers are rendered canonically and compared across insertion orders,
//! across processes, and with a pinned digest, and the bounded searches are
//! checked to refuse with the bound they spent rather than answer empty.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::process::Command;

use panproto_mig::hom_search::{
    DomainConstraints, FoundMorphism, SearchOptions, find_morphisms, find_morphisms_budgeted,
};
use panproto_mig::span::{SchemaSpan, SpanSearch};
use panproto_mig::{BuildError, CfnError, IsoError, LimitKind, SearchBudget, SpanError};
use panproto_reference::enumerate::migration_search::{schemas, schemas_count};
use panproto_reference::pages::migration_search::{
    self as reference, Assignment, Cost, Edge, EdgeMap, Optimum, Request, Schema, VertexMap,
};
use panproto_schema::{Protocol, SchemaBuilder};
use proptest::prelude::*;

fn protocol() -> Protocol {
    Protocol {
        name: "search".into(),
        schema_theory: "ThSearch".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        ..Default::default()
    }
}

/// The production schema of a reference schema, its vertices and edges added
/// in ascending order, or in descending order when `reversed` holds.
fn production(schema: &Schema, reversed: bool) -> panproto_schema::Schema {
    let proto = protocol();
    let mut vertices: Vec<(&String, &String)> = schema.vertices.iter().collect();
    let mut edges: Vec<&Edge> = schema.edges.iter().collect();
    if reversed {
        vertices.reverse();
        edges.reverse();
    }
    let mut builder = SchemaBuilder::new(&proto);
    for (id, kind) in vertices {
        builder = match builder.vertex(id, kind, None::<&str>) {
            Ok(next) => next,
            Err(error) => panic!("vertex {id} of the scope is refused: {error}"),
        };
    }
    for edge in edges {
        builder = match builder.edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref()) {
            Ok(next) => next,
            Err(error) => panic!("edge {edge:?} of the scope is refused: {error}"),
        };
    }
    match builder.build() {
        Ok(schema) => schema,
        Err(error) => panic!("a schema of the scope is refused: {error}"),
    }
}

fn reference_edge(edge: &panproto_schema::Edge) -> Edge {
    Edge::new(
        edge.src.as_str(),
        edge.tgt.as_str(),
        edge.kind.as_str(),
        edge.name.as_ref().map(panproto_gat::Name::as_str),
    )
}

fn vertex_map(
    map: &std::collections::HashMap<panproto_gat::Name, panproto_gat::Name>,
) -> VertexMap {
    map.iter()
        .map(|(from, to)| (from.to_string(), to.to_string()))
        .collect()
}

fn edge_map(
    map: &std::collections::HashMap<panproto_schema::Edge, panproto_schema::Edge>,
) -> EdgeMap {
    map.iter()
        .map(|(from, to)| (reference_edge(from), reference_edge(to)))
        .collect()
}

fn options(request: Request) -> SearchOptions {
    SearchOptions {
        monic: request.monic,
        epic: request.epic,
        iso: request.iso,
        ..SearchOptions::default()
    }
}

/// The quality production reports for a reference cost, by the formula
/// `1 - units / 10^9` evaluated in binary64.
fn reported_quality(cost: Cost) -> f64 {
    let Ok(units) = u32::try_from(cost.quality_units) else {
        panic!(
            "a quality cost of {} units is above the scale",
            cost.quality_units
        );
    };
    1.0 - f64::from(units) / 1.0e9
}

/// The total requests compared: the default, injective, surjective,
/// bijective and isomorphism searches.
const TOTAL_REQUESTS: [Request; 5] = [
    Request {
        monic: false,
        epic: false,
        iso: false,
    },
    Request {
        monic: true,
        epic: false,
        iso: false,
    },
    Request {
        monic: false,
        epic: true,
        iso: false,
    },
    Request {
        monic: true,
        epic: true,
        iso: false,
    },
    Request {
        monic: false,
        epic: false,
        iso: true,
    },
];

/// The span requests compared: the default, injective and induced searches.
const SPAN_REQUESTS: [Request; 3] = [TOTAL_REQUESTS[0], TOTAL_REQUESTS[1], TOTAL_REQUESTS[4]];

/// One total morphism as a reference assignment.
fn total_assignment(found: &FoundMorphism) -> Assignment {
    found
        .vertex_map
        .iter()
        .map(|(from, to)| (from.to_string(), Some(to.to_string())))
        .collect()
}

/// One span's right leg as a reference assignment over the source.
fn span_assignment(source: &Schema, span: &SchemaSpan) -> Assignment {
    let right = vertex_map(&span.right.vertex_map);
    source
        .vertices
        .keys()
        .map(|id| (id.clone(), right.get(id).cloned()))
        .collect()
}

fn reference_schema(schema: &panproto_schema::Schema) -> Schema {
    Schema {
        vertices: schema
            .vertices
            .iter()
            .map(|(id, vertex)| (id.to_string(), vertex.kind.to_string()))
            .collect(),
        edges: schema.edges.keys().map(reference_edge).collect(),
    }
}

/// Compare one total search with the reference.
fn assert_total_agrees(source: &Schema, target: &Schema, request: Request) {
    let (src, tgt) = (production(source, false), production(target, false));
    let answer = match find_morphisms(&src, &tgt, &options(request)) {
        Ok(answer) => answer,
        Err(error) => panic!("{source:?} -> {target:?} under {request:?} refused: {error}"),
    };
    let optimum = reference::optimal_total(source, target, request);
    let context = || format!("{source:?} -> {target:?} under {request:?}");
    assert!(!answer.truncated, "{}: truncated", context());
    let Some(Optimum { cost, assignments }) = optimum else {
        assert_eq!(
            answer.morphisms.len(),
            0,
            "{}: no morphism exists",
            context()
        );
        return;
    };
    assert!(
        !answer.morphisms.is_empty(),
        "{}: a morphism of cost {cost:?} exists",
        context()
    );
    let mut seen = BTreeSet::new();
    for found in &answer.morphisms {
        let vertices = vertex_map(&found.vertex_map);
        let edges = edge_map(&found.edge_map);
        assert_eq!(
            reference::check_total(source, target, request, &vertices, &edges),
            Ok(()),
            "{}: the morphism fails the check",
            context()
        );
        let assignment = total_assignment(found);
        assert_eq!(
            reference::cost(source, target, &assignment),
            Some(cost),
            "{}: the morphism is not optimal",
            context()
        );
        assert_eq!(
            found.quality.to_bits(),
            reported_quality(cost).to_bits(),
            "{}: reported quality",
            context()
        );
        assert!(
            seen.insert(assignment),
            "{}: a repeated morphism",
            context()
        );
    }
    if request == TOTAL_REQUESTS[0] {
        let expected: BTreeSet<Assignment> = assignments.into_iter().collect();
        assert_eq!(seen, expected, "{}: the optima", context());
    } else {
        assert_eq!(answer.morphisms.len(), 1, "{}: one answer", context());
    }
}

fn render_total(morphisms: &[FoundMorphism], truncated: bool) -> String {
    let mut text = format!("truncated={truncated};");
    for found in morphisms {
        let vertices = vertex_map(&found.vertex_map);
        let edges = edge_map(&found.edge_map);
        let _ = write!(
            text,
            "[{vertices:?}|{edges:?}|{:016x}]",
            found.quality.to_bits()
        );
    }
    text
}

/// Compare one span search with the reference.
fn assert_span_agrees(source: &Schema, target: &Schema, request: Request) {
    let proto = protocol();
    let (src, tgt) = (production(source, false), production(target, false));
    let span = match SpanSearch::new(&proto)
        .with_options(options(request))
        .run(&src, &tgt)
    {
        Ok(span) => span,
        Err(error) => panic!("{source:?} -> {target:?} under {request:?} refused: {error}"),
    };
    let context = || format!("{source:?} -> {target:?} under {request:?}");
    let Some(Optimum { cost, assignments }) = reference::optimal_span(source, target, request)
    else {
        panic!("{}: the reference has no span optimum", context());
    };
    let apex = reference_schema(&span.apex);
    assert_eq!(
        reference::check_span(
            source,
            target,
            request,
            &apex,
            (
                &vertex_map(&span.left.vertex_map),
                &edge_map(&span.left.edge_map)
            ),
            (
                &vertex_map(&span.right.vertex_map),
                &edge_map(&span.right.edge_map)
            ),
        ),
        Ok(()),
        "{}: the span fails the check",
        context()
    );
    let assignment = span_assignment(source, &span);
    assert!(span.certificate.proven_optimal, "{}: not proven", context());
    assert_eq!(
        reference::cost(source, target, &assignment),
        Some(cost),
        "{}: the span is not optimal",
        context()
    );
    assert!(
        assignments.contains(&assignment),
        "{}: an optimum",
        context()
    );
    assert_eq!(
        span.quality.to_bits(),
        reported_quality(cost).to_bits(),
        "{}: reported quality",
        context()
    );
    let coverage = if source.vertices.is_empty() {
        1.0
    } else {
        f64::from(u32::try_from(apex.vertices.len()).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(source.vertices.len()).unwrap_or(u32::MAX))
    };
    assert_eq!(
        span.apex_coverage.to_bits(),
        coverage.to_bits(),
        "{}: coverage",
        context()
    );
}

/// The pairs of the bounded scope: every ordered pair of schemas of at most
/// two vertices and two edges.
fn scope_pairs() -> Vec<(Schema, Schema)> {
    let family = schemas(2, 2);
    assert_eq!(family.len() as u64, schemas_count(2, 2));
    assert_eq!(family.len(), 468);
    let mut pairs = Vec::new();
    for source in &family {
        for target in &family {
            pairs.push((source.clone(), target.clone()));
        }
    }
    pairs
}

#[test]
fn total_search_returns_exactly_the_reference_optima_on_every_bounded_pair() {
    let pairs = scope_pairs();
    let mut compared = 0_usize;
    for (source, target) in &pairs {
        assert!(reference::terms_clear_of_ties(source, target));
        for request in TOTAL_REQUESTS {
            assert_total_agrees(source, target, request);
            compared += 1;
        }
    }
    println!("bound: vertices=2, edges=2; total searches compared: {compared}");
    assert_eq!(compared, 468 * 468 * 5);
}

#[test]
fn span_search_returns_a_reference_optimum_on_every_bounded_pair() {
    let family = schemas(2, 1);
    assert_eq!(family.len() as u64, schemas_count(2, 1));
    assert_eq!(family.len(), 126);
    let mut compared = 0_usize;
    for source in &family {
        for target in &family {
            for request in SPAN_REQUESTS {
                assert_span_agrees(source, target, request);
                compared += 1;
            }
        }
    }
    println!("bound: vertices=2, edges=1; span searches compared: {compared}");
    assert_eq!(compared, 126 * 126 * 3);
}

/// The canonical rendering of every total and span answer over the
/// determinism scope (every ordered pair of schemas of at most two vertices
/// and one edge), one line per pair and request, schemas built in the order
/// `reversed` selects.
fn answers(reversed: bool) -> String {
    let proto = protocol();
    let family = schemas(2, 1);
    assert_eq!(family.len() as u64, schemas_count(2, 1));
    let mut lines = Vec::new();
    for source in &family {
        for target in &family {
            let (src, tgt) = (production(source, reversed), production(target, reversed));
            for request in TOTAL_REQUESTS {
                let line = match find_morphisms(&src, &tgt, &options(request)) {
                    Ok(answer) => render_total(&answer.morphisms, answer.truncated),
                    Err(error) => format!("refused: {error}"),
                };
                lines.push(format!("{source:?}>{target:?}>{request:?}>{line}"));
            }
            for request in SPAN_REQUESTS {
                let line = match SpanSearch::new(&proto)
                    .with_options(options(request))
                    .run(&src, &tgt)
                {
                    Ok(span) => format!(
                        "[{:?}|{:?}|{:016x}|{:016x}|{:?}]",
                        vertex_map(&span.right.vertex_map),
                        edge_map(&span.right.edge_map),
                        span.quality.to_bits(),
                        span.apex_coverage.to_bits(),
                        span.certificate.tie_break_order,
                    ),
                    Err(error) => format!("refused: {error}"),
                };
                lines.push(format!("{source:?}>{target:?}>{request:?}>span>{line}"));
            }
        }
    }
    lines.join("\n")
}

/// The digest of [`answers`] on every platform, in every process and under
/// either insertion order.
const ANSWERS_DIGEST: u64 = 0x0fb3_2512_97d6_cab3;

/// How many separate processes the answers are compared across.
const PROCESSES: usize = 4;

/// The child half: one process's digest, printed.
#[test]
fn one_process_search_digest() {
    if std::env::var_os("PP_SEARCH_DIGEST").is_none() {
        return;
    }
    print!(
        "<<<{:016x}>>>",
        reference::digest(answers(false).as_bytes())
    );
}

fn child_digest(exe: &std::path::Path) -> String {
    let output = match Command::new(exe)
        .args(["one_process_search_digest", "--exact", "--nocapture"])
        .env("PP_SEARCH_DIGEST", "1")
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("the test binary does not re-run: {error}"),
    };
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let (Some(start), Some(end)) = (text.find("<<<"), text.find(">>>")) else {
        panic!("the child printed no digest: {text}");
    };
    text[start + 3..end].to_owned()
}

#[test]
fn search_answers_are_the_same_in_every_process_and_insertion_order() {
    if std::env::var_os("PP_SEARCH_DIGEST").is_some() {
        return;
    }
    let forward = answers(false);
    assert_eq!(forward, answers(true), "insertion order changed an answer");
    let here = format!("{:016x}", reference::digest(forward.as_bytes()));
    let Ok(exe) = std::env::current_exe() else {
        panic!("the test binary does not know its own path");
    };
    for _ in 0..PROCESSES {
        assert_eq!(
            child_digest(&exe),
            here,
            "a child process answered differently"
        );
    }
    assert_eq!(
        here,
        format!("{ANSWERS_DIGEST:016x}"),
        "the answers moved from the pinned digest"
    );
}

/// The budgets of the refusal scope: each bound lowered alone, to values on
/// both sides of what the scope's searches spend.
fn budgets() -> Vec<SearchBudget> {
    let mut out = vec![SearchBudget::default()];
    for nodes in [0, 1, 2, 4] {
        out.push(SearchBudget::default().with_max_nodes(Some(nodes)));
    }
    for ops in [0, 1, 8, 64, 512] {
        out.push(SearchBudget::default().with_op_budget(ops));
    }
    for bytes in [0, 64] {
        out.push(SearchBudget::default().with_mem_bytes(bytes));
    }
    out
}

/// Whether a stop on `limit` names a bound `budget` lowered.
fn names_a_lowered_bound(budget: &SearchBudget, limit: LimitKind) -> bool {
    let default = SearchBudget::default();
    match limit {
        LimitKind::Nodes => budget.max_nodes.is_some(),
        LimitKind::Operations => budget.op_budget < default.op_budget,
        LimitKind::Time => budget.max_millis.is_some(),
        _ => false,
    }
}

/// Whether a refusal is the memory bound refusing, under a lowered one: the
/// network's cost tables or the isomorphism search's frames over the budget.
fn memory_refusal(budget: &SearchBudget, error: &SpanError) -> bool {
    budget.mem_bytes < SearchBudget::default().mem_bytes
        && matches!(
            error,
            SpanError::Build {
                source: BuildError::Network {
                    source: CfnError::OverMemoryBudget { .. }
                }
            } | SpanError::Iso {
                source: IsoError::OverMemoryBudget { .. }
            }
        )
}

/// Compare one bounded total search with the reference, counting a stop or
/// refusal under its bound in `refusals`.
fn assert_bounded_total(
    source: &Schema,
    target: &Schema,
    request: Request,
    budget: &SearchBudget,
    refusals: &mut BTreeMap<String, usize>,
) {
    let (src, tgt) = (production(source, false), production(target, false));
    let context = format!("{source:?} -> {target:?} {request:?} {budget:?}");
    let exists = reference::optimal_total(source, target, request).is_some();
    match find_morphisms_budgeted(
        &src,
        &tgt,
        &options(request),
        &DomainConstraints::default(),
        budget,
    ) {
        Ok(answer) if answer.morphisms.is_empty() => {
            assert!(!exists, "{context}: an empty success");
        }
        Ok(answer) => {
            for found in &answer.morphisms {
                assert_eq!(
                    reference::check_total(
                        source,
                        target,
                        request,
                        &vertex_map(&found.vertex_map),
                        &edge_map(&found.edge_map),
                    ),
                    Ok(()),
                    "{context}"
                );
            }
        }
        Err(SpanError::Stopped { limit }) => {
            assert!(names_a_lowered_bound(budget, limit), "{context}: {limit:?}");
            *refusals.entry(format!("total {limit:?}")).or_insert(0) += 1;
        }
        Err(error) => {
            assert!(memory_refusal(budget, &error), "{context}: {error}");
            *refusals.entry("total memory".to_owned()).or_insert(0) += 1;
        }
    }
}

/// Compare one bounded span search with the reference, counting a stop or
/// refusal under its bound in `refusals`.
fn assert_bounded_span(
    source: &Schema,
    target: &Schema,
    request: Request,
    budget: &SearchBudget,
    refusals: &mut BTreeMap<String, usize>,
) {
    let proto = protocol();
    let (src, tgt) = (production(source, false), production(target, false));
    let context = format!("{source:?} -> {target:?} span {request:?} {budget:?}");
    match SpanSearch::new(&proto)
        .with_options(options(request))
        .with_budget(*budget)
        .run(&src, &tgt)
    {
        Ok(span) => {
            let apex = reference_schema(&span.apex);
            assert_eq!(
                reference::check_span(
                    source,
                    target,
                    request,
                    &apex,
                    (
                        &vertex_map(&span.left.vertex_map),
                        &edge_map(&span.left.edge_map)
                    ),
                    (
                        &vertex_map(&span.right.vertex_map),
                        &edge_map(&span.right.edge_map)
                    ),
                ),
                Ok(()),
                "{context}"
            );
            if let Some(limit) = span.certificate.limit_hit {
                assert!(!span.certificate.proven_optimal, "{context}");
                assert!(names_a_lowered_bound(budget, limit), "{context}: {limit:?}");
                *refusals.entry(format!("span {limit:?}")).or_insert(0) += 1;
            } else {
                assert!(span.certificate.proven_optimal, "{context}");
                let optimum =
                    reference::optimal_span(source, target, request).map(|optimum| optimum.cost);
                assert_eq!(
                    reference::cost(source, target, &span_assignment(source, &span)),
                    optimum,
                    "{context}: an unstopped span is optimal"
                );
            }
        }
        Err(error) => {
            assert!(memory_refusal(budget, &error), "{context}: {error}");
            *refusals.entry("span memory".to_owned()).or_insert(0) += 1;
        }
    }
}

/// The environment variable that selects one shard of the refusal scope.
const SHARD_VARIABLE: &str = "PANPROTO_TEST_SHARD";

/// One shard of the refusal scope split `count` ways: the ordered pairs whose
/// position in the enumeration is congruent to `index` modulo `count`. The
/// shards with indexes `0` to `count - 1` partition the pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shard {
    index: usize,
    count: usize,
}

impl Shard {
    /// Parse `index/count`: two unsigned decimal numerals with
    /// `0 <= index < count`.
    fn parse(value: &str) -> Result<Self, String> {
        let numeral = |part: &str, role: &str| {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(format!(
                    "the shard {role} {part:?} is not an unsigned decimal numeral"
                ));
            }
            part.parse::<usize>()
                .map_err(|error| format!("the shard {role} {part:?} does not fit: {error}"))
        };
        let (index, count) = value
            .split_once('/')
            .ok_or_else(|| format!("{value:?} is not of the form index/count"))?;
        let (index, count) = (numeral(index, "index")?, numeral(count, "count")?);
        if count == 0 {
            return Err(format!("{value:?} has a shard count of zero"));
        }
        if index >= count {
            return Err(format!(
                "{value:?} has shard index {index}, which is not below the shard count {count}"
            ));
        }
        Ok(Self { index, count })
    }

    /// The shard [`SHARD_VARIABLE`] names, or every pair when it is unset. A
    /// value that is not a well-formed shard panics, so a misconfigured job
    /// fails instead of checking the wrong pairs.
    fn from_environment() -> Self {
        match std::env::var(SHARD_VARIABLE) {
            Err(std::env::VarError::NotPresent) => Self { index: 0, count: 1 },
            Err(std::env::VarError::NotUnicode(value)) => {
                panic!("{SHARD_VARIABLE}={} is not valid Unicode", value.display())
            }
            Ok(value) => Self::parse(&value).unwrap_or_else(|error| {
                panic!("{SHARD_VARIABLE} must be index/count with index < count: {error}")
            }),
        }
    }

    /// How many of `total` positions fall in this shard.
    const fn size(self, total: usize) -> usize {
        (total + self.count - 1 - self.index) / self.count
    }
}

/// The searches of the refusal scope per ordered pair: twelve budgets under
/// five total and three span requests.
const SEARCHES_PER_PAIR: usize = 12 * 8;

/// Compare every bounded search of the pairs of the refusal scope in
/// `shard`, returning how many searches were compared.
fn compare_bounded_searches(shard: Shard) -> usize {
    let family = schemas(2, 1);
    assert_eq!(family.len(), 126);
    let mut compared = 0_usize;
    let mut refusals = BTreeMap::new();
    let pairs = family
        .iter()
        .flat_map(|source| family.iter().map(move |target| (source, target)));
    for (position, (source, target)) in pairs.enumerate() {
        if position % shard.count != shard.index {
            continue;
        }
        for budget in budgets() {
            for request in TOTAL_REQUESTS {
                assert_bounded_total(source, target, request, &budget, &mut refusals);
                compared += 1;
            }
            for request in SPAN_REQUESTS {
                assert_bounded_span(source, target, request, &budget, &mut refusals);
                compared += 1;
            }
        }
    }
    println!(
        "bound: vertices=2, edges=1, budgets=12, shard={}/{}; searches compared: {compared}",
        shard.index, shard.count
    );
    println!("refusals and stops by bound: {refusals:?}");
    compared
}

#[test]
fn a_bounded_search_answers_or_names_its_bound_on_every_bounded_pair() {
    let shard = Shard::from_environment();
    let compared = compare_bounded_searches(shard);
    assert_eq!(compared, shard.size(126 * 126) * SEARCHES_PER_PAIR);
    if shard.count == 1 {
        assert_eq!(compared, 1_524_096);
    }
}

#[test]
fn bounded_searches_on_one_sixteenth_of_the_refusal_scope_answer_or_name_their_bound() {
    let shard = Shard {
        index: 0,
        count: 16,
    };
    assert_eq!(shard.size(126 * 126), 993);
    assert_eq!(compare_bounded_searches(shard), 993 * SEARCHES_PER_PAIR);
}

#[test]
fn the_refusal_scope_shards_partition_its_pairs() {
    let total = 126 * 126;
    for count in [1, 4, 16] {
        let sizes: usize = (0..count)
            .map(|index| Shard { index, count }.size(total))
            .sum();
        assert_eq!(sizes, total, "the {count} shards cover every pair once");
    }
    assert_eq!(Shard::parse("3/4"), Ok(Shard { index: 3, count: 4 }));
    assert!(Shard::parse("4/4").is_err());
    assert!(Shard::parse("0/0").is_err());
    assert!(Shard::parse("+1/4").is_err());
    assert!(Shard::parse("1").is_err());
}

fn schema_strategy() -> impl Strategy<Value = Schema> {
    let ids = ["a", "ab", "b", "ba"];
    let vertices = prop::collection::btree_map(
        prop::sample::select(ids.to_vec()),
        prop::sample::select(vec!["object", "string"]),
        1..=4,
    );
    vertices.prop_flat_map(move |vertices| {
        let names: Vec<String> = vertices.keys().map(|id| (*id).to_owned()).collect();
        let kinds: BTreeMap<String, String> = vertices
            .iter()
            .map(|(id, kind)| ((*id).to_owned(), (*kind).to_owned()))
            .collect();
        let edge = if names.is_empty() {
            Just(None).boxed()
        } else {
            (
                prop::sample::select(names.clone()),
                prop::sample::select(names),
                prop::sample::select(vec!["prop", "item"]),
                prop::sample::select(vec![None, Some("n"), Some("m")]),
            )
                .prop_map(|(src, tgt, kind, name)| Some(Edge::new(&src, &tgt, kind, name)))
                .boxed()
        };
        prop::collection::vec(edge, 0..=4).prop_map(move |edges| Schema {
            vertices: kinds.clone(),
            edges: edges.into_iter().flatten().collect(),
        })
    })
}

fn budget_strategy() -> impl Strategy<Value = SearchBudget> {
    prop_oneof![
        (0_u64..=8).prop_map(|nodes| SearchBudget::default().with_max_nodes(Some(nodes))),
        (0_u64..=1024).prop_map(|ops| SearchBudget::default().with_op_budget(ops)),
        (0_usize..=256).prop_map(|bytes| SearchBudget::default().with_mem_bytes(bytes)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_total_searches_return_the_reference_optima(
        source in schema_strategy(),
        target in schema_strategy(),
    ) {
        prop_assume!(reference::terms_clear_of_ties(&source, &target));
        for request in TOTAL_REQUESTS {
            assert_total_agrees(&source, &target, request);
        }
    }

    #[test]
    fn randomized_span_searches_return_a_reference_optimum(
        source in schema_strategy(),
        target in schema_strategy(),
    ) {
        prop_assume!(reference::terms_clear_of_ties(&source, &target));
        for request in SPAN_REQUESTS {
            assert_span_agrees(&source, &target, request);
        }
    }

    #[test]
    fn randomized_insertion_orders_give_the_same_answers(
        source in schema_strategy(),
        target in schema_strategy(),
    ) {
        let proto = protocol();
        for reversed in [(false, true), (true, false), (true, true)] {
            let forward = (production(&source, false), production(&target, false));
            let other = (production(&source, reversed.0), production(&target, reversed.1));
            for request in TOTAL_REQUESTS {
                let render = |pair: &(panproto_schema::Schema, panproto_schema::Schema)| {
                    match find_morphisms(&pair.0, &pair.1, &options(request)) {
                        Ok(answer) => render_total(&answer.morphisms, answer.truncated),
                        Err(error) => format!("refused: {error}"),
                    }
                };
                prop_assert_eq!(render(&forward), render(&other));
            }
            for request in SPAN_REQUESTS {
                let render = |pair: &(panproto_schema::Schema, panproto_schema::Schema)| {
                    match SpanSearch::new(&proto)
                        .with_options(options(request))
                        .run(&pair.0, &pair.1)
                    {
                        Ok(span) => format!(
                            "{:?}|{:?}|{:016x}",
                            vertex_map(&span.right.vertex_map),
                            edge_map(&span.right.edge_map),
                            span.quality.to_bits()
                        ),
                        Err(error) => format!("refused: {error}"),
                    }
                };
                prop_assert_eq!(render(&forward), render(&other));
            }
        }
    }

    #[test]
    fn randomized_bounded_searches_answer_or_name_their_bound(
        source in schema_strategy(),
        target in schema_strategy(),
        budget in budget_strategy(),
    ) {
        let mut refusals = BTreeMap::new();
        for request in TOTAL_REQUESTS {
            assert_bounded_total(&source, &target, request, &budget, &mut refusals);
        }
        for request in SPAN_REQUESTS {
            assert_bounded_span(&source, &target, request, &budget, &mut refusals);
        }
    }
}
