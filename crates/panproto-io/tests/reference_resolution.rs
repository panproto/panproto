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
    indexed_sources, inherited_base_chains, injective_renamings, single_step_basis,
    uri_directive_sequences, uri_written_forms,
};
use panproto_reference::resolution::{
    IndexedSource, IndexedView, ResolveDomainViolation, SingleStep, SynthIndex, apply_single_step,
    check_put_domain as reference_domain, coherent_rekey, coherent_rename, declared_key_equality,
    equivalent, gated_put_twice, get as reference_get, guarded_partition, indexed_get, indexed_put,
    inherited_base, nearest_preceding, preserving_registry, put as reference_put, rfc3986,
    standoff_form, standoff_inverse, text_identity, uri_environment,
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
    try_tree_picks_with_equality(equality).unwrap()
}

/// The tree-picks resolution whose target key `cid` declares `equality`.
/// The declaration is installed after building, so a malformed equality
/// vector reaches resolution compilation instead of the builder.
fn try_tree_picks_with_equality(
    equality: Vec<KeyEquality>,
) -> Result<(Resolution, Edge), ResolveError> {
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
                equality: Vec::new(),
            },
        )
        .reference(Reference {
            cardinality: Cardinality::Many,
            nullable: true,
            ..reference(picked.clone())
        })
        .build()
        .unwrap();
    let mut schema = schema;
    schema.declarations.insert_key(
        "customer".into(),
        Key {
            id: "cid".into(),
            parts: vec![KeyPart::Path(vec![prop("customer", "label", "cid")])],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality,
        },
    );
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
    let resolution = Resolution::for_protocol(&protocol, &schema, Arc::new(provider))?;
    Ok((resolution, picked))
}

fn variable_resolution(binding: VariableBinding) -> (Resolution, Edge) {
    variable_resolution_with(binding, Vec::new())
}

/// The variable-binding resolution whose binder key declares `equality`.
fn variable_resolution_with(
    binding: VariableBinding,
    equality: Vec<KeyEquality>,
) -> (Resolution, Edge) {
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
                equality,
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
    rename_view_state(view, complement, &BTreeMap::new(), renaming)
}

/// Rename view node ids by `nodes` and view edge ids by `edges` in the view
/// and in every complement coordinate that denotes a view carrier: occurrence
/// carriers and holders, definition targets and holders, child orders,
/// spanning parents, span nodes with their nested text sites, and unresolved
/// holders and candidates. Rows, arcs and other source coordinates are not
/// view ids and stay as they are.
fn rename_view_state(
    view: &GInstance,
    complement: &ResolutionComplement,
    nodes: &BTreeMap<u32, u32>,
    edges: &BTreeMap<u32, u32>,
) -> (GInstance, ResolutionComplement) {
    let node = |id: u32| nodes.get(&id).copied().unwrap_or(id);
    let edge = |id: panproto_inst::EdgeId| {
        panproto_inst::EdgeId(edges.get(&id.0).copied().unwrap_or(id.0))
    };
    let carrier = |reference: &ViewRef| match reference {
        ViewRef::Node(id) => ViewRef::Node(node(*id)),
        ViewRef::Edge(id) => ViewRef::Edge(edge(*id)),
        other => other.clone(),
    };
    let site = |site: &panproto_inst::ResolutionValueSite| site.with_holder(carrier(site.holder()));
    let mut renamed = GInstance::new();
    for (id, value) in view.nodes() {
        let mut value = value.clone();
        value.id = node(*id);
        renamed.insert_node(value).expect("injective node renaming");
    }
    for (id, value) in view.edges_sorted() {
        renamed
            .insert_edge(
                edge(id),
                panproto_inst::GEdge {
                    src: node(value.src),
                    tgt: node(value.tgt),
                    edge: value.edge.clone(),
                    position: value.position,
                },
            )
            .expect("injective edge renaming");
    }
    let mut next = complement.clone();
    next.occurrences = complement
        .occurrences
        .iter()
        .map(|(key, records)| {
            let records = records
                .iter()
                .map(|record| {
                    let mut record = record.clone();
                    record.holder = carrier(&record.holder);
                    record
                })
                .collect();
            (carrier(key), records)
        })
        .collect();
    next.definitions = complement
        .definitions
        .iter()
        .map(|(key, holder)| {
            let mut key = key.clone();
            key.target = node(key.target);
            (key, carrier(holder))
        })
        .collect();
    next.order = complement
        .order
        .iter()
        .map(|(parent, children)| (node(*parent), children.iter().map(carrier).collect()))
        .collect();
    next.spanning = complement
        .spanning
        .iter()
        .map(|(child, parent)| (node(*child), carrier(parent)))
        .collect();
    next.span_nodes = complement
        .span_nodes
        .iter()
        .map(|(id, span)| {
            let mut span = span.clone();
            span.text = span.text.with_site(site(span.text.site()));
            (node(*id), span)
        })
        .collect();
    for record in &mut next.unresolved {
        record.holder = carrier(&record.holder);
        if let UnresolvedReason::Ambiguous { candidates } = &mut record.reason {
            for candidate in candidates.iter_mut() {
                *candidate = carrier(candidate);
            }
            candidates.sort();
        }
    }
    next.unresolved.sort_by(|a, b| {
        (&a.holder, &a.reference, a.index).cmp(&(&b.holder, &b.reference, b.index))
    });
    (renamed, next)
}

/// A tree with its node ids transported by `nodes`: the induced source map
/// of a view renaming on a tree-to-graph resolution.
fn rename_tree(tree: &WInstance, nodes: &BTreeMap<u32, u32>) -> WInstance {
    let node = |id: u32| nodes.get(&id).copied().unwrap_or(id);
    WInstance::new(
        tree.nodes
            .values()
            .map(|value| {
                let mut value = value.clone();
                value.id = node(value.id);
                (value.id, value)
            })
            .collect(),
        tree.arcs
            .iter()
            .map(|(parent, child, label)| (node(*parent), node(*child), label.clone()))
            .collect(),
        tree.fans.clone(),
        node(tree.root),
        tree.schema_root.clone(),
    )
}

/// Production `PutGet` on the picks carrier compared exactly modulo the
/// synthesized resolution edges: every node with its fields, values and
/// positions, every non-resolution edge by id, and the resolution edges as
/// the multiset of their (source, target, position), which is the coherent
/// renaming through their synthesis keys (one occurrence per pick row).
fn production_put_get_equal(candidate: &GInstance, again: &GInstance, picked: &Edge) -> bool {
    let nodes = |view: &GInstance| -> BTreeMap<u32, serde_json::Value> {
        view.nodes()
            .iter()
            .map(|(id, node)| (*id, serde_json::to_value(node).expect("serialize node")))
            .collect()
    };
    let structural = |view: &GInstance| -> Vec<(u32, u32, u32, Edge, Option<u32>)> {
        view.edges_sorted()
            .filter(|(_, edge)| edge.edge != *picked)
            .map(|(id, edge)| (id.0, edge.src, edge.tgt, edge.edge.clone(), edge.position))
            .collect()
    };
    let synthesized = |view: &GInstance| -> Vec<(u32, u32, Option<u32>)> {
        let mut out: Vec<_> = view
            .edges_sorted()
            .filter(|(_, edge)| edge.edge == *picked)
            .map(|(_, edge)| (edge.src, edge.tgt, edge.position))
            .collect();
        out.sort_unstable();
        out
    };
    let one_per_holder = |view: &GInstance| {
        let holders: Vec<u32> = synthesized(view).iter().map(|(src, _, _)| *src).collect();
        holders.windows(2).all(|pair| pair[0] != pair[1])
    };
    one_per_holder(candidate)
        && one_per_holder(again)
        && nodes(candidate) == nodes(again)
        && structural(candidate) == structural(again)
        && synthesized(candidate) == synthesized(again)
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

/// How a test declaration reads its occurrences.
#[derive(Clone, Copy, Debug)]
enum DeclaredSource {
    KeyValue,
    Inline,
    /// A standoff source: whether it has fragments, and its text form
    /// (0 local, 1 span-relative, 2 referenced, 3 URI-named).
    Standoff(bool, u8),
}

/// One declaration over every combination of the deferred forms, written
/// once for production and once for the reference.
fn deferred_declaration(
    source: DeclaredSource,
    target_set: bool,
    null_vertex: bool,
    scope_via: bool,
) -> (Schema, reference_schema::Schema) {
    use panproto_reference::schema as rs;
    let field = |name: &str| prop("note", "label", name);
    let sfield = |name: &str| reference_model::SEdge::new("note", "label", "prop", Some(name));
    let (production_source, reference_source) = match source {
        DeclaredSource::KeyValue => (
            RefSource::KeyValue {
                parts: vec![vec![field("key")]],
            },
            rs::RefSource::KeyValue {
                parts: vec![vec![sfield("key")]],
            },
        ),
        DeclaredSource::Inline => (
            RefSource::Inline {
                field: vec![field("inline")],
            },
            rs::RefSource::Inline {
                field: vec![sfield("inline")],
            },
        ),
        DeclaredSource::Standoff(fragments, text) => {
            let (production_text, reference_text) = match text {
                0 => (
                    panproto_schema::RefTextSource::Local(vec![field("text")]),
                    rs::RefTextSource::Local(vec![sfield("text")]),
                ),
                1 => (
                    panproto_schema::RefTextSource::Span {
                        reference: "outer".into(),
                    },
                    rs::RefTextSource::Span {
                        reference: "outer".to_owned(),
                    },
                ),
                2 => (
                    panproto_schema::RefTextSource::Referenced {
                        reference: "sofa".into(),
                        text: vec![field("text")],
                        external: None,
                    },
                    rs::RefTextSource::Referenced {
                        reference: "sofa".to_owned(),
                        text: vec![sfield("text")],
                        external: None,
                    },
                ),
                _ => (
                    panproto_schema::RefTextSource::ExternalUri {
                        field: vec![field("href")],
                        base: panproto_schema::UriBase::Absolute,
                    },
                    rs::RefTextSource::ExternalUri {
                        field: vec![sfield("href")],
                        base: rs::UriBase::Absolute,
                    },
                ),
            };
            (
                RefSource::Standoff {
                    text: production_text,
                    start: vec![field("start")],
                    end: vec![field("end")],
                    unit: panproto_schema::OffsetUnit::Byte,
                    end_inclusive: false,
                    end_form: SpanEnd::Offset,
                    origin: 0,
                    fragments: fragments
                        .then(|| edge("note", "fragment", "prop", Some("fragments"))),
                },
                rs::RefSource::Standoff {
                    text: reference_text,
                    start: vec![sfield("start")],
                    end: vec![sfield("end")],
                    unit: rs::OffsetUnit::Byte,
                    end_inclusive: false,
                    end_form: rs::SpanEnd::Offset,
                    origin: 0,
                    fragments: fragments.then(|| {
                        reference_model::SEdge::new("note", "fragment", "prop", Some("fragments"))
                    }),
                },
            )
        }
    };
    let production = Reference {
        id: "mention".into(),
        holder: "note".into(),
        source: production_source,
        target_vertex: "target".into(),
        target_key: None,
        edge: Some(edge("note", "target", "ref", Some("mention"))),
        cardinality: Cardinality::Many,
        nullable: true,
        on_unresolved: OnUnresolved::Complement,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: NullMatch::default(),
        null_vertex: null_vertex.then(|| "null".into()),
        scope_via: scope_via.then(|| panproto_schema::ScopeVia {
            ancestor: "doc".into(),
            hops: vec!["hop".into()],
        }),
    };
    let reference = rs::Reference {
        id: "mention".to_owned(),
        holder: "note".to_owned(),
        source: reference_source,
        target_vertex: "target".to_owned(),
        target_key: None,
        edge: Some(reference_model::SEdge::new(
            "note",
            "target",
            "ref",
            Some("mention"),
        )),
        cardinality: rs::Cardinality::Many,
        nullable: true,
        on_unresolved: rs::OnUnresolved::Complement,
        mode: rs::ResolveMode::AddEdge,
        guard: None,
        null_match: rs::NullMatch::Simple,
        null_vertex: null_vertex.then(|| "null".to_owned()),
        scope_via: scope_via.then(|| rs::ScopeVia {
            ancestor: "doc".to_owned(),
            hops: vec!["hop".to_owned()],
        }),
    };
    let mut production_schema = Schema::default();
    production_schema.declarations.insert_reference(production);
    let mut reference_schema = rs::Schema::default();
    reference_schema
        .references
        .insert("mention".to_owned(), reference);
    if target_set {
        production_schema.declarations.insert_target_set(
            "target".into(),
            panproto_schema::TargetSet {
                members: Vec::new(),
                scope: KeyScope::Document,
            },
        );
        reference_schema.target_sets.insert(
            "target".to_owned(),
            rs::TargetSet {
                members: Vec::new(),
                scope: rs::KeyScope::Document,
            },
        );
    }
    (production_schema, reference_schema)
}

#[test]
fn deferred_forms_are_refused_by_production_exactly_where_the_reference_declares_them() {
    use panproto_io::resolution::ResolutionFeature;
    use panproto_reference::resolution::{DeferredForm, deferred_form};

    let feature_of = |form: DeferredForm| match form {
        DeferredForm::InlineReference => ResolutionFeature::InlineReference,
        DeferredForm::TargetSet => ResolutionFeature::TargetSet,
        DeferredForm::NullVertex => ResolutionFeature::NullVertex,
        DeferredForm::ScopeVia => ResolutionFeature::ScopeVia,
        DeferredForm::StandoffFragments => ResolutionFeature::StandoffFragments,
        DeferredForm::SpanText => ResolutionFeature::SpanText,
        DeferredForm::ReferencedText => ResolutionFeature::ReferencedText,
        DeferredForm::ExternalUriText => ResolutionFeature::ExternalUriText,
    };
    let deferred = [
        ResolutionFeature::InlineReference,
        ResolutionFeature::TargetSet,
        ResolutionFeature::NullVertex,
        ResolutionFeature::ScopeVia,
        ResolutionFeature::StandoffFragments,
        ResolutionFeature::SpanText,
        ResolutionFeature::ReferencedText,
        ResolutionFeature::ExternalUriText,
    ];
    let mut protocol = protocol("deferred-forms");
    protocol.syntactic_shape = InstanceShapeKind::Tree;
    let sources = [DeclaredSource::KeyValue, DeclaredSource::Inline]
        .into_iter()
        .chain([false, true].into_iter().flat_map(|fragments| {
            (0..4).map(move |text| DeclaredSource::Standoff(fragments, text))
        }));
    let mut cases = 0usize;
    let mut refused = BTreeSet::new();
    for source in sources {
        for target_set in [false, true] {
            for null_vertex in [false, true] {
                for scope_via in [false, true] {
                    cases += 1;
                    let (schema, reference_schema) =
                        deferred_declaration(source, target_set, null_vertex, scope_via);
                    let expected =
                        deferred_form(&reference_schema, &reference_schema.references["mention"])
                            .map(feature_of);
                    let provider = Provider {
                        protocol: protocol.clone(),
                        spec: ResolutionSpec {
                            kind: ResolutionKind::TreeToGraph,
                            provider: ResolutionProviderId {
                                name: "deferred-forms".into(),
                                version: 1,
                            },
                            semantic_schema: schema.clone(),
                            protocol_references: BTreeMap::new(),
                            structural_edges: BTreeSet::new(),
                            reference_writers: BTreeMap::new(),
                            context_requirements: BTreeSet::new(),
                            assembly: None,
                        },
                        kind: ResolutionKind::TreeToGraph,
                    };
                    let found =
                        match Resolution::for_protocol(&protocol, &schema, Arc::new(provider)) {
                            Err(ResolveError::UnsupportedFeature { feature, provider })
                                if deferred.contains(&feature) =>
                            {
                                assert_eq!(
                                    provider.map(|id| id.name),
                                    Some("deferred-forms".into())
                                );
                                Some(feature)
                            }
                            // Outside the deferred domain production goes on to its
                            // ordinary checks, whose verdict this test does not
                            // fix; it must not be a deferred-form refusal.
                            _ => None,
                        };
                    assert_eq!(
                        found, expected,
                        "{source:?}, target_set={target_set}, null_vertex={null_vertex}, scope_via={scope_via}"
                    );
                    refused.extend(found.map(|feature| format!("{feature:?}")));
                }
            }
        }
    }
    assert_eq!(cases, 10 * 8);
    // Every one of the eight forms is the first refusal of some declaration.
    assert_eq!(refused.len(), 8, "{refused:?}");
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
    let mut congruence_cases = 0usize;
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
            let stale_reference =
                reference_tree_edit(&reference_view, &reference_picked, &step, &indexed_view);
            let stale_production =
                production_tree_edit(&production_view, &production_picked, &step, &indexed_view);
            // The put domain of this carrier, stated without put: a candidate
            // is admitted exactly when the holder's visible sequence lists
            // the targets of its resolution edges in occurrence order. An
            // edge-only edit leaves that sequence stale and is refused.
            assert!(
                reference_put(&reference_schema, &stale_reference, &reference_complement).is_err(),
                "reference stale edit: {source:?}, {step:?}"
            );
            assert!(
                production_resolution
                    .check_put_domain(
                        &AnyInstance::Graph(stale_production.clone()),
                        &production_complement,
                        &ResolveContext::empty(),
                    )
                    .is_err(),
                "production stale edit: {source:?}, {step:?}"
            );
            assert!(
                production_resolution
                    .put(
                        &AnyInstance::Graph(stale_production.clone()),
                        &production_complement,
                        &ResolveContext::empty(),
                    )
                    .is_err(),
                "production stale put: {source:?}, {step:?}"
            );
            let sequence: Vec<String> = apply_single_step(&indexed_view, &step)
                .into_values()
                .collect();
            let mut reference_candidate = stale_reference;
            reference_candidate
                .nodes
                .get_mut(&1)
                .expect("reference holder")
                .fields
                .insert(
                    "customer".to_owned(),
                    ReferenceValue::List(
                        sequence.iter().cloned().map(ReferenceValue::Str).collect(),
                    ),
                );
            let mut production_candidate = stale_production;
            production_candidate
                .node_mut(1)
                .expect("production holder")
                .extra_fields
                .insert(
                    "customer".to_owned(),
                    Value::List(sequence.iter().map(|target| text(target)).collect()),
                );
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
            assert!(
                production_domain.is_ok(),
                "production admits the coherent edit: {source:?}, {step:?}: {production_domain:?}"
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
            // Congruence over node and edge renamings: every permutation of
            // the view's node ids, with the resolution edges moved above
            // every original id, renames each complement coordinate; the
            // verdict is unchanged and the put is the original put with its
            // node ids transported by the induced source map.
            let node_ids: Vec<u32> = production_candidate
                .nodes()
                .keys()
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let pool = u32::try_from(node_ids.len()).expect("small view");
            let edge_floor = production_candidate
                .max_edge_id()
                .map_or(0, |id| id.0 + 1)
                .max(
                    production_complement
                        .occurrences
                        .keys()
                        .filter_map(|carrier| match carrier {
                            ViewRef::Edge(id) => Some(id.0 + 1),
                            _ => None,
                        })
                        .max()
                        .unwrap_or(0),
                );
            // Structural edge ids of a tree view are the source's arc slots,
            // a source coordinate like a row or an arc, so only the
            // synthesized resolution edges are renamed.
            let edge_renaming: BTreeMap<u32, u32> = production_candidate
                .edges_sorted()
                .filter(|(_, edge)| edge.edge == production_picked)
                .map(|(id, _)| id.0)
                .chain(production_complement.occurrences.keys().filter_map(
                    |carrier| match carrier {
                        ViewRef::Edge(id) => Some(id.0),
                        _ => None,
                    },
                ))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|id| (id, edge_floor + id))
                .collect();
            for node_renaming in injective_renamings(&node_ids, pool) {
                congruence_cases += 1;
                let (renamed_view, renamed_complement) = rename_view_state(
                    &production_candidate,
                    &production_complement,
                    &node_renaming,
                    &edge_renaming,
                );
                let renamed_put = production_resolution.put(
                    &AnyInstance::Graph(renamed_view),
                    &renamed_complement,
                    &ResolveContext::empty(),
                );
                match (&production_once, renamed_put) {
                    (Ok(AnyInstance::Tree(once)), Ok(AnyInstance::Tree(renamed))) => assert!(
                        same_instance(
                            &AnyInstance::Tree(renamed),
                            &AnyInstance::Tree(rename_tree(once, &node_renaming))
                        ),
                        "node Congruence result: {source:?}, {step:?}, {node_renaming:?}"
                    ),
                    (Err(_), Err(_)) => {}
                    (once, renamed) => panic!(
                        "node Congruence verdict: {source:?}, {step:?}, {node_renaming:?}: \
                         {once:?} vs {renamed:?}"
                    ),
                }
            }
            let (Ok(reference_once), Ok(AnyInstance::Tree(production_once))) =
                (reference_once, production_once)
            else {
                panic!("a coherent single step is admitted: {source:?}, {step:?}");
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
                "reference GetPut at the put source: {source:?}, {step:?}"
            );
            let production_twice = production_resolution
                .put(
                    &AnyInstance::Graph(production_again),
                    &production_next,
                    &ResolveContext::empty(),
                )
                .expect("production GetPut at the put source");
            assert!(same_instance(
                &production_twice,
                &AnyInstance::Tree(production_once)
            ));
        }
    }
    assert_eq!(candidate_count, 48);
    // 5! node permutations of the five-node view for each candidate.
    assert_eq!(congruence_cases, 48 * 120);
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
            assert!(
                production_put_get_equal(&production_candidate, &production_again, &picked),
                "production PutGet: {source:?}, {step:?}"
            );
            let reference_exact = reference_again == candidate;
            let production_exact = GInstance::identical(&production_again, &production_candidate);
            assert_eq!(reference_exact, production_exact, "exact PutGet: {step:?}");
            reference_counts.exact_put_get += usize::from(reference_exact);
            production_counts.exact_put_get += usize::from(production_exact);

            let raw_reference =
                indexed_put(&candidate, &reference_next) == Ok(reference_once.clone());
            let raw_result = resolution.put(
                &AnyInstance::Graph(production_candidate.clone()),
                &production_next,
                &ResolveContext::empty(),
            );
            let raw_production =
                matches!(&raw_result, Ok(twice) if same_instance(twice, &production_once));
            if source == IndexedSource::from(["b".to_owned()])
                && step == SingleStep::Add("a".to_owned())
            {
                // The re-allocating case: get on the put source gives `b` a
                // new synthesized id, so the untransported complement names
                // an edited carrier and the raw second put is refused.
                assert!(
                    matches!(
                        raw_result,
                        Err(ResolveError::PutDomain(
                            ResolveDomainError::SynthesizedEdited { .. }
                        ))
                    ),
                    "raw PutTwice on the re-allocating case: {raw_result:?}"
                );
                assert!(!raw_reference);
            }
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
    eprintln!(
        "raw PutTwice failures: reference {}, production {}",
        reference_counts.raw_put_twice_failures, production_counts.raw_put_twice_failures
    );
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
            // Admitted renamed states put to the same source: the renaming
            // moves only synthesized ids, which put discards.
            proptest::prop_assert_eq!(
                indexed_put(&renamed, &renamed_index).ok(),
                indexed_put(&candidate, &index).ok()
            );
        }
    }
}

#[test]
fn reference_gated_put_twice_is_exact_where_raw_put_twice_fails() {
    let (mut admitted, mut exact, mut raw_failures) = (0usize, 0usize, 0usize);
    for source in indexed_sources() {
        let (view, index) = indexed_get(&source);
        for step in single_step_basis(&view) {
            let candidate = apply_single_step(&view, &step);
            let Ok(once) = indexed_put(&candidate, &index) else {
                continue;
            };
            admitted += 1;
            let (again, next) = indexed_get(&once);
            exact += usize::from(again == candidate);
            match indexed_put(&candidate, &next) {
                Ok(twice) => assert_eq!(twice, once, "raw PutTwice result: {step:?}"),
                Err(violations) => {
                    // DESIGN's first case: the untransported complement names
                    // a re-allocated synthesized id, so put refuses it.
                    assert!(
                        violations.iter().all(|violation| matches!(
                            violation,
                            ResolveDomainViolation::SynthesizedEdited(_)
                        )),
                        "raw PutTwice refusal: {step:?}, {violations:?}"
                    );
                    assert_ne!(again, candidate, "a raw failure needs a re-allocation");
                    raw_failures += 1;
                }
            }
            assert!(
                gated_put_twice(&candidate, &index),
                "gated PutTwice: {source:?}, {step:?}"
            );
        }
    }
    assert_eq!((admitted, exact, raw_failures), (24, 14, 6));
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
        ("../../../../g", "http://a/g"),
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
        // Section 5.4.2 reads this example strictly: a scheme makes the
        // reference absolute, so the base is not consulted.
        ("http:g", "http:g"),
    ];
    assert_eq!(vectors.len(), 23 + 19);
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

/// A reference URI schema: elements (`el`) nested under a document, base
/// (`dir`) and prefix (`pfx`) directives as document children, and targets
/// keyed by their IRI; each element's `ptr` is a URI reference under `base`.
fn uri_schema(
    base: reference_schema::UriBase,
) -> (reference_schema::Schema, reference_model::SEdge) {
    let field =
        |src: &str, name: &str| reference_model::SEdge::new(src, "label", "prop", Some(name));
    let points = reference_model::SEdge::new("el", "target", "ref", Some("points"));
    let mut schema = reference_schema::Schema::default()
        .vertex("doc", "object")
        .vertex("el", "object")
        .vertex("dir", "string")
        .vertex("pfx", "object")
        .vertex("target", "object")
        .vertex("label", "string")
        .edge("doc", "el", "prop", Some("el"))
        .edge("el", "el", "prop", Some("child"))
        .edge("doc", "dir", "prop", Some("dir"))
        .edge("doc", "pfx", "prop", Some("pfx"))
        .edge("doc", "target", "prop", Some("targets"));
    schema.edges.extend([
        field("el", "base"),
        field("el", "ptr"),
        field("pfx", "prefix"),
        field("pfx", "iri"),
        field("target", "iri"),
        points.clone(),
    ]);
    schema.entries.push("doc".to_owned());
    schema.keys.insert(
        "target".to_owned(),
        vec![reference_schema::Key {
            id: "iri".to_owned(),
            parts: vec![reference_schema::KeyPart::Path(vec![field(
                "target", "iri",
            )])],
            role: reference_schema::KeyRole::Primary,
            scope: reference_schema::KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "points".to_owned(),
        reference_schema::Reference {
            id: "points".to_owned(),
            holder: "el".to_owned(),
            source: reference_schema::RefSource::Path {
                field: vec![field("el", "ptr")],
                syntax: reference_schema::PathSyntax::Uri { base },
            },
            target_vertex: "target".to_owned(),
            target_key: Some("iri".to_owned()),
            edge: Some(points.clone()),
            cardinality: reference_schema::Cardinality::One,
            nullable: true,
            on_unresolved: reference_schema::OnUnresolved::Complement,
            mode: reference_schema::ResolveMode::AddEdge,
            guard: None,
            null_match: reference_schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    (schema, points)
}

/// A document-child arc of the URI schema.
fn uri_arc(child: u32, name: &str) -> (u32, u32, reference_model::SEdge) {
    (
        0,
        child,
        reference_model::SEdge::new("doc", name, "prop", Some(name)),
    )
}

/// Append one target per IRI to a URI document.
fn with_uri_targets(tree: &mut reference_model::Tree, iris: &BTreeSet<String>) {
    let mut id = tree.nodes.keys().next_back().copied().unwrap_or(0) + 1;
    for iri in iris {
        tree.nodes.insert(
            id,
            reference_model::Node::new(id, "target")
                .with_field("iri", ReferenceValue::Str(iri.clone())),
        );
        tree.arcs.push((
            0,
            id,
            reference_model::SEdge::new("doc", "target", "prop", Some("targets")),
        ));
        id += 1;
    }
}

/// The IRI each element of a URI document resolves to under reference get:
/// the `iri` of the target its resolution edge reaches, if any.
fn reference_uri_targets(
    schema: &reference_schema::Schema,
    tree: &reference_model::Tree,
) -> BTreeMap<u32, Option<String>> {
    let (graph, complement) = reference_get(schema, tree, false).expect("reference URI get");
    let mut out: BTreeMap<u32, Option<String>> = tree
        .nodes
        .values()
        .filter(|node| node.anchor == "el")
        .map(|node| (node.id, None))
        .collect();
    for (edge, occurrence) in &complement.resolved {
        let target = complement.targets[edge];
        let iri = match &graph.nodes[&target].fields["iri"] {
            ReferenceValue::Str(iri) => iri.clone(),
            other => panic!("non-string target IRI {other:?}"),
        };
        assert!(
            out.insert(occurrence.holder, Some(iri))
                .is_some_and(|before| before.is_none()),
            "one resolution per element"
        );
    }
    out
}

fn production_directive(
    directive: &panproto_reference::resolution::UriDirective,
) -> panproto_io::resolution::UriDirective {
    match directive {
        panproto_reference::resolution::UriDirective::Base(iri) => {
            panproto_io::resolution::UriDirective::Base(iri.clone())
        }
        panproto_reference::resolution::UriDirective::Prefix { label, iri } => {
            panproto_io::resolution::UriDirective::Prefix {
                label: label.clone(),
                iri: iri.clone(),
            }
        }
    }
}

#[test]
fn uri_environment_uses_only_the_preceding_directive_prefix() {
    use panproto_reference::resolution::UriDirective;
    let base = |iri: &str| UriDirective::Base(iri.to_owned());
    // A relative first base gives no base, and a later absolute base sets it,
    // so the fold does not stop at an undefined step.
    let environment = uri_environment(&[base("c/"), base("http://a/b/")]);
    assert_eq!(environment.base.as_deref(), Some("http://a/b/"));
    assert_eq!(environment.resolve("g").as_deref(), Some("http://a/b/g"));
    let chained = uri_environment(&[
        base("http://a/b/"),
        base("c/"),
        UriDirective::Prefix {
            label: "p".to_owned(),
            iri: "y#".to_owned(),
        },
    ]);
    assert_eq!(chained.base.as_deref(), Some("http://a/b/c/"));
    assert_eq!(chained.resolve("p:q").as_deref(), Some("http://a/b/c/y#q"));
    assert_eq!(uri_environment(&[base("c/")]).resolve("g"), None);

    let sequences = uri_directive_sequences();
    assert_eq!(sequences.len(), 1 + 5 + 25 + 125);
    let forms = uri_written_forms();
    // Every directive after the occurrences, or none, as a locality probe.
    let suffixes: Vec<Option<UriDirective>> = std::iter::once(None)
        .chain(
            sequences
                .iter()
                .filter(|s| s.len() == 1)
                .map(|s| Some(s[0].clone())),
        )
        .collect();
    let mut iris = BTreeSet::new();
    for sequence in &sequences {
        let environment = uri_environment(sequence);
        let production_sequence: Vec<_> = sequence.iter().map(production_directive).collect();
        let production = fold_uri_environment(&production_sequence);
        assert_eq!(production.base, environment.base, "base: {sequence:?}");
        assert_eq!(
            production.prefixes, environment.prefixes,
            "prefixes: {sequence:?}"
        );
        for form in forms {
            assert_eq!(
                production.resolve(form),
                environment.resolve(form),
                "{sequence:?} {form}"
            );
            iris.extend(environment.resolve(form));
        }
    }

    assert_positional_get_matches_the_fold(&sequences, &suffixes, &iris);
}

/// Reference get under a positional base resolves each occurrence to the IRI
/// that the fold of the directives before it gives, whatever follows it.
fn assert_positional_get_matches_the_fold(
    sequences: &[Vec<panproto_reference::resolution::UriDirective>],
    suffixes: &[Option<panproto_reference::resolution::UriDirective>],
    iris: &BTreeSet<String>,
) {
    use panproto_reference::resolution::UriDirective;
    let forms = uri_written_forms();
    let (schema, _) = uri_schema(reference_schema::UriBase::Positional {
        base: Some("dir".to_owned()),
        prefix: Some("pfx".to_owned()),
    });
    let mut documents = 0usize;
    for sequence in sequences {
        let environment = uri_environment(sequence);
        for suffix in suffixes {
            let mut nodes = BTreeMap::from([(0, reference_model::Node::new(0, "doc"))]);
            let mut arcs = Vec::new();
            let mut id = 1;
            let push_directive = |directive: &UriDirective,
                                  nodes: &mut BTreeMap<u32, reference_model::Node>,
                                  arcs: &mut Vec<_>,
                                  id: &mut u32| {
                let node = match directive {
                    UriDirective::Base(iri) => {
                        arcs.push(uri_arc(*id, "dir"));
                        reference_model::Node::new(*id, "dir")
                            .with_value(ReferenceValue::Str(iri.clone()))
                    }
                    UriDirective::Prefix { label, iri } => {
                        arcs.push(uri_arc(*id, "pfx"));
                        reference_model::Node::new(*id, "pfx")
                            .with_field("prefix", ReferenceValue::Str(label.clone()))
                            .with_field("iri", ReferenceValue::Str(iri.clone()))
                    }
                };
                nodes.insert(*id, node);
                *id += 1;
            };
            for directive in sequence {
                push_directive(directive, &mut nodes, &mut arcs, &mut id);
            }
            let mut expected = BTreeMap::new();
            for form in forms {
                nodes.insert(
                    id,
                    reference_model::Node::new(id, "el")
                        .with_field("ptr", ReferenceValue::Str(form.to_owned())),
                );
                arcs.push(uri_arc(id, "el"));
                expected.insert(id, environment.resolve(form));
                id += 1;
            }
            if let Some(directive) = suffix {
                push_directive(directive, &mut nodes, &mut arcs, &mut id);
            }
            let mut tree = reference_model::Tree {
                nodes,
                arcs,
                fans: Vec::new(),
                root: 0,
            };
            with_uri_targets(&mut tree, iris);
            assert_eq!(
                reference_uri_targets(&schema, &tree),
                expected,
                "reference get under Positional: {sequence:?} then {suffix:?}"
            );
            documents += 1;
        }
    }
    assert_eq!(documents, 156 * 6);
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

/// Every text of at most four code points over `a`, `é` and U+1F600, with at
/// most one character outside the basic multilingual plane.
fn standoff_texts() -> Vec<String> {
    panproto_reference::enumerate::sequences(&['a', 'é', '\u{1F600}'], 4)
        .into_iter()
        .filter(|text| text.iter().filter(|c| **c == '\u{1F600}').count() <= 1)
        .map(|text| text.into_iter().collect())
        .collect()
}

#[test]
fn standoff_forms_are_injective_and_left_inverse_in_every_unit() {
    use panproto_io::resolution::{bytes_to_written_form, written_form_to_bytes};
    use panproto_reference::resolution::{standoff_byte_inverse, standoff_byte_span};
    use panproto_schema::OffsetUnit as ProductionUnit;
    use reference_schema::OffsetUnit as ReferenceUnit;

    let texts = standoff_texts();
    assert_eq!(texts.len(), 1 + 3 + 8 + 20 + 48);
    let units = [
        (ReferenceUnit::Byte, ProductionUnit::Byte),
        (ReferenceUnit::Utf8Char, ProductionUnit::Utf8Char),
        (ReferenceUnit::Utf16, ProductionUnit::Utf16),
    ];
    let (mut written_cases, mut admitted, mut byte_cases) = (0usize, 0usize, 0usize);
    for text in &texts {
        let bytes = text.as_bytes();
        let byte_len = u64::try_from(bytes.len()).expect("short text");
        for (reference_unit, production_unit) in &units {
            let unit_len = match reference_unit {
                ReferenceUnit::Byte => byte_len,
                ReferenceUnit::Utf8Char => u64::try_from(text.chars().count()).expect("short"),
                _ => u64::try_from(text.encode_utf16().count()).expect("short"),
            };
            for origin in [0u64, 1] {
                for end_is_length in [false, true] {
                    let end_form = if end_is_length {
                        SpanEnd::Length
                    } else {
                        SpanEnd::Offset
                    };
                    // Every written pair up to one past the text, so that
                    // out-of-range and in-unit offsets are refused alike.
                    let mut images = BTreeMap::new();
                    for start in 0..=unit_len + origin + 1 {
                        for end in 0..=unit_len + origin + 1 {
                            written_cases += 1;
                            let reference = standoff_byte_span(
                                text,
                                (start, end),
                                origin,
                                end_is_length,
                                *reference_unit,
                            );
                            let production = written_form_to_bytes(
                                bytes,
                                (start, end),
                                origin,
                                end_form,
                                production_unit,
                            );
                            assert_eq!(
                                production, reference,
                                "decode {text:?} {reference_unit:?} ({start}, {end}) o={origin} len={end_is_length}"
                            );
                            let Some(span) = reference else { continue };
                            admitted += 1;
                            assert_eq!(
                                images.insert(span, (start, end)),
                                None,
                                "two forms name {span:?} in {text:?} {reference_unit:?}"
                            );
                            // Converse: the inverse reproduces every admitted form.
                            assert_eq!(
                                standoff_byte_inverse(
                                    text,
                                    span,
                                    origin,
                                    end_is_length,
                                    *reference_unit
                                ),
                                Some((start, end))
                            );
                            assert_eq!(
                                bytes_to_written_form(
                                    bytes,
                                    span,
                                    origin,
                                    end_form,
                                    production_unit
                                ),
                                Some((start, end))
                            );
                        }
                    }
                    // Left inverse: decoding the written form of every byte
                    // span the unit can express returns that span.
                    for byte_start in 0..=byte_len {
                        for byte_end in byte_start..=byte_len {
                            byte_cases += 1;
                            let span = (byte_start, byte_end);
                            let written = standoff_byte_inverse(
                                text,
                                span,
                                origin,
                                end_is_length,
                                *reference_unit,
                            );
                            assert_eq!(
                                bytes_to_written_form(
                                    bytes,
                                    span,
                                    origin,
                                    end_form,
                                    production_unit
                                ),
                                written,
                                "encode {text:?} {reference_unit:?} {span:?}"
                            );
                            if let Some(written) = written {
                                assert_eq!(
                                    standoff_byte_span(
                                        text,
                                        written,
                                        origin,
                                        end_is_length,
                                        *reference_unit
                                    ),
                                    Some(span)
                                );
                            } else {
                                assert!(images.values().all(|form| standoff_byte_span(
                                    text,
                                    *form,
                                    origin,
                                    end_is_length,
                                    *reference_unit
                                ) != Some(span)));
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("written={written_cases} admitted={admitted} byte_spans={byte_cases}");
    assert!(admitted > 0 && byte_cases > 0);
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
    for (policy, reference_policy) in [
        (OnUnresolved::Error, reference_schema::OnUnresolved::Error),
        (
            OnUnresolved::Complement,
            reference_schema::OnUnresolved::Complement,
        ),
    ] {
        assert_eq!(
            reference_index_get(reference_policy),
            Err(
                panproto_reference::resolution::ResolveError::KeyNotResolvable {
                    reference: "index-reference".to_owned(),
                    key: "index".to_owned(),
                }
            ),
            "reference get under {policy:?}"
        );
        assert_production_index_refusal(policy);
    }
}

/// Reference get of one holder whose literal equals one target's index,
/// through a reference to that `Index` key.
fn reference_index_get(
    on_unresolved: reference_schema::OnUnresolved,
) -> Result<(), panproto_reference::resolution::ResolveError> {
    let field = reference_model::SEdge::new("target", "scalar", "prop", Some("index"));
    let source = reference_model::SEdge::new("holder", "scalar", "prop", Some("target"));
    let resolved = reference_model::SEdge::new("holder", "target", "ref", Some("resolved"));
    let mut schema = reference_schema::Schema::default()
        .vertex("doc", "object")
        .vertex("holder", "object")
        .vertex("target", "object")
        .vertex("scalar", "string")
        .edge("doc", "holder", "prop", Some("holder"))
        .edge("doc", "target", "prop", Some("target"));
    schema
        .edges
        .extend([field.clone(), source.clone(), resolved.clone()]);
    schema.entries.push("doc".to_owned());
    schema.keys.insert(
        "target".to_owned(),
        vec![reference_schema::Key {
            id: "index".to_owned(),
            parts: vec![reference_schema::KeyPart::Path(vec![field])],
            role: reference_schema::KeyRole::Index,
            scope: reference_schema::KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "index-reference".to_owned(),
        reference_schema::Reference {
            id: "index-reference".to_owned(),
            holder: "holder".to_owned(),
            source: reference_schema::RefSource::KeyValue {
                parts: vec![vec![source]],
            },
            target_vertex: "target".to_owned(),
            target_key: Some("index".to_owned()),
            edge: Some(resolved),
            cardinality: reference_schema::Cardinality::One,
            nullable: false,
            on_unresolved,
            mode: reference_schema::ResolveMode::AddEdge,
            guard: None,
            null_match: reference_schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let tree = reference_model::Tree {
        nodes: BTreeMap::from([
            (0, reference_model::Node::new(0, "doc")),
            (
                1,
                reference_model::Node::new(1, "holder")
                    .with_field("target", ReferenceValue::Str("0".to_owned())),
            ),
            (
                2,
                reference_model::Node::new(2, "target")
                    .with_field("index", ReferenceValue::Str("0".to_owned())),
            ),
        ]),
        arcs: vec![
            (
                0,
                1,
                reference_model::SEdge::new("doc", "holder", "prop", Some("holder")),
            ),
            (
                0,
                2,
                reference_model::SEdge::new("doc", "target", "prop", Some("target")),
            ),
        ],
        fans: Vec::new(),
        root: 0,
    };
    reference_get(&schema, &tree, true).map(|_| ())
}

/// Production refuses a reference through an `Index` key when the
/// resolution compiles, whatever the reference's unresolved policy.
fn assert_production_index_refusal(on_unresolved: OnUnresolved) {
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
            on_unresolved,
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
fn inherited_base_folds_the_ancestor_or_self_chain() {
    // XML Base section 4.2: each xml:base resolves against the base so far.
    let bases = [Some("http://a/b/".to_owned()), Some("c/".to_owned())];
    assert_eq!(
        inherited_base(&bases, "x").as_deref(),
        Some("http://a/b/c/x")
    );
    assert_eq!(
        resolve_inherited_uri_base(&bases, "x").as_deref(),
        Some("http://a/b/c/x")
    );
    let nested = [Some("http://x/a/".to_owned()), Some("b/".to_owned())];
    assert_eq!(
        inherited_base(&nested, "c").as_deref(),
        Some("http://x/a/b/c")
    );
    // A relative value met with no base gives no base; a later absolute one sets it.
    let relative_first = [Some("c/".to_owned()), Some("http://a/b/".to_owned())];
    assert_eq!(
        inherited_base(&relative_first, "x").as_deref(),
        Some("http://a/b/x")
    );
    assert_eq!(inherited_base(&[Some("c/".to_owned())], "x"), None);
    assert_eq!(inherited_base(&[None, None], "c"), None);
    assert_eq!(
        inherited_base(&[None], "http://e/f").as_deref(),
        Some("http://e/f"),
        "an absolute pointer needs no base"
    );

    let forms = ["#w1", "c", "http://e/f"];
    let chains = inherited_base_chains();
    assert_eq!(chains.len(), 84);
    let mut iris = BTreeSet::new();
    for chain in &chains {
        for form in forms {
            // Independent statement of XML Base 4.2 by recursion on the chain.
            let mut expected_base: Option<String> = None;
            for value in chain.iter().flatten() {
                expected_base = if value.contains(':') {
                    rfc3986(value, value)
                } else {
                    expected_base.and_then(|base| rfc3986(&base, value))
                };
            }
            let expected = if form.contains(':') {
                rfc3986(form, form)
            } else {
                expected_base.and_then(|base| rfc3986(&base, form))
            };
            assert_eq!(inherited_base(chain, form), expected, "{chain:?} {form}");
            assert_eq!(
                resolve_inherited_uri_base(chain, form),
                expected,
                "production {chain:?} {form}"
            );
            iris.extend(expected);
        }
    }

    assert_inherited_get_matches_the_fold(forms, &iris);
}

/// Reference get on every tree of at most three elements, siblings included:
/// each element resolves by the fold over its own ancestor-or-self chain.
fn assert_inherited_get_matches_the_fold(forms: [&str; 3], iris: &BTreeSet<String>) {
    let (schema, _) = uri_schema(reference_schema::UriBase::Inherited {
        field: vec![reference_model::SEdge::new(
            "el",
            "label",
            "prop",
            Some("base"),
        )],
    });
    let shapes: [&[Option<usize>]; 4] = [
        &[None],
        &[None, Some(0)],
        &[None, Some(0), Some(1)],
        &[None, Some(0), Some(0)],
    ];
    let values = [None, Some("http://a/b/"), Some("c/"), Some("../d/")];
    let mut trees = 0usize;
    for parents in shapes {
        let mut assignment = vec![0usize; parents.len()];
        loop {
            for form in forms {
                let mut nodes = BTreeMap::from([(0, reference_model::Node::new(0, "doc"))]);
                let mut arcs = Vec::new();
                let mut expected = BTreeMap::new();
                for (index, parent) in parents.iter().enumerate() {
                    let id = u32::try_from(index).expect("small") + 1;
                    let mut node = reference_model::Node::new(id, "el")
                        .with_field("ptr", ReferenceValue::Str(form.to_owned()));
                    if let Some(base) = values[assignment[index]] {
                        node = node.with_field("base", ReferenceValue::Str(base.to_owned()));
                    }
                    nodes.insert(id, node);
                    arcs.push(match parent {
                        None => uri_arc(id, "el"),
                        Some(parent) => (
                            u32::try_from(*parent).expect("small") + 1,
                            id,
                            reference_model::SEdge::new("el", "el", "prop", Some("child")),
                        ),
                    });
                    let mut chain = Vec::new();
                    let mut at = Some(index);
                    while let Some(current) = at {
                        chain.push(values[assignment[current]].map(str::to_owned));
                        at = parents[current];
                    }
                    chain.reverse();
                    expected.insert(id, inherited_base(&chain, form));
                }
                let mut tree = reference_model::Tree {
                    nodes,
                    arcs,
                    fans: Vec::new(),
                    root: 0,
                };
                with_uri_targets(&mut tree, iris);
                assert_eq!(
                    reference_uri_targets(&schema, &tree),
                    expected,
                    "reference get under Inherited: {parents:?} {assignment:?} {form}"
                );
                trees += 1;
            }
            let Some(slot) = assignment
                .iter()
                .rposition(|value| *value + 1 < values.len())
            else {
                break;
            };
            assignment[slot] += 1;
            assignment[slot + 1..].fill(0);
        }
    }
    assert_eq!(trees, (4 + 16 + 64 + 64) * 3);
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
                assert!(
                    !matches!(
                        actual,
                        Err(panproto_inst::key::KeyingError::InvalidNumericRepresentation { .. })
                    ),
                    "a well-formed value never violates its numeric representation"
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
fn declared_key_equality_refusals_propagate_through_resolution() {
    use panproto_io::resolution::KeyEqualityReason;
    use panproto_reference::keys::{
        KeyEquality as ReferenceEquality, SqlCollation as ReferenceCollation,
        SqlPad as ReferencePad,
    };
    use panproto_schema::{SqlCollation, SqlPad};

    let source: IndexedSource = std::iter::once("a".to_owned()).collect();
    let reference_with = |equality: Vec<ReferenceEquality>| {
        let (mut schema, tree, _) = reference_pick_tree(&source);
        schema.keys.get_mut("customer").expect("customer keys")[0].equality = equality;
        reference_get(&schema, &tree, false)
    };

    // A one-part key with two equality entries: the reference refuses get,
    // and production refuses the declaration when the resolution compiles.
    assert_eq!(
        reference_with(vec![
            ReferenceEquality::Representation,
            ReferenceEquality::Representation
        ])
        .err(),
        Some(
            panproto_reference::resolution::ResolveError::EqualityArity {
                key: "cid".to_owned(),
                expected: 1,
                found: 2,
            }
        )
    );
    match try_tree_picks_with_equality(vec![
        KeyEquality::Representation,
        KeyEquality::Representation,
    ]) {
        Err(ResolveError::Schema(KeyRefError::EqualityArity {
            vertex,
            key,
            expected: 1,
            found: 2,
        })) => assert_eq!((vertex.as_ref(), key.as_ref()), ("customer", "cid")),
        Err(other) => panic!("expected an equality-arity refusal, found {other:?}"),
        Ok(_) => panic!("expected an equality-arity refusal, found a compiled resolution"),
    }

    // A named collation is declared freely, but keying text under it is
    // refused by get on both sides, never compared as bytes.
    assert_eq!(
        reference_with(vec![ReferenceEquality::Sql {
            collation: ReferenceCollation::Named("database".to_owned()),
            pad: ReferencePad::NoPad,
        }])
        .err(),
        Some(
            panproto_reference::resolution::ResolveError::UnsupportedCollation {
                collation: "database".to_owned(),
            }
        )
    );
    let (named, _) = tree_picks_with_equality(vec![KeyEquality::Sql {
        collation: SqlCollation::Named("database".into()),
        pad: SqlPad::NoPad,
    }]);
    match named.get(
        &AnyInstance::Tree(production_pick_tree(&source)),
        &ResolveOptions { lenient: true },
        &ResolveContext::empty(),
    ) {
        Err(ResolveError::KeyEquality {
            key,
            reason: KeyEqualityReason::UnsupportedCollation(collation),
        }) => assert_eq!((key.as_ref(), collation.as_ref()), ("cid", "database")),
        Err(other) => panic!("expected an unsupported-collation refusal, found {other:?}"),
        Ok(_) => panic!("expected an unsupported-collation refusal, found a resolution"),
    }
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

/// The reference form of the variable-binding fixture: concepts named
/// `names` in document order, bound by an identifier key under `equality`.
fn reference_variable_tree(
    names: &[&str],
    binding: reference_schema::VariableBinding,
    equality: Vec<panproto_reference::keys::KeyEquality>,
) -> (
    reference_schema::Schema,
    reference_model::Tree,
    reference_model::SEdge,
) {
    let child = reference_model::SEdge::new("doc", "concept", "prop", Some("child"));
    let variable = reference_model::SEdge::new("concept", "name", "prop", Some("variable"));
    let bound = reference_model::SEdge::new("concept", "concept", "ref", Some("binding"));
    let mut schema = reference_schema::Schema::default()
        .vertex("doc", "object")
        .vertex("concept", "object")
        .vertex("name", "string")
        .edge("doc", "concept", "prop", Some("child"));
    schema.edges.extend([variable.clone(), bound.clone()]);
    schema.entries.push("doc".to_owned());
    schema.keys.insert(
        "concept".to_owned(),
        vec![reference_schema::Key {
            id: "binder".to_owned(),
            parts: vec![reference_schema::KeyPart::Path(vec![variable.clone()])],
            role: reference_schema::KeyRole::Identifier,
            scope: reference_schema::KeyScope::Document,
            equality,
        }],
    );
    schema.references.insert(
        "variable".to_owned(),
        reference_schema::Reference {
            id: "variable".to_owned(),
            holder: "concept".to_owned(),
            source: reference_schema::RefSource::Variable {
                field: vec![variable],
                binder_key: "binder".to_owned(),
                binding,
            },
            target_vertex: "concept".to_owned(),
            target_key: None,
            edge: Some(bound.clone()),
            cardinality: reference_schema::Cardinality::One,
            nullable: false,
            on_unresolved: reference_schema::OnUnresolved::Error,
            mode: reference_schema::ResolveMode::AddEdge,
            guard: None,
            null_match: reference_schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let mut nodes = BTreeMap::from([(0, reference_model::Node::new(0, "doc"))]);
    let mut arcs = Vec::new();
    for (id, name) in (1u32..).zip(names) {
        nodes.insert(
            id,
            reference_model::Node::new(id, "concept")
                .with_field("variable", ReferenceValue::Str((*name).to_owned())),
        );
        arcs.push((0, id, child.clone()));
    }
    let tree = reference_model::Tree {
        nodes,
        arcs,
        fans: Vec::new(),
        root: 0,
    };
    (schema, tree, bound)
}

fn production_variable_tree(names: &[&str]) -> WInstance {
    let mut nodes = HashMap::from([(0, Node::new(0, "doc"))]);
    for (id, name) in (1u32..).zip(names) {
        let mut node = Node::new(id, "concept");
        node.extra_fields.insert("variable".to_owned(), text(name));
        nodes.insert(id, node);
    }
    let child = prop("doc", "concept", "child");
    let count = u32::try_from(names.len()).expect("short fixture");
    WInstance::new(
        nodes,
        (1..=count)
            .map(|target| (0, target, child.clone()))
            .collect(),
        Vec::new(),
        0,
        "doc".into(),
    )
}

#[test]
fn variable_binding_partitions_names_by_the_binder_keys_declared_equality() {
    use panproto_io::resolution::KeyEqualityReason;
    use panproto_reference::keys::{
        KeyEquality as ReferenceEquality, SqlCollation as ReferenceCollation,
        SqlPad as ReferencePad,
    };
    use panproto_schema::{SqlCollation, SqlPad};

    let case_insensitive = (
        ReferenceEquality::Sql {
            collation: ReferenceCollation::AsciiCaseInsensitive,
            pad: ReferencePad::NoPad,
        },
        KeyEquality::Sql {
            collation: SqlCollation::AsciiCaseInsensitive,
            pad: SqlPad::NoPad,
        },
    );
    let representation = (
        ReferenceEquality::Representation,
        KeyEquality::Representation,
    );
    let names = ["x", "X", "y", "x"];
    let mut cases = 0usize;
    for (reference_equality, production_equality) in [case_insensitive, representation] {
        for (reference_binding, production_binding) in [
            (
                reference_schema::VariableBinding::FirstDefines,
                VariableBinding::FirstDefines,
            ),
            (
                reference_schema::VariableBinding::NearestPreceding,
                VariableBinding::NearestPreceding,
            ),
        ] {
            cases += 1;
            let (schema, tree, bound) = reference_variable_tree(
                &names,
                reference_binding,
                vec![reference_equality.clone()],
            );
            let (reference_view, _) =
                reference_get(&schema, &tree, false).expect("reference variable get");
            let mut expected: Vec<(u32, u32)> = reference_view
                .edges
                .values()
                .filter(|edge| edge.label == bound)
                .map(|edge| (edge.src, edge.tgt))
                .collect();
            expected.sort_unstable();

            let (resolution, binding) =
                variable_resolution_with(production_binding, vec![production_equality.clone()]);
            let source = production_variable_tree(&names);
            let (view, complement) = resolution
                .get(
                    &AnyInstance::Tree(source.clone()),
                    &ResolveOptions::default(),
                    &ResolveContext::empty(),
                )
                .expect("production variable get");
            let AnyInstance::Graph(view) = view else {
                panic!("TreeToGraph returned a non-graph")
            };
            let mut found: Vec<(u32, u32)> = view
                .edges_sorted()
                .filter(|(_, edge)| edge.edge == binding)
                .map(|(_, edge)| (edge.src, edge.tgt))
                .collect();
            found.sort_unstable();
            assert_eq!(
                found, expected,
                "{production_equality:?}, {production_binding:?}"
            );
            let restored = resolution
                .put(
                    &AnyInstance::Graph(view),
                    &complement,
                    &ResolveContext::empty(),
                )
                .expect("variable GetPut");
            assert!(same_instance(&restored, &AnyInstance::Tree(source)));
        }
    }
    assert_eq!(cases, 4);

    // Under the ASCII case-insensitive binder `X` is a later binding of `x`.
    let (resolution, binding) = variable_resolution_with(
        VariableBinding::FirstDefines,
        vec![KeyEquality::Sql {
            collation: SqlCollation::AsciiCaseInsensitive,
            pad: SqlPad::NoPad,
        }],
    );
    let (view, _) = resolution
        .get(
            &AnyInstance::Tree(production_variable_tree(&["x", "X"])),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .expect("case-insensitive binding");
    let AnyInstance::Graph(view) = view else {
        panic!("TreeToGraph returned a non-graph")
    };
    assert!(
        view.edges_sorted()
            .any(|(_, edge)| edge.edge == binding && edge.src == 2 && edge.tgt == 1)
    );

    // Text under a named collation has no executable equality: refused.
    let (named, _) = variable_resolution_with(
        VariableBinding::FirstDefines,
        vec![KeyEquality::Sql {
            collation: SqlCollation::Named("database".into()),
            pad: SqlPad::NoPad,
        }],
    );
    match named.get(
        &AnyInstance::Tree(production_variable_tree(&["x", "X"])),
        &ResolveOptions { lenient: true },
        &ResolveContext::empty(),
    ) {
        Err(ResolveError::KeyEquality {
            key,
            reason: KeyEqualityReason::UnsupportedCollation(collation),
        }) => assert_eq!((key.as_ref(), collation.as_ref()), ("binder", "database")),
        Err(other) => panic!("expected an unsupported-collation refusal, found {other:?}"),
        Ok(_) => panic!("expected an unsupported-collation refusal, found a resolution"),
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

/// The variable-binding law as a same-input differential: every sequence of
/// one to five holders under one document, each holder's binder value drawn
/// from three values, under both binding modes and under two declared
/// equalities, is resolved by the production resolver, and its binding
/// edges are compared with the reference binder on the same positions.
/// Under `NumericValue` the values `Int(1)` and `Float(1.0)` are one name,
/// so a binder value matches a literal under the key's declared equality.
#[test]
fn variable_binding_matches_the_reference_on_every_bounded_holder_sequence() {
    use panproto_reference::keys::KeyEquality as ReferenceEquality;

    let carriers = [
        (
            Vec::new(),
            ReferenceEquality::Representation,
            [
                (text("a"), ReferenceValue::Str("a".to_owned())),
                (text("b"), ReferenceValue::Str("b".to_owned())),
                (Value::Int(1), ReferenceValue::Int(1)),
            ],
        ),
        (
            vec![KeyEquality::NumericValue],
            ReferenceEquality::NumericValue,
            [
                (Value::Int(1), ReferenceValue::Int(1)),
                (Value::Float(1.0), ReferenceValue::Float(1.0_f64.to_bits())),
                (Value::Int(2), ReferenceValue::Int(2)),
            ],
        ),
    ];
    let child = prop("doc", "concept", "child");
    let mut sequences = 0usize;
    let mut bound = 0usize;
    for (equality, reference_equality, values) in &carriers {
        // The name class of each value: the first value it equals under the
        // declared equality, decided by the reference relation.
        let class = |index: usize| {
            (0..values.len())
                .find(|&first| {
                    declared_key_equality(reference_equality, &values[first].1, &values[index].1)
                        == Some(true)
                })
                .map_or_else(|| index.to_string(), |first| first.to_string())
        };
        for policy in [
            VariableBinding::FirstDefines,
            VariableBinding::NearestPreceding,
        ] {
            let (resolution, binding) = variable_resolution_with(policy, equality.clone());
            for length in 1..=5_u32 {
                for code in 0..3_usize.pow(length) {
                    let picks: Vec<usize> = (0..length)
                        .scan(code, |rest, _| {
                            let pick = *rest % 3;
                            *rest /= 3;
                            Some(pick)
                        })
                        .collect();
                    let mut nodes = HashMap::from([(0, Node::new(0, "doc"))]);
                    for (position, pick) in picks.iter().enumerate() {
                        let id = u32::try_from(position + 1).unwrap();
                        let mut node = Node::new(id, "concept");
                        node.extra_fields
                            .insert("variable".to_owned(), values[*pick].0.clone());
                        nodes.insert(id, node);
                    }
                    let source = WInstance::new(
                        nodes,
                        (1..=length)
                            .map(|target| (0, target, child.clone()))
                            .collect(),
                        Vec::new(),
                        0,
                        "doc".into(),
                    );
                    let (view, _) = resolution
                        .get(
                            &AnyInstance::Tree(source),
                            &ResolveOptions::default(),
                            &ResolveContext::empty(),
                        )
                        .expect("variable-binding get succeeds");
                    let AnyInstance::Graph(view) = view else {
                        panic!("TreeToGraph returned a non-graph")
                    };
                    let mut actual: Vec<(u32, u32)> = view
                        .edges_sorted()
                        .filter(|(_, candidate)| candidate.edge == binding)
                        .map(|(_, candidate)| (candidate.src, candidate.tgt))
                        .collect();
                    actual.sort_unstable();
                    let definitions: Vec<(usize, String, u32)> = picks
                        .iter()
                        .enumerate()
                        .map(|(position, pick)| {
                            (position, class(*pick), u32::try_from(position + 1).unwrap())
                        })
                        .collect();
                    let nearest = policy == VariableBinding::NearestPreceding;
                    let mut expected: Vec<(u32, u32)> = definitions
                        .iter()
                        .filter_map(|(position, name, id)| {
                            nearest_preceding(&definitions, *position, name, nearest)
                                .map(|target| (*id, target))
                        })
                        .collect();
                    expected.sort_unstable();
                    assert_eq!(
                        actual, expected,
                        "{policy:?} under {equality:?} on {picks:?}"
                    );
                    bound += expected.len();
                    sequences += 1;
                }
            }
        }
    }
    assert_eq!(sequences, 2 * 2 * (3 + 9 + 27 + 81 + 243));
    assert!(bound > 0);
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
