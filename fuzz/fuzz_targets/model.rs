//! The generator the structural targets share.
//!
//! Every schema handed to the engine here is built through
//! [`SchemaBuilder`], so it is well formed by construction and a failure
//! downstream is the engine's rather than the generator's. The generator
//! pre-checks each edge against the protocol's rules and only then calls the
//! builder, so a builder rejection is itself a finding: the two disagree
//! about what the protocol permits.

// Each target uses a different slice of this module, so the unused half is
// dead in every individual binary and live across the set.
#![allow(dead_code)]

use arbitrary::{Arbitrary, Unstructured};
use panproto_gat::Name;
use panproto_schema::{
    Edge, EdgeRule, Protocol, RecursionPoint, Schema, SchemaBuilder, Span, Variant,
};
use rustc_hash::FxHashSet;

/// The vertex kinds the generated protocol knows.
pub const KINDS: [&str; 6] = ["record", "object", "array", "string", "integer", "boolean"];

/// The edge kinds, with the source and target kinds each admits.
pub const EDGE_KINDS: [(&str, &[&str], &[&str]); 4] = [
    ("record-schema", &["record"], &["object"]),
    (
        "prop",
        &["object"],
        &["string", "integer", "boolean", "object", "array"],
    ),
    ("item", &["array"], &["string", "integer", "object"]),
    ("variant", &["object"], &["object", "string", "integer"]),
];

/// The protocol every generated schema is built against.
///
/// Rich enough that a vertex's kind carries information and that the edge
/// rules actually reject something, which is what makes the domains of the
/// constraint network non-trivial.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "fuzz".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: EDGE_KINDS
            .iter()
            .map(|(kind, src, tgt)| EdgeRule {
                edge_kind: (*kind).to_owned(),
                src_kinds: src.iter().map(|k| (*k).to_owned()).collect(),
                tgt_kinds: tgt.iter().map(|k| (*k).to_owned()).collect(),
            })
            .collect(),
        obj_kinds: KINDS.iter().map(|k| (*k).to_owned()).collect(),
        constraint_sorts: vec!["maxLength".to_owned(), "format".to_owned()],
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// How the generator was told to shape one schema.
#[derive(Debug, Clone, Arbitrary)]
pub struct Shape {
    /// Vertex kinds, one per vertex. Length is clamped by the caller.
    kinds: Vec<u8>,
    /// Candidate edges as `(src, tgt, kind, named)`.
    arcs: Vec<(u8, u8, u8, bool)>,
    /// Which vertices are entry points.
    entries: Vec<u8>,
    /// Which vertices carry a constraint.
    constrained: Vec<(u8, bool)>,
    /// Which edges are declared required for their source.
    required: Vec<u16>,
    /// Coproduct annotations as `(coproduct, variant id, parent, dangling)`.
    variants: Vec<(u8, u8, u8, bool)>,
    /// Fixpoint markers as `(mu vertex, target vertex, dangling)`.
    recursion: Vec<(u8, u8, bool)>,
    /// Spans as `(left, right, dangling)`.
    spans: Vec<(u8, u8, bool)>,
}

/// A schema plus the pieces of it a target may want to inspect.
pub struct Generated {
    /// The finished schema.
    pub schema: Schema,
    /// Its vertex names, in generation order.
    pub vertex_names: Vec<Name>,
}

/// Build one well-formed schema from `shape`, with names carrying `prefix`.
///
/// Returns `None` when the shape asks for no vertices, since
/// [`SchemaBuilder`] cannot produce the empty schema and the empty case is
/// covered by its own unit tests.
///
/// # Panics
///
/// If [`SchemaBuilder`] rejects a vertex or an edge the protocol's own rules
/// permit. That disagreement is the invariant this function asserts.
#[must_use]
pub fn build(shape: &Shape, prefix: &str, max_vertices: usize) -> Option<Generated> {
    let protocol = protocol();
    let kinds: Vec<&str> = shape
        .kinds
        .iter()
        .take(max_vertices)
        .map(|k| KINDS[usize::from(*k) % KINDS.len()])
        .collect();
    if kinds.is_empty() {
        return None;
    }

    let names: Vec<String> = (0..kinds.len()).map(|i| format!("{prefix}v{i}")).collect();
    let mut builder = SchemaBuilder::new(&protocol);
    for (name, kind) in names.iter().zip(&kinds) {
        builder = builder
            .vertex(name, kind, None::<&str>)
            .expect("a fresh name with a protocol kind is a legal vertex");
    }

    // Only edges the protocol's own rules admit are offered to the builder,
    // and duplicates are filtered here rather than by catching the error,
    // because `edge` consumes the builder on the way out.
    let mut seen: FxHashSet<(usize, usize, usize, bool)> = FxHashSet::default();
    let mut built: Vec<Edge> = Vec::new();
    for (raw_src, raw_tgt, raw_kind, named) in &shape.arcs {
        let src = usize::from(*raw_src) % kinds.len();
        let tgt = usize::from(*raw_tgt) % kinds.len();
        let kind_idx = usize::from(*raw_kind) % EDGE_KINDS.len();
        let (kind, src_ok, tgt_ok) = EDGE_KINDS[kind_idx];
        if !src_ok.contains(&kinds[src]) || !tgt_ok.contains(&kinds[tgt]) {
            continue;
        }
        if !seen.insert((src, tgt, kind_idx, *named)) {
            continue;
        }
        let label = if *named {
            Some(format!("f{}", built.len()))
        } else {
            None
        };
        builder = builder
            .edge(&names[src], &names[tgt], kind, label.as_deref())
            .expect("an edge the protocol's rules admit must be accepted");
        built.push(Edge {
            src: Name::from(names[src].as_str()),
            tgt: Name::from(names[tgt].as_str()),
            kind: Name::from(kind),
            name: label.as_deref().map(Name::from),
        });
    }

    for raw in &shape.entries {
        builder = builder.entry(&names[usize::from(*raw) % names.len()]);
    }
    for (raw, which) in &shape.constrained {
        let sort = if *which { "maxLength" } else { "format" };
        builder = builder.constraint(&names[usize::from(*raw) % names.len()], sort, "1");
    }
    if !built.is_empty() {
        for raw in &shape.required {
            let edge = built[usize::from(*raw) % built.len()].clone();
            let owner = edge.src.to_string();
            builder = builder.required(&owner, vec![edge]);
        }
    }

    let mut schema = builder
        .build()
        .expect("a builder fed only legal vertices and edges must build");

    // The annotation maps `SchemaBuilder` cannot reach. These are what
    // `build_cfn` turns into hard `⊤` constraints, so they are the only way
    // the all-`⊥` assignment could stop being feasible and the search could
    // start refusing. A `dangling` flag names a vertex the schema does not
    // hold, which is the case a well-formed builder can never produce and a
    // hand-assembled or deserialised schema can.
    let pick = |raw: u8, dangling: bool| -> Name {
        if dangling {
            Name::from("absent")
        } else {
            Name::from(names[usize::from(raw) % names.len()].as_str())
        }
    };
    for (coproduct, id, parent, dangling) in &shape.variants {
        let owner = pick(*coproduct, false);
        schema
            .variants
            .entry(owner.clone())
            .or_default()
            .push(Variant {
                id: pick(*id, *dangling),
                parent_vertex: pick(*parent, false),
                tag: None,
            });
    }
    for (mu, target, dangling) in &shape.recursion {
        schema.recursion_points.insert(
            pick(*mu, false),
            RecursionPoint {
                target_vertex: pick(*target, *dangling),
            },
        );
    }
    for (index, (left, right, dangling)) in shape.spans.iter().enumerate() {
        let id = Name::from(format!("span{index}").as_str());
        schema.spans.insert(
            id.clone(),
            Span {
                id,
                left: pick(*left, false),
                right: pick(*right, *dangling),
            },
        );
    }

    Some(Generated {
        schema,
        vertex_names: names.iter().map(|n| Name::from(n.as_str())).collect(),
    })
}

/// Build two schemas from one input, which is what a pair-shaped target wants.
#[must_use]
pub fn pair(u: &mut Unstructured<'_>, max_vertices: usize) -> Option<(Generated, Generated)> {
    let left: Shape = u.arbitrary().ok()?;
    let right: Shape = u.arbitrary().ok()?;
    let src = build(&left, "a.", max_vertices)?;
    let tgt = build(&right, "b.", max_vertices)?;
    Some((src, tgt))
}
