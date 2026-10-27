#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Independent reference checks for resolution.
//!
//! The state-law oracle in this file comes from `panproto-reference`; it
//! calls neither the production law checker nor its private candidate
//! generator. The production side is exercised only through the public
//! resolution API.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use panproto_gat::Name;
use panproto_inst::{FInstance, GInstance, Node, RowId, Value, ViewRef, WInstance};
use panproto_io::resolution::{
    AnyInstance, Resolution, ResolutionComplement, ResolutionKind, ResolutionProvider,
    ResolutionProviderId, ResolutionSpec, ResolveContext, ResolveDomainError, ResolveError,
    ResolveOptions, UnresolvedReason, decode_standoff_form, encode_standoff_form,
    fold_uri_environment, resolution_text_identity, resolve_inherited_uri_base,
    resolve_uri_reference,
};
use panproto_reference::enumerate::resolution::{
    indexed_sources, injective_renamings, single_step_basis, uri_directive_sequences,
};
use panproto_reference::resolution::{
    IndexedSource, IndexedView, ResolveDomainViolation, SingleStep, SynthIndex, apply_single_step,
    check_put_domain as reference_domain, coherent_rekey, coherent_rename, declared_key_equality,
    equivalent, gated_put_twice, get as reference_get, guarded_partition, index_target,
    indexed_get, indexed_put, inherited_base, nearest_preceding, preserving_registry,
    put as reference_put, rfc3986, standoff_form, standoff_inverse, text_identity, uri_environment,
};
use panproto_reference::value::Value as ReferenceValue;
use panproto_reference::{model as reference_model, schema as reference_schema};
use panproto_schema::{
    Cardinality, Edge, InstanceShapeKind, Key, KeyEquality, KeyPart, KeyRefError, KeyRole,
    KeyScope, NullMatch, OnUnresolved, Protocol, RefGuard, RefSource, Reference, ResolveMode,
    Schema, SchemaBuilder, SpanEnd, VariableBinding,
};

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

fn protocol(name: &str) -> Protocol {
    Protocol {
        name: name.to_owned(),
        syntactic_shape: InstanceShapeKind::Table,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    }
}

fn key(id: &str, parts: Vec<KeyPart>) -> Key {
    Key {
        id: id.into(),
        parts,
        role: KeyRole::Primary,
        scope: KeyScope::Document,
        equality: vec![],
    }
}

fn reference(edge: Edge) -> Reference {
    Reference {
        id: "pick".into(),
        holder: "pick".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![prop("pick", "label", "customer")]],
        },
        target_vertex: "customer".into(),
        target_key: None,
        edge: Some(edge),
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: NullMatch::default(),
        null_vertex: None,
        scope_via: None,
    }
}

struct Provider {
    protocol: Protocol,
    spec: ResolutionSpec,
    kind: ResolutionKind,
}

impl ResolutionProvider for Provider {
    fn id(&self) -> ResolutionProviderId {
        self.spec.provider.clone()
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
}

fn picks() -> (Resolution, Edge) {
    let protocol = protocol("reference-picks");
    let picked = edge("pick", "customer", "ref", Some("picked"));
    let schema = SchemaBuilder::new(&protocol)
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
            ),
        )
        .reference(reference(picked.clone()))
        .build()
        .unwrap();
    let provider_id = ResolutionProviderId {
        name: "reference-picks".into(),
        version: 1,
    };
    let spec = ResolutionSpec {
        kind: ResolutionKind::TableToGraph,
        provider: provider_id,
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: schema
            .edges
            .keys()
            .filter(|candidate| **candidate != picked)
            .cloned()
            .collect(),
        reference_writers: BTreeMap::from([(picked.clone(), "pick".into())]),
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    let provider = Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TableToGraph,
    };
    let resolution = Resolution::for_protocol(&protocol, &schema, Arc::new(provider)).unwrap();
    (resolution, picked)
}

fn tree_picks() -> (Resolution, Edge) {
    tree_picks_with_equality(Vec::new())
}

fn tree_picks_with_equality(equality: Vec<KeyEquality>) -> (Resolution, Edge) {
    let mut protocol = protocol("reference-tree-picks");
    protocol.syntactic_shape = InstanceShapeKind::Tree;
    let picked = edge("pick", "customer", "ref", Some("picked"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("customer", "object", None)
        .unwrap()
        .vertex("pick", "object", None)
        .unwrap()
        .vertex("label", "string", None)
        .unwrap()
        .edge("doc", "pick", "prop", Some("pick"))
        .unwrap()
        .edge("doc", "customer", "prop", Some("customers"))
        .unwrap()
        .edge("customer", "label", "prop", Some("cid"))
        .unwrap()
        .edge("pick", "label", "prop", Some("customer"))
        .unwrap()
        .edge("pick", "customer", "ref", Some("picked"))
        .unwrap()
        .entry("doc")
        .key(
            "customer",
            Key {
                id: "cid".into(),
                parts: vec![KeyPart::Path(vec![prop("customer", "label", "cid")])],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
                equality,
            },
        )
        .reference(Reference {
            cardinality: Cardinality::Many,
            nullable: true,
            ..reference(picked.clone())
        })
        .build()
        .unwrap();
    let provider_id = ResolutionProviderId {
        name: "reference-tree-picks".into(),
        version: 1,
    };
    let spec = ResolutionSpec {
        kind: ResolutionKind::TreeToGraph,
        provider: provider_id,
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: schema
            .edges
            .keys()
            .filter(|candidate| **candidate != picked)
            .cloned()
            .collect(),
        reference_writers: BTreeMap::from([(picked.clone(), "pick".into())]),
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    let provider = Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TreeToGraph,
    };
    let resolution = Resolution::for_protocol(&protocol, &schema, Arc::new(provider)).unwrap();
    (resolution, picked)
}

fn variable_resolution(binding: VariableBinding) -> (Resolution, Edge) {
    let mut protocol = protocol("reference-variable-binding");
    protocol.syntactic_shape = InstanceShapeKind::Tree;
    let child = prop("doc", "concept", "child");
    let variable = prop("concept", "name", "variable");
    let binding_edge = edge("concept", "concept", "ref", Some("binding"));
    let schema = SchemaBuilder::new(&protocol)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("concept", "object", None)
        .unwrap()
        .vertex("name", "string", None)
        .unwrap()
        .edge("doc", "concept", "prop", Some("child"))
        .unwrap()
        .edge("concept", "name", "prop", Some("variable"))
        .unwrap()
        .edge("concept", "concept", "ref", Some("binding"))
        .unwrap()
        .entry("doc")
        .key(
            "concept",
            Key {
                id: "binder".into(),
                parts: vec![KeyPart::Path(vec![variable.clone()])],
                role: KeyRole::Identifier,
                scope: KeyScope::Document,
                equality: Vec::new(),
            },
        )
        .reference(Reference {
            id: "variable".into(),
            holder: "concept".into(),
            source: RefSource::Variable {
                field: vec![variable],
                binder_key: "binder".into(),
                binding,
            },
            target_vertex: "concept".into(),
            target_key: None,
            edge: Some(binding_edge.clone()),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: NullMatch::default(),
            null_vertex: None,
            scope_via: None,
        })
        .build()
        .unwrap();
    let provider_id = ResolutionProviderId {
        name: "reference-variable-binding".into(),
        version: 1,
    };
    let spec = ResolutionSpec {
        kind: ResolutionKind::TreeToGraph,
        provider: provider_id,
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: BTreeSet::from([child]),
        reference_writers: BTreeMap::from([(binding_edge.clone(), "variable".into())]),
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    let provider = Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TreeToGraph,
    };
    let resolution = Resolution::for_protocol(&protocol, &schema, Arc::new(provider)).unwrap();
    (resolution, binding_edge)
}

fn variable_tree() -> WInstance {
    let mut nodes = HashMap::from([(0, Node::new(0, "doc"))]);
    for (id, name) in [(1, "x"), (2, "y"), (3, "x"), (4, "x")] {
        let mut node = Node::new(id, "concept");
        node.extra_fields.insert("variable".to_owned(), text(name));
        nodes.insert(id, node);
    }
    let child = prop("doc", "concept", "child");
    WInstance::new(
        nodes,
        (1..=4).map(|target| (0, target, child.clone())).collect(),
        Vec::new(),
        0,
        "doc".into(),
    )
}

fn guarded_picks() -> (Resolution, Edge, Edge) {
    let mut protocol = protocol("reference-guarded-picks");
    protocol.syntactic_shape = InstanceShapeKind::Tree;
    let left = edge("pick", "customer", "ref", Some("left"));
    let right = edge("pick", "customer", "ref", Some("right"));
    let customer = prop("pick", "label", "customer");
    let kind = prop("pick", "label", "kind");
    let make_reference = |id: &str, edge: Edge, guard: &str| Reference {
        id: id.into(),
        holder: "pick".into(),
        source: RefSource::KeyValue {
            parts: vec![vec![customer.clone()]],
        },
        target_vertex: "customer".into(),
        target_key: None,
        edge: Some(edge),
        cardinality: Cardinality::Many,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: Some(RefGuard {
            field: vec![kind.clone()],
            equals: text(guard),
        }),
        null_match: NullMatch::default(),
        null_vertex: None,
        scope_via: None,
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("customer", "object", None)
        .unwrap()
        .vertex("pick", "object", None)
        .unwrap()
        .vertex("label", "string", None)
        .unwrap()
        .edge("doc", "pick", "prop", Some("pick"))
        .unwrap()
        .edge("doc", "customer", "prop", Some("customers"))
        .unwrap()
        .edge("customer", "label", "prop", Some("cid"))
        .unwrap()
        .edge("pick", "label", "prop", Some("customer"))
        .unwrap()
        .edge("pick", "label", "prop", Some("kind"))
        .unwrap()
        .edge("pick", "customer", "ref", Some("left"))
        .unwrap()
        .edge("pick", "customer", "ref", Some("right"))
        .unwrap()
        .entry("doc")
        .key(
            "customer",
            key(
                "cid",
                vec![KeyPart::Path(vec![prop("customer", "label", "cid")])],
            ),
        )
        .reference(make_reference("left", left.clone(), "left"))
        .reference(make_reference("right", right.clone(), "right"))
        .build()
        .unwrap();
    let provider_id = ResolutionProviderId {
        name: "reference-guarded-picks".into(),
        version: 1,
    };
    let spec = ResolutionSpec {
        kind: ResolutionKind::TreeToGraph,
        provider: provider_id,
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: schema
            .edges
            .keys()
            .filter(|candidate| **candidate != left && **candidate != right)
            .cloned()
            .collect(),
        reference_writers: BTreeMap::from([
            (left.clone(), "left".into()),
            (right.clone(), "right".into()),
        ]),
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    let provider = Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TreeToGraph,
    };
    let resolution = Resolution::for_protocol(&protocol, &schema, Arc::new(provider)).unwrap();
    (resolution, left, right)
}

fn guarded_pick_tree(kind: &str) -> WInstance {
    let mut source = production_pick_tree(&BTreeSet::from(["a".to_owned()]));
    source
        .nodes
        .get_mut(&1)
        .expect("pick holder")
        .extra_fields
        .insert("kind".to_owned(), text(kind));
    source
}

fn production_pick_tree(source: &IndexedSource) -> WInstance {
    let mut pick = Node::new(1, "pick");
    pick.extra_fields.insert(
        "customer".to_owned(),
        Value::List(source.iter().map(|target| text(target)).collect()),
    );
    let mut nodes = HashMap::from([(0, Node::new(0, "doc")), (1, pick)]);
    for (target, name) in TARGETS.iter().enumerate() {
        let id = 2 + u32::try_from(target).expect("three targets");
        let mut customer = Node::new(id, "customer");
        customer.extra_fields.insert("cid".to_owned(), text(name));
        nodes.insert(id, customer);
    }
    let pick = prop("doc", "pick", "pick");
    let customers = prop("doc", "customer", "customers");
    WInstance::new(
        nodes,
        std::iter::once((0, 1, pick))
            .chain((0..3).map(|target| (0, 2 + target, customers.clone())))
            .collect(),
        Vec::new(),
        0,
        "doc".into(),
    )
}

fn numeric_pick_tree() -> WInstance {
    let mut pick = Node::new(1, "pick");
    pick.extra_fields
        .insert("customer".to_owned(), Value::List(vec![Value::Float(1.0)]));
    let mut customer = Node::new(2, "customer");
    customer
        .extra_fields
        .insert("cid".to_owned(), Value::Int(1));
    WInstance::new(
        HashMap::from([(0, Node::new(0, "doc")), (1, pick), (2, customer)]),
        vec![
            (0, 1, prop("doc", "pick", "pick")),
            (0, 2, prop("doc", "customer", "customers")),
        ],
        Vec::new(),
        0,
        "doc".into(),
    )
}

fn null_match_resolution(null_match: NullMatch) -> (Resolution, Edge) {
    let mut protocol = protocol("reference-null-match");
    protocol.syntactic_shape = InstanceShapeKind::Tree;
    let picked = edge("pick", "customer", "ref", Some("picked"));
    let key_parts = ["left", "right"]
        .map(|name| KeyPart::Path(vec![prop("customer", "label", &format!("{name}_id"))]));
    let reference_parts = ["left", "right"]
        .map(|name| vec![prop("pick", "label", name)])
        .to_vec();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("customer", "object", None)
        .unwrap()
        .vertex("pick", "object", None)
        .unwrap()
        .vertex("label", "scalar", None)
        .unwrap()
        .edge("doc", "pick", "prop", Some("pick"))
        .unwrap()
        .edge("doc", "customer", "prop", Some("customer"))
        .unwrap()
        .edge("customer", "label", "prop", Some("left_id"))
        .unwrap()
        .edge("customer", "label", "prop", Some("right_id"))
        .unwrap()
        .edge("pick", "label", "prop", Some("left"))
        .unwrap()
        .edge("pick", "label", "prop", Some("right"))
        .unwrap()
        .edge("pick", "customer", "ref", Some("picked"))
        .unwrap()
        .entry("doc")
        .key("customer", key("pair", key_parts.into_iter().collect()))
        .reference(Reference {
            id: "pair".into(),
            holder: "pick".into(),
            source: RefSource::KeyValue {
                parts: reference_parts,
            },
            target_vertex: "customer".into(),
            target_key: Some("pair".into()),
            edge: Some(picked.clone()),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match,
            null_vertex: None,
            scope_via: None,
        })
        .build()
        .unwrap();
    let provider_id = ResolutionProviderId {
        name: "reference-null-match".into(),
        version: 1,
    };
    let spec = ResolutionSpec {
        kind: ResolutionKind::TreeToGraph,
        provider: provider_id,
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: schema
            .edges
            .keys()
            .filter(|candidate| **candidate != picked)
            .cloned()
            .collect(),
        reference_writers: BTreeMap::from([(picked.clone(), "pair".into())]),
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    let provider = Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TreeToGraph,
    };
    (
        Resolution::for_protocol(&protocol, &schema, Arc::new(provider)).unwrap(),
        picked,
    )
}

fn null_match_tree(left: Value, right: Value) -> WInstance {
    let mut holder = Node::new(1, "pick");
    holder.extra_fields.insert("left".to_owned(), left);
    holder.extra_fields.insert("right".to_owned(), right);
    let mut target = Node::new(2, "customer");
    target
        .extra_fields
        .insert("left_id".to_owned(), Value::Int(1));
    target
        .extra_fields
        .insert("right_id".to_owned(), Value::Int(2));
    WInstance::new(
        HashMap::from([(0, Node::new(0, "doc")), (1, holder), (2, target)]),
        vec![
            (0, 1, prop("doc", "pick", "pick")),
            (0, 2, prop("doc", "customer", "customer")),
        ],
        Vec::new(),
        0,
        "doc".into(),
    )
}

fn reference_pick_tree(
    source: &IndexedSource,
) -> (
    reference_schema::Schema,
    reference_model::Tree,
    reference_model::SEdge,
) {
    let pick = reference_model::SEdge::new("doc", "pick", "prop", Some("pick"));
    let customers = reference_model::SEdge::new("doc", "customer", "prop", Some("customers"));
    let cid = reference_model::SEdge::new("customer", "label", "prop", Some("cid"));
    let customer = reference_model::SEdge::new("pick", "label", "prop", Some("customer"));
    let picked = reference_model::SEdge::new("pick", "customer", "ref", Some("picked"));
    let mut schema = reference_schema::Schema::default()
        .vertex("doc", "object")
        .vertex("customer", "object")
        .vertex("pick", "object")
        .vertex("label", "string")
        .edge("doc", "pick", "prop", Some("pick"))
        .edge("doc", "customer", "prop", Some("customers"));
    schema
        .edges
        .extend([cid.clone(), customer.clone(), picked.clone()]);
    schema.entries.push("doc".to_owned());
    schema.keys.insert(
        "customer".to_owned(),
        vec![reference_schema::Key {
            id: "cid".to_owned(),
            parts: vec![reference_schema::KeyPart::Path(vec![cid])],
            role: reference_schema::KeyRole::Primary,
            scope: reference_schema::KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "pick".to_owned(),
        reference_schema::Reference {
            id: "pick".to_owned(),
            holder: "pick".to_owned(),
            source: reference_schema::RefSource::KeyValue {
                parts: vec![vec![customer]],
            },
            target_vertex: "customer".to_owned(),
            target_key: Some("cid".to_owned()),
            edge: Some(picked.clone()),
            cardinality: reference_schema::Cardinality::Many,
            nullable: true,
            on_unresolved: reference_schema::OnUnresolved::Error,
            mode: reference_schema::ResolveMode::AddEdge,
            guard: None,
            null_match: reference_schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let mut nodes = BTreeMap::from([
        (0, reference_model::Node::new(0, "doc")),
        (
            1,
            reference_model::Node::new(1, "pick").with_field(
                "customer",
                ReferenceValue::List(
                    source
                        .iter()
                        .map(|target| ReferenceValue::Str(target.clone()))
                        .collect(),
                ),
            ),
        ),
    ]);
    for (target, name) in TARGETS.iter().enumerate() {
        let id = 2 + u32::try_from(target).expect("three targets");
        nodes.insert(
            id,
            reference_model::Node::new(id, "customer")
                .with_field("cid", ReferenceValue::Str((*name).to_owned())),
        );
    }
    let tree = reference_model::Tree {
        nodes,
        arcs: std::iter::once((0, 1, pick))
            .chain((0..3).map(|target| (0, 2 + target, customers.clone())))
            .collect(),
        fans: Vec::new(),
        root: 0,
    };
    (schema, tree, picked)
}

fn reference_null_match(
    null_match: reference_schema::NullMatch,
    left: ReferenceValue,
    right: ReferenceValue,
) -> (
    reference_schema::Schema,
    reference_model::Tree,
    reference_model::SEdge,
) {
    let root_pick = reference_model::SEdge::new("doc", "pick", "prop", Some("pick"));
    let root_customer = reference_model::SEdge::new("doc", "customer", "prop", Some("customer"));
    let left_id = reference_model::SEdge::new("customer", "label", "prop", Some("left_id"));
    let right_id = reference_model::SEdge::new("customer", "label", "prop", Some("right_id"));
    let left_field = reference_model::SEdge::new("pick", "label", "prop", Some("left"));
    let right_field = reference_model::SEdge::new("pick", "label", "prop", Some("right"));
    let picked = reference_model::SEdge::new("pick", "customer", "ref", Some("picked"));
    let mut schema = reference_schema::Schema::default()
        .vertex("doc", "object")
        .vertex("customer", "object")
        .vertex("pick", "object")
        .vertex("label", "scalar")
        .edge("doc", "pick", "prop", Some("pick"))
        .edge("doc", "customer", "prop", Some("customer"));
    schema.edges.extend([
        left_id.clone(),
        right_id.clone(),
        left_field.clone(),
        right_field.clone(),
        picked.clone(),
    ]);
    schema.entries.push("doc".to_owned());
    schema.keys.insert(
        "customer".to_owned(),
        vec![reference_schema::Key {
            id: "pair".to_owned(),
            parts: vec![
                reference_schema::KeyPart::Path(vec![left_id]),
                reference_schema::KeyPart::Path(vec![right_id]),
            ],
            role: reference_schema::KeyRole::Primary,
            scope: reference_schema::KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "pair".to_owned(),
        reference_schema::Reference {
            id: "pair".to_owned(),
            holder: "pick".to_owned(),
            source: reference_schema::RefSource::KeyValue {
                parts: vec![vec![left_field], vec![right_field]],
            },
            target_vertex: "customer".to_owned(),
            target_key: Some("pair".to_owned()),
            edge: Some(picked.clone()),
            cardinality: reference_schema::Cardinality::One,
            nullable: false,
            on_unresolved: reference_schema::OnUnresolved::Error,
            mode: reference_schema::ResolveMode::AddEdge,
            guard: None,
            null_match,
            null_vertex: None,
            scope_via: None,
        },
    );
    let nodes = BTreeMap::from([
        (0, reference_model::Node::new(0, "doc")),
        (
            1,
            reference_model::Node::new(1, "pick")
                .with_field("left", left)
                .with_field("right", right),
        ),
        (
            2,
            reference_model::Node::new(2, "customer")
                .with_field("left_id", ReferenceValue::Int(1))
                .with_field("right_id", ReferenceValue::Int(2)),
        ),
    ]);
    let tree = reference_model::Tree {
        nodes,
        arcs: vec![(0, 1, root_pick), (0, 2, root_customer)],
        fans: Vec::new(),
        root: 0,
    };
    (schema, tree, picked)
}

const TARGETS: [&str; 3] = ["a", "b", "c"];

fn target_number(target: &str) -> u32 {
    TARGETS
        .iter()
        .position(|candidate| *candidate == target)
        .map(|index| u32::try_from(index).expect("three targets"))
        .expect("enumerated target")
}

const fn pick_row(target: u32) -> u32 {
    10 + target
}

fn row(cells: &[(&str, Value)]) -> HashMap<String, Value> {
    cells
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn text(value: &str) -> Value {
    Value::Str(value.to_owned())
}

fn picks_table(source: &IndexedSource) -> FInstance {
    FInstance::new()
        .with_rows(
            "customer",
            TARGETS.iter().enumerate().map(|(target, name)| {
                (
                    RowId(u32::try_from(target).expect("three targets")),
                    row(&[("cid", text(name))]),
                )
            }),
        )
        .expect("customer rows")
        .with_rows(
            "pick",
            source.iter().map(|name| {
                let target = target_number(name);
                (RowId(pick_row(target)), row(&[("customer", text(name))]))
            }),
        )
        .expect("pick rows")
}

fn pick_node(target: &str) -> Node {
    let target_number = target_number(target);
    let mut node = Node::new(pick_row(target_number), "pick");
    node.extra_fields
        .insert("customer".to_owned(), text(target));
    node
}

fn production_get(
    resolution: &Resolution,
    source: &IndexedSource,
) -> (GInstance, ResolutionComplement) {
    let (view, complement) = resolution
        .get(
            &AnyInstance::Table(picks_table(source)),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .expect("reference source resolves");
    let AnyInstance::Graph(view) = view else {
        panic!("TableToGraph returned a non-graph")
    };
    (view, complement)
}

fn pick_edges(view: &GInstance, picked: &Edge) -> BTreeMap<u32, panproto_inst::EdgeId> {
    view.edges_sorted()
        .filter(|(_, edge)| edge.edge == *picked)
        .map(|(id, edge)| (edge.src, id))
        .collect()
}

fn pick_pairs(view: &GInstance, picked: &Edge) -> Vec<(u32, u32)> {
    let mut pairs: Vec<_> = view
        .edges_sorted()
        .filter(|(_, edge)| edge.edge == *picked)
        .map(|(_, edge)| (edge.src, edge.tgt))
        .collect();
    pairs.sort_unstable();
    pairs
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NullOutcome {
    Resolved,
    Dangling,
    Absent,
}

fn production_null_outcome(
    null_match: NullMatch,
    left: Value,
    right: Value,
) -> (NullOutcome, Option<Value>) {
    let (resolution, picked) = null_match_resolution(null_match);
    let source = null_match_tree(left, right);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions { lenient: true },
            &ResolveContext::empty(),
        )
        .expect("null-matching get");
    let AnyInstance::Graph(view) = view else {
        panic!("TreeToGraph returned a non-graph")
    };
    let restored = resolution
        .put(
            &AnyInstance::Graph(view.clone()),
            &complement,
            &ResolveContext::empty(),
        )
        .expect("null-matching GetPut");
    assert!(same_instance(&restored, &AnyInstance::Tree(source)));
    let resolved = view
        .edges_sorted()
        .filter(|(_, edge)| edge.edge == picked)
        .count();
    match (resolved, complement.unresolved.as_slice()) {
        (1, []) => (NullOutcome::Resolved, None),
        (0, [record]) if matches!(record.reason, UnresolvedReason::Dangling) => {
            (NullOutcome::Dangling, Some(record.literal.clone()))
        }
        (0, []) => (NullOutcome::Absent, None),
        other => panic!("unexpected production null-match result: {other:?}"),
    }
}

fn reference_null_outcome(
    null_match: reference_schema::NullMatch,
    left: ReferenceValue,
    right: ReferenceValue,
) -> (NullOutcome, Option<ReferenceValue>) {
    let (schema, tree, picked) = reference_null_match(null_match, left, right);
    let (view, complement) = reference_get(&schema, &tree, true).expect("reference null get");
    assert_eq!(reference_put(&schema, &view, &complement), Ok(tree));
    let resolved = view
        .edges
        .values()
        .filter(|edge| edge.label == picked)
        .count();
    match (resolved, complement.unresolved.as_slice()) {
        (1, []) => (NullOutcome::Resolved, None),
        (0, [(record, panproto_reference::resolution::Unresolved::Dangling)]) => {
            (NullOutcome::Dangling, Some(record.literal.clone()))
        }
        (0, []) => (NullOutcome::Absent, None),
        other => panic!("unexpected reference null-match result: {other:?}"),
    }
}

fn same_instance(left: &AnyInstance, right: &AnyInstance) -> bool {
    serde_json::to_value(left).expect("serialize left")
        == serde_json::to_value(right).expect("serialize right")
}

fn production_edit(
    view: &GInstance,
    reference_view: &IndexedView,
    picked: &Edge,
    step: &SingleStep,
) -> GInstance {
    let production_edges: Vec<_> = pick_edges(view, picked).into_values().collect();
    let edge_of = |id: u32| {
        let rank = reference_view
            .keys()
            .position(|candidate| *candidate == id)
            .expect("reference carrier occurs");
        production_edges[rank]
    };
    let mut edited = view.clone();
    match step {
        SingleStep::Add(target) => {
            let target_number = target_number(target);
            let fresh = view
                .edges()
                .keys()
                .map(|id| id.0)
                .max()
                .map_or(0, |last| last.checked_add(1).expect("bounded ids"));
            edited
                .insert_node(pick_node(target))
                .expect("fresh pick row");
            edited
                .insert_edge(
                    panproto_inst::EdgeId(fresh),
                    panproto_inst::GEdge::new(
                        pick_row(target_number),
                        target_number,
                        picked.clone(),
                    ),
                )
                .expect("fresh occurrence edge");
        }
        SingleStep::Delete(id) => {
            let edge = edge_of(*id);
            let holder = view.edge(edge).expect("occurrence edge").src;
            edited.remove_edge(edge).expect("remove occurrence edge");
            edited.remove_node(holder).expect("remove pick row");
        }
        SingleStep::Retarget { id, target } => {
            let edge = edge_of(*id);
            let holder = view.edge(edge).expect("occurrence edge").src;
            edited
                .set_endpoints(edge, holder, target_number(target))
                .expect("retarget occurrence edge");
        }
    }
    edited
}

fn production_violations(
    resolution: &Resolution,
    candidate: &GInstance,
    complement: &ResolutionComplement,
) -> Vec<ResolveDomainError> {
    resolution
        .check_put_domain(
            &AnyInstance::Graph(candidate.clone()),
            complement,
            &ResolveContext::empty(),
        )
        .err()
        .unwrap_or_default()
}

fn rekeyed_complement(
    complement: &ResolutionComplement,
    candidate: &GInstance,
    picked: &Edge,
) -> ResolutionComplement {
    let by_holder = pick_edges(candidate, picked);
    let mut rekeyed = complement.clone();
    rekeyed.occurrences = complement
        .occurrences
        .iter()
        .map(|(carrier, occurrences)| {
            let carrier = occurrences.first().map_or_else(
                || carrier.clone(),
                |occurrence| match (&occurrence.holder, carrier) {
                    (ViewRef::Row(row), ViewRef::Edge(_)) => by_holder
                        .get(&row.0)
                        .map_or_else(|| carrier.clone(), |edge| ViewRef::Edge(*edge)),
                    _ => carrier.clone(),
                },
            );
            (carrier, occurrences.clone())
        })
        .collect();
    rekeyed
}

fn rename_production(
    view: &GInstance,
    complement: &ResolutionComplement,
    renaming: &BTreeMap<u32, u32>,
) -> (GInstance, ResolutionComplement) {
    let mut renamed = view.clone();
    let moved: Vec<_> = renaming
        .keys()
        .filter_map(|id| {
            let id = panproto_inst::EdgeId(*id);
            view.edge(id).cloned().map(|edge| (id, edge))
        })
        .collect();
    for (id, _) in &moved {
        renamed.remove_edge(*id).expect("renamed occurrence");
    }
    for (id, edge) in moved {
        renamed
            .insert_edge(panproto_inst::EdgeId(renaming[&id.0]), edge)
            .expect("injective presentation");
    }
    let mut complement = complement.clone();
    complement.occurrences = complement
        .occurrences
        .into_iter()
        .map(|(carrier, occurrences)| match carrier {
            ViewRef::Edge(id) => (
                ViewRef::Edge(panproto_inst::EdgeId(
                    renaming.get(&id.0).copied().unwrap_or(id.0),
                )),
                occurrences,
            ),
            other => (other, occurrences),
        })
        .collect();
    (renamed, complement)
}

fn renamed_reference_violation(
    violation: &ResolveDomainViolation,
    renaming: &BTreeMap<u32, u32>,
) -> ResolveDomainViolation {
    match violation {
        ResolveDomainViolation::SynthesizedEdited(id) => {
            ResolveDomainViolation::SynthesizedEdited(renaming.get(id).copied().unwrap_or(*id))
        }
        ResolveDomainViolation::RepeatedTarget(target) => {
            ResolveDomainViolation::RepeatedTarget(target.clone())
        }
    }
}

fn renamed_production_violation(
    violation: &ResolveDomainError,
    renaming: &BTreeMap<u32, u32>,
) -> ResolveDomainError {
    let rename = |carrier: &ViewRef| match carrier {
        ViewRef::Edge(id) => ViewRef::Edge(panproto_inst::EdgeId(
            renaming.get(&id.0).copied().unwrap_or(id.0),
        )),
        other => other.clone(),
    };
    let rename_optional = |carrier: &Option<ViewRef>| carrier.as_ref().map(&rename);
    match violation {
        ResolveDomainError::NoPlacement { carrier } => ResolveDomainError::NoPlacement {
            carrier: rename(carrier),
        },
        ResolveDomainError::AmbiguousPlacement {
            carrier,
            references,
        } => ResolveDomainError::AmbiguousPlacement {
            carrier: rename(carrier),
            references: references.clone(),
        },
        ResolveDomainError::UnrepresentableReference { carrier, reference } => {
            ResolveDomainError::UnrepresentableReference {
                carrier: rename(carrier),
                reference: reference.clone(),
            }
        }
        ResolveDomainError::OffsetOutOfRange { carrier, reference } => {
            ResolveDomainError::OffsetOutOfRange {
                carrier: rename(carrier),
                reference: reference.clone(),
            }
        }
        ResolveDomainError::OffsetUnitViolation {
            carrier,
            reference,
            reason,
        } => ResolveDomainError::OffsetUnitViolation {
            carrier: rename(carrier),
            reference: reference.clone(),
            reason: *reason,
        },
        ResolveDomainError::SynthesizedEdited { carrier } => {
            ResolveDomainError::SynthesizedEdited {
                carrier: rename(carrier),
            }
        }
        ResolveDomainError::ContextMismatch { at } => ResolveDomainError::ContextMismatch {
            at: rename_optional(at),
        },
        ResolveDomainError::Assembly {
            at,
            production,
            reason,
        } => ResolveDomainError::Assembly {
            at: rename_optional(at),
            production: production.clone(),
            reason: *reason,
        },
    }
}

fn production_tree_pick_edges(
    view: &GInstance,
    picked: &Edge,
) -> BTreeMap<u32, panproto_inst::EdgeId> {
    view.edges_sorted()
        .filter(|(_, edge)| edge.edge == *picked)
        .map(|(id, edge)| (edge.tgt, id))
        .collect()
}

fn reference_tree_pick_edges(
    view: &reference_model::Graph,
    picked: &reference_model::SEdge,
) -> BTreeMap<u32, u32> {
    view.edges
        .iter()
        .filter(|(_, edge)| edge.label == *picked)
        .map(|(id, edge)| (edge.tgt, *id))
        .collect()
}

fn production_tree_edit(
    view: &GInstance,
    picked: &Edge,
    step: &SingleStep,
    indexed: &IndexedView,
) -> GInstance {
    let mut edited = view.clone();
    let edges = production_tree_pick_edges(view, picked);
    let target_of = |id: u32| 2 + target_number(&indexed[&id]);
    match step {
        SingleStep::Add(target) => {
            let fresh = view
                .edges()
                .keys()
                .map(|id| id.0)
                .max()
                .map_or(0, |last| last.checked_add(1).expect("bounded ids"));
            edited
                .insert_edge(
                    panproto_inst::EdgeId(fresh),
                    panproto_inst::GEdge::new(1, 2 + target_number(target), picked.clone()),
                )
                .expect("fresh resolution edge");
        }
        SingleStep::Delete(id) => {
            edited
                .remove_edge(edges[&target_of(*id)])
                .expect("existing resolution edge");
        }
        SingleStep::Retarget { id, target } => {
            let edge = edges[&target_of(*id)];
            edited
                .set_endpoints(edge, 1, 2 + target_number(target))
                .expect("retarget resolution edge");
        }
    }
    edited
}

fn reference_tree_edit(
    view: &reference_model::Graph,
    picked: &reference_model::SEdge,
    step: &SingleStep,
    indexed: &IndexedView,
) -> reference_model::Graph {
    let mut edited = view.clone();
    let edges = reference_tree_pick_edges(view, picked);
    let target_of = |id: u32| 2 + target_number(&indexed[&id]);
    match step {
        SingleStep::Add(target) => {
            let fresh = view
                .edges
                .keys()
                .next_back()
                .map_or(0, |last| last.checked_add(1).expect("bounded ids"));
            edited.edges.insert(
                fresh,
                reference_model::GEdge {
                    src: 1,
                    tgt: 2 + target_number(target),
                    label: picked.clone(),
                    position: None,
                },
            );
        }
        SingleStep::Delete(id) => {
            edited.edges.remove(&edges[&target_of(*id)]);
        }
        SingleStep::Retarget { id, target } => {
            edited
                .edges
                .get_mut(&edges[&target_of(*id)])
                .expect("existing resolution edge")
                .tgt = 2 + target_number(target);
        }
    }
    edited
}

fn reference_tree_sequence(tree: &reference_model::Tree) -> Vec<String> {
    match &tree.nodes[&1].fields["customer"] {
        ReferenceValue::List(values) => values
            .iter()
            .map(|value| match value {
                ReferenceValue::Str(value) => value.clone(),
                other => panic!("non-string reference target {other:?}"),
            })
            .collect(),
        other => panic!("non-sequence reference field {other:?}"),
    }
}

fn production_tree_sequence(tree: &WInstance) -> Vec<String> {
    match &tree.nodes[&1].extra_fields["customer"] {
        Value::List(values) => values
            .iter()
            .map(|value| match value {
                Value::Str(value) => value.clone(),
                other => panic!("non-string production target {other:?}"),
            })
            .collect(),
        other => panic!("non-sequence production field {other:?}"),
    }
}

fn production_tree_targets(view: &GInstance, picked: &Edge) -> Vec<u32> {
    production_tree_pick_edges(view, picked)
        .into_keys()
        .collect()
}

fn reference_tree_targets(
    view: &reference_model::Graph,
    picked: &reference_model::SEdge,
) -> Vec<u32> {
    reference_tree_pick_edges(view, picked)
        .into_keys()
        .collect()
}

fn assert_reference_first_add_requires_coherence(
    source: &IndexedSource,
    indexed_view: &IndexedView,
) {
    let (reference_schema, reference_source, reference_picked) = reference_pick_tree(source);
    let (reference_view, reference_complement) =
        reference_get(&reference_schema, &reference_source, false)
            .expect("empty reference source resolves");
    let reference_edge_only = reference_tree_edit(
        &reference_view,
        &reference_picked,
        &SingleStep::Add("a".to_owned()),
        indexed_view,
    );
    assert_eq!(
        reference_put(
            &reference_schema,
            &reference_edge_only,
            &reference_complement,
        ),
        Err(
            panproto_reference::resolution::ResolveError::UnrepresentableReference {
                carrier: panproto_reference::lens::ViewRef::Node(1),
                reference: Some("pick".to_owned()),
            }
        )
    );
    let mut reference_coherent = reference_edge_only;
    reference_coherent
        .nodes
        .get_mut(&1)
        .expect("reference holder")
        .fields
        .insert(
            "customer".to_owned(),
            ReferenceValue::List(vec![ReferenceValue::Str("a".to_owned())]),
        );
    let reference_restored = reference_put(
        &reference_schema,
        &reference_coherent,
        &reference_complement,
    )
    .expect("coherent reference first occurrence is writable");
    assert_eq!(reference_tree_sequence(&reference_restored), ["a"]);
}

fn assert_production_first_add_requires_coherence(
    source: &IndexedSource,
    indexed_view: &IndexedView,
) {
    let (resolution, picked) = tree_picks();
    let production_source = production_pick_tree(source);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(production_source),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .expect("empty nullable Many source resolves");
    let AnyInstance::Graph(view) = view else {
        panic!("TreeToGraph returned a non-graph")
    };
    let edge_only = production_tree_edit(
        &view,
        &picked,
        &SingleStep::Add("a".to_owned()),
        indexed_view,
    );
    let expected = ResolveDomainError::NoPlacement {
        carrier: ViewRef::Node(1),
    };
    let violations = resolution
        .check_put_domain(
            &AnyInstance::Graph(edge_only.clone()),
            &complement,
            &ResolveContext::empty(),
        )
        .expect_err("stale visible sequence must be refused");
    assert_eq!(violations.first(), Some(&expected));
    match resolution.put(
        &AnyInstance::Graph(edge_only),
        &complement,
        &ResolveContext::empty(),
    ) {
        Err(ResolveError::PutDomain(found)) => assert_eq!(found, expected),
        other => panic!("expected stale-field put refusal, found {other:?}"),
    }

    let mut coherent = production_tree_edit(
        &view,
        &picked,
        &SingleStep::Add("a".to_owned()),
        indexed_view,
    );
    coherent
        .node_mut(1)
        .expect("holder")
        .extra_fields
        .insert("customer".to_owned(), Value::List(vec![text("a")]));
    assert!(
        resolution
            .check_put_domain(
                &AnyInstance::Graph(coherent.clone()),
                &complement,
                &ResolveContext::empty(),
            )
            .is_ok()
    );
    let restored = resolution
        .put(
            &AnyInstance::Graph(coherent),
            &complement,
            &ResolveContext::empty(),
        )
        .expect("coherent first occurrence is writable");
    let AnyInstance::Tree(restored) = restored else {
        panic!("TreeToGraph put returned a non-tree")
    };
    assert_eq!(production_tree_sequence(&restored), ["a"]);
}

#[test]
fn first_add_edge_requires_a_coherent_visible_sequence() {
    let source = IndexedSource::new();
    let (indexed_view, _) = indexed_get(&source);
    assert_reference_first_add_requires_coherence(&source, &indexed_view);
    assert_production_first_add_requires_coherence(&source, &indexed_view);
}

#[test]
#[allow(clippy::too_many_lines)]
fn declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope() {
    let (production_resolution, production_picked) = tree_picks();
    let mut candidate_count = 0usize;
    for source in indexed_sources() {
        let (reference_schema, reference_source, reference_picked) = reference_pick_tree(&source);
        let (reference_view, reference_complement) =
            reference_get(&reference_schema, &reference_source, false)
                .expect("reference get succeeds");
        assert_eq!(
            reference_put(&reference_schema, &reference_view, &reference_complement),
            Ok(reference_source.clone()),
            "reference GetPut: {source:?}"
        );

        let production_source = production_pick_tree(&source);
        let (production_view, production_complement) = production_resolution
            .get(
                &AnyInstance::Tree(production_source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .expect("production get succeeds");
        let AnyInstance::Graph(production_view) = production_view else {
            panic!("TreeToGraph returned a non-graph")
        };
        let restored = production_resolution
            .put(
                &AnyInstance::Graph(production_view.clone()),
                &production_complement,
                &ResolveContext::empty(),
            )
            .expect("production GetPut");
        assert!(same_instance(
            &restored,
            &AnyInstance::Tree(production_source.clone())
        ));
        assert_eq!(
            production_tree_targets(&production_view, &production_picked),
            reference_tree_targets(&reference_view, &reference_picked),
            "get targets: {source:?}"
        );

        let (indexed_view, _) = indexed_get(&source);
        for step in single_step_basis(&indexed_view) {
            candidate_count += 1;
            let reference_candidate =
                reference_tree_edit(&reference_view, &reference_picked, &step, &indexed_view);
            let production_candidate =
                production_tree_edit(&production_view, &production_picked, &step, &indexed_view);
            let reference_once = reference_put(
                &reference_schema,
                &reference_candidate,
                &reference_complement,
            );
            let production_domain = production_resolution.check_put_domain(
                &AnyInstance::Graph(production_candidate.clone()),
                &production_complement,
                &ResolveContext::empty(),
            );
            let production_once = production_resolution.put(
                &AnyInstance::Graph(production_candidate.clone()),
                &production_complement,
                &ResolveContext::empty(),
            );
            assert_eq!(
                production_domain.is_ok(),
                production_once.is_ok(),
                "production domain/put: {source:?}, {step:?}"
            );
            assert_eq!(
                production_once.is_ok(),
                reference_once.is_ok(),
                "reference/production put domain: {source:?}, {step:?}; \
                 reference={reference_once:?}, production={production_once:?}"
            );
            let (Ok(reference_once), Ok(AnyInstance::Tree(production_once))) =
                (reference_once, production_once)
            else {
                continue;
            };
            assert_eq!(
                production_tree_sequence(&production_once),
                reference_tree_sequence(&reference_once),
                "put result: {source:?}, {step:?}"
            );
            let (reference_again, reference_next) =
                reference_get(&reference_schema, &reference_once, false)
                    .expect("reference PutGet get");
            let (production_again, production_next) = production_resolution
                .get(
                    &AnyInstance::Tree(production_once.clone()),
                    &ResolveOptions::default(),
                    &ResolveContext::empty(),
                )
                .expect("production PutGet get");
            let AnyInstance::Graph(production_again) = production_again else {
                panic!("TreeToGraph returned a non-graph")
            };
            assert_eq!(
                production_tree_targets(&production_again, &production_picked),
                reference_tree_targets(&reference_again, &reference_picked),
                "PutGet targets: {source:?}, {step:?}"
            );
            assert_eq!(
                reference_put(&reference_schema, &reference_again, &reference_next),
                Ok(reference_once),
                "reference gated second put: {source:?}, {step:?}"
            );
            let production_twice = production_resolution
                .put(
                    &AnyInstance::Graph(production_again),
                    &production_next,
                    &ResolveContext::empty(),
                )
                .expect("production gated second put");
            assert!(same_instance(
                &production_twice,
                &AnyInstance::Tree(production_once)
            ));
        }
    }
    assert_eq!(candidate_count, 48);
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Counts {
    sources: usize,
    candidates: usize,
    admitted: usize,
    exact_put_get: usize,
    raw_put_twice_failures: usize,
    gated_put_twice_failures: usize,
}

#[test]
#[allow(clippy::too_many_lines)]
fn all_five_resolution_laws_agree_on_the_exact_s8_scope() {
    let (resolution, picked) = picks();
    let mut reference_counts = Counts::default();
    let mut production_counts = Counts::default();
    for source in indexed_sources() {
        reference_counts.sources += 1;
        production_counts.sources += 1;
        let (declared_schema, declared_source, _) = reference_pick_tree(&source);
        let (declared_view, declared_complement) =
            reference_get(&declared_schema, &declared_source, false)
                .expect("declared reference get");
        assert_eq!(
            reference_put(&declared_schema, &declared_view, &declared_complement),
            Ok(declared_source),
            "declared reference GetPut: {source:?}"
        );
        let (reference_view, reference_index) = indexed_get(&source);
        assert_eq!(
            indexed_put(&reference_view, &reference_index),
            Ok(source.clone())
        );
        let (production_view, production_complement) = production_get(&resolution, &source);
        let production_source = AnyInstance::Table(picks_table(&source));
        let restored = resolution
            .put(
                &AnyInstance::Graph(production_view.clone()),
                &production_complement,
                &ResolveContext::empty(),
            )
            .expect("production GetPut");
        assert!(
            same_instance(&restored, &production_source),
            "GetPut: {source:?}"
        );

        for step in single_step_basis(&reference_view) {
            reference_counts.candidates += 1;
            production_counts.candidates += 1;
            let candidate = apply_single_step(&reference_view, &step);
            let reference_violations = reference_domain(&candidate, &reference_index);
            let production_candidate =
                production_edit(&production_view, &reference_view, &picked, &step);
            let production_errors =
                production_violations(&resolution, &production_candidate, &production_complement);
            assert_eq!(
                production_errors.is_empty(),
                reference_violations.is_empty(),
                "put domain polarity differs for {source:?}, {step:?}: reference={reference_violations:?}, production={production_errors:?}"
            );

            let ids: Vec<u32> = candidate
                .keys()
                .chain(reference_index.edges.keys())
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            for renaming in injective_renamings(&ids, 5) {
                let (renamed_view, renamed_index) =
                    coherent_rename(&candidate, &reference_index, &renaming);
                let expected: Vec<_> = reference_violations
                    .iter()
                    .map(|violation| renamed_reference_violation(violation, &renaming))
                    .collect();
                assert_eq!(
                    reference_domain(&renamed_view, &renamed_index),
                    expected,
                    "reference Congruence: {source:?}, {step:?}, {renaming:?}"
                );
            }

            let production_ids: Vec<u32> = pick_edges(&production_candidate, &picked)
                .into_values()
                .chain(
                    production_complement
                        .occurrences
                        .keys()
                        .filter_map(|carrier| {
                            if let ViewRef::Edge(id) = carrier {
                                Some(*id)
                            } else {
                                None
                            }
                        }),
                )
                .map(|id| id.0)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let production_floor = production_ids.iter().max().copied().unwrap_or(0) + 1;
            for renaming in injective_renamings(&production_ids, 5) {
                let renaming: BTreeMap<_, _> = renaming
                    .into_iter()
                    .map(|(id, image)| (id, production_floor + image))
                    .collect();
                let (renamed_view, renamed_complement) =
                    rename_production(&production_candidate, &production_complement, &renaming);
                let mut expected: Vec<_> = production_errors
                    .iter()
                    .map(|violation| renamed_production_violation(violation, &renaming))
                    .collect();
                expected.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
                expected.dedup();
                assert_eq!(
                    production_violations(&resolution, &renamed_view, &renamed_complement),
                    expected,
                    "production Congruence domain: {source:?}, {step:?}, {renaming:?}"
                );
            }

            let Ok(reference_once) = indexed_put(&candidate, &reference_index) else {
                continue;
            };
            reference_counts.admitted += 1;
            production_counts.admitted += 1;
            let production_once = resolution
                .put(
                    &AnyInstance::Graph(production_candidate.clone()),
                    &production_complement,
                    &ResolveContext::empty(),
                )
                .expect("production Totality");
            let (reference_again, reference_next) = indexed_get(&reference_once);
            assert!(
                equivalent(
                    &candidate,
                    &coherent_rekey(&reference_next, &candidate),
                    &reference_again,
                    &reference_next,
                ),
                "reference PutGet: {source:?}, {step:?}"
            );
            let (production_again, production_next) = resolution
                .get(
                    &production_once,
                    &ResolveOptions::default(),
                    &ResolveContext::empty(),
                )
                .expect("production PutGet");
            let AnyInstance::Graph(production_again) = production_again else {
                panic!("TableToGraph returned a non-graph")
            };
            assert_eq!(
                pick_pairs(&production_candidate, &picked),
                pick_pairs(&production_again, &picked),
                "production PutGet: {source:?}, {step:?}"
            );
            let reference_exact = reference_again == candidate;
            let production_exact = GInstance::identical(&production_again, &production_candidate);
            assert_eq!(reference_exact, production_exact, "exact PutGet: {step:?}");
            reference_counts.exact_put_get += usize::from(reference_exact);
            production_counts.exact_put_get += usize::from(production_exact);

            let raw_reference =
                indexed_put(&candidate, &reference_next) == Ok(reference_once.clone());
            let raw_production = resolution
                .put(
                    &AnyInstance::Graph(production_candidate.clone()),
                    &production_next,
                    &ResolveContext::empty(),
                )
                .is_ok_and(|twice| same_instance(&twice, &production_once));
            reference_counts.raw_put_twice_failures += usize::from(!raw_reference);
            production_counts.raw_put_twice_failures += usize::from(!raw_production);

            assert!(gated_put_twice(&candidate, &reference_index));
            let production_rekeyed =
                rekeyed_complement(&production_next, &production_candidate, &picked);
            let gated_production = resolution
                .put(
                    &AnyInstance::Graph(production_candidate.clone()),
                    &production_rekeyed,
                    &ResolveContext::empty(),
                )
                .is_ok_and(|twice| same_instance(&twice, &production_once));
            production_counts.gated_put_twice_failures += usize::from(!gated_production);
            assert!(gated_production, "gated PutTwice: {source:?}, {step:?}");

            let occurrence_ids: Vec<u32> = pick_edges(&production_candidate, &picked)
                .into_values()
                .chain(
                    production_complement
                        .occurrences
                        .keys()
                        .filter_map(|carrier| {
                            if let ViewRef::Edge(id) = carrier {
                                Some(*id)
                            } else {
                                None
                            }
                        }),
                )
                .map(|id| id.0)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let floor = occurrence_ids.iter().max().copied().unwrap_or(0) + 1;
            for renaming in injective_renamings(&occurrence_ids, 5) {
                let renaming: BTreeMap<_, _> = renaming
                    .into_iter()
                    .map(|(id, image)| (id, floor + image))
                    .collect();
                let (renamed_view, renamed_complement) =
                    rename_production(&production_candidate, &production_complement, &renaming);
                assert!(
                    production_violations(&resolution, &renamed_view, &renamed_complement,)
                        .is_empty(),
                    "production Congruence domain: {step:?}, {renaming:?}"
                );
                let renamed_put = resolution
                    .put(
                        &AnyInstance::Graph(renamed_view),
                        &renamed_complement,
                        &ResolveContext::empty(),
                    )
                    .expect("production Congruence put");
                assert!(
                    same_instance(&renamed_put, &production_once),
                    "production Congruence result: {step:?}, {renaming:?}"
                );
            }
        }
    }
    assert_eq!(
        reference_counts,
        Counts {
            sources: 8,
            candidates: 48,
            admitted: 24,
            exact_put_get: 14,
            raw_put_twice_failures: 6,
            gated_put_twice_failures: 0,
        }
    );
    assert_eq!(production_counts.sources, reference_counts.sources);
    assert_eq!(production_counts.candidates, reference_counts.candidates);
    assert_eq!(production_counts.admitted, reference_counts.admitted);
    assert_eq!(
        production_counts.exact_put_get,
        reference_counts.exact_put_get
    );
    assert_eq!(production_counts.gated_put_twice_failures, 0);
}

proptest::proptest! {
    #[test]
    fn coherent_reference_renaming_is_a_property_of_every_generated_state(
        bits in 0u8..8,
        offset in 8u32..10_000,
    ) {
        let source = indexed_sources().remove(usize::from(bits));
        let (view, index) = indexed_get(&source);
        for step in single_step_basis(&view) {
            let candidate = apply_single_step(&view, &step);
            let ids: Vec<u32> = candidate
                .keys()
                .chain(index.edges.keys())
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let renaming: BTreeMap<_, _> = ids
                .iter()
                .enumerate()
                .map(|(rank, id)| (*id, offset + u32::try_from(rank).expect("bounded rank")))
                .collect();
            let expected: Vec<_> = reference_domain(&candidate, &index)
                .iter()
                .map(|violation| renamed_reference_violation(violation, &renaming))
                .collect();
            let (renamed, renamed_index) = coherent_rename(&candidate, &index, &renaming);
            proptest::prop_assert_eq!(reference_domain(&renamed, &renamed_index), expected);
        }
    }
}

#[test]
fn the_reference_put_domain_is_not_defined_by_reference_put() {
    let source: IndexedSource = std::iter::once("b".to_owned()).collect();
    let (view, index) = indexed_get(&source);
    let edited = apply_single_step(
        &view,
        &SingleStep::Retarget {
            id: 0,
            target: "a".to_owned(),
        },
    );
    assert_eq!(
        reference_domain(&edited, &index),
        vec![ResolveDomainViolation::SynthesizedEdited(0)]
    );
    assert!(indexed_put(&edited, &index).is_err());
}

#[test]
fn synth_index_is_a_stable_key_not_a_numeric_identity() {
    let source: IndexedSource = ["a".to_owned(), "c".to_owned()].into_iter().collect();
    let (view, index) = indexed_get(&source);
    let renaming = BTreeMap::from([(0, 9), (1, 4)]);
    let (renamed, renamed_index) = coherent_rename(&view, &index, &renaming);
    assert!(equivalent(&view, &index, &renamed, &renamed_index));
    assert_ne!(view, renamed);
    assert_ne!(index, SynthIndex::default());
}

#[test]
fn uri_resolution_matches_the_rfc_3986_section_5_4_vectors() {
    let base = "http://a/b/c/d;p?q";
    let vectors = [
        ("g:h", "g:h"),
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("g#s", "http://a/b/c/g#s"),
        ("g?y#s", "http://a/b/c/g?y#s"),
        (";x", "http://a/b/c/;x"),
        ("g;x", "http://a/b/c/g;x"),
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("./", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../..", "http://a/"),
        ("../../", "http://a/"),
        ("../../g", "http://a/g"),
        ("../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("/../g", "http://a/g"),
        ("g.", "http://a/b/c/g."),
        (".g", "http://a/b/c/.g"),
        ("g..", "http://a/b/c/g.."),
        ("..g", "http://a/b/c/..g"),
        ("./../g", "http://a/b/g"),
        ("./g/.", "http://a/b/c/g/"),
        ("g/./h", "http://a/b/c/g/h"),
        ("g/../h", "http://a/b/c/h"),
        ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/./x", "http://a/b/c/g?y/./x"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
        ("g#s/./x", "http://a/b/c/g#s/./x"),
        ("g#s/../x", "http://a/b/c/g#s/../x"),
    ];
    for (reference, expected) in vectors {
        assert_eq!(
            resolve_uri_reference(base, reference).as_deref(),
            Some(expected),
            "production {reference}"
        );
        assert_eq!(
            rfc3986(base, reference).as_deref(),
            Some(expected),
            "{reference}"
        );
    }
    assert_eq!(rfc3986("relative/base", "child"), None);
    assert_eq!(resolve_uri_reference("relative/base", "child"), None);
}

#[test]
fn uri_environment_uses_only_the_preceding_directive_prefix() {
    let directives = [
        "https://example.test/a/b".to_owned(),
        "../c".to_owned(),
        "d".to_owned(),
    ];
    let expected = [
        None,
        Some("https://example.test/a/b".to_owned()),
        Some("https://example.test/c".to_owned()),
        Some("https://example.test/d".to_owned()),
    ];
    for (prefix_len, expected) in expected.into_iter().enumerate() {
        assert_eq!(
            uri_environment(&directives[..prefix_len]),
            expected,
            "prefix length {prefix_len}"
        );
        assert_eq!(
            fold_uri_environment(&directives[..prefix_len]),
            expected,
            "production prefix length {prefix_len}"
        );
    }
    assert_eq!(uri_environment(&["relative".to_owned()]), None);

    let sequences = uri_directive_sequences();
    assert_eq!(sequences.len(), 1 + 3 + 9 + 27);
    for sequence in sequences {
        let expected = sequence.iter().try_fold(None, |base, directive| {
            if directive.contains("://") {
                Some(Some(directive.clone()))
            } else {
                Some(Some(rfc3986(base.as_deref()?, directive)?))
            }
        });
        assert_eq!(
            uri_environment(&sequence),
            expected.flatten(),
            "{sequence:?}"
        );
        assert_eq!(
            fold_uri_environment(&sequence),
            uri_environment(&sequence),
            "production {sequence:?}"
        );
    }
}

#[test]
fn referenced_text_identity_is_field_qualified_and_value_independent() {
    let left = text_identity(7, "left", None);
    let left_again = text_identity(7, "left", None);
    let right = text_identity(7, "right", None);
    let external = text_identity(7, "left", Some(("route-a", b"key-a")));
    let external_other_context = text_identity(7, "left", Some(("route-b", b"key-b")));
    let site = panproto_inst::ResolutionValueSite::Field {
        holder: ViewRef::Node(7),
        field: "left".to_owned(),
    };
    let production_left = resolution_text_identity(site.clone(), false);
    let production_left_again = resolution_text_identity(site.clone(), false);
    let production_right = resolution_text_identity(
        panproto_inst::ResolutionValueSite::Field {
            holder: ViewRef::Node(7),
            field: "right".to_owned(),
        },
        false,
    );
    let production_external = resolution_text_identity(site.clone(), true);
    let production_external_other_context = resolution_text_identity(site.clone(), true);
    assert!(production_left.is_local());
    assert_eq!(production_left.site(), &site);
    assert!(!production_external.is_local());
    assert_eq!(production_external.site(), &site);
    assert_eq!(production_left == production_left_again, left == left_again);
    assert_eq!(production_left == production_right, left == right);
    assert_eq!(production_left == production_external, left == external);
    assert_eq!(
        production_external == production_external_other_context,
        external == external_other_context
    );
    assert_eq!(left, left_again);
    assert_ne!(left, right);
    assert_ne!(left, external);
    assert_eq!(external, external_other_context);
}

#[test]
fn guarded_occurrences_select_the_first_representation_equal_guard() {
    let guards = vec![
        ("first".to_owned(), ReferenceValue::Int(1)),
        ("second".to_owned(), ReferenceValue::Int(1)),
        ("other".to_owned(), ReferenceValue::Int(2)),
    ];
    assert_eq!(
        guarded_partition(&guards, &ReferenceValue::Int(1)),
        Some("first")
    );
    assert_eq!(
        guarded_partition(&guards, &ReferenceValue::Int(2)),
        Some("other")
    );
    assert_eq!(guarded_partition(&guards, &ReferenceValue::Int(3)), None);

    let (resolution, left, right) = guarded_picks();
    let production_guards = vec![
        ("left".to_owned(), ReferenceValue::Str("left".to_owned())),
        ("right".to_owned(), ReferenceValue::Str("right".to_owned())),
    ];
    for kind in ["left", "right", "other"] {
        let source = guarded_pick_tree(kind);
        let (view, complement) = resolution
            .get(
                &AnyInstance::Tree(source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .expect("guarded get succeeds");
        let AnyInstance::Graph(view) = view else {
            panic!("TreeToGraph returned a non-graph")
        };
        let selected = guarded_partition(&production_guards, &ReferenceValue::Str(kind.to_owned()));
        let has_left = view
            .edges_sorted()
            .any(|(_, candidate)| candidate.edge == left);
        let has_right = view
            .edges_sorted()
            .any(|(_, candidate)| candidate.edge == right);
        assert_eq!(has_left, selected == Some("left"), "{kind}");
        assert_eq!(has_right, selected == Some("right"), "{kind}");
        match selected {
            Some(_) => assert!(complement.unresolved.is_empty(), "{kind}"),
            None => assert!(matches!(
                complement.unresolved.as_slice(),
                [record]
                    if record.reference == "left"
                        && record.literal.canonical_cmp(&text(kind)).is_eq()
                        && matches!(
                            &record.reason,
                            UnresolvedReason::Unselected { value }
                                if value.canonical_cmp(&text(kind)).is_eq()
                        )
            )),
        }
        let restored = resolution
            .put(
                &AnyInstance::Graph(view),
                &complement,
                &ResolveContext::empty(),
            )
            .expect("guarded GetPut");
        assert!(same_instance(&restored, &AnyInstance::Tree(source)));
    }
}

#[test]
fn standoff_forms_are_injective_and_the_inverse_is_left_inverse_on_the_exact_scope() {
    use panproto_reference::enumerate::resolution::standoff_coordinates;

    let mut cases = 0usize;
    for origin in [0, 1, 2, u64::MAX] {
        for end_is_length in [false, true] {
            let mut images = BTreeMap::new();
            for (written_start, written_end, candidate_origin, candidate_form) in
                standoff_coordinates()
            {
                if candidate_origin != origin || candidate_form != end_is_length {
                    continue;
                }
                cases += 1;
                let end_form = if end_is_length {
                    SpanEnd::Length
                } else {
                    SpanEnd::Offset
                };
                if let Some(span) = standoff_form(written_start, written_end, origin, end_is_length)
                {
                    assert_eq!(
                        decode_standoff_form(written_start, written_end, origin, end_form),
                        Ok(span),
                        "production decode at {written_start}, {written_end}, {origin}, {end_is_length}"
                    );
                    assert_eq!(
                        images.insert(span, (written_start, written_end)),
                        None,
                        "two written forms mapped to {span:?} at origin={origin}, length={end_is_length}"
                    );
                    assert_eq!(
                        standoff_inverse(span.0, span.1, origin, end_is_length),
                        Some((written_start, written_end))
                    );
                    assert_eq!(
                        encode_standoff_form(span.0, span.1, origin, end_form),
                        Ok((written_start, written_end)),
                        "production encode at {span:?}, {origin}, {end_is_length}"
                    );
                } else {
                    assert!(
                        decode_standoff_form(written_start, written_end, origin, end_form).is_err(),
                        "production admitted a refused form at {written_start}, {written_end}, {origin}, {end_is_length}"
                    );
                }
            }
        }
    }
    assert_eq!(cases, 128);
}

proptest::proptest! {
    #[test]
    fn standoff_inverse_property_holds_without_overflow(
        start in 0u32..1_000_000,
        length in 0u32..1_000_000,
        origin in 0u32..1_000_000,
        end_is_length in proptest::bool::ANY,
    ) {
        let start = u64::from(start);
        let end = start + u64::from(length);
        let origin = u64::from(origin);
        let written = standoff_inverse(start, end, origin, end_is_length)
            .expect("bounded arithmetic cannot overflow");
        let end_form = if end_is_length {
            SpanEnd::Length
        } else {
            SpanEnd::Offset
        };
        proptest::prop_assert_eq!(
            encode_standoff_form(start, end, origin, end_form),
            Ok(written)
        );
        proptest::prop_assert_eq!(
            decode_standoff_form(written.0, written.1, origin, end_form),
            Ok((start, end))
        );
        proptest::prop_assert_eq!(
            standoff_form(written.0, written.1, origin, end_is_length),
            Some((start, end))
        );
    }
}

#[test]
fn index_fields_never_resolve_targets() {
    for index in [None, Some(0), Some(1), Some(u64::MAX)] {
        for target in [None, Some(0), Some(1), Some(u64::MAX)] {
            assert!(!index_target(index, target), "{index:?}, {target:?}");
        }
    }

    let protocol = protocol("reference-index-target");
    let resolved = edge("holder", "target", "ref", Some("resolved"));
    let field = prop("target", "scalar", "index");
    let source = prop("holder", "scalar", "target");
    let schema = SchemaBuilder::new(&protocol)
        .vertex("holder", "table", None)
        .unwrap()
        .vertex("target", "table", None)
        .unwrap()
        .vertex("scalar", "string", None)
        .unwrap()
        .edge("target", "scalar", "prop", Some("index"))
        .unwrap()
        .edge("holder", "scalar", "prop", Some("target"))
        .unwrap()
        .edge("holder", "target", "ref", Some("resolved"))
        .unwrap()
        .key(
            "target",
            Key {
                id: "index".into(),
                parts: vec![KeyPart::Path(vec![field])],
                role: KeyRole::Index,
                scope: KeyScope::Document,
                equality: Vec::new(),
            },
        )
        .reference(Reference {
            id: "index-reference".into(),
            holder: "holder".into(),
            source: RefSource::KeyValue {
                parts: vec![vec![source]],
            },
            target_vertex: "target".into(),
            target_key: Some("index".into()),
            edge: Some(resolved.clone()),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: NullMatch::default(),
            null_vertex: None,
            scope_via: None,
        })
        .build()
        .unwrap();
    let provider_id = ResolutionProviderId {
        name: "reference-index-target".into(),
        version: 1,
    };
    let spec = ResolutionSpec {
        kind: ResolutionKind::TableToGraph,
        provider: provider_id,
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: schema
            .edges
            .keys()
            .filter(|candidate| **candidate != resolved)
            .cloned()
            .collect(),
        reference_writers: BTreeMap::from([(resolved, "index-reference".into())]),
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    let provider = Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TableToGraph,
    };
    assert!(matches!(
        Resolution::for_protocol(&protocol, &schema, Arc::new(provider)),
        Err(ResolveError::Schema(KeyRefError::KeyNotResolvable { reference, key }))
            if reference == "index-reference" && key == "index"
    ));
}

#[test]
fn inherited_base_uses_the_nearest_present_ancestor() {
    let bases = vec![
        Some("https://outer.test/a/".to_owned()),
        None,
        Some("https://inner.test/b/".to_owned()),
        None,
    ];
    assert_eq!(
        inherited_base(&bases, "../c"),
        Some("https://inner.test/c".to_owned())
    );
    assert_eq!(
        resolve_inherited_uri_base(&bases, "../c"),
        inherited_base(&bases, "../c")
    );
    assert_eq!(inherited_base(&[None, None], "c"), None);
    assert_eq!(resolve_inherited_uri_base(&[None, None], "c"), None);

    let ancestors = panproto_reference::enumerate::resolution::sequences(&[
        None,
        Some("https://outer.test/a/".to_owned()),
        Some("relative/".to_owned()),
    ]);
    for bases in ancestors {
        for reference in ["c", "../d", "https://absolute.test/e"] {
            assert_eq!(
                resolve_inherited_uri_base(&bases, reference),
                inherited_base(&bases, reference),
                "{bases:?}, {reference}"
            );
            let Some(nearest) = bases.iter().rposition(Option::is_some) else {
                continue;
            };
            let mut changed = bases.clone();
            changed[..nearest].fill(Some("https://irrelevant.test/changed/".to_owned()));
            assert_eq!(
                resolve_inherited_uri_base(&changed, reference),
                resolve_inherited_uri_base(&bases, reference),
                "non-nearest bases: {bases:?}, {reference}"
            );
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn resolution_uses_the_exact_declared_key_equality_operation() {
    use panproto_reference::keys::{
        KeyEquality as ReferenceEquality, NumericPrimitive as ReferenceNumeric,
        SqlCollation as ReferenceCollation, SqlPad as ReferencePad,
    };
    use panproto_schema::{
        KeyEquality as ProductionEquality, NumericPrimitive as ProductionNumeric,
        SqlCollation as ProductionCollation, SqlPad as ProductionPad,
    };

    let equalities = [
        (
            ReferenceEquality::Representation,
            ProductionEquality::Representation,
        ),
        (
            ReferenceEquality::NumericValue,
            ProductionEquality::NumericValue,
        ),
        (
            ReferenceEquality::NumericValueWithinKind(ReferenceNumeric::Any),
            ProductionEquality::NumericValueWithinKind(ProductionNumeric::Any),
        ),
        (
            ReferenceEquality::Sql {
                collation: ReferenceCollation::AsciiCaseInsensitive,
                pad: ReferencePad::PadSpace,
            },
            ProductionEquality::Sql {
                collation: ProductionCollation::AsciiCaseInsensitive,
                pad: ProductionPad::PadSpace,
            },
        ),
        (
            ReferenceEquality::Sql {
                collation: ReferenceCollation::Named("database".to_owned()),
                pad: ReferencePad::NoPad,
            },
            ProductionEquality::Sql {
                collation: ProductionCollation::Named("database".into()),
                pad: ProductionPad::NoPad,
            },
        ),
        (
            ReferenceEquality::XmlNonCdata,
            ProductionEquality::XmlNonCdata,
        ),
    ];
    let values = [
        (ReferenceValue::Int(1), Value::Int(1)),
        (ReferenceValue::Float(1.0f64.to_bits()), Value::Float(1.0)),
        (ReferenceValue::Str("A".to_owned()), text("A")),
        (ReferenceValue::Str("a ".to_owned()), text("a ")),
        (ReferenceValue::Str("a\tb".to_owned()), text("a\tb")),
        (ReferenceValue::Str("a b".to_owned()), text("a b")),
        (ReferenceValue::Null, Value::Null),
    ];
    for (reference_equality, production_equality) in &equalities {
        for (reference_left, production_left) in &values {
            for (reference_right, production_right) in &values {
                let expected =
                    declared_key_equality(reference_equality, reference_left, reference_right);
                let actual = panproto_inst::key::key_equivalence(
                    production_equality,
                    production_left,
                    production_right,
                );
                assert_eq!(
                    actual.as_ref().ok().copied(),
                    expected,
                    "{reference_equality:?}, {reference_left:?}, {reference_right:?}"
                );
                assert_eq!(
                    actual.is_err(),
                    expected.is_none(),
                    "refusal polarity: {reference_equality:?}, {reference_left:?}, {reference_right:?}"
                );
            }
        }
    }

    assert_eq!(
        declared_key_equality(
            &ReferenceEquality::NumericValue,
            &ReferenceValue::Float(1.0f64.to_bits()),
            &ReferenceValue::Int(1),
        ),
        Some(true)
    );
    let source = numeric_pick_tree();
    let (numeric, picked) = tree_picks_with_equality(vec![ProductionEquality::NumericValue]);
    let (view, _) = numeric
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .expect("numeric-equality resolution");
    let AnyInstance::Graph(view) = view else {
        panic!("TreeToGraph returned a non-graph")
    };
    assert!(
        view.edges_sorted()
            .any(|(_, edge)| edge.edge == picked && edge.src == 1 && edge.tgt == 2)
    );

    let (representation, picked) =
        tree_picks_with_equality(vec![ProductionEquality::Representation]);
    let (view, complement) = representation
        .get(
            &AnyInstance::Tree(source),
            &ResolveOptions { lenient: true },
            &ResolveContext::empty(),
        )
        .expect("representation-equality lenient resolution");
    let AnyInstance::Graph(view) = view else {
        panic!("TreeToGraph returned a non-graph")
    };
    assert!(!view.edges_sorted().any(|(_, edge)| edge.edge == picked));
    assert!(matches!(
        complement.unresolved.as_slice(),
        [record] if matches!(record.reason, UnresolvedReason::Dangling)
    ));
}

#[test]
fn composite_foreign_key_null_matching_uses_real_resolution_occurrences() {
    let cases = [
        (
            Value::Int(1),
            Value::Int(2),
            ReferenceValue::Int(1),
            ReferenceValue::Int(2),
            NullOutcome::Resolved,
            NullOutcome::Resolved,
        ),
        (
            Value::Int(1),
            Value::Int(3),
            ReferenceValue::Int(1),
            ReferenceValue::Int(3),
            NullOutcome::Dangling,
            NullOutcome::Dangling,
        ),
        (
            Value::Null,
            Value::Null,
            ReferenceValue::Null,
            ReferenceValue::Null,
            NullOutcome::Absent,
            NullOutcome::Absent,
        ),
        (
            Value::Int(1),
            Value::Null,
            ReferenceValue::Int(1),
            ReferenceValue::Null,
            NullOutcome::Absent,
            NullOutcome::Dangling,
        ),
        (
            Value::Null,
            Value::Int(2),
            ReferenceValue::Null,
            ReferenceValue::Int(2),
            NullOutcome::Absent,
            NullOutcome::Dangling,
        ),
    ];
    for (left, right, reference_left, reference_right, simple, full) in cases {
        for (production_mode, reference_mode, expected) in [
            (
                NullMatch::Simple,
                reference_schema::NullMatch::Simple,
                simple,
            ),
            (NullMatch::Full, reference_schema::NullMatch::Full, full),
        ] {
            let production = production_null_outcome(production_mode, left.clone(), right.clone());
            let reference = reference_null_outcome(
                reference_mode,
                reference_left.clone(),
                reference_right.clone(),
            );
            assert_eq!(
                production.0, expected,
                "{production_mode:?}: {left:?}, {right:?}"
            );
            assert_eq!(
                reference.0, expected,
                "{reference_mode:?}: {left:?}, {right:?}"
            );
            assert_eq!(production.0, reference.0);
            if expected == NullOutcome::Dangling {
                assert_eq!(
                    production.1,
                    Some(Value::List(vec![left.clone(), right.clone()]))
                );
                assert_eq!(
                    reference.1,
                    Some(ReferenceValue::List(vec![
                        reference_left.clone(),
                        reference_right.clone(),
                    ]))
                );
            } else {
                assert_eq!(production.1, None);
                assert_eq!(reference.1, None);
            }
        }
    }
}

#[test]
fn nearest_preceding_binding_ignores_later_and_other_named_definitions() {
    let definitions = vec![
        (0, "x".to_owned(), 10),
        (2, "y".to_owned(), 20),
        (3, "x".to_owned(), 30),
        (7, "x".to_owned(), 70),
    ];
    assert_eq!(nearest_preceding(&definitions, 6, "x", true), Some(30));
    assert_eq!(nearest_preceding(&definitions, 6, "x", false), Some(10));
    assert_eq!(nearest_preceding(&definitions, 0, "x", true), None);
    assert_eq!(nearest_preceding(&definitions, 6, "z", true), None);

    let source = variable_tree();
    for (policy, expected) in [
        (VariableBinding::FirstDefines, vec![(3, 1), (4, 1)]),
        (VariableBinding::NearestPreceding, vec![(3, 1), (4, 3)]),
    ] {
        let (resolution, binding) = variable_resolution(policy);
        let (view, complement) = resolution
            .get(
                &AnyInstance::Tree(source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .expect("variable-binding get succeeds");
        let AnyInstance::Graph(view) = view else {
            panic!("TreeToGraph returned a non-graph")
        };
        let mut bindings: Vec<_> = view
            .edges_sorted()
            .filter(|(_, candidate)| candidate.edge == binding)
            .map(|(_, candidate)| (candidate.src, candidate.tgt))
            .collect();
        bindings.sort_unstable();
        assert_eq!(bindings, expected);

        let restored = resolution
            .put(
                &AnyInstance::Graph(view),
                &complement,
                &ResolveContext::empty(),
            )
            .expect("variable-binding GetPut");
        assert!(same_instance(&restored, &AnyInstance::Tree(source.clone())));
    }
    assert_eq!(nearest_preceding(&definitions, 4, "x", true), Some(30));
}

#[test]
fn preserving_registry_is_the_ordered_cartesian_product() {
    let choices = vec![vec!["a", "b"], vec!["x"], vec!["0", "1", "2"]];
    let product = preserving_registry(&choices);
    assert_eq!(product.len(), 6);
    assert_eq!(
        product,
        vec![
            vec!["a", "x", "0"],
            vec!["a", "x", "1"],
            vec!["a", "x", "2"],
            vec!["b", "x", "0"],
            vec!["b", "x", "1"],
            vec!["b", "x", "2"],
        ]
    );
    assert_eq!(preserving_registry::<u8>(&[]), vec![Vec::<u8>::new()]);
    assert!(preserving_registry(&[vec![1u8], Vec::new()]).is_empty());
}
