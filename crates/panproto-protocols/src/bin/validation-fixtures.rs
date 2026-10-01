//! Regenerate the frozen validation corpus.
//!
//! Each fixture is one validation case: the schema, the protocol it is read
//! under (its name, and its definition when the name is not a registered
//! protocol's), the validation options, budget and external verdicts, the
//! instance, and the report validation gives. It is written three times, as
//! named `MessagePack` (`<case>.named.msgpack`), CBOR (`<case>.cbor`) and
//! JSON (`<case>.json`), and a graph instance is also written as node-link
//! JSON (`<case>.nodelink.json`). A report's diagnostics carry their
//! failure witnesses, which `Diagnostic::verify` confirms against the
//! instance. The cases are grouped by what they exercise: `anchors`,
//! `endpoints`, `values`, `cardinality`, `ordering`, `cycles`, `fans`,
//! `keys` and `incomplete`.
//!
//! Instances that construction or decoding rejects are written as
//! rejections (`<case>.rejection.*`): the instance as its wire form, and
//! the error decoding it gives.
//!
//! Built with the `fixtures` feature: `cargo run -p panproto-protocols
//! --features fixtures --bin validation-fixtures [OUT_DIR]`, where `OUT_DIR`
//! defaults to the repository's `tests/fixtures/validation`. The validation
//! fixture tests run this binary into a temporary directory and fail when
//! its output differs from the committed corpus.

use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::path::{Path, PathBuf};

use panproto_gat::Name;
use panproto_inst::validation::{
    ExternalCheckDecl, ExternalVerdict, ExternalVerdicts, Requirement, SchemaRef, SourceLocation,
    ValidationBudget, ValidationOptions, ValidationPlan, ValidationReport,
};
use panproto_inst::{
    Digest, FInstance, Fan, FieldPresence, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, Node,
    NodeShape, RowId, Value, ViewRef, WInstance, to_node_link_json,
};
use panproto_schema::{
    Cardinality, Constraint, ConstraintSemantics, Edge, HyperEdge, InstanceShapeKind, Key, KeyPart,
    KeyRole, KeyScope, LengthUnit, OnUnresolved, Protocol, RefSource, Reference, ResolveMode,
    Schema, Vertex,
};
use serde::Serialize;

/// What building a fixture may fail with.
type R<T> = Result<T, Box<dyn Error>>;

/// The instance of a fixture, in its shape.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Instance {
    /// A graph.
    Graph(GInstance),
    /// A tree.
    Tree(WInstance),
    /// A table instance.
    Table(FInstance),
}

/// One validation case as it is written to disk.
#[derive(Serialize)]
struct Fixture {
    /// The case's path in the corpus, without extension.
    name: String,
    /// The protocol's name.
    protocol: String,
    /// The protocol, when its name is not a registered protocol's.
    protocol_definition: Option<Protocol>,
    /// The schema.
    schema: Schema,
    /// The validation options.
    options: ValidationOptions,
    /// The budget.
    budget: ValidationBudget,
    /// The external verdicts.
    external: ExternalVerdicts,
    /// The instance.
    instance: Instance,
    /// The report validation gives.
    report: ValidationReport,
}

/// One instance that construction or decoding rejects, as it is written to
/// disk.
#[derive(Serialize)]
struct Rejection {
    /// The case's path in the corpus, without extension.
    name: String,
    /// The shape the instance would have.
    shape: InstanceShapeKind,
    /// The instance's wire form.
    instance: serde_json::Value,
    /// The error decoding it gives.
    error: String,
}

/// The name of the corpus's own protocols, by the shape their instances are
/// written in.
fn protocol_name(syntactic: InstanceShapeKind) -> String {
    let shape = match syntactic {
        InstanceShapeKind::Tree => "tree",
        InstanceShapeKind::Table => "table",
        InstanceShapeKind::Graph => "graph",
    };
    format!("validation-fixtures-{shape}")
}

/// The corpus's protocol for instances written in shape `syntactic`: a
/// length bound in characters, inclusive value bounds, out-degree bounds, a
/// declared acyclic self-edge, a pattern and a required value; resolved
/// instances are graphs.
fn fixture_protocol(syntactic: InstanceShapeKind) -> Protocol {
    let semantics = [
        (
            "maxLength",
            ConstraintSemantics::MaxLength(LengthUnit::Chars),
        ),
        ("minimum", ConstraintSemantics::Minimum),
        ("maximum", ConstraintSemantics::Maximum),
        ("maxOut", ConstraintSemantics::MaxOutDegree),
        ("minOut", ConstraintSemantics::MinOutDegree),
        ("acyclic", ConstraintSemantics::Acyclic),
        ("pattern", ConstraintSemantics::Pattern),
        ("required", ConstraintSemantics::RequiredValue),
    ];
    Protocol {
        name: protocol_name(syntactic),
        constraint_sorts: semantics.iter().map(|(s, _)| (*s).to_owned()).collect(),
        constraint_semantics: semantics
            .into_iter()
            .map(|(s, m)| (s.to_owned(), m))
            .collect(),
        syntactic_shape: syntactic,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    }
}

/// An edge of kind `kind` named `name`.
fn edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(name.into()),
    }
}

/// A `prop` edge named `name`.
fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    edge(src, tgt, "prop", name)
}

/// A schema under construction, its adjacency indices kept up to date.
#[derive(Default)]
struct S(Schema);

impl S {
    fn vertex(mut self, id: &str, kind: &str) -> Self {
        self.0.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
        self
    }

    fn edge(mut self, e: &Edge) -> Self {
        self.0.edges.insert(e.clone(), e.kind.clone());
        self.0
            .outgoing
            .entry(e.src.clone())
            .or_default()
            .push(e.clone());
        self.0
            .incoming
            .entry(e.tgt.clone())
            .or_default()
            .push(e.clone());
        self.0
            .between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
        self
    }

    fn constraint(mut self, vertex: &str, sort: &str, value: &str) -> Self {
        self.0
            .constraints
            .entry(vertex.into())
            .or_default()
            .push(Constraint {
                sort: sort.into(),
                value: value.into(),
            });
        self
    }

    fn required(mut self, vertex: &str, e: &Edge) -> Self {
        self.0
            .required
            .entry(vertex.into())
            .or_default()
            .push(e.clone());
        self
    }

    fn ordered(mut self, e: &Edge) -> Self {
        self.0.orderings.insert(e.clone(), 0);
        self
    }

    fn hyper_edge(mut self, id: &str, signature: &[(&str, &str)], parent: &str) -> Self {
        self.0.hyper_edges.insert(
            id.into(),
            HyperEdge {
                id: id.into(),
                kind: "relation".into(),
                signature: signature
                    .iter()
                    .map(|(l, v)| (Name::from(*l), Name::from(*v)))
                    .collect(),
                parent_label: parent.into(),
            },
        );
        self
    }

    fn key(mut self, vertex: &str, id: &str, parts: &[Edge], role: KeyRole) -> Self {
        self.0.declarations.insert_key(
            vertex.into(),
            Key {
                id: id.into(),
                parts: parts
                    .iter()
                    .map(|e| KeyPart::Path(vec![e.clone()]))
                    .collect(),
                role,
                scope: KeyScope::Global,
            },
        );
        self
    }

    fn build(self) -> Schema {
        self.0
    }
}

/// A node anchored at `anchor` with the string value `value`.
fn valued(id: u32, anchor: &str, value: Value) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(value))
}

/// A table row of the given cells.
fn row(cells: &[(&str, Value)]) -> HashMap<String, Value> {
    cells
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

/// One validation case before it is validated.
struct Case {
    name: &'static str,
    syntactic: InstanceShapeKind,
    schema: Schema,
    options: ValidationOptions,
    budget: ValidationBudget,
    external: ExternalVerdicts,
    instance: Instance,
}

impl Case {
    /// A case with no options, an unlimited budget and no verdicts, read
    /// under the corpus protocol of its instance's shape.
    fn new(name: &'static str, schema: Schema, instance: Instance) -> Self {
        let syntactic = match &instance {
            Instance::Graph(_) => InstanceShapeKind::Graph,
            Instance::Tree(_) => InstanceShapeKind::Tree,
            Instance::Table(_) => InstanceShapeKind::Table,
        };
        Self {
            name,
            syntactic,
            schema,
            options: ValidationOptions::default(),
            budget: ValidationBudget::default(),
            external: ExternalVerdicts::default(),
            instance,
        }
    }

    /// The fixture: the case and the report validation gives.
    fn validate(self) -> Fixture {
        let protocol = fixture_protocol(self.syntactic);
        let plan = ValidationPlan::compile(&self.schema, &protocol, &self.options);
        let report = match &self.instance {
            Instance::Graph(g) => plan.validate_graph(g, &self.budget, &self.external),
            Instance::Tree(t) => plan.validate_tree(t, &self.budget, &self.external),
            Instance::Table(f) => plan.validate_table(f, &self.budget, &self.external),
        };
        Fixture {
            name: self.name.to_owned(),
            protocol: protocol.name.clone(),
            protocol_definition: Some(protocol),
            schema: self.schema,
            options: self.options,
            budget: self.budget,
            external: self.external,
            instance: self.instance,
            report,
        }
    }
}

/// A graph of the given nodes and edges, edges numbered in order.
fn graph(nodes: Vec<Node>, edges: &[(u32, u32, &Edge)]) -> GInstance {
    let mut g = GInstance::new();
    for n in nodes {
        g = g.with_node(n);
    }
    for (s, t, e) in edges {
        g = g.with_edge(*s, *t, (*e).clone());
    }
    g
}

/// A tree of the given nodes and arcs, rooted at node 0, with `fans`.
fn tree(nodes: Vec<Node>, arcs: &[(u32, u32, &Edge)], fans: Vec<Fan>) -> WInstance {
    let root_anchor = nodes
        .first()
        .map_or_else(|| Name::from("doc"), |n| n.anchor.clone());
    WInstance::new(
        nodes.into_iter().map(|n| (n.id, n)).collect(),
        arcs.iter()
            .map(|(p, c, e)| (*p, *c, (*e).clone()))
            .collect(),
        fans,
        0,
        root_anchor,
    )
}

/// The probe schema: a person works at an organization, must, and has a
/// name of at most three characters.
fn probe_schema() -> Schema {
    let works_at = prop("person", "org", "worksAt");
    S::default()
        .vertex("person", "object")
        .vertex("org", "object")
        .edge(&works_at)
        .required("person", &works_at)
        .constraint("person", "maxLength", "3")
        .build()
}

/// Missing and unknown anchors, undeclared labels, in every shape.
fn anchors() -> R<Vec<Case>> {
    let works_at = prop("person", "org", "worksAt");
    let undeclared = prop("person", "org", "undeclared");
    let s = |v: &str| Value::Str(v.into());
    // Five faults (a ghost anchor, a person working at a person, an
    // undeclared label, a person working nowhere, a name too long) beside
    // two legal parallel `worksAt` edges.
    let probe = graph(
        vec![
            valued(0, "person", s("ann")),
            valued(1, "person", s("abcdefgh")),
            Node::new(2, "org"),
            valued(3, "person", s("bob")),
            Node::new(4, "ghost"),
        ],
        &[
            (0, 2, &works_at),
            (0, 2, &works_at),
            (1, 2, &works_at),
            (1, 0, &works_at),
            (0, 2, &undeclared),
        ],
    );
    let tables = S::default()
        .vertex("film", "table")
        .vertex("actor", "table")
        .vertex("title", "string")
        .edge(&prop("film", "title", "title"))
        .edge(&edge("film", "actor", "fk", "stars"))
        .build();
    let unknown_table = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[("title", s("Alien"))]))])
        .and_then(|f| f.with_rows("ghost_table", [(RowId(1), row(&[]))]))?;
    let unknown_fk_label = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[("title", s("Alien"))]))])
        .and_then(|f| f.with_rows("actor", [(RowId(1), row(&[]))]))
        .and_then(|f| {
            f.with_foreign_key(
                edge("film", "actor", "fk", "directs"),
                [(RowId(0), RowId(1))],
            )
        })?;
    let ghost_root = tree(vec![Node::new(0, "ghost")], &[], vec![]);
    Ok(vec![
        Case::new("anchors/probe", probe_schema(), Instance::Graph(probe)),
        Case::new(
            "anchors/unknown_table",
            tables.clone(),
            Instance::Table(unknown_table),
        ),
        Case::new(
            "anchors/unknown_fk_label",
            tables,
            Instance::Table(unknown_fk_label),
        ),
        Case::new(
            "anchors/tree_unknown_root_anchor",
            probe_schema(),
            Instance::Tree(ghost_root),
        ),
    ])
}

/// Incoherent endpoints: the source side, the target side, both, and a
/// self-loop on an edge that is not a self-edge; a coherent edge; a tree
/// arc.
fn endpoints() -> Vec<Case> {
    let ab = prop("a", "b", "ab");
    let schema = S::default()
        .vertex("a", "object")
        .vertex("b", "object")
        .edge(&ab)
        .build();
    let pair = |x: &str, y: &str| graph(vec![Node::new(0, x), Node::new(1, y)], &[(0, 1, &ab)]);
    vec![
        Case::new(
            "endpoints/coherent",
            schema.clone(),
            Instance::Graph(pair("a", "b")),
        ),
        Case::new(
            "endpoints/src_side",
            schema.clone(),
            Instance::Graph(pair("b", "b")),
        ),
        Case::new(
            "endpoints/tgt_side",
            schema.clone(),
            Instance::Graph(pair("a", "a")),
        ),
        Case::new(
            "endpoints/both_sides",
            schema.clone(),
            Instance::Graph(pair("b", "a")),
        ),
        Case::new(
            "endpoints/self_loop_on_non_self_edge",
            schema.clone(),
            Instance::Graph(graph(vec![Node::new(0, "a")], &[(0, 0, &ab)])),
        ),
        Case::new(
            "endpoints/tree_tgt_side",
            schema,
            Instance::Tree(tree(
                vec![Node::new(0, "a"), Node::new(1, "a")],
                &[(0, 1, &ab)],
                vec![],
            )),
        ),
    ]
}

/// Values and attributes: a value breaking its bound, an undeclared
/// attribute and a mistyped one, in each shape.
fn values() -> R<Vec<Case>> {
    let name = prop("item", "name", "name");
    let schema = S::default()
        .vertex("item", "object")
        .vertex("name", "string")
        .edge(&name)
        .constraint("name", "maxLength", "3")
        .build();
    let s = |v: &str| Value::Str(v.into());
    let mut item = Node::new(0, "item");
    item.extra_fields.insert("name".into(), Value::Int(5));
    item.extra_fields.insert("colour".into(), s("red"));
    let graph_case = graph(
        vec![item.clone(), valued(1, "name", s("toolong"))],
        &[(0, 1, &name)],
    );
    let tree_case = tree(
        vec![item, valued(1, "name", s("toolong"))],
        &[(0, 1, &name)],
        vec![],
    );
    let table_schema = S::default()
        .vertex("item", "table")
        .vertex("name", "string")
        .edge(&name)
        .constraint("name", "maxLength", "3")
        .build();
    let table_case = FInstance::new().with_rows(
        "item",
        [
            (
                RowId(0),
                row(&[("name", s("toolong")), ("colour", s("red"))]),
            ),
            (RowId(1), row(&[("name", Value::Int(5))])),
        ],
    )?;
    Ok(vec![
        Case::new(
            "values/graph_values_and_attributes",
            schema.clone(),
            Instance::Graph(graph_case),
        ),
        Case::new(
            "values/tree_values_and_attributes",
            schema,
            Instance::Tree(tree_case),
        ),
        Case::new(
            "values/table_values_and_attributes",
            table_schema,
            Instance::Table(table_case),
        ),
    ])
}

/// Required edges present and absent, out-degree bounds at and past the
/// boundary, parallel edges counted, and a required foreign key.
fn cardinality() -> R<Vec<Case>> {
    let child = prop("parent", "child", "child");
    let base = || {
        S::default()
            .vertex("parent", "object")
            .vertex("child", "object")
            .edge(&child)
    };
    let required = base().required("parent", &child).build();
    let at_most = |n: &str| base().constraint("parent", "maxOut", n).build();
    let at_least = base().constraint("parent", "minOut", "1").build();
    let fan_out = |k: u32| {
        let mut nodes = vec![Node::new(0, "parent")];
        nodes.extend((1..=k).map(|i| Node::new(i, "child")));
        let edges: Vec<(u32, u32, &Edge)> = (1..=k).map(|i| (0, i, &child)).collect();
        graph(nodes, &edges)
    };
    let parallel = graph(
        vec![Node::new(0, "parent"), Node::new(1, "child")],
        &[(0, 1, &child), (0, 1, &child)],
    );
    let stars = edge("film", "actor", "fk", "stars");
    let films = S::default()
        .vertex("film", "table")
        .vertex("actor", "table")
        .edge(&stars)
        .required("film", &stars)
        .build();
    let unstarred = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[])), (RowId(1), row(&[]))])
        .and_then(|f| f.with_rows("actor", [(RowId(2), row(&[]))]))
        .and_then(|f| f.with_foreign_key(stars.clone(), [(RowId(0), RowId(2))]))?;
    Ok(vec![
        Case::new(
            "cardinality/required_edge_present",
            required.clone(),
            Instance::Graph(fan_out(1)),
        ),
        Case::new(
            "cardinality/required_edge_absent",
            required,
            Instance::Graph(fan_out(0)),
        ),
        Case::new(
            "cardinality/degree_at_upper_bound",
            at_most("2"),
            Instance::Graph(fan_out(2)),
        ),
        Case::new(
            "cardinality/degree_above_upper_bound",
            at_most("2"),
            Instance::Graph(fan_out(3)),
        ),
        Case::new(
            "cardinality/degree_at_lower_bound",
            at_least.clone(),
            Instance::Graph(fan_out(1)),
        ),
        Case::new(
            "cardinality/degree_below_lower_bound",
            at_least,
            Instance::Graph(fan_out(0)),
        ),
        Case::new(
            "cardinality/parallel_edges_counted",
            at_most("1"),
            Instance::Graph(parallel),
        ),
        Case::new(
            "cardinality/table_required_foreign_key_absent",
            films,
            Instance::Table(unstarred),
        ),
    ])
}

/// Positions of an ordered edge: two out-edges sharing one, absent
/// positions, distinct positions.
fn ordering() -> R<Vec<Case>> {
    let items = prop("list", "item", "items");
    let schema = S::default()
        .vertex("list", "object")
        .vertex("item", "object")
        .edge(&items)
        .ordered(&items)
        .build();
    let positioned = |p: [Option<u32>; 2]| -> R<GInstance> {
        let mut g = graph(
            vec![
                Node::new(0, "list"),
                Node::new(1, "item"),
                Node::new(2, "item"),
            ],
            &[(0, 1, &items), (0, 2, &items)],
        );
        let ids: Vec<_> = g.edges_sorted().map(|(id, _)| id).collect();
        for (id, position) in ids.into_iter().zip(p) {
            g.set_edge_position(id, position)?;
        }
        Ok(g)
    };
    Ok(vec![
        Case::new(
            "ordering/duplicate_positions",
            schema.clone(),
            Instance::Graph(positioned([Some(0), Some(0)])?),
        ),
        Case::new(
            "ordering/absent_positions_allowed",
            schema.clone(),
            Instance::Graph(positioned([None, None])?),
        ),
        Case::new(
            "ordering/distinct_positions",
            schema,
            Instance::Graph(positioned([Some(0), Some(1)])?),
        ),
    ])
}

/// Cycles of a self-edge: legal without a declaration; with `Acyclic`, a
/// two-cycle, a longer cycle and a self-loop fail and a chain passes.
fn cycles() -> Vec<Case> {
    let next = prop("item", "item", "next");
    let base = S::default().vertex("item", "object").edge(&next);
    let free = base.build();
    let acyclic = S::default()
        .vertex("item", "object")
        .edge(&next)
        .constraint("item", "acyclic", "next")
        .build();
    let ring = |n: u32, closed: bool| {
        let nodes = (0..n).map(|i| Node::new(i, "item")).collect();
        let mut edges: Vec<(u32, u32, &Edge)> = (0..n.saturating_sub(1))
            .map(|i| (i, i + 1, &next))
            .collect();
        if closed {
            edges.push((n - 1, 0, &next));
        }
        graph(nodes, &edges)
    };
    vec![
        Case::new(
            "cycles/legal_without_declaration",
            free,
            Instance::Graph(ring(2, true)),
        ),
        Case::new(
            "cycles/two_cycle",
            acyclic.clone(),
            Instance::Graph(ring(2, true)),
        ),
        Case::new(
            "cycles/long_cycle",
            acyclic.clone(),
            Instance::Graph(ring(4, true)),
        ),
        Case::new(
            "cycles/self_loop",
            acyclic.clone(),
            Instance::Graph(ring(1, true)),
        ),
        Case::new(
            "cycles/acyclic_chain",
            acyclic,
            Instance::Graph(ring(3, false)),
        ),
        // A subtree moved under its own descendant: nodes 1 and 2 hang under
        // each other, cut off from the root.
        Case::new(
            "cycles/tree_subtree_under_its_own_descendant",
            free_tree_schema(),
            Instance::Tree(tree(
                vec![
                    Node::new(0, "item"),
                    Node::new(1, "item"),
                    Node::new(2, "item"),
                ],
                &[(2, 1, &next), (1, 2, &next)],
                vec![],
            )),
        ),
    ]
}

/// An item whose `next` edge is free to cycle.
fn free_tree_schema() -> Schema {
    S::default()
        .vertex("item", "object")
        .edge(&prop("item", "item", "next"))
        .build()
}

/// The fan schema: a document of words, and the relation `rel` between two
/// words of a document.
fn fan_schema() -> Schema {
    S::default()
        .vertex("doc", "object")
        .vertex("word", "object")
        .edge(&prop("doc", "word", "words"))
        .hyper_edge("rel", &[("arg0", "word"), ("arg1", "word")], "about")
        .build()
}

/// A document of two words, related by `fan`.
fn fan_tree(fan: Fan) -> WInstance {
    let words = prop("doc", "word", "words");
    tree(
        vec![
            Node::new(0, "doc"),
            Node::new(1, "word"),
            Node::new(2, "word"),
        ],
        &[(0, 1, &words), (0, 2, &words)],
        vec![fan],
    )
}

/// The well-formed fan.
fn rel() -> Fan {
    Fan::new("rel", 0)
        .with_child("arg0", 1)
        .with_child("arg1", 2)
}

/// A label of the fan vocabulary: of kind `kind`, from the hyper-edge `rel`
/// to `tgt`, named `name`.
fn fan_label(kind: &str, tgt: &str, name: &str) -> Edge {
    edge("rel", tgt, kind, name)
}

/// Every fan violation, in graphs and trees, and well-formed fans.
fn fans() -> R<Vec<Case>> {
    let schema = fan_schema();
    let reified = |fan: Fan| GInstance::from_wtype_with_schema(&fan_tree(fan), &schema);
    let well_formed = reified(rel())?;
    let fan_node = well_formed
        .nodes()
        .values()
        .find(|n| n.shape == NodeShape::HyperEdge)
        .map(|n| n.id)
        .ok_or("the well-formed fan has no fan node")?;
    let with = |e: Edge, src: u32, tgt: u32| well_formed.clone().with_edge(src, tgt, e);
    let mut missing_parent = well_formed.clone();
    let parent_edge = missing_parent
        .out_edges(fan_node)
        .iter()
        .copied()
        .find(|e| {
            missing_parent
                .edge(*e)
                .is_some_and(|g| g.edge.kind == HYPER_PARENT_KIND)
        })
        .ok_or("the fan node has no parent edge")?;
    missing_parent.remove_edge(parent_edge)?;
    let graphs = vec![
        ("fans/well_formed", well_formed.clone()),
        ("fans/missing_parent", missing_parent),
        (
            "fans/multiple_parents",
            with(fan_label(HYPER_PARENT_KIND, "word", "about"), fan_node, 1),
        ),
        (
            "fans/duplicate_label",
            with(fan_label(HYPER_LABEL_KIND, "word", "arg0"), fan_node, 2),
        ),
        (
            "fans/unknown_label",
            with(fan_label(HYPER_LABEL_KIND, "word", "arg9"), fan_node, 1),
        ),
        (
            "fans/reserved_kind_outside_fan",
            with(edge("word", "word", HYPER_LABEL_KIND, "arg0"), 1, 2),
        ),
        (
            "fans/foreign_incident_edge",
            with(prop("doc", "word", "words"), 0, fan_node),
        ),
        (
            "fans/unknown_hyper_edge",
            reified(Fan::new("nope", 0).with_child("arg0", 1))?,
        ),
    ];
    let mut out: Vec<Case> = graphs
        .into_iter()
        .map(|(name, g)| Case::new(name, schema.clone(), Instance::Graph(g)))
        .collect();
    for (name, fan) in [
        ("fans/tree_well_formed", rel()),
        (
            "fans/tree_missing_parent",
            Fan::new("rel", 9).with_child("arg0", 1),
        ),
        ("fans/tree_missing_child", rel().with_child("arg1", 9)),
        ("fans/tree_unknown_label", rel().with_child("arg9", 2)),
        (
            "fans/tree_unknown_hyper_edge",
            Fan::new("nope", 0).with_child("arg0", 1),
        ),
    ] {
        out.push(Case::new(
            name,
            schema.clone(),
            Instance::Tree(fan_tree(fan)),
        ));
    }
    Ok(out)
}

/// A table whose rows hold `columns`, each an integer or string column
/// vertex named `<table>.<column>`.
fn table(s: S, name: &str, columns: &[(&str, &str)]) -> S {
    let mut s = s.vertex(name, "table");
    for (column, kind) in columns {
        let vertex = format!("{name}.{column}");
        s = s.vertex(&vertex, kind).edge(&prop(name, &vertex, column));
    }
    s
}

/// The reference from `holder`'s `column` to `target`'s key `key`, which
/// becomes the foreign key `fk`.
fn reference(
    id: &str,
    holder: &str,
    column: &str,
    target: &str,
    key: &str,
    nullable: bool,
    on_unresolved: OnUnresolved,
) -> Reference {
    Reference {
        id: id.into(),
        holder: holder.into(),
        source: RefSource::KeyValue {
            parts: vec![vec![prop(holder, &format!("{holder}.{column}"), column)]],
        },
        target_vertex: target.into(),
        target_key: Some(key.into()),
        edge: Some(edge(holder, target, "fk", id)),
        cardinality: Cardinality::One,
        nullable,
        on_unresolved,
        mode: ResolveMode::AddEdge,
    }
}

/// The Sakila-style schema: actors and films keyed by id, the film-actor
/// table keyed by the pair, and staff reporting to staff; the film-actor
/// references resolve on `on_unresolved`.
fn sakila(on_unresolved: OnUnresolved) -> R<Schema> {
    let int = "integer";
    let mut s = table(
        S::default(),
        "actor",
        &[("actor_id", int), ("name", "string")],
    );
    s = table(s, "film", &[("film_id", int), ("title", "string")]);
    s = table(s, "film_actor", &[("actor_id", int), ("film_id", int)]);
    s = table(s, "staff", &[("staff_id", int), ("reports_to", int)]);
    let col = |t: &str, c: &str| prop(t, &format!("{t}.{c}"), c);
    s = s
        .key(
            "actor",
            "actor_pk",
            &[col("actor", "actor_id")],
            KeyRole::Primary,
        )
        .key(
            "film",
            "film_pk",
            &[col("film", "film_id")],
            KeyRole::Primary,
        )
        .key(
            "film_actor",
            "film_actor_pk",
            &[col("film_actor", "actor_id"), col("film_actor", "film_id")],
            KeyRole::Primary,
        )
        .key(
            "staff",
            "staff_pk",
            &[col("staff", "staff_id")],
            KeyRole::Primary,
        );
    for r in [
        reference(
            "film_actor_actor",
            "film_actor",
            "actor_id",
            "actor",
            "actor_pk",
            false,
            on_unresolved.clone(),
        ),
        reference(
            "film_actor_film",
            "film_actor",
            "film_id",
            "film",
            "film_pk",
            false,
            on_unresolved,
        ),
        reference(
            "staff_reports_to",
            "staff",
            "reports_to",
            "staff",
            "staff_pk",
            true,
            OnUnresolved::Error,
        ),
    ] {
        let fk = r.edge.clone().ok_or("a reference without an edge")?;
        s = s.edge(&fk);
        s.0.declarations.insert_reference(r);
    }
    Ok(s.build())
}

/// Sakila rows: two actors, two films, the given film-actor pairs, and a
/// staff chain.
fn sakila_rows(film_actor: &[(i64, i64)]) -> R<FInstance> {
    let i = Value::Int;
    let s = |v: &str| Value::Str(v.into());
    let mut next = 0_u32;
    let mut id = || {
        next += 1;
        RowId(next)
    };
    let actors =
        [(1, "Ann"), (2, "Bo")].map(|(a, n)| (id(), row(&[("actor_id", i(a)), ("name", s(n))])));
    let films = [(10, "Alien"), (11, "Heat")]
        .map(|(f, t)| (id(), row(&[("film_id", i(f)), ("title", s(t))])));
    let pairs: Vec<_> = film_actor
        .iter()
        .map(|(a, f)| (id(), row(&[("actor_id", i(*a)), ("film_id", i(*f))])))
        .collect();
    let staff = [
        row(&[("staff_id", i(1))]),
        row(&[("staff_id", i(2)), ("reports_to", i(1))]),
        row(&[("staff_id", i(3)), ("reports_to", i(2))]),
    ]
    .map(|r| (id(), r));
    FInstance::new()
        .with_rows("actor", actors)
        .and_then(|f| f.with_rows("film", films))
        .and_then(|f| f.with_rows("film_actor", pairs))
        .and_then(|f| f.with_rows("staff", staff))
        .map_err(Into::into)
}

/// Keys and references: a Sakila composite key held and broken, a
/// self-referencing foreign key, and a dangling foreign key, strict and
/// lenient.
fn keys() -> R<Vec<Case>> {
    let strict = sakila(OnUnresolved::Error)?;
    Ok(vec![
        Case::new(
            "keys/sakila_composite_key",
            strict.clone(),
            Instance::Table(sakila_rows(&[(1, 10), (1, 11), (2, 10)])?),
        ),
        Case::new(
            "keys/sakila_composite_key_duplicate",
            strict.clone(),
            Instance::Table(sakila_rows(&[(1, 10), (2, 11), (1, 10)])?),
        ),
        Case::new(
            "keys/self_referencing_foreign_key",
            strict.clone(),
            Instance::Table(sakila_rows(&[])?),
        ),
        Case::new(
            "keys/dangling_foreign_key_strict",
            strict,
            Instance::Table(sakila_rows(&[(9, 10)])?),
        ),
        Case::new(
            "keys/dangling_foreign_key_lenient",
            sakila(OnUnresolved::Complement)?,
            Instance::Table(sakila_rows(&[(9, 10)])?),
        ),
    ])
}

/// The application check `audit`, about every item.
fn audit_options() -> ValidationOptions {
    let mut options = ValidationOptions::default();
    options.external.insert(
        "audit".into(),
        ExternalCheckDecl {
            name: "audit".into(),
            anchors: BTreeSet::from([Name::from("item")]),
            requirement: Requirement::Required,
        },
    );
    options
}

/// Reports that are not `Passed` because something was left undecided: a
/// budget too small for an acyclicity check, a sort the protocol does not
/// know, a pattern the regular-expression engine cannot compile, an
/// external check with no verdict and one with a rejecting verdict, a
/// failure beside an undecided check, and a source map the diagnostics
/// carry.
fn incomplete() -> Vec<Case> {
    let next = prop("item", "item", "next");
    let chain_schema = S::default()
        .vertex("item", "object")
        .edge(&next)
        .constraint("item", "acyclic", "next")
        .build();
    let chain = graph(
        (0..6).map(|i| Node::new(i, "item")).collect(),
        &(0..5).map(|i| (i, i + 1, &next)).collect::<Vec<_>>(),
    );
    let mut budget = Case::new(
        "incomplete/acyclic_budget",
        chain_schema,
        Instance::Graph(chain),
    );
    budget.budget = ValidationBudget { max_reads: Some(3) };

    let item = || graph(vec![Node::new(0, "item")], &[]);
    let colour = S::default()
        .vertex("item", "object")
        .constraint("item", "colour", "red")
        .build();
    let name = S::default()
        .vertex("name", "string")
        .constraint("name", "pattern", "(?=a)a")
        .build();
    let items = S::default().vertex("item", "object").build();
    let mut missing = Case::new(
        "incomplete/external_missing",
        items.clone(),
        Instance::Graph(item()),
    );
    missing.options = audit_options();
    let mut rejected = Case::new(
        "incomplete/external_rejected",
        items.clone(),
        Instance::Graph(item()),
    );
    rejected.options = audit_options();
    rejected.external.insert(
        "audit".into(),
        ViewRef::Node(0),
        ExternalVerdict {
            passed: false,
            witness: Digest([7; 32]),
        },
    );
    let mut mixed = Case::new(
        "incomplete/mixed_failure_and_incomplete",
        items,
        Instance::Graph(item().with_node(Node::new(1, "ghost"))),
    );
    mixed.options = audit_options();

    let bounded = S::default()
        .vertex("name", "string")
        .constraint("name", "maxLength", "3")
        .build();
    let mut sourced = Case::new(
        "incomplete/source_map",
        bounded,
        Instance::Graph(graph(
            vec![valued(0, "name", Value::Str("toolong".into()))],
            &[],
        )),
    );
    sourced.options.sources.0.insert(
        SchemaRef::Constraint {
            vertex: "name".into(),
            constraint: Constraint {
                sort: "maxLength".into(),
                value: "3".into(),
            },
        },
        SourceLocation {
            source: "schemas/name.json".into(),
            start_byte: 12,
            end_byte: 27,
        },
    );
    vec![
        budget,
        Case::new(
            "incomplete/unregistered_sort",
            colour,
            Instance::Graph(item()),
        ),
        Case::new(
            "incomplete/pattern_lookahead",
            name,
            Instance::Graph(graph(vec![valued(0, "name", Value::Str("a".into()))], &[])),
        ),
        missing,
        rejected,
        mixed,
        sourced,
    ]
}

/// Replace, anywhere in `value`, the number `from` under the key `key` with
/// `to`.
fn patch(value: &mut serde_json::Value, key: &str, from: u64, to: u64) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if k == key && v.as_u64() == Some(from) {
                    *v = serde_json::Value::from(to);
                } else {
                    patch(v, key, from, to);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                patch(v, key, from, to);
            }
        }
        _ => {}
    }
}

/// Instances construction and decoding reject: an edge to a node the graph
/// does not hold, and a foreign-key pair to a row the instance does not
/// hold.
fn rejections() -> Result<Vec<Rejection>, Box<dyn Error>> {
    let ab = prop("a", "b", "ab");
    let mut dangling_edge = serde_json::to_value(graph(
        vec![Node::new(0, "a"), Node::new(1, "b")],
        &[(0, 1, &ab)],
    ))?;
    patch(&mut dangling_edge, "tgt", 1, 99);
    let edge_error = serde_json::from_value::<GInstance>(dangling_edge.clone())
        .err()
        .ok_or("a graph with a dangling edge decodes")?;
    let stars = edge("film", "actor", "fk", "stars");
    let films = FInstance::new()
        .with_rows("film", [(RowId(0), row(&[]))])
        .and_then(|f| f.with_rows("actor", [(RowId(1), row(&[]))]))
        .and_then(|f| f.with_foreign_key(stars, [(RowId(0), RowId(1))]))?;
    let mut dangling_pair = serde_json::to_value(&films)?;
    let before = dangling_pair.clone();
    replace_row_in_pairs(&mut dangling_pair);
    if dangling_pair == before {
        return Err("the foreign-key pair was not found in the wire form".into());
    }
    let pair_error = serde_json::from_value::<FInstance>(dangling_pair.clone())
        .err()
        .ok_or("a table instance with a dangling pair decodes")?;
    Ok(vec![
        Rejection {
            name: "anchors/dangling_edge.rejection".into(),
            shape: InstanceShapeKind::Graph,
            instance: dangling_edge,
            error: edge_error.to_string(),
        },
        Rejection {
            name: "anchors/dangling_foreign_key.rejection".into(),
            shape: InstanceShapeKind::Table,
            instance: dangling_pair,
            error: pair_error.to_string(),
        },
    ])
}

/// Point the target of every foreign-key pair of a table instance's wire
/// form (`foreign_keys`: a list of `[edge, [[src, tgt], ...]]`) at row 99.
fn replace_row_in_pairs(wire: &mut serde_json::Value) {
    let Some(keys) = wire
        .get_mut("foreign_keys")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for entry in keys {
        let Some(pairs) = entry.get_mut(1).and_then(serde_json::Value::as_array_mut) else {
            continue;
        };
        for pair in pairs {
            if let Some(tgt) = pair.get_mut(1) {
                *tgt = serde_json::Value::from(99);
            }
        }
    }
}

/// Write `value` to `<out>/<name>.named.msgpack`, `.cbor` and `.json`.
fn write_three<T: Serialize>(out: &Path, name: &str, value: &T) -> Result<(), Box<dyn Error>> {
    let path = |ext: &str| out.join(format!("{name}.{ext}"));
    if let Some(dir) = path("json").parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path("named.msgpack"), rmp_serde::to_vec_named(value)?)?;
    let mut cbor = Vec::new();
    ciborium::into_writer(value, &mut cbor)?;
    std::fs::write(path("cbor"), cbor)?;
    let mut json = serde_json::to_string_pretty(value)?;
    json.push('\n');
    std::fs::write(path("json"), json)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/validation"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&out)?;
    let mut names = BTreeSet::new();
    let cases = [
        anchors()?,
        endpoints(),
        values()?,
        cardinality()?,
        ordering()?,
        cycles(),
        fans()?,
        keys()?,
        incomplete(),
    ];
    for case in cases.into_iter().flatten() {
        let fixture = case.validate();
        if !names.insert(fixture.name.clone()) {
            return Err(format!("fixture {} is listed twice", fixture.name).into());
        }
        write_three(&out, &fixture.name, &fixture)?;
        if let Instance::Graph(g) = &fixture.instance {
            let mut json = serde_json::to_string_pretty(&to_node_link_json(g))?;
            json.push('\n');
            std::fs::write(out.join(format!("{}.nodelink.json", fixture.name)), json)?;
        }
    }
    for rejection in rejections()? {
        write_three(&out, &rejection.name, &rejection)?;
    }
    Ok(())
}
