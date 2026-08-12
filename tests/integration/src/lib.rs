//! Integration test support library for panproto.
//!
//! This crate holds the shared `proptest` strategies the integration
//! tests draw from. It is a dev-only crate and is not published.
//!
//! The generators here are the repository's single source of random
//! [`Schema`] values. A test binary that needs a random schema imports
//! one of these rather than defining its own, so a shape that breaks
//! one law is reachable from every property test and a fix to the
//! generator benefits all of them at once.
//!
//! Six families live here:
//!
//! 1. [`arb_schema`] and [`arb_schema_pair`]: general graphs over the
//!    permissive [`open_protocol`], for laws that hold of any schema.
//! 2. [`arb_schema_rich`]: one schema per draw with all 21 [`Schema`]
//!    fields populated, for anything that must account for every field
//!    rather than the structural core.
//! 3. [`arb_small_schema_pair`]: the oracle generator, bounded so the
//!    whole assignment space of a morphism search can be enumerated.
//! 4. [`arb_cost_weights`]: normalised weight vectors on a rational
//!    grid.
//! 5. [`arb_small_cfn_instance`]: a small schema pair together with the
//!    cost function network built over it, for checking a search path
//!    against exhaustive enumeration.
//! 6. [`arb_scored_pair`]: a schema pair admitting at least one total
//!    morphism, for the properties that quantify over morphisms rather
//!    than over assignments.
//!
//! [`enumerate_total_morphisms`] and the helpers around it are shared for
//! the same reason the generators are: the notion of "is a morphism" a
//! test uses has to be the one the network encodes, or a disagreement
//! between them reads as a failure of whatever is under test.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use panproto_gat::Name;
use panproto_mig::solve::build::{Evidence, build_cfn, edge_image};
use panproto_mig::solve::oracle::{MAX_ORACLE_ASSIGNMENTS, assignment_count};
use panproto_mig::{
    Assignment, Cfn, CostWeights, DomainConstraints, SearchOptions, SortLensWitness,
    default_witness_library,
};
use panproto_schema::{
    CoercionSpec, Edge, EdgeRule, Protocol, RecursionPoint, Schema, SchemaBuilder, Span, UsageMode,
    Variant,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Protocols
// ---------------------------------------------------------------------------

/// A protocol that accepts any vertex kind and any edge kind.
///
/// Being permissive lets a generator explore arbitrary schemas without
/// the builder rejecting them for protocol-mismatch reasons unrelated
/// to the properties under test.
#[must_use]
pub fn open_protocol() -> Protocol {
    Protocol {
        name: "test-open".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        }],
        obj_kinds: vec!["object".into(), "string".into(), "ref".into()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// The vertex kinds [`arb_small_schema_pair`] draws from.
const SMALL_KINDS: [&str; 3] = ["object", "string", "integer"];

/// The edge kinds [`arb_small_schema_pair`] draws from.
const SMALL_EDGE_KINDS: [&str; 2] = ["prop", "item"];

/// The protocol [`arb_small_schema_pair`] builds both of its schemas
/// over.
///
/// Both edge rules leave `src_kinds` and `tgt_kinds` empty, so the
/// builder accepts any endpoint pairing and the only rejections the
/// generator has to filter are duplicate edges.
#[must_use]
pub fn small_protocol() -> Protocol {
    Protocol {
        name: "test-small".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        edge_rules: SMALL_EDGE_KINDS
            .iter()
            .map(|k| EdgeRule {
                edge_kind: (*k).to_owned(),
                src_kinds: vec![],
                tgt_kinds: vec![],
            })
            .collect(),
        obj_kinds: SMALL_KINDS.iter().map(|k| (*k).to_owned()).collect(),
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// The vertex kinds [`arb_schema_rich`] draws from.
const RICH_KINDS: [&str; 5] = ["object", "string", "integer", "union", "array"];

/// The edge kinds [`arb_schema_rich`] draws from.
const RICH_EDGE_KINDS: [&str; 3] = ["prop", "item", "variant"];

/// The constraint sorts [`arb_schema_rich`] draws from, and the key
/// space of the `policies` field it populates.
const RICH_SORTS: [&str; 3] = ["maxLength", "format", "minimum"];

/// The protocol [`arb_schema_rich`] builds over.
///
/// Every structural and enrichment feature flag is set, and every
/// vertex kind, edge kind, and constraint sort the generator can emit
/// is declared, so [`panproto_schema::validate`] reports nothing for a
/// schema the generator produced.
#[must_use]
pub fn rich_protocol() -> Protocol {
    Protocol {
        name: "test-rich".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        edge_rules: RICH_EDGE_KINDS
            .iter()
            .map(|k| EdgeRule {
                edge_kind: (*k).to_owned(),
                src_kinds: vec![],
                tgt_kinds: vec![],
            })
            .collect(),
        obj_kinds: RICH_KINDS.iter().map(|k| (*k).to_owned()).collect(),
        constraint_sorts: RICH_SORTS.iter().map(|s| (*s).to_owned()).collect(),
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        has_causal: true,
        nominal_identity: true,
        has_defaults: true,
        has_coercions: true,
        has_mergers: true,
        has_policies: true,
        ..Protocol::default()
    }
}

// ---------------------------------------------------------------------------
// General schema generators
// ---------------------------------------------------------------------------

/// A vertex kind recognised by [`open_protocol`].
pub fn arb_kind() -> impl Strategy<Value = &'static str> {
    prop_oneof!(Just("object"), Just("string"), Just("ref"))
}

/// A built schema with between 1 and 6 vertices, an arbitrary subset
/// of edges between them, and an arbitrary (possibly empty) subset of
/// vertex ids flagged as entries.
///
/// Shapes the builder rejects (a duplicate edge, for instance) are
/// discarded by the `prop_filter_map` rather than surfacing as a
/// failure.
pub fn arb_schema() -> impl Strategy<Value = Schema> {
    // 1..=6 vertices.
    (1usize..=6)
        .prop_flat_map(|n| {
            // Per-vertex kind.
            let kinds = prop::collection::vec(arb_kind(), n);
            // Edges: bounded subset of (src_idx, tgt_idx, name?).
            let edges = prop::collection::vec((0..n, 0..n, prop::option::of(0u32..5)), 0..=n * 2);
            // Entries: subset of vertex indices.
            let entry_idxs = prop::collection::vec(0..n, 0..=n);

            (kinds, edges, entry_idxs)
        })
        .prop_filter_map("unbuildable schema shape", |(kinds, edges, entry_idxs)| {
            let proto = open_protocol();
            let mut b = SchemaBuilder::new(&proto);
            for (i, k) in kinds.iter().enumerate() {
                b = b.vertex(&format!("v{i}"), k, None).ok()?;
            }
            let mut seen_edges = HashSet::new();
            for (s, t, name_idx) in edges {
                let src = format!("v{s}");
                let tgt = format!("v{t}");
                let name = name_idx.map(|n| format!("e{n}"));
                // Avoid the builder's DuplicateEdge rejection by
                // tracking (src, tgt, name) locally.
                if !seen_edges.insert((src.clone(), tgt.clone(), name.clone())) {
                    continue;
                }
                b = b.edge(&src, &tgt, "prop", name.as_deref()).ok()?;
            }
            let mut seen_entries = HashSet::new();
            for i in entry_idxs {
                let v = format!("v{i}");
                if seen_entries.insert(v.clone()) {
                    b = b.entry(&v);
                }
            }
            b.build().ok()
        })
}

/// Two independently sampled schemas over one protocol.
///
/// Both are built by [`arb_schema`], so both carry the
/// [`open_protocol`] name and both draw vertex kinds from the same
/// three-element alphabet. That shared alphabet is what makes the pair
/// usable: a morphism search compares source and target vertices by
/// kind, so a pair drawn over two different protocols would be
/// dominated by kind mismatches rather than by the structure under
/// test.
pub fn arb_schema_pair() -> impl Strategy<Value = (Schema, Schema)> {
    (arb_schema(), arb_schema())
}

// ---------------------------------------------------------------------------
// The rich generator
// ---------------------------------------------------------------------------

/// The raw draw behind [`arb_schema_rich`], in field order: per-vertex
/// kinds, per-vertex NSID flags, `(src, tgt, kind, name)` edge tuples,
/// `(vertex, sort, value)` constraint tuples, entry indices, per-vertex
/// selectors, and per-edge selectors.
type RichShape = (
    Vec<&'static str>,
    Vec<bool>,
    Vec<(usize, usize, &'static str, Option<&'static str>)>,
    Vec<(usize, &'static str, u16)>,
    Vec<usize>,
    Vec<u8>,
    Vec<u8>,
);

/// A schema over [`rich_protocol`] with every one of the 21 [`Schema`]
/// fields populated non-trivially on every draw.
///
/// The split between the two halves of the construction is dictated by
/// [`SchemaBuilder`]:
///
/// * The builder sets `protocol`, `vertices`, `edges`, `hyper_edges`,
///   `constraints`, `required`, `nsids`, `entries`, `coercions`,
///   `mergers`, `defaults`, and `policies`, and `build()` derives
///   `outgoing`, `incoming`, and `between` from the accumulated edges.
/// * `variants`, `orderings`, `recursion_points`, `spans`,
///   `usage_modes`, and `nominal` have no builder method, so they are
///   written onto the finished [`Schema`] afterwards. Every name they
///   reference is a vertex id or an edge that survived `build()`, so a
///   sub-schema cut is free to drop them by referential reachability
///   without hitting a dangling key.
///
/// No field is left empty. The four expression-valued fields
/// (`coercions`, `mergers`, `defaults`, `policies`) take their values
/// from [`panproto_mig::default_witness_library`], whose forward and
/// inverse expressions are the real carrier conversions the coercion
/// machinery uses; `panproto-expr` is not a dependency of this crate,
/// so the witness library is also the only handle on `Expr` values
/// available here.
pub fn arb_schema_rich() -> impl Strategy<Value = (Protocol, Schema)> {
    arb_rich_shape().prop_filter_map("unbuildable rich schema shape", |shape| {
        let protocol = rich_protocol();
        let mut schema = build_rich_core(&protocol, &shape)?;
        decorate_rich(&mut schema, &shape.5, &shape.6);
        Some((protocol, schema))
    })
}

/// Draw the raw shape behind [`arb_schema_rich`].
///
/// Every collection has a non-zero lower bound, which is what makes
/// the corresponding [`Schema`] field non-empty on every draw.
fn arb_rich_shape() -> impl Strategy<Value = RichShape> {
    (2usize..=6).prop_flat_map(|n| {
        (
            prop::collection::vec(prop::sample::select(RICH_KINDS.as_slice()), n),
            prop::collection::vec(any::<bool>(), n),
            prop::collection::vec(
                (
                    0..n,
                    0..n,
                    prop::sample::select(RICH_EDGE_KINDS.as_slice()),
                    prop::option::of(prop::sample::select(["a", "b"].as_slice())),
                ),
                1..=8,
            ),
            prop::collection::vec(
                (0..n, prop::sample::select(RICH_SORTS.as_slice()), 0u16..64),
                1..=6,
            ),
            prop::collection::vec(0..n, 1..=n),
            prop::collection::vec(0u8..8, n),
            prop::collection::vec(0u8..8, 8),
        )
    })
}

/// Build the twelve builder-settable fields plus the three derived
/// adjacency indices.
///
/// Returns `None` when the sampled shape is unbuildable, which the
/// caller's `prop_filter_map` discards.
fn build_rich_core(protocol: &Protocol, shape: &RichShape) -> Option<Schema> {
    let (kinds, nsid_flags, raw_edges, raw_constraints, entry_idxs, ..) = shape;
    let mut b = SchemaBuilder::new(protocol);

    for (i, kind) in kinds.iter().enumerate() {
        // Vertex 0 always carries an NSID so `nsids` is never empty.
        let nsid = (i == 0 || nsid_flags.get(i).copied().unwrap_or(false))
            .then(|| format!("test.ns.v{i}"));
        b = b.vertex(&format!("v{i}"), kind, nsid.as_deref()).ok()?;
    }

    let edges = distinct_edges(raw_edges);
    if edges.is_empty() {
        return None;
    }
    for e in &edges {
        b = b
            .edge(e.src.as_ref(), e.tgt.as_ref(), e.kind.as_ref(), name_of(e))
            .ok()?;
        // Each edge is required at its source, so `required` is
        // non-empty and every entry in it names a live edge.
        b = b.required(e.src.as_ref(), vec![e.clone()]);
    }

    for (v, sort, value) in raw_constraints {
        b = b.constraint(&format!("v{v}"), sort, &value.to_string());
    }

    // A single hyper-edge over the two lowest-numbered vertices; the
    // shape draws at least two, so both endpoints always exist.
    let signature: HashMap<String, String> = [
        ("head".to_owned(), "v0".to_owned()),
        ("tail".to_owned(), "v1".to_owned()),
    ]
    .into_iter()
    .collect();
    b = b.hyper_edge("h0", "hedge", signature, "head").ok()?;

    let mut seen_entries = HashSet::new();
    for i in entry_idxs {
        let v = format!("v{i}");
        if seen_entries.insert(v.clone()) {
            b = b.entry(&v);
        }
    }

    b = attach_rich_expressions(b, kinds)?;
    b.build().ok()
}

/// Populate `coercions`, `mergers`, `defaults`, and `policies`.
///
/// Values cycle through [`panproto_mig::default_witness_library`], so
/// the expressions are real and the assignment is a deterministic
/// function of the draw.
fn attach_rich_expressions(mut b: SchemaBuilder, kinds: &[&'static str]) -> Option<SchemaBuilder> {
    let pool: Vec<SortLensWitness> = default_witness_library().iter().cloned().collect();

    // `coercions` is keyed by a pair of vertex *kinds*, so the keys are
    // drawn from the kinds actually present, sorted for reproducibility.
    let mut present: Vec<&str> = kinds.to_vec();
    present.sort_unstable();
    present.dedup();
    for (i, source) in present.iter().enumerate() {
        let target = cycle(&present, i + 1)?;
        let w = cycle(&pool, i)?;
        b = b.coercion(
            source,
            target,
            CoercionSpec {
                forward: w.forward.clone(),
                inverse: w.inverse.clone(),
                class: w.class,
            },
        );
    }

    for i in 0..kinds.len() {
        let vertex = format!("v{i}");
        b = b.merger(&vertex, cycle(&pool, i)?.forward.clone());
        b = b.default_expr(&vertex, cycle(&pool, i + 1)?.forward.clone());
    }

    // `policies` is keyed by constraint sort name, not by vertex id.
    for (i, sort) in RICH_SORTS.iter().enumerate() {
        b = b.policy(sort, cycle(&pool, i + 2)?.forward.clone());
    }

    Some(b)
}

/// Write the six fields [`SchemaBuilder`] cannot set.
///
/// Vertex ids and edges are sorted before use so the result is a
/// function of the draw rather than of `HashMap` iteration order.
fn decorate_rich(schema: &mut Schema, vertex_selectors: &[u8], edge_selectors: &[u8]) {
    let mut ids: Vec<Name> = schema.vertices.keys().cloned().collect();
    ids.sort_unstable();
    let mut edges: Vec<Edge> = schema.edges.keys().cloned().collect();
    edges.sort_unstable();

    let Some(parent) = ids.first().cloned() else {
        return;
    };

    // The lowest-numbered vertex is the coproduct; every other vertex
    // is one of its injections. Both `id` and `parent_vertex` are
    // vertex ids, per the key-space table of
    // [`panproto_schema::induce`]: `id` names the injection vertex and
    // `parent_vertex` names the coproduct, which is also the map key.
    // Anything else makes an arm dangle, and a cut then drops every arm
    // rather than the ones whose vertices left.
    let variants: Vec<Variant> = ids
        .iter()
        .skip(1)
        .enumerate()
        .map(|(i, id)| Variant {
            id: id.clone(),
            parent_vertex: parent.clone(),
            tag: (i % 2 == 0).then(|| Name::from(format!("t{i}"))),
        })
        .collect();
    schema.variants.insert(parent, variants);

    for (i, edge) in edges.iter().enumerate() {
        let sel = edge_selectors.get(i).copied().unwrap_or(0);
        schema.orderings.insert(edge.clone(), u32::from(sel));
        schema.usage_modes.insert(
            edge.clone(),
            match sel % 3 {
                0 => UsageMode::Structural,
                1 => UsageMode::Linear,
                _ => UsageMode::Affine,
            },
        );
    }

    for (i, id) in ids.iter().enumerate() {
        let sel = vertex_selectors.get(i).copied().unwrap_or(0);
        schema.nominal.insert(id.clone(), sel % 2 == 0);

        // Each vertex is a fixpoint marker unfolding to its successor,
        // cyclically, so both ends of every recursion point and every
        // span are live vertices.
        let Some(next) = cycle(&ids, i + 1).cloned() else {
            continue;
        };
        schema.recursion_points.insert(
            id.clone(),
            RecursionPoint {
                mu_id: id.clone(),
                target_vertex: next.clone(),
            },
        );
        let span_id = Name::from(format!("sp{i}"));
        schema.spans.insert(
            span_id.clone(),
            Span {
                id: span_id,
                left: id.clone(),
                right: next,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// The oracle generator
// ---------------------------------------------------------------------------

/// The raw draw behind one side of [`arb_small_schema_pair`]:
/// per-vertex kinds and `(src, tgt, kind, name)` edge tuples.
type SmallShape = (
    Vec<&'static str>,
    Vec<(usize, usize, &'static str, Option<&'static str>)>,
);

/// A protocol together with a source and a target schema small enough
/// that a morphism search's whole assignment space can be enumerated.
///
/// The source has 1 to 5 vertices and the target 1 to 4, so the number
/// of assignments including the "unmapped" value on each variable is
/// bounded by `(|V_t| + 1)^{|V_s|} <= 5^5 = 3125`. Each side draws 0 to
/// 6 edges with kinds from `{"prop", "item"}` and names from
/// `{None, Some("a"), Some("b")}`, and both are built through
/// [`SchemaBuilder`] over [`small_protocol`], with duplicate-edge and
/// unknown-edge-kind shapes discarded rather than reported.
pub fn arb_small_schema_pair() -> impl Strategy<Value = (Protocol, Schema, Schema)> {
    (arb_small_shape(5), arb_small_shape(4)).prop_filter_map(
        "unbuildable small schema shape",
        |(src_shape, tgt_shape)| {
            let protocol = small_protocol();
            let src = build_small(&protocol, &src_shape)?;
            let tgt = build_small(&protocol, &tgt_shape)?;
            Some((protocol, src, tgt))
        },
    )
}

/// Draw one side of [`arb_small_schema_pair`] with at most
/// `max_vertices` vertices.
fn arb_small_shape(max_vertices: usize) -> impl Strategy<Value = SmallShape> {
    (1usize..=max_vertices).prop_flat_map(|n| {
        (
            prop::collection::vec(prop::sample::select(SMALL_KINDS.as_slice()), n),
            prop::collection::vec(
                (
                    0..n,
                    0..n,
                    prop::sample::select(SMALL_EDGE_KINDS.as_slice()),
                    prop::option::of(prop::sample::select(["a", "b"].as_slice())),
                ),
                0..=6,
            ),
        )
    })
}

/// Build one side of [`arb_small_schema_pair`], returning `None` for a
/// shape the builder rejects.
fn build_small(protocol: &Protocol, shape: &SmallShape) -> Option<Schema> {
    let (kinds, raw_edges) = shape;
    let mut b = SchemaBuilder::new(protocol);
    for (i, kind) in kinds.iter().enumerate() {
        b = b.vertex(&format!("v{i}"), kind, None).ok()?;
    }
    for (s, t, kind, name) in raw_edges {
        b = b
            .edge(&format!("v{s}"), &format!("v{t}"), kind, *name)
            .ok()?;
    }
    b.build().ok()
}

// ---------------------------------------------------------------------------
// Cost weights
// ---------------------------------------------------------------------------

/// One component of [`arb_cost_weights`], on the rational grid
/// `{0, 0.25, 0.5, 0.75, 1}`.
fn arb_weight_component() -> impl Strategy<Value = f64> {
    prop_oneof!(
        Just(0.0_f64),
        Just(0.25_f64),
        Just(0.5_f64),
        Just(0.75_f64),
        Just(1.0_f64)
    )
}

/// Five component weights drawn from the rational grid
/// `{0, 0.25, 0.5, 0.75, 1}` and normalised to sum to 1.
///
/// The all-zero draw is filtered out, so the pre-normalisation total is
/// at least `0.25` and the division is never by zero. Drawing from a
/// finite grid of exactly representable binary fractions rather than
/// from arbitrary `f64` keeps a shrunk counterexample legible and keeps
/// the normalisation reproducible bit for bit.
pub fn arb_cost_weights() -> impl Strategy<Value = [f64; 5]> {
    (
        arb_weight_component(),
        arb_weight_component(),
        arb_weight_component(),
        arb_weight_component(),
        arb_weight_component(),
    )
        .prop_map(<[f64; 5]>::from)
        .prop_filter("an all-zero weight vector cannot be normalised", |w| {
            w.iter().sum::<f64>() > 0.0
        })
        .prop_map(|w| {
            let total: f64 = w.iter().sum();
            w.map(|x| x / total)
        })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Index `items` cyclically, returning `None` only when it is empty.
fn cycle<T>(items: &[T], i: usize) -> Option<&T> {
    items.get(i % items.len().max(1))
}

/// The optional edge label as a `&str`, for handing back to
/// [`SchemaBuilder::edge`].
fn name_of(edge: &Edge) -> Option<&str> {
    edge.name.as_ref().map(AsRef::as_ref)
}

/// Turn `(src, tgt, kind, name)` index tuples into [`Edge`] values,
/// dropping repeats.
///
/// A repeated tuple would make [`SchemaBuilder::edge`] return
/// `DuplicateEdge` and cost the whole sample; skipping it instead keeps
/// the rejection rate low enough that the edge list is never empty.
fn distinct_edges(raw: &[(usize, usize, &'static str, Option<&'static str>)]) -> Vec<Edge> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(raw.len());
    for (s, t, kind, name) in raw {
        let edge = Edge {
            src: Name::from(format!("v{s}")),
            tgt: Name::from(format!("v{t}")),
            kind: Name::from(*kind),
            name: name.map(Name::from),
        };
        if seen.insert(edge.clone()) {
            out.push(edge);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Cost function network instances
// ---------------------------------------------------------------------------

/// The floor [`arb_small_cfn_instance`] keeps its networks above.
///
/// A network with a handful of assignments is a search no implementation
/// can get wrong, so an oracle test drawn over one asserts nothing, and
/// kind filtering makes that the common case on schemas this small.
/// Measured over 500 draws each side, scoring both columns with
/// `DEFAULT_WEIGHTS` and no evidence so that the filter is the only
/// difference between them:
///
/// | | no floor | floor of 12 |
/// |---|---|---|
/// | median `∏_v \|D_v\|` | 4 | 27 |
/// | mean | 12.3 | 42.1 |
/// | 95th percentile | 36 | 125 |
/// | maximum | 243 | 1024 |
/// | pairs sharing no vertex kind | 12.0% | 0% |
/// | draws with no free variable | 12.0% | 0% |
/// | draws whose optimum is all-`⊥` | 21.8% | 0.6% |
/// | draws with a tied optimum | 9.8% | 27.6% |
///
/// The last two rows are the ones that matter. Without the floor, one
/// draw in five has the empty apex as its optimum, which every search
/// path reaches without searching, and only one in ten has two optima to
/// choose between, which is the only way a tie-break rule gets tested.
///
/// The two tail rows, the 95th percentile and the maximum, move with the
/// seed: a second 500-draw sweep read 108 and 256 where this one read 125
/// and 1024. The rows the floor is justified by are the medians and the
/// four percentages, which reproduce within a point.
///
/// The cost is a 71% rejection rate, which proptest absorbs without
/// approaching its local rejection limit, and a shift in the `|V_s|`
/// distribution: a one-vertex source can no longer reach the floor, so
/// the draws concentrate on four and five vertices. That trade is
/// deliberate. The degenerate shapes this floor excludes (no variables,
/// one variable, a domain of `{⊥}` alone, an all-`⊥` optimum) are
/// covered by hand-written unit tests in `panproto_mig::solve`, where
/// they can be asserted exactly rather than waited for.
pub const MIN_ASSIGNMENT_SPACE: u64 = 12;

/// A schema pair small enough to enumerate exhaustively, the weights it
/// is scored with, and the cost function network built over it.
///
/// Named rather than written out because the tuple is threaded through
/// several strategy combinators and a five-field tuple in each
/// signature is harder to read than the name.
pub type CfnInstance = (Protocol, Schema, Schema, CostWeights, Cfn);

/// A small schema pair and the network a morphism search over it
/// minimises.
///
/// The pair comes from [`arb_small_schema_pair`], so the source has 1 to
/// 5 vertices and the target 1 to 4, and the network comes from
/// [`build_cfn`], so it is the network the search actually runs on
/// rather than a reconstruction of one. Weights come from
/// [`arb_cost_weights`], and anchor evidence is drawn on a random subset
/// of the `(source, target)` pairs with values on the same rational
/// grid, so the anchor component is exercised rather than left at zero.
///
/// # The size bound
///
/// Domains are kind-filtered, so the assignment space is bounded by
/// `(|V_t| + 1)^{|V_s|} <= 5^5 = 3125`, two orders of magnitude under
/// [`MAX_ORACLE_ASSIGNMENTS`]. The ceiling filter at the end of this
/// strategy is therefore a backstop against a future change to the
/// bounds of [`arb_small_schema_pair`], not the mechanism that enforces
/// the bound; over 500 draws it has never fired.
///
/// # Degeneracy
///
/// The floor is the mechanism that does work. Kind filtering is
/// aggressive on schemas this small: a source vertex whose kind appears
/// nowhere in the target has the singleton domain `{⊥}`, and a pair
/// sharing no kind at all leaves exactly one assignment, which no search
/// can get wrong. Draws below [`MIN_ASSIGNMENT_SPACE`] are rejected, so
/// every instance carries at least a handful of genuine choices. The
/// filter runs on the schema pair rather than on the built network, so a
/// rejected draw costs no network construction.
pub fn arb_small_cfn_instance() -> impl Strategy<Value = CfnInstance> {
    arb_small_schema_pair()
        .prop_filter(
            "too few kind-compatible targets to make the search non-trivial",
            |(_, src, tgt)| kind_filtered_space(src, tgt) >= MIN_ASSIGNMENT_SPACE,
        )
        .prop_flat_map(|(protocol, src, tgt)| {
            let pairs = src.vertices.len() * tgt.vertices.len();
            (
                Just((protocol, src, tgt)),
                arb_cost_weights(),
                prop::collection::vec(prop::option::of(arb_weight_component()), pairs),
            )
        })
        .prop_filter_map(
            "small schema pair with no buildable network",
            |((protocol, src, tgt), raw, draw)| {
                let weights = CostWeights::new(raw[0], raw[1], raw[2], raw[3], raw[4]).ok()?;
                let evidence = DrawnEvidence(evidence_table(&src, &tgt, &draw));
                let cfn = build_cfn(
                    &src,
                    &tgt,
                    &SearchOptions::default(),
                    &DomainConstraints::default(),
                    &evidence,
                    weights,
                )
                .ok()?;
                Some((protocol, src, tgt, weights, cfn))
            },
        )
        .prop_filter(
            "network too large for the brute force oracle",
            |(_, _, _, _, cfn)| assignment_count(cfn) <= MAX_ORACLE_ASSIGNMENTS,
        )
}

/// The assignment space a kind-filtered network over this pair would
/// have, computed from the schemas rather than from a built network.
///
/// `∏_v (1 + |{ a ∈ V_t : kind(a) = kind(v) }|)`, the `1` being `⊥`. It
/// agrees with [`assignment_count`] on every network [`build_cfn`]
/// produces under default options, and exists so that the degeneracy
/// filter can run before the network is built.
#[must_use]
pub fn kind_filtered_space(src: &Schema, tgt: &Schema) -> u64 {
    let mut total = 1u64;
    for vertex in src.vertices.values() {
        let compatible = tgt
            .vertices
            .values()
            .filter(|target| target.kind == vertex.kind)
            .count();
        total = total.saturating_mul(u64::try_from(compatible).unwrap_or(u64::MAX) + 1);
    }
    total
}

// ---------------------------------------------------------------------------
// Total morphisms
// ---------------------------------------------------------------------------

/// How many total morphisms [`arb_scored_pair`] keeps enumerating before
/// it stops.
///
/// The decomposition theorem quantifies over every total morphism of a
/// pair, and a pair drawn by [`arb_schema_pair`] can admit thousands. A
/// cap keeps one proptest case cheap; taking them in the enumeration's
/// own order rather than sampling keeps a shrunk counterexample stable.
pub const MAX_ENUMERATED_MORPHISMS: usize = 64;

/// Every total morphism from `src` to `tgt`, up to `limit` of them.
///
/// A total morphism here is a map assigning every source vertex a
/// kind-compatible target vertex such that every source edge has an
/// image: `edge_image` finds a target edge of the same kind between the
/// images of its endpoints. That is exactly the condition under which
/// the network scores the corresponding assignment below `⊤`, and it is
/// the condition `build_morphism_weighted` accepts, so the two notions
/// of "is a morphism" are one notion.
///
/// The walk is a depth-first extension in ascending source name order
/// that checks each new vertex's edges against the vertices already
/// placed, so a partial map that has already broken naturality is
/// abandoned rather than completed. The order the morphisms come out in
/// is therefore a function of the schema pair alone.
#[must_use]
pub fn enumerate_total_morphisms(
    src: &Schema,
    tgt: &Schema,
    limit: usize,
) -> Vec<HashMap<Name, Name>> {
    let sources = sorted_ids(src);
    let mut found = Vec::new();
    let mut partial: HashMap<Name, Name> = HashMap::new();
    extend_morphism(src, tgt, &sources, 0, &mut partial, limit, &mut found);
    found
}

/// Place `sources[depth]`, keeping the map a morphism at every step.
fn extend_morphism(
    src: &Schema,
    tgt: &Schema,
    sources: &[Name],
    depth: usize,
    partial: &mut HashMap<Name, Name>,
    limit: usize,
    found: &mut Vec<HashMap<Name, Name>>,
) {
    if found.len() >= limit {
        return;
    }
    let Some(source) = sources.get(depth) else {
        found.push(partial.clone());
        return;
    };
    let Some(vertex) = src.vertices.get(source) else {
        return;
    };
    for target in sorted_ids(tgt) {
        if tgt.vertices.get(&target).map(|v| &v.kind) != Some(&vertex.kind) {
            continue;
        }
        partial.insert(source.clone(), target);
        if edges_are_natural(src, tgt, partial, source) {
            extend_morphism(src, tgt, sources, depth + 1, partial, limit, found);
        }
        partial.remove(source);
        if found.len() >= limit {
            return;
        }
    }
}

/// Whether every source edge with both endpoints placed, and at least one
/// endpoint at `source`, has an image under `partial`.
fn edges_are_natural(
    src: &Schema,
    tgt: &Schema,
    partial: &HashMap<Name, Name>,
    source: &Name,
) -> bool {
    src.edges.keys().all(|edge| {
        if &edge.src != source && &edge.tgt != source {
            return true;
        }
        let (Some(from), Some(to)) = (partial.get(&edge.src), partial.get(&edge.tgt)) else {
            return true;
        };
        edge_image(tgt, edge, from, to).is_some()
    })
}

/// The edge map the right leg of a span carries, for a total vertex map.
///
/// Built from [`edge_image`], which is the one function the objective's
/// edge component, the naturality constraint and the span all read, so
/// this is the same edge map on all three counts.
#[must_use]
pub fn edge_map_of<S: BuildHasher>(
    src: &Schema,
    tgt: &Schema,
    vertex_map: &HashMap<Name, Name, S>,
) -> HashMap<Edge, Edge> {
    let mut edge_map = HashMap::new();
    for edge in src.edges.keys() {
        let (Some(from), Some(to)) = (vertex_map.get(&edge.src), vertex_map.get(&edge.tgt)) else {
            continue;
        };
        if let Some(image) = edge_image(tgt, edge, from, to) {
            edge_map.insert(edge.clone(), image.clone());
        }
    }
    edge_map
}

/// The assignment standing for a total vertex map, or `None` if some
/// vertex's image is outside its domain.
#[must_use]
pub fn assignment_of<S: BuildHasher>(
    cfn: &Cfn,
    vertex_map: &HashMap<Name, Name, S>,
) -> Option<Assignment> {
    let mut values = Vec::with_capacity(cfn.n_variables());
    for variable in cfn.variables() {
        let image = vertex_map.get(variable.name())?;
        values.push(variable.value_id(image)?);
    }
    Some(Assignment::from_values(values))
}

/// `|C_src|`: the source vertices with at least one named outgoing edge.
///
/// This is the decomposition's fixed normaliser for the Jaccard
/// component.
#[must_use]
pub fn prop_class_size(src: &Schema) -> usize {
    src.vertices
        .keys()
        .filter(|source| {
            src.outgoing_edges(source)
                .iter()
                .any(|edge| edge.name.is_some())
        })
        .count()
}

/// `|C(m)|`: the mapped pairs at least one side of which has a named
/// outgoing edge.
///
/// This is the reference score's assignment-dependent normaliser, and
/// the two agree exactly when this equals [`prop_class_size`].
#[must_use]
pub fn pair_class_size<S: BuildHasher>(
    src: &Schema,
    tgt: &Schema,
    vertex_map: &HashMap<Name, Name, S>,
) -> usize {
    vertex_map
        .iter()
        .filter(|(source, target)| {
            let named = |schema: &Schema, id: &Name| {
                schema
                    .outgoing_edges(id)
                    .iter()
                    .any(|edge| edge.name.is_some())
            };
            named(src, source) || named(tgt, target)
        })
        .count()
}

/// How one source vertex is renamed on the target side.
///
/// All three forms are injective in the vertex index and disjoint from
/// each other, so a rename plan never collapses two source vertices onto
/// one target vertex and never collides with an added target vertex.
const RENAMINGS: [&str; 3] = ["v{}", "w{}", "v{}x"];

/// What happens to one source edge's name on the target side.
#[derive(Copy, Clone, Debug)]
enum EdgeRenaming {
    /// The label survives, so the edge component reads a match.
    Keep,
    /// The label changes, so the edge is preserved but renamed.
    Change,
    /// The label is dropped, so the target edge is unnamed.
    Erase,
}

/// The raw draw behind [`arb_scored_pair`]'s target side: one renaming
/// per source vertex, one per source edge, the kinds of the added target
/// vertices, and `(src, tgt, name?)` tuples for the added target edges.
type PerturbationShape = (
    Vec<usize>,
    Vec<EdgeRenaming>,
    Vec<&'static str>,
    Vec<(usize, usize, Option<u32>)>,
);

/// A schema pair admitting at least one total morphism, together with the
/// weights it is scored under.
///
/// # Why the target is a perturbation of the source
///
/// The decomposition theorem quantifies over total morphisms, so a pair
/// admitting none asserts nothing. Two independently drawn graphs almost
/// never admit one: naturality demands a target edge of the right kind
/// between the images of every source edge's endpoints, and over 2000
/// draws of [`arb_schema_pair`] only 17.9% of pairs cleared it. Filtering
/// an independent pair on that condition does not fix the corpus, it
/// skews it: an edgeless source satisfies naturality vacuously, so the
/// survivors are the sources with no edges. Measured over 500 accepted
/// draws of the filtered pair, the median source had **zero** edges, which
/// leaves the edge component and the naturality constraint untested in
/// more than half the corpus.
///
/// So the target is drawn as a perturbation of the source instead: every
/// source vertex gets an injectively renamed counterpart of its own kind,
/// every source edge gets a counterpart of its own kind whose label is
/// kept, changed or erased, and 0 to 2 further vertices and 0 to 3 further
/// edges are added on top. The counterpart map is a total morphism by
/// construction, so no filter is needed and the source keeps the edge
/// distribution [`arb_schema`] gave it. This is also the shape the search
/// actually meets: two versions of one schema, not two unrelated graphs.
///
/// Each of the three perturbation axes moves a different component. The
/// vertex renaming moves the name component, the edge renaming moves the
/// edge component, and the added target vertices and edges are what make
/// `|C(m)|` differ from `|C_src|`, which is the regime where the two
/// scores are claimed to differ rather than agree.
pub fn arb_scored_pair() -> impl Strategy<Value = (Schema, Schema, CostWeights)> {
    (arb_schema(), arb_cost_weights())
        .prop_flat_map(|(src, raw)| {
            let vertices = src.vertices.len();
            let edges = src.edges.len();
            (
                Just(src),
                Just(raw),
                arb_perturbation_shape(vertices, edges),
            )
        })
        .prop_filter_map("unbuildable perturbed pair", |(src, raw, shape)| {
            let weights = CostWeights::new(raw[0], raw[1], raw[2], raw[3], raw[4]).ok()?;
            let tgt = perturb(&src, &shape)?;
            Some((src, tgt, weights))
        })
}

/// Draw the perturbation applied to a source with these many vertices and
/// edges.
fn arb_perturbation_shape(
    vertices: usize,
    edges: usize,
) -> impl Strategy<Value = PerturbationShape> {
    (
        prop::collection::vec(0..RENAMINGS.len(), vertices),
        prop::collection::vec(
            prop_oneof!(
                Just(EdgeRenaming::Keep),
                Just(EdgeRenaming::Change),
                Just(EdgeRenaming::Erase)
            ),
            edges,
        ),
        prop::collection::vec(arb_kind(), 0..=2),
        prop::collection::vec(
            (0..vertices + 2, 0..vertices + 2, prop::option::of(0u32..5)),
            0..=3,
        ),
    )
}

/// Build the perturbed target, or `None` for a shape the builder rejects.
fn perturb(src: &Schema, shape: &PerturbationShape) -> Option<Schema> {
    let (vertex_plan, edge_plan, added_kinds, added_edges) = shape;
    let protocol = open_protocol();
    let sources = sorted_ids(src);

    // One injectively renamed counterpart per source vertex, same kind.
    let mut counterpart: HashMap<Name, Name> = HashMap::new();
    let mut builder = SchemaBuilder::new(&protocol);
    let mut target_ids: Vec<Name> = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let kind = src.vertices.get(source).map(|v| v.kind.clone())?;
        let pattern = RENAMINGS[vertex_plan.get(index).copied().unwrap_or(0) % RENAMINGS.len()];
        let id = Name::from(pattern.replace("{}", &index.to_string()));
        builder = builder.vertex(id.as_str(), kind.as_str(), None).ok()?;
        counterpart.insert(source.clone(), id.clone());
        target_ids.push(id);
    }

    // Up to two further target vertices, which is what lets `|C(m)|` exceed
    // `|C_src|` and what gives a source vertex more than one image to choose
    // between.
    for (index, kind) in added_kinds.iter().enumerate() {
        let id = Name::from(format!("added{index}"));
        builder = builder.vertex(id.as_str(), kind, None).ok()?;
        target_ids.push(id);
    }

    // One counterpart per source edge, of the same kind, under a label that is
    // kept, changed or erased.
    let mut edges: Vec<&Edge> = src.edges.keys().collect();
    edges.sort_unstable();
    let mut placed: HashSet<(Name, Name, Name, Option<Name>)> = HashSet::new();
    for (index, edge) in edges.iter().enumerate() {
        let (Some(from), Some(to)) = (counterpart.get(&edge.src), counterpart.get(&edge.tgt))
        else {
            continue;
        };
        let name = match edge_plan.get(index).copied().unwrap_or(EdgeRenaming::Keep) {
            EdgeRenaming::Keep => edge.name.clone(),
            EdgeRenaming::Change => Some(Name::from(format!("r{index}"))),
            EdgeRenaming::Erase => None,
        };
        let key = (from.clone(), to.clone(), edge.kind.clone(), name.clone());
        if !placed.insert(key) {
            // Two source edges perturbed onto one target edge. The target edge
            // is already there, so both source edges still have an image.
            continue;
        }
        builder = builder
            .edge(
                from.as_str(),
                to.as_str(),
                edge.kind.as_str(),
                name.as_deref(),
            )
            .ok()?;
    }

    // Up to three further target edges, which give the target out-degrees and
    // outgoing-label sets the source does not have.
    for (from, to, label) in added_edges {
        let (Some(from), Some(to)) = (target_ids.get(*from), target_ids.get(*to)) else {
            continue;
        };
        let name = label.map(|n| Name::from(format!("a{n}")));
        let key = (from.clone(), to.clone(), Name::from("prop"), name.clone());
        if !placed.insert(key) {
            continue;
        }
        builder = builder
            .edge(from.as_str(), to.as_str(), "prop", name.as_deref())
            .ok()?;
    }

    builder.build().ok()
}

/// Anchor evidence read out of a drawn table.
///
/// A pair the draw said nothing about reports zero confidence, which is
/// what [`NoEvidence`](panproto_mig::solve::build::NoEvidence) reports
/// for every pair, so an empty draw is the no-evidence search.
struct DrawnEvidence(HashMap<(Name, Name), f64>);

impl Evidence for DrawnEvidence {
    fn confidence(&self, source: &Name, target: &Name) -> f64 {
        self.0
            .get(&(source.clone(), target.clone()))
            .copied()
            .unwrap_or(0.0)
    }
}

/// Anchor evidence on a subset of the `(source, target)` vertex pairs.
///
/// The draw is one `Option` per pair in ascending name order on both
/// sides, so a shrunk counterexample keeps the same pair association as
/// the draw it shrank from.
fn evidence_table(src: &Schema, tgt: &Schema, draw: &[Option<f64>]) -> HashMap<(Name, Name), f64> {
    let mut table = HashMap::new();
    let mut index = 0usize;
    for source in sorted_ids(src) {
        for target in sorted_ids(tgt) {
            if let Some(Some(value)) = draw.get(index) {
                table.insert((source.clone(), target.clone()), *value);
            }
            index += 1;
        }
    }
    table
}

/// A schema's vertex ids in ascending order.
///
/// `Schema::vertices` is a `HashMap`, so every walk over it that feeds a
/// [`VarId`] or a [`ValId`](panproto_mig::ValId) has to fix an order
/// first, and it has to be this one: the network's value order is
/// ascending target name.
fn sorted_ids(schema: &Schema) -> Vec<Name> {
    let mut ids: Vec<Name> = schema.vertices.keys().cloned().collect();
    ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    ids
}

// ---------------------------------------------------------------------------
// Generator contracts
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The 20 collection-valued fields of [`Schema`], each paired with
    /// whether it is populated. The twenty-first field, `protocol`, is
    /// a `String` and is checked separately.
    fn field_population(schema: &Schema) -> Vec<(&'static str, bool)> {
        vec![
            ("vertices", !schema.vertices.is_empty()),
            ("edges", !schema.edges.is_empty()),
            ("hyper_edges", !schema.hyper_edges.is_empty()),
            ("constraints", !schema.constraints.is_empty()),
            ("required", !schema.required.is_empty()),
            ("nsids", !schema.nsids.is_empty()),
            ("entries", !schema.entries.is_empty()),
            ("variants", !schema.variants.is_empty()),
            ("orderings", !schema.orderings.is_empty()),
            ("recursion_points", !schema.recursion_points.is_empty()),
            ("spans", !schema.spans.is_empty()),
            ("usage_modes", !schema.usage_modes.is_empty()),
            ("nominal", !schema.nominal.is_empty()),
            ("coercions", !schema.coercions.is_empty()),
            ("mergers", !schema.mergers.is_empty()),
            ("defaults", !schema.defaults.is_empty()),
            ("policies", !schema.policies.is_empty()),
            ("outgoing", !schema.outgoing.is_empty()),
            ("incoming", !schema.incoming.is_empty()),
            ("between", !schema.between.is_empty()),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// Every one of the 21 `Schema` fields is populated on every
        /// draw. This is the contract `arb_schema_rich` exists to
        /// provide, so it is asserted rather than assumed.
        #[test]
        fn rich_schema_populates_every_field((_, schema) in arb_schema_rich()) {
            prop_assert!(!schema.protocol.is_empty(), "protocol is empty");
            for (field, populated) in field_population(&schema) {
                prop_assert!(populated, "field {field} is empty");
            }
        }

        /// A rich schema validates against the protocol it was built
        /// over, so a consumer that re-validates a cut of it is testing
        /// the cut rather than the generator.
        #[test]
        fn rich_schema_validates((protocol, schema) in arb_schema_rich()) {
            let errors = panproto_schema::validate(&schema, &protocol);
            prop_assert!(errors.is_empty(), "validation reported {errors:?}");
        }

        /// Every name a rich schema's post-build fields reference is a
        /// live vertex or a live edge. Nothing dangles.
        ///
        /// `Variant::id` is checked as well as `parent_vertex`, because
        /// it is the field a sub-schema cut tests: an `id` that names no
        /// vertex makes the arm-retention branch of
        /// `panproto_schema::induce` unreachable, and a property test
        /// over `variants` then asserts nothing.
        #[test]
        fn rich_schema_has_no_dangling_references((_, schema) in arb_schema_rich()) {
            for (parent, variants) in &schema.variants {
                prop_assert!(schema.vertices.contains_key(parent));
                for v in variants {
                    prop_assert!(schema.vertices.contains_key(&v.parent_vertex));
                    prop_assert!(schema.vertices.contains_key(&v.id));
                    prop_assert_eq!(&v.parent_vertex, parent);
                }
            }
            for rp in schema.recursion_points.values() {
                prop_assert!(schema.vertices.contains_key(&rp.mu_id));
                prop_assert!(schema.vertices.contains_key(&rp.target_vertex));
            }
            for span in schema.spans.values() {
                prop_assert!(schema.vertices.contains_key(&span.left));
                prop_assert!(schema.vertices.contains_key(&span.right));
            }
            for edge in schema.orderings.keys().chain(schema.usage_modes.keys()) {
                prop_assert!(schema.edges.contains_key(edge));
            }
            for id in schema.nominal.keys() {
                prop_assert!(schema.vertices.contains_key(id));
            }
        }

        /// Both halves of a schema pair name the same protocol, which
        /// is what makes their vertex kinds comparable.
        #[test]
        fn schema_pair_shares_a_protocol((a, b) in arb_schema_pair()) {
            prop_assert_eq!(&a.protocol, &b.protocol);
        }

        /// The oracle pair respects the size bounds of the brute-force
        /// enumeration: the assignment space, counting the unmapped
        /// value on each source variable, stays under 3125.
        #[test]
        fn small_pair_is_brute_forceable((_, src, tgt) in arb_small_schema_pair()) {
            prop_assert!((1..=5).contains(&src.vertices.len()), "|V_s| = {}", src.vertices.len());
            prop_assert!((1..=4).contains(&tgt.vertices.len()), "|V_t| = {}", tgt.vertices.len());
            prop_assert!(src.edges.len() <= 6, "|E_s| = {}", src.edges.len());
            prop_assert!(tgt.edges.len() <= 6, "|E_t| = {}", tgt.edges.len());

            let assignments = (tgt.vertices.len() + 1).pow(
                u32::try_from(src.vertices.len()).unwrap()
            );
            prop_assert!(assignments <= 3125, "{assignments} assignments");
        }

        /// Cost weights are a probability vector: every component is in
        /// `[0, 1]` and they sum to 1 within one ulp of accumulated
        /// rounding.
        #[test]
        fn cost_weights_are_normalised(w in arb_cost_weights()) {
            for x in w {
                prop_assert!((0.0..=1.0).contains(&x), "component {x} out of range");
            }
            let total: f64 = w.iter().sum();
            prop_assert!((total - 1.0).abs() < 1e-12, "weights sum to {total}");
        }

        /// Every network the instance generator yields is inside the
        /// window the brute force oracle can enumerate: above the floor
        /// that keeps the search non-trivial and under the ceiling that
        /// keeps enumeration cheap.
        #[test]
        fn cfn_instance_stays_inside_the_oracle_window(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let space = assignment_count(&cfn);
            prop_assert!(space >= MIN_ASSIGNMENT_SPACE, "{space} assignments");
            prop_assert!(space <= MAX_ORACLE_ASSIGNMENTS, "{space} assignments");
        }

        /// The size a filter can compute from the schemas alone is the
        /// size the built network actually has.
        ///
        /// `kind_filtered_space` is what the degeneracy filter consults
        /// before paying for a network, so a divergence between it and
        /// `assignment_count` would let the filter admit an instance the
        /// oracle then refuses.
        #[test]
        fn the_predicted_and_built_assignment_spaces_agree(
            (_, src, tgt, _, cfn) in arb_small_cfn_instance()
        ) {
            prop_assert_eq!(kind_filtered_space(&src, &tgt), assignment_count(&cfn));
        }

        /// Every instance leaves at least two variables with a real
        /// choice to make, so no draw is a search with one candidate.
        #[test]
        fn cfn_instance_has_more_than_one_free_variable(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let free = cfn
                .variable_ids()
                .filter(|var| cfn.domain(*var).is_some_and(|d| d.len() > 1))
                .count();
            prop_assert!(free >= 2, "{free} variables with more than one value");
        }

        /// One variable per source vertex, `⊥` in every domain, and the
        /// network's own view of its variables agrees with the source
        /// schema it was built from.
        #[test]
        fn cfn_instance_has_one_variable_per_source_vertex(
            (_, src, _, _, cfn) in arb_small_cfn_instance()
        ) {
            prop_assert_eq!(cfn.n_variables(), src.vertices.len());
            for var in cfn.variable_ids() {
                let variable = cfn.variable(var).unwrap();
                prop_assert!(src.vertices.contains_key(variable.name()));
                let domain = cfn.domain(var).unwrap();
                prop_assert!(domain.contains(panproto_mig::ValId::BOTTOM));
            }
        }
    }
}
