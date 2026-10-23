#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The resolution corpus: reference resolution between instance shapes,
//! its typed failures, its put domain and its five laws, on small exact
//! fixtures that pin each counterexample the resolution carrier answers.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use panproto_gat::Name;
use panproto_inst::{FInstance, FieldPresence, GInstance, Node, RowId, Value, ViewRef, WInstance};
use panproto_io::resolution::{
    AnyInstance, ExternalTextStore, OffsetUnitReason, PathResolution, Resolution,
    ResolutionComplement, ResolutionContextKind, ResolutionFeature, ResolutionKind,
    ResolutionProvider, ResolutionProviderId, ResolutionSpec, ResolutionTextId,
    ResolutionValueSite, ResolveContext, ResolveDomainError, ResolveError, ResolveOptions,
    TokenBoundaryProvider, UnresolvedReason, check_resolution_laws,
};
use panproto_lens::{Mutation, SkipReason};
use panproto_schema::{
    Cardinality, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, OffsetUnit,
    OnUnresolved, PathSyntax, Protocol, RefSource, RefTextSource, Reference, ResolveMode, Schema,
    SchemaBuilder,
};

// ---------------------------------------------------------------------------
// Fixture support
// ---------------------------------------------------------------------------

/// A protocol with no descriptor in `panproto-protocols`.
fn protocol(name: &str, syntactic: InstanceShapeKind, semantic: InstanceShapeKind) -> Protocol {
    Protocol {
        name: name.to_owned(),
        syntactic_shape: syntactic,
        semantic_shape: semantic,
        ..Protocol::default()
    }
}

fn edge(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: name.map(Name::from),
    }
}

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    edge(src, tgt, "prop", Some(name))
}

/// A provider whose spec is fixed per test.
struct Provider {
    id: ResolutionProviderId,
    protocol: Protocol,
    kind: ResolutionKind,
    spec: ResolutionSpec,
    paths: BTreeMap<String, ViewRef>,
    context: Option<panproto_inst::Digest>,
}

impl ResolutionProvider for Provider {
    fn id(&self) -> ResolutionProviderId {
        self.id.clone()
    }

    fn kind(&self) -> ResolutionKind {
        self.kind
    }

    fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    fn spec(
        &self,
        _protocol: &Protocol,
        _syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        Ok(self.spec.clone())
    }

    fn resolve_path(
        &self,
        reference: &Reference,
        _holder: ViewRef,
        literal: &Value,
        source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<PathResolution, ResolveError> {
        if !matches!(reference.source, RefSource::Path { .. }) {
            return Err(ResolveError::UnsupportedFeature {
                provider: Some(self.id()),
                feature: ResolutionFeature::Path(PathSyntax::IdRef),
            });
        }
        let Value::Str(text) = literal else {
            return Ok(PathResolution {
                candidates: Vec::new(),
                context: self.context,
            });
        };
        // `raw:` lists node ids the provider returns as they are, so that a
        // test can hand the engine candidates out of order or absent.
        if let Some(ids) = text.strip_prefix("raw:") {
            return Ok(PathResolution {
                candidates: ids
                    .split(',')
                    .map(|id| ViewRef::Node(id.parse().unwrap_or(u32::MAX)))
                    .collect(),
                context: self.context,
            });
        }
        let exists = |candidate: &ViewRef| match (source, candidate) {
            (AnyInstance::Tree(tree), ViewRef::Node(id)) => tree.nodes.contains_key(id),
            (AnyInstance::Table(table), ViewRef::Row(id)) => table.row(*id).is_some(),
            _ => false,
        };
        Ok(PathResolution {
            candidates: self
                .paths
                .get(text)
                .filter(|candidate| exists(candidate))
                .cloned()
                .into_iter()
                .collect(),
            context: self.context,
        })
    }

    fn render_path(
        &self,
        _reference: &Reference,
        _holder: ViewRef,
        target: ViewRef,
        _view: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<Value, ResolveError> {
        self.paths
            .iter()
            .find(|(_, candidate)| **candidate == target)
            .map(|(path, _)| Value::Str(path.clone()))
            .ok_or_else(|| ResolveError::ProviderFailure {
                provider: self.id(),
                code: "no_path".into(),
                at: Some(Box::new(target)),
            })
    }
}

fn provider_id(name: &str) -> ResolutionProviderId {
    ResolutionProviderId {
        name: name.into(),
        version: 1,
    }
}

/// A spec for a resolving kind over `schema`: the semantic schema is the
/// syntactic one, every non-reference edge is structural, and each
/// reference edge is written by the named reference.
fn spec_for(
    name: &str,
    kind: ResolutionKind,
    schema: &Schema,
    writers: &[(&Edge, &str)],
    context: &[ResolutionContextKind],
) -> ResolutionSpec {
    let reference_edges: BTreeSet<&Edge> = schema
        .declarations
        .references
        .values()
        .filter_map(|reference| reference.edge.as_ref())
        .collect();
    ResolutionSpec {
        kind,
        provider: provider_id(name),
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: schema
            .edges
            .keys()
            .filter(|edge| !reference_edges.contains(edge))
            .cloned()
            .collect(),
        reference_writers: writers
            .iter()
            .map(|(edge, reference)| ((*edge).clone(), Name::from(*reference)))
            .collect(),
        context_requirements: context.iter().copied().collect(),
        assembly: None,
    }
}

fn compile(
    name: &str,
    kind: ResolutionKind,
    schema: &Schema,
    writers: &[(&Edge, &str)],
) -> Resolution {
    compile_with(name, kind, schema, writers, &[], BTreeMap::new())
}

fn compile_with(
    name: &str,
    kind: ResolutionKind,
    schema: &Schema,
    writers: &[(&Edge, &str)],
    context: &[ResolutionContextKind],
    paths: BTreeMap<String, ViewRef>,
) -> Resolution {
    let protocol = protocol(name, kind.syntactic_shape(), kind.semantic_shape());
    let provider = Provider {
        id: provider_id(name),
        protocol: protocol.clone(),
        kind,
        spec: spec_for(name, kind, schema, writers, context),
        paths,
        context: None,
    };
    Resolution::for_protocol(&protocol, schema, Arc::new(provider)).expect("the fixture compiles")
}

fn reference(
    id: &str,
    holder: &str,
    source: RefSource,
    target: &str,
    edge_label: Edge,
) -> Reference {
    Reference {
        id: id.into(),
        holder: holder.into(),
        source,
        target_vertex: target.into(),
        target_key: None,
        edge: Some(edge_label),
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    }
}

fn key(id: &str, parts: Vec<KeyPart>, role: KeyRole, scope: KeyScope) -> Key {
    Key {
        id: id.into(),
        parts,
        role,
        scope,
    }
}

fn leaf(id: u32, anchor: &str, value: Value) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(value))
}

fn tree(nodes: Vec<Node>, arcs: Vec<(u32, u32, Edge)>, root: u32) -> WInstance {
    let root_anchor = nodes
        .iter()
        .find(|node| node.id == root)
        .map(|node| node.anchor.clone())
        .expect("the root is a node");
    WInstance::new(
        nodes.into_iter().map(|node| (node.id, node)).collect(),
        arcs,
        Vec::new(),
        root,
        root_anchor,
    )
}

/// Whether two instances are the same presentation, compared through
/// their serialized form (every map of an instance serializes sorted).
fn same(a: &AnyInstance, b: &AnyInstance) -> bool {
    serde_json::to_value(a).expect("serializes") == serde_json::to_value(b).expect("serializes")
}

fn get(resolution: &Resolution, source: &AnyInstance) -> (GInstance, ResolutionComplement) {
    let (view, complement) = resolution
        .get(source, &ResolveOptions::default(), &ResolveContext::empty())
        .expect("get succeeds");
    match view {
        AnyInstance::Graph(graph) => (graph, complement),
        other => panic!("a resolving kind returns a graph, got {:?}", other.shape()),
    }
}

/// The edges of a graph as `(src, tgt, label name)`, sorted.
fn edge_triples(graph: &GInstance) -> Vec<(u32, u32, String)> {
    let mut triples: Vec<_> = graph
        .edges_sorted()
        .map(|(_, edge)| {
            (
                edge.src,
                edge.tgt,
                edge.edge
                    .name
                    .as_ref()
                    .map_or_else(|| edge.edge.kind.to_string(), ToString::to_string),
            )
        })
        .collect();
    triples.sort();
    triples
}

// ---------------------------------------------------------------------------
// The posts fixture: a tree of users and posts, each post naming its
// author by the author's id.
// ---------------------------------------------------------------------------

struct Posts {
    schema: Schema,
    author_ref: Edge,
}

fn posts_schema(reference_override: impl FnOnce(&mut Reference)) -> Posts {
    let base = protocol("posts", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let author_ref = edge("post", "user", "ref", Some("author_ref"));
    let mut author = reference(
        "post_author",
        "post",
        RefSource::KeyValue {
            parts: vec![vec![prop("post", "author", "author")]],
        },
        "user",
        author_ref.clone(),
    );
    reference_override(&mut author);
    let schema = SchemaBuilder::new(&base)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("user", "object", None)
        .unwrap()
        .vertex("user_id", "string", None)
        .unwrap()
        .vertex("post", "object", None)
        .unwrap()
        .vertex("author", "string", None)
        .unwrap()
        .edge("doc", "user", "prop", Some("users"))
        .unwrap()
        .edge("user", "user_id", "prop", Some("id"))
        .unwrap()
        .edge("doc", "post", "prop", Some("posts"))
        .unwrap()
        .edge("post", "author", "prop", Some("author"))
        .unwrap()
        .edge("post", "user", "ref", Some("author_ref"))
        .unwrap()
        .entry("doc")
        .key(
            "user",
            key(
                "id",
                vec![KeyPart::Path(vec![prop("user", "user_id", "id")])],
                KeyRole::Primary,
                KeyScope::Document,
            ),
        )
        .reference(author)
        .build()
        .unwrap();
    Posts { schema, author_ref }
}

/// doc 0; users 1 (id 2 = "u1") and 3 (id 4 = "u2"); posts 5 (author 6 =
/// "u1") and 7 (author 8 = "u2").
/// The posts tree laid out canonically: each parent's children in
/// ascending `(label, child)` order, arcs in document order.
fn canonical_posts_tree() -> WInstance {
    let mut tree = posts_tree();
    let arcs = vec![
        tree.arcs[4].clone(),
        tree.arcs[5].clone(),
        tree.arcs[6].clone(),
        tree.arcs[7].clone(),
        tree.arcs[0].clone(),
        tree.arcs[1].clone(),
        tree.arcs[2].clone(),
        tree.arcs[3].clone(),
    ];
    tree = WInstance::new(tree.nodes, arcs, Vec::new(), 0, "doc".into());
    tree
}

fn posts_tree() -> WInstance {
    let users = prop("doc", "user", "users");
    let id = prop("user", "user_id", "id");
    let posts = prop("doc", "post", "posts");
    let author = prop("post", "author", "author");
    tree(
        vec![
            Node::new(0, "doc"),
            Node::new(1, "user"),
            leaf(2, "user_id", Value::Str("u1".into())),
            Node::new(3, "user"),
            leaf(4, "user_id", Value::Str("u2".into())),
            Node::new(5, "post"),
            leaf(6, "author", Value::Str("u1".into())),
            Node::new(7, "post"),
            leaf(8, "author", Value::Str("u2".into())),
        ],
        vec![
            (0, 1, users.clone()),
            (1, 2, id.clone()),
            (0, 3, users),
            (3, 4, id),
            (0, 5, posts.clone()),
            (5, 6, author.clone()),
            (0, 7, posts),
            (7, 8, author),
        ],
        0,
    )
}

#[test]
fn a_tree_resolves_each_key_reference_to_one_edge() {
    let posts = posts_schema(|_| {});
    let resolution = compile(
        "posts",
        ResolutionKind::TreeToGraph,
        &posts.schema,
        &[(&posts.author_ref, "post_author")],
    );
    let source = AnyInstance::Tree(posts_tree());
    let (view, complement) = get(&resolution, &source);
    let references: Vec<_> = edge_triples(&view)
        .into_iter()
        .filter(|(_, _, label)| label == "author_ref")
        .collect();
    assert_eq!(
        references,
        vec![
            (5, 1, "author_ref".to_owned()),
            (7, 3, "author_ref".to_owned())
        ]
    );
    assert_eq!(complement.occurrences.len(), 2);
    let restored = resolution
        .put(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .expect("put of the unchanged view");
    let AnyInstance::Tree(restored) = restored else {
        panic!("a tree source");
    };
    assert_eq!(restored.arcs, posts_tree().arcs);
    let report = check_resolution_laws(
        &resolution,
        &source,
        &ResolveOptions::default(),
        &ResolveContext::empty(),
    )
    .expect("the five laws hold");
    assert!(!report.ran.is_empty());
}

#[test]
fn a_recorded_occurrence_preserves_its_label_source_and_absent_position() {
    let (resolution, _) = posts_resolution(|_| {});
    let (view, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    let occurrence = complement
        .occurrences
        .keys()
        .find_map(|carrier| match carrier {
            ViewRef::Edge(id) => Some(*id),
            ViewRef::Node(_) | ViewRef::Arc { .. } | ViewRef::Row(_) | ViewRef::Pair { .. } => None,
        })
        .expect("an occurrence edge");
    let original = view.edge(occurrence).expect("the occurrence edge").clone();

    let mut wrong_label = view.clone();
    let mut edge = wrong_label
        .remove_edge(occurrence)
        .expect("remove the occurrence edge");
    edge.edge = prop("post", "user", "another_author_ref");
    wrong_label
        .insert_edge(occurrence, edge)
        .expect("replace the occurrence edge with another label");

    let mut wrong_source = view.clone();
    wrong_source
        .set_endpoints(occurrence, 7, original.tgt)
        .expect("move the occurrence edge to another post");

    let mut positioned = view;
    positioned
        .set_edge_position(occurrence, Some(0))
        .expect("position the occurrence edge");

    let expected = ResolveDomainError::SynthesizedEdited {
        carrier: ViewRef::Edge(occurrence),
    };
    for (name, edited) in [
        ("label", wrong_label),
        ("source", wrong_source),
        ("position", positioned),
    ] {
        let violations = violations_of(&resolution, &edited, &complement, &ctx());
        assert!(
            violations.contains(&expected),
            "changing only the recorded occurrence's {name} must report \
             SynthesizedEdited: {violations:?}"
        );
    }
}

/// The posts tree with post 5's author field holding `first` and post 7's
/// holding `second`.
fn posts_tree_with(first: Value, second: Value) -> WInstance {
    let mut tree = posts_tree();
    for (node, value) in [(6, first), (8, second)] {
        if let Some(record) = tree.nodes.get_mut(&node) {
            record.value = Some(FieldPresence::Present(value));
        }
    }
    tree
}

fn posts_resolution(reference_override: impl FnOnce(&mut Reference)) -> (Resolution, Posts) {
    let posts = posts_schema(reference_override);
    let resolution = compile(
        "posts",
        ResolutionKind::TreeToGraph,
        &posts.schema,
        &[(&posts.author_ref, "post_author")],
    );
    (resolution, posts)
}

fn laws_hold(resolution: &Resolution, source: &AnyInstance, options: ResolveOptions) {
    check_resolution_laws(resolution, source, &options, &ResolveContext::empty())
        .expect("the five laws hold");
}

#[test]
fn the_law_report_counts_each_family_by_its_contract_class() {
    let (resolution, _) = posts_resolution(|_| {});
    let report = check_resolution_laws(
        &resolution,
        &AnyInstance::Tree(posts_tree()),
        &ResolveOptions::default(),
        &ResolveContext::empty(),
    )
    .expect("the five laws hold");
    let counts: Vec<_> = report
        .ran
        .iter()
        .map(|family| {
            (
                family.mutation,
                family.in_domain,
                family.in_domain_ok,
                family.negative,
                family.negative_ok,
            )
        })
        .collect();
    assert_eq!(
        counts,
        vec![
            (Mutation::ValueEdit, 0, 0, 0, 0),
            (Mutation::FieldEdit, 18, 18, 0, 0),
            (Mutation::AddNode, 0, 0, 0, 0),
            (Mutation::RemoveNode, 0, 0, 0, 0),
            (Mutation::AddEdge, 0, 0, 0, 0),
            (Mutation::RemoveEdge, 0, 0, 0, 0),
            (Mutation::Reconnect, 0, 0, 0, 0),
            (Mutation::Reorder, 9, 9, 0, 0),
            (Mutation::AddParallelEdge, 0, 0, 0, 0),
            (Mutation::RemoveSynthesized, 0, 0, 2, 2),
            (Mutation::EditSynthesized, 0, 0, 2, 2),
        ]
    );
    assert_eq!(
        report.skipped,
        vec![
            (Mutation::RelabelNode, SkipReason::NoCandidate),
            (Mutation::RelabelEdge, SkipReason::NoCandidate),
            (Mutation::AddSelfLoop, SkipReason::NoCandidate),
            (Mutation::AddContractedEdge, SkipReason::NoCandidate),
            (Mutation::ReservedIdReuse, SkipReason::NoCandidate),
            (Mutation::MalformedFan, SkipReason::NoCandidate),
        ]
    );
}

#[test]
fn redirected_occurrence_basis_ignores_collateral_source_slot_order() {
    let (resolution, _) = posts_resolution(|reference| {
        reference.mode = ResolveMode::Redirect;
    });
    let mut source = posts_tree();
    source.arcs.rotate_left(4);
    let report = check_resolution_laws(
        &resolution,
        &AnyInstance::Tree(source),
        &ResolveOptions::default(),
        &ctx(),
    )
    .expect("redirect fragment rank is outside the exact-negative basis");
    for mutation in [Mutation::RemoveSynthesized, Mutation::EditSynthesized] {
        assert!(
            report
                .skipped
                .contains(&(mutation, SkipReason::NoCandidate)),
            "{mutation:?} must not predict collateral source-slot refusals"
        );
    }
}

#[test]
fn resolution_compiles_a_valid_provider_spec() {
    let (resolution, posts) = posts_resolution(|_| {});
    assert_eq!(resolution.spec().kind, ResolutionKind::TreeToGraph);
    assert_eq!(resolution.protocol_name(), "posts");
    assert_eq!(resolution.references().len(), 1);
    assert_eq!(
        resolution.identity(),
        &panproto_inst::identity::resolution_identity("posts", &posts.schema, resolution.spec())
    );
}

#[test]
fn semantic_schema_contains_each_resolved_edge() {
    let (resolution, posts) = posts_resolution(|_| {});
    for reference in resolution.references().values() {
        let label = reference
            .edge
            .as_ref()
            .expect("a resolving reference has an edge");
        assert!(resolution.semantic_schema().edges.contains_key(label));
    }
    assert!(
        resolution
            .semantic_schema()
            .edges
            .contains_key(&posts.author_ref)
    );
}

/// Compile the posts fixture with `alter` applied to its protocol, spec
/// and provider, returning the compilation error.
fn posts_refusal(
    alter: impl FnOnce(&mut Protocol, &mut ResolutionSpec, &mut ResolutionKind),
) -> ResolveError {
    let posts = posts_schema(|_| {});
    let mut protocol = protocol("posts", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let mut spec = spec_for(
        "posts",
        ResolutionKind::TreeToGraph,
        &posts.schema,
        &[(&posts.author_ref, "post_author")],
        &[],
    );
    let mut kind = ResolutionKind::TreeToGraph;
    alter(&mut protocol, &mut spec, &mut kind);
    let provider = Provider {
        id: provider_id("posts"),
        protocol: protocol.clone(),
        kind,
        spec,
        paths: BTreeMap::new(),
        context: None,
    };
    Resolution::for_protocol(&protocol, &posts.schema, Arc::new(provider))
        .expect_err("the altered fixture is refused")
}

#[test]
fn compilation_refuses_each_inconsistent_plan() {
    let other = protocol("posts", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let posts = posts_schema(|_| {});
    let provider = Provider {
        id: provider_id("elsewhere"),
        protocol: protocol(
            "elsewhere",
            InstanceShapeKind::Tree,
            InstanceShapeKind::Graph,
        ),
        kind: ResolutionKind::TreeToGraph,
        spec: spec_for(
            "elsewhere",
            ResolutionKind::TreeToGraph,
            &posts.schema,
            &[],
            &[],
        ),
        paths: BTreeMap::new(),
        context: None,
    };
    assert!(matches!(
        Resolution::for_protocol(&other, &posts.schema, Arc::new(provider)),
        Err(ResolveError::ProviderUnavailable { protocol }) if protocol == "posts"
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| spec.provider.version = 9),
        ResolveError::ProviderFailure { code, .. } if code == "spec_provider"
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| spec.kind = ResolutionKind::TableToGraph),
        ResolveError::ProviderFailure { code, .. } if code == "spec_kind"
    ));
    assert!(matches!(
        posts_refusal(|protocol, _, _| protocol.semantic_shape = InstanceShapeKind::Table),
        ResolveError::UnsupportedKind {
            syntactic: InstanceShapeKind::Tree,
            semantic: InstanceShapeKind::Table
        }
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| {
            spec.assembly = Some(panproto_io::resolution::AssemblyPlanId {
                grammar: "g".into(),
                version: 1,
            });
        }),
        ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::Assembly(ResolutionKind::TreeToGraph),
            ..
        }
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| {
            let mut conflicting = posts_schema(|_| {}).schema.declarations.references
                ["post_author"]
                .clone();
            conflicting.nullable = true;
            spec.protocol_references.insert("post_author".into(), conflicting);
        }),
        ResolveError::ReferenceConflict { reference } if reference == "post_author"
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| spec.reference_writers.clear()),
        ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::ReferenceWriter,
            ..
        }
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| {
            let label = posts_schema(|_| {}).author_ref;
            spec.reference_writers.insert(label, "missing".into());
        }),
        ResolveError::ReferenceConflict { reference } if reference == "missing"
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| {
            spec.semantic_schema.edges.clear();
        }),
        ResolveError::Schema(panproto_schema::KeyRefError::PathDoesNotResolve { .. })
    ));
    assert!(matches!(
        posts_refusal(|_, spec, _| {
            spec.structural_edges
                .insert(edge("doc", "nowhere", "prop", Some("x")));
        }),
        ResolveError::Schema(panproto_schema::KeyRefError::PathDoesNotResolve { .. })
    ));
}

#[test]
fn a_reference_without_an_edge_is_refused_for_a_graph_target() {
    let posts = posts_schema(|reference| reference.edge = None);
    let protocol = protocol("posts", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let provider = Provider {
        id: provider_id("posts"),
        protocol: protocol.clone(),
        kind: ResolutionKind::TreeToGraph,
        spec: spec_for(
            "posts",
            ResolutionKind::TreeToGraph,
            &posts.schema,
            &[],
            &[],
        ),
        paths: BTreeMap::new(),
        context: None,
    };
    let error = Resolution::for_protocol(&protocol, &posts.schema, Arc::new(provider))
        .expect_err("a reference of a resolving kind needs an edge");
    assert!(matches!(
        error,
        ResolveError::Schema(panproto_schema::KeyRefError::EdgeRequired { reference })
            if reference == "post_author"
    ));
}

#[test]
fn get_and_put_reject_the_wrong_shape() {
    let (resolution, _) = posts_resolution(|_| {});
    let ctx = ResolveContext::empty();
    let graph = AnyInstance::Graph(GInstance::new());
    assert!(matches!(
        resolution.get(&graph, &ResolveOptions::default(), &ctx),
        Err(ResolveError::ShapeMismatch {
            expected: InstanceShapeKind::Tree,
            found: InstanceShapeKind::Graph
        })
    ));
    let (_, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    let tree = AnyInstance::Tree(posts_tree());
    assert!(matches!(
        resolution.put(&tree, &complement, &ctx),
        Err(ResolveError::ShapeMismatch {
            expected: InstanceShapeKind::Graph,
            found: InstanceShapeKind::Tree
        })
    ));
    assert_eq!(
        resolution.check_put_domain(&tree, &complement, &ctx),
        Err(Vec::new())
    );
}

#[test]
fn resolution_get_stamps_its_complement() {
    let (resolution, _) = posts_resolution(|_| {});
    let (_, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    assert_eq!(complement.identity.as_ref(), Some(resolution.identity()));
}

#[test]
fn resolution_put_rejects_the_wrong_spec() {
    let (resolution, posts) = posts_resolution(|_| {});
    let other = compile_with(
        "posts",
        ResolutionKind::TreeToGraph,
        &posts.schema,
        &[(&posts.author_ref, "post_author")],
        &[ResolutionContextKind::ExternalText],
        BTreeMap::new(),
    );
    let (view, complement) = get(&other, &AnyInstance::Tree(posts_tree()));
    assert_ne!(other.identity(), resolution.identity());
    let error = resolution
        .put(
            &AnyInstance::Graph(view.clone()),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("a complement of another spec");
    assert!(matches!(error, ResolveError::Identity(_)));
    let mut unbound = complement;
    unbound.identity = None;
    assert!(matches!(
        resolution.put(
            &AnyInstance::Graph(view),
            &unbound,
            &ResolveContext::empty()
        ),
        Err(ResolveError::Identity(_))
    ));
}

#[test]
fn canonical_put_stamps_its_complement() {
    let (resolution, _) = posts_resolution(|_| {});
    let source = AnyInstance::Tree(canonical_posts_tree());
    let (view, _) = get(&resolution, &source);
    let (canonical, complement) = resolution
        .put_canonical(&AnyInstance::Graph(view.clone()), &ResolveContext::empty())
        .expect("canonical put of a resolved view");
    assert_eq!(complement.identity.as_ref(), Some(resolution.identity()));
    let restored = resolution
        .put(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .expect("the canonical complement puts the view");
    assert!(same(&canonical, &restored));
    assert!(same(&canonical, &source));
}

#[test]
fn a_resolution_get_and_canonical_put_stamp_the_resolution_envelope() {
    let (resolution, posts) = posts_resolution(|_| {});
    let (view, complement) = get(&resolution, &AnyInstance::Tree(canonical_posts_tree()));
    let envelope = complement.identity.expect("stamped");
    assert_eq!(
        envelope.source_schema,
        panproto_inst::identity::schema_digest(&posts.schema)
    );
    assert_eq!(
        envelope.target_schema,
        panproto_inst::identity::schema_digest(resolution.semantic_schema())
    );
    assert_eq!(
        envelope.lens,
        panproto_inst::identity::resolution_digest(
            "posts",
            ResolutionKind::TreeToGraph,
            &envelope.source_schema,
            &envelope.target_schema,
            &panproto_inst::identity::resolution_spec_digest(resolution.spec()),
        )
    );
    assert_eq!(
        envelope.complement_format,
        panproto_inst::ComplementShape::Resolution.current_format()
    );
    let (_, canonical) = resolution
        .put_canonical(&AnyInstance::Graph(view), &ResolveContext::empty())
        .expect("canonical put");
    assert_eq!(canonical.identity, Some(envelope));
}

#[test]
fn on_unresolved_takes_precedence_over_lenient() {
    let dangling = AnyInstance::Tree(posts_tree_with(
        Value::Str("u1".into()),
        Value::Str("ghost".into()),
    ));
    let ctx = ResolveContext::empty();
    let (strict, _) = posts_resolution(|_| {});
    match strict.get(&dangling, &ResolveOptions::default(), &ctx) {
        Err(ResolveError::DanglingReference { reference, at, key }) => {
            assert_eq!(reference, "post_author");
            assert_eq!(*at, ViewRef::Node(7));
            assert_eq!(key, vec![Value::Str("ghost".into())]);
        }
        other => panic!("strict get of an Error reference must fail, got {other:?}"),
    }
    let lenient = ResolveOptions { lenient: true };
    let (_, complement) = strict.get(&dangling, &lenient, &ctx).expect("lenient get");
    assert_eq!(complement.unresolved.len(), 1);
    assert_eq!(complement.unresolved[0].holder, ViewRef::Node(7));
    assert!(matches!(
        complement.unresolved[0].reason,
        UnresolvedReason::Dangling
    ));
    laws_hold(&strict, &dangling, lenient);
    let (recorded, _) =
        posts_resolution(|reference| reference.on_unresolved = OnUnresolved::Complement);
    let (_, complement) = recorded
        .get(&dangling, &ResolveOptions::default(), &ctx)
        .expect("a Complement reference records the occurrence even under strict options");
    assert_eq!(complement.unresolved.len(), 1);
    laws_hold(&recorded, &dangling, ResolveOptions::default());
}

#[test]
fn resolution_ambiguity_is_recorded() {
    let mut source = posts_tree();
    if let Some(node) = source.nodes.get_mut(&4) {
        node.value = Some(FieldPresence::Present(Value::Str("u1".into())));
    }
    let source = AnyInstance::Tree(source);
    let (resolution, _) =
        posts_resolution(|reference| reference.on_unresolved = OnUnresolved::Complement);
    let (view, complement) = get(&resolution, &source);
    let reasons: Vec<_> = complement
        .unresolved
        .iter()
        .map(|record| (record.holder.clone(), record.reason.clone()))
        .collect();
    assert_eq!(reasons.len(), 2);
    assert_eq!(reasons[0].0, ViewRef::Node(5));
    assert!(matches!(
        &reasons[0].1,
        UnresolvedReason::Ambiguous { candidates }
            if candidates == &vec![ViewRef::Node(1), ViewRef::Node(3)]
    ));
    assert_eq!(reasons[1].0, ViewRef::Node(7));
    assert!(matches!(reasons[1].1, UnresolvedReason::Dangling));
    assert!(
        edge_triples(&view)
            .iter()
            .all(|(_, _, label)| label != "author_ref")
    );
    laws_hold(&resolution, &source, ResolveOptions::default());
    let (strict, _) = posts_resolution(|_| {});
    assert!(matches!(
        strict.get(&source, &ResolveOptions::default(), &ResolveContext::empty()),
        Err(ResolveError::AmbiguousReference { candidates, .. })
            if candidates == vec![ViewRef::Node(1), ViewRef::Node(3)]
    ));
}

#[test]
fn resolution_duplicates_are_kept() {
    let source = AnyInstance::Tree(posts_tree_with(
        Value::List(vec![Value::Str("u1".into()), Value::Str("u1".into())]),
        Value::List(vec![Value::Str("u2".into())]),
    ));
    let (resolution, _) = posts_resolution(|reference| reference.cardinality = Cardinality::Many);
    let (view, complement) = get(&resolution, &source);
    let references: Vec<_> = edge_triples(&view)
        .into_iter()
        .filter(|(_, _, label)| label == "author_ref")
        .collect();
    assert_eq!(
        references,
        vec![
            (5, 1, "author_ref".to_owned()),
            (5, 1, "author_ref".to_owned()),
            (7, 3, "author_ref".to_owned())
        ]
    );
    let indexes: Vec<u32> = complement
        .occurrences
        .values()
        .flatten()
        .filter(|record| record.holder == ViewRef::Node(5))
        .map(|record| record.index)
        .collect();
    assert_eq!(indexes, vec![0, 1]);
    let report = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx())
        .expect("duplicate occurrence ranks satisfy the laws");
    for mutation in [Mutation::RemoveSynthesized, Mutation::EditSynthesized] {
        let family = report
            .ran
            .iter()
            .find(|family| family.mutation == mutation)
            .expect("the other holder has one independently certifiable occurrence");
        assert_eq!(
            (family.negative, family.negative_ok),
            (1, 1),
            "{mutation:?} must omit both duplicate-rank carriers"
        );
    }
}

#[test]
fn put_keeps_duplicate_add_and_remove() {
    let (resolution, _) = posts_resolution(|reference| {
        reference.cardinality = Cardinality::Many;
        reference.mode = ResolveMode::Redirect;
    });
    let source = AnyInstance::Tree(posts_tree_with(
        Value::List(vec![Value::Str("u1".into()), Value::Str("u1".into())]),
        Value::List(vec![Value::Str("u2".into())]),
    ));
    let (view, complement) = get(&resolution, &source);
    let duplicates: Vec<panproto_inst::EdgeId> = complement
        .occurrences
        .iter()
        .filter_map(|(carrier, records)| match carrier {
            ViewRef::Edge(id) if records[0].holder == ViewRef::Node(5) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(duplicates.len(), 2);
    let author = |tree: &WInstance| tree.nodes.get(&6).and_then(|node| node.value.clone());
    let list = |count: usize| {
        Some(FieldPresence::Present(Value::List(vec![
            Value::Str(
                "u1".into()
            );
            count
        ])))
    };
    let references_to_user = |tree: WInstance| {
        let (again, _) = get(&resolution, &AnyInstance::Tree(tree));
        edge_triples(&again)
            .into_iter()
            .filter(|(_, target, label)| *target == 1 && label == "author_ref")
            .count()
    };

    // Removing either duplicate drops one element, and the other one keeps
    // its target.
    for gone in &duplicates {
        let mut removed = view.clone();
        removed.remove_edge(*gone).expect("remove one duplicate");
        let restored = put_tree(&resolution, &removed, &complement).expect("admitted");
        assert_eq!(author(&restored), list(1));
        assert_eq!(references_to_user(restored), 1);
    }

    // Removing both removes the redirected post.
    let mut both = view.clone();
    for gone in &duplicates {
        both.remove_edge(*gone).expect("remove a duplicate");
    }
    let restored = put_tree(&resolution, &both, &complement).expect("admitted");
    assert!(!restored.nodes.contains_key(&5));
    assert_eq!(references_to_user(restored), 0);

    // Adding a third equal edge from the attachment parent adds a holder
    // with the same literal; the two duplicates stay.
    let mut added = view.clone();
    let copy = added
        .edge(duplicates[0])
        .expect("an occurrence edge")
        .clone();
    added
        .insert_edge(panproto_inst::EdgeId(60), copy)
        .expect("a third equal reference");
    let restored = put_tree(&resolution, &added, &complement).expect("admitted");
    assert_eq!(author(&restored), list(2));
    assert_eq!(references_to_user(restored), 3);

    let unchanged = put_tree(&resolution, &view, &complement).expect("GetPut");
    assert_eq!(author(&unchanged), list(2));
    laws_hold(&resolution, &source, ResolveOptions::default());
}

// ---------------------------------------------------------------------------
// The aliases fixture: a map whose items are anchored nodes and aliases
// naming an anchor, each alias redirected to the node it names.
// ---------------------------------------------------------------------------

struct Aliases {
    schema: Schema,
    alias_of: Edge,
    item: Edge,
    item_alias: Edge,
    val: Edge,
}

fn aliases_schema(alter: impl FnOnce(&mut Reference)) -> Aliases {
    let base = protocol("aliases", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let alias_of = edge("alias", "node", "ref", Some("alias_of"));
    let mut alias_ref = reference(
        "alias_ref",
        "alias",
        RefSource::Variable {
            field: Vec::new(),
            binder_key: "anchor".into(),
        },
        "node",
        alias_of.clone(),
    );
    alias_ref.mode = ResolveMode::Redirect;
    alter(&mut alias_ref);
    let schema = SchemaBuilder::new(&base)
        .vertex("map", "object", None)
        .unwrap()
        .vertex("node", "object", None)
        .unwrap()
        .vertex("value", "string", None)
        .unwrap()
        .vertex("alias", "string", None)
        .unwrap()
        .vertex("anchor_name", "string", None)
        .unwrap()
        .edge("map", "node", "prop", Some("item"))
        .unwrap()
        .edge("map", "alias", "prop", Some("item"))
        .unwrap()
        .edge("node", "value", "prop", Some("val"))
        .unwrap()
        .edge("node", "anchor_name", "prop", Some("anchor"))
        .unwrap()
        .edge("alias", "node", "ref", Some("alias_of"))
        .unwrap()
        .entry("map")
        .key(
            "node",
            key(
                "anchor",
                vec![KeyPart::Path(vec![prop("node", "anchor_name", "anchor")])],
                KeyRole::Identifier,
                KeyScope::Document,
            ),
        )
        .reference(alias_ref)
        .build()
        .unwrap();
    Aliases {
        schema,
        alias_of,
        item: prop("map", "node", "item"),
        item_alias: prop("map", "alias", "item"),
        val: prop("node", "value", "val"),
    }
}

fn anchored(id: u32, anchor: &str) -> Node {
    let mut node = Node::new(id, "node");
    node.extra_fields
        .insert("anchor".to_owned(), Value::Str(anchor.to_owned()));
    node
}

/// map 0: node 1 (anchor "a", value 2 = "x"), alias 3 = "a", node 4
/// (anchor "b", value 5 = "y"), alias 6 = "b".
fn aliases_tree(fixture: &Aliases) -> WInstance {
    tree(
        vec![
            Node::new(0, "map"),
            anchored(1, "a"),
            leaf(2, "value", Value::Str("x".into())),
            leaf(3, "alias", Value::Str("a".into())),
            anchored(4, "b"),
            leaf(5, "value", Value::Str("y".into())),
            leaf(6, "alias", Value::Str("b".into())),
        ],
        vec![
            (0, 1, fixture.item.clone()),
            (1, 2, fixture.val.clone()),
            (0, 3, fixture.item_alias.clone()),
            (0, 4, fixture.item.clone()),
            (4, 5, fixture.val.clone()),
            (0, 6, fixture.item_alias.clone()),
        ],
        0,
    )
}

fn aliases() -> (Resolution, Aliases) {
    let fixture = aliases_schema(|_| {});
    let resolution = compile(
        "aliases",
        ResolutionKind::TreeToGraph,
        &fixture.schema,
        &[(&fixture.alias_of, "alias_ref")],
    );
    (resolution, fixture)
}

fn put_tree(
    resolution: &Resolution,
    view: &GInstance,
    complement: &ResolutionComplement,
) -> Result<WInstance, ResolveError> {
    match resolution.put(
        &AnyInstance::Graph(view.clone()),
        complement,
        &ResolveContext::empty(),
    )? {
        AnyInstance::Tree(tree) => Ok(tree),
        other => panic!("a tree source, got {:?}", other.shape()),
    }
}

#[test]
fn an_alias_redirects_its_incoming_arc_to_the_anchor() {
    let (resolution, fixture) = aliases();
    let source = AnyInstance::Tree(aliases_tree(&fixture));
    let (view, complement) = get(&resolution, &source);
    assert!(view.node(3).is_none() && view.node(6).is_none());
    let redirected: Vec<_> = edge_triples(&view)
        .into_iter()
        .filter(|(_, _, label)| label == "alias_of")
        .collect();
    assert_eq!(
        redirected,
        vec![(0, 1, "alias_of".to_owned()), (0, 4, "alias_of".to_owned())]
    );
    assert_eq!(complement.redirected.len(), 2);
    let fragment = &complement.redirected[&3];
    let attachment = fragment.attachment.as_ref().expect("an alias has a parent");
    assert_eq!(
        (attachment.parent, attachment.child, attachment.source_slot),
        (0, 3, 2)
    );
    assert_eq!(attachment.edge, fixture.item_alias);
    let restored = put_tree(&resolution, &view, &complement).expect("GetPut");
    assert!(same(&AnyInstance::Tree(restored), &source));
    laws_hold(&resolution, &source, ResolveOptions::default());
}

/// The occurrence edge of `holder` in a view.
fn occurrence_edge(complement: &ResolutionComplement, holder: u32) -> panproto_inst::EdgeId {
    complement
        .occurrences
        .iter()
        .find_map(|(carrier, records)| match carrier {
            ViewRef::Edge(id) if records[0].holder == ViewRef::Node(holder) => Some(*id),
            _ => None,
        })
        .expect("the holder has an occurrence edge")
}

#[test]
fn put_writes_a_reconnected_variable_reference() {
    let (resolution, fixture) = aliases();
    let source = AnyInstance::Tree(aliases_tree(&fixture));
    let (mut view, complement) = get(&resolution, &source);
    let first = occurrence_edge(&complement, 3);
    view.set_endpoints(first, 0, 4)
        .expect("reconnect the alias of a to b");
    let restored = put_tree(&resolution, &view, &complement).expect("a reconnect is admitted");
    assert_eq!(
        restored.nodes[&3].value,
        Some(FieldPresence::Present(Value::Str("b".into())))
    );
    assert_eq!(restored.arcs, aliases_tree(&fixture).arcs);
}

#[test]
fn removing_the_last_alias_removes_its_fragment() {
    let (resolution, fixture) = aliases();
    let (mut view, complement) = get(&resolution, &AnyInstance::Tree(aliases_tree(&fixture)));
    view.remove_edge(occurrence_edge(&complement, 6))
        .expect("remove the second alias");
    let restored = put_tree(&resolution, &view, &complement).expect("admitted");
    assert!(!restored.nodes.contains_key(&6));
    assert_eq!(restored.arcs.len(), 5);
}

#[test]
fn adding_an_alias_creates_a_holder() {
    let (resolution, fixture) = aliases();
    let (mut view, complement) = get(&resolution, &AnyInstance::Tree(aliases_tree(&fixture)));
    let fresh = view
        .edges()
        .keys()
        .map(|id| id.0)
        .max()
        .map(|max| panproto_inst::EdgeId(max + 1))
        .expect("the view has edges");
    view.insert_edge(
        fresh,
        panproto_inst::GEdge::new(0, 1, fixture.alias_of.clone()),
    )
    .expect("add an alias of a");
    let restored = put_tree(&resolution, &view, &complement).expect("admitted");
    assert_eq!(restored.nodes.len(), 8);
    let holder = restored
        .nodes
        .get(&7)
        .expect("a new alias node above every id");
    assert_eq!(holder.anchor, "alias");
    assert_eq!(
        holder.value,
        Some(FieldPresence::Present(Value::Str("a".into())))
    );
    assert_eq!(restored.arcs.last(), Some(&(0, 7, fixture.item_alias)));
}

/// map 0: node 1 anchored "id001", node 4 unanchored, each with a value.
fn unanchored_tree(fixture: &Aliases) -> WInstance {
    tree(
        vec![
            Node::new(0, "map"),
            anchored(1, "id001"),
            leaf(2, "value", text("x")),
            Node::new(4, "node"),
            leaf(5, "value", text("y")),
        ],
        vec![
            (0, 1, fixture.item.clone()),
            (1, 2, fixture.val.clone()),
            (0, 4, fixture.item.clone()),
            (4, 5, fixture.val.clone()),
        ],
        0,
    )
}

#[test]
fn an_alias_of_an_unanchored_node_is_unrepresentable_at_its_edge() {
    // Put generates no anchor: the anchor is a field of the target, a node
    // of the view, so a name put wrote there would change the view.
    let fixture = aliases_schema(|reference| reference.on_unresolved = OnUnresolved::Complement);
    let resolution = compile(
        "aliases",
        ResolutionKind::TreeToGraph,
        &fixture.schema,
        &[(&fixture.alias_of, "alias_ref")],
    );
    let source = AnyInstance::Tree(unanchored_tree(&fixture));
    let (mut view, complement) = get(&resolution, &source);
    let fresh = panproto_inst::EdgeId(view.edges().keys().map(|id| id.0 + 1).max().unwrap_or(0));
    view.insert_edge(fresh, panproto_inst::GEdge::new(0, 4, fixture.alias_of))
        .expect("an alias of the unanchored node");
    assert_eq!(
        violations_of(&resolution, &view, &complement, &ctx()),
        vec![ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Edge(fresh),
            reference: "alias_ref".into(),
        }]
    );
    match resolution.put_canonical(&AnyInstance::Graph(view), &ctx()) {
        Err(ResolveError::PutDomain(ResolveDomainError::UnrepresentableReference {
            carrier,
            ..
        })) => assert_eq!(carrier, ViewRef::Edge(fresh)),
        other => panic!("canonical put names the alias edge, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The variables fixture: concepts that bind their own variable, the first
// occurrence of a name in document order defining it and every later one
// a bare reference redirected to the definition.
// ---------------------------------------------------------------------------

struct Variables {
    schema: Schema,
    reentrancy: Edge,
    arg: [Edge; 3],
}

fn variables_schema() -> Variables {
    let base = protocol(
        "variables",
        InstanceShapeKind::Tree,
        InstanceShapeKind::Graph,
    );
    let reentrancy = edge("concept", "concept", "ref", Some("reentrancy"));
    let var = prop("concept", "var_name", "var");
    let mut variable = reference(
        "variable",
        "concept",
        RefSource::Variable {
            field: vec![var.clone()],
            binder_key: "var".into(),
        },
        "concept",
        reentrancy.clone(),
    );
    variable.mode = ResolveMode::Redirect;
    let schema = SchemaBuilder::new(&base)
        .vertex("concept", "object", None)
        .unwrap()
        .vertex("var_name", "string", None)
        .unwrap()
        .edge("concept", "concept", "prop", Some("ARG0"))
        .unwrap()
        .edge("concept", "concept", "prop", Some("ARG1"))
        .unwrap()
        .edge("concept", "concept", "prop", Some("ARG2"))
        .unwrap()
        .edge("concept", "var_name", "prop", Some("var"))
        .unwrap()
        .edge("concept", "concept", "ref", Some("reentrancy"))
        .unwrap()
        .entry("concept")
        .key(
            "concept",
            key(
                "var",
                vec![KeyPart::Path(vec![var])],
                KeyRole::Identifier,
                KeyScope::Document,
            ),
        )
        .reference(variable)
        .build()
        .unwrap();
    Variables {
        schema,
        reentrancy,
        arg: [
            prop("concept", "concept", "ARG0"),
            prop("concept", "concept", "ARG1"),
            prop("concept", "concept", "ARG2"),
        ],
    }
}

fn concept(id: u32, var: &str, instance: Option<&str>) -> Node {
    let mut node = Node::new(id, "concept");
    node.extra_fields
        .insert("var".to_owned(), Value::Str(var.to_owned()));
    if let Some(instance) = instance {
        node.extra_fields
            .insert("instance".to_owned(), Value::Str(instance.to_owned()));
    }
    node
}

/// (a / want :ARG0 (b / boy) :ARG1 (g / go :ARG0 b) :ARG2 (d / dog)):
/// want 0, boy 1, go 2, the bare b 3, dog 4.
fn variables_tree(fixture: &Variables) -> WInstance {
    let [arg0, arg1, arg2] = fixture.arg.clone();
    tree(
        vec![
            concept(0, "a", Some("want")),
            concept(1, "b", Some("boy")),
            concept(2, "g", Some("go")),
            concept(3, "b", None),
            concept(4, "d", Some("dog")),
        ],
        vec![
            (0, 1, arg0.clone()),
            (0, 2, arg1),
            (2, 3, arg0),
            (0, 4, arg2),
        ],
        0,
    )
}

fn variables() -> (Resolution, Variables) {
    let fixture = variables_schema();
    let resolution = compile(
        "variables",
        ResolutionKind::TreeToGraph,
        &fixture.schema,
        &[(&fixture.reentrancy, "variable")],
    );
    (resolution, fixture)
}

#[test]
fn the_first_binding_defines_and_later_ones_refer() {
    let (resolution, fixture) = variables();
    let source = AnyInstance::Tree(variables_tree(&fixture));
    let (view, complement) = get(&resolution, &source);
    assert!(view.node(3).is_none());
    assert!(edge_triples(&view).contains(&(2, 1, "reentrancy".to_owned())));
    let defined: Vec<u32> = complement
        .definitions
        .keys()
        .map(|key| key.target)
        .collect();
    assert_eq!(defined, vec![0, 1, 2, 4]);
    let restored = put_tree(&resolution, &view, &complement).expect("GetPut");
    assert!(same(&AnyInstance::Tree(restored), &source));
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn a_captured_reference_is_unrepresentable() {
    let (resolution, fixture) = variables();
    let (mut view, complement) = get(&resolution, &AnyInstance::Tree(variables_tree(&fixture)));
    let reentrant = occurrence_edge(&complement, 3);
    view.set_endpoints(reentrant, 2, 4)
        .expect("point the bare b at the dog");
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("the rewritten reference precedes the definition of d");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        ResolveDomainError::UnrepresentableReference { reference, .. } if reference == "variable"
    )), "{violations:?}");
}

#[test]
fn a_reconnected_reference_to_an_earlier_definition_is_written() {
    let (resolution, fixture) = variables();
    let (mut view, complement) = get(&resolution, &AnyInstance::Tree(variables_tree(&fixture)));
    let reentrant = occurrence_edge(&complement, 3);
    view.set_endpoints(reentrant, 2, 0)
        .expect("point the bare b at want");
    let restored = put_tree(&resolution, &view, &complement).expect("admitted");
    assert_eq!(
        restored.nodes[&3].extra_fields["var"],
        Value::Str("a".into())
    );
}

// ---------------------------------------------------------------------------
// The sections fixture: item codes are unique within a section, and a
// reference resolves only within its holder's own section.
// ---------------------------------------------------------------------------

fn sections_schema(policy: OnUnresolved) -> (Schema, Edge) {
    let base = protocol(
        "sections",
        InstanceShapeKind::Tree,
        InstanceShapeKind::Graph,
    );
    let see_ref = edge("item", "item", "ref", Some("see_ref"));
    let mut see = reference(
        "see",
        "item",
        RefSource::KeyValue {
            parts: vec![vec![prop("item", "see", "see")]],
        },
        "item",
        see_ref.clone(),
    );
    see.target_key = Some("code".into());
    see.nullable = true;
    see.on_unresolved = policy;
    let schema = SchemaBuilder::new(&base)
        .vertex("catalog", "object", None)
        .unwrap()
        .vertex("section", "object", None)
        .unwrap()
        .vertex("item", "object", None)
        .unwrap()
        .vertex("code", "string", None)
        .unwrap()
        .vertex("see", "string", None)
        .unwrap()
        .edge("catalog", "section", "prop", Some("sections"))
        .unwrap()
        .edge("section", "item", "prop", Some("items"))
        .unwrap()
        .edge("catalog", "item", "prop", Some("items"))
        .unwrap()
        .edge("item", "code", "prop", Some("code"))
        .unwrap()
        .edge("item", "see", "prop", Some("see"))
        .unwrap()
        .edge("item", "item", "ref", Some("see_ref"))
        .unwrap()
        .entry("catalog")
        .key(
            "item",
            key(
                "code",
                vec![KeyPart::Path(vec![prop("item", "code", "code")])],
                KeyRole::Unique,
                KeyScope::Within("section".into()),
            ),
        )
        .reference(see)
        .build()
        .unwrap();
    (schema, see_ref)
}

fn item(id: u32, code: &str, see: Option<&str>) -> Node {
    let mut node = Node::new(id, "item");
    node.extra_fields
        .insert("code".to_owned(), Value::Str(code.to_owned()));
    if let Some(see) = see {
        node.extra_fields
            .insert("see".to_owned(), Value::Str(see.to_owned()));
    }
    node
}

/// catalog 0; section 1 with items 2 (code x, see y) and 3 (code y);
/// section 4 with item 5 (code y, see x); item 6 (code x, see x) directly
/// under the catalog.
fn sections_tree() -> WInstance {
    tree(
        vec![
            Node::new(0, "catalog"),
            Node::new(1, "section"),
            item(2, "x", Some("y")),
            item(3, "y", None),
            Node::new(4, "section"),
            item(5, "y", Some("x")),
            item(6, "x", Some("x")),
        ],
        vec![
            (0, 1, prop("catalog", "section", "sections")),
            (1, 2, prop("section", "item", "items")),
            (1, 3, prop("section", "item", "items")),
            (0, 4, prop("catalog", "section", "sections")),
            (4, 5, prop("section", "item", "items")),
            (0, 6, prop("catalog", "item", "items")),
        ],
        0,
    )
}

fn sections(policy: OnUnresolved) -> Resolution {
    let (schema, see_ref) = sections_schema(policy);
    compile(
        "sections",
        ResolutionKind::TreeToGraph,
        &schema,
        &[(&see_ref, "see")],
    )
}

#[test]
fn resolution_scope_leak_is_refused() {
    let source = AnyInstance::Tree(sections_tree());
    let resolution = sections(OnUnresolved::Complement);
    let (view, complement) = get(&resolution, &source);
    let references: Vec<_> = edge_triples(&view)
        .into_iter()
        .filter(|(_, _, label)| label == "see_ref")
        .collect();
    assert_eq!(references, vec![(2, 3, "see_ref".to_owned())]);
    let dangling: Vec<ViewRef> = complement
        .unresolved
        .iter()
        .map(|record| record.holder.clone())
        .collect();
    assert_eq!(dangling, vec![ViewRef::Node(5), ViewRef::Node(6)]);
    laws_hold(&resolution, &source, ResolveOptions::default());
    let strict = sections(OnUnresolved::Error);
    assert!(matches!(
        strict.get(&source, &ResolveOptions::default(), &ResolveContext::empty()),
        Err(ResolveError::DanglingReference { at, .. }) if *at == ViewRef::Node(5)
    ));
}

#[test]
fn a_holder_without_a_within_ancestor_does_not_resolve() {
    let source = AnyInstance::Tree(sections_tree());
    let (_, complement) = get(&sections(OnUnresolved::Complement), &source);
    let loose = complement
        .unresolved
        .iter()
        .find(|record| record.holder == ViewRef::Node(6))
        .expect("the loose item's reference is recorded");
    assert!(matches!(loose.reason, UnresolvedReason::Dangling));
}

// ---------------------------------------------------------------------------
// The grid fixture: a selection names cells by composite many-valued keys,
// zipped positionally.
// ---------------------------------------------------------------------------

fn grid_schema(nullable: bool) -> (Schema, Edge) {
    let base = protocol("grid", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let selected = edge("sel", "cell", "ref", Some("selected"));
    let mut selects = reference(
        "selects",
        "sel",
        RefSource::KeyValue {
            parts: vec![
                vec![prop("sel", "num", "rows")],
                vec![prop("sel", "num", "cols")],
            ],
        },
        "cell",
        selected.clone(),
    );
    selects.cardinality = Cardinality::Many;
    selects.nullable = nullable;
    let schema = SchemaBuilder::new(&base)
        .vertex("grid", "object", None)
        .unwrap()
        .vertex("cell", "object", None)
        .unwrap()
        .vertex("sel", "object", None)
        .unwrap()
        .vertex("num", "integer", None)
        .unwrap()
        .edge("grid", "cell", "prop", Some("cells"))
        .unwrap()
        .edge("grid", "sel", "prop", Some("sel"))
        .unwrap()
        .edge("cell", "num", "prop", Some("r"))
        .unwrap()
        .edge("cell", "num", "prop", Some("c"))
        .unwrap()
        .edge("sel", "num", "prop", Some("rows"))
        .unwrap()
        .edge("sel", "num", "prop", Some("cols"))
        .unwrap()
        .edge("sel", "cell", "ref", Some("selected"))
        .unwrap()
        .entry("grid")
        .key(
            "cell",
            key(
                "rc",
                vec![
                    KeyPart::Path(vec![prop("cell", "num", "r")]),
                    KeyPart::Path(vec![prop("cell", "num", "c")]),
                ],
                KeyRole::Primary,
                KeyScope::Document,
            ),
        )
        .reference(selects)
        .build()
        .unwrap();
    (schema, selected)
}

fn cell(id: u32, r: i64, c: i64) -> Node {
    let mut node = Node::new(id, "cell");
    node.extra_fields.insert("r".to_owned(), Value::Int(r));
    node.extra_fields.insert("c".to_owned(), Value::Int(c));
    node
}

/// grid 0 with cells 1 (1,1), 2 (2,1), 3 (1,2) and a selection 4 whose
/// rows and cols are the given lists.
fn grid_tree(rows: Vec<i64>, cols: Vec<i64>) -> WInstance {
    let mut sel = Node::new(4, "sel");
    sel.extra_fields.insert(
        "rows".to_owned(),
        Value::List(rows.into_iter().map(Value::Int).collect()),
    );
    sel.extra_fields.insert(
        "cols".to_owned(),
        Value::List(cols.into_iter().map(Value::Int).collect()),
    );
    tree(
        vec![
            Node::new(0, "grid"),
            cell(1, 1, 1),
            cell(2, 2, 1),
            cell(3, 1, 2),
            sel,
        ],
        vec![
            (0, 1, prop("grid", "cell", "cells")),
            (0, 2, prop("grid", "cell", "cells")),
            (0, 3, prop("grid", "cell", "cells")),
            (0, 4, prop("grid", "sel", "sel")),
        ],
        0,
    )
}

fn grid(nullable: bool) -> Resolution {
    let (schema, selected) = grid_schema(nullable);
    compile(
        "grid",
        ResolutionKind::TreeToGraph,
        &schema,
        &[(&selected, "selects")],
    )
}

#[test]
fn composite_many_keys_zip_their_parts() {
    let source = AnyInstance::Tree(grid_tree(vec![1, 2, 1], vec![1, 1, 1]));
    let resolution = grid(false);
    let (view, complement) = get(&resolution, &source);
    let selected: Vec<_> = edge_triples(&view)
        .into_iter()
        .filter(|(_, _, label)| label == "selected")
        .collect();
    assert_eq!(
        selected,
        vec![
            (4, 1, "selected".to_owned()),
            (4, 1, "selected".to_owned()),
            (4, 2, "selected".to_owned())
        ]
    );
    let literals: Vec<Value> = complement
        .occurrences
        .values()
        .map(|records| records[0].literal.clone())
        .collect();
    assert_eq!(
        literals,
        vec![
            Value::List(vec![Value::Int(1), Value::Int(1)]),
            Value::List(vec![Value::Int(2), Value::Int(1)]),
            Value::List(vec![Value::Int(1), Value::Int(1)])
        ]
    );
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn resolution_composite_cardinality_mismatch() {
    let resolution = grid(false);
    let ctx = ResolveContext::empty();
    let options = ResolveOptions::default();
    match resolution.get(
        &AnyInstance::Tree(grid_tree(vec![1, 2], vec![1])),
        &options,
        &ctx,
    ) {
        Err(ResolveError::ReferenceCardinalityMismatch {
            reference,
            at,
            lengths,
        }) => {
            assert_eq!(reference, "selects");
            assert_eq!(*at, ViewRef::Node(4));
            assert_eq!(lengths, vec![2, 1]);
        }
        other => panic!("unequal parts are a mismatch, got {other:?}"),
    }
    assert!(matches!(
        resolution.get(&AnyInstance::Tree(grid_tree(vec![], vec![])), &options, &ctx),
        Err(ResolveError::ReferenceCardinalityMismatch { lengths, .. }) if lengths == vec![0, 0]
    ));
    let (_, complement) = grid(true)
        .get(
            &AnyInstance::Tree(grid_tree(vec![], vec![])),
            &options,
            &ctx,
        )
        .expect("a nullable reference may be empty");
    assert!(complement.occurrences.is_empty());
}

// ---------------------------------------------------------------------------
// The links fixture: items and sections carrying identifiers, links naming
// an identifier through the provider's path syntax, and pointers holding a
// target's key as their own value.
// ---------------------------------------------------------------------------

struct Links {
    schema: Schema,
    link_to: Edge,
    points_to: Edge,
}

fn links_schema(mode: ResolveMode, policy: OnUnresolved) -> Links {
    let base = protocol("links", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let link_to = edge("link", "item", "ref", Some("link_to"));
    let points_to = edge("pointer", "item", "ref", Some("points_to"));
    let mut link = reference(
        "link",
        "link",
        RefSource::Path {
            field: Vec::new(),
            syntax: PathSyntax::IdRef,
        },
        "item",
        link_to.clone(),
    );
    link.mode = mode.clone();
    link.on_unresolved = policy;
    let mut pointer = reference(
        "pointer",
        "pointer",
        RefSource::KeyValue {
            parts: vec![Vec::new()],
        },
        "item",
        points_to.clone(),
    );
    pointer.mode = mode;
    let schema = SchemaBuilder::new(&base)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("item", "object", None)
        .unwrap()
        .vertex("section", "object", None)
        .unwrap()
        .vertex("name", "string", None)
        .unwrap()
        .vertex("link", "string", None)
        .unwrap()
        .vertex("pointer", "string", None)
        .unwrap()
        .edge("doc", "item", "prop", Some("items"))
        .unwrap()
        .edge("doc", "section", "prop", Some("sections"))
        .unwrap()
        .edge("doc", "link", "prop", Some("links"))
        .unwrap()
        .edge("doc", "pointer", "prop", Some("pointers"))
        .unwrap()
        .edge("item", "name", "prop", Some("name"))
        .unwrap()
        .edge("link", "item", "ref", Some("link_to"))
        .unwrap()
        .edge("pointer", "item", "ref", Some("points_to"))
        .unwrap()
        .entry("doc")
        .key(
            "item",
            key(
                "name",
                vec![KeyPart::Path(vec![prop("item", "name", "name")])],
                KeyRole::Primary,
                KeyScope::Document,
            ),
        )
        .reference(link)
        .reference(pointer)
        .build()
        .unwrap();
    Links {
        schema,
        link_to,
        points_to,
    }
}

fn named(id: u32, name: &str) -> Node {
    let mut node = Node::new(id, "item");
    node.extra_fields
        .insert("name".to_owned(), Value::Str(name.to_owned()));
    node
}

/// doc 0; items 1 ("i1") and 2 ("i2"); section 3; link 4 = `link`;
/// pointer 5 = "i1".
fn links_tree(link: &str) -> WInstance {
    tree(
        vec![
            Node::new(0, "doc"),
            named(1, "i1"),
            named(2, "i2"),
            Node::new(3, "section"),
            leaf(4, "link", Value::Str(link.to_owned())),
            leaf(5, "pointer", Value::Str("i1".into())),
        ],
        vec![
            (0, 1, prop("doc", "item", "items")),
            (0, 2, prop("doc", "item", "items")),
            (0, 3, prop("doc", "section", "sections")),
            (0, 4, prop("doc", "link", "links")),
            (0, 5, prop("doc", "pointer", "pointers")),
        ],
        0,
    )
}

fn links(mode: ResolveMode, policy: OnUnresolved) -> Resolution {
    let fixture = links_schema(mode, policy);
    let paths = BTreeMap::from([
        ("#i1".to_owned(), ViewRef::Node(1)),
        ("#i2".to_owned(), ViewRef::Node(2)),
        ("#s".to_owned(), ViewRef::Node(3)),
    ]);
    compile_with(
        "links",
        ResolutionKind::TreeToGraph,
        &fixture.schema,
        &[(&fixture.link_to, "link"), (&fixture.points_to, "pointer")],
        &[],
        paths,
    )
}

#[test]
fn an_idref_to_another_type_is_recorded_not_rejected() {
    let resolution = links(ResolveMode::AddEdge, OnUnresolved::Complement);
    let source = AnyInstance::Tree(links_tree("#s"));
    let (view, complement) = get(&resolution, &source);
    let link = complement
        .unresolved
        .iter()
        .find(|record| record.reference == "link")
        .expect("the link to a section is recorded");
    assert_eq!(link.holder, ViewRef::Node(4));
    assert!(matches!(link.reason, UnresolvedReason::Dangling));
    assert!(
        !edge_triples(&view)
            .iter()
            .any(|(_, _, label)| label == "link_to")
    );
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn an_idref_matching_no_id_is_recorded_not_rejected() {
    let resolution = links(ResolveMode::AddEdge, OnUnresolved::Complement);
    let source = AnyInstance::Tree(links_tree("#nowhere"));
    let (_, complement) = get(&resolution, &source);
    assert!(
        complement
            .unresolved
            .iter()
            .any(|record| record.reference == "link"
                && matches!(record.reason, UnresolvedReason::Dangling))
    );
}

#[test]
fn put_writes_each_reference_form() {
    let resolution = links(ResolveMode::Redirect, OnUnresolved::Error);
    let source = AnyInstance::Tree(links_tree("#i1"));
    let (mut view, complement) = get(&resolution, &source);
    let report = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx())
        .expect("mixed redirect reference forms satisfy the laws");
    for mutation in [Mutation::RemoveSynthesized, Mutation::EditSynthesized] {
        assert!(
            report
                .skipped
                .contains(&(mutation, SkipReason::NoCandidate)),
            "{mutation:?} must not predict mixed redirect-reference closure"
        );
    }
    view.set_endpoints(occurrence_edge(&complement, 4), 0, 2)
        .expect("relink to i2");
    view.set_endpoints(occurrence_edge(&complement, 5), 0, 2)
        .expect("repoint to i2");
    let restored = put_tree(&resolution, &view, &complement).expect("admitted");
    assert_eq!(
        restored.nodes[&4].value,
        Some(FieldPresence::Present(Value::Str("#i2".into()))),
        "the path is the provider's canonical rendering"
    );
    assert_eq!(
        restored.nodes[&5].value,
        Some(FieldPresence::Present(Value::Str("i2".into()))),
        "the key value is the target's key"
    );
}

#[test]
fn rendered_paths_resolve_to_their_target() {
    let resolution = links(ResolveMode::Redirect, OnUnresolved::Error);
    for (target, path) in [(1, "#i1"), (2, "#i2")] {
        let source = AnyInstance::Tree(links_tree(path));
        let (view, complement) = get(&resolution, &source);
        let rendered = resolution
            .provider()
            .render_path(
                &resolution.references()["link"],
                ViewRef::Node(4),
                ViewRef::Node(target),
                &AnyInstance::Graph(view.clone()),
                &ResolveContext::empty(),
            )
            .expect("the provider renders a path to an item");
        let resolved = resolution
            .provider()
            .resolve_path(
                &resolution.references()["link"],
                ViewRef::Node(4),
                &rendered,
                &source,
                &ResolveContext::empty(),
            )
            .expect("the rendered path resolves");
        assert_eq!(resolved.candidates, vec![ViewRef::Node(target)]);
        let restored = put_tree(&resolution, &view, &complement).expect("GetPut");
        assert!(same(&AnyInstance::Tree(restored), &source));
    }
}

// ---------------------------------------------------------------------------
// The catalog fixture: shelves with a title hold books with a name and
// links redirected to a book. A link's path is relative, a book's name,
// looked up on the link's own shelf first and then in the whole document,
// or absolute, "/title/name". The provider's canonical rendering is the
// relative form; the absolute form is its one more qualified form.
// ---------------------------------------------------------------------------

/// The catalog provider, whose path syntax is the fixture's.
struct Catalog {
    protocol: Protocol,
    spec: ResolutionSpec,
}

/// The parent of a node in a tree.
fn parent_in(tree: &WInstance, node: u32) -> Option<u32> {
    tree.arcs
        .iter()
        .find(|(_, child, _)| *child == node)
        .map(|(parent, _, _)| *parent)
}

/// A string field of a node.
fn text_field<'n>(node: &'n Node, field: &str) -> Option<&'n str> {
    match node.extra_fields.get(field) {
        Some(Value::Str(value)) => Some(value),
        _ => None,
    }
}

impl ResolutionProvider for Catalog {
    fn id(&self) -> ResolutionProviderId {
        provider_id("catalog")
    }

    fn kind(&self) -> ResolutionKind {
        ResolutionKind::TreeToGraph
    }

    fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    fn spec(
        &self,
        _protocol: &Protocol,
        _syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        Ok(self.spec.clone())
    }

    fn resolve_path(
        &self,
        _reference: &Reference,
        holder: ViewRef,
        literal: &Value,
        source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<PathResolution, ResolveError> {
        let (AnyInstance::Tree(tree), Value::Str(path), ViewRef::Node(holder)) =
            (source, literal, holder)
        else {
            return Ok(PathResolution {
                candidates: Vec::new(),
                context: None,
            });
        };
        let books = |shelf: Option<u32>, name: &str| -> Vec<ViewRef> {
            tree.nodes
                .values()
                .filter(|node| node.anchor == "book" && text_field(node, "name") == Some(name))
                .filter(|node| shelf.is_none() || parent_in(tree, node.id) == shelf)
                .map(|node| ViewRef::Node(node.id))
                .collect()
        };
        let mut candidates: Vec<ViewRef> = path.strip_prefix('/').map_or_else(
            || {
                let own = books(parent_in(tree, holder), path);
                if own.is_empty() {
                    books(None, path)
                } else {
                    own
                }
            },
            |absolute| {
                let (title, name) = absolute.split_once('/').unwrap_or((absolute, ""));
                tree.nodes
                    .values()
                    .filter(|node| {
                        node.anchor == "shelf" && text_field(node, "title") == Some(title)
                    })
                    .flat_map(|shelf| books(Some(shelf.id), name))
                    .collect()
            },
        );
        candidates.sort();
        Ok(PathResolution {
            candidates,
            context: None,
        })
    }

    fn render_path(
        &self,
        _reference: &Reference,
        _holder: ViewRef,
        target: ViewRef,
        view: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<Value, ResolveError> {
        let (AnyInstance::Graph(view), ViewRef::Node(target)) = (view, target) else {
            return Err(ResolveError::ProviderFailure {
                provider: self.id(),
                code: "no_path".into(),
                at: None,
            });
        };
        view.node(target)
            .and_then(|book| text_field(book, "name"))
            .map(|name| Value::Str(name.to_owned()))
            .ok_or_else(|| ResolveError::ProviderFailure {
                provider: self.id(),
                code: "no_path".into(),
                at: Some(Box::new(ViewRef::Node(target))),
            })
    }

    fn qualified_forms(
        &self,
        _reference: &Reference,
        _holder: ViewRef,
        target: ViewRef,
        first: &Value,
        view: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<Vec<Value>, ResolveError> {
        let (AnyInstance::Graph(view), ViewRef::Node(target), Value::Str(name)) =
            (view, target, first)
        else {
            return Ok(Vec::new());
        };
        let titles: Vec<Value> = view
            .in_edges(target)
            .iter()
            .filter_map(|edge| view.edge(*edge))
            .filter(|edge| edge.edge.name.as_deref() == Some("books"))
            .filter_map(|edge| view.node(edge.src))
            .filter_map(|shelf| text_field(shelf, "title"))
            .map(|title| Value::Str(format!("/{title}/{name}")))
            .collect();
        Ok(titles)
    }
}

struct CatalogFixture {
    schema: Schema,
    see: Edge,
}

fn catalog_schema() -> CatalogFixture {
    let base = protocol("catalog", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let see = edge("link", "book", "ref", Some("see"));
    let mut link = reference(
        "see",
        "link",
        RefSource::Path {
            field: Vec::new(),
            syntax: PathSyntax::JsonPointer,
        },
        "book",
        see.clone(),
    );
    link.mode = ResolveMode::Redirect;
    let schema = SchemaBuilder::new(&base)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("shelf", "object", None)
        .unwrap()
        .vertex("book", "object", None)
        .unwrap()
        .vertex("link", "string", None)
        .unwrap()
        .edge("doc", "shelf", "prop", Some("shelves"))
        .unwrap()
        .edge("shelf", "book", "prop", Some("books"))
        .unwrap()
        .edge("shelf", "link", "prop", Some("links"))
        .unwrap()
        .edge("link", "book", "ref", Some("see"))
        .unwrap()
        .entry("doc")
        .reference(link)
        .build()
        .unwrap();
    CatalogFixture { schema, see }
}

fn catalog() -> (Resolution, CatalogFixture) {
    let fixture = catalog_schema();
    let protocol = protocol("catalog", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let provider = Catalog {
        protocol: protocol.clone(),
        spec: spec_for(
            "catalog",
            ResolutionKind::TreeToGraph,
            &fixture.schema,
            &[(&fixture.see, "see")],
            &[],
        ),
    };
    let resolution = Resolution::for_protocol(&protocol, &fixture.schema, Arc::new(provider))
        .expect("the catalog compiles");
    (resolution, fixture)
}

fn shelf(id: u32, title: &str) -> Node {
    let mut node = Node::new(id, "shelf");
    node.extra_fields
        .insert("title".to_owned(), Value::Str(title.to_owned()));
    node
}

fn book(id: u32, name: &str) -> Node {
    let mut node = Node::new(id, "book");
    node.extra_fields
        .insert("name".to_owned(), Value::Str(name.to_owned()));
    node
}

/// doc 0; shelf 1 ("a") with link 4 = `link` and books 2 ("x") and 3
/// ("w"); shelf 5 (`second`) with books 6 ("x") and 7 ("y"); with
/// `third`, a shelf 8 of that title with book 9 ("x").
fn catalog_tree(link: &str, second: &str, third: Option<&str>) -> WInstance {
    let shelves = prop("doc", "shelf", "shelves");
    let books = prop("shelf", "book", "books");
    let links = prop("shelf", "link", "links");
    let mut nodes = vec![
        Node::new(0, "doc"),
        shelf(1, "a"),
        book(2, "x"),
        book(3, "w"),
        leaf(4, "link", Value::Str(link.to_owned())),
        shelf(5, second),
        book(6, "x"),
        book(7, "y"),
    ];
    // Shelf a lists its link before its books, the canonical child order
    // (a link's reference label sorts before the books label), so the
    // view's structural edge ids are the ones canonical emission gives.
    let mut arcs = vec![
        (0, 1, shelves.clone()),
        (1, 4, links),
        (1, 2, books.clone()),
        (1, 3, books.clone()),
        (0, 5, shelves.clone()),
        (5, 6, books.clone()),
        (5, 7, books.clone()),
    ];
    if let Some(title) = third {
        nodes.extend([shelf(8, title), book(9, "x")]);
        arcs.extend([(0, 8, shelves), (8, 9, books)]);
    }
    tree(nodes, arcs, 0)
}

/// The literal of link 4 in a put source.
fn link_literal(tree: &WInstance) -> Option<&Value> {
    match tree.nodes.get(&4)?.value.as_ref()? {
        FieldPresence::Present(value) => Some(value),
        _ => None,
    }
}

#[test]
fn a_written_reference_takes_the_first_form_that_re_resolves() {
    let (resolution, fixture) = catalog();
    let source = AnyInstance::Tree(catalog_tree("y", "b", None));
    let (view, complement) = get(&resolution, &source);
    laws_hold(&resolution, &source, ResolveOptions::default());
    let see = occurrence_edge(&complement, 4);
    assert_eq!(view.edge(see).map(|edge| edge.tgt), Some(7));
    // Reconnected to shelf b's "x": the relative form "x" names shelf a's
    // own "x", so the absolute form is written.
    let mut reconnected = view.clone();
    reconnected
        .set_endpoints(see, 1, 6)
        .expect("reconnect the link to shelf b's x");
    let restored = put_tree(&resolution, &reconnected, &complement).expect("admitted");
    assert_eq!(link_literal(&restored), Some(&Value::Str("/b/x".into())));
    // A new link from shelf a to shelf b's "x" is written the same way.
    let mut added = view;
    let fresh = panproto_inst::EdgeId(40);
    added
        .insert_edge(fresh, panproto_inst::GEdge::new(1, 6, fixture.see))
        .expect("a new link");
    let restored = put_tree(&resolution, &added, &complement).expect("admitted");
    let holder = restored
        .nodes
        .values()
        .find(|node| node.anchor == "link" && node.id != 4)
        .expect("a new link holder");
    assert_eq!(
        holder.value,
        Some(FieldPresence::Present(Value::Str("/b/x".into())))
    );
    // Canonical emission writes the absolute form too.
    let (canonical, _) = resolution
        .put_canonical(&AnyInstance::Graph(reconnected), &ctx())
        .expect("a canonical source");
    let AnyInstance::Tree(canonical) = canonical else {
        panic!("a tree source");
    };
    let written: Vec<&Value> = canonical
        .nodes
        .values()
        .filter(|node| node.anchor == "link")
        .filter_map(|node| match &node.value {
            Some(FieldPresence::Present(value)) => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(written, vec![&Value::Str("/b/x".into())]);
}

#[test]
fn a_reference_whose_canonical_form_re_resolves_is_written_unchanged() {
    let (resolution, _) = catalog();
    let source = AnyInstance::Tree(catalog_tree("w", "b", None));
    let (mut view, complement) = get(&resolution, &source);
    let see = occurrence_edge(&complement, 4);
    assert_eq!(view.edge(see).map(|edge| edge.tgt), Some(3));
    view.set_endpoints(see, 1, 7)
        .expect("reconnect the link to shelf b's y");
    let rendered = resolution
        .provider()
        .render_path(
            &resolution.references()["see"],
            ViewRef::Node(4),
            ViewRef::Node(7),
            &AnyInstance::Graph(view.clone()),
            &ctx(),
        )
        .expect("the canonical rendering");
    let restored = put_tree(&resolution, &view, &complement).expect("admitted");
    assert_eq!(link_literal(&restored), Some(&rendered));
    assert_eq!(rendered, Value::Str("y".into()));
}

#[test]
fn a_reference_with_no_re_resolving_form_is_unrepresentable() {
    let (resolution, _) = catalog();
    let source = AnyInstance::Tree(catalog_tree("y", "b", Some("b")));
    let (mut view, complement) = get(&resolution, &source);
    let see = occurrence_edge(&complement, 4);
    view.set_endpoints(see, 1, 6)
        .expect("reconnect the link to the first shelf b's x");
    let expected = ResolveDomainError::UnrepresentableReference {
        carrier: ViewRef::Edge(see),
        reference: "see".into(),
    };
    let violations = violations_of(&resolution, &view, &complement, &ctx());
    assert!(violations.contains(&expected), "{violations:?}");
    assert!(matches!(
        resolution.put(&AnyInstance::Graph(view), &complement, &ctx()),
        Err(ResolveError::PutDomain(_))
    ));
}

#[test]
fn a_kept_reference_is_never_respelled() {
    let (resolution, _) = catalog();
    let source = AnyInstance::Tree(catalog_tree("y", "b", None));
    let (view, complement) = get(&resolution, &source);
    let see = occurrence_edge(&complement, 4);
    // Renaming shelf a's "w" to "y" captures the kept link: its relative
    // form now names the book on its own shelf. The absolute form "/b/y"
    // would re-resolve, but a kept occurrence is not respelled.
    let mut renamed = view;
    renamed
        .node_mut(3)
        .expect("book w")
        .extra_fields
        .insert("name".to_owned(), Value::Str("y".into()));
    assert_eq!(renamed.edge(see).map(|edge| edge.tgt), Some(7));
    let violations = violations_of(&resolution, &renamed, &complement, &ctx());
    assert!(
        violations.contains(&ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Edge(see),
            reference: "see".into(),
        }),
        "{violations:?}"
    );
    assert!(
        resolution
            .put(&AnyInstance::Graph(renamed), &complement, &ctx())
            .is_err()
    );
}

// ---------------------------------------------------------------------------
// The notes fixture: standoff spans over a note's own text field, or over
// text loaded under a key a note holds, materialized as span nodes.
// ---------------------------------------------------------------------------

/// One standoff declaration of the notes fixture.
#[derive(Clone)]
struct Standoff {
    id: &'static str,
    text: RefTextSource,
    start: &'static str,
    end: &'static str,
    unit: OffsetUnit,
    end_inclusive: bool,
    mode: ResolveMode,
    many: bool,
}

impl Standoff {
    fn local(id: &'static str, unit: OffsetUnit) -> Self {
        Self {
            id,
            text: RefTextSource::Local(vec![prop("note", "text", "text")]),
            start: "start",
            end: "end",
            unit,
            end_inclusive: false,
            mode: ResolveMode::AddEdge,
            many: false,
        }
    }

    fn external(id: &'static str, field: &'static str) -> Self {
        Self {
            text: RefTextSource::External(vec![prop("note", "key", field)]),
            ..Self::local(id, OffsetUnit::Byte)
        }
    }
}

struct Notes {
    schema: Schema,
    labels: Vec<Edge>,
}

fn notes_schema(declarations: &[Standoff]) -> Notes {
    let base = protocol("notes", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let mut builder = SchemaBuilder::new(&base)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("note", "object", None)
        .unwrap()
        .vertex("text", "string", None)
        .unwrap()
        .vertex("key", "string", None)
        .unwrap()
        .vertex("offset", "integer", None)
        .unwrap()
        .vertex("span", "object", None)
        .unwrap()
        .edge("doc", "note", "prop", Some("notes"))
        .unwrap()
        .edge("note", "note", "prop", Some("inner"))
        .unwrap()
        .edge("note", "text", "prop", Some("text"))
        .unwrap()
        .edge("note", "text", "prop", Some("body"))
        .unwrap()
        .edge("note", "key", "prop", Some("doc_a"))
        .unwrap()
        .edge("note", "key", "prop", Some("doc_b"))
        .unwrap()
        .entry("doc");
    let mut labels = Vec::new();
    let mut offsets = BTreeSet::new();
    for declaration in declarations {
        let label = edge("note", "span", "ref", Some(declaration.id));
        builder = builder
            .edge("note", "span", "ref", Some(declaration.id))
            .unwrap();
        for field in [declaration.start, declaration.end] {
            if offsets.insert(field) {
                builder = builder.edge("note", "offset", "prop", Some(field)).unwrap();
            }
        }
        let mut standoff = reference(
            declaration.id,
            "note",
            RefSource::Standoff {
                text: declaration.text.clone(),
                start: vec![prop("note", "offset", declaration.start)],
                end: vec![prop("note", "offset", declaration.end)],
                unit: declaration.unit.clone(),
                end_inclusive: declaration.end_inclusive,
            },
            "span",
            label.clone(),
        );
        standoff.mode = declaration.mode.clone();
        standoff.nullable = true;
        if declaration.many {
            standoff.cardinality = Cardinality::Many;
        }
        builder = builder.reference(standoff);
        labels.push(label);
    }
    Notes {
        schema: builder.build().unwrap(),
        labels,
    }
}

fn notes(declarations: &[Standoff], context: &[ResolutionContextKind]) -> Resolution {
    let fixture = notes_schema(declarations);
    let writers: Vec<(&Edge, &str)> = fixture
        .labels
        .iter()
        .zip(declarations)
        .map(|(label, declaration)| (label, declaration.id))
        .collect();
    compile_with(
        "notes",
        ResolutionKind::TreeToGraph,
        &fixture.schema,
        &writers,
        context,
        BTreeMap::new(),
    )
}

/// A note with the given fields.
fn note(id: u32, fields: &[(&str, Value)]) -> Node {
    let mut node = Node::new(id, "note");
    for (field, value) in fields {
        node.extra_fields.insert((*field).to_owned(), value.clone());
    }
    node
}

/// doc 0 holding the given notes, each a child of the doc.
fn notes_tree(notes: Vec<Node>) -> WInstance {
    let arcs = notes
        .iter()
        .map(|note| (0, note.id, prop("doc", "note", "notes")))
        .collect();
    let mut all = vec![Node::new(0, "doc")];
    all.extend(notes);
    tree(all, arcs, 0)
}

const fn int(value: i64) -> Value {
    Value::Int(value)
}

fn text(value: &str) -> Value {
    Value::Str(value.to_owned())
}

/// The span records of a complement, by span node.
fn spans(complement: &ResolutionComplement) -> Vec<panproto_io::resolution::ResolutionSpan> {
    complement.span_nodes.values().cloned().collect()
}

fn local_site(holder: u32, field: &str) -> ResolutionTextId {
    ResolutionTextId::Local(ResolutionValueSite::Field {
        holder: ViewRef::Node(holder),
        field: field.to_owned(),
    })
}

#[test]
fn equal_values_at_distinct_text_sites_do_not_alias() {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Byte)], &[]);
    let fields = [
        ("text", text("same words")),
        ("start", int(0)),
        ("end", int(4)),
    ];
    let source = AnyInstance::Tree(notes_tree(vec![note(1, &fields), note(2, &fields)]));
    let (view, complement) = get(&resolution, &source);
    assert_eq!(
        spans(&complement)
            .into_iter()
            .map(|span| span.text)
            .collect::<Vec<_>>(),
        vec![local_site(1, "text"), local_site(2, "text")]
    );
    assert_eq!(view.node_count(), 5);
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn the_same_text_site_and_interval_deduplicate_across_references() {
    let second = Standoff {
        start: "from",
        end: "to",
        ..Standoff::local("echo", OffsetUnit::Byte)
    };
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Byte), second], &[]);
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("text", text("same words")),
            ("start", int(5)),
            ("end", int(10)),
            ("from", int(5)),
            ("to", int(10)),
        ],
    )]));
    let (view, complement) = get(&resolution, &source);
    assert_eq!(complement.span_nodes.len(), 1);
    let span_node = *complement.span_nodes.keys().next().expect("one span");
    let into_span = edge_triples(&view)
        .into_iter()
        .filter(|(_, target, _)| *target == span_node)
        .count();
    assert_eq!(into_span, 2);
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn a_byte_and_a_scalar_declaration_share_a_span_exactly_when_their_byte_intervals_agree() {
    let bytes = Standoff::local("by_byte", OffsetUnit::Byte);
    let chars = Standoff {
        start: "from",
        end: "to",
        ..Standoff::local("by_char", OffsetUnit::Utf8Char)
    };
    let resolution = notes(&[bytes, chars], &[]);
    let words = text("héllo wörld");
    let agree = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("text", words.clone()),
            ("start", int(0)),
            ("end", int(3)),
            ("from", int(0)),
            ("to", int(2)),
        ],
    )]));
    let (_, complement) = get(&resolution, &agree);
    assert_eq!(
        spans(&complement).len(),
        1,
        "\"hé\" is bytes 0..3 and scalars 0..2"
    );
    assert_eq!(
        (spans(&complement)[0].start, spans(&complement)[0].end),
        (0, 3)
    );
    let disagree = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("text", words),
            ("start", int(0)),
            ("end", int(2)),
            ("from", int(0)),
            ("to", int(2)),
        ],
    )]));
    let (_, complement) = get(&resolution, &disagree);
    let intervals: Vec<(u64, u64)> = spans(&complement)
        .into_iter()
        .map(|span| (span.start, span.end))
        .collect();
    assert_eq!(intervals, vec![(0, 2), (0, 3)]);
    laws_hold(&resolution, &disagree, ResolveOptions::default());
}

#[test]
fn a_utf16_offset_inside_a_surrogate_pair_fails_get() {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Utf16)], &[]);
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", text("a😀b")), ("start", int(2)), ("end", int(4))],
    )]));
    match resolution.get(
        &source,
        &ResolveOptions::default(),
        &ResolveContext::empty(),
    ) {
        Err(ResolveError::OffsetUnitViolation {
            reference,
            at,
            unit,
            offset,
            reason,
        }) => {
            assert_eq!(reference, "mention");
            assert_eq!(*at, ViewRef::Node(1));
            assert_eq!(unit, OffsetUnit::Utf16);
            assert_eq!(offset, 2);
            assert_eq!(reason, OffsetUnitReason::NotOnBoundary);
        }
        other => panic!("an offset inside a surrogate pair is refused, got {other:?}"),
    }
    let whole = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", text("a😀b")), ("start", int(1)), ("end", int(3))],
    )]));
    let (_, complement) = get(&resolution, &whole);
    assert_eq!(
        (spans(&complement)[0].start, spans(&complement)[0].end),
        (1, 5)
    );
}

#[test]
fn offsets_are_refused_never_clamped() {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Utf8Char)], &[]);
    let ctx = ResolveContext::empty();
    let options = ResolveOptions::default();
    let past = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", text("héllo")), ("start", int(2)), ("end", int(9))],
    )]));
    assert!(matches!(
        resolution.get(&past, &options, &ctx),
        Err(ResolveError::OffsetOutOfRange {
            start: 2,
            end: 9,
            len: 5,
            ..
        })
    ));
    let reversed = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", text("héllo")), ("start", int(3)), ("end", int(1))],
    )]));
    assert!(matches!(
        resolution.get(&reversed, &options, &ctx),
        Err(ResolveError::OffsetOutOfRange {
            start: 3,
            end: 1,
            ..
        })
    ));
    let not_utf8 = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("text", Value::Bytes(vec![0xff, 0xfe])),
            ("start", int(0)),
            ("end", int(1)),
        ],
    )]));
    assert!(matches!(
        resolution.get(&not_utf8, &options, &ctx),
        Err(ResolveError::OffsetUnitViolation {
            reason: OffsetUnitReason::TextNotUtf8,
            ..
        })
    ));
    let negative = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", text("héllo")), ("start", int(-1)), ("end", int(1))],
    )]));
    assert!(matches!(
        resolution.get(&negative, &options, &ctx),
        Err(ResolveError::Fidelity(_))
    ));
    let inclusive = notes(
        &[Standoff {
            end_inclusive: true,
            ..Standoff::local("mention", OffsetUnit::Byte)
        }],
        &[],
    );
    let empty = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", text("hello")), ("start", int(2)), ("end", int(1))],
    )]));
    assert!(matches!(
        inclusive.get(&empty, &options, &ctx),
        Err(ResolveError::OffsetUnitViolation {
            reason: OffsetUnitReason::EmptyInclusive,
            offset: 1,
            ..
        })
    ));
    let (_, complement) = get(
        &inclusive,
        &AnyInstance::Tree(notes_tree(vec![note(
            1,
            &[("text", text("hello")), ("start", int(1)), ("end", int(3))],
        )])),
    );
    assert_eq!(
        (spans(&complement)[0].start, spans(&complement)[0].end),
        (1, 4)
    );
}

#[test]
fn standoff_text_path_requires_exactly_one_value_site() {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Byte)], &[]);
    let missing = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("start", int(0)), ("end", int(1))],
    )]));
    assert!(matches!(
        resolution.get(&missing, &ResolveOptions::default(), &ResolveContext::empty()),
        Err(ResolveError::ReferenceCardinalityMismatch { lengths, .. }) if lengths == vec![0]
    ));
    let mut doubled = notes_tree(vec![note(1, &[("start", int(0)), ("end", int(1))])]);
    for id in [2, 3] {
        doubled.nodes.insert(id, leaf(id, "text", text("twice")));
    }
    let mut arcs = doubled.arcs.clone();
    arcs.push((1, 2, prop("note", "text", "text")));
    arcs.push((1, 3, prop("note", "text", "text")));
    let doubled = WInstance::new(doubled.nodes, arcs, Vec::new(), 0, "doc".into());
    assert!(matches!(
        resolution.get(
            &AnyInstance::Tree(doubled),
            &ResolveOptions::default(),
            &ResolveContext::empty()
        ),
        Err(ResolveError::ReferenceCardinalityMismatch { lengths, .. }) if lengths == vec![2]
    ));
}

/// A store of external texts by key.
struct Store(BTreeMap<String, Vec<u8>>);

impl ExternalTextStore for Store {
    fn load(&self, key: &Value) -> Result<Vec<u8>, ResolveError> {
        let Value::Str(key) = key else {
            return Err(ResolveError::ProviderFailure {
                provider: provider_id("store"),
                code: "key".into(),
                at: None,
            });
        };
        self.0
            .get(key)
            .cloned()
            .ok_or_else(|| ResolveError::ProviderFailure {
                provider: provider_id("store"),
                code: "missing".into(),
                at: None,
            })
    }
}

/// Token boundaries at ASCII spaces, recording the text identities asked
/// for; `shift` moves every interior boundary one byte right.
struct Spaces {
    asked: std::sync::Mutex<Vec<ResolutionTextId>>,
    shift: bool,
}

impl Spaces {
    const fn new(shift: bool) -> Self {
        Self {
            asked: std::sync::Mutex::new(Vec::new()),
            shift,
        }
    }
}

impl TokenBoundaryProvider for Spaces {
    fn boundaries(&self, text: ResolutionTextId, utf8: &[u8]) -> Result<Vec<u64>, ResolveError> {
        self.asked.lock().expect("unpoisoned").push(text);
        let len = utf8.len() as u64;
        let mut boundaries = vec![0];
        for (index, byte) in utf8.iter().enumerate() {
            if *byte == b' ' {
                let at = index as u64 + 1 + u64::from(self.shift);
                if at < len && boundaries.last() != Some(&at) {
                    boundaries.push(at);
                }
            }
        }
        if boundaries.last() != Some(&len) {
            boundaries.push(len);
        }
        Ok(boundaries)
    }
}

#[test]
fn two_external_key_fields_at_one_tree_holder_have_distinct_span_keys() {
    let resolution = notes(
        &[
            Standoff::external("left", "doc_a"),
            Standoff::external("right", "doc_b"),
        ],
        &[ResolutionContextKind::ExternalText],
    );
    let store = Store(BTreeMap::from([("k".to_owned(), b"shared bytes".to_vec())]));
    let ctx = ResolveContext {
        external_text: Some(&store),
        token_boundaries: None,
    };
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("doc_a", text("k")),
            ("doc_b", text("k")),
            ("start", int(0)),
            ("end", int(6)),
        ],
    )]));
    let (_, complement) = resolution
        .get(&source, &ResolveOptions::default(), &ctx)
        .expect("get with the store");
    let texts: Vec<ResolutionTextId> = spans(&complement)
        .into_iter()
        .map(|span| span.text)
        .collect();
    assert_eq!(
        texts,
        vec![
            ResolutionTextId::ExternalKey(ResolutionValueSite::Field {
                holder: ViewRef::Node(1),
                field: "doc_a".to_owned(),
            }),
            ResolutionTextId::ExternalKey(ResolutionValueSite::Field {
                holder: ViewRef::Node(1),
                field: "doc_b".to_owned(),
            }),
        ]
    );
    check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx)
        .expect("the five laws hold");
    assert!(matches!(
        resolution.get(
            &source,
            &ResolveOptions::default(),
            &ResolveContext::empty()
        ),
        Err(ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::ExternalText,
            ..
        })
    ));
}

#[test]
fn changed_external_key_or_bytes_is_a_context_mismatch() {
    let resolution = notes(
        &[Standoff::external("left", "doc_a")],
        &[ResolutionContextKind::ExternalText],
    );
    let before = Store(BTreeMap::from([
        ("k".to_owned(), b"some bytes".to_vec()),
        ("j".to_owned(), b"some bytes".to_vec()),
    ]));
    let ctx = ResolveContext {
        external_text: Some(&before),
        token_boundaries: None,
    };
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("doc_a", text("k")), ("start", int(0)), ("end", int(4))],
    )]));
    let (view, complement) = resolution
        .get(&source, &ResolveOptions::default(), &ctx)
        .expect("get");
    let AnyInstance::Graph(view) = view else {
        panic!("a graph view");
    };
    let after = Store(BTreeMap::from([("k".to_owned(), b"some other".to_vec())]));
    let changed = ResolveContext {
        external_text: Some(&after),
        token_boundaries: None,
    };
    let violations = resolution
        .check_put_domain(&AnyInstance::Graph(view.clone()), &complement, &changed)
        .expect_err("the loaded bytes changed");
    assert!(violations.iter().any(|violation| matches!(
        violation,
        ResolveDomainError::ContextMismatch { at: Some(_) }
    )));
    assert!(matches!(
        resolution.put(&AnyInstance::Graph(view.clone()), &complement, &changed),
        Err(ResolveError::PutDomain(
            ResolveDomainError::ContextMismatch { .. }
        ))
    ));
    let mut rekeyed = view;
    if let Some(node) = rekeyed.node_mut(1) {
        node.extra_fields.insert("doc_a".to_owned(), text("j"));
    }
    let violations = resolution
        .check_put_domain(&AnyInstance::Graph(rekeyed), &complement, &ctx)
        .expect_err("the key changed");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, ResolveDomainError::ContextMismatch { .. })),
        "{violations:?}"
    );
}

#[test]
fn put_rejects_changed_external_context() {
    let resolution = notes(
        &[Standoff::external("left", "doc_a")],
        &[ResolutionContextKind::ExternalText],
    );
    let store = Store(BTreeMap::from([("k".to_owned(), b"some bytes".to_vec())]));
    let ctx = ResolveContext {
        external_text: Some(&store),
        token_boundaries: None,
    };
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("doc_a", text("k")), ("start", int(0)), ("end", int(4))],
    )]));
    let (view, complement) = resolution
        .get(&source, &ResolveOptions::default(), &ctx)
        .expect("get");
    assert!(resolution.put(&view, &complement, &ctx).is_ok());
    let gone = Store(BTreeMap::new());
    let without = ResolveContext {
        external_text: Some(&gone),
        token_boundaries: None,
    };
    assert!(matches!(
        resolution.put(&view, &complement, &without),
        Err(ResolveError::PutDomain(
            ResolveDomainError::ContextMismatch { .. }
        ))
    ));
}

#[test]
fn token_boundary_provider_receives_the_exact_resolution_text_id() {
    let tokens = Standoff {
        text: RefTextSource::Local(vec![prop("note", "text", "body")]),
        ..Standoff::local("by_token", OffsetUnit::Token)
    };
    let resolution = notes(&[tokens], &[ResolutionContextKind::TokenBoundaries]);
    let spaces = Spaces::new(false);
    let ctx = ResolveContext {
        external_text: None,
        token_boundaries: Some(&spaces),
    };
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("text", text("one two")),
            ("body", text("alpha beta gamma")),
            ("start", int(1)),
            ("end", int(2)),
        ],
    )]));
    let (_, complement) = resolution
        .get(&source, &ResolveOptions::default(), &ctx)
        .expect("get");
    assert_eq!(
        spaces.asked.lock().expect("unpoisoned").clone(),
        vec![local_site(1, "body")]
    );
    assert_eq!(
        (spans(&complement)[0].start, spans(&complement)[0].end),
        (6, 11),
        "token 1 of \"alpha beta gamma\" is \"beta \""
    );
    check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx)
        .expect("the five laws hold");
    assert!(
        spaces
            .asked
            .lock()
            .expect("unpoisoned")
            .iter()
            .all(|asked| *asked == local_site(1, "body"))
    );
}

#[test]
fn put_refuses_a_token_context_mismatch() {
    let tokens = Standoff {
        text: RefTextSource::External(vec![prop("note", "key", "doc_a")]),
        ..Standoff::local("by_token", OffsetUnit::Token)
    };
    let resolution = notes(
        &[tokens],
        &[
            ResolutionContextKind::ExternalText,
            ResolutionContextKind::TokenBoundaries,
        ],
    );
    let store = Store(BTreeMap::from([(
        "k".to_owned(),
        b"alpha beta gamma".to_vec(),
    )]));
    let spaces = Spaces::new(false);
    let ctx = ResolveContext {
        external_text: Some(&store),
        token_boundaries: Some(&spaces),
    };
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("doc_a", text("k")), ("start", int(0)), ("end", int(1))],
    )]));
    let (view, complement) = resolution
        .get(&source, &ResolveOptions::default(), &ctx)
        .expect("get");
    let shifted = Spaces::new(true);
    let changed = ResolveContext {
        external_text: Some(&store),
        token_boundaries: Some(&shifted),
    };
    let violations = resolution
        .check_put_domain(&view, &complement, &changed)
        .expect_err("the token boundaries changed");
    assert!(
        violations
            .iter()
            .any(|violation| matches!(violation, ResolveDomainError::ContextMismatch { .. })),
        "{violations:?}"
    );
}

#[test]
fn a_missing_context_requirement_is_refused_at_compilation() {
    let fixture = notes_schema(&[Standoff::external("left", "doc_a")]);
    let protocol = protocol("notes", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let provider = Provider {
        id: provider_id("notes"),
        protocol: protocol.clone(),
        kind: ResolutionKind::TreeToGraph,
        spec: spec_for(
            "notes",
            ResolutionKind::TreeToGraph,
            &fixture.schema,
            &[(&fixture.labels[0], "left")],
            &[],
        ),
        paths: BTreeMap::new(),
        context: None,
    };
    assert!(matches!(
        Resolution::for_protocol(&protocol, &fixture.schema, Arc::new(provider)),
        Err(ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::ExternalText,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// The nested notes fixture: an outer note spans the text of its inner
// note's body by bytes; the inner note, a redirecting holder, spans the
// same body by scalars.
// ---------------------------------------------------------------------------

fn nested_declarations(inclusive: bool) -> [Standoff; 2] {
    let outer = Standoff {
        text: RefTextSource::Local(vec![
            prop("note", "note", "inner"),
            prop("note", "text", "body"),
        ]),
        ..Standoff::local("outer", OffsetUnit::Byte)
    };
    let inner = Standoff {
        text: RefTextSource::Local(vec![prop("note", "text", "body")]),
        start: "from",
        end: "to",
        end_inclusive: inclusive,
        mode: ResolveMode::Redirect,
        ..Standoff::local("inner_ref", OffsetUnit::Utf8Char)
    };
    [outer, inner]
}

/// doc 0; outer note 1 (start, end) holding inner note 2 (from, to) whose
/// body 3 is `body`; a second outer note 4 (start 0, end 1) holding note 5
/// whose body 6 is "zz".
fn nested_tree(outer: (i64, i64), inner: (i64, i64), body: &str) -> WInstance {
    let notes = prop("doc", "note", "notes");
    let inner_arc = prop("note", "note", "inner");
    let body_arc = prop("note", "text", "body");
    tree(
        vec![
            Node::new(0, "doc"),
            note(1, &[("start", int(outer.0)), ("end", int(outer.1))]),
            note(2, &[("from", int(inner.0)), ("to", int(inner.1))]),
            leaf(3, "text", text(body)),
            note(4, &[("start", int(0)), ("end", int(1))]),
            note(5, &[]),
            leaf(6, "text", text("zz")),
        ],
        vec![
            (0, 1, notes.clone()),
            (1, 2, inner_arc.clone()),
            (2, 3, body_arc.clone()),
            (0, 4, notes),
            (4, 5, inner_arc),
            (5, 6, body_arc),
        ],
        0,
    )
}

/// The span node of `holder`'s occurrence of `reference`.
fn span_of(
    view: &GInstance,
    complement: &ResolutionComplement,
    holder: u32,
    reference: &str,
) -> (panproto_inst::EdgeId, u32) {
    complement
        .occurrences
        .iter()
        .find_map(|(carrier, records)| match carrier {
            ViewRef::Edge(id)
                if records[0].holder == ViewRef::Node(holder)
                    && records[0].reference == reference =>
            {
                view.edge(*id).map(|edge| (*id, edge.tgt))
            }
            _ => None,
        })
        .expect("the occurrence has a span edge")
}

#[test]
fn standoff_reconnect_writes_text_start_and_end() {
    let resolution = notes(&nested_declarations(false), &[]);
    let source = AnyInstance::Tree(nested_tree((0, 6), (7, 11), "héllo wörld"));
    let (mut view, complement) = get(&resolution, &source);
    let report = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx())
        .expect("mixed standoff and redirect occurrences satisfy the laws");
    for mutation in [Mutation::RemoveSynthesized, Mutation::EditSynthesized] {
        assert!(
            report
                .skipped
                .contains(&(mutation, SkipReason::NoCandidate)),
            "{mutation:?} must not predict standoff-span or redirect closure"
        );
    }
    let (outer_edge, outer_span) = span_of(&view, &complement, 1, "outer");
    let (inner_edge, inner_span) = span_of(&view, &complement, 2, "inner_ref");
    assert_ne!(outer_span, inner_span);
    assert_eq!(view.edge(inner_edge).map(|edge| edge.src), Some(1));
    let _ = outer_edge;
    view.set_endpoints(inner_edge, 1, outer_span)
        .expect("reconnect the inner span to the outer one");
    view.remove_node(inner_span)
        .expect("the old span is unreferenced");
    let restored = put_tree(&resolution, &view, &complement).expect("a same-text reconnect");
    let inner = &restored.nodes[&2];
    assert_eq!(inner.extra_fields["from"], int(0));
    assert_eq!(
        inner.extra_fields["to"],
        int(5),
        "bytes 0..6 are the scalars 0..5"
    );
    assert_eq!(
        restored.nodes[&3].value,
        Some(FieldPresence::Present(text("héllo wörld"))),
        "the text site is written with its own value"
    );
}

#[test]
fn unrepresentable_cross_text_reconnect_is_in_the_put_domain_vector() {
    let resolution = notes(&nested_declarations(false), &[]);
    let source = AnyInstance::Tree(nested_tree((0, 6), (7, 11), "héllo wörld"));
    let (mut view, complement) = get(&resolution, &source);
    let (_, other_span) = span_of(&view, &complement, 4, "outer");
    let (inner_edge, inner_span) = span_of(&view, &complement, 2, "inner_ref");
    view.set_endpoints(inner_edge, 1, other_span)
        .expect("reconnect to a span of another text");
    view.remove_node(inner_span).expect("unreferenced");
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("the inner note's body cannot name the other text");
    assert!(
        violations.contains(&ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Edge(inner_edge),
            reference: "inner_ref".into(),
        }),
        "{violations:?}"
    );
}

#[test]
fn a_byte_span_ending_mid_scalar_is_outside_a_scalar_declaration() {
    let resolution = notes(&nested_declarations(false), &[]);
    let source = AnyInstance::Tree(nested_tree((0, 2), (7, 11), "héllo wörld"));
    let (mut view, complement) = get(&resolution, &source);
    let (_, outer_span) = span_of(&view, &complement, 1, "outer");
    let (inner_edge, inner_span) = span_of(&view, &complement, 2, "inner_ref");
    view.set_endpoints(inner_edge, 1, outer_span)
        .expect("reconnect to bytes 0..2, which end inside \"é\"");
    view.remove_node(inner_span).expect("unreferenced");
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("a scalar declaration cannot write a mid-scalar end");
    assert!(
        violations.contains(&ResolveDomainError::OffsetUnitViolation {
            carrier: ViewRef::Edge(inner_edge),
            reference: "inner_ref".into(),
            reason: OffsetUnitReason::NotOnBoundary,
        }),
        "{violations:?}"
    );
}

#[test]
fn an_empty_span_is_outside_an_inclusive_declaration() {
    let resolution = notes(&nested_declarations(true), &[]);
    let source = AnyInstance::Tree(nested_tree((3, 3), (7, 10), "héllo wörld"));
    let (mut view, complement) = get(&resolution, &source);
    let (_, outer_span) = span_of(&view, &complement, 1, "outer");
    let (inner_edge, inner_span) = span_of(&view, &complement, 2, "inner_ref");
    view.set_endpoints(inner_edge, 1, outer_span)
        .expect("reconnect to the empty span 3..3");
    view.remove_node(inner_span).expect("unreferenced");
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("an inclusive declaration cannot write an empty span");
    assert!(
        violations.contains(&ResolveDomainError::OffsetUnitViolation {
            carrier: ViewRef::Edge(inner_edge),
            reference: "inner_ref".into(),
            reason: OffsetUnitReason::EmptyInclusive,
        }),
        "{violations:?}"
    );
}

#[test]
fn a_non_leaf_string_range_is_not_standoff() {
    let resolution = notes(&nested_declarations(false), &[]);
    let mut source = nested_tree((0, 2), (7, 11), "héllo wörld");
    let mut arcs = source.arcs.clone();
    source.nodes.insert(7, leaf(7, "text", text("child")));
    arcs.push((3, 7, prop("note", "text", "body")));
    let source = WInstance::new(source.nodes, arcs, Vec::new(), 0, "doc".into());
    assert!(matches!(
        resolution.get(
            &AnyInstance::Tree(source),
            &ResolveOptions::default(),
            &ResolveContext::empty()
        ),
        Err(ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::Path(PathSyntax::XPath),
            ..
        })
    ));
}

/// A provider whose format counts offsets over a normalized text.
struct Normalizing(Protocol);

impl ResolutionProvider for Normalizing {
    fn id(&self) -> ResolutionProviderId {
        provider_id("normalizing")
    }

    fn kind(&self) -> ResolutionKind {
        ResolutionKind::TreeToGraph
    }

    fn protocol(&self) -> &Protocol {
        &self.0
    }

    fn spec(
        &self,
        _protocol: &Protocol,
        _syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        Err(ResolveError::UnsupportedFeature {
            provider: Some(self.id()),
            feature: ResolutionFeature::NormalizedOffsets,
        })
    }
}

#[test]
fn a_normalizing_offset_convention_is_unsupported() {
    let fixture = notes_schema(&[Standoff::local("mention", OffsetUnit::Utf8Char)]);
    let protocol = protocol(
        "normalizing",
        InstanceShapeKind::Tree,
        InstanceShapeKind::Graph,
    );
    let provider = Normalizing(protocol.clone());
    let error = Resolution::for_protocol(&protocol, &fixture.schema, Arc::new(provider))
        .expect_err("a normalizing convention is refused");
    assert!(matches!(
        error,
        ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::NormalizedOffsets,
            ..
        }
    ));
    assert_eq!(
        serde_json::to_string(&ResolutionFeature::NormalizedOffsets).expect("serializes"),
        "\"normalized_offsets\""
    );
}

// ---------------------------------------------------------------------------
// Local text edits under surviving spans, one fixture per unit and case.
// ---------------------------------------------------------------------------

/// The put-domain verdict of editing note 1's text to `edited`, the note
/// spanning `start..end` of `original` in `unit`.
fn local_text_edit(
    unit: OffsetUnit,
    original: Value,
    (start, end): (i64, i64),
    edited: Value,
) -> Result<(), Vec<ResolveDomainError>> {
    let token = matches!(unit, OffsetUnit::Token);
    let context: &[ResolutionContextKind] = if token {
        &[ResolutionContextKind::TokenBoundaries]
    } else {
        &[]
    };
    let resolution = notes(&[Standoff::local("mention", unit)], context);
    let spaces = Spaces::new(false);
    let ctx = ResolveContext {
        external_text: None,
        token_boundaries: token.then_some(&spaces as &dyn TokenBoundaryProvider),
    };
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", original), ("start", int(start)), ("end", int(end))],
    )]));
    let (view, complement) = resolution
        .get(&source, &ResolveOptions::default(), &ctx)
        .expect("the original spans are valid");
    let AnyInstance::Graph(mut view) = view else {
        panic!("a graph view");
    };
    if let Some(node) = view.node_mut(1) {
        node.extra_fields.insert("text".to_owned(), edited);
    }
    let view = AnyInstance::Graph(view);
    let verdict = resolution.check_put_domain(&view, &complement, &ctx);
    if verdict.is_ok() {
        let restored = resolution
            .put(&view, &complement, &ctx)
            .expect("an admitted edit puts");
        let (again, _) = resolution
            .get(&restored, &ResolveOptions::default(), &ctx)
            .expect("the put source resolves");
        assert!(matches!(again, AnyInstance::Graph(_)));
    }
    verdict
}

fn refused_with(verdict: Result<(), Vec<ResolveDomainError>>, expected: &ResolveDomainError) {
    let violations = verdict.expect_err("the edit is refused");
    assert!(violations.contains(expected), "{violations:?}");
}

fn span_violation(reason: Option<OffsetUnitReason>) -> ResolveDomainError {
    let carrier = ViewRef::Edge(panproto_inst::EdgeId(1));
    let reference = Name::from("mention");
    match reason {
        None => ResolveDomainError::OffsetOutOfRange { carrier, reference },
        Some(reason) => ResolveDomainError::OffsetUnitViolation {
            carrier,
            reference,
            reason,
        },
    }
}

#[test]
fn local_text_edit_byte_admitted() {
    assert_eq!(
        local_text_edit(
            OffsetUnit::Byte,
            text("hello world"),
            (0, 5),
            text("hello there")
        ),
        Ok(())
    );
}

#[test]
fn local_text_edit_byte_out_of_range() {
    refused_with(
        local_text_edit(OffsetUnit::Byte, text("hello world"), (0, 5), text("hi")),
        &span_violation(None),
    );
}

#[test]
fn local_text_edit_byte_off_boundary() {
    // Every byte offset is a byte boundary: a span that now ends inside a
    // scalar of the edited text is still on its unit's boundaries.
    assert_eq!(
        local_text_edit(
            OffsetUnit::Byte,
            text("hello world"),
            (0, 2),
            text("hé wörld")
        ),
        Ok(())
    );
}

#[test]
fn local_text_edit_utf8char_admitted() {
    assert_eq!(
        local_text_edit(
            OffsetUnit::Utf8Char,
            text("héllo world"),
            (0, 5),
            text("héllo wörld")
        ),
        Ok(())
    );
}

#[test]
fn local_text_edit_utf8char_out_of_range() {
    refused_with(
        local_text_edit(
            OffsetUnit::Utf8Char,
            text("héllo world"),
            (0, 5),
            text("hé"),
        ),
        &span_violation(None),
    );
}

#[test]
fn local_text_edit_utf8char_off_boundary() {
    refused_with(
        local_text_edit(
            OffsetUnit::Utf8Char,
            text("héllo world"),
            (0, 5),
            Value::Bytes(vec![0xff; 12]),
        ),
        &span_violation(Some(OffsetUnitReason::TextNotUtf8)),
    );
}

#[test]
fn local_text_edit_utf16_admitted() {
    assert_eq!(
        local_text_edit(OffsetUnit::Utf16, text("ab😀"), (0, 1), text("a😀")),
        Ok(())
    );
}

#[test]
fn local_text_edit_utf16_out_of_range() {
    refused_with(
        local_text_edit(OffsetUnit::Utf16, text("ab😀"), (0, 4), text("ab")),
        &span_violation(None),
    );
}

#[test]
fn local_text_edit_utf16_off_boundary() {
    refused_with(
        local_text_edit(OffsetUnit::Utf16, text("ab😀"), (0, 1), text("😀b")),
        &span_violation(Some(OffsetUnitReason::NotOnBoundary)),
    );
}

#[test]
fn local_text_edit_token_admitted() {
    assert_eq!(
        local_text_edit(
            OffsetUnit::Token,
            text("hello world"),
            (0, 1),
            text("hello there")
        ),
        Ok(())
    );
}

#[test]
fn local_text_edit_token_out_of_range() {
    refused_with(
        local_text_edit(
            OffsetUnit::Token,
            text("hello world"),
            (0, 2),
            text("hello"),
        ),
        &span_violation(None),
    );
}

#[test]
fn local_text_edit_token_off_boundary() {
    // The surviving span is the byte interval [0, 5) of "hello world"; the
    // tokens of "hi world" start at bytes 0 and 3, so the span's end is not
    // on a token boundary of the edited text.
    refused_with(
        local_text_edit(
            OffsetUnit::Token,
            text("hello world"),
            (0, 1),
            text("hi world"),
        ),
        &span_violation(Some(OffsetUnitReason::NotOnBoundary)),
    );
}

// ---------------------------------------------------------------------------
// The orders fixture: a table instance whose order rows name customers by
// a key held in a cell, beside a foreign key the table declares.
// ---------------------------------------------------------------------------

struct Orders {
    schema: Schema,
    placed_by: Edge,
    region: Edge,
    left: Edge,
    right: Edge,
}

fn orders_schema(many: bool) -> Orders {
    let base = protocol("orders", InstanceShapeKind::Table, InstanceShapeKind::Graph);
    let placed_by = edge("order", "customer", "ref", Some("placed_by"));
    let region = edge("order", "customer", "fk", Some("region"));
    let left = edge("order", "span", "ref", Some("left_span"));
    let right = edge("order", "span", "ref", Some("right_span"));
    let mut placed = reference(
        "placed",
        "order",
        RefSource::KeyValue {
            parts: vec![vec![prop("order", "label", "customer")]],
        },
        "customer",
        placed_by.clone(),
    );
    if many {
        placed.cardinality = Cardinality::Many;
    }
    let standoff = |id: &str, field: &str, label: &Edge| {
        let mut standoff = reference(
            id,
            "order",
            RefSource::Standoff {
                text: RefTextSource::Local(vec![prop("order", "label", field)]),
                start: vec![prop("order", "num", "start")],
                end: vec![prop("order", "num", "end")],
                unit: OffsetUnit::Byte,
                end_inclusive: false,
            },
            "span",
            label.clone(),
        );
        standoff.nullable = true;
        standoff
    };
    let schema = SchemaBuilder::new(&base)
        .vertex("customer", "table", None)
        .unwrap()
        .vertex("order", "table", None)
        .unwrap()
        .vertex("span", "table", None)
        .unwrap()
        .vertex("label", "string", None)
        .unwrap()
        .vertex("num", "integer", None)
        .unwrap()
        .edge("customer", "label", "prop", Some("cid"))
        .unwrap()
        .edge("order", "label", "prop", Some("customer"))
        .unwrap()
        .edge("order", "label", "prop", Some("left"))
        .unwrap()
        .edge("order", "label", "prop", Some("right"))
        .unwrap()
        .edge("order", "num", "prop", Some("start"))
        .unwrap()
        .edge("order", "num", "prop", Some("end"))
        .unwrap()
        .edge("order", "customer", "ref", Some("placed_by"))
        .unwrap()
        .edge("order", "customer", "fk", Some("region"))
        .unwrap()
        .edge("order", "span", "ref", Some("left_span"))
        .unwrap()
        .edge("order", "span", "ref", Some("right_span"))
        .unwrap()
        .key(
            "customer",
            key(
                "cid",
                vec![KeyPart::Path(vec![prop("customer", "label", "cid")])],
                KeyRole::Primary,
                KeyScope::Document,
            ),
        )
        .reference(placed)
        .reference(standoff("left_text", "left", &left))
        .reference(standoff("right_text", "right", &right))
        .build()
        .unwrap();
    Orders {
        schema,
        placed_by,
        region,
        left,
        right,
    }
}

fn row(cells: &[(&str, Value)]) -> std::collections::HashMap<String, Value> {
    cells
        .iter()
        .map(|(cell, value)| ((*cell).to_owned(), value.clone()))
        .collect()
}

/// customers 0 ("a") and 1 ("b"); orders 2 and 3 naming `first` and
/// `second`; the foreign-key pair (2, 1) of `region`.
fn orders_table(fixture: &Orders, first: Value, second: Value) -> FInstance {
    FInstance::new()
        .with_rows(
            "customer",
            [
                (RowId(0), row(&[("cid", text("a"))])),
                (RowId(1), row(&[("cid", text("b"))])),
            ],
        )
        .expect("customers")
        .with_rows(
            "order",
            [
                (RowId(2), row(&[("customer", first)])),
                (RowId(3), row(&[("customer", second)])),
            ],
        )
        .expect("orders")
        .with_foreign_key(fixture.region.clone(), [(RowId(2), RowId(1))])
        .expect("a region pair")
}

fn orders(many: bool) -> (Resolution, Orders) {
    let fixture = orders_schema(many);
    let resolution = compile(
        "orders",
        ResolutionKind::TableToGraph,
        &fixture.schema,
        &[
            (&fixture.placed_by, "placed"),
            (&fixture.left, "left_text"),
            (&fixture.right, "right_text"),
        ],
    );
    (resolution, fixture)
}

#[test]
fn table_to_graph_adds_one_edge_per_occurrence() {
    let (resolution, fixture) = orders(false);
    let table = orders_table(&fixture, text("a"), text("b"));
    let source = AnyInstance::Table(table.clone());
    let (view, complement) = get(&resolution, &source);
    assert_eq!(
        edge_triples(&view),
        vec![
            (2, 0, "placed_by".to_owned()),
            (2, 1, "region".to_owned()),
            (3, 1, "placed_by".to_owned())
        ]
    );
    let mut structural = view;
    for carrier in complement.occurrences.keys() {
        if let ViewRef::Edge(id) = carrier {
            structural.remove_edge(*id).expect("an occurrence edge");
        }
    }
    let (encoding, _) = GInstance::from_finstance(&table).expect("the table's graph form");
    assert!(GInstance::identical(&structural, &encoding));
    assert!(
        complement
            .occurrences
            .values()
            .all(|records| matches!(records[0].holder, ViewRef::Row(_)))
    );
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn duplicate_references_preserve_multiplicity() {
    let (resolution, fixture) = orders(true);
    let source = AnyInstance::Table(orders_table(
        &fixture,
        Value::List(vec![text("a"), text("a")]),
        Value::List(vec![text("b")]),
    ));
    let (view, complement) = get(&resolution, &source);
    let placed: Vec<_> = edge_triples(&view)
        .into_iter()
        .filter(|(_, _, label)| label == "placed_by")
        .collect();
    assert_eq!(
        placed,
        vec![
            (2, 0, "placed_by".to_owned()),
            (2, 0, "placed_by".to_owned()),
            (3, 1, "placed_by".to_owned())
        ]
    );
    let indexes: Vec<u32> = complement
        .occurrences
        .values()
        .flatten()
        .filter(|record| record.holder == ViewRef::Row(RowId(2)))
        .map(|record| record.index)
        .collect();
    assert_eq!(indexes, vec![0, 1]);
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn two_table_text_fields_at_one_row_have_distinct_span_keys() {
    let (resolution, fixture) = orders(false);
    let mut table = orders_table(&fixture, text("a"), text("b"));
    if let Some(order) = table.row_mut(RowId(2)) {
        order.insert("left".to_owned(), text("same"));
        order.insert("right".to_owned(), text("same"));
        order.insert("start".to_owned(), int(0));
        order.insert("end".to_owned(), int(2));
    }
    let source = AnyInstance::Table(table);
    let (_, complement) = get(&resolution, &source);
    let texts: Vec<ResolutionTextId> = spans(&complement)
        .into_iter()
        .map(|span| span.text)
        .collect();
    assert_eq!(
        texts,
        vec![
            ResolutionTextId::Local(ResolutionValueSite::Field {
                holder: ViewRef::Row(RowId(2)),
                field: "left".to_owned(),
            }),
            ResolutionTextId::Local(ResolutionValueSite::Field {
                holder: ViewRef::Row(RowId(2)),
                field: "right".to_owned(),
            }),
        ]
    );
    laws_hold(&resolution, &source, ResolveOptions::default());
}

// ---------------------------------------------------------------------------
// Put-domain vectors.
// ---------------------------------------------------------------------------

fn sorted_by_key(violations: &[ResolveDomainError]) -> bool {
    violations
        .windows(2)
        .all(|pair| pair[0].sort_key() <= pair[1].sort_key())
}

#[test]
fn put_domain_simultaneous_unplaceable_nodes() {
    let (resolution, _) = posts_resolution(|_| {});
    let (mut view, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    view.insert_node(Node::new(101, "post"))
        .expect("a stray post");
    view.insert_node(Node::new(100, "user"))
        .expect("a stray user");
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("two nodes without a parent");
    assert_eq!(
        violations,
        vec![
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(100)
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(101)
            },
        ]
    );
}

#[test]
fn check_put_domain_reports_every_violation_in_order() {
    let (resolution, posts) = posts_resolution(|_| {});
    let (mut view, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    view.insert_node(Node::new(100, "user"))
        .expect("a stray user");
    let occurrence = occurrence_edge(&complement, 5);
    view.set_edge_position(occurrence, Some(3))
        .expect("edit a synthesized edge");
    let fresh = panproto_inst::EdgeId(50);
    view.insert_edge(
        fresh,
        panproto_inst::GEdge::new(0, 1, edge("doc", "user", "prop", Some("users"))),
    )
    .expect("a second parent for user 1");
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(view.clone()),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("several violations");
    assert!(sorted_by_key(&violations), "{violations:?}");
    assert!(violations.contains(&ResolveDomainError::NoPlacement {
        carrier: ViewRef::Node(100)
    }));
    assert!(violations.contains(&ResolveDomainError::SynthesizedEdited {
        carrier: ViewRef::Edge(occurrence)
    }));
    match resolution.put(
        &AnyInstance::Graph(view),
        &complement,
        &ResolveContext::empty(),
    ) {
        Err(ResolveError::PutDomain(first)) => assert_eq!(first, violations[0]),
        other => panic!("put returns the first violation, got {other:?}"),
    }
    let _ = posts;
}

/// The put-domain violations of `view`, empty when it is admitted.
fn violations_of(
    resolution: &Resolution,
    view: &GInstance,
    complement: &ResolutionComplement,
    ctx: &ResolveContext<'_>,
) -> Vec<ResolveDomainError> {
    resolution
        .check_put_domain(&AnyInstance::Graph(view.clone()), complement, ctx)
        .err()
        .unwrap_or_default()
}

/// `a` and `b` together, sorted and without repeats, as the put domain
/// lists them.
fn union_of(a: &[ResolveDomainError], b: &[ResolveDomainError]) -> Vec<ResolveDomainError> {
    let mut union: Vec<ResolveDomainError> = a.iter().chain(b).cloned().collect();
    union.sort_by(|x, y| x.sort_key().cmp(&y.sort_key()));
    union.dedup();
    union
}

#[test]
fn check_put_domain_reports_the_violations_of_independent_edits_together() {
    let (resolution, _) = posts_resolution(|_| {});
    let (view, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    let mut rekeyed = view.clone();
    rekeyed.node_mut(4).expect("user 3's id").value =
        Some(FieldPresence::Present(Value::Str("u1".into())));
    let mut stray = view;
    stray
        .insert_node(Node::new(100, "user"))
        .expect("a stray user");
    let mut both = rekeyed.clone();
    both.insert_node(Node::new(100, "user"))
        .expect("a stray user");
    let alone = violations_of(&resolution, &rekeyed, &complement, &ctx());
    let unplaced = violations_of(&resolution, &stray, &complement, &ctx());
    assert_eq!(
        alone,
        vec![
            ResolveDomainError::UnrepresentableReference {
                carrier: ViewRef::Node(5),
                reference: "post_author".into(),
            },
            ResolveDomainError::UnrepresentableReference {
                carrier: ViewRef::Node(7),
                reference: "post_author".into(),
            },
            ResolveDomainError::UnrepresentableReference {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(8)),
                reference: "post_author".into(),
            },
            ResolveDomainError::UnrepresentableReference {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(9)),
                reference: "post_author".into(),
            },
        ]
    );
    assert_eq!(
        unplaced,
        vec![ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(100)
        }]
    );
    assert_eq!(
        violations_of(&resolution, &both, &complement, &ctx()),
        union_of(&alone, &unplaced)
    );
}

#[test]
fn check_put_domain_reports_an_unreadable_occurrence_beside_other_violations() {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Byte)], &[]);
    let source = AnyInstance::Tree(notes_tree(vec![
        note(
            1,
            &[
                ("text", text("hello world")),
                ("start", int(0)),
                ("end", int(5)),
            ],
        ),
        note(
            2,
            &[
                ("text", text("good night")),
                ("start", int(0)),
                ("end", int(4)),
            ],
        ),
    ]));
    let (view, complement) = get(&resolution, &source);
    let mut short = view.clone();
    short
        .node_mut(1)
        .expect("note 1")
        .extra_fields
        .insert("text".to_owned(), text("hi"));
    let mut moved = view.clone();
    moved
        .node_mut(2)
        .expect("note 2")
        .extra_fields
        .insert("end".to_owned(), int(3));
    let mut stray = view;
    stray
        .insert_node(Node::new(100, "note"))
        .expect("a stray note");
    let out_of_range = violations_of(&resolution, &short, &complement, &ctx());
    let respanned = violations_of(&resolution, &moved, &complement, &ctx());
    let unplaced = violations_of(&resolution, &stray, &complement, &ctx());
    for found in [&out_of_range, &respanned, &unplaced] {
        assert!(!found.is_empty());
    }
    assert!(
        out_of_range
            .iter()
            .any(|violation| matches!(violation, ResolveDomainError::OffsetOutOfRange { .. })),
        "{out_of_range:?}"
    );
    let mut all = short;
    all.node_mut(2)
        .expect("note 2")
        .extra_fields
        .insert("end".to_owned(), int(3));
    all.insert_node(Node::new(100, "note"))
        .expect("a stray note");
    assert_eq!(
        violations_of(&resolution, &all, &complement, &ctx()),
        union_of(&union_of(&out_of_range, &respanned), &unplaced)
    );
}

#[test]
fn put_domain_missing_text_site() {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Byte)], &[]);
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("text", text("hello world")),
            ("start", int(0)),
            ("end", int(5)),
        ],
    )]));
    let (view, complement) = get(&resolution, &source);
    let mut edited = view;
    if let Some(node) = edited.node_mut(1) {
        node.extra_fields.remove("text");
    }
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(edited),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("the span's text site is gone");
    assert_eq!(
        violations,
        vec![ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Edge(panproto_inst::EdgeId(1)),
            reference: "mention".into(),
        }]
    );
}

#[test]
fn put_domain_changed_text_site() {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Byte)], &[]);
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[
            ("text", text("hello world")),
            ("start", int(0)),
            ("end", int(5)),
        ],
    )]));
    let (view, complement) = get(&resolution, &source);
    let mut edited = view;
    if let Some(node) = edited.node_mut(1) {
        node.extra_fields.remove("text");
    }
    edited
        .insert_node(leaf(9, "text", text("hello world")))
        .expect("the text as a child");
    edited
        .insert_edge(
            panproto_inst::EdgeId(9),
            panproto_inst::GEdge::new(1, 9, prop("note", "text", "text")),
        )
        .expect("its arc");
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(edited),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("the text now sits at another site");
    let span = *complement.span_nodes.keys().next().expect("one span");
    assert!(
        violations.contains(&ResolveDomainError::SynthesizedEdited {
            carrier: ViewRef::Node(span)
        }),
        "{violations:?}"
    );
    assert!(sorted_by_key(&violations));
}

// ---------------------------------------------------------------------------
// Canonical placement: r, a and v, where a reaches v structurally and r
// reaches v both structurally and by a reference written on the same label.
// ---------------------------------------------------------------------------

fn rav_schema(mode: ResolveMode) -> (Schema, Edge) {
    let base = protocol("rav", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let y = prop("node", "node", "y");
    let parts = match mode {
        ResolveMode::AddEdge => vec![vec![prop("node", "label", "target")]],
        ResolveMode::Redirect => vec![Vec::new()],
    };
    let mut via = reference(
        "via",
        "node",
        RefSource::KeyValue { parts },
        "node",
        y.clone(),
    );
    via.target_key = Some("id".into());
    via.nullable = true;
    via.mode = mode;
    let schema = SchemaBuilder::new(&base)
        .vertex("node", "object", None)
        .unwrap()
        .vertex("label", "string", None)
        .unwrap()
        .edge("node", "node", "prop", Some("x"))
        .unwrap()
        .edge("node", "node", "prop", Some("y"))
        .unwrap()
        .edge("node", "node", "prop", Some("z"))
        .unwrap()
        .edge("node", "label", "prop", Some("id"))
        .unwrap()
        .edge("node", "label", "prop", Some("target"))
        .unwrap()
        .entry("node")
        .key(
            "node",
            key(
                "id",
                vec![KeyPart::Path(vec![prop("node", "label", "id")])],
                KeyRole::Unique,
                KeyScope::Document,
            ),
        )
        .reference(via)
        .build()
        .unwrap();
    (schema, y)
}

fn rav(mode: ResolveMode) -> Resolution {
    let (schema, y) = rav_schema(mode);
    let protocol = protocol("rav", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let mut spec = spec_for(
        "rav",
        ResolutionKind::TreeToGraph,
        &schema,
        &[(&y, "via")],
        &[],
    );
    spec.structural_edges = [
        prop("node", "node", "x"),
        prop("node", "node", "y"),
        prop("node", "node", "z"),
        prop("node", "label", "id"),
        prop("node", "label", "target"),
    ]
    .into_iter()
    .collect();
    let provider = Provider {
        id: provider_id("rav"),
        protocol: protocol.clone(),
        kind: ResolutionKind::TreeToGraph,
        spec,
        paths: BTreeMap::new(),
        context: None,
    };
    Resolution::for_protocol(&protocol, &schema, Arc::new(provider)).expect("the fixture compiles")
}

fn rav_node(id: u32, fields: &[(&str, &str)]) -> Node {
    let mut node = Node::new(id, "node");
    for (field, value) in fields {
        node.extra_fields.insert((*field).to_owned(), text(value));
    }
    node
}

/// A tree's nodes in document order: preorder, children in arc order.
fn preorder(tree: &WInstance) -> Vec<u32> {
    let mut order = Vec::new();
    let mut stack = vec![tree.root];
    while let Some(node) = stack.pop() {
        order.push(node);
        stack.extend(tree.children(node).iter().rev().copied());
    }
    order
}

/// r 0, a 1 and v 2 with edges x: r to a, z: a to v and y: r to v, under
/// the ids a canonical source gives them.
fn rav_view(r_fields: &[(&str, &str)]) -> GInstance {
    GInstance::new()
        .with_node(rav_node(0, r_fields))
        .with_node(rav_node(1, &[("id", "a")]))
        .with_node(rav_node(2, &[("id", "v")]))
        .with_edge_id(panproto_inst::EdgeId(0), 0, 1, prop("node", "node", "x"))
        .with_edge_id(panproto_inst::EdgeId(1), 1, 2, prop("node", "node", "z"))
        .with_edge_id(panproto_inst::EdgeId(2), 0, 2, prop("node", "node", "y"))
}

#[test]
fn canonical_placement_is_depth_first_so_embeddings_precede_references() {
    let adding = rav(ResolveMode::AddEdge);
    let view = AnyInstance::Graph(rav_view(&[("id", "r"), ("target", "v")]));
    let (source, complement) = adding
        .put_canonical(&view, &ResolveContext::empty())
        .expect("the view has a canonical source");
    let AnyInstance::Tree(tree) = &source else {
        panic!("a tree source");
    };
    assert_eq!(
        tree.parent(2),
        Some(1),
        "v is embedded under a, depth first"
    );
    assert_eq!(
        tree.arcs,
        vec![
            (0, 1, prop("node", "node", "x")),
            (1, 2, prop("node", "node", "z"))
        ]
    );
    let restored = adding
        .put(&view, &complement, &ResolveContext::empty())
        .expect("the canonical complement puts the view");
    assert!(same(&restored, &source));

    let redirecting = rav(ResolveMode::Redirect);
    let view = AnyInstance::Graph(rav_view(&[("id", "r")]));
    let (source, _) = redirecting
        .put_canonical(&view, &ResolveContext::empty())
        .expect("the view has a canonical source");
    let AnyInstance::Tree(tree) = &source else {
        panic!("a tree source");
    };
    let order = preorder(tree);
    let embedding = order.iter().position(|id| *id == 2).expect("v");
    let alias = order
        .iter()
        .position(|id| *id == 3)
        .expect("the alias holder");
    assert!(
        embedding < alias,
        "the embedding of v precedes the alias: {order:?}"
    );
    assert_eq!(
        tree.nodes[&3].value,
        Some(FieldPresence::Present(text("v")))
    );
}

// ---------------------------------------------------------------------------
// An assembly provider: a table of entries assembled into a list tree.
// ---------------------------------------------------------------------------

struct Entries {
    protocol: Protocol,
    spec: ResolutionSpec,
    cell: &'static str,
}

fn items_arc() -> Edge {
    prop("list", "item", "items")
}

impl Entries {
    fn new() -> (Self, Schema) {
        Self::reading("name")
    }

    fn reading(cell: &'static str) -> (Self, Schema) {
        let protocol = protocol("entries", InstanceShapeKind::Table, InstanceShapeKind::Tree);
        let syntactic = SchemaBuilder::new(&protocol)
            .vertex("entry", "table", None)
            .unwrap()
            .vertex("name", "string", None)
            .unwrap()
            .edge("entry", "name", "prop", Some("name"))
            .unwrap()
            .build()
            .unwrap();
        let semantic = SchemaBuilder::new(&protocol)
            .vertex("list", "object", None)
            .unwrap()
            .vertex("item", "string", None)
            .unwrap()
            .edge("list", "item", "prop", Some("items"))
            .unwrap()
            .entry("list")
            .build()
            .unwrap();
        let spec = ResolutionSpec {
            kind: ResolutionKind::TableToTree,
            provider: provider_id("entries"),
            semantic_schema: semantic,
            protocol_references: BTreeMap::new(),
            structural_edges: BTreeSet::new(),
            reference_writers: BTreeMap::new(),
            context_requirements: BTreeSet::new(),
            assembly: Some(panproto_io::resolution::AssemblyPlanId {
                grammar: "entries".into(),
                version: 1,
            }),
        };
        (
            Self {
                protocol,
                spec,
                cell,
            },
            syntactic,
        )
    }
}

fn assembled(
    table: &FInstance,
    cell: &str,
) -> (WInstance, Vec<panproto_io::resolution::AssemblyTrace>) {
    let mut nodes = vec![Node::new(0, "list")];
    let mut arcs = Vec::new();
    let mut trace = Vec::new();
    for (id, cells) in table.rows("entry") {
        let node = id.0 + 1;
        let name = cells.get(cell).cloned().unwrap_or(Value::Null);
        nodes.push(leaf(node, "item", name));
        arcs.push((0, node, items_arc()));
        trace.push(panproto_io::resolution::AssemblyTrace {
            production: "entry".into(),
            source: vec![ViewRef::Row(id)],
            view: vec![ViewRef::Node(node)],
        });
    }
    (tree(nodes, arcs, 0), trace)
}

impl ResolutionProvider for Entries {
    fn id(&self) -> ResolutionProviderId {
        provider_id("entries")
    }

    fn kind(&self) -> ResolutionKind {
        ResolutionKind::TableToTree
    }

    fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    fn spec(
        &self,
        _protocol: &Protocol,
        _syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        Ok(self.spec.clone())
    }

    fn assembly_get(
        &self,
        _plan: &panproto_io::resolution::AssemblyPlanId,
        source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, panproto_io::resolution::AssemblyResidue), ResolveError> {
        let AnyInstance::Table(table) = source else {
            return Err(ResolveError::ShapeMismatch {
                expected: InstanceShapeKind::Table,
                found: source.shape(),
            });
        };
        let (view, trace) = assembled(table, self.cell);
        Ok((
            AnyInstance::Tree(view),
            panproto_io::resolution::AssemblyResidue {
                source: Box::new(source.clone()),
                trace,
                context: None,
            },
        ))
    }

    fn assembly_check_put_domain(
        &self,
        _plan: &panproto_io::resolution::AssemblyPlanId,
        view: &AnyInstance,
        _residue: &panproto_io::resolution::AssemblyResidue,
        _ctx: &ResolveContext<'_>,
    ) -> Vec<ResolveDomainError> {
        let AnyInstance::Tree(tree) = view else {
            return vec![ResolveDomainError::Assembly {
                at: None,
                production: None,
                reason: panproto_io::resolution::AssemblyDomainReason::ResidueMismatch,
            }];
        };
        let mut violations = Vec::new();
        let canonical = tree.root == 0
            && tree.nodes.get(&0).is_some_and(|root| {
                root.anchor == "list"
                    && root.value.is_none()
                    && root.extra_fields.is_empty()
                    && root.discriminator.is_none()
                    && root.position.is_none()
                    && root.annotations.is_empty()
                    && root.shape == panproto_inst::NodeShape::Plain
            })
            && tree.fans.is_empty();
        if !canonical {
            violations.push(ResolveDomainError::Assembly {
                at: None,
                production: None,
                reason: panproto_io::resolution::AssemblyDomainReason::ResidueMismatch,
            });
        }
        let children = tree.children(0);
        let ascending = children.windows(2).all(|pair| pair[0] < pair[1]);
        let mut ids: Vec<u32> = tree.nodes.keys().copied().filter(|id| *id != 0).collect();
        ids.sort_unstable();
        for id in ids {
            let node = &tree.nodes[&id];
            let entry = node.anchor == "item"
                && matches!(node.value, Some(FieldPresence::Present(Value::Str(_))))
                && node.extra_fields.is_empty()
                && node.discriminator.is_none()
                && node.position.is_none()
                && node.annotations.is_empty()
                && node.shape == panproto_inst::NodeShape::Plain
                && tree.parent(id) == Some(0)
                && tree
                    .arcs
                    .iter()
                    .any(|(p, c, e)| *p == 0 && *c == id && *e == items_arc())
                && tree.children(id).is_empty()
                && ascending;
            if !entry {
                violations.push(ResolveDomainError::Assembly {
                    at: Some(ViewRef::Node(id)),
                    production: Some("entry".into()),
                    reason: panproto_io::resolution::AssemblyDomainReason::MissingProduction,
                });
            }
        }
        violations
    }

    fn assembly_put(
        &self,
        _plan: &panproto_io::resolution::AssemblyPlanId,
        view: &AnyInstance,
        residue: &panproto_io::resolution::AssemblyResidue,
        _ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        let (AnyInstance::Tree(tree), AnyInstance::Table(snapshot)) =
            (view, residue.source.as_ref())
        else {
            return Err(ResolveError::ShapeMismatch {
                expected: InstanceShapeKind::Tree,
                found: view.shape(),
            });
        };
        let mut table = FInstance::new();
        for &node in tree.children(0) {
            let row = RowId(node - 1);
            let mut cells = snapshot.row(row).cloned().unwrap_or_default();
            if let Some(FieldPresence::Present(name)) = &tree.nodes[&node].value {
                cells.insert(self.cell.to_owned(), name.clone());
            }
            table.insert_row("entry", row, cells).map_err(|_| {
                ResolveError::Instance(panproto_io::resolution::ResolutionInstanceError::NotTable)
            })?;
        }
        Ok(AnyInstance::Table(table))
    }

    fn assembly_put_canonical(
        &self,
        plan: &panproto_io::resolution::AssemblyPlanId,
        view: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, panproto_io::resolution::AssemblyResidue), ResolveError> {
        let empty = panproto_io::resolution::AssemblyResidue {
            source: Box::new(AnyInstance::Table(FInstance::new())),
            trace: Vec::new(),
            context: None,
        };
        let source = self.assembly_put(plan, view, &empty, ctx)?;
        let (_, residue) = self.assembly_get(plan, &source, ctx)?;
        Ok((source, residue))
    }
}

fn entries_table(names: &[&str]) -> FInstance {
    FInstance::new()
        .with_rows(
            "entry",
            names.iter().zip(0u32..).map(|(name, id)| {
                (
                    RowId(id),
                    row(&[("name", text(name)), ("note", int(i64::from(id)))]),
                )
            }),
        )
        .expect("entries")
}

#[test]
fn an_assembly_provider_puts_through_its_residue() {
    let (provider, syntactic) = Entries::new();
    let protocol = provider.protocol.clone();
    let resolution =
        Resolution::for_protocol(&protocol, &syntactic, Arc::new(provider)).expect("compiles");
    let source = AnyInstance::Table(entries_table(&["a", "b"]));
    let (view, complement) = resolution
        .get(
            &source,
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .expect("assembly get");
    let AnyInstance::Tree(tree) = &view else {
        panic!("a tree view");
    };
    assert_eq!(tree.children(0), &[1, 2]);
    assert!(complement.assembly.is_some());
    let restored = resolution
        .put(&view, &complement, &ResolveContext::empty())
        .expect("GetPut");
    assert!(same(&restored, &source), "the residue keeps the note cells");
    laws_hold(&resolution, &source, ResolveOptions::default());
    let (canonical, stamped) = resolution
        .put_canonical(&view, &ResolveContext::empty())
        .expect("a canonical source");
    assert_eq!(stamped.identity.as_ref(), Some(resolution.identity()));
    let AnyInstance::Table(canonical) = canonical else {
        panic!("a table source");
    };
    assert_eq!(
        canonical.row(RowId(1)).and_then(|row| row.get("name")),
        Some(&text("b"))
    );
}

#[test]
fn put_domain_assembly_refusal() {
    let (provider, syntactic) = Entries::new();
    let protocol = provider.protocol.clone();
    let resolution =
        Resolution::for_protocol(&protocol, &syntactic, Arc::new(provider)).expect("compiles");
    let source = AnyInstance::Table(entries_table(&["a", "b"]));
    let (view, complement) = resolution
        .get(
            &source,
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .expect("assembly get");
    let AnyInstance::Tree(mut tree) = view else {
        panic!("a tree view");
    };
    if let Some(node) = tree.nodes.get_mut(&2) {
        node.value = Some(FieldPresence::Present(int(7)));
    }
    let edited = AnyInstance::Tree(tree);
    let violations = resolution
        .check_put_domain(&edited, &complement, &ResolveContext::empty())
        .expect_err("an entry name must be a string");
    assert_eq!(
        violations,
        vec![ResolveDomainError::Assembly {
            at: Some(ViewRef::Node(2)),
            production: Some("entry".into()),
            reason: panproto_io::resolution::AssemblyDomainReason::MissingProduction,
        }]
    );
    assert!(matches!(
        resolution.put(&edited, &complement, &ResolveContext::empty()),
        Err(ResolveError::PutDomain(ResolveDomainError::Assembly { .. }))
    ));
    let mut without = complement;
    without.assembly = None;
    assert!(matches!(
        resolution.check_put_domain(&edited, &without, &ResolveContext::empty()),
        Err(violations) if violations == vec![ResolveDomainError::Assembly {
            at: None,
            production: None,
            reason: panproto_io::resolution::AssemblyDomainReason::ResidueMismatch,
        }]
    ));
}

#[test]
fn an_assembly_kind_needs_its_plan_and_its_provider() {
    let (mut provider, syntactic) = Entries::new();
    provider.spec.assembly = None;
    let protocol = provider.protocol.clone();
    assert!(matches!(
        Resolution::for_protocol(&protocol, &syntactic, Arc::new(provider)),
        Err(ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::Assembly(ResolutionKind::TableToTree),
            ..
        })
    ));
    let (entries, _) = Entries::new();
    let generic = Provider {
        id: provider_id("entries"),
        protocol: entries.protocol.clone(),
        kind: ResolutionKind::TableToTree,
        spec: entries.spec.clone(),
        paths: BTreeMap::new(),
        context: None,
    };
    let resolution = Resolution::for_protocol(&entries.protocol, &syntactic, Arc::new(generic))
        .expect("the plan compiles");
    let source = AnyInstance::Table(entries_table(&["a"]));
    assert!(matches!(
        resolution.get(
            &source,
            &ResolveOptions::default(),
            &ResolveContext::empty()
        ),
        Err(ResolveError::UnsupportedFeature {
            feature: ResolutionFeature::Assembly(ResolutionKind::TableToTree),
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// Registry routes.
// ---------------------------------------------------------------------------

/// A tree codec writing a tree as its JSON serialization.
struct TreeJson(&'static str);

impl panproto_io::InstanceParser for TreeJson {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn native_repr(&self) -> panproto_io::NativeRepr {
        panproto_io::NativeRepr::Tree
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        input: &[u8],
    ) -> Result<WInstance, panproto_io::ParseInstanceError> {
        serde_json::from_slice(input).map_err(|error| panproto_io::ParseInstanceError::Parse {
            protocol: self.0.to_owned(),
            message: error.to_string(),
        })
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, panproto_io::ParseInstanceError> {
        Err(panproto_io::ParseInstanceError::Parse {
            protocol: self.0.to_owned(),
            message: "a tree codec".to_owned(),
        })
    }
}

impl panproto_io::InstanceEmitter for TreeJson {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn emit_wtype(
        &self,
        _schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, panproto_io::EmitInstanceError> {
        serde_json::to_vec(instance).map_err(|error| panproto_io::EmitInstanceError::Emit {
            protocol: self.0.to_owned(),
            message: error.to_string(),
        })
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        _instance: &FInstance,
    ) -> Result<Vec<u8>, panproto_io::EmitInstanceError> {
        Err(panproto_io::EmitInstanceError::Emit {
            protocol: self.0.to_owned(),
            message: "a tree codec".to_owned(),
        })
    }
}

impl panproto_io::registry::ProtocolCodec for TreeJson {}

/// A graph codec writing a graph as its JSON serialization.
struct GraphJson(&'static str);

impl panproto_io::traits::GraphInstanceParser for GraphJson {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn parse_graph(
        &self,
        _schema: &Schema,
        input: &[u8],
    ) -> Result<GInstance, panproto_io::ParseInstanceError> {
        serde_json::from_slice(input).map_err(|error| panproto_io::ParseInstanceError::Parse {
            protocol: self.0.to_owned(),
            message: error.to_string(),
        })
    }
}

impl panproto_io::traits::GraphInstanceEmitter for GraphJson {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn emit_graph(
        &self,
        _schema: &Schema,
        instance: &GInstance,
    ) -> Result<Vec<u8>, panproto_io::EmitInstanceError> {
        serde_json::to_vec(instance).map_err(|error| panproto_io::EmitInstanceError::Emit {
            protocol: self.0.to_owned(),
            message: error.to_string(),
        })
    }
}

/// The posts provider for a protocol whose default shapes are `shapes`.
fn posts_provider(shapes: (InstanceShapeKind, InstanceShapeKind)) -> (Arc<Provider>, Schema) {
    let posts = posts_schema(|_| {});
    let protocol = protocol("posts", shapes.0, shapes.1);
    let provider = Provider {
        id: provider_id("posts"),
        protocol,
        kind: ResolutionKind::TreeToGraph,
        spec: spec_for(
            "posts",
            ResolutionKind::TreeToGraph,
            &posts.schema,
            &[(&posts.author_ref, "post_author")],
            &[],
        ),
        paths: BTreeMap::new(),
        context: None,
    };
    (Arc::new(provider), posts.schema)
}

const fn ctx() -> ResolveContext<'static> {
    ResolveContext::empty()
}

#[test]
fn same_shape_default_does_not_hide_a_registered_resolution_route() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_graph(GraphJson("posts"));
    registry.register_resolution_route(TreeJson("posts"), provider);
    let bytes = serde_json::to_vec(&posts_tree()).expect("serializes");
    let (view, complement) = registry
        .parse_semantic_with_complement(
            "posts",
            &schema,
            &bytes,
            &ResolveOptions::default(),
            &ctx(),
        )
        .expect("the route is selected although both default shapes are graphs");
    assert!(matches!(view, AnyInstance::Graph(_)));
    assert_eq!(complement.occurrences.len(), 2);
    let (graph, _) = registry
        .parse_graph_with_complement("posts", &schema, &bytes, &ResolveOptions::default(), &ctx())
        .expect("the checked graph wrapper");
    assert_eq!(graph.edge_count(), 10);
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Ok(true)
    ));
}

#[test]
fn native_and_resolved_routes_coexist() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_graph(GraphJson("posts"));
    registry.register_resolution_route(TreeJson("posts"), provider);
    let native = GInstance::new().with_node(Node::new(4, "user"));
    let graph_bytes = serde_json::to_vec(&native).expect("serializes");
    let parsed = registry
        .parse_graph("posts", &schema, &graph_bytes)
        .expect("parse_graph keeps the native codec");
    assert!(GInstance::identical(&parsed, &native));
    assert_eq!(
        registry
            .emit_graph("posts", &schema, &native)
            .expect("native emit"),
        graph_bytes
    );
    assert!(registry.supports_graph("posts"));
}

#[test]
fn native_repr_ignores_a_dedicated_resolution_codec() {
    let (provider, _) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_graph(GraphJson("posts"));
    registry.register_resolution_route(TreeJson("posts"), provider);
    assert_eq!(
        registry.native_repr("posts").expect("registered"),
        panproto_io::NativeRepr::Graph
    );
    assert!(
        registry
            .parse_wtype("posts", &Schema::default(), b"{}")
            .is_err(),
        "the dedicated tree codec is not the default codec"
    );
}

/// A table-native codec reading nothing, for shape checks.
struct TableStub(&'static str);

impl panproto_io::InstanceParser for TableStub {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn native_repr(&self) -> panproto_io::NativeRepr {
        panproto_io::NativeRepr::Table
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<WInstance, panproto_io::ParseInstanceError> {
        Err(panproto_io::ParseInstanceError::Parse {
            protocol: self.0.to_owned(),
            message: "a table codec".to_owned(),
        })
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, panproto_io::ParseInstanceError> {
        Ok(FInstance::new())
    }
}

impl panproto_io::InstanceEmitter for TableStub {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn emit_wtype(
        &self,
        _schema: &Schema,
        _instance: &WInstance,
    ) -> Result<Vec<u8>, panproto_io::EmitInstanceError> {
        Ok(Vec::new())
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        _instance: &FInstance,
    ) -> Result<Vec<u8>, panproto_io::EmitInstanceError> {
        Ok(Vec::new())
    }
}

impl panproto_io::registry::ProtocolCodec for TableStub {}

#[test]
fn resolved_route_checks_its_codec_source_shape() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    let () = registry.register_resolution_route(TableStub("posts"), provider.clone());
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Err(ResolveError::UnsupportedKind {
            syntactic: InstanceShapeKind::Table,
            semantic: InstanceShapeKind::Graph
        })
    ));
    assert!(
        !registry.supports_graph("posts"),
        "a malformed route is not advertised"
    );
    let () = registry.register_resolution_provider(provider.clone());
    assert!(
        matches!(
            registry.supports_semantic_for("posts", &schema),
            Err(ResolveError::UnsupportedKind {
                syntactic: InstanceShapeKind::Graph,
                semantic: InstanceShapeKind::Graph
            })
        ),
        "a default route reads the protocol's syntactic shape"
    );
    registry.register_resolution_route(TreeJson("other"), provider);
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Err(ResolveError::ProviderFailure { code, .. }) if code == "route_name"
    ));
    assert!(
        !registry.supports_graph("posts"),
        "a misnamed route is not advertised"
    );
    let (tree_default, _) = posts_provider((InstanceShapeKind::Tree, InstanceShapeKind::Graph));
    registry.register_resolution_provider(tree_default);
    assert!(
        matches!(
            registry.supports_semantic_for("posts", &schema),
            Err(ResolveError::UnsupportedKind { .. })
        ),
        "no default codec reads the tree source yet"
    );
    registry.register(TreeJson("posts"));
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Ok(true)
    ));
    assert!(registry.supports_graph("posts"));
}

#[test]
fn a_route_is_keyed_by_its_provider_protocol_and_checked_when_compiled() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Tree, InstanceShapeKind::Graph));
    let misnamed = Arc::new(Provider {
        id: provider_id("other"),
        protocol: provider.protocol.clone(),
        kind: provider.kind,
        spec: provider.spec.clone(),
        paths: BTreeMap::new(),
        context: None,
    });
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register(TreeJson("posts"));
    registry.register_resolution_provider(misnamed);
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Err(ResolveError::ProviderFailure { code, .. }) if code == "route_name"
    ));
    assert!(
        matches!(registry.supports_semantic_for("other", &schema), Ok(false)),
        "the route is keyed by the provider's protocol, not its id"
    );
    let wrong_target = Arc::new(Provider {
        id: provider_id("posts"),
        protocol: provider.protocol.clone(),
        kind: ResolutionKind::TreeToTable,
        spec: provider.spec.clone(),
        paths: BTreeMap::new(),
        context: None,
    });
    registry.register_resolution_provider(wrong_target);
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Err(ResolveError::UnsupportedKind {
            syntactic: InstanceShapeKind::Tree,
            semantic: InstanceShapeKind::Graph
        })
    ));
    assert!(!registry.supports_table("posts"));
    registry.register_resolution_provider(provider);
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Ok(true)
    ));
}

#[test]
fn supports_semantic_for_rejects_an_invalid_dedicated_route() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_resolution_route(TreeJson("posts"), provider);
    let foreign = SchemaBuilder::new(&protocol(
        "posts",
        InstanceShapeKind::Tree,
        InstanceShapeKind::Graph,
    ))
    .vertex("doc", "object", None)
    .unwrap()
    .build()
    .unwrap();
    assert!(
        matches!(
            registry.supports_semantic_for("posts", &foreign),
            Err(ResolveError::Schema(
                panproto_schema::KeyRefError::PathDoesNotResolve { .. }
            ))
        ),
        "the provider's spec does not fit this syntactic schema"
    );
    assert!(matches!(
        registry.supports_semantic_for("posts", &schema),
        Ok(true)
    ));
    assert!(
        matches!(
            registry.supports_semantic_for("unregistered-format", &Schema::default()),
            Ok(false)
        ),
        "no route and no descriptor"
    );
}

const fn resolution_state_refusal(error: &ResolveError) -> bool {
    matches!(
        error,
        ResolveError::UnsupportedFeature {
            provider: None,
            feature: ResolutionFeature::ResolutionState,
        }
    )
}

#[test]
fn resolution_state_without_a_resolution_route_is_rejected() {
    let (_, schema) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_graph(GraphJson("posts"));
    let bytes = serde_json::to_vec(&GInstance::new()).expect("serializes");
    match registry.parse_semantic_with_complement(
        "posts",
        &schema,
        &bytes,
        &ResolveOptions::default(),
        &ctx(),
    ) {
        Err(panproto_io::ParseInstanceError::Resolution(error)) => {
            assert!(resolution_state_refusal(&error));
        }
        other => panic!("no route, no resolution state: {other:?}"),
    }
    match registry.parse_semantic_preserving(
        "posts",
        &schema,
        &bytes,
        &ResolveOptions::default(),
        &ctx(),
    ) {
        Err(panproto_io::ParseInstanceError::Resolution(error)) => {
            assert!(resolution_state_refusal(&error));
        }
        other => panic!("no route, no preserving resolution state: {other:?}"),
    }
    let complement = ResolutionComplement::default();
    match registry.emit_semantic(
        "posts",
        &schema,
        &AnyInstance::Graph(GInstance::new()),
        Some(&complement),
        &ctx(),
    ) {
        Err(panproto_io::EmitInstanceError::Resolution(error)) => {
            assert!(resolution_state_refusal(&error));
        }
        other => panic!("a complement needs a route: {other:?}"),
    }
    let state = panproto_io::ResolvedState {
        presentation: None,
        resolution: complement,
    };
    match registry.emit_semantic_preserving(
        "posts",
        &schema,
        &AnyInstance::Graph(GInstance::new()),
        &state,
        &ctx(),
    ) {
        Err(panproto_io::EmitInstanceError::Resolution(error)) => {
            assert!(resolution_state_refusal(&error));
        }
        other => panic!("preserving emit needs a route: {other:?}"),
    }
}

#[test]
fn a_supplied_resolution_complement_forces_the_resolved_route() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_graph(GraphJson("posts"));
    registry.register_resolution_route(TreeJson("posts"), provider);
    let tree_bytes = serde_json::to_vec(&posts_tree()).expect("serializes");
    let (view, complement) = registry
        .parse_semantic_with_complement(
            "posts",
            &schema,
            &tree_bytes,
            &ResolveOptions::default(),
            &ctx(),
        )
        .expect("the route");
    let emitted = registry
        .emit_semantic("posts", &schema, &view, Some(&complement), &ctx())
        .expect("put through the route");
    assert_eq!(
        emitted, tree_bytes,
        "the dedicated codec writes the restored tree"
    );
}

#[test]
fn complement_free_semantic_io_prefers_the_native_semantic_codec() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_graph(GraphJson("posts"));
    registry.register_resolution_route(TreeJson("posts"), provider);
    let native = GInstance::new().with_node(Node::new(4, "user"));
    let graph_bytes = serde_json::to_vec(&native).expect("serializes");
    let parsed = registry
        .parse_semantic(
            "posts",
            &schema,
            &graph_bytes,
            &ResolveOptions::default(),
            &ctx(),
        )
        .expect("the native graph codec");
    assert!(matches!(&parsed, AnyInstance::Graph(graph) if GInstance::identical(graph, &native)));
    let emitted = registry
        .emit_semantic("posts", &schema, &AnyInstance::Graph(native), None, &ctx())
        .expect("the native graph codec");
    assert_eq!(emitted, graph_bytes);
}

#[test]
fn without_a_native_codec_semantic_io_uses_the_route_and_canonical_put() {
    let (provider, schema) = posts_provider((InstanceShapeKind::Tree, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register(TreeJson("posts"));
    registry.register_resolution_provider(provider);
    let canonical = serde_json::to_vec(&canonical_posts_tree()).expect("serializes");
    let view = registry
        .parse_semantic(
            "posts",
            &schema,
            &canonical,
            &ResolveOptions::default(),
            &ctx(),
        )
        .expect("resolved through the route");
    assert!(matches!(view, AnyInstance::Graph(_)));
    let emitted = registry
        .emit_semantic("posts", &schema, &view, None, &ctx())
        .expect("canonical put");
    assert_eq!(emitted, canonical);
    let graph = registry
        .parse_graph("posts", &schema, &canonical)
        .expect("parse_graph reaches the route");
    assert_eq!(graph.edge_count(), 10);
    let noncanonical = serde_json::to_vec(&posts_tree()).expect("serializes");
    let view = registry
        .parse_semantic(
            "posts",
            &schema,
            &noncanonical,
            &ResolveOptions::default(),
            &ctx(),
        )
        .expect("resolved");
    assert!(
        matches!(
            registry.emit_semantic("posts", &schema, &view, None, &ctx()),
            Err(panproto_io::EmitInstanceError::Resolution(error))
                if matches!(*error, ResolveError::PutDomain(ResolveDomainError::NoPlacement { .. }))
        ),
        "a view whose arc ids are not canonical has no canonical source"
    );
}

#[test]
fn a_protocol_with_neither_a_codec_nor_a_route_has_no_provider() {
    let registry = panproto_io::ProtocolRegistry::new();
    assert!(matches!(
        registry.parse_semantic("nothing-here", &Schema::default(), b"", &ResolveOptions::default(), &ctx()),
        Err(panproto_io::ParseInstanceError::Resolution(error))
            if matches!(*error, ResolveError::ProviderUnavailable { .. })
    ));
}

#[test]
fn table_to_tree_is_reachable_through_the_semantic_entry_points() {
    let (provider, syntactic) = Entries::reading("col_0");
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register(panproto_io::byte_tabular::ByteTabularCodec::new(
        "entries",
        "entry",
        b'\t',
        Some(b'#'),
    ));
    registry.register_resolution_provider(Arc::new(provider));
    assert!(registry.supports_tree("entries"));
    let input = b"# names\r\nalpha\r\nbeta";
    let (view, state) = registry
        .parse_semantic_preserving(
            "entries",
            &syntactic,
            input,
            &ResolveOptions::default(),
            &ctx(),
        )
        .expect("assembled through the route");
    assert!(matches!(view, AnyInstance::Tree(_)));
    assert!(state.presentation.is_some(), "the byte layout is kept");
    assert!(
        state.resolution.assembly.is_some(),
        "the assembly residue is kept"
    );
    let emitted = registry
        .emit_semantic_preserving("entries", &syntactic, &view, &state, &ctx())
        .expect("put, then the layout replayed");
    assert_eq!(
        emitted,
        input.to_vec(),
        "comments, CRLF and the missing final newline survive"
    );
}

#[test]
fn every_advertised_graph_semantic_protocol_has_a_working_route() {
    let registry = panproto_io::default_registry();
    let mut checked = 0;
    for name in registry.protocol_names() {
        let Some(descriptor) = panproto_protocols::registry::descriptor(name) else {
            continue;
        };
        if (descriptor.protocol)().semantic_shape != InstanceShapeKind::Graph
            || !registry.supports_graph(name)
        {
            continue;
        }
        checked += 1;
        assert!(
            matches!(
                registry.supports_semantic_for(name, &Schema::default()),
                Ok(true)
            ),
            "{name} advertises a graph without a working route"
        );
    }
    let _ = checked;
}

// ---------------------------------------------------------------------------
// Wire forms and precedence.
// ---------------------------------------------------------------------------

/// The external tag a value serializes under.
fn tag<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("serializes") {
        serde_json::Value::String(tag) => tag,
        serde_json::Value::Object(object) => {
            assert_eq!(object.len(), 1, "externally tagged");
            object.keys().next().cloned().expect("one key")
        }
        other => panic!("not externally tagged: {other}"),
    }
}

fn round_trips<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T) {
    let json = serde_json::to_value(value).expect("serializes");
    let back: T = serde_json::from_value(json.clone()).expect("deserializes");
    assert_eq!(serde_json::to_value(&back).expect("serializes"), json);
}

/// One resolve error of each variant, in declaration order.
fn one_resolve_error_of_each_variant() -> Vec<ResolveError> {
    let at = || Box::new(ViewRef::Node(3));
    let reference = || Name::from("r");
    vec![
        ResolveError::DanglingReference {
            reference: reference(),
            at: at(),
            key: vec![int(1)],
        },
        ResolveError::AmbiguousReference {
            reference: reference(),
            at: at(),
            key: vec![int(1)],
            candidates: vec![ViewRef::Node(1), ViewRef::Node(2)],
        },
        ResolveError::OffsetOutOfRange {
            reference: reference(),
            at: at(),
            start: 1,
            end: 9,
            len: 4,
        },
        ResolveError::OffsetUnitViolation {
            reference: reference(),
            at: at(),
            unit: OffsetUnit::Utf16,
            offset: 2,
            reason: OffsetUnitReason::NotOnBoundary,
        },
        ResolveError::ShapeMismatch {
            expected: InstanceShapeKind::Tree,
            found: InstanceShapeKind::Graph,
        },
        ResolveError::Identity(panproto_inst::IdentityError::UnboundComplement),
        ResolveError::UnsupportedKind {
            syntactic: InstanceShapeKind::Tree,
            semantic: InstanceShapeKind::Table,
        },
        ResolveError::ReferenceConflict {
            reference: reference(),
        },
        ResolveError::ReferenceCardinalityMismatch {
            reference: reference(),
            at: at(),
            lengths: vec![2, 1],
        },
        ResolveError::ProviderUnavailable {
            protocol: "p".to_owned(),
        },
        ResolveError::UnsupportedFeature {
            provider: Some(provider_id("p")),
            feature: ResolutionFeature::TokenBoundaries,
        },
        ResolveError::ContextMismatch {
            at: Some(at()),
            expected: panproto_inst::Digest([1; 32]),
            found: None,
        },
        ResolveError::IdExhausted {
            kind: panproto_inst::segment::IdKind::Node,
        },
        ResolveError::Schema(panproto_schema::KeyRefError::EdgeRequired {
            reference: reference(),
        }),
        ResolveError::Fidelity(panproto_inst::ValueFidelityError {
            site: panproto_inst::ValueSite::new("v", None),
            lexeme: "x".to_owned(),
            target: panproto_schema::ScalarKind::Str,
            reason: panproto_inst::FidelityReason::InvalidEncoding,
        }),
        ResolveError::Instance(panproto_io::resolution::ResolutionInstanceError::NotTree),
        ResolveError::ProviderFailure {
            provider: provider_id("p"),
            code: "c".into(),
            at: None,
        },
        ResolveError::PutDomain(ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(1),
        }),
    ]
}

#[test]
fn every_resolve_error_variant_has_its_wire_form() {
    let errors = one_resolve_error_of_each_variant();
    let tags: Vec<String> = errors.iter().map(tag).collect();
    assert_eq!(
        tags,
        [
            "dangling_reference",
            "ambiguous_reference",
            "offset_out_of_range",
            "offset_unit_violation",
            "shape_mismatch",
            "identity",
            "unsupported_kind",
            "reference_conflict",
            "reference_cardinality_mismatch",
            "provider_unavailable",
            "unsupported_feature",
            "context_mismatch",
            "id_exhausted",
            "schema",
            "fidelity",
            "instance",
            "provider_failure",
            "put_domain",
        ]
    );
    for error in &errors {
        round_trips(error);
    }
    assert_eq!(
        serde_json::to_string(&errors[3]).expect("serializes"),
        r#"{"offset_unit_violation":{"reference":"r","at":{"node":3},"unit":"utf16","offset":2,"reason":"not_on_boundary"}}"#
    );
}

#[test]
fn every_resolution_feature_reason_and_instance_error_has_its_wire_form() {
    let features = [
        ResolutionFeature::Path(PathSyntax::XPath),
        ResolutionFeature::ExternalText,
        ResolutionFeature::TokenBoundaries,
        ResolutionFeature::Assembly(ResolutionKind::TableToTree),
        ResolutionFeature::ReferenceWriter,
        ResolutionFeature::ResolutionState,
        ResolutionFeature::NormalizedOffsets,
    ];
    let feature_tags: Vec<String> = features.iter().map(tag).collect();
    assert_eq!(
        feature_tags,
        [
            "path",
            "external_text",
            "token_boundaries",
            "assembly",
            "reference_writer",
            "resolution_state",
            "normalized_offsets"
        ]
    );
    let reasons = [
        OffsetUnitReason::NotOnBoundary,
        OffsetUnitReason::TextNotUtf8,
        OffsetUnitReason::EmptyInclusive,
    ];
    assert_eq!(
        serde_json::to_string(&reasons).expect("serializes"),
        r#"["not_on_boundary","text_not_utf8","empty_inclusive"]"#
    );
    let instance_errors = [
        panproto_io::resolution::ResolutionInstanceError::MissingCarrier {
            carrier: ViewRef::Node(1),
        },
        panproto_io::resolution::ResolutionInstanceError::DanglingCarrier {
            carrier: ViewRef::Node(1),
        },
        panproto_io::resolution::ResolutionInstanceError::NotTree,
        panproto_io::resolution::ResolutionInstanceError::NotTable,
        panproto_io::resolution::ResolutionInstanceError::NotGraph,
    ];
    let instance_tags: Vec<String> = instance_errors.iter().map(tag).collect();
    assert_eq!(
        instance_tags,
        [
            "missing_carrier",
            "dangling_carrier",
            "not_tree",
            "not_table",
            "not_graph"
        ]
    );
}

#[test]
fn every_resolve_domain_error_variant_has_its_wire_form() {
    let carrier = || ViewRef::Edge(panproto_inst::EdgeId(4));
    let errors = [
        ResolveDomainError::NoPlacement { carrier: carrier() },
        ResolveDomainError::AmbiguousPlacement {
            carrier: carrier(),
            references: vec!["a".into(), "b".into()],
        },
        ResolveDomainError::UnrepresentableReference {
            carrier: carrier(),
            reference: "r".into(),
        },
        ResolveDomainError::OffsetOutOfRange {
            carrier: carrier(),
            reference: "r".into(),
        },
        ResolveDomainError::OffsetUnitViolation {
            carrier: carrier(),
            reference: "r".into(),
            reason: OffsetUnitReason::EmptyInclusive,
        },
        ResolveDomainError::SynthesizedEdited { carrier: carrier() },
        ResolveDomainError::ContextMismatch { at: None },
        ResolveDomainError::Assembly {
            at: None,
            production: Some("p".into()),
            reason: panproto_io::resolution::AssemblyDomainReason::AmbiguousProduction,
        },
    ];
    let tags: Vec<String> = errors.iter().map(tag).collect();
    assert_eq!(
        tags,
        [
            "no_placement",
            "ambiguous_placement",
            "unrepresentable_reference",
            "offset_out_of_range",
            "offset_unit_violation",
            "synthesized_edited",
            "context_mismatch",
            "assembly",
        ]
    );
    for (index, error) in errors.iter().enumerate() {
        round_trips(error);
        assert_eq!(usize::from(error.variant_index()), index);
    }
    assert_eq!(
        serde_json::to_string(&errors[4]).expect("serializes"),
        r#"{"offset_unit_violation":{"carrier":{"edge":4},"reference":"r","reason":"empty_inclusive"}}"#
    );
    let reasons = [
        panproto_io::resolution::AssemblyDomainReason::MissingProduction,
        panproto_io::resolution::AssemblyDomainReason::AmbiguousProduction,
        panproto_io::resolution::AssemblyDomainReason::ResidueMismatch,
    ];
    assert_eq!(
        serde_json::to_string(&reasons).expect("serializes"),
        r#"["missing_production","ambiguous_production","residue_mismatch"]"#
    );
}

#[test]
fn resolve_failures_follow_the_stated_precedence() {
    let (resolution, _) = posts_resolution(|_| {});
    let (view, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    let mut edited = view.clone();
    edited
        .insert_node(Node::new(100, "user"))
        .expect("an unplaceable node");
    let mut unbound = complement.clone();
    unbound.identity = None;
    // Shape precedes identity and domain.
    assert!(matches!(
        resolution.put(
            &AnyInstance::Tree(posts_tree()),
            &unbound,
            &ResolveContext::empty()
        ),
        Err(ResolveError::ShapeMismatch { .. })
    ));
    // Identity precedes domain.
    assert!(matches!(
        resolution.put(
            &AnyInstance::Graph(edited.clone()),
            &unbound,
            &ResolveContext::empty()
        ),
        Err(ResolveError::Identity(_))
    ));
    // Domain precedes the operation.
    assert!(matches!(
        resolution.put(
            &AnyInstance::Graph(edited),
            &complement,
            &ResolveContext::empty()
        ),
        Err(ResolveError::PutDomain(
            ResolveDomainError::NoPlacement { .. }
        ))
    ));
    // Get: shape first, then the source in holder order.
    assert!(matches!(
        resolution.get(
            &AnyInstance::Graph(view),
            &ResolveOptions::default(),
            &ResolveContext::empty()
        ),
        Err(ResolveError::ShapeMismatch { .. })
    ));
    let both = AnyInstance::Tree(posts_tree_with(text("nobody"), text("ghost")));
    assert!(
        matches!(
            resolution.get(&both, &ResolveOptions::default(), &ResolveContext::empty()),
            Err(ResolveError::DanglingReference { at, .. }) if *at == ViewRef::Node(5)
        ),
        "the first failing occurrence in holder order"
    );
}

// ---------------------------------------------------------------------------
// The tree embedding, and bounded exhaustive law checks over enumerated
// fixture families. The enumerators are plain loops over finite alphabets;
// they call nothing under test.
// ---------------------------------------------------------------------------

#[test]
fn from_wtype_stays_structural_and_resolves_nothing() {
    let posts = posts_schema(|_| {});
    let tree = posts_tree();
    let structural = GInstance::from_wtype(&tree).expect("the tree's graph form");
    assert_eq!(structural.edge_count(), tree.arcs.len());
    assert!(
        edge_triples(&structural)
            .iter()
            .all(|(_, _, label)| label != "author_ref")
    );
    let (provider, schema) = posts_provider((InstanceShapeKind::Tree, InstanceShapeKind::Graph));
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register(TreeJson("posts"));
    registry.register_resolution_provider(provider);
    let resolved = registry
        .parse_graph(
            "posts",
            &schema,
            &serde_json::to_vec(&tree).expect("serializes"),
        )
        .expect("the route");
    let references = edge_triples(&resolved)
        .into_iter()
        .filter(|(_, _, label)| label == "author_ref")
        .count();
    assert_eq!(references, 2);
    assert_eq!(resolved.edge_count(), structural.edge_count() + references);
    let _ = posts;
}

/// Every posts tree with up to `bound` users (ids from "u1", "u2") and up to
/// `bound` posts (authors from "u1", "u2", "zz"), users before posts.
fn all_posts_trees(bound: usize) -> Vec<WInstance> {
    fn sequences(alphabet: &[&'static str], bound: usize) -> Vec<Vec<&'static str>> {
        let mut all = vec![Vec::new()];
        let mut frontier = vec![Vec::new()];
        for _ in 0..bound {
            let mut next = Vec::new();
            for prefix in &frontier {
                for letter in alphabet {
                    let mut longer: Vec<&'static str> = prefix.clone();
                    longer.push(letter);
                    next.push(longer);
                }
            }
            all.extend(next.iter().cloned());
            frontier = next;
        }
        all
    }
    let mut trees = Vec::new();
    for users in sequences(&["u1", "u2"], bound) {
        for authors in sequences(&["u1", "u2", "zz"], bound) {
            let mut nodes = vec![Node::new(0, "doc")];
            let mut arcs = Vec::new();
            let mut next = 1u32;
            for user in &users {
                nodes.push(Node::new(next, "user"));
                nodes.push(leaf(next + 1, "user_id", text(user)));
                arcs.push((0, next, prop("doc", "user", "users")));
                arcs.push((next, next + 1, prop("user", "user_id", "id")));
                next += 2;
            }
            for author in &authors {
                nodes.push(Node::new(next, "post"));
                nodes.push(leaf(next + 1, "author", text(author)));
                arcs.push((0, next, prop("doc", "post", "posts")));
                arcs.push((next, next + 1, prop("post", "author", "author")));
                next += 2;
            }
            trees.push(tree(nodes, arcs, 0));
        }
    }
    trees
}

#[test]
fn the_laws_hold_on_every_posts_tree_up_to_two_users_and_two_posts() {
    let trees = all_posts_trees(2);
    assert_eq!(trees.len(), 7 * 13);
    let lenient = ResolveOptions { lenient: true };
    let (strict, _) = posts_resolution(|_| {});
    let (recorded, _) =
        posts_resolution(|reference| reference.on_unresolved = OnUnresolved::Complement);
    for source in trees {
        let source = AnyInstance::Tree(source);
        laws_hold(&strict, &source, lenient);
        laws_hold(&recorded, &source, ResolveOptions::default());
        match strict.get(
            &source,
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        ) {
            Ok(_) => laws_hold(&strict, &source, ResolveOptions::default()),
            Err(error) => assert!(matches!(
                error,
                ResolveError::DanglingReference { .. } | ResolveError::AmbiguousReference { .. }
            )),
        }
    }
}

/// Every alias map with up to two anchored nodes (anchors from "a", "b")
/// and up to two aliases (names from "a", "b", "c"), in both orders.
fn all_alias_trees(fixture: &Aliases) -> Vec<WInstance> {
    let anchors: Vec<Vec<&str>> =
        vec![vec![], vec!["a"], vec!["b"], vec!["a", "b"], vec!["a", "a"]];
    let names: Vec<Vec<&str>> = vec![vec![], vec!["a"], vec!["c"], vec!["a", "b"], vec!["b", "b"]];
    let mut trees = Vec::new();
    for nodes in &anchors {
        for aliases in &names {
            for aliases_first in [false, true] {
                let mut all = vec![Node::new(0, "map")];
                let mut arcs = Vec::new();
                let mut next = 1u32;
                let place_nodes = |all: &mut Vec<Node>, arcs: &mut Vec<_>, next: &mut u32| {
                    for anchor in nodes {
                        all.push(anchored(*next, anchor));
                        all.push(leaf(*next + 1, "value", text("x")));
                        arcs.push((0, *next, fixture.item.clone()));
                        arcs.push((*next, *next + 1, fixture.val.clone()));
                        *next += 2;
                    }
                };
                let place_aliases = |all: &mut Vec<Node>, arcs: &mut Vec<_>, next: &mut u32| {
                    for name in aliases {
                        all.push(leaf(*next, "alias", text(name)));
                        arcs.push((0, *next, fixture.item_alias.clone()));
                        *next += 1;
                    }
                };
                if aliases_first {
                    place_aliases(&mut all, &mut arcs, &mut next);
                    place_nodes(&mut all, &mut arcs, &mut next);
                } else {
                    place_nodes(&mut all, &mut arcs, &mut next);
                    place_aliases(&mut all, &mut arcs, &mut next);
                }
                trees.push(tree(all, arcs, 0));
            }
        }
    }
    trees
}

#[test]
fn the_laws_hold_on_every_alias_map_up_to_two_nodes_and_two_aliases() {
    let fixture = aliases_schema(|reference| reference.on_unresolved = OnUnresolved::Complement);
    let resolution = compile(
        "aliases",
        ResolutionKind::TreeToGraph,
        &fixture.schema,
        &[(&fixture.alias_of, "alias_ref")],
    );
    let trees = all_alias_trees(&fixture);
    assert_eq!(trees.len(), 50);
    for source in trees {
        laws_hold(
            &resolution,
            &AnyInstance::Tree(source),
            ResolveOptions::default(),
        );
    }
}

#[test]
fn the_laws_hold_on_every_short_byte_and_scalar_span() {
    let bytes = Standoff::local("by_byte", OffsetUnit::Byte);
    let chars = Standoff {
        start: "from",
        end: "to",
        ..Standoff::local("by_char", OffsetUnit::Utf8Char)
    };
    let resolution = notes(&[bytes, chars], &[]);
    let mut checked = 0;
    for body in ["", "ab", "hé"] {
        for (start, end, from, to) in (0..=3)
            .flat_map(|a| (0..=3).map(move |b| (a, b)))
            .flat_map(|(a, b)| (0..=2).flat_map(move |c| (0..=2).map(move |d| (a, b, c, d))))
        {
            let source = AnyInstance::Tree(notes_tree(vec![note(
                1,
                &[
                    ("text", text(body)),
                    ("start", int(start)),
                    ("end", int(end)),
                    ("from", int(from)),
                    ("to", int(to)),
                ],
            )]));
            match resolution.get(
                &source,
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            ) {
                Ok(_) => {
                    laws_hold(&resolution, &source, ResolveOptions::default());
                    checked += 1;
                }
                Err(error) => assert!(
                    matches!(
                        error,
                        ResolveError::OffsetOutOfRange { .. }
                            | ResolveError::OffsetUnitViolation { .. }
                    ),
                    "{error:?}"
                ),
            }
        }
    }
    assert!(checked > 0);
}

/// Every orders table with up to two customers (ids "a", "b") and up to two
/// orders naming customers from "a", "b", "zz".
#[test]
fn the_laws_hold_on_every_orders_table_up_to_two_customers_and_two_orders() {
    let fixture = orders_schema(false);
    let resolution = {
        let mut schema = fixture.schema.clone();
        for reference in schema.declarations.references.values_mut() {
            reference.on_unresolved = OnUnresolved::Complement;
        }
        compile(
            "orders",
            ResolutionKind::TableToGraph,
            &schema,
            &[
                (&fixture.placed_by, "placed"),
                (&fixture.left, "left_text"),
                (&fixture.right, "right_text"),
            ],
        )
    };
    let customers: Vec<Vec<&str>> = vec![vec![], vec!["a"], vec!["a", "b"], vec!["a", "a"]];
    let orders: Vec<Vec<&str>> = vec![
        vec![],
        vec!["a"],
        vec!["zz"],
        vec!["a", "b"],
        vec!["b", "zz"],
    ];
    for cs in &customers {
        for os in &orders {
            let mut table = FInstance::new();
            for (id, cid) in (0u32..).zip(cs) {
                table
                    .insert_row("customer", RowId(id), row(&[("cid", text(cid))]))
                    .expect("a customer");
            }
            for (id, customer) in (10u32..).zip(os) {
                table
                    .insert_row("order", RowId(id), row(&[("customer", text(customer))]))
                    .expect("an order");
            }
            laws_hold(
                &resolution,
                &AnyInstance::Table(table),
                ResolveOptions::default(),
            );
        }
    }
}

mod embedding {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn tree_to_graph_restricted_to_the_tree_is_the_embedding(
            users in proptest::collection::vec(prop_oneof![Just("u1"), Just("u2"), Just("u3")], 0..4),
            authors in proptest::collection::vec(prop_oneof![Just("u1"), Just("u2"), Just("zz")], 0..4),
            posts_first in any::<bool>(),
        ) {
            let mut nodes = vec![Node::new(0, "doc")];
            let mut arcs = Vec::new();
            let mut users_part = Vec::new();
            let mut posts_part = Vec::new();
            let mut next = 1u32;
            for user in &users {
                nodes.push(Node::new(next, "user"));
                nodes.push(leaf(next + 1, "user_id", text(user)));
                users_part.push((0, next, prop("doc", "user", "users")));
                users_part.push((next, next + 1, prop("user", "user_id", "id")));
                next += 2;
            }
            for author in &authors {
                nodes.push(Node::new(next, "post"));
                nodes.push(leaf(next + 1, "author", text(author)));
                posts_part.push((0, next, prop("doc", "post", "posts")));
                posts_part.push((next, next + 1, prop("post", "author", "author")));
                next += 2;
            }
            if posts_first {
                arcs.extend(posts_part);
                arcs.extend(users_part);
            } else {
                arcs.extend(users_part);
                arcs.extend(posts_part);
            }
            let source_tree = tree(nodes, arcs, 0);
            let (resolution, _) =
                posts_resolution(|reference| reference.on_unresolved = OnUnresolved::Complement);
            let source = AnyInstance::Tree(source_tree.clone());
            let (view, complement) = get(&resolution, &source);
            let (embedded, embedding) = GInstance::from_wtype_embedded(&source_tree, Some(resolution.syntactic_schema()))
                .expect("the tree's graph form");
            let mut restricted = view.clone();
            let mut occurrence_ids = Vec::new();
            for (carrier, records) in &complement.occurrences {
                if let ViewRef::Edge(id) = carrier {
                    restricted.remove_edge(*id).expect("an occurrence edge");
                    occurrence_ids.push((records[0].holder.clone(), records[0].index, *id));
                }
            }
            prop_assert!(GInstance::identical(&restricted, &embedded));
            let mut ordered = occurrence_ids.clone();
            ordered.sort();
            let ids: Vec<u32> = ordered.iter().map(|(_, _, id)| id.0).collect();
            let floor = embedding.next_edge;
            let expected: Vec<u32> = (floor..).take(ids.len()).collect();
            prop_assert_eq!(ids, expected);
            let restored = resolution
                .put(&AnyInstance::Graph(view), &complement, &ResolveContext::empty())
                .expect("GetPut");
            prop_assert!(same(&restored, &source));
        }
    }
}

/// A table codec that reads its input as the one cell of one row and keeps
/// no presentation state, so the preserving defaults apply to it.
struct TableEcho(&'static str);

impl panproto_io::InstanceParser for TableEcho {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn native_repr(&self) -> panproto_io::NativeRepr {
        panproto_io::NativeRepr::Table
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<WInstance, panproto_io::ParseInstanceError> {
        Err(panproto_io::ParseInstanceError::Parse {
            protocol: self.0.to_owned(),
            message: "a table codec".to_owned(),
        })
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, panproto_io::ParseInstanceError> {
        let cell = text(&String::from_utf8_lossy(input));
        FInstance::new()
            .with_rows("cell", [(RowId(0), row(&[("text", cell)]))])
            .map_err(|error| panproto_io::ParseInstanceError::Parse {
                protocol: self.0.to_owned(),
                message: error.to_string(),
            })
    }
}

impl panproto_io::InstanceEmitter for TableEcho {
    fn protocol_name(&self) -> &str {
        self.0
    }

    fn emit_wtype(
        &self,
        _schema: &Schema,
        _instance: &WInstance,
    ) -> Result<Vec<u8>, panproto_io::EmitInstanceError> {
        Ok(Vec::new())
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, panproto_io::EmitInstanceError> {
        serde_json::to_vec(instance).map_err(|error| panproto_io::EmitInstanceError::Emit {
            protocol: self.0.to_owned(),
            message: error.to_string(),
        })
    }
}

impl panproto_io::registry::ProtocolCodec for TableEcho {}

#[test]
fn a_codec_without_presentation_state_preserves_by_parsing_canonically() {
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register(TableEcho("echo"));
    let schema = Schema::default();
    let (table, presentation) = registry
        .parse_functor_preserving("echo", &schema, b"abc")
        .expect("the default parses canonically");
    let canonical = panproto_io::InstanceParser::parse_functor(&TableEcho("echo"), &schema, b"abc")
        .expect("parses");
    assert_eq!(
        serde_json::to_value(&table).expect("serializes"),
        serde_json::to_value(&canonical).expect("serializes")
    );
    assert!(presentation.is_none());
    let emitted = registry
        .emit_functor_preserving("echo", &schema, &table, None)
        .expect("no state emits canonically");
    assert_eq!(emitted, serde_json::to_vec(&canonical).expect("serializes"));
}

#[test]
fn a_registry_with_only_a_route_or_a_graph_codec_is_not_empty() {
    let empty = panproto_io::ProtocolRegistry::new();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);

    let (provider, _) = posts_provider((InstanceShapeKind::Graph, InstanceShapeKind::Graph));
    let mut routed = panproto_io::ProtocolRegistry::new();
    routed.register_resolution_route(TreeJson("posts"), provider);
    assert!(!routed.is_empty());
    assert_eq!(routed.len(), 1);

    let mut graphs = panproto_io::ProtocolRegistry::new();
    graphs.register_graph(GraphJson("posts"));
    assert!(!graphs.is_empty());
    assert_eq!(graphs.len(), 1);
}

/// The posts tree with post 5 holding two author fields, `[u1]` then
/// `[u2, u1]`, and post 7 holding `[u2]`.
fn posts_tree_with_two_author_sites() -> WInstance {
    let users = prop("doc", "user", "users");
    let id = prop("user", "user_id", "id");
    let posts = prop("doc", "post", "posts");
    let author = prop("post", "author", "author");
    let authors = |names: &[&str]| {
        Value::List(
            names
                .iter()
                .map(|name| Value::Str((*name).into()))
                .collect(),
        )
    };
    tree(
        vec![
            Node::new(0, "doc"),
            Node::new(1, "user"),
            leaf(2, "user_id", Value::Str("u1".into())),
            Node::new(3, "user"),
            leaf(4, "user_id", Value::Str("u2".into())),
            Node::new(5, "post"),
            leaf(6, "author", authors(&["u1"])),
            leaf(9, "author", authors(&["u2", "u1"])),
            Node::new(7, "post"),
            leaf(8, "author", authors(&["u2"])),
        ],
        vec![
            (0, 1, users.clone()),
            (1, 2, id.clone()),
            (0, 3, users),
            (3, 4, id),
            (0, 5, posts.clone()),
            (5, 6, author.clone()),
            (5, 9, author.clone()),
            (0, 7, posts),
            (7, 8, author),
        ],
        0,
    )
}

#[test]
fn removing_a_later_occurrence_takes_it_from_its_own_site() {
    let (resolution, _) = posts_resolution(|reference| {
        reference.cardinality = Cardinality::Many;
        reference.mode = ResolveMode::Redirect;
    });
    let source = AnyInstance::Tree(posts_tree_with_two_author_sites());
    let (view, complement) = get(&resolution, &source);
    let last = complement
        .occurrences
        .iter()
        .find_map(|(carrier, records)| match (carrier, records.as_slice()) {
            (ViewRef::Edge(id), [record])
                if record.holder == ViewRef::Node(5) && record.index == 2 =>
            {
                Some(*id)
            }
            _ => None,
        })
        .expect("post 5's third occurrence");
    let mut removed = view;
    removed
        .remove_edge(last)
        .expect("remove the third occurrence");
    let restored = put_tree(&resolution, &removed, &complement).expect("admitted");
    let value = |id: u32| restored.nodes.get(&id).and_then(|node| node.value.clone());
    assert_eq!(
        value(6),
        Some(FieldPresence::Present(Value::List(vec![Value::Str(
            "u1".into()
        )])))
    );
    assert_eq!(
        value(9),
        Some(FieldPresence::Present(Value::List(vec![Value::Str(
            "u2".into()
        )])))
    );
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn a_table_view_node_that_is_more_than_a_row_has_no_placement() {
    let (resolution, fixture) = orders(false);
    let source = AnyInstance::Table(orders_table(&fixture, text("a"), text("b")));
    let (view, complement) = get(&resolution, &source);
    for (edit, name) in [
        (
            Box::new(|node: &mut Node| node.value = Some(FieldPresence::Present(text("v"))))
                as Box<dyn Fn(&mut Node)>,
            "a value",
        ),
        (
            Box::new(|node: &mut Node| node.position = Some(0)),
            "a position",
        ),
        (
            Box::new(|node: &mut Node| node.discriminator = Some("d".into())),
            "a discriminator",
        ),
    ] {
        let mut edited = view.clone();
        edit(edited.node_mut(0).expect("customer 0"));
        let violations = resolution
            .check_put_domain(
                &AnyInstance::Graph(edited),
                &complement,
                &ResolveContext::empty(),
            )
            .expect_err(name);
        assert!(
            violations.contains(&ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(0)
            }),
            "{name}: {violations:?}"
        );
    }
}

#[test]
fn canonical_put_of_a_table_view_reads_its_rows_and_pairs() {
    let (resolution, fixture) = orders(false);
    let source = AnyInstance::Table(orders_table(&fixture, text("a"), text("b")));
    let (view, _) = get(&resolution, &source);
    let (restored, complement) = resolution
        .put_canonical(&AnyInstance::Graph(view.clone()), &ctx())
        .expect("a canonical table");
    assert!(same(&restored, &source));
    let restored_again = resolution
        .put(&AnyInstance::Graph(view), &complement, &ctx())
        .expect("the stamped complement puts the view");
    assert!(same(&restored_again, &source));
}

fn one_note_with_a_span() -> (Resolution, AnyInstance) {
    let resolution = notes(&[Standoff::local("mention", OffsetUnit::Byte)], &[]);
    let source = AnyInstance::Tree(notes_tree(vec![note(
        1,
        &[("text", text("hello")), ("start", int(0)), ("end", int(2))],
    )]));
    (resolution, source)
}

#[test]
fn canonical_put_keeps_a_span_node_and_refuses_a_stray_node() {
    let (resolution, source) = one_note_with_a_span();
    let (view, complement) = get(&resolution, &source);
    assert_eq!(complement.span_nodes.len(), 1);
    let (restored, _) = resolution
        .put_canonical(&AnyInstance::Graph(view.clone()), &ctx())
        .expect("the span node is placed as a span");
    assert!(same(&restored, &source));

    let mut stray = view;
    stray
        .insert_node(Node::new(50, "note"))
        .expect("a note with no parent");
    assert!(matches!(
        resolution.put_canonical(&AnyInstance::Graph(stray), &ctx()),
        Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(50)
        }))
    ));
}

#[test]
fn a_node_of_another_vertex_under_a_span_edge_is_not_a_span() {
    let (resolution, source) = one_note_with_a_span();
    let (view, complement) = get(&resolution, &source);
    let span_edge = complement
        .occurrences
        .keys()
        .find_map(|carrier| match carrier {
            ViewRef::Edge(id) => Some(*id),
            _ => None,
        })
        .expect("the span's occurrence edge");
    let mut edited = view;
    let mut edge = edited.edge(span_edge).expect("an occurrence edge").clone();
    edited
        .insert_node(Node::new(50, "note"))
        .expect("a note in the span's place");
    edge.tgt = 50;
    edited
        .insert_edge(panproto_inst::EdgeId(60), edge)
        .expect("a span edge into the note");
    assert!(matches!(
        resolution.put_canonical(&AnyInstance::Graph(edited), &ctx()),
        Err(ResolveError::PutDomain(ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(50)
        }))
    ));
}

// ---------------------------------------------------------------------------
// Fans: the posts fixture with a hyper-edge over the document, whose fans
// name posts and users.
// ---------------------------------------------------------------------------

fn fanned_posts_resolution(mode: ResolveMode) -> Resolution {
    let mut posts = posts_schema(|reference| reference.mode = mode);
    posts.schema.hyper_edges.insert(
        "rel".into(),
        panproto_schema::HyperEdge {
            id: "rel".into(),
            kind: "rel".into(),
            signature: [
                (Name::from("p"), Name::from("doc")),
                (Name::from("post"), Name::from("post")),
                (Name::from("user"), Name::from("user")),
            ]
            .into_iter()
            .collect(),
            parent_label: "p".into(),
        },
    );
    compile(
        "posts",
        ResolutionKind::TreeToGraph,
        &posts.schema,
        &[(&posts.author_ref, "post_author")],
    )
}

/// The posts tree with fans over the document naming `children`, each a
/// `(label, node)` pair, in order.
fn fanned_posts_tree(children: &[(&str, u32)]) -> WInstance {
    let plain = posts_tree();
    let fans = children
        .iter()
        .map(|(label, node)| panproto_inst::Fan::new("rel", 0).with_child(*label, *node))
        .collect();
    WInstance::new(plain.nodes, plain.arcs, fans, plain.root, plain.schema_root)
}

#[test]
fn fans_survive_get_and_put_of_added_references() {
    let resolution = fanned_posts_resolution(ResolveMode::AddEdge);
    let source = AnyInstance::Tree(fanned_posts_tree(&[("post", 5), ("user", 1), ("post", 7)]));
    let (view, complement) = get(&resolution, &source);
    let restored = put_tree(&resolution, &view, &complement).expect("GetPut");
    assert!(same(&AnyInstance::Tree(restored), &source));
    laws_hold(&resolution, &source, ResolveOptions::default());
}

#[test]
fn malformed_fan_exact_basis_stops_before_a_later_fan_cascade() {
    let resolution = fanned_posts_resolution(ResolveMode::AddEdge);
    let source = AnyInstance::Tree(fanned_posts_tree(&[("post", 5), ("user", 1), ("post", 7)]));
    let report = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx())
        .expect("the last-fan certificate is exact");
    let family = report
        .ran
        .iter()
        .find(|family| family.mutation == Mutation::MalformedFan)
        .expect("a fanned view has one conservative malformed witness");
    assert_eq!((family.negative, family.negative_ok), (1, 1));
}

#[test]
fn fans_of_redirected_posts_return_to_their_slots() {
    let resolution = fanned_posts_resolution(ResolveMode::Redirect);
    let source = AnyInstance::Tree(fanned_posts_tree(&[("user", 1), ("post", 5), ("post", 7)]));
    let (view, complement) = get(&resolution, &source);
    assert_eq!(complement.redirected.len(), 2);
    assert!(
        complement
            .redirected
            .values()
            .all(|fragment| fragment.fans.len() == 1),
        "each redirected post keeps the fan that names it"
    );
    let restored = put_tree(&resolution, &view, &complement).expect("GetPut");
    assert!(same(&AnyInstance::Tree(restored), &source));
    laws_hold(&resolution, &source, ResolveOptions::default());
}

/// Whether two trees are one tree under the instance identity, which
/// writes fans in their canonical record order and so ignores the order a
/// tree lists them in.
fn same_identity(a: &WInstance, b: &WInstance) -> bool {
    panproto_inst::segment::canonical_winstance_bytes(a)
        == panproto_inst::segment::canonical_winstance_bytes(b)
}

#[test]
fn fragment_fans_out_of_root_order_round_trip_up_to_fan_order() {
    let resolution = fanned_posts_resolution(ResolveMode::Redirect);
    let source = fanned_posts_tree(&[("post", 7), ("user", 1), ("post", 5)]);
    let any = AnyInstance::Tree(source.clone());
    let (view, complement) = get(&resolution, &any);
    for fragment in complement.redirected.values() {
        let records: Vec<panproto_inst::FanRecord> = fragment
            .fans
            .iter()
            .map(panproto_inst::FanRecord::of)
            .collect();
        assert!(
            records.windows(2).all(|pair| pair[0] <= pair[1]),
            "a fragment lists its fans in canonical record order"
        );
    }
    let restored = put_tree(&resolution, &view, &complement).expect("GetPut");
    assert!(same_identity(&restored, &source));
    laws_hold(&resolution, &any, ResolveOptions::default());
}

/// The redirecting posts resolution with a second hyper-edge whose fans
/// name two posts.
fn paired_posts_resolution() -> Resolution {
    let mut posts = posts_schema(|reference| reference.mode = ResolveMode::Redirect);
    posts.schema.hyper_edges.insert(
        "pair".into(),
        panproto_schema::HyperEdge {
            id: "pair".into(),
            kind: "pair".into(),
            signature: [
                (Name::from("p"), Name::from("doc")),
                (Name::from("a"), Name::from("post")),
                (Name::from("b"), Name::from("post")),
            ]
            .into_iter()
            .collect(),
            parent_label: "p".into(),
        },
    );
    compile(
        "posts",
        ResolutionKind::TreeToGraph,
        &posts.schema,
        &[(&posts.author_ref, "post_author")],
    )
}

#[test]
fn a_fan_naming_two_redirected_holders_is_kept_once() {
    let resolution = paired_posts_resolution();
    let plain = posts_tree();
    let fan = panproto_inst::Fan::new("pair", 0)
        .with_child("a", 5)
        .with_child("b", 7);
    let source = WInstance::new(
        plain.nodes,
        plain.arcs,
        vec![fan],
        plain.root,
        plain.schema_root,
    );
    let any = AnyInstance::Tree(source.clone());
    let (view, complement) = get(&resolution, &any);
    let kept: usize = complement
        .redirected
        .values()
        .map(|fragment| fragment.fans.len())
        .sum();
    assert_eq!(kept, 1, "the fan is held by the least fragment it touches");
    assert_eq!(complement.redirected[&5].fans.len(), 1);
    let restored = put_tree(&resolution, &view, &complement).expect("GetPut");
    assert!(same_identity(&restored, &source));
    laws_hold(&resolution, &any, ResolveOptions::default());
    // Removing the second post's alias removes the post, and with it the
    // fan, which can no longer name both posts.
    let mut edited = view;
    edited
        .remove_edge(occurrence_edge(&complement, 7))
        .expect("remove the second post's reference");
    let restored = put_tree(&resolution, &edited, &complement).expect("admitted");
    assert!(!restored.nodes.contains_key(&7));
    assert!(restored.fans.is_empty());
}

#[test]
fn negative_families_are_refused_for_their_intended_reason() {
    for (resolution, source) in [
        (posts_resolution(|_| {}).0, AnyInstance::Tree(posts_tree())),
        (
            fanned_posts_resolution(ResolveMode::AddEdge),
            AnyInstance::Tree(fanned_posts_tree(&[("post", 5), ("user", 1)])),
        ),
    ] {
        let report = check_resolution_laws(
            &resolution,
            &source,
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .expect("the laws hold");
        for family in &report.ran {
            assert_eq!(family.negative, family.negative_ok, "{family:?}");
            assert_eq!(family.in_domain, family.in_domain_ok, "{family:?}");
        }
        let count = |mutation: Mutation| {
            report
                .ran
                .iter()
                .find(|family| family.mutation == mutation)
                .map_or(0, |family| {
                    usize::try_from(family.negative).unwrap_or(usize::MAX)
                })
        };
        let (view, complement) = get(&resolution, &source);
        let fan_parents = view
            .edges_sorted()
            .filter(|(_, edge)| edge.edge.kind.as_ref() == panproto_inst::HYPER_PARENT_KIND)
            .count();
        assert_eq!(
            count(Mutation::RemoveSynthesized),
            complement.occurrences.len()
        );
        assert_eq!(
            count(Mutation::EditSynthesized),
            complement.occurrences.len()
        );
        assert_eq!(
            count(Mutation::MalformedFan),
            usize::from(fan_parents > 0),
            "only the last fan has a refusal independent of later rank shifts"
        );
        assert!(
            report.ran.iter().any(|family| family.in_domain > 0),
            "some single edit is in the put domain: {:?}",
            report.ran
        );
    }
}

#[test]
fn a_violation_sorts_by_carrier_then_name_then_variant() {
    let edge = ViewRef::Edge(panproto_inst::EdgeId(4));
    let name = Name::from("r");
    assert_eq!(
        ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(1)
        }
        .sort_key(),
        (Some(&ViewRef::Node(1)), None, 0)
    );
    assert_eq!(
        ResolveDomainError::AmbiguousPlacement {
            carrier: edge.clone(),
            references: vec![name.clone(), Name::from("s")],
        }
        .sort_key(),
        (Some(&edge), Some(&name), 1)
    );
    assert_eq!(
        ResolveDomainError::UnrepresentableReference {
            carrier: edge.clone(),
            reference: name.clone(),
        }
        .sort_key(),
        (Some(&edge), Some(&name), 2)
    );
    assert_eq!(
        ResolveDomainError::ContextMismatch { at: None }.sort_key(),
        (None, None, 6)
    );
    let production = Name::from("p");
    assert_eq!(
        ResolveDomainError::Assembly {
            at: Some(edge.clone()),
            production: Some(production.clone()),
            reason: panproto_io::resolution::AssemblyDomainReason::MissingProduction,
        }
        .sort_key(),
        (Some(&edge), Some(&production), 7)
    );
}

#[test]
fn an_overlay_equal_to_the_schema_reference_is_not_a_conflict() {
    let posts = posts_schema(|_| {});
    let existing = posts
        .schema
        .declarations
        .references
        .get("post_author")
        .expect("the schema declares post_author")
        .clone();
    let mut spec = spec_for(
        "posts",
        ResolutionKind::TreeToGraph,
        &posts.schema,
        &[(&posts.author_ref, "post_author")],
        &[],
    );
    spec.protocol_references
        .insert("post_author".into(), existing.clone());
    let posts_protocol = protocol("posts", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let provider = Provider {
        id: provider_id("posts"),
        protocol: posts_protocol.clone(),
        kind: ResolutionKind::TreeToGraph,
        spec: spec.clone(),
        paths: BTreeMap::new(),
        context: None,
    };
    let resolution = Resolution::for_protocol(&posts_protocol, &posts.schema, Arc::new(provider))
        .expect("an equal overlay merges");
    assert_eq!(resolution.references()["post_author"], existing);

    let mut different = existing;
    different.cardinality = Cardinality::Many;
    spec.protocol_references
        .insert("post_author".into(), different);
    let provider = Provider {
        id: provider_id("posts"),
        protocol: posts_protocol.clone(),
        kind: ResolutionKind::TreeToGraph,
        spec,
        paths: BTreeMap::new(),
        context: None,
    };
    assert!(matches!(
        Resolution::for_protocol(&posts_protocol, &posts.schema, Arc::new(provider)),
        Err(ResolveError::ReferenceConflict { reference }) if reference == "post_author"
    ));
}

#[test]
fn unordered_or_foreign_path_candidates_are_a_provider_failure() {
    let resolution = links(ResolveMode::AddEdge, OnUnresolved::Complement);
    for (literal, failing) in [("raw:2,1", true), ("raw:1,99", true), ("raw:1,2", false)] {
        let outcome = resolution.get(
            &AnyInstance::Tree(links_tree(literal)),
            &ResolveOptions::default(),
            &ctx(),
        );
        let failed = matches!(
            &outcome,
            Err(ResolveError::ProviderFailure { code, .. }) if code == "path_candidates"
        );
        assert_eq!(failed, failing, "{literal}: {outcome:?}");
    }
}

// ---------------------------------------------------------------------------
// Positions: items keyed by their index among the document's items, and
// pointers holding an index.
// ---------------------------------------------------------------------------

fn positions(mode: ResolveMode) -> Resolution {
    let base = protocol("slots", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let points_to = edge("pointer", "item", "ref", Some("points_to"));
    let mut pointer = reference(
        "pointer",
        "pointer",
        RefSource::KeyValue {
            parts: vec![Vec::new()],
        },
        "item",
        points_to.clone(),
    );
    pointer.mode = mode;
    let mut schema = SchemaBuilder::new(&base)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("item", "object", None)
        .unwrap()
        .vertex("pointer", "integer", None)
        .unwrap()
        .edge("doc", "item", "prop", Some("items"))
        .unwrap()
        .edge("doc", "pointer", "prop", Some("pointers"))
        .unwrap()
        .edge("pointer", "item", "ref", Some("points_to"))
        .unwrap()
        .entry("doc")
        .key(
            "item",
            key(
                "index",
                vec![KeyPart::Position {
                    within: prop("doc", "item", "items"),
                }],
                KeyRole::Primary,
                KeyScope::Document,
            ),
        )
        .reference(pointer)
        .build()
        .unwrap();
    schema.orderings.insert(prop("doc", "item", "items"), 0);
    compile(
        "slots",
        ResolutionKind::TreeToGraph,
        &schema,
        &[(&points_to, "pointer")],
    )
}

fn positions_tree(pointer: i64) -> WInstance {
    tree(
        vec![
            Node::new(0, "doc"),
            Node::new(1, "item"),
            Node::new(2, "item"),
            Node::new(3, "item"),
            leaf(4, "pointer", Value::Int(pointer)),
        ],
        vec![
            (0, 1, prop("doc", "item", "items")),
            (0, 2, prop("doc", "item", "items")),
            (0, 4, prop("doc", "pointer", "pointers")),
            (0, 3, prop("doc", "item", "items")),
        ],
        0,
    )
}

#[test]
fn a_position_key_is_the_index_among_the_ordered_edge_targets() {
    let resolution = positions(ResolveMode::AddEdge);
    for (pointer, target) in [(0, 1), (1, 2), (2, 3)] {
        let source = AnyInstance::Tree(positions_tree(pointer));
        let (view, _) = get(&resolution, &source);
        let into: Vec<_> = edge_triples(&view)
            .into_iter()
            .filter(|(_, _, label)| label == "points_to")
            .collect();
        assert_eq!(into, vec![(4, target, "points_to".to_owned())], "{pointer}");
        laws_hold(&resolution, &source, ResolveOptions::default());
    }
}

// ---------------------------------------------------------------------------
// The picks fixture and the reference model of PutTwice: customers a, b and
// c are rows 0, 1 and 2 keyed by their `cid` cell; the pick of customer t is
// row 10 + t, whose `customer` cell names t. A source is the set of picked
// customers, so the holders' order is the targets' order and get allocates
// the occurrence edges in target order.
// ---------------------------------------------------------------------------

/// The picks resolution and its reference edge.
fn picks() -> (Resolution, Edge) {
    let base = protocol("picks", InstanceShapeKind::Table, InstanceShapeKind::Graph);
    let picked = edge("pick", "customer", "ref", Some("picked"));
    let pick = reference(
        "pick",
        "pick",
        RefSource::KeyValue {
            parts: vec![vec![prop("pick", "label", "customer")]],
        },
        "customer",
        picked.clone(),
    );
    let schema = SchemaBuilder::new(&base)
        .vertex("customer", "table", None)
        .unwrap()
        .vertex("pick", "table", None)
        .unwrap()
        .vertex("label", "string", None)
        .unwrap()
        .edge("customer", "label", "prop", Some("cid"))
        .unwrap()
        .edge("pick", "label", "prop", Some("customer"))
        .unwrap()
        .edge("pick", "customer", "ref", Some("picked"))
        .unwrap()
        .key(
            "customer",
            key(
                "cid",
                vec![KeyPart::Path(vec![prop("customer", "label", "cid")])],
                KeyRole::Primary,
                KeyScope::Document,
            ),
        )
        .reference(pick)
        .build()
        .unwrap();
    let resolution = compile(
        "picks",
        ResolutionKind::TableToGraph,
        &schema,
        &[(&picked, "pick")],
    );
    (resolution, picked)
}

const CUSTOMERS: [&str; 3] = ["a", "b", "c"];

/// The pick row of customer `target`.
const fn pick_row(target: u8) -> u32 {
    10 + target as u32
}

/// The picks table of a set of customers.
fn picks_table(picked: &BTreeSet<u8>) -> FInstance {
    FInstance::new()
        .with_rows(
            "customer",
            (0u8..3).map(|target| {
                (
                    RowId(u32::from(target)),
                    row(&[("cid", text(CUSTOMERS[usize::from(target)]))]),
                )
            }),
        )
        .expect("customers")
        .with_rows(
            "pick",
            picked.iter().map(|target| {
                (
                    RowId(pick_row(*target)),
                    row(&[("customer", text(CUSTOMERS[usize::from(*target)]))]),
                )
            }),
        )
        .expect("picks")
}

/// The pick row node of customer `target`, as a view holds it.
fn pick_node(target: u8) -> Node {
    let mut node = Node::new(pick_row(target), "pick");
    node.extra_fields
        .insert("customer".to_owned(), text(CUSTOMERS[usize::from(target)]));
    node
}

/// The reference model of the picks fixture, written from Spec ruling S8's
/// text and calling nothing of the engine: a source is a set of targets;
/// get lists them in ascending order and gives the i-th the synthesized id
/// i, recording each id's target in the complement's index; put admits a
/// view only when every id the index records names the index's target
/// (otherwise the synthesized carrier was edited apart from its key,
/// `SynthesizedEdited`) and no target is named twice, and returns the set
/// of targets the view names.
mod model {
    use std::collections::{BTreeMap, BTreeSet};

    pub type Source = BTreeSet<u8>;
    pub type View = BTreeMap<u32, u8>;
    pub type Index = BTreeMap<u32, u8>;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Refusal {
        SynthesizedEdited(u32),
        Repeated(u8),
    }

    pub fn get(source: &Source) -> (View, Index) {
        let view: View = (0u32..).zip(source.iter().copied()).collect();
        (view.clone(), view)
    }

    pub fn put(view: &View, index: &Index) -> Result<Source, Refusal> {
        for (id, target) in view {
            if index.get(id).is_some_and(|recorded| recorded != target) {
                return Err(Refusal::SynthesizedEdited(*id));
            }
        }
        let mut source = Source::new();
        for target in view.values() {
            if !source.insert(*target) {
                return Err(Refusal::Repeated(*target));
            }
        }
        Ok(source)
    }

    /// One single-step candidate.
    #[derive(Clone, Copy, Debug)]
    pub enum Edit {
        /// A reference to an unreferenced target, with a fresh id.
        Add(u8),
        /// The reference with this id removed.
        Delete(u32),
        /// The reference with this id pointed at another target.
        Retarget(u32, u8),
    }

    pub fn candidates(view: &View) -> Vec<Edit> {
        let named: BTreeSet<u8> = view.values().copied().collect();
        let mut edits: Vec<Edit> = (0u8..3)
            .filter(|target| !named.contains(target))
            .map(Edit::Add)
            .collect();
        for (id, target) in view {
            edits.push(Edit::Delete(*id));
            for other in (0u8..3).filter(|other| other != target) {
                edits.push(Edit::Retarget(*id, other));
            }
        }
        edits
    }

    pub fn apply(view: &View, edit: Edit) -> View {
        let mut edited = view.clone();
        match edit {
            Edit::Add(target) => {
                let fresh = view.keys().max().map_or(0, |max| max + 1);
                edited.insert(fresh, target);
            }
            Edit::Delete(id) => {
                edited.remove(&id);
            }
            Edit::Retarget(id, target) => {
                edited.insert(id, target);
            }
        }
        edited
    }

    /// The index `next` rekeyed onto `view`'s ids: each id of `next` is
    /// renamed to the id `view` gives the same target (the producing
    /// index, keys being targets here).
    pub fn rekeyed(next: &Index, view: &View) -> Index {
        next.iter()
            .map(|(id, target)| {
                let own = view
                    .iter()
                    .find(|(_, named)| *named == target)
                    .map_or(*id, |(own, _)| *own);
                (own, *target)
            })
            .collect()
    }

    /// Every injective renaming of `ids` into `0..pool`.
    pub fn renamings(ids: &[u32], pool: u32) -> Vec<BTreeMap<u32, u32>> {
        let mut out = vec![BTreeMap::new()];
        for id in ids {
            out = out
                .into_iter()
                .flat_map(|partial: BTreeMap<u32, u32>| {
                    (0..pool)
                        .filter(|image| !partial.values().any(|used| used == image))
                        .map(|image| {
                            let mut next = partial.clone();
                            next.insert(*id, image);
                            next
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
        }
        out
    }

    pub fn rename(map: &BTreeMap<u32, u8>, renaming: &BTreeMap<u32, u32>) -> BTreeMap<u32, u8> {
        map.iter()
            .map(|(id, target)| (renaming.get(id).copied().unwrap_or(*id), *target))
            .collect()
    }

    /// The counts of the exhaustive run.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub struct Counts {
        pub admitted: usize,
        pub raw_put_twice_failures: usize,
        pub gated_put_twice_failures: usize,
        pub exact: usize,
    }

    /// Every source over three targets.
    pub fn sources() -> Vec<Source> {
        (0u8..8)
            .map(|bits| (0u8..3).filter(|t| bits & (1 << t) != 0).collect())
            .collect()
    }

    /// Run the five laws, `PutTwice` in both forms, over every source and
    /// candidate, panicking at the first law that fails.
    pub fn check() -> Counts {
        let mut counts = Counts::default();
        for source in sources() {
            let (view, index) = get(&source);
            assert_eq!(put(&view, &index), Ok(source.clone()), "GetPut");
            for edit in candidates(&view) {
                let candidate = apply(&view, edit);
                let verdict = put(&candidate, &index);
                let ids: Vec<u32> = candidate
                    .keys()
                    .chain(index.keys())
                    .copied()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                for renaming in renamings(&ids, 5) {
                    let expected = match &verdict {
                        Err(Refusal::SynthesizedEdited(id)) => {
                            Err(Refusal::SynthesizedEdited(renaming[id]))
                        }
                        other => other.clone(),
                    };
                    assert_eq!(
                        put(&rename(&candidate, &renaming), &rename(&index, &renaming)),
                        expected,
                        "Congruence"
                    );
                }
                // Totality: the domain is put's success, so an admitted
                // candidate's put succeeds by construction.
                let Ok(once) = verdict else {
                    continue;
                };
                counts.admitted += 1;
                let (again, next) = get(&once);
                let mut named: Vec<u8> = candidate.values().copied().collect();
                named.sort_unstable();
                assert_eq!(
                    again.values().copied().collect::<Vec<_>>(),
                    named,
                    "PutGet up to renumbering"
                );
                if again == candidate {
                    counts.exact += 1;
                }
                if put(&candidate, &next) != Ok(once.clone()) {
                    counts.raw_put_twice_failures += 1;
                }
                if put(&candidate, &rekeyed(&next, &candidate)) != Ok(once.clone()) {
                    counts.gated_put_twice_failures += 1;
                }
            }
        }
        counts
    }
}

/// The occurrence edge of each pick row in a picks view, by row.
fn pick_edges(view: &GInstance, picked: &Edge) -> BTreeMap<u32, panproto_inst::EdgeId> {
    view.edges_sorted()
        .filter(|(_, edge)| edge.edge == *picked)
        .map(|(id, edge)| (edge.src, id))
        .collect()
}

/// The production view edit of a model edit, through the order-preserving
/// bijection between the model's ids and the view's occurrence edges.
fn production_edit(
    view: &GInstance,
    model_view: &model::View,
    picked: &Edge,
    edit: model::Edit,
) -> GInstance {
    let edges: Vec<panproto_inst::EdgeId> = pick_edges(view, picked).into_values().collect();
    let of_model = |id: u32| {
        let rank = model_view
            .keys()
            .position(|own| *own == id)
            .expect("a model id of the view");
        edges[rank]
    };
    let mut edited = view.clone();
    match edit {
        model::Edit::Add(target) => {
            let fresh = view.edges().keys().map(|id| id.0 + 1).max().unwrap_or(0);
            edited.insert_node(pick_node(target)).expect("a new pick");
            edited
                .insert_edge(
                    panproto_inst::EdgeId(fresh),
                    panproto_inst::GEdge::new(pick_row(target), u32::from(target), picked.clone()),
                )
                .expect("its reference, with a fresh id");
        }
        model::Edit::Delete(id) => {
            let edge = of_model(id);
            let holder = view.edge(edge).expect("the reference").src;
            edited.remove_edge(edge).expect("remove the reference");
            edited.remove_node(holder).expect("and its pick");
        }
        model::Edit::Retarget(id, target) => {
            let edge = of_model(id);
            let holder = view.edge(edge).expect("the reference").src;
            edited
                .set_endpoints(edge, holder, u32::from(target))
                .expect("retarget the reference");
        }
    }
    edited
}

/// `next` rekeyed onto `view`'s presentation: each occurrence edge of
/// `next` renamed to the view's edge of the same pick row, which is the
/// producing index on this fixture, where each row holds one occurrence.
fn rekeyed_picks(
    next: &ResolutionComplement,
    view: &GInstance,
    picked: &Edge,
) -> ResolutionComplement {
    let edges = pick_edges(view, picked);
    let mut rekeyed = next.clone();
    rekeyed.occurrences = next
        .occurrences
        .iter()
        .map(|(carrier, records)| {
            let renamed = match (&records[0].holder, carrier) {
                (ViewRef::Row(row), ViewRef::Edge(_)) => edges
                    .get(&row.0)
                    .map_or_else(|| carrier.clone(), |edge| ViewRef::Edge(*edge)),
                _ => carrier.clone(),
            };
            (renamed, records.clone())
        })
        .collect();
    rekeyed
}

/// The occurrence edges of a view as `(source, target)` pairs, sorted: the
/// view up to the presentation ids of its synthesized edges.
fn pick_pairs(view: &GInstance, picked: &Edge) -> Vec<(u32, u32)> {
    let mut pairs: Vec<(u32, u32)> = view
        .edges_sorted()
        .filter(|(_, edge)| edge.edge == *picked)
        .map(|(_, edge)| (edge.src, edge.tgt))
        .collect();
    pairs.sort_unstable();
    pairs
}

/// The view with its occurrence edges renamed by `renaming`, and the
/// complement with its occurrence keys renamed alike.
fn renamed_picks(
    view: &GInstance,
    complement: &ResolutionComplement,
    renaming: &BTreeMap<u32, u32>,
) -> (GInstance, ResolutionComplement) {
    let mut renamed = view.clone();
    let edges: Vec<(panproto_inst::EdgeId, panproto_inst::GEdge)> = renaming
        .keys()
        .filter_map(|id| {
            let id = panproto_inst::EdgeId(*id);
            view.edge(id).cloned().map(|edge| (id, edge))
        })
        .collect();
    for (id, _) in &edges {
        renamed.remove_edge(*id).expect("a renamed edge");
    }
    for (id, edge) in edges {
        renamed
            .insert_edge(panproto_inst::EdgeId(renaming[&id.0]), edge)
            .expect("an injective renaming");
    }
    let mut complement = complement.clone();
    complement.occurrences = complement
        .occurrences
        .into_iter()
        .map(|(carrier, records)| match carrier {
            ViewRef::Edge(id) => (
                ViewRef::Edge(panproto_inst::EdgeId(
                    renaming.get(&id.0).copied().unwrap_or(id.0),
                )),
                records,
            ),
            other => (other, records),
        })
        .collect();
    (renamed, complement)
}

/// A put-domain violation with its edge carrier renamed.
fn renamed_violation(
    violation: &ResolveDomainError,
    renaming: &BTreeMap<u32, u32>,
) -> ResolveDomainError {
    let rename = |carrier: &ViewRef| match carrier {
        ViewRef::Edge(id) => ViewRef::Edge(panproto_inst::EdgeId(
            renaming.get(&id.0).copied().unwrap_or(id.0),
        )),
        other => other.clone(),
    };
    match violation {
        ResolveDomainError::SynthesizedEdited { carrier } => {
            ResolveDomainError::SynthesizedEdited {
                carrier: rename(carrier),
            }
        }
        ResolveDomainError::UnrepresentableReference { carrier, reference } => {
            ResolveDomainError::UnrepresentableReference {
                carrier: rename(carrier),
                reference: reference.clone(),
            }
        }
        ResolveDomainError::NoPlacement { carrier } => ResolveDomainError::NoPlacement {
            carrier: rename(carrier),
        },
        other => other.clone(),
    }
}

/// Congruence at one picks state: renaming the occurrence edges of the
/// view and the complement together, by every injective renaming of them
/// into five fresh ids, renames the put-domain verdict and leaves the put.
fn picks_congruence(
    resolution: &Resolution,
    picked: &Edge,
    candidate: &GInstance,
    complement: &ResolutionComplement,
    verdict: &[ResolveDomainError],
) {
    let ids: Vec<u32> = pick_edges(candidate, picked)
        .into_values()
        .chain(
            complement
                .occurrences
                .keys()
                .filter_map(|carrier| match carrier {
                    ViewRef::Edge(id) => Some(*id),
                    _ => None,
                }),
        )
        .map(|id| id.0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let floor = ids.iter().max().copied().unwrap_or(0) + 1;
    for renaming in model::renamings(&ids, 5) {
        let renaming: BTreeMap<u32, u32> = renaming
            .into_iter()
            .map(|(id, image)| (id, floor + image))
            .collect();
        let (renamed, renamed_complement) = renamed_picks(candidate, complement, &renaming);
        let found = violations_of(resolution, &renamed, &renamed_complement, &ctx());
        let expected = union_of(
            &verdict
                .iter()
                .map(|violation| renamed_violation(violation, &renaming))
                .collect::<Vec<_>>(),
            &[],
        );
        assert_eq!(found, expected, "Congruence verdicts");
        if verdict.is_empty() {
            let a = resolution
                .put(&AnyInstance::Graph(candidate.clone()), complement, &ctx())
                .expect("admitted");
            let b = resolution
                .put(&AnyInstance::Graph(renamed), &renamed_complement, &ctx())
                .expect("admitted when renamed");
            assert!(same(&a, &b), "Congruence puts");
        }
    }
}

/// `PutGet`, `Totality` and `PutTwice` at one admitted picks candidate,
/// against the model's candidate and put: whether get of the put is the
/// candidate exactly (asserted to agree with the model), whether the gated
/// second put fails, and whether the raw one fails (asserted wherever the
/// model's raw form fails).
fn picks_admitted(
    resolution: &Resolution,
    picked: &Edge,
    candidate: &GInstance,
    complement: &ResolutionComplement,
    (model_candidate, model_once): (&model::View, &model::Source),
    edit: model::Edit,
) -> (bool, bool, bool) {
    let once = resolution
        .put(&AnyInstance::Graph(candidate.clone()), complement, &ctx())
        .expect("Totality");
    let (again, next) = get(resolution, &once);
    assert_eq!(
        pick_pairs(&again, picked),
        pick_pairs(candidate, picked),
        "PutGet up to presentation, {edit:?}"
    );
    let (model_again, model_next) = model::get(model_once);
    let exact = GInstance::identical(&again, candidate);
    assert_eq!(
        exact,
        model_again == *model_candidate,
        "exact PutGet agrees, {edit:?}"
    );
    let rekeyed = rekeyed_picks(&next, candidate, picked);
    let gated_fails = !matches!(
        resolution
            .put(&AnyInstance::Graph(candidate.clone()), &rekeyed, &ctx())
            .map(|twice| same(&twice, &once)),
        Ok(true)
    );
    let raw_fails = !matches!(
        resolution
            .put(&AnyInstance::Graph(candidate.clone()), &next, &ctx())
            .map(|raw| same(&raw, &once)),
        Ok(true)
    );
    if model::put(model_candidate, &model_next).as_ref() != Ok(model_once) {
        assert!(
            raw_fails,
            "the raw form fails where the model's does, {edit:?}"
        );
    }
    (exact, gated_fails, raw_fails)
}

#[test]
fn resolution_laws_agree_with_the_model_on_three_targets() {
    let counts = model::check();
    println!(
        "model: admitted={} raw_put_twice_failures={} gated_put_twice_failures={} exact={}",
        counts.admitted,
        counts.raw_put_twice_failures,
        counts.gated_put_twice_failures,
        counts.exact
    );
    assert_eq!(
        counts,
        model::Counts {
            admitted: 24,
            raw_put_twice_failures: 6,
            gated_put_twice_failures: 0,
            exact: 14,
        }
    );
    let (resolution, picked) = picks();
    let mut production = model::Counts::default();
    let mut raw_refused = 0usize;
    for picks_set in model::sources() {
        let source = AnyInstance::Table(picks_table(&picks_set));
        check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx())
            .expect("the production gates hold on every source");
        let (view, complement) = get(&resolution, &source);
        let restored = resolution
            .put(&AnyInstance::Graph(view.clone()), &complement, &ctx())
            .expect("GetPut");
        assert!(same(&restored, &source), "GetPut");
        let (model_view, model_index) = model::get(&picks_set);
        for edit in model::candidates(&model_view) {
            let model_candidate = model::apply(&model_view, edit);
            let model_verdict = model::put(&model_candidate, &model_index);
            let candidate = production_edit(&view, &model_view, &picked, edit);
            let verdict = violations_of(&resolution, &candidate, &complement, &ctx());
            match &model_verdict {
                Ok(_) => assert!(verdict.is_empty(), "{edit:?}: {verdict:?}"),
                Err(model::Refusal::SynthesizedEdited(_)) => assert!(
                    verdict.iter().any(|violation| matches!(
                        violation,
                        ResolveDomainError::SynthesizedEdited { .. }
                    )),
                    "{edit:?}: {verdict:?}"
                ),
                Err(model::Refusal::Repeated(_)) => assert!(!verdict.is_empty(), "{edit:?}"),
            }
            picks_congruence(&resolution, &picked, &candidate, &complement, &verdict);
            if !verdict.is_empty() {
                continue;
            }
            production.admitted += 1;
            let model_once = model_verdict.expect("the model admits it too");
            let (exact, gated_fails, raw_fails) = picks_admitted(
                &resolution,
                &picked,
                &candidate,
                &complement,
                (&model_candidate, &model_once),
                edit,
            );
            production.exact += usize::from(exact);
            production.gated_put_twice_failures += usize::from(gated_fails);
            raw_refused += usize::from(raw_fails);
        }
    }
    println!(
        "production: admitted={} gated_put_twice_failures={} exact={} raw_put_twice_failures={}",
        production.admitted, production.gated_put_twice_failures, production.exact, raw_refused
    );
    assert_eq!(production.admitted, 24);
    assert_eq!(production.exact, 14);
    assert_eq!(production.gated_put_twice_failures, 0);
}

#[test]
fn put_twice_is_checked_where_put_get_holds_up_to_presentation() {
    let (resolution, picked) = picks();
    let source = AnyInstance::Table(picks_table(&BTreeSet::from([1])));
    let (view, complement) = get(&resolution, &source);
    let (model_view, _) = model::get(&BTreeSet::from([1]));
    // Add a pick of customer a: its key (row 10) sorts before the existing
    // pick's (row 11), so get re-allocates the existing occurrence's id.
    let candidate = production_edit(&view, &model_view, &picked, model::Edit::Add(0));
    let existing = pick_edges(&view, &picked)[&pick_row(1)];
    assert!(violations_of(&resolution, &candidate, &complement, &ctx()).is_empty());
    let once = resolution
        .put(&AnyInstance::Graph(candidate.clone()), &complement, &ctx())
        .expect("admitted");
    let (again, next) = get(&resolution, &once);
    assert_ne!(
        pick_edges(&again, &picked)[&pick_row(1)],
        existing,
        "get re-allocated the existing occurrence's id"
    );
    assert!(!GInstance::identical(&again, &candidate));
    assert_eq!(pick_pairs(&again, &picked), pick_pairs(&candidate, &picked));
    // The raw form puts against get's complement as it is stamped: refused.
    match resolution.put(&AnyInstance::Graph(candidate.clone()), &next, &ctx()) {
        Err(ResolveError::PutDomain(ResolveDomainError::SynthesizedEdited { .. })) => {}
        other => panic!("the raw second put is refused as an edited carrier, got {other:?}"),
    }
    // The gated form puts against it rekeyed to the candidate's
    // presentation: the same source.
    let rekeyed = rekeyed_picks(&next, &candidate, &picked);
    let twice = resolution
        .put(&AnyInstance::Graph(candidate), &rekeyed, &ctx())
        .expect("the gated second put");
    assert!(same(&twice, &once));
    laws_hold(&resolution, &source, ResolveOptions::default());
    laws_hold(&resolution, &once, ResolveOptions::default());
}

// ---------------------------------------------------------------------------
// One exact negative per law gate: providers broken in one way each, so
// that the checker reports exactly the gate that law belongs to.
// ---------------------------------------------------------------------------

/// How a [`Faulty`] assembly provider breaks its put.
#[derive(Clone, Copy, Debug)]
enum Fault {
    /// Put drops the cells the view does not show: `GetPut` fails.
    DropsResidue,
    /// Put of an edited view writes every name upper-cased: `PutGet` fails.
    Garbles,
    /// Put of an edited view fails although the domain admits it: Totality
    /// fails.
    Fails,
    /// Put of an edited view, or of any view over a source it wrote,
    /// shifts every note cell: the second put moves them again, so
    /// `PutTwice` fails.
    Drifts,
}

/// The entries provider with one fault in its put.
struct Faulty {
    inner: Entries,
    fault: Fault,
}

impl Faulty {
    /// Whether `view` is the view the residue's own source assembles to.
    fn unchanged(
        &self,
        plan: &panproto_io::resolution::AssemblyPlanId,
        view: &AnyInstance,
        residue: &panproto_io::resolution::AssemblyResidue,
        ctx: &ResolveContext<'_>,
    ) -> bool {
        self.inner
            .assembly_get(plan, &residue.source, ctx)
            .is_ok_and(|(own, _)| same(&own, view))
    }
}

/// The table with every cell `cell` of every row rewritten by `edit`.
fn table_edited(table: &FInstance, edit: impl Fn(&str, &Value) -> Value) -> FInstance {
    let mut edited = table.clone();
    for id in table.row_ids() {
        if let Some(cells) = edited.row_mut(id) {
            for (cell, value) in cells.iter_mut() {
                *value = edit(cell, value);
            }
        }
    }
    edited
}

impl ResolutionProvider for Faulty {
    fn id(&self) -> ResolutionProviderId {
        self.inner.id()
    }

    fn kind(&self) -> ResolutionKind {
        self.inner.kind()
    }

    fn protocol(&self) -> &Protocol {
        self.inner.protocol()
    }

    fn spec(
        &self,
        protocol: &Protocol,
        syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        self.inner.spec(protocol, syntactic)
    }

    fn assembly_get(
        &self,
        plan: &panproto_io::resolution::AssemblyPlanId,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, panproto_io::resolution::AssemblyResidue), ResolveError> {
        self.inner.assembly_get(plan, source, ctx)
    }

    fn assembly_check_put_domain(
        &self,
        plan: &panproto_io::resolution::AssemblyPlanId,
        view: &AnyInstance,
        residue: &panproto_io::resolution::AssemblyResidue,
        ctx: &ResolveContext<'_>,
    ) -> Vec<ResolveDomainError> {
        self.inner
            .assembly_check_put_domain(plan, view, residue, ctx)
    }

    fn assembly_put(
        &self,
        plan: &panproto_io::resolution::AssemblyPlanId,
        view: &AnyInstance,
        residue: &panproto_io::resolution::AssemblyResidue,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        let put = self.inner.assembly_put(plan, view, residue, ctx)?;
        let AnyInstance::Table(table) = &put else {
            return Ok(put);
        };
        let unchanged = self.unchanged(plan, view, residue, ctx);
        let edited = match self.fault {
            Fault::DropsResidue => {
                let mut dropped = FInstance::new();
                for (id, cells) in table.rows("entry") {
                    let kept = cells
                        .iter()
                        .filter(|(cell, _)| *cell == "name")
                        .map(|(cell, value)| (cell.clone(), value.clone()))
                        .collect();
                    dropped.insert_row("entry", id, kept).expect("a row");
                }
                dropped
            }
            Fault::Garbles if !unchanged => {
                table_edited(table, |cell, value| match (cell, value) {
                    ("name", Value::Str(name)) => Value::Str(name.to_uppercase()),
                    _ => value.clone(),
                })
            }
            Fault::Fails if !unchanged => {
                return Err(ResolveError::ProviderFailure {
                    provider: self.id(),
                    code: "fails".into(),
                    at: None,
                });
            }
            Fault::Drifts => {
                let written = table
                    .row_ids()
                    .into_iter()
                    .filter_map(|id| table.row(id).and_then(|cells| cells.get("note")))
                    .any(|note| matches!(note, Value::Int(note) if *note >= 100));
                if unchanged && !written {
                    table.clone()
                } else {
                    table_edited(table, |cell, value| match (cell, value) {
                        ("note", Value::Int(note)) => Value::Int(note + 100),
                        _ => value.clone(),
                    })
                }
            }
            Fault::Garbles | Fault::Fails => table.clone(),
        };
        Ok(AnyInstance::Table(edited))
    }
}

/// A links provider whose path `#name` (or `#name/`) names the items with
/// that name, and which spells a path `#name/` exactly when the view's
/// largest edge id is odd: its put depends on presentation ids, so
/// Congruence fails.
struct Parity(Provider);

impl ResolutionProvider for Parity {
    fn id(&self) -> ResolutionProviderId {
        self.0.id()
    }

    fn kind(&self) -> ResolutionKind {
        self.0.kind()
    }

    fn protocol(&self) -> &Protocol {
        self.0.protocol()
    }

    fn spec(
        &self,
        protocol: &Protocol,
        syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        self.0.spec(protocol, syntactic)
    }

    fn resolve_path(
        &self,
        _reference: &Reference,
        _holder: ViewRef,
        literal: &Value,
        source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<PathResolution, ResolveError> {
        let (Value::Str(path), AnyInstance::Tree(tree)) = (literal, source) else {
            return Ok(PathResolution {
                candidates: Vec::new(),
                context: None,
            });
        };
        let name = path.trim_start_matches('#').trim_end_matches('/');
        let candidates = tree
            .nodes
            .values()
            .filter(|node| node.anchor == "item" && text_field(node, "name") == Some(name))
            .map(|node| ViewRef::Node(node.id))
            .collect();
        Ok(PathResolution {
            candidates,
            context: None,
        })
    }

    fn render_path(
        &self,
        _reference: &Reference,
        _holder: ViewRef,
        target: ViewRef,
        view: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<Value, ResolveError> {
        let (AnyInstance::Graph(graph), ViewRef::Node(target)) = (view, target) else {
            return Err(ResolveError::ProviderFailure {
                provider: self.id(),
                code: "no_path".into(),
                at: None,
            });
        };
        let name = graph
            .node(target)
            .and_then(|item| text_field(item, "name"))
            .ok_or_else(|| ResolveError::ProviderFailure {
                provider: self.id(),
                code: "no_path".into(),
                at: None,
            })?;
        let odd = graph.edges().keys().map(|id| id.0).max().unwrap_or(0) % 2 == 1;
        Ok(Value::Str(if odd {
            format!("#{name}/")
        } else {
            format!("#{name}")
        }))
    }
}

#[test]
fn each_law_gate_rejects_its_exact_negative() {
    use panproto_io::resolution::ResolutionLawViolation;

    let source = AnyInstance::Table(entries_table(&["a", "b"]));
    for fault in [
        Fault::DropsResidue,
        Fault::Garbles,
        Fault::Fails,
        Fault::Drifts,
    ] {
        let (inner, syntactic) = Entries::new();
        let protocol = inner.protocol.clone();
        let resolution =
            Resolution::for_protocol(&protocol, &syntactic, Arc::new(Faulty { inner, fault }))
                .expect("compiles");
        let verdict =
            check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx());
        let gate = match &verdict {
            Err(ResolutionLawViolation::GetPut { .. }) => "GetPut",
            Err(ResolutionLawViolation::PutGet { .. }) => "PutGet",
            Err(ResolutionLawViolation::Totality { .. }) => "Totality",
            Err(ResolutionLawViolation::PutTwice { .. }) => "PutTwice",
            other => panic!("{fault:?}: {other:?}"),
        };
        let expected = match fault {
            Fault::DropsResidue => "GetPut",
            Fault::Garbles => "PutGet",
            Fault::Fails => "Totality",
            Fault::Drifts => "PutTwice",
        };
        assert_eq!(gate, expected, "{fault:?}");
    }
    let fixture = links_schema(ResolveMode::Redirect, OnUnresolved::Error);
    let protocol = protocol("links", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let provider = Provider {
        id: provider_id("links"),
        protocol: protocol.clone(),
        kind: ResolutionKind::TreeToGraph,
        spec: spec_for(
            "links",
            ResolutionKind::TreeToGraph,
            &fixture.schema,
            &[(&fixture.link_to, "link"), (&fixture.points_to, "pointer")],
            &[],
        ),
        paths: BTreeMap::new(),
        context: None,
    };
    let resolution =
        Resolution::for_protocol(&protocol, &fixture.schema, Arc::new(Parity(provider)))
            .expect("compiles");
    let source = AnyInstance::Tree(links_tree("#i1"));
    let verdict = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx());
    assert!(
        matches!(verdict, Err(ResolutionLawViolation::Congruence { .. })),
        "{verdict:?}"
    );
}

// ---------------------------------------------------------------------------
// Property tests of the five laws, beside the exhaustive bounded checks:
// each property builds random sources of one fixture and runs the checker.
// ---------------------------------------------------------------------------

mod law_properties {
    use super::*;
    use proptest::prelude::*;

    /// A posts tree with the given users and post authors, users first or
    /// posts first.
    fn posts_tree_of(users: &[&str], authors: &[&str], posts_first: bool) -> WInstance {
        let mut nodes = vec![Node::new(0, "doc")];
        let mut users_part = Vec::new();
        let mut posts_part = Vec::new();
        let mut next = 1u32;
        for user in users {
            nodes.push(Node::new(next, "user"));
            nodes.push(leaf(next + 1, "user_id", text(user)));
            users_part.push((0, next, prop("doc", "user", "users")));
            users_part.push((next, next + 1, prop("user", "user_id", "id")));
            next += 2;
        }
        for author in authors {
            nodes.push(Node::new(next, "post"));
            nodes.push(leaf(next + 1, "author", text(author)));
            posts_part.push((0, next, prop("doc", "post", "posts")));
            posts_part.push((next, next + 1, prop("post", "author", "author")));
            next += 2;
        }
        let arcs = if posts_first {
            posts_part.into_iter().chain(users_part).collect()
        } else {
            users_part.into_iter().chain(posts_part).collect()
        };
        tree(nodes, arcs, 0)
    }

    fn name() -> impl Strategy<Value = &'static str> {
        prop_oneof![Just("u1"), Just("u2"), Just("u3")]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn the_laws_hold_on_random_posts_trees(
            users in proptest::collection::vec(name(), 0..4),
            authors in proptest::collection::vec(prop_oneof![name(), Just("zz")], 0..4),
            posts_first in any::<bool>(),
            redirect in any::<bool>(),
        ) {
            let mode = if redirect { ResolveMode::Redirect } else { ResolveMode::AddEdge };
            let (resolution, _) = posts_resolution(|reference| {
                reference.on_unresolved = OnUnresolved::Complement;
                reference.mode = mode;
            });
            let source = AnyInstance::Tree(posts_tree_of(&users, &authors, posts_first));
            let verdict = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx());
            prop_assert!(verdict.is_ok(), "{verdict:?}");
        }

        #[test]
        fn the_laws_hold_on_random_alias_maps(
            anchors in proptest::collection::vec(prop_oneof![Just("a"), Just("b")], 0..3),
            aliases in proptest::collection::vec(prop_oneof![Just("a"), Just("b"), Just("c")], 0..3),
            aliases_first in any::<bool>(),
        ) {
            let fixture = aliases_schema(|reference| reference.on_unresolved = OnUnresolved::Complement);
            let resolution = compile(
                "aliases",
                ResolutionKind::TreeToGraph,
                &fixture.schema,
                &[(&fixture.alias_of, "alias_ref")],
            );
            let mut all = vec![Node::new(0, "map")];
            let mut nodes_part = Vec::new();
            let mut aliases_part = Vec::new();
            let mut next = 1u32;
            for anchor in &anchors {
                all.push(anchored(next, anchor));
                all.push(leaf(next + 1, "value", text("x")));
                nodes_part.push((0, next, fixture.item.clone()));
                nodes_part.push((next, next + 1, fixture.val.clone()));
                next += 2;
            }
            for name in &aliases {
                all.push(leaf(next, "alias", text(name)));
                aliases_part.push((0, next, fixture.item_alias.clone()));
                next += 1;
            }
            let arcs = if aliases_first {
                aliases_part.into_iter().chain(nodes_part).collect()
            } else {
                nodes_part.into_iter().chain(aliases_part).collect()
            };
            let source = AnyInstance::Tree(tree(all, arcs, 0));
            let verdict = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx());
            prop_assert!(verdict.is_ok(), "{verdict:?}");
        }

        #[test]
        fn the_laws_hold_on_random_picks_tables(
            picked in proptest::collection::btree_set(0u8..3, 0..4),
        ) {
            let (resolution, _) = picks();
            let source = AnyInstance::Table(picks_table(&picked));
            let verdict = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx());
            prop_assert!(verdict.is_ok(), "{verdict:?}");
        }

        #[test]
        fn the_laws_hold_on_random_spans(
            body in "[a-c é]{0,6}",
            start in 0i64..8,
            length in 0i64..4,
            redirect in any::<bool>(),
        ) {
            let declaration = Standoff {
                mode: if redirect { ResolveMode::Redirect } else { ResolveMode::AddEdge },
                ..Standoff::local("mention", OffsetUnit::Utf8Char)
            };
            let resolution = notes(&[declaration], &[]);
            let source = AnyInstance::Tree(notes_tree(vec![note(
                1,
                &[("text", text(&body)), ("start", int(start)), ("end", int(start + length))],
            )]));
            match resolution.get(&source, &ResolveOptions::default(), &ctx()) {
                Ok(_) => {
                    let verdict = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx());
                    prop_assert!(verdict.is_ok(), "{verdict:?}");
                }
                Err(error) => prop_assert!(matches!(
                    error,
                    ResolveError::OffsetOutOfRange { .. } | ResolveError::OffsetUnitViolation { .. }
                ), "{error:?}"),
            }
        }

        #[test]
        fn the_laws_hold_on_random_catalogs(
            link in prop_oneof![Just("x"), Just("w"), Just("y"), Just("/a/x"), Just("/b/x"), Just("/b/y")],
            second in prop_oneof![Just("a"), Just("b")],
            third in proptest::option::of(prop_oneof![Just("a"), Just("b")]),
        ) {
            let (resolution, _) = catalog();
            let source = AnyInstance::Tree(catalog_tree(link, second, third));
            match resolution.get(&source, &ResolveOptions::default(), &ctx()) {
                Ok(_) => {
                    let verdict = check_resolution_laws(&resolution, &source, &ResolveOptions::default(), &ctx());
                    prop_assert!(verdict.is_ok(), "{verdict:?}");
                }
                Err(error) => prop_assert!(matches!(
                    error,
                    ResolveError::DanglingReference { .. } | ResolveError::AmbiguousReference { .. }
                ), "{error:?}"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// A TreeToTable route: a list of items assembled into a table of entries,
// reached through the table entry points of the registry.
// ---------------------------------------------------------------------------

/// The listing provider: the items of a list tree are the rows of an
/// `entry` table, row `i` holding the `i`-th item's value as its `name`.
struct Listing {
    protocol: Protocol,
    spec: ResolutionSpec,
}

impl Listing {
    fn new() -> (Self, Schema) {
        let protocol = protocol("listing", InstanceShapeKind::Tree, InstanceShapeKind::Table);
        let syntactic = SchemaBuilder::new(&protocol)
            .vertex("list", "object", None)
            .unwrap()
            .vertex("item", "string", None)
            .unwrap()
            .edge("list", "item", "prop", Some("items"))
            .unwrap()
            .entry("list")
            .build()
            .unwrap();
        let semantic = SchemaBuilder::new(&protocol)
            .vertex("entry", "table", None)
            .unwrap()
            .vertex("name", "string", None)
            .unwrap()
            .edge("entry", "name", "prop", Some("name"))
            .unwrap()
            .build()
            .unwrap();
        let spec = ResolutionSpec {
            kind: ResolutionKind::TreeToTable,
            provider: provider_id("listing"),
            semantic_schema: semantic,
            protocol_references: BTreeMap::new(),
            structural_edges: BTreeSet::new(),
            reference_writers: BTreeMap::new(),
            context_requirements: BTreeSet::new(),
            assembly: Some(panproto_io::resolution::AssemblyPlanId {
                grammar: "listing".into(),
                version: 1,
            }),
        };
        (Self { protocol, spec }, syntactic)
    }
}

/// The list tree of some item values.
fn listing_tree(values: &[&str]) -> WInstance {
    let mut nodes = vec![Node::new(0, "list")];
    let mut arcs = Vec::new();
    for (value, id) in values.iter().zip(1u32..) {
        nodes.push(leaf(id, "item", text(value)));
        arcs.push((0, id, prop("list", "item", "items")));
    }
    tree(nodes, arcs, 0)
}

impl ResolutionProvider for Listing {
    fn id(&self) -> ResolutionProviderId {
        provider_id("listing")
    }

    fn kind(&self) -> ResolutionKind {
        ResolutionKind::TreeToTable
    }

    fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    fn spec(
        &self,
        _protocol: &Protocol,
        _syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        Ok(self.spec.clone())
    }

    fn assembly_get(
        &self,
        _plan: &panproto_io::resolution::AssemblyPlanId,
        source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, panproto_io::resolution::AssemblyResidue), ResolveError> {
        let AnyInstance::Tree(list) = source else {
            return Err(ResolveError::ShapeMismatch {
                expected: InstanceShapeKind::Tree,
                found: source.shape(),
            });
        };
        let mut table = FInstance::new();
        for &item in list.children(0) {
            let name = match &list.nodes[&item].value {
                Some(FieldPresence::Present(value)) => value.clone(),
                _ => Value::Null,
            };
            table
                .insert_row("entry", RowId(item), row(&[("name", name)]))
                .map_err(|_| {
                    ResolveError::Instance(
                        panproto_io::resolution::ResolutionInstanceError::NotTable,
                    )
                })?;
        }
        Ok((
            AnyInstance::Table(table),
            panproto_io::resolution::AssemblyResidue {
                source: Box::new(source.clone()),
                trace: Vec::new(),
                context: None,
            },
        ))
    }

    fn assembly_put_canonical(
        &self,
        plan: &panproto_io::resolution::AssemblyPlanId,
        view: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, panproto_io::resolution::AssemblyResidue), ResolveError> {
        let AnyInstance::Table(table) = view else {
            return Err(ResolveError::ShapeMismatch {
                expected: InstanceShapeKind::Table,
                found: view.shape(),
            });
        };
        let mut nodes = vec![Node::new(0, "list")];
        let mut arcs = Vec::new();
        for (id, cells) in table.rows("entry") {
            nodes.push(leaf(
                id.0,
                "item",
                cells.get("name").cloned().unwrap_or(Value::Null),
            ));
            arcs.push((0, id.0, prop("list", "item", "items")));
        }
        let source = AnyInstance::Tree(tree(nodes, arcs, 0));
        let (_, residue) = self.assembly_get(plan, &source, ctx)?;
        Ok((source, residue))
    }
}

#[test]
fn a_table_route_serves_the_table_entry_points() {
    let (provider, syntactic) = Listing::new();
    let provider = Arc::new(provider);
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register_resolution_route(TreeJson("listing"), provider);
    assert!(registry.supports_table("listing"));
    assert!(
        !registry.supports_tree("listing"),
        "the route targets a table"
    );
    assert!(!registry.supports_graph("listing"));
    let bytes = serde_json::to_vec(&listing_tree(&["a", "b"])).expect("serializes");
    let table = registry
        .parse_table("listing", &syntactic, &bytes)
        .expect("parsed through the route");
    let names: Vec<Value> = table
        .rows("entry")
        .into_iter()
        .map(|(_, cells)| cells["name"].clone())
        .collect();
    assert_eq!(names, vec![text("a"), text("b")]);
    let (with_state, complement) = registry
        .parse_table_with_complement(
            "listing",
            &syntactic,
            &bytes,
            &ResolveOptions::default(),
            &ctx(),
        )
        .expect("the checked table wrapper");
    assert!(same(
        &AnyInstance::Table(with_state),
        &AnyInstance::Table(table.clone())
    ));
    assert!(complement.assembly.is_some());
    assert!(matches!(
        registry.parse_tree_with_complement(
            "listing",
            &syntactic,
            &bytes,
            &ResolveOptions::default(),
            &ctx()
        ),
        Err(panproto_io::ParseInstanceError::Resolution(error))
            if matches!(*error, ResolveError::ShapeMismatch { expected: InstanceShapeKind::Tree, found: InstanceShapeKind::Table })
    ));
    let emitted = registry
        .emit_table("listing", &syntactic, &table)
        .expect("emitted through canonical put");
    let reparsed = registry
        .parse_table("listing", &syntactic, &emitted)
        .expect("reparsed");
    assert!(same(
        &AnyInstance::Table(reparsed),
        &AnyInstance::Table(table)
    ));
    // A native table codec of the protocol is preferred over the route.
    registry.register(TableEcho("listing"));
    let native = registry
        .parse_table("listing", &syntactic, b"plain")
        .expect("the native codec");
    assert!(!native.rows("cell").is_empty());
    assert!(native.rows("entry").is_empty());
    let native_bytes = registry
        .emit_table("listing", &syntactic, &native)
        .expect("the native codec");
    assert_eq!(
        native_bytes,
        serde_json::to_vec(&native).expect("serializes")
    );
}

#[test]
fn a_provider_without_overrides_lists_no_qualified_form_and_admits_no_assembly() {
    let (resolution, _) = posts_resolution(|_| {});
    let reference = &resolution.references()["post_author"];
    assert_eq!(
        resolution
            .provider()
            .qualified_forms(
                reference,
                ViewRef::Node(5),
                ViewRef::Node(1),
                &text("u1"),
                &AnyInstance::Graph(GInstance::new()),
                &ctx(),
            )
            .expect("the default"),
        Vec::<Value>::new()
    );
    let (provider, syntactic) = Listing::new();
    let protocol = provider.protocol.clone();
    let listing =
        Resolution::for_protocol(&protocol, &syntactic, Arc::new(provider)).expect("compiles");
    let source = AnyInstance::Tree(listing_tree(&["a"]));
    let (view, complement) = listing
        .get(&source, &ResolveOptions::default(), &ctx())
        .expect("assembled");
    assert_eq!(
        listing.check_put_domain(&view, &complement, &ctx()),
        Err(vec![ResolveDomainError::Assembly {
            at: None,
            production: None,
            reason: panproto_io::resolution::AssemblyDomainReason::MissingProduction,
        }]),
        "the default domain admits nothing"
    );
}

#[test]
fn a_resolution_and_its_context_show_what_they_hold() {
    let (resolution, _) = posts_resolution(|_| {});
    let shown = format!("{resolution:?}");
    assert!(shown.starts_with("Resolution"), "{shown}");
    assert!(
        shown.contains("posts") && shown.contains("TreeToGraph"),
        "{shown}"
    );
    let spaces = Spaces::new(false);
    let with_tokens = ResolveContext {
        external_text: None,
        token_boundaries: Some(&spaces),
    };
    assert_eq!(
        format!("{with_tokens:?}"),
        "ResolveContext { external_text: false, token_boundaries: true }"
    );
    assert_eq!(
        format!("{:?}", ResolveContext::empty()),
        "ResolveContext { external_text: false, token_boundaries: false }"
    );
}

#[test]
fn a_protocol_with_neither_a_table_codec_nor_a_table_route_reads_no_table() {
    let mut registry = panproto_io::ProtocolRegistry::new();
    registry.register(TreeJson("trees"));
    assert!(matches!(
        registry.parse_table("trees", &Schema::default(), b"{}"),
        Err(panproto_io::ParseInstanceError::UnsupportedRepresentation {
            requested: panproto_io::NativeRepr::Table,
            native: panproto_io::NativeRepr::Tree,
            ..
        })
    ));
    assert!(matches!(
        registry.emit_table("trees", &Schema::default(), &FInstance::new()),
        Err(panproto_io::EmitInstanceError::UnsupportedRepresentation {
            requested: panproto_io::NativeRepr::Table,
            native: panproto_io::NativeRepr::Tree,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// Placement of edited structure: spanning arcs, promoted arcs, new holders
// and table edges, each pinned to its exact put-domain vector.
// ---------------------------------------------------------------------------

#[test]
fn a_reconnected_spanning_arc_is_not_structural() {
    let (resolution, _) = posts_resolution(|_| {});
    let (mut view, complement) = get(&resolution, &AnyInstance::Tree(posts_tree()));
    // Arc 0 is user 1's spanning arc; pointing it at user 3 leaves user 1
    // without a parent and gives user 3 a second incoming arc.
    view.set_endpoints(panproto_inst::EdgeId(0), 0, 3)
        .expect("reconnect user 1's arc to user 3");
    assert_eq!(
        violations_of(&resolution, &view, &complement, &ctx()),
        vec![
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(1)
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(2)
            },
            ResolveDomainError::UnrepresentableReference {
                carrier: ViewRef::Node(5),
                reference: "post_author".into()
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(0))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(1))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(2))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(3))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(4))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(5))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(6))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(7))
            },
        ]
    );
}

#[test]
fn the_least_new_arc_into_a_parentless_node_is_promoted() {
    // Every author dangles, so the view has no occurrence edge and a new
    // arc's slot is the next arc position, 8.
    let (resolution, _) = posts_resolution(|reference| {
        reference.on_unresolved = OnUnresolved::Complement;
        reference.nullable = true;
    });
    let source = AnyInstance::Tree(posts_tree_with(text("zz"), text("zz")));
    let (mut view, complement) = get(&resolution, &source);
    let posts = prop("doc", "post", "posts");
    view.insert_node(Node::new(100, "post"))
        .expect("a new post");
    // The arc from post 5 has the smaller id; the document's arc has the
    // least key `(label, source, id)`, so id order is not key order.
    view.insert_edge(
        panproto_inst::EdgeId(8),
        panproto_inst::GEdge::new(5, 100, posts.clone()),
    )
    .expect("an arc from post 5");
    view.insert_edge(
        panproto_inst::EdgeId(20),
        panproto_inst::GEdge::new(0, 100, posts),
    )
    .expect("an arc from the document");
    assert_eq!(
        violations_of(&resolution, &view, &complement, &ctx()),
        vec![
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(8))
            },
            ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(panproto_inst::EdgeId(20))
            },
        ]
    );
}

#[test]
fn a_new_redirect_from_a_fan_node_is_unrepresentable() {
    let resolution = fanned_posts_resolution(ResolveMode::Redirect);
    let source = AnyInstance::Tree(fanned_posts_tree(&[("user", 1)]));
    let (mut view, complement) = get(&resolution, &source);
    let fan = view
        .nodes()
        .iter()
        .find(|(_, node)| node.shape == panproto_inst::NodeShape::HyperEdge)
        .map(|(id, _)| *id)
        .expect("a fan node");
    let label = posts_schema(|_| {}).author_ref;
    view.insert_edge(
        panproto_inst::EdgeId(90),
        panproto_inst::GEdge::new(fan, 1, label),
    )
    .expect("a reference from the fan node");
    assert_eq!(
        violations_of(&resolution, &view, &complement, &ctx()),
        vec![ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Edge(panproto_inst::EdgeId(90)),
            reference: "post_author".into()
        },]
    );
}

#[test]
fn a_new_redirect_whose_key_path_has_two_hops_is_unrepresentable() {
    let base = protocol("deep", InstanceShapeKind::Tree, InstanceShapeKind::Graph);
    let label = edge("post", "user", "ref", Some("author_ref"));
    let mut author = reference(
        "post_author",
        "post",
        RefSource::KeyValue {
            parts: vec![vec![
                prop("post", "meta", "meta"),
                prop("meta", "author", "author"),
            ]],
        },
        "user",
        label.clone(),
    );
    author.mode = ResolveMode::Redirect;
    let schema = SchemaBuilder::new(&base)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("user", "object", None)
        .unwrap()
        .vertex("user_id", "string", None)
        .unwrap()
        .vertex("post", "object", None)
        .unwrap()
        .vertex("meta", "object", None)
        .unwrap()
        .vertex("author", "string", None)
        .unwrap()
        .edge("doc", "user", "prop", Some("users"))
        .unwrap()
        .edge("user", "user_id", "prop", Some("id"))
        .unwrap()
        .edge("doc", "post", "prop", Some("posts"))
        .unwrap()
        .edge("post", "meta", "prop", Some("meta"))
        .unwrap()
        .edge("meta", "author", "prop", Some("author"))
        .unwrap()
        .edge("post", "user", "ref", Some("author_ref"))
        .unwrap()
        .entry("doc")
        .key(
            "user",
            key(
                "id",
                vec![KeyPart::Path(vec![prop("user", "user_id", "id")])],
                KeyRole::Primary,
                KeyScope::Document,
            ),
        )
        .reference(author)
        .build()
        .unwrap();
    let resolution = compile(
        "deep",
        ResolutionKind::TreeToGraph,
        &schema,
        &[(&label, "post_author")],
    );
    let source = AnyInstance::Tree(tree(
        vec![
            Node::new(0, "doc"),
            Node::new(1, "user"),
            leaf(2, "user_id", text("u1")),
        ],
        vec![
            (0, 1, prop("doc", "user", "users")),
            (1, 2, prop("user", "user_id", "id")),
        ],
        0,
    ));
    let (mut view, complement) = get(&resolution, &source);
    view.insert_edge(
        panproto_inst::EdgeId(20),
        panproto_inst::GEdge::new(0, 1, label),
    )
    .expect("a new reference");
    assert_eq!(
        violations_of(&resolution, &view, &complement, &ctx()),
        vec![ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Edge(panproto_inst::EdgeId(20)),
            reference: "post_author".into()
        },]
    );
}

#[test]
fn a_table_edge_into_a_span_has_no_placement() {
    let (resolution, fixture) = orders(false);
    let mut table = orders_table(&fixture, text("a"), text("b"));
    if let Some(order) = table.row_mut(RowId(3)) {
        order.insert("left".to_owned(), text("hello"));
        order.insert("right".to_owned(), text("world"));
        order.insert("start".to_owned(), int(0));
        order.insert("end".to_owned(), int(2));
    }
    let (mut view, complement) = get(&resolution, &AnyInstance::Table(table));
    let span = *complement.span_nodes.keys().next().expect("a span node");
    view.insert_edge(
        panproto_inst::EdgeId(99),
        panproto_inst::GEdge::new(3, span, fixture.region),
    )
    .expect("a foreign-key edge into the span");
    assert_eq!(
        violations_of(&resolution, &view, &complement, &ctx()),
        vec![ResolveDomainError::NoPlacement {
            carrier: ViewRef::Edge(panproto_inst::EdgeId(99))
        },]
    );
}

#[test]
fn a_reconnect_after_a_removal_is_written_at_its_shifted_index() {
    let (resolution, _) = posts_resolution(|reference| {
        reference.cardinality = Cardinality::Many;
        reference.mode = ResolveMode::Redirect;
    });
    let source = AnyInstance::Tree(posts_tree_with(
        Value::List(vec![text("u1"), text("u2"), text("u1")]),
        Value::List(vec![text("u2")]),
    ));
    let (mut view, complement) = get(&resolution, &source);
    let at = |index: u32| {
        complement
            .occurrences
            .iter()
            .find_map(|(carrier, records)| match (carrier, records.as_slice()) {
                (ViewRef::Edge(id), [record])
                    if record.holder == ViewRef::Node(5) && record.index == index =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .expect("an occurrence of post 5")
    };
    let (first, third) = (at(0), at(2));
    view.remove_edge(first)
        .expect("remove the first occurrence");
    view.set_endpoints(third, 0, 3)
        .expect("reconnect the third occurrence to user 3");
    let restored = put_tree(&resolution, &view, &complement).expect("admitted");
    assert_eq!(
        restored.nodes[&6].value,
        Some(FieldPresence::Present(Value::List(vec![
            text("u2"),
            text("u2")
        ])))
    );
}
