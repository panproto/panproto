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

/// A fixture provider whose IDREF reader independently searches customer keys.
struct IdRefProvider(Provider);

impl ResolutionProvider for IdRefProvider {
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
        reference: &Reference,
        _holder: ViewRef,
        literal: &Value,
        source: &AnyInstance,
        _ctx: &ResolveContext<'_>,
    ) -> Result<panproto_io::resolution::PathResolution, ResolveError> {
        let AnyInstance::Tree(source) = source else {
            panic!("the IDREF fixture resolves against its syntactic tree");
        };
        let mut candidates: Vec<_> = source
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.anchor == reference.target_vertex
                    && node.extra_fields.get("cid") == Some(literal)
            })
            .map(|(id, _)| ViewRef::Node(*id))
            .collect();
        candidates.sort_unstable();
        Ok(panproto_io::resolution::PathResolution {
            candidates,
            context: None,
        })
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
    try_tree_picks_with_equality_and_mode(equality, ResolveMode::AddEdge)
}

fn try_tree_picks_with_equality_and_mode(
    equality: Vec<KeyEquality>,
    mode: ResolveMode,
) -> Result<(Resolution, Edge), ResolveError> {
    tree_picks_with_source(equality, mode, None)
}

fn tree_picks_with_source(
    equality: Vec<KeyEquality>,
    mode: ResolveMode,
    source: Option<RefSource>,
) -> Result<(Resolution, Edge), ResolveError> {
    let uses_idref = matches!(
        &source,
        Some(RefSource::Path {
            syntax: panproto_schema::PathSyntax::IdRef,
            ..
        })
    );
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
            mode,
            source: source.unwrap_or_else(|| reference(picked.clone()).source),
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
    let provider = tree_pick_provider(&protocol, &schema, &picked);
    let provider: Arc<dyn ResolutionProvider> = if uses_idref {
        Arc::new(IdRefProvider(provider))
    } else {
        Arc::new(provider)
    };
    let resolution = Resolution::for_protocol(&protocol, &schema, provider)?;
    Ok((resolution, picked))
}

fn tree_pick_provider(protocol: &Protocol, schema: &Schema, picked: &Edge) -> Provider {
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
            .filter(|candidate| *candidate != picked)
            .cloned()
            .collect(),
        reference_writers: BTreeMap::from([(picked.clone(), "pick".into())]),
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TreeToGraph,
    }
}

fn variable_resolution(binding: VariableBinding) -> (Resolution, Edge) {
    variable_resolution_with(binding, Vec::new())
}

/// The variable-binding resolution whose binder key declares `equality`.
fn variable_resolution_with(
    binding: VariableBinding,
    equality: Vec<KeyEquality>,
) -> (Resolution, Edge) {
    variable_resolution_with_cardinality(binding, equality, Cardinality::One)
}

fn variable_resolution_with_cardinality(
    binding: VariableBinding,
    equality: Vec<KeyEquality>,
    cardinality: Cardinality,
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
            cardinality,
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
    null_match_resolution_with(null_match, Cardinality::One, ResolveMode::AddEdge)
}

fn null_match_resolution_with(
    null_match: NullMatch,
    cardinality: Cardinality,
    mode: ResolveMode,
) -> (Resolution, Edge) {
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
            nullable: cardinality == Cardinality::Many,
            cardinality,
            on_unresolved: OnUnresolved::Error,
            mode,
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
    let hidden: BTreeSet<_> = complement
        .redirected
        .values()
        .flat_map(|fragment| fragment.nodes.keys().copied())
        .collect();
    for record in &mut next.definition_occurrences {
        if !matches!(&record.holder, ViewRef::Node(id) if hidden.contains(id)) {
            record.holder = carrier(&record.holder);
        }
    }
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
    let rename_optional = |carrier: &Option<ViewRef>| carrier.as_ref().map(rename);
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
    let (production_source, reference_source) = deferred_sources(source);
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

/// The occurrence source of [`deferred_declaration`], written once for
/// production and once for the reference.
fn deferred_sources(source: DeclaredSource) -> (RefSource, panproto_reference::schema::RefSource) {
    use panproto_reference::schema as rs;
    let field = |name: &str| prop("note", "label", name);
    let sfield = |name: &str| reference_model::SEdge::new("note", "label", "prop", Some(name));
    match source {
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
            let (production_text, reference_text) = standoff_text_sources(text);
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
    }
}

/// The standoff text form `text` (0 local, 1 span-relative, 2 referenced,
/// 3 URI-named), written once for production and once for the reference.
fn standoff_text_sources(
    text: u8,
) -> (
    panproto_schema::RefTextSource,
    panproto_reference::schema::RefTextSource,
) {
    use panproto_reference::schema as rs;
    let field = |name: &str| prop("note", "label", name);
    let sfield = |name: &str| reference_model::SEdge::new("note", "label", "prop", Some(name));
    match text {
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
    }
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
fn declared_get_and_put_match_tree_to_graph_production_on_the_resolution_scope() {
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
fn all_five_resolution_laws_agree_on_the_exact_resolution_scope() {
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
        let (resolution, picked) = picks();
        let (production_view, production_complement) = production_get(&resolution, &source);
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
            let production_candidate = production_edit(&production_view, &view, &picked, &step);
            let edge_ids: BTreeSet<_> = pick_edges(&production_candidate, &picked).into_values()
                .map(|id| id.0).chain(production_complement.occurrences.keys().filter_map(|carrier|
                    match carrier { ViewRef::Edge(id) => Some(id.0), _ => None })).collect();
            let production_renaming: BTreeMap<_, _> = edge_ids.into_iter().enumerate()
                .map(|(rank, id)| (id, offset + u32::try_from(rank).unwrap())).collect();
            let (production_renamed, transported) = rename_production(&production_candidate,
                &production_complement, &production_renaming);
            let mut expected_production: Vec<_> = production_violations(&resolution,
                &production_candidate, &production_complement).iter()
                .map(|error| renamed_production_violation(error, &production_renaming)).collect();
            expected_production.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
            expected_production.dedup();
            proptest::prop_assert_eq!(production_violations(&resolution, &production_renamed, &transported),
                expected_production);
            let before = resolution.put(&AnyInstance::Graph(production_candidate),
                &production_complement, &ResolveContext::empty());
            let after = resolution.put(&AnyInstance::Graph(production_renamed),
                &transported, &ResolveContext::empty());
            proptest::prop_assert_eq!(before.is_ok(), after.is_ok());
            if let (Ok(before), Ok(after)) = (before, after) {
                proptest::prop_assert!(same_instance(&before, &after));
            }
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
    let first = tree.nodes.keys().next_back().copied().unwrap_or(0) + 1;
    for (id, iri) in (first..).zip(iris) {
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
                    let (images, written, admitted_here) = written_forms_agree(
                        text,
                        unit_len,
                        origin,
                        end_is_length,
                        *reference_unit,
                        production_unit,
                    );
                    written_cases += written;
                    admitted += admitted_here;
                    // Left inverse: decoding the written form of every byte
                    // span the unit can express returns that span.
                    byte_cases += byte_spans_agree(
                        text,
                        origin,
                        end_is_length,
                        *reference_unit,
                        production_unit,
                        &images,
                    );
                }
            }
        }
    }
    eprintln!("written={written_cases} admitted={admitted} byte_spans={byte_cases}");
    assert!(admitted > 0 && byte_cases > 0);
}

/// Each admitted byte span with the written form that names it.
type StandoffImages = BTreeMap<(u64, u64), (u64, u64)>;

/// The images of every written standoff pair up to one past `text`, with the
/// number of pairs and the number admitted, after checking that production
/// and the reference decode each pair alike and that each admitted pair is
/// the only form of its span and is reproduced by both inverses.
fn written_forms_agree(
    text: &str,
    unit_len: u64,
    origin: u64,
    end_is_length: bool,
    reference_unit: reference_schema::OffsetUnit,
    production_unit: &panproto_schema::OffsetUnit,
) -> (StandoffImages, usize, usize) {
    use panproto_io::resolution::{bytes_to_written_form, written_form_to_bytes};
    use panproto_reference::resolution::{standoff_byte_inverse, standoff_byte_span};

    let bytes = text.as_bytes();
    let end_form = if end_is_length {
        SpanEnd::Length
    } else {
        SpanEnd::Offset
    };
    // Every written pair up to one past the text, so that
    // out-of-range and in-unit offsets are refused alike.
    let mut images = BTreeMap::new();
    let (mut written_cases, mut admitted) = (0usize, 0usize);
    for start in 0..=unit_len + origin + 1 {
        for end in 0..=unit_len + origin + 1 {
            written_cases += 1;
            let reference =
                standoff_byte_span(text, (start, end), origin, end_is_length, reference_unit);
            let production =
                written_form_to_bytes(bytes, (start, end), origin, end_form, production_unit);
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
                standoff_byte_inverse(text, span, origin, end_is_length, reference_unit),
                Some((start, end))
            );
            assert_eq!(
                bytes_to_written_form(bytes, span, origin, end_form, production_unit),
                Some((start, end))
            );
        }
    }
    (images, written_cases, admitted)
}

/// The number of byte spans of `text`, after checking that production and
/// the reference write each span alike and that decoding the written form
/// returns the span, or, for a span with no written form, that no admitted
/// form in `images` decodes to it.
fn byte_spans_agree(
    text: &str,
    origin: u64,
    end_is_length: bool,
    reference_unit: reference_schema::OffsetUnit,
    production_unit: &panproto_schema::OffsetUnit,
    images: &StandoffImages,
) -> usize {
    use panproto_io::resolution::bytes_to_written_form;
    use panproto_reference::resolution::{standoff_byte_inverse, standoff_byte_span};

    let bytes = text.as_bytes();
    let byte_len = u64::try_from(bytes.len()).expect("short text");
    let end_form = if end_is_length {
        SpanEnd::Length
    } else {
        SpanEnd::Offset
    };
    let mut byte_cases = 0usize;
    for byte_start in 0..=byte_len {
        for byte_end in byte_start..=byte_len {
            byte_cases += 1;
            let span = (byte_start, byte_end);
            let written = standoff_byte_inverse(text, span, origin, end_is_length, reference_unit);
            assert_eq!(
                bytes_to_written_form(bytes, span, origin, end_form, production_unit),
                written,
                "encode {text:?} {reference_unit:?} {span:?}"
            );
            if let Some(written) = written {
                assert_eq!(
                    standoff_byte_span(text, written, origin, end_is_length, reference_unit),
                    Some(span)
                );
            } else {
                assert!(images.values().all(|form| standoff_byte_span(
                    text,
                    *form,
                    origin,
                    end_is_length,
                    reference_unit
                ) != Some(span)));
            }
        }
    }
    byte_cases
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
                    arcs.push(parent.as_ref().map_or_else(
                        || uri_arc(id, "el"),
                        |parent| {
                            (
                                u32::try_from(*parent).expect("small") + 1,
                                id,
                                reference_model::SEdge::new("el", "el", "prop", Some("child")),
                            )
                        },
                    ));
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

/// Convert only the literal carriers used by the syntax-preservation matrix.
fn syntax_literal(value: &ReferenceValue) -> Value {
    match value {
        ReferenceValue::Null => Value::Null,
        ReferenceValue::Int(value) => Value::Int(*value),
        ReferenceValue::Str(value) => text(value),
        ReferenceValue::List(values) => Value::List(values.iter().map(syntax_literal).collect()),
        _ => panic!("the syntax matrix uses null, integer, string and list literals"),
    }
}

fn assert_many_syntax_round_trip(literal: Option<ReferenceValue>, redirect: bool) {
    let seed = IndexedSource::from(["a".to_owned()]);
    let (mut schema, mut expected_source, _) = reference_pick_tree(&seed);
    schema.references.get_mut("pick").unwrap().mode = if redirect {
        reference_schema::ResolveMode::Redirect
    } else {
        reference_schema::ResolveMode::AddEdge
    };
    let mut actual_source = production_pick_tree(&seed);
    if let Some(value) = literal {
        actual_source
            .nodes
            .get_mut(&1)
            .unwrap()
            .extra_fields
            .insert("customer".to_owned(), syntax_literal(&value));
        expected_source
            .nodes
            .get_mut(&1)
            .unwrap()
            .fields
            .insert("customer".to_owned(), value);
    } else {
        actual_source
            .nodes
            .get_mut(&1)
            .unwrap()
            .extra_fields
            .remove("customer");
        expected_source
            .nodes
            .get_mut(&1)
            .unwrap()
            .fields
            .remove("customer");
    }
    let (view, complement) = reference_get(&schema, &expected_source, true).unwrap();
    assert_eq!(
        reference_put(&schema, &view, &complement),
        Ok(expected_source)
    );
    let mode = if redirect {
        ResolveMode::Redirect
    } else {
        ResolveMode::AddEdge
    };
    let (resolution, _) = try_tree_picks_with_equality_and_mode(Vec::new(), mode).unwrap();
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(actual_source.clone()),
            &ResolveOptions { lenient: true },
            &ResolveContext::empty(),
        )
        .unwrap();
    let restored = resolution
        .put(&view, &complement, &ResolveContext::empty())
        .unwrap();
    assert!(same_instance(&restored, &AnyInstance::Tree(actual_source)));
}

#[test]
fn many_resolution_get_put_preserves_scalar_lists_and_null_slots() {
    use ReferenceValue::{List, Null, Str};
    let a = || Str("a".to_owned());
    let b = || Str("b".to_owned());
    let cases = [
        None,
        Some(Null),
        Some(List(vec![])),
        Some(a()),
        Some(List(vec![a()])),
        Some(List(vec![Null, a()])),
        Some(List(vec![a(), Null])),
        Some(List(vec![Null, a(), Null, b()])),
        Some(List(vec![a(), a()])),
        Some(List(vec![Null, Null])),
        Some(List(vec![Null, Str("missing".to_owned()), a()])),
    ];
    assert_eq!(cases.len(), 11);
    for redirect in [false, true] {
        for value in &cases {
            assert_many_syntax_round_trip(value.clone(), redirect);
        }
    }
}

#[test]
fn many_resolution_preservation_still_checks_changed_edges() {
    let seed = IndexedSource::from(["a".to_owned()]);
    for redirect in [false, true] {
        let (mut schema, mut source, _) = reference_pick_tree(&seed);
        schema.references.get_mut("pick").unwrap().mode = if redirect {
            reference_schema::ResolveMode::Redirect
        } else {
            reference_schema::ResolveMode::AddEdge
        };
        let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
        let carrier = *complement.resolved.keys().next().unwrap();
        view.edges.get_mut(&carrier).unwrap().tgt = 3;
        let expected = reference_put(&schema, &view, &complement);
        let mode = if redirect {
            ResolveMode::Redirect
        } else {
            ResolveMode::AddEdge
        };
        let (resolution, _) = try_tree_picks_with_equality_and_mode(Vec::new(), mode).unwrap();
        let actual_source = production_pick_tree(&seed);
        let (actual_view, actual_complement) = resolution
            .get(
                &AnyInstance::Tree(actual_source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        let AnyInstance::Graph(mut actual_view) = actual_view else {
            panic!("graph expected")
        };
        let actual_carrier = actual_complement
            .occurrences
            .keys()
            .find_map(|carrier| match carrier {
                ViewRef::Edge(id) => Some(*id),
                _ => None,
            })
            .unwrap();
        let parent = actual_view.edge(actual_carrier).unwrap().src;
        actual_view
            .set_endpoints(actual_carrier, parent, 3)
            .unwrap();
        let actual = resolution.put(
            &AnyInstance::Graph(actual_view),
            &actual_complement,
            &ResolveContext::empty(),
        );
        if redirect {
            source.nodes.get_mut(&1).unwrap().fields.insert(
                "customer".to_owned(),
                ReferenceValue::List(vec![ReferenceValue::Str("b".to_owned())]),
            );
            assert_eq!(expected, Ok(source));
            let mut rewritten = actual_source;
            rewritten
                .nodes
                .get_mut(&1)
                .unwrap()
                .extra_fields
                .insert("customer".to_owned(), Value::List(vec![text("b")]));
            assert!(same_instance(
                &actual.unwrap(),
                &AnyInstance::Tree(rewritten)
            ));
        } else {
            assert!(matches!(
                expected,
                Err(panproto_reference::resolution::ResolveError::UnrepresentableReference { .. })
            ));
            assert!(matches!(actual, Err(ResolveError::PutDomain(_))));
            view.edges.remove(&carrier);
            assert!(reference_put(&schema, &view, &complement).is_err());
        }
    }
}

#[test]
fn many_composite_null_matching_preserves_each_original_part() {
    use ReferenceValue::{Int, List, Null};
    let cases = [
        (Int(1), Int(2)),
        (
            List(vec![Null, Int(1), Null, Int(1)]),
            List(vec![Null, Int(2), Int(2), Null]),
        ),
        (List(vec![Null, Null]), List(vec![Null, Null])),
    ];
    for full in [false, true] {
        for redirect in [false, true] {
            for (left, right) in &cases {
                let reference_mode = if full {
                    reference_schema::NullMatch::Full
                } else {
                    reference_schema::NullMatch::Simple
                };
                let (mut schema, source, _) =
                    reference_null_match(reference_mode, left.clone(), right.clone());
                let declaration = schema.references.get_mut("pair").unwrap();
                declaration.cardinality = reference_schema::Cardinality::Many;
                declaration.nullable = true;
                declaration.mode = if redirect {
                    reference_schema::ResolveMode::Redirect
                } else {
                    reference_schema::ResolveMode::AddEdge
                };
                let (view, complement) = reference_get(&schema, &source, true).unwrap();
                assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
                let mode = if redirect {
                    ResolveMode::Redirect
                } else {
                    ResolveMode::AddEdge
                };
                let null_match = if full {
                    NullMatch::Full
                } else {
                    NullMatch::Simple
                };
                let (resolution, _) =
                    null_match_resolution_with(null_match, Cardinality::Many, mode);
                let source = null_match_tree(syntax_literal(left), syntax_literal(right));
                let (view, complement) = resolution
                    .get(
                        &AnyInstance::Tree(source.clone()),
                        &ResolveOptions { lenient: true },
                        &ResolveContext::empty(),
                    )
                    .unwrap();
                let restored = resolution
                    .put(&view, &complement, &ResolveContext::empty())
                    .unwrap();
                assert!(same_instance(&restored, &AnyInstance::Tree(source)));
            }
        }
    }
}

#[test]
fn many_path_and_variable_scalar_sites_round_trip_exactly() {
    let seed = IndexedSource::from(["a".to_owned()]);
    let (mut schema, mut source, _) = reference_pick_tree(&seed);
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("customer".to_owned(), ReferenceValue::Str("a".to_owned()));
    schema.references.get_mut("pick").unwrap().source = reference_schema::RefSource::Path {
        field: vec![reference_model::SEdge::new(
            "pick",
            "label",
            "prop",
            Some("customer"),
        )],
        syntax: reference_schema::PathSyntax::IdRef,
    };
    let (view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
    let (resolution, _) = tree_picks_with_source(
        Vec::new(),
        ResolveMode::AddEdge,
        Some(RefSource::Path {
            field: vec![prop("pick", "label", "customer")],
            syntax: panproto_schema::PathSyntax::IdRef,
        }),
    )
    .unwrap();
    let mut source = production_pick_tree(&seed);
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .extra_fields
        .insert("customer".to_owned(), text("a"));
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    assert!(same_instance(
        &resolution
            .put(&view, &complement, &ResolveContext::empty())
            .unwrap(),
        &AnyInstance::Tree(source)
    ));

    let (mut schema, source, _) = reference_variable_tree(
        &["x", "x"],
        reference_schema::VariableBinding::FirstDefines,
        Vec::new(),
    );
    schema.references.get_mut("variable").unwrap().cardinality =
        reference_schema::Cardinality::Many;
    let (view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
    let (resolution, _) = variable_resolution_with_cardinality(
        VariableBinding::FirstDefines,
        Vec::new(),
        Cardinality::Many,
    );
    let source = production_variable_tree(&["x", "x"]);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    assert!(same_instance(
        &resolution
            .put(&view, &complement, &ResolveContext::empty())
            .unwrap(),
        &AnyInstance::Tree(source)
    ));
}

fn syntax_standoff_resolution() -> (Resolution, reference_schema::Schema) {
    let (declarations, mut reference_schema) =
        deferred_declaration(DeclaredSource::Standoff(false, 0), false, false, false);
    let mut protocol = protocol("syntax-standoff");
    protocol.syntactic_shape = InstanceShapeKind::Tree;
    let mut builder = SchemaBuilder::new(&protocol);
    for (id, kind) in [
        ("doc", "object"),
        ("note", "object"),
        ("label", "scalar"),
        ("target", "object"),
    ] {
        builder = builder.vertex(id, kind, None).unwrap();
        reference_schema
            .vertices
            .insert(id.to_owned(), kind.to_owned());
    }
    for (src, tgt, kind, name) in [
        ("doc", "note", "prop", "note"),
        ("note", "label", "prop", "text"),
        ("note", "label", "prop", "start"),
        ("note", "label", "prop", "end"),
        ("note", "target", "ref", "mention"),
    ] {
        builder = builder.edge(src, tgt, kind, Some(name)).unwrap();
        reference_schema
            .edges
            .insert(reference_model::SEdge::new(src, tgt, kind, Some(name)));
    }
    let schema = builder
        .entry("doc")
        .reference(declarations.declarations.references["mention"].clone())
        .build()
        .unwrap();
    reference_schema.entries.push("doc".to_owned());
    let mention = edge("note", "target", "ref", Some("mention"));
    let provider = Provider {
        protocol: protocol.clone(),
        kind: ResolutionKind::TreeToGraph,
        spec: ResolutionSpec {
            kind: ResolutionKind::TreeToGraph,
            provider: ResolutionProviderId {
                name: "syntax-standoff".into(),
                version: 1,
            },
            semantic_schema: schema.clone(),
            protocol_references: BTreeMap::new(),
            structural_edges: schema
                .edges
                .keys()
                .filter(|edge| **edge != mention)
                .cloned()
                .collect(),
            reference_writers: BTreeMap::from([(mention, "mention".into())]),
            context_requirements: BTreeSet::new(),
            assembly: None,
        },
    };
    (
        Resolution::for_protocol(&protocol, &schema, Arc::new(provider)).unwrap(),
        reference_schema,
    )
}

#[test]
fn nullable_standoff_source_sites_round_trip_exactly() {
    let (resolution, schema) = syntax_standoff_resolution();
    for coordinates in [
        None,
        Some((ReferenceValue::Null, ReferenceValue::Null)),
        Some((ReferenceValue::Int(0), ReferenceValue::Int(1))),
    ] {
        let mut note = reference_model::Node::new(1, "note")
            .with_field("text", ReferenceValue::Str("abc".to_owned()));
        let mut actual_note = Node::new(1, "note");
        actual_note
            .extra_fields
            .insert("text".to_owned(), text("abc"));
        if let Some((start, end)) = coordinates {
            for (name, value) in [("start", start), ("end", end)] {
                actual_note
                    .extra_fields
                    .insert(name.to_owned(), syntax_literal(&value));
                note.fields.insert(name.to_owned(), value);
            }
        }
        let source = reference_model::Tree {
            nodes: BTreeMap::from([(0, reference_model::Node::new(0, "doc")), (1, note)]),
            arcs: vec![(
                0,
                1,
                reference_model::SEdge::new("doc", "note", "prop", Some("note")),
            )],
            fans: Vec::new(),
            root: 0,
        };
        let actual_source = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (1, actual_note)]),
            vec![(0, 1, prop("doc", "note", "note"))],
            Vec::new(),
            0,
            "doc".into(),
        );
        let (view, complement) = reference_get(&schema, &source, false).unwrap();
        assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
        let (view, complement) = resolution
            .get(
                &AnyInstance::Tree(actual_source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        let restored = resolution
            .put(&view, &complement, &ResolveContext::empty())
            .unwrap();
        assert!(same_instance(&restored, &AnyInstance::Tree(actual_source)));
    }
}

#[test]
fn synthesized_reference_edges_do_not_shadow_original_source_sites() {
    let seed = IndexedSource::from(["a".to_owned()]);
    let (mut schema, mut source, picked) = reference_pick_tree(&seed);
    schema.references.get_mut("pick").unwrap().source = reference_schema::RefSource::KeyValue {
        parts: vec![vec![picked]],
    };
    let holder = source.nodes.get_mut(&1).unwrap();
    let value = holder.fields.remove("customer").unwrap();
    holder.fields.insert("picked".to_owned(), value);
    let (view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
    let picked = edge("pick", "customer", "ref", Some("picked"));
    let (resolution, _) = tree_picks_with_source(
        Vec::new(),
        ResolveMode::AddEdge,
        Some(RefSource::KeyValue {
            parts: vec![vec![picked]],
        }),
    )
    .unwrap();
    let mut source = production_pick_tree(&seed);
    let holder = source.nodes.get_mut(&1).unwrap();
    let value = holder.extra_fields.remove("customer").unwrap();
    holder.extra_fields.insert("picked".to_owned(), value);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    let restored = resolution
        .put(&view, &complement, &ResolveContext::empty())
        .unwrap();
    assert!(same_instance(&restored, &AnyInstance::Tree(source)));
}

#[test]
fn original_structural_reference_labels_keep_their_source_edge_slots() {
    let seed = IndexedSource::from(["a".to_owned()]);
    let (schema, mut source, picked) = reference_pick_tree(&seed);
    source.arcs.retain(|(_, target, _)| *target != 2);
    source.arcs.push((1, 2, picked));
    let (encoded, _) = panproto_reference::encode::tree_to_graph(&source, Some(&schema)).unwrap();
    let (view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(
        complement.source_edges,
        encoded.edges.keys().copied().collect::<BTreeSet<_>>()
    );
    assert!(
        complement
            .resolved
            .keys()
            .all(|id| !complement.source_edges.contains(id))
    );
    assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
    let (resolution, picked) = tree_picks_with_equality(Vec::new());
    let mut source = production_pick_tree(&seed);
    source.arcs.retain(|(_, target, _)| *target != 2);
    source.arcs.push((1, 2, picked));
    let source = WInstance::new(
        source.nodes,
        source.arcs,
        source.fans,
        source.root,
        source.schema_root,
    );
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    let restored = resolution
        .put(&view, &complement, &ResolveContext::empty())
        .unwrap();
    assert!(same_instance(&restored, &AnyInstance::Tree(source)));
}

#[test]
fn redirected_source_edge_slots_are_distinguished_from_resolution_carriers() {
    for attachment_last in [false, true] {
        let seed = IndexedSource::from(["a".to_owned()]);
        let (mut schema, mut source, _) = reference_pick_tree(&seed);
        schema.references.get_mut("pick").unwrap().mode = reference_schema::ResolveMode::Redirect;
        if attachment_last {
            // The removed largest slot can be reused by a recorded resolution carrier.
            let attachment = source.arcs.remove(0);
            source.arcs.push(attachment);
        }
        let (encoded, _) =
            panproto_reference::encode::tree_to_graph(&source, Some(&schema)).unwrap();
        let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
        assert_eq!(
            complement.source_edges,
            encoded.edges.keys().copied().collect::<BTreeSet<_>>()
        );
        let source_slot = *complement.redirected[&1].edges.keys().next().unwrap();
        let synthesized = *complement.resolved.keys().next().unwrap();
        assert_eq!(source_slot == synthesized, attachment_last);
        assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
        if !attachment_last {
            // Reusing an unrecorded hidden slot cannot turn it into a structural edge.
            view.edges
                .insert(source_slot, view.edges[&synthesized].clone());
            assert_eq!(
                reference_put(&schema, &view, &complement),
                Err(
                    panproto_reference::resolution::ResolveError::UnrepresentableReference {
                        carrier: panproto_reference::lens::ViewRef::Edge(source_slot),
                        reference: None,
                    }
                )
            );
        }
        let (resolution, _) =
            try_tree_picks_with_equality_and_mode(Vec::new(), ResolveMode::Redirect).unwrap();
        let mut source = production_pick_tree(&seed);
        if attachment_last {
            let attachment = source.arcs.remove(0);
            source.arcs.push(attachment);
        }
        let source = WInstance::new(
            source.nodes,
            source.arcs,
            source.fans,
            source.root,
            source.schema_root,
        );
        let (view, complement) = resolution
            .get(
                &AnyInstance::Tree(source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        let carrier = complement
            .occurrences
            .keys()
            .find_map(|carrier| match carrier {
                ViewRef::Edge(id) => Some(id.0),
                _ => None,
            })
            .unwrap();
        assert_eq!(carrier, synthesized);
        let restored = resolution
            .put(&view, &complement, &ResolveContext::empty())
            .unwrap();
        assert!(same_instance(&restored, &AnyInstance::Tree(source)));
    }
}

/// These word edits preserve the source's structural carriers and add only
/// reference edges. Compare every structural id and all node/edge payloads and
/// multiplicities, normalizing both recorded and newly appended reference ids.
fn canonical_reference_resolution_view(
    view: &reference_model::Graph,
    complement: &panproto_reference::resolution::ResComplement,
) -> reference_model::Graph {
    let mut view = view.clone();
    let mut synthesized = Vec::new();
    view.edges.retain(|id, edge| {
        if complement.resolved.contains_key(id) || !complement.source_edges.contains(id) {
            synthesized.push(edge.clone());
            false
        } else {
            true
        }
    });
    synthesized.sort_by(|left, right| {
        (left.src, left.tgt, &left.label, left.position).cmp(&(
            right.src,
            right.tgt,
            &right.label,
            right.position,
        ))
    });
    let first = view.edges.keys().next_back().map_or(0, |id| id + 1);
    for (index, edge) in synthesized.into_iter().enumerate() {
        view.edges
            .insert(first + u32::try_from(index).unwrap(), edge);
    }
    view
}

fn canonical_production_resolution_view(
    view: &GInstance,
    complement: &ResolutionComplement,
) -> serde_json::Value {
    let mut structural = Vec::new();
    let mut synthesized = Vec::new();
    for (id, edge) in view.edges_sorted() {
        let payload = (edge.src, edge.tgt, edge.edge.clone(), edge.position);
        if complement.occurrences.contains_key(&ViewRef::Edge(id))
            || !complement
                .spanning
                .values()
                .any(|carrier| *carrier == ViewRef::Edge(id))
        {
            synthesized.push(payload);
        } else {
            structural.push((id, payload));
        }
    }
    synthesized.sort();
    serde_json::json!([view.nodes(), structural, synthesized])
}

fn edit_reference_word_view(
    view: &mut reference_model::Graph,
    complement: &panproto_reference::resolution::ResComplement,
    edits: &[(u32, Option<u32>)],
) {
    for (index, target) in edits {
        let ids: Vec<_> = complement
            .resolved
            .iter()
            .filter(|(_, record)| record.index == *index)
            .map(|(id, _)| *id)
            .collect();
        assert!(
            !ids.is_empty(),
            "an original occurrence has the requested index"
        );
        for id in ids {
            if let Some(target) = target {
                view.edges.get_mut(&id).unwrap().tgt = *target;
            } else {
                view.edges.remove(&id);
            }
        }
    }
}

fn edit_production_word_view(
    view: &mut GInstance,
    complement: &ResolutionComplement,
    edits: &[(u32, Option<u32>)],
) {
    for (index, target) in edits {
        let carriers: Vec<_> = complement
            .occurrences
            .iter()
            .filter_map(|(carrier, records)| match (carrier, records.as_slice()) {
                (ViewRef::Edge(id), [record]) if record.index == *index => Some(*id),
                _ => None,
            })
            .collect();
        assert!(
            !carriers.is_empty(),
            "an original occurrence has the requested index"
        );
        for carrier in carriers {
            if let Some(target) = target {
                let parent = view.edge(carrier).unwrap().src;
                view.set_endpoints(carrier, parent, *target).unwrap();
            } else {
                view.remove_edge(carrier).unwrap();
            }
        }
    }
}

fn assert_reference_word_result(
    schema: &reference_schema::Schema,
    view: &reference_model::Graph,
    complement: &panproto_reference::resolution::ResComplement,
    expected: &reference_model::Tree,
) -> Vec<Value> {
    let restored = reference_put(schema, view, complement).unwrap();
    assert_eq!(&restored, expected);
    let (regotten, recomplement) = reference_get(schema, &restored, true).unwrap();
    assert_eq!(
        canonical_reference_resolution_view(view, complement),
        canonical_reference_resolution_view(&regotten, &recomplement)
    );
    recomplement
        .unresolved
        .iter()
        .map(|(record, _)| syntax_literal(&record.literal))
        .collect()
}

fn assert_production_word_result(
    resolution: &Resolution,
    view: &GInstance,
    complement: &ResolutionComplement,
    expected: &WInstance,
) -> Vec<Value> {
    let restored = resolution
        .put(
            &AnyInstance::Graph(view.clone()),
            complement,
            &ResolveContext::empty(),
        )
        .unwrap();
    assert!(same_instance(
        &restored,
        &AnyInstance::Tree(expected.clone())
    ));
    let (regotten, after_complement) = resolution
        .get(
            &restored,
            &ResolveOptions { lenient: true },
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(regotten) = regotten else {
        panic!("graph view")
    };
    assert_eq!(
        canonical_production_resolution_view(view, complement),
        canonical_production_resolution_view(&regotten, &after_complement)
    );
    after_complement
        .unresolved
        .into_iter()
        .map(|record| record.literal)
        .collect()
}

fn assert_changed_key_word(
    before: ReferenceValue,
    edits: &[(u32, Option<u32>)],
    after: ReferenceValue,
) {
    let actual_before = syntax_literal(&before);
    let actual_after = syntax_literal(&after);
    let seed = IndexedSource::from(["a".to_owned()]);
    let (mut schema, mut source, _) = reference_pick_tree(&seed);
    schema.references.get_mut("pick").unwrap().mode = reference_schema::ResolveMode::Redirect;
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("customer".to_owned(), before);
    let (mut view, complement) = reference_get(&schema, &source, true).unwrap();
    edit_reference_word_view(&mut view, &complement, edits);
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("customer".to_owned(), after);
    let expected_unresolved = assert_reference_word_result(&schema, &view, &complement, &source);
    let (resolution, _) =
        try_tree_picks_with_equality_and_mode(Vec::new(), ResolveMode::Redirect).unwrap();
    let mut source = production_pick_tree(&seed);
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .extra_fields
        .insert("customer".to_owned(), actual_before);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions { lenient: true },
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(mut view) = view else {
        panic!("graph view")
    };
    edit_production_word_view(&mut view, &complement, edits);
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .extra_fields
        .insert("customer".to_owned(), actual_after);
    let actual_unresolved = assert_production_word_result(&resolution, &view, &complement, &source);
    assert_eq!(actual_unresolved, expected_unresolved);
}

#[test]
fn changed_many_key_words_preserve_indices_and_unresolved_literals() {
    use ReferenceValue::{List, Null, Str};
    let a = || Str("a".to_owned());
    let b = || Str("b".to_owned());
    assert_changed_key_word(List(vec![Null, a()]), &[(0, Some(3))], List(vec![b()]));
    assert_changed_key_word(
        List(vec![Null, a(), Null, b()]),
        &[(0, None), (1, Some(4))],
        List(vec![Str("c".to_owned())]),
    );
    assert_changed_key_word(
        List(vec![Null, a(), Str("missing".to_owned()), Null, b()]),
        &[(0, Some(3))],
        List(vec![b(), Str("missing".to_owned()), b()]),
    );
    assert_changed_key_word(a(), &[(0, Some(3))], List(vec![b()]));
    assert_changed_key_word(a(), &[], a());
    assert_changed_key_word(List(vec![Null, a()]), &[], List(vec![Null, a()]));
}

fn resolution_for_word_schema(schema: &Schema, protocol_name: &str) -> Resolution {
    let mut protocol = protocol(protocol_name);
    protocol.syntactic_shape = InstanceShapeKind::Tree;
    let writers: BTreeMap<_, _> = schema
        .declarations
        .references
        .values()
        .filter_map(|reference| {
            reference
                .edge
                .clone()
                .map(|edge| (edge, reference.id.clone()))
        })
        .collect();
    let spec = ResolutionSpec {
        kind: ResolutionKind::TreeToGraph,
        provider: ResolutionProviderId {
            name: "word-writer".into(),
            version: 1,
        },
        semantic_schema: schema.clone(),
        protocol_references: BTreeMap::new(),
        structural_edges: schema
            .edges
            .keys()
            .filter(|edge| !writers.contains_key(*edge))
            .cloned()
            .collect(),
        reference_writers: writers,
        context_requirements: BTreeSet::new(),
        assembly: None,
    };
    let provider = Provider {
        protocol: protocol.clone(),
        spec,
        kind: ResolutionKind::TreeToGraph,
    };
    Resolution::for_protocol(&protocol, schema, Arc::new(provider)).unwrap()
}

#[test]
fn overlapping_many_key_writers_use_each_original_word() {
    let seed = IndexedSource::from(["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    let (mut schema, mut source, _) = reference_pick_tree(&seed);
    schema.references.get_mut("pick").unwrap().mode = reference_schema::ResolveMode::Redirect;
    let mut second = schema.references["pick"].clone();
    second.id = "second".to_owned();
    let second_edge = reference_model::SEdge::new("pick", "customer", "ref", Some("second"));
    second.edge = Some(second_edge.clone());
    schema.edges.insert(second_edge);
    schema.references.insert("second".to_owned(), second);
    let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
    let edits = [(0, None), (1, Some(4))];
    edit_reference_word_view(&mut view, &complement, &edits);
    source.nodes.get_mut(&1).unwrap().fields.insert(
        "customer".to_owned(),
        ReferenceValue::List(vec![ReferenceValue::Str("c".to_owned()); 2]),
    );
    assert_eq!(
        assert_reference_word_result(&schema, &view, &complement, &source).len(),
        0
    );
    let (base, picked) =
        try_tree_picks_with_equality_and_mode(Vec::new(), ResolveMode::Redirect).unwrap();
    let mut schema = base.syntactic_schema().clone();
    let mut second = schema.declarations.references[&Name::from("pick")].clone();
    second.id = "second".into();
    let second_edge = edge("pick", "customer", "ref", Some("second"));
    second.edge = Some(second_edge.clone());
    schema
        .edges
        .insert(second_edge, schema.edges[&picked].clone());
    schema.declarations.insert_reference(second);
    let resolution = resolution_for_word_schema(&schema, "reference-tree-picks");
    let mut source = production_pick_tree(&seed);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(mut view) = view else {
        panic!("graph view")
    };
    edit_production_word_view(&mut view, &complement, &edits);
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .extra_fields
        .insert("customer".to_owned(), Value::List(vec![text("c"); 2]));
    assert_eq!(
        assert_production_word_result(&resolution, &view, &complement, &source).len(),
        0
    );
}

fn composite_reference_word_case(full: bool) -> (reference_schema::Schema, reference_model::Tree) {
    use ReferenceValue::{Int, List, Null};
    let mode = if full {
        reference_schema::NullMatch::Full
    } else {
        reference_schema::NullMatch::Simple
    };
    let (mut schema, mut source, _) = reference_null_match(
        mode,
        List(vec![Null, Int(1), Null]),
        List(vec![Int(2), Int(2), Null]),
    );
    let reference = schema.references.get_mut("pair").unwrap();
    reference.cardinality = reference_schema::Cardinality::Many;
    reference.nullable = true;
    reference.mode = reference_schema::ResolveMode::Redirect;
    source.nodes.insert(
        3,
        reference_model::Node::new(3, "customer")
            .with_field("left_id", Int(3))
            .with_field("right_id", Int(4)),
    );
    source.arcs.push((
        0,
        3,
        reference_model::SEdge::new("doc", "customer", "prop", Some("customer")),
    ));
    (schema, source)
}

fn composite_production_word_case(full: bool) -> (Resolution, WInstance) {
    let mode = if full {
        NullMatch::Full
    } else {
        NullMatch::Simple
    };
    let (resolution, _) =
        null_match_resolution_with(mode, Cardinality::Many, ResolveMode::Redirect);
    let mut source = null_match_tree(
        Value::List(vec![Value::Null, Value::Int(1), Value::Null]),
        Value::List(vec![Value::Int(2), Value::Int(2), Value::Null]),
    );
    let mut target = Node::new(3, "customer");
    target
        .extra_fields
        .insert("left_id".to_owned(), Value::Int(3));
    target
        .extra_fields
        .insert("right_id".to_owned(), Value::Int(4));
    source.nodes.insert(3, target);
    source
        .arcs
        .push((0, 3, prop("doc", "customer", "customer")));
    (
        resolution,
        WInstance::new(
            source.nodes,
            source.arcs,
            source.fans,
            source.root,
            source.schema_root,
        ),
    )
}

#[test]
fn changed_composite_many_key_words_zip_before_null_filtering() {
    use ReferenceValue::{Int, List, Null};
    for full in [false, true] {
        let left = List(if full {
            vec![Null, Int(3)]
        } else {
            vec![Int(3)]
        });
        let right = List(if full {
            vec![Int(2), Int(4)]
        } else {
            vec![Int(4)]
        });
        let edits = [(u32::from(full), Some(3))];
        let (schema, mut source) = composite_reference_word_case(full);
        let (mut view, complement) = reference_get(&schema, &source, true).unwrap();
        edit_reference_word_view(&mut view, &complement, &edits);
        source
            .nodes
            .get_mut(&1)
            .unwrap()
            .fields
            .insert("left".to_owned(), left.clone());
        source
            .nodes
            .get_mut(&1)
            .unwrap()
            .fields
            .insert("right".to_owned(), right.clone());
        let expected_unresolved =
            assert_reference_word_result(&schema, &view, &complement, &source);
        assert_eq!(expected_unresolved.len(), usize::from(full));
        let (resolution, mut source) = composite_production_word_case(full);
        let (view, complement) = resolution
            .get(
                &AnyInstance::Tree(source.clone()),
                &ResolveOptions { lenient: true },
                &ResolveContext::empty(),
            )
            .unwrap();
        let AnyInstance::Graph(mut view) = view else {
            panic!("graph view")
        };
        edit_production_word_view(&mut view, &complement, &edits);
        source
            .nodes
            .get_mut(&1)
            .unwrap()
            .extra_fields
            .insert("left".to_owned(), syntax_literal(&left));
        source
            .nodes
            .get_mut(&1)
            .unwrap()
            .extra_fields
            .insert("right".to_owned(), syntax_literal(&right));
        assert_eq!(
            assert_production_word_result(&resolution, &view, &complement, &source),
            expected_unresolved
        );
    }
}

#[test]
fn many_variable_lists_preserve_each_occurrence_and_exact_source() {
    let (mut schema, mut source, _) = reference_variable_tree(
        &["x", "x"],
        reference_schema::VariableBinding::FirstDefines,
        Vec::new(),
    );
    schema.references.get_mut("variable").unwrap().cardinality =
        reference_schema::Cardinality::Many;
    source.nodes.get_mut(&2).unwrap().fields.insert(
        "variable".to_owned(),
        ReferenceValue::List(vec![ReferenceValue::Str("x".to_owned()); 2]),
    );
    let (view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(complement.resolved.len(), 2);
    assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
    let (resolution, _) = variable_resolution_with_cardinality(
        VariableBinding::FirstDefines,
        Vec::new(),
        Cardinality::Many,
    );
    let mut source = production_variable_tree(&["x", "x"]);
    source
        .nodes
        .get_mut(&2)
        .unwrap()
        .extra_fields
        .insert("variable".to_owned(), Value::List(vec![text("x"); 2]));
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    assert_eq!(
        complement.occurrences.len(),
        2,
        "production resolves both x occurrences"
    );
    let restored = resolution
        .put(&view, &complement, &ResolveContext::empty())
        .unwrap();
    assert!(same_instance(&restored, &AnyInstance::Tree(source)));
}

#[test]
fn many_idref_lists_preserve_each_path_literal_and_exact_source() {
    let seed = IndexedSource::from(["a".to_owned(), "b".to_owned()]);
    let (mut schema, source, _) = reference_pick_tree(&seed);
    schema.references.get_mut("pick").unwrap().source = reference_schema::RefSource::Path {
        field: vec![reference_model::SEdge::new(
            "pick",
            "label",
            "prop",
            Some("customer"),
        )],
        syntax: reference_schema::PathSyntax::IdRef,
    };
    let (view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(complement.resolved.len(), 2);
    assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
    let (resolution, _) = tree_picks_with_source(
        Vec::new(),
        ResolveMode::AddEdge,
        Some(RefSource::Path {
            field: vec![prop("pick", "label", "customer")],
            syntax: panproto_schema::PathSyntax::IdRef,
        }),
    )
    .unwrap();
    let source = production_pick_tree(&seed);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    assert_eq!(
        complement.occurrences.len(),
        2,
        "production resolves both path texts"
    );
    let restored = resolution
        .put(&view, &complement, &ResolveContext::empty())
        .unwrap();
    assert!(same_instance(&restored, &AnyInstance::Tree(source)));
}

#[test]
fn many_variable_definitions_remain_in_the_exact_source_word() {
    for redirect in [false, true] {
        let (mut schema, mut source, _) = reference_variable_tree(
            &["x", "y"],
            reference_schema::VariableBinding::FirstDefines,
            Vec::new(),
        );
        let declaration = schema.references.get_mut("variable").unwrap();
        declaration.cardinality = reference_schema::Cardinality::Many;
        declaration.mode = if redirect {
            reference_schema::ResolveMode::Redirect
        } else {
            reference_schema::ResolveMode::AddEdge
        };
        source.nodes.get_mut(&2).unwrap().fields.insert(
            "variable".into(),
            ReferenceValue::List(vec![
                ReferenceValue::Str("x".into()),
                ReferenceValue::Str("y".into()),
            ]),
        );
        let (view, complement) = reference_get(&schema, &source, false).unwrap();
        assert_eq!(complement.resolved.len(), 1);
        assert_eq!(complement.unresolved.len(), 0);
        let restored = reference_put(&schema, &view, &complement);
        assert_eq!(
            restored,
            Ok(source.clone()),
            "definitions retain their exact source positions"
        );
        assert_eq!(
            complement
                .definition_occurrences
                .iter()
                .map(|record| (record.holder, record.index, record.literal.clone()))
                .collect::<Vec<_>>(),
            vec![
                (1, 0, ReferenceValue::Str("x".into())),
                (2, 1, ReferenceValue::Str("y".into()))
            ]
        );

        let (base, _) = variable_resolution_with_cardinality(
            VariableBinding::FirstDefines,
            Vec::new(),
            Cardinality::Many,
        );
        let mut actual_schema = base.syntactic_schema().clone();
        actual_schema
            .declarations
            .references
            .get_mut("variable")
            .unwrap()
            .mode = if redirect {
            ResolveMode::Redirect
        } else {
            ResolveMode::AddEdge
        };
        let resolution = resolution_for_word_schema(&actual_schema, "reference-variable-binding");
        let actual_source = plain_reader_tree(&source);
        let (view, complement) = resolution
            .get(
                &AnyInstance::Tree(actual_source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        assert_eq!(complement.occurrences.len(), 1);
        assert_eq!(complement.unresolved.len(), 0);
        assert_eq!(
            complement
                .definition_occurrences
                .iter()
                .map(|record| (record.holder.clone(), record.index, record.literal.clone()))
                .collect::<Vec<_>>(),
            vec![
                (ViewRef::Node(1), 0, text("x")),
                (ViewRef::Node(2), 1, text("y"))
            ]
        );
        let restored = resolution
            .put(&view, &complement, &ResolveContext::empty())
            .unwrap();
        assert!(same_instance(&restored, &AnyInstance::Tree(actual_source)));
    }
}

#[test]
fn many_variable_binding_uses_occurrence_positions() {
    for same_holder in [false, true] {
        let names: &[&str] = if same_holder { &["x"] } else { &["x", "y"] };
        let (mut schema, mut source, _) = reference_variable_tree(
            names,
            reference_schema::VariableBinding::FirstDefines,
            Vec::new(),
        );
        schema.references.get_mut("variable").unwrap().cardinality =
            reference_schema::Cardinality::Many;
        let first = if same_holder {
            vec!["x", "x"]
        } else {
            vec!["x", "y"]
        };
        source.nodes.get_mut(&1).unwrap().fields.insert(
            "variable".into(),
            ReferenceValue::List(
                first
                    .into_iter()
                    .map(|s| ReferenceValue::Str(s.into()))
                    .collect(),
            ),
        );
        if !same_holder {
            source.nodes.get_mut(&2).unwrap().fields.insert(
                "variable".into(),
                ReferenceValue::List(vec![ReferenceValue::Str("y".into())]),
            );
        }
        let (expected_view, expected) = reference_get(&schema, &source, false).unwrap();
        assert_eq!(
            expected.resolved.len(),
            1,
            "scalar names bind at their word positions"
        );
        let (carrier, occurrence) = expected.resolved.iter().next().unwrap();
        assert_eq!(
            (
                occurrence.holder,
                occurrence.index,
                expected_view.edges[carrier].tgt
            ),
            (if same_holder { 1 } else { 2 }, u32::from(same_holder), 1)
        );
        let (resolution, _) = variable_resolution_with_cardinality(
            VariableBinding::FirstDefines,
            Vec::new(),
            Cardinality::Many,
        );
        let (view, actual) = resolution
            .get(
                &AnyInstance::Tree(plain_reader_tree(&source)),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        assert_eq!(actual.occurrences.len(), 1);
        let AnyInstance::Graph(view) = view else {
            panic!("graph expected")
        };
        let (carrier, records) = actual.occurrences.iter().next().unwrap();
        let ViewRef::Edge(id) = carrier else {
            panic!("edge expected")
        };
        let [record] = records.as_slice() else {
            panic!("single occurrence expected")
        };
        assert_eq!(
            record.holder,
            ViewRef::Node(if same_holder { 1 } else { 2 })
        );
        assert_eq!(record.index, u32::from(same_holder));
        assert_eq!(view.edge(*id).unwrap().tgt, 1);
    }
}

#[test]
fn many_variable_retarget_uses_filtered_occurrence_positions() {
    let (mut schema, mut source, _) = reference_variable_tree(
        &["x", "y", "x"],
        reference_schema::VariableBinding::FirstDefines,
        Vec::new(),
    );
    let declaration = schema.references.get_mut("variable").unwrap();
    declaration.cardinality = reference_schema::Cardinality::Many;
    declaration.mode = reference_schema::ResolveMode::Redirect;
    source.nodes.get_mut(&3).unwrap().fields.insert(
        "variable".into(),
        ReferenceValue::List(vec![ReferenceValue::Null, ReferenceValue::Str("x".into())]),
    );
    let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(complement.resolved.len(), 1);
    let (&id, occurrence) = complement.resolved.iter().next().unwrap();
    assert_eq!((occurrence.holder, occurrence.index), (3, 0));
    view.edges.get_mut(&id).unwrap().tgt = 2;
    let mut wanted = source.clone();
    wanted.nodes.get_mut(&3).unwrap().fields.insert(
        "variable".into(),
        ReferenceValue::List(vec![ReferenceValue::Str("y".into())]),
    );
    assert_eq!(
        reference_put(&schema, &view, &complement),
        Ok(wanted.clone())
    );

    let (base, _) = variable_resolution_with_cardinality(
        VariableBinding::FirstDefines,
        Vec::new(),
        Cardinality::Many,
    );
    let mut actual_schema = base.syntactic_schema().clone();
    actual_schema
        .declarations
        .references
        .get_mut("variable")
        .unwrap()
        .mode = ResolveMode::Redirect;
    let resolution = resolution_for_word_schema(&actual_schema, "reference-variable-binding");
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(plain_reader_tree(&source)),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(mut view) = view else {
        panic!("graph expected")
    };
    assert_eq!(complement.occurrences.len(), 1);
    let id = complement
        .occurrences
        .keys()
        .find_map(|carrier| match carrier {
            ViewRef::Edge(id) => Some(*id),
            _ => None,
        })
        .unwrap();
    let parent = view.edge(id).unwrap().src;
    view.set_endpoints(id, parent, 2).unwrap();
    let restored = resolution
        .put(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .unwrap();
    assert!(same_instance(
        &restored,
        &AnyInstance::Tree(plain_reader_tree(&wanted))
    ));
}

#[test]
fn non_self_binding_variable_retains_ordinary_lookup_ambiguity() {
    let (mut schema, mut source, _) = reference_variable_tree(
        &["x", "x"],
        reference_schema::VariableBinding::FirstDefines,
        Vec::new(),
    );
    schema.keys.get_mut("concept").unwrap()[0].parts =
        vec![reference_schema::KeyPart::Path(Vec::new())];
    schema.references.get_mut("variable").unwrap().nullable = true;
    source.nodes.get_mut(&1).unwrap().fields.remove("variable");
    for id in [1, 2] {
        source.nodes.get_mut(&id).unwrap().value = Some(
            panproto_reference::value::Presence::Present(ReferenceValue::Str("x".into())),
        );
    }
    assert_eq!(
        reference_get(&schema, &source, false),
        Err(
            panproto_reference::resolution::ResolveError::AmbiguousReference {
                reference: "variable".into(),
                at: panproto_reference::lens::ViewRef::Node(2),
                key: vec![ReferenceValue::Str("x".into())],
                candidates: vec![
                    panproto_reference::lens::ViewRef::Node(1),
                    panproto_reference::lens::ViewRef::Node(2)
                ],
            }
        )
    );

    let (base, _) = variable_resolution_with_cardinality(
        VariableBinding::FirstDefines,
        Vec::new(),
        Cardinality::One,
    );
    let mut actual_schema = base.syntactic_schema().clone();
    actual_schema.declarations.keys.get_mut("concept").unwrap()[0].parts =
        vec![KeyPart::Path(Vec::new())];
    actual_schema
        .declarations
        .references
        .get_mut("variable")
        .unwrap()
        .nullable = true;
    let resolution = resolution_for_word_schema(&actual_schema, "reference-variable-binding");
    let error = resolution
        .get(
            &AnyInstance::Tree(plain_reader_tree(&source)),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap_err();
    let ResolveError::AmbiguousReference {
        reference,
        at,
        key,
        candidates,
    } = error
    else {
        panic!("ordinary non-self-binding lookup must be ambiguous: {error:?}")
    };
    assert_eq!(reference.as_ref(), "variable");
    assert_eq!(*at, ViewRef::Node(2));
    assert_eq!(key, vec![text("x")]);
    assert_eq!(candidates, vec![ViewRef::Node(1), ViewRef::Node(2)]);
}

#[test]
fn deleting_a_definition_only_holder_removes_its_residue() {
    let (schema, source, _) = reference_variable_tree(
        &["x", "y"],
        reference_schema::VariableBinding::FirstDefines,
        Vec::new(),
    );
    let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
    assert_eq!(complement.definition_occurrences.len(), 2);
    view.edges.retain(|_, edge| edge.src != 2 && edge.tgt != 2);
    view.nodes.remove(&2);
    let mut expected = source.clone();
    expected.nodes.remove(&2);
    expected.arcs.retain(|(src, tgt, _)| *src != 2 && *tgt != 2);
    assert_eq!(
        reference_put(&schema, &view, &complement),
        Ok(expected.clone())
    );
    let (resolution, _) = variable_resolution_with_cardinality(
        VariableBinding::FirstDefines,
        Vec::new(),
        Cardinality::One,
    );
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(plain_reader_tree(&source)),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    assert_eq!(complement.definition_occurrences.len(), 2);
    let AnyInstance::Graph(mut view) = view else {
        panic!("graph expected")
    };
    let incident: Vec<_> = view
        .edges_sorted()
        .filter(|(_, edge)| edge.src == 2 || edge.tgt == 2)
        .map(|(id, _)| id)
        .collect();
    for id in incident {
        view.remove_edge(id).unwrap();
    }
    view.remove_node(2).unwrap();
    let restored = resolution
        .put(
            &AnyInstance::Graph(view),
            &complement,
            &ResolveContext::empty(),
        )
        .unwrap();
    assert!(same_instance(
        &restored,
        &AnyInstance::Tree(plain_reader_tree(&expected))
    ));
}

#[test]
fn many_variable_binding_policies_distinguish_the_nearest_holder() {
    for (reference_binding, binding, target) in [
        (
            reference_schema::VariableBinding::FirstDefines,
            VariableBinding::FirstDefines,
            1,
        ),
        (
            reference_schema::VariableBinding::NearestPreceding,
            VariableBinding::NearestPreceding,
            2,
        ),
    ] {
        let (mut schema, mut source, _) =
            reference_variable_tree(&["x", "x", "x"], reference_binding, Vec::new());
        schema.references.get_mut("variable").unwrap().cardinality =
            reference_schema::Cardinality::Many;
        for node in source.nodes.values_mut().filter(|node| node.id != 0) {
            node.fields.insert(
                "variable".into(),
                ReferenceValue::List(vec![ReferenceValue::Str("x".into())]),
            );
        }
        let (view, complement) = reference_get(&schema, &source, false).unwrap();
        let (id, _) = complement
            .resolved
            .iter()
            .find(|(_, occurrence)| occurrence.holder == 3)
            .unwrap();
        assert_eq!(view.edges[id].tgt, target);
        assert_eq!(
            reference_put(&schema, &view, &complement),
            Ok(source.clone())
        );
        let (resolution, _) =
            variable_resolution_with_cardinality(binding, Vec::new(), Cardinality::Many);
        let source = AnyInstance::Tree(plain_reader_tree(&source));
        let (view, complement) = resolution
            .get(
                &source,
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        let AnyInstance::Graph(graph) = &view else {
            panic!("graph expected")
        };
        let (ViewRef::Edge(id), _) = complement
            .occurrences
            .iter()
            .find(|(_, records)| {
                records
                    .iter()
                    .any(|record| record.holder == ViewRef::Node(3))
            })
            .unwrap()
        else {
            panic!("edge expected")
        };
        assert_eq!(graph.edge(*id).unwrap().tgt, target);
        assert!(same_instance(
            &resolution
                .put(&view, &complement, &ResolveContext::empty())
                .unwrap(),
            &source
        ));
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

    case_insensitive_binder_binds_a_later_spelling();
    named_collation_binder_is_refused();
}

/// Under the ASCII case-insensitive binder `X` is a later binding of `x`.
fn case_insensitive_binder_binds_a_later_spelling() {
    use panproto_schema::{SqlCollation, SqlPad};

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
}

/// Text under a named collation has no executable equality: refused.
fn named_collation_binder_is_refused() {
    use panproto_io::resolution::KeyEqualityReason;
    use panproto_schema::{SqlCollation, SqlPad};

    let (collated, _) = variable_resolution_with(
        VariableBinding::FirstDefines,
        vec![KeyEquality::Sql {
            collation: SqlCollation::Named("database".into()),
            pad: SqlPad::NoPad,
        }],
    );
    match collated.get(
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
                    let picks = base_three_digits(code, length);
                    let variables: Vec<Value> =
                        picks.iter().map(|pick| values[*pick].0.clone()).collect();
                    let source = holder_sequence_tree(&variables);
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

/// The `length` base-three digits of `code`, least significant first.
fn base_three_digits(code: usize, length: u32) -> Vec<usize> {
    (0..length)
        .scan(code, |rest, _| {
            let pick = *rest % 3;
            *rest /= 3;
            Some(pick)
        })
        .collect()
}

/// A document whose children are concepts holding `variables` in order.
fn holder_sequence_tree(variables: &[Value]) -> WInstance {
    let child = prop("doc", "concept", "child");
    let mut nodes = HashMap::from([(0, Node::new(0, "doc"))]);
    for (position, variable) in variables.iter().enumerate() {
        let id = u32::try_from(position + 1).unwrap();
        let mut node = Node::new(id, "concept");
        node.extra_fields
            .insert("variable".to_owned(), variable.clone());
        nodes.insert(id, node);
    }
    let length = u32::try_from(variables.len()).unwrap();
    WInstance::new(
        nodes,
        (1..=length)
            .map(|target| (0, target, child.clone()))
            .collect(),
        Vec::new(),
        0,
        "doc".into(),
    )
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
    assert_eq!(
        preserving_registry(&[vec![1u8], Vec::new()]),
        Vec::<Vec<u8>>::new()
    );
}

/// A target whose key value has no canonical bytes (a temporal value with a
/// fixed offset of a whole day, outside the admitted carrier) is refused by
/// resolution on both sides, never indexed by a total encoding.
#[test]
fn a_key_value_without_canonical_bytes_is_refused_by_resolution() {
    use panproto_inst::value::{ExtendedTemporal, Offset, Temporal};
    use panproto_io::resolution::KeyEqualityReason;
    use panproto_reference::temporal::{
        Offset as ReferenceOffset, OffsetForm, Temporal as ReferenceTemporal,
    };

    let source: IndexedSource = std::iter::once("a".to_owned()).collect();
    let (schema, mut tree, _) = reference_pick_tree(&source);
    tree.nodes.get_mut(&2).expect("first target").fields.insert(
        "cid".to_owned(),
        ReferenceValue::Temporal(ReferenceTemporal {
            date: None,
            time: None,
            separator: None,
            offset: ReferenceOffset::Fixed(1440, OffsetForm::Extended),
            infinity: None,
        }),
    );
    assert_eq!(
        reference_get(&schema, &tree, true).err(),
        Some(
            panproto_reference::resolution::ResolveError::UnsupportedSource {
                reference: "pick".to_owned(),
            }
        )
    );

    let mut production = production_pick_tree(&source);
    production
        .nodes
        .get_mut(&2)
        .expect("first target")
        .extra_fields
        .insert(
            "cid".to_owned(),
            Value::Temporal(Temporal::Extended(ExtendedTemporal {
                date: None,
                time: None,
                separator: None,
                offset: Offset::Fixed(1440),
                infinity: None,
            })),
        );
    let (resolution, _) = tree_picks_with_equality(Vec::new());
    match resolution.get(
        &AnyInstance::Tree(production),
        &ResolveOptions { lenient: true },
        &ResolveContext::empty(),
    ) {
        Err(ResolveError::KeyEquality {
            key,
            reason: KeyEqualityReason::NoCanonicalBytes,
        }) => assert_eq!(key.as_ref(), "cid"),
        Err(other) => panic!("expected a canonical-bytes refusal, found {other:?}"),
        Ok(_) => panic!("expected a canonical-bytes refusal, found a resolution"),
    }
}

fn assert_appended_key_word(
    before: &[&str],
    removed: Option<u32>,
    additions: &[u32],
    after: &[&str],
) {
    let seed = before.iter().map(|value| (*value).to_owned()).collect();
    let (schema, mut source, picked) = reference_pick_tree(&seed);
    let (mut view, complement) = reference_get(&schema, &source, true).unwrap();
    let literal = ReferenceValue::List(
        after
            .iter()
            .map(|value| ReferenceValue::Str((*value).into()))
            .collect(),
    );
    view.nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("customer".into(), literal.clone());
    let next = view.edges.keys().next_back().copied().unwrap() + 1;
    if let Some(index) = removed {
        edit_reference_word_view(&mut view, &complement, &[(index, None)]);
    }
    for (next, target) in (next..).zip(additions) {
        view.edges.insert(
            next,
            reference_model::GEdge {
                src: 1,
                tgt: *target,
                label: picked.clone(),
                position: None,
            },
        );
    }
    source
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("customer".into(), literal.clone());
    let expected_unresolved = assert_reference_word_result(&schema, &view, &complement, &source);
    let (resolution, picked) = tree_picks_with_equality(Vec::new());
    let mut actual_source = production_pick_tree(&seed);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(actual_source.clone()),
            &ResolveOptions { lenient: true },
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(mut view) = view else {
        panic!("graph view")
    };
    view.node_mut(1)
        .unwrap()
        .extra_fields
        .insert("customer".into(), syntax_literal(&literal));
    let next = view.edges_sorted().map(|(id, _)| id.0).max().unwrap() + 1;
    if let Some(index) = removed {
        edit_production_word_view(&mut view, &complement, &[(index, None)]);
    }
    for (next, target) in (next..).zip(additions) {
        view.insert_edge(
            panproto_inst::EdgeId(next),
            panproto_inst::GEdge {
                src: 1,
                tgt: *target,
                edge: picked.clone(),
                position: None,
            },
        )
        .unwrap();
    }
    actual_source
        .nodes
        .get_mut(&1)
        .unwrap()
        .extra_fields
        .insert("customer".into(), syntax_literal(&literal));
    let actual_unresolved =
        assert_production_word_result(&resolution, &view, &complement, &actual_source);
    assert_eq!(actual_unresolved, expected_unresolved);
}

#[test]
fn new_key_edges_append_after_unresolved_and_earlier_additions() {
    assert_appended_key_word(&["a", "missing"], None, &[3], &["a", "missing", "b"]);
    assert_appended_key_word(&["a"], None, &[3, 4], &["a", "b", "c"]);
    assert_appended_key_word(&["a", "b"], Some(1), &[4], &["a", "c"]);
}

fn partitioned_key_case(
    sites: &[Option<ReferenceValue>],
) -> (reference_schema::Schema, reference_model::Tree, Resolution) {
    let seed = IndexedSource::from(["a".to_owned()]);
    let (mut schema, mut source, _) = reference_pick_tree(&seed);
    schema.references.get_mut("pick").unwrap().mode = reference_schema::ResolveMode::Redirect;
    source.nodes.get_mut(&1).unwrap().fields.remove("customer");
    for (index, value) in sites.iter().enumerate() {
        let id = 5 + u32::try_from(index).unwrap();
        let mut node = reference_model::Node::new(id, "label");
        node.value = value
            .clone()
            .map(panproto_reference::value::Presence::Present);
        source.nodes.insert(id, node);
        source.arcs.push((
            1,
            id,
            reference_model::SEdge::new("pick", "label", "prop", Some("customer")),
        ));
    }
    let (resolution, _) =
        try_tree_picks_with_equality_and_mode(Vec::new(), ResolveMode::Redirect).unwrap();
    (schema, source, resolution)
}

fn assert_partitioned_key_change(
    sites: &[Option<ReferenceValue>],
    edits: &[(u32, Option<u32>)],
    after: &[Option<ReferenceValue>],
) {
    assert_eq!(sites.len(), after.len());
    let (schema, mut source, resolution) = partitioned_key_case(sites);
    let (mut view, complement) = reference_get(&schema, &source, true).unwrap();
    let actual_source = plain_reader_tree(&source);
    let (actual_view, actual_complement) = resolution
        .get(
            &AnyInstance::Tree(actual_source),
            &ResolveOptions { lenient: true },
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(mut actual_view) = actual_view else {
        panic!("graph view")
    };
    edit_reference_word_view(&mut view, &complement, edits);
    edit_production_word_view(&mut actual_view, &actual_complement, edits);
    for (index, value) in after.iter().enumerate() {
        source
            .nodes
            .get_mut(&(5 + u32::try_from(index).unwrap()))
            .unwrap()
            .value = value
            .clone()
            .map(panproto_reference::value::Presence::Present);
    }
    let expected_unresolved = assert_reference_word_result(&schema, &view, &complement, &source);
    let actual_unresolved = assert_production_word_result(
        &resolution,
        &actual_view,
        &actual_complement,
        &plain_reader_tree(&source),
    );
    assert_eq!(actual_unresolved, expected_unresolved);
}

#[test]
fn changed_many_key_words_preserve_original_site_partitions() {
    use ReferenceValue::{List, Null, Str};
    let a = || Str("a".into());
    let b = || Str("b".into());
    let c = || Str("c".into());
    assert_partitioned_key_change(
        &[Some(List(vec![Null, a()])), None],
        &[(0, Some(3))],
        &[Some(List(vec![Null, b()])), None],
    );
    assert_partitioned_key_change(
        &[Some(List(vec![a()])), Some(List(vec![b(), c()]))],
        &[(0, None), (1, Some(4))],
        &[Some(List(vec![])), Some(List(vec![c(), c()]))],
    );
    assert_partitioned_key_change(
        &[Some(a()), Some(b())],
        &[(0, Some(4))],
        &[Some(c()), Some(b())],
    );
    assert_partitioned_key_change(
        &[Some(a()), Some(List(vec![Null, b()]))],
        &[],
        &[Some(a()), Some(List(vec![Null, b()]))],
    );
    assert_partitioned_key_change(
        &[
            Some(List(vec![a(), Str("missing".into())])),
            Some(List(vec![b()])),
        ],
        &[(0, Some(4))],
        &[
            Some(List(vec![c(), Str("missing".into())])),
            Some(List(vec![b()])),
        ],
    );
}

#[test]
fn multi_site_scalar_removal_has_no_list_slot() {
    let sites = [
        Some(ReferenceValue::Str("a".into())),
        Some(ReferenceValue::Str("b".into())),
    ];
    let (schema, source, resolution) = partitioned_key_case(&sites);
    let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
    edit_reference_word_view(&mut view, &complement, &[(0, None)]);
    assert!(matches!(reference_put(&schema, &view, &complement),
        Err(panproto_reference::resolution::ResolveError::UnrepresentableReference { reference: Some(reference), .. }) if reference == "pick"));
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(plain_reader_tree(&source)),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(mut view) = view else {
        panic!("graph view")
    };
    edit_production_word_view(&mut view, &complement, &[(0, None)]);
    assert!(
        matches!(resolution.put(&AnyInstance::Graph(view), &complement, &ResolveContext::empty()),
        Err(ResolveError::PutDomain(ResolveDomainError::UnrepresentableReference { reference, .. })) if reference == "pick")
    );
}

#[test]
fn multi_site_composite_writes_keep_each_parts_distinct_boundaries() {
    use ReferenceValue::{Int, List, Null};
    for full in [false, true] {
        let (schema, mut source) = composite_reference_word_case(full);
        source.nodes.get_mut(&1).unwrap().fields.clear();
        let sites = [
            (5, "left", List(vec![Null, Int(1)])),
            (6, "left", List(vec![Null])),
            (7, "right", List(vec![Int(2)])),
            (8, "right", List(vec![Int(2), Null])),
        ];
        for (id, field, value) in sites {
            source.nodes.insert(
                id,
                reference_model::Node::new(id, "label").with_value(value),
            );
            source.arcs.push((
                1,
                id,
                reference_model::SEdge::new("pick", "label", "prop", Some(field)),
            ));
        }
        let (mut view, complement) = reference_get(&schema, &source, true).unwrap();
        let (resolution, _) = composite_production_word_case(full);
        let (actual_view, actual_complement) = resolution
            .get(
                &AnyInstance::Tree(plain_reader_tree(&source)),
                &ResolveOptions { lenient: true },
                &ResolveContext::empty(),
            )
            .unwrap();
        let AnyInstance::Graph(mut actual_view) = actual_view else {
            panic!("graph view")
        };
        let edits = [(u32::from(full), Some(3))];
        edit_reference_word_view(&mut view, &complement, &edits);
        edit_production_word_view(&mut actual_view, &actual_complement, &edits);
        source.nodes.get_mut(&5).unwrap().value =
            Some(panproto_reference::value::Presence::Present(List(vec![
                Null,
                Int(3),
            ])));
        source.nodes.get_mut(&8).unwrap().value =
            Some(panproto_reference::value::Presence::Present(List(vec![
                Int(4),
                Null,
            ])));
        let expected = assert_reference_word_result(&schema, &view, &complement, &source);
        assert_eq!(expected.len(), usize::from(full));
        assert_eq!(
            assert_production_word_result(
                &resolution,
                &actual_view,
                &actual_complement,
                &plain_reader_tree(&source)
            ),
            expected
        );
    }
}

fn assert_multi_site_addition(reinsert: bool, coherent: bool) {
    use ReferenceValue::{List, Null, Str};
    let sites = [Some(List(vec![Null, Str("a".into())])), None];
    let (mut schema, mut source, base) = partitioned_key_case(&sites);
    schema.references.get_mut("pick").unwrap().mode = reference_schema::ResolveMode::AddEdge;
    let mut actual_schema = base.syntactic_schema().clone();
    actual_schema
        .declarations
        .references
        .get_mut("pick")
        .unwrap()
        .mode = ResolveMode::AddEdge;
    let resolution = resolution_for_word_schema(&actual_schema, "reference-tree-picks");
    let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
    let (actual_view, actual_complement) = resolution
        .get(
            &AnyInstance::Tree(plain_reader_tree(&source)),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(mut actual_view) = actual_view else {
        panic!("graph view")
    };
    let next = view.edges.keys().next_back().copied().unwrap() + 1;
    let actual_next = actual_view
        .edges_sorted()
        .map(|(id, _)| id.0)
        .max()
        .unwrap()
        + 1;
    if reinsert {
        edit_reference_word_view(&mut view, &complement, &[(0, None)]);
        edit_production_word_view(&mut actual_view, &actual_complement, &[(0, None)]);
    }
    let target = if reinsert { 2 } else { 3 };
    view.edges.insert(
        next,
        reference_model::GEdge {
            src: 1,
            tgt: target,
            label: schema.references["pick"].edge.clone().unwrap(),
            position: None,
        },
    );
    actual_view
        .insert_edge(
            panproto_inst::EdgeId(actual_next),
            panproto_inst::GEdge {
                src: 1,
                tgt: target,
                edge: actual_schema.declarations.references["pick"]
                    .edge
                    .clone()
                    .unwrap(),
                position: None,
            },
        )
        .unwrap();
    if coherent && !reinsert {
        let value = List(vec![Null, Str("a".into()), Str("b".into())]);
        source.nodes.get_mut(&5).unwrap().value =
            Some(panproto_reference::value::Presence::Present(value.clone()));
        view.nodes.get_mut(&5).unwrap().value =
            Some(panproto_reference::value::Presence::Present(value.clone()));
        actual_view.node_mut(5).unwrap().value = Some(panproto_inst::FieldPresence::Present(
            syntax_literal(&value),
        ));
    }
    if coherent {
        assert_eq!(
            assert_reference_word_result(&schema, &view, &complement, &source).len(),
            0
        );
        assert_eq!(
            assert_production_word_result(
                &resolution,
                &actual_view,
                &actual_complement,
                &plain_reader_tree(&source)
            )
            .len(),
            0
        );
    } else {
        assert!(matches!(reference_put(&schema, &view, &complement),
            Err(panproto_reference::resolution::ResolveError::UnrepresentableReference { reference: Some(reference), .. }) if reference == "pick"));
        assert!(
            matches!(resolution.put(&AnyInstance::Graph(actual_view), &actual_complement, &ResolveContext::empty()),
            Err(ResolveError::PutDomain(ResolveDomainError::UnrepresentableReference { reference, .. })) if reference == "pick")
        );
    }
}

#[test]
fn multi_site_additions_preserve_coherent_sites_and_refuse_unplaced_words() {
    assert_multi_site_addition(false, false);
    assert_multi_site_addition(false, true);
    assert_multi_site_addition(true, true);
}

#[test]
fn multi_site_shared_part_slots_require_compatible_values() {
    use ReferenceValue::{Int, List};
    for compatible in [false, true] {
        let (mut schema, mut source) = composite_reference_word_case(false);
        let left = reference_model::SEdge::new("pick", "label", "prop", Some("left"));
        schema.references.get_mut("pair").unwrap().source = reference_schema::RefSource::KeyValue {
            parts: vec![vec![left.clone()], vec![left.clone()]],
        };
        source.nodes.get_mut(&1).unwrap().fields.clear();
        source
            .nodes
            .get_mut(&2)
            .unwrap()
            .fields
            .insert("right_id".into(), Int(1));
        source
            .nodes
            .get_mut(&3)
            .unwrap()
            .fields
            .insert("right_id".into(), Int(if compatible { 3 } else { 4 }));
        for id in [5, 6] {
            source.nodes.insert(
                id,
                reference_model::Node::new(id, "label").with_value(List(vec![Int(1)])),
            );
            source.arcs.push((1, id, left.clone()));
        }
        let (base, _) = composite_production_word_case(false);
        let mut actual_schema = base.syntactic_schema().clone();
        let left = prop("pick", "label", "left");
        actual_schema
            .declarations
            .references
            .get_mut("pair")
            .unwrap()
            .source = RefSource::KeyValue {
            parts: vec![vec![left.clone()], vec![left]],
        };
        let resolution = resolution_for_word_schema(&actual_schema, "reference-null-match");
        let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
        let (actual_view, actual_complement) = resolution
            .get(
                &AnyInstance::Tree(plain_reader_tree(&source)),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        let AnyInstance::Graph(mut actual_view) = actual_view else {
            panic!("graph view")
        };
        edit_reference_word_view(&mut view, &complement, &[(0, Some(3))]);
        edit_production_word_view(&mut actual_view, &actual_complement, &[(0, Some(3))]);
        if compatible {
            source.nodes.get_mut(&5).unwrap().value =
                Some(panproto_reference::value::Presence::Present(List(vec![
                    Int(3),
                ])));
            assert_eq!(
                assert_reference_word_result(&schema, &view, &complement, &source).len(),
                0
            );
            assert_eq!(
                assert_production_word_result(
                    &resolution,
                    &actual_view,
                    &actual_complement,
                    &plain_reader_tree(&source)
                )
                .len(),
                0
            );
        } else {
            assert!(matches!(reference_put(&schema, &view, &complement),
                Err(panproto_reference::resolution::ResolveError::UnrepresentableReference { reference: Some(reference), .. }) if reference == "pair"));
            assert!(
                matches!(resolution.put(&AnyInstance::Graph(actual_view), &actual_complement, &ResolveContext::empty()),
                Err(ResolveError::PutDomain(ResolveDomainError::UnrepresentableReference { reference, .. })) if reference == "pair")
            );
        }
    }
}

/// Convert only the plain, fan-free fixture carrier used by these reader tests.
fn plain_reader_tree(source: &reference_model::Tree) -> WInstance {
    assert_eq!(source.fans.len(), 0);
    let nodes = source
        .nodes
        .iter()
        .map(|(id, source)| {
            assert_eq!(source.shape, reference_model::NodeShape::Plain);
            assert_eq!(source.discriminator, None);
            assert_eq!(source.annotations.len(), 0);
            let mut node = Node::new(*id, source.anchor.as_str());
            node.position = source.position;
            node.value = source.value.as_ref().map(|value| match value {
                panproto_reference::value::Presence::Absent => panproto_inst::FieldPresence::Absent,
                panproto_reference::value::Presence::Null => panproto_inst::FieldPresence::Null,
                panproto_reference::value::Presence::Present(value) => {
                    panproto_inst::FieldPresence::Present(syntax_literal(value))
                }
            });
            node.extra_fields = source
                .fields
                .iter()
                .map(|(name, value)| (name.clone(), syntax_literal(value)))
                .collect();
            (*id, node)
        })
        .collect();
    let arcs = source
        .arcs
        .iter()
        .map(|(parent, child, label)| {
            (
                *parent,
                *child,
                edge(&label.src, &label.tgt, &label.kind, label.name.as_deref()),
            )
        })
        .collect();
    WInstance::new(
        nodes,
        arcs,
        Vec::new(),
        source.root,
        source.nodes[&source.root].anchor.as_str().into(),
    )
}

fn assert_reader_word(
    schema: &reference_schema::Schema,
    source: &reference_model::Tree,
    resolution: &Resolution,
    expected: &[(u32, ReferenceValue)],
) {
    let (view, complement) = reference_get(schema, source, false).unwrap();
    let mut recorded: Vec<_> = complement
        .resolved
        .iter()
        .map(|(carrier, record)| {
            assert_eq!((record.holder, record.reference.as_str()), (1, "pick"));
            (
                record.index,
                view.edges[carrier].tgt,
                record.literal.clone(),
            )
        })
        .collect();
    recorded.sort_by_key(|record| record.0);
    let expected: Vec<_> = expected
        .iter()
        .enumerate()
        .map(|(index, (target, value))| (u32::try_from(index).unwrap(), *target, value.clone()))
        .collect();
    assert_eq!(recorded, expected);
    assert_eq!(complement.unresolved.len(), 0);
    assert_eq!(
        reference_put(schema, &view, &complement),
        Ok(source.clone())
    );
    let source = plain_reader_tree(source);
    let (view, complement) = resolution
        .get(
            &AnyInstance::Tree(source.clone()),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap();
    let AnyInstance::Graph(graph) = &view else {
        panic!("graph view")
    };
    let mut recorded: Vec<_> = complement
        .occurrences
        .iter()
        .map(|(carrier, records)| {
            let ViewRef::Edge(carrier) = carrier else {
                panic!("edge carrier")
            };
            let [record] = records.as_slice() else {
                panic!("one occurrence")
            };
            assert_eq!(
                (&record.holder, record.reference.as_ref()),
                (&ViewRef::Node(1), "pick")
            );
            (
                record.index,
                graph.edge(*carrier).unwrap().tgt,
                record.literal.clone(),
            )
        })
        .collect();
    recorded.sort_by_key(|record| record.0);
    let expected: Vec<_> = expected
        .iter()
        .map(|(index, target, value)| (*index, *target, syntax_literal(value)))
        .collect();
    assert_eq!(recorded, expected);
    assert_eq!(complement.unresolved.len(), 0);
    let restored = resolution
        .put(&view, &complement, &ResolveContext::empty())
        .unwrap();
    assert!(same_instance(&restored, &AnyInstance::Tree(source)));
}

#[test]
fn many_key_reader_flattens_all_sites_in_source_order() {
    use ReferenceValue::{List, Null, Str};
    let seed = IndexedSource::from(["a".to_owned()]);
    let cases = [
        (
            vec![Some(Str("a".into())), Some(Str("b".into()))],
            vec![(2, Str("a".into())), (3, Str("b".into()))],
        ),
        (
            vec![
                Some(List(vec![Null, Str("a".into())])),
                Some(List(vec![Str("b".into()), Null])),
            ],
            vec![(2, Str("a".into())), (3, Str("b".into()))],
        ),
        (
            vec![
                None,
                Some(Null),
                Some(List(vec![Null])),
                Some(Str("b".into())),
            ],
            vec![(3, Str("b".into()))],
        ),
        (vec![None, Some(Null), Some(List(vec![Null]))], vec![]),
    ];
    let (resolution, _) = tree_picks_with_equality(Vec::new());
    for (sites, expected) in cases {
        for reverse in [false, true] {
            let (schema, mut source, _) = reference_pick_tree(&seed);
            source.nodes.get_mut(&1).unwrap().fields.remove("customer");
            let mut arcs = Vec::new();
            for (index, value) in sites.iter().enumerate() {
                let id = 5 + u32::try_from(index).unwrap();
                let mut node = reference_model::Node::new(id, "label");
                node.position = Some(100 - id);
                node.value = value
                    .clone()
                    .map(panproto_reference::value::Presence::Present);
                source.nodes.insert(id, node);
                arcs.push((
                    1,
                    id,
                    reference_model::SEdge::new("pick", "label", "prop", Some("customer")),
                ));
            }
            if reverse {
                arcs.reverse();
            }
            source.arcs.extend(arcs);
            let mut expected = expected.clone();
            if reverse {
                expected.reverse();
            }
            assert_reader_word(&schema, &source, &resolution, &expected);
        }
    }
}

#[test]
fn many_key_reader_uses_global_final_hop_fallback() {
    let seed = IndexedSource::from(["a".to_owned()]);
    for reached in [false, true] {
        for absent in [false, true] {
            let (mut schema, mut source, _) = reference_pick_tree(&seed);
            let branch = reference_model::SEdge::new("pick", "label", "prop", Some("branch"));
            let terminal = reference_model::SEdge::new("label", "label", "prop", Some("value"));
            schema.edges.extend([branch.clone(), terminal.clone()]);
            schema.references.get_mut("pick").unwrap().source =
                reference_schema::RefSource::KeyValue {
                    parts: vec![vec![branch.clone(), terminal.clone()]],
                };
            source.nodes.get_mut(&1).unwrap().fields.remove("customer");
            for (id, value) in [(5, "a"), (6, "b")] {
                source.nodes.insert(
                    id,
                    reference_model::Node::new(id, "label")
                        .with_field("value", ReferenceValue::Str(value.into())),
                );
                source.arcs.push((1, id, branch.clone()));
            }
            if reached {
                let mut node = reference_model::Node::new(7, "label");
                if !absent {
                    node.value = Some(panproto_reference::value::Presence::Present(
                        ReferenceValue::Str("c".into()),
                    ));
                }
                source.nodes.insert(7, node);
                source.arcs.push((5, 7, terminal));
            }
            let (base, _) = tree_picks_with_equality(Vec::new());
            let mut production_schema = base.syntactic_schema().clone();
            let branch = prop("pick", "label", "branch");
            let terminal = prop("label", "label", "value");
            let signature = production_schema.edges[&prop("pick", "label", "customer")].clone();
            production_schema
                .edges
                .insert(branch.clone(), signature.clone());
            production_schema.edges.insert(terminal.clone(), signature);
            production_schema
                .declarations
                .references
                .get_mut("pick")
                .unwrap()
                .source = RefSource::KeyValue {
                parts: vec![vec![branch, terminal]],
            };
            let resolution = resolution_for_word_schema(&production_schema, "reference-tree-picks");
            let expected = if reached {
                if absent {
                    vec![]
                } else {
                    vec![(4, ReferenceValue::Str("c".into()))]
                }
            } else {
                vec![
                    (2, ReferenceValue::Str("a".into())),
                    (3, ReferenceValue::Str("b".into())),
                ]
            };
            assert_reader_word(&schema, &source, &resolution, &expected);
        }
    }
}

fn empty_key_reader_case(
    targets: u32,
    many: bool,
    full: bool,
    nullable: bool,
) -> (reference_schema::Schema, reference_model::Tree, Resolution) {
    let seed = IndexedSource::from(["a".to_owned()]);
    let (mut schema, mut source, _) = reference_pick_tree(&seed);
    schema.keys.clear();
    let reference = schema.references.get_mut("pick").unwrap();
    reference.target_key = None;
    reference.source = reference_schema::RefSource::KeyValue { parts: Vec::new() };
    reference.cardinality = if many {
        reference_schema::Cardinality::Many
    } else {
        reference_schema::Cardinality::One
    };
    reference.null_match = if full {
        reference_schema::NullMatch::Full
    } else {
        reference_schema::NullMatch::Simple
    };
    reference.nullable = nullable;
    source.nodes.retain(|id, _| *id < 2 + targets);
    source.arcs.retain(|(_, child, _)| *child < 2 + targets);
    let (base, _) = tree_picks_with_equality(Vec::new());
    let mut actual_schema = base.syntactic_schema().clone();
    actual_schema.declarations.keys.clear();
    let reference = actual_schema
        .declarations
        .references
        .get_mut("pick")
        .unwrap();
    reference.target_key = None;
    reference.source = RefSource::KeyValue { parts: Vec::new() };
    reference.cardinality = if many {
        Cardinality::Many
    } else {
        Cardinality::One
    };
    reference.null_match = if full {
        NullMatch::Full
    } else {
        NullMatch::Simple
    };
    reference.nullable = nullable;
    let resolution = resolution_for_word_schema(&actual_schema, "reference-tree-picks");
    (schema, source, resolution)
}

fn assert_empty_key_refusal(
    expected: panproto_reference::resolution::ResolveError,
    actual: ResolveError,
) {
    use panproto_reference::resolution::ResolveError as R;
    match (expected, actual) {
        (
            R::DanglingReference { reference, at, key },
            ResolveError::DanglingReference {
                reference: ar,
                at: aa,
                key: ak,
            },
        ) => {
            assert_eq!(
                (reference.as_str(), at, key),
                ("pick", panproto_reference::lens::ViewRef::Node(1), vec![])
            );
            assert_eq!((ar.as_ref(), *aa, ak), ("pick", ViewRef::Node(1), vec![]));
        }
        (
            R::AmbiguousReference {
                reference,
                at,
                key,
                candidates,
            },
            ResolveError::AmbiguousReference {
                reference: ar,
                at: aa,
                key: ak,
                candidates: ac,
            },
        ) => {
            assert_eq!(
                (reference.as_str(), at, key),
                ("pick", panproto_reference::lens::ViewRef::Node(1), vec![])
            );
            assert_eq!((ar.as_ref(), *aa, ak), ("pick", ViewRef::Node(1), vec![]));
            assert_eq!(
                candidates,
                vec![
                    panproto_reference::lens::ViewRef::Node(2),
                    panproto_reference::lens::ViewRef::Node(3)
                ]
            );
            assert_eq!(ac, vec![ViewRef::Node(2), ViewRef::Node(3)]);
        }
        (
            R::ReferenceCardinalityMismatch {
                reference,
                at,
                lengths,
            },
            ResolveError::ReferenceCardinalityMismatch {
                reference: ar,
                at: aa,
                lengths: al,
            },
        ) => {
            assert_eq!(
                (reference.as_str(), at, lengths),
                ("pick", panproto_reference::lens::ViewRef::Node(1), vec![])
            );
            assert_eq!((ar.as_ref(), *aa, al), ("pick", ViewRef::Node(1), vec![]));
        }
        other => panic!("empty-key refusal disagreement: {other:?}"),
    }
}

fn assert_empty_key_success(
    expected: &panproto_reference::resolution::ResComplement,
    actual: &ResolutionComplement,
    targets: u32,
    has_occurrence: bool,
) {
    let resolved = usize::from(has_occurrence && targets == 1);
    let unresolved = usize::from(has_occurrence && targets != 1);
    assert_eq!(expected.resolved.len(), resolved);
    assert_eq!(actual.occurrences.len(), resolved);
    assert_eq!(expected.unresolved.len(), unresolved);
    assert_eq!(actual.unresolved.len(), unresolved);
    for record in expected.resolved.values() {
        assert_eq!(
            (
                record.holder,
                record.index,
                record.reference.as_str(),
                &record.literal
            ),
            (1, 0, "pick", &ReferenceValue::List(vec![]))
        );
    }
    for records in actual.occurrences.values() {
        let [record] = records.as_slice() else {
            panic!("one resolved empty tuple")
        };
        assert_eq!(
            (
                &record.holder,
                record.index,
                record.reference.as_ref(),
                &record.literal
            ),
            (&ViewRef::Node(1), 0, "pick", &Value::List(vec![]))
        );
    }
    if let ([(record, reason)], [actual]) =
        (expected.unresolved.as_slice(), actual.unresolved.as_slice())
    {
        assert_eq!(
            (
                record.holder,
                record.index,
                record.reference.as_str(),
                &record.literal
            ),
            (1, 0, "pick", &ReferenceValue::List(vec![]))
        );
        assert_eq!(
            (
                &actual.holder,
                actual.index,
                actual.reference.as_ref(),
                &actual.literal
            ),
            (&ViewRef::Node(1), 0, "pick", &Value::List(vec![]))
        );
        match (reason, &actual.reason, targets) {
            (
                panproto_reference::resolution::Unresolved::Dangling,
                UnresolvedReason::Dangling,
                0,
            ) => {}
            (
                panproto_reference::resolution::Unresolved::Ambiguous(expected),
                UnresolvedReason::Ambiguous { candidates },
                2,
            ) => {
                assert_eq!(expected, &[2, 3]);
                assert_eq!(candidates, &[ViewRef::Node(2), ViewRef::Node(3)]);
            }
            other => panic!("empty-key unresolved disagreement: {other:?}"),
        }
    }
}

#[test]
fn empty_key_source_obeys_target_count_null_match_and_cardinality() {
    let configurations = [
        (false, false, false),
        (false, true, false),
        (true, false, true),
        (true, false, false),
    ];
    let mut checked = 0;
    for targets in 0..=2 {
        for lenient in [false, true] {
            for (many, full, nullable) in configurations {
                let (schema, source, resolution) =
                    empty_key_reader_case(targets, many, full, nullable);
                let actual_source = plain_reader_tree(&source);
                let expected = reference_get(&schema, &source, lenient);
                let actual = resolution.get(
                    &AnyInstance::Tree(actual_source.clone()),
                    &ResolveOptions { lenient },
                    &ResolveContext::empty(),
                );
                let refused = (many && !nullable) || (!many && !full && targets != 1 && !lenient);
                assert_eq!(expected.is_err(), refused);
                assert_eq!(actual.is_err(), refused);
                match (expected, actual) {
                    (Ok((view, complement)), Ok((actual_view, actual_complement))) => {
                        assert_empty_key_success(
                            &complement,
                            &actual_complement,
                            targets,
                            !many && !full,
                        );
                        assert_eq!(reference_put(&schema, &view, &complement), Ok(source));
                        let restored = resolution
                            .put(&actual_view, &actual_complement, &ResolveContext::empty())
                            .unwrap();
                        assert!(same_instance(&restored, &AnyInstance::Tree(actual_source)));
                    }
                    (Err(expected), Err(actual)) => assert_empty_key_refusal(expected, actual),
                    other => panic!("empty-key get disagreement: {other:?}"),
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 24);
}

fn one_site_reader_case(
    sites: &[Option<ReferenceValue>],
    nullable: bool,
) -> (reference_schema::Schema, reference_model::Tree, Resolution) {
    let seed = IndexedSource::from(["a".to_owned()]);
    let (mut schema, mut source, _) = reference_pick_tree(&seed);
    let reference = schema.references.get_mut("pick").unwrap();
    reference.cardinality = reference_schema::Cardinality::One;
    reference.nullable = nullable;
    source.nodes.get_mut(&1).unwrap().fields.remove("customer");
    for (index, value) in sites.iter().enumerate() {
        let id = 5 + u32::try_from(index).unwrap();
        let mut node = reference_model::Node::new(id, "label");
        node.value = value
            .clone()
            .map(panproto_reference::value::Presence::Present);
        source.nodes.insert(id, node);
        source.arcs.push((
            1,
            id,
            reference_model::SEdge::new("pick", "label", "prop", Some("customer")),
        ));
    }
    let (base, _) = tree_picks_with_equality(Vec::new());
    let mut actual = base.syntactic_schema().clone();
    let reference = actual.declarations.references.get_mut("pick").unwrap();
    reference.cardinality = Cardinality::One;
    reference.nullable = nullable;
    let resolution = resolution_for_word_schema(&actual, "reference-tree-picks");
    (schema, source, resolution)
}

fn assert_reader_cardinality(
    schema: &reference_schema::Schema,
    source: &reference_model::Tree,
    resolution: &Resolution,
    lengths: &[u64],
) {
    assert_eq!(schema.references.len(), 1);
    let name = schema.references.keys().next().unwrap().as_str();
    let expected = reference_get(schema, source, false).unwrap_err();
    let actual = resolution
        .get(
            &AnyInstance::Tree(plain_reader_tree(source)),
            &ResolveOptions::default(),
            &ResolveContext::empty(),
        )
        .unwrap_err();
    let panproto_reference::resolution::ResolveError::ReferenceCardinalityMismatch {
        reference,
        at,
        lengths: found,
    } = expected
    else {
        panic!("reference cardinality refusal: {expected:?}")
    };
    assert_eq!(
        (reference.as_str(), at),
        (name, panproto_reference::lens::ViewRef::Node(1))
    );
    assert_eq!(found, lengths);
    let ResolveError::ReferenceCardinalityMismatch {
        reference,
        at,
        lengths: found,
    } = actual
    else {
        panic!("production cardinality refusal: {actual:?}")
    };
    assert_eq!((reference.as_ref(), *at), (name, ViewRef::Node(1)));
    assert_eq!(found, lengths);
}

#[test]
fn one_key_reader_counts_sites_before_null_filtering() {
    let a = ReferenceValue::Str("a".into());
    let (schema, source, resolution) =
        one_site_reader_case(&[Some(a.clone()), Some(a.clone())], false);
    assert_reader_cardinality(&schema, &source, &resolution, &[2]);
    for nullable in [false, true] {
        let (schema, source, resolution) = one_site_reader_case(&[None, None], nullable);
        if nullable {
            assert_reader_word(&schema, &source, &resolution, &[]);
        } else {
            assert_reader_cardinality(&schema, &source, &resolution, &[0]);
        }
    }
    let (schema, source, resolution) = one_site_reader_case(&[Some(ReferenceValue::Null)], false);
    assert_reader_word(&schema, &source, &resolution, &[]);
    let list = ReferenceValue::List(vec![a]);
    let (schema, mut source, resolution) = one_site_reader_case(&[Some(list.clone())], false);
    source
        .nodes
        .get_mut(&2)
        .unwrap()
        .fields
        .insert("cid".into(), list.clone());
    assert_reader_word(&schema, &source, &resolution, &[(2, list)]);
}

#[test]
fn one_composite_key_reader_reports_each_part_length() {
    for missing in [false, true] {
        let (schema, mut source, _) = reference_null_match(
            reference_schema::NullMatch::Simple,
            ReferenceValue::Int(1),
            ReferenceValue::Int(2),
        );
        source.nodes.get_mut(&1).unwrap().fields.remove("left");
        if missing {
            source.nodes.get_mut(&1).unwrap().fields.remove("right");
        }
        for id in [5, 6] {
            source.nodes.insert(
                id,
                reference_model::Node::new(id, "label").with_value(ReferenceValue::Int(1)),
            );
            source.arcs.push((
                1,
                id,
                reference_model::SEdge::new("pick", "label", "prop", Some("left")),
            ));
        }
        let (resolution, _) = null_match_resolution(NullMatch::Simple);
        assert_reader_cardinality(&schema, &source, &resolution, &[2, u64::from(!missing)]);
    }
}

mod randomized {
    use super::*;
    use panproto_io::resolution::{
        fold_uri_environment, resolution_text_identity, resolve_inherited_uri_base,
        resolve_uri_reference,
    };
    use panproto_reference::resolution::{
        check_put_domain as reference_domain, coherent_rekey, declared_key_equality, equivalent,
        gated_put_twice, get as reference_get, guarded_partition, indexed_get, indexed_put,
        inherited_base, nearest_preceding, put as reference_put, rfc3986, text_identity,
        uri_environment,
    };
    use proptest::prelude::*;

    fn assert_generated_indexed_case(source: &IndexedSource, choice: usize) {
        let (resolution, picked) = picks();
        let (view, index) = indexed_get(source);
        let (actual_view, complement) = production_get(&resolution, source);
        let restored = resolution
            .put(
                &AnyInstance::Graph(actual_view.clone()),
                &complement,
                &ResolveContext::empty(),
            )
            .unwrap();
        assert_eq!(indexed_put(&view, &index), Ok(source.clone()));
        assert!(same_instance(
            &restored,
            &AnyInstance::Table(picks_table(source))
        ));
        let steps = single_step_basis(&view);
        let step = &steps[choice % steps.len()];
        let candidate = apply_single_step(&view, step);
        let actual_candidate = production_edit(&actual_view, &view, &picked, step);
        let expected_refusal = reference_domain(&candidate, &index);
        let actual_refusal = production_violations(&resolution, &actual_candidate, &complement);
        assert_eq!(actual_refusal.is_empty(), expected_refusal.is_empty());
        let expected = indexed_put(&candidate, &index);
        let actual = resolution.put(
            &AnyInstance::Graph(actual_candidate.clone()),
            &complement,
            &ResolveContext::empty(),
        );
        assert_eq!(actual.is_ok(), expected.is_ok());
        if let Ok(expected_source) = expected {
            let actual_source = actual.unwrap();
            assert!(same_instance(
                &actual_source,
                &AnyInstance::Table(picks_table(&expected_source))
            ));
            let (again, next) = indexed_get(&expected_source);
            assert!(equivalent(
                &candidate,
                &coherent_rekey(&next, &candidate),
                &again,
                &next
            ));
            let (actual_again, actual_next) = resolution
                .get(
                    &actual_source,
                    &ResolveOptions::default(),
                    &ResolveContext::empty(),
                )
                .unwrap();
            let AnyInstance::Graph(actual_again) = actual_again else {
                panic!("graph expected")
            };
            assert!(production_put_get_equal(
                &actual_candidate,
                &actual_again,
                &picked
            ));
            assert_eq!(
                candidate == again,
                GInstance::identical(&actual_candidate, &actual_again)
            );
            assert!(gated_put_twice(&candidate, &index));
            let rekeyed = rekeyed_complement(&actual_next, &actual_candidate, &picked);
            let twice = resolution
                .put(
                    &AnyInstance::Graph(actual_candidate),
                    &rekeyed,
                    &ResolveContext::empty(),
                )
                .unwrap();
            assert!(same_instance(&twice, &actual_source));
        }
    }

    fn assert_source_get_put(
        resolution: &Resolution,
        source: &AnyInstance,
    ) -> (AnyInstance, ResolutionComplement) {
        let (view, complement) = resolution
            .get(source, &ResolveOptions::default(), &ResolveContext::empty())
            .unwrap();
        let restored = resolution
            .put(&view, &complement, &ResolveContext::empty())
            .unwrap();
        assert!(same_instance(&restored, source));
        (view, complement)
    }

    fn assert_generated_declared_case(source: &IndexedSource, choice: usize) {
        let (schema, tree, picked) = reference_pick_tree(source);
        let (view, complement) = reference_get(&schema, &tree, false).unwrap();
        assert_eq!(reference_put(&schema, &view, &complement), Ok(tree));
        let (resolution, actual_picked) = tree_picks();
        let actual_tree = production_pick_tree(source);
        let (actual_view, actual_complement) =
            assert_source_get_put(&resolution, &AnyInstance::Tree(actual_tree.clone()));
        let AnyInstance::Graph(actual_view) = actual_view else {
            panic!("graph expected")
        };
        assert_eq!(
            production_tree_targets(&actual_view, &actual_picked),
            reference_tree_targets(&view, &picked)
        );
        let (indexed, _) = indexed_get(source);
        let steps = single_step_basis(&indexed);
        let step = &steps[choice % steps.len()];
        let mut candidate = reference_tree_edit(&view, &picked, step, &indexed);
        let mut actual_candidate =
            production_tree_edit(&actual_view, &actual_picked, step, &indexed);
        assert!(reference_put(&schema, &candidate, &complement).is_err());
        assert!(
            resolution
                .check_put_domain(
                    &AnyInstance::Graph(actual_candidate.clone()),
                    &actual_complement,
                    &ResolveContext::empty()
                )
                .is_err()
        );
        let sequence: Vec<_> = apply_single_step(&indexed, step).into_values().collect();
        candidate.nodes.get_mut(&1).unwrap().fields.insert(
            "customer".to_owned(),
            ReferenceValue::List(sequence.iter().cloned().map(ReferenceValue::Str).collect()),
        );
        actual_candidate.node_mut(1).unwrap().extra_fields.insert(
            "customer".to_owned(),
            Value::List(sequence.iter().map(|target| text(target)).collect()),
        );
        let expected = reference_put(&schema, &candidate, &complement).unwrap();
        assert!(
            resolution
                .check_put_domain(
                    &AnyInstance::Graph(actual_candidate.clone()),
                    &actual_complement,
                    &ResolveContext::empty()
                )
                .is_ok()
        );
        let actual = resolution
            .put(
                &AnyInstance::Graph(actual_candidate),
                &actual_complement,
                &ResolveContext::empty(),
            )
            .unwrap();
        let expected_sequence = reference_tree_sequence(&expected);
        let mut transported = actual_tree;
        transported.nodes.get_mut(&1).unwrap().extra_fields.insert(
            "customer".to_owned(),
            Value::List(
                expected_sequence
                    .iter()
                    .map(|target| text(target))
                    .collect(),
            ),
        );
        assert!(same_instance(&actual, &AnyInstance::Tree(transported)));
        let (again, next) = reference_get(&schema, &expected, false).unwrap();
        let (actual_again, actual_next) = resolution
            .get(
                &actual,
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        let AnyInstance::Graph(actual_again) = actual_again else {
            panic!("graph expected")
        };
        assert_eq!(
            production_tree_targets(&actual_again, &actual_picked),
            reference_tree_targets(&again, &picked)
        );
        assert_eq!(reference_put(&schema, &again, &next), Ok(expected));
        let twice = resolution
            .put(
                &AnyInstance::Graph(actual_again),
                &actual_next,
                &ResolveContext::empty(),
            )
            .unwrap();
        assert!(same_instance(&twice, &actual));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn indexed_get_put_domain_and_gated_put_match_on_generated_candidates(
            source in 0_usize..8,
            choice in 0_usize..12,
        ) {
            assert_generated_indexed_case(&indexed_sources().remove(source), choice);
            assert_generated_declared_case(&indexed_sources().remove(source), choice);
        }

        #[test]
        fn uri_environment_matches_on_generated_directive_prefixes(
            choice in 0_usize..156,
            form_choice in 0_usize..4,
        ) {
            let directives = uri_directive_sequences().remove(choice);
            let forms = uri_written_forms();
            let expected = uri_environment(&directives);
            let actual = fold_uri_environment(&directives.iter().map(production_directive).collect::<Vec<_>>());
            prop_assert_eq!(&actual.base, &expected.base);
            prop_assert_eq!(&actual.prefixes, &expected.prefixes);
            prop_assert_eq!(actual.resolve(forms[form_choice]), expected.resolve(forms[form_choice]));
        }

        #[test]
        fn inherited_base_matches_on_generated_ancestor_chains(
            choice in 0_usize..84,
            form_choice in 0_usize..3,
        ) {
            let chain = inherited_base_chains().remove(choice);
            let form = ["#w1", "c", "http://e/f"][form_choice];
            prop_assert_eq!(resolve_inherited_uri_base(&chain, form), inherited_base(&chain, form));
        }

        #[test]
        fn text_identity_matches_on_generated_field_and_locality_pairs(
            left_field in proptest::bool::ANY,
            right_field in proptest::bool::ANY,
            left_external in proptest::bool::ANY,
            right_external in proptest::bool::ANY,
        ) {
            let field = |right| if right { "right" } else { "left" };
            let site = |right| panproto_inst::ResolutionValueSite::Field {
                holder: ViewRef::Node(7), field: field(right).to_owned(),
            };
            let left = text_identity(7, field(left_field),
                left_external.then_some(("route-a", b"key-a".as_slice())));
            let right = text_identity(7, field(right_field),
                right_external.then_some(("route-b", b"key-b".as_slice())));
            let production_left = resolution_text_identity(site(left_field), left_external);
            let production_right = resolution_text_identity(site(right_field), right_external);
            prop_assert_eq!(production_left.is_local(), !left_external);
            prop_assert_eq!(production_right.is_local(), !right_external);
            prop_assert_eq!(production_left.site(), &site(left_field));
            prop_assert_eq!(production_right.site(), &site(right_field));
            prop_assert_eq!(production_left == production_right, left == right);
        }

        #[test]
        fn rfc3986_matches_on_generated_published_vectors(choice in 0_usize..42) {
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
            let (form, expected) = vectors[choice];
            let actual = resolve_uri_reference("http://a/b/c/d;p?q", form);
            prop_assert_eq!(actual.as_deref(), Some(expected));
            prop_assert_eq!(actual, rfc3986("http://a/b/c/d;p?q", form));
        }

        #[test]
        fn guarded_selection_matches_on_generated_existing_literals(choice in 0_usize..3) {
            let kind = ["left", "right", "other"][choice];
            let guards = vec![("left".to_owned(), ReferenceValue::Str("left".to_owned())),
                ("right".to_owned(), ReferenceValue::Str("right".to_owned()))];
            let selected = guarded_partition(&guards, &ReferenceValue::Str(kind.to_owned()));
            let (resolution, left, right) = guarded_picks();
            let source = guarded_pick_tree(kind);
            let (view, complement) = resolution.get(&AnyInstance::Tree(source.clone()),
                &ResolveOptions::default(), &ResolveContext::empty()).unwrap();
            let AnyInstance::Graph(view) = view else { panic!("graph expected") };
            prop_assert_eq!(view.edges_sorted().any(|(_, edge)| edge.edge == left), selected == Some("left"));
            prop_assert_eq!(view.edges_sorted().any(|(_, edge)| edge.edge == right), selected == Some("right"));
            match selected {
                Some(_) => prop_assert!(complement.unresolved.is_empty()),
                None => prop_assert!(matches!(complement.unresolved.as_slice(), [record]
                    if record.reference == "left" && record.literal.canonical_cmp(&text(kind)).is_eq()
                    && matches!(&record.reason, UnresolvedReason::Unselected { value }
                        if value.canonical_cmp(&text(kind)).is_eq())), "unselected literal is preserved"),
            }
            let restored = resolution.put(&AnyInstance::Graph(view), &complement,
                &ResolveContext::empty()).unwrap();
            prop_assert!(same_instance(&restored, &AnyInstance::Tree(source)));
        }

        #[test]
        fn index_refusal_matches_under_generated_unresolved_policy(complement in proptest::bool::ANY) {
            let (actual, expected) = if complement {
                (OnUnresolved::Complement, reference_schema::OnUnresolved::Complement)
            } else { (OnUnresolved::Error, reference_schema::OnUnresolved::Error) };
            prop_assert_eq!(reference_index_get(expected), Err(
                panproto_reference::resolution::ResolveError::KeyNotResolvable {
                    reference: "index-reference".to_owned(), key: "index".to_owned(),
                }));
            assert_production_index_refusal(actual);
        }

        #[test]
        fn null_match_outcomes_match_on_generated_existing_occurrences(
            choice in 0_usize..5,
            full in proptest::bool::ANY,
        ) {
            let cases = [(Value::Int(1), Value::Int(2), ReferenceValue::Int(1), ReferenceValue::Int(2)),
                (Value::Int(1), Value::Int(3), ReferenceValue::Int(1), ReferenceValue::Int(3)),
                (Value::Null, Value::Null, ReferenceValue::Null, ReferenceValue::Null),
                (Value::Int(1), Value::Null, ReferenceValue::Int(1), ReferenceValue::Null),
                (Value::Null, Value::Int(2), ReferenceValue::Null, ReferenceValue::Int(2))];
            let (left, right, rleft, rright) = cases[choice].clone();
            let (actual_mode, expected_mode) = if full { (NullMatch::Full, reference_schema::NullMatch::Full) }
                else { (NullMatch::Simple, reference_schema::NullMatch::Simple) };
            let actual = production_null_outcome(actual_mode, left.clone(), right.clone());
            let expected = reference_null_outcome(expected_mode, rleft.clone(), rright.clone());
            prop_assert_eq!(&actual.0, &expected.0);
            if actual.0 == NullOutcome::Dangling {
                prop_assert_eq!(actual.1, Some(Value::List(vec![left, right])));
                prop_assert_eq!(expected.1, Some(ReferenceValue::List(vec![rleft, rright])));
            } else { prop_assert_eq!(actual.1, None); prop_assert_eq!(expected.1, None); }
        }

        #[test]
        fn variable_bindings_match_on_generated_bounded_holder_sequences(
            picks in proptest::collection::vec(0_usize..3, 1..=5),
            numeric in proptest::bool::ANY,
            nearest in proptest::bool::ANY,
        ) {
            use panproto_reference::keys::KeyEquality as ReferenceEquality;
            let (equality, reference_equality, values) = if numeric {
                (vec![KeyEquality::NumericValue], ReferenceEquality::NumericValue,
                    [(Value::Int(1), ReferenceValue::Int(1)),
                     (Value::Float(1.0), ReferenceValue::Float(1.0_f64.to_bits())),
                     (Value::Int(2), ReferenceValue::Int(2))])
            } else {
                (Vec::new(), ReferenceEquality::Representation,
                    [(text("a"), ReferenceValue::Str("a".to_owned())),
                     (text("b"), ReferenceValue::Str("b".to_owned())),
                     (Value::Int(1), ReferenceValue::Int(1))])
            };
            let class = |index: usize| (0..values.len()).find(|&first|
                declared_key_equality(&reference_equality, &values[first].1, &values[index].1) == Some(true))
                .map_or_else(|| index.to_string(), |first| first.to_string());
            let definitions: Vec<_> = picks.iter().enumerate().map(|(position, pick)|
                (position, class(*pick), u32::try_from(position + 1).unwrap())).collect();
            let mut expected: Vec<_> = definitions.iter().filter_map(|(position, name, id)|
                nearest_preceding(&definitions, *position, name, nearest).map(|target| (*id, target))).collect();
            expected.sort_unstable();
            let variables: Vec<_> = picks.iter().map(|pick| values[*pick].0.clone()).collect();
            let source = holder_sequence_tree(&variables);
            let policy = if nearest { VariableBinding::NearestPreceding } else { VariableBinding::FirstDefines };
            let (resolution, binding) = variable_resolution_with(policy, equality);
            let (view, complement) = resolution.get(&AnyInstance::Tree(source.clone()),
                &ResolveOptions::default(), &ResolveContext::empty()).unwrap();
            let AnyInstance::Graph(view) = view else { panic!("graph expected") };
            let mut actual: Vec<_> = view.edges_sorted().filter(|(_, edge)| edge.edge == binding)
                .map(|(_, edge)| (edge.src, edge.tgt)).collect();
            actual.sort_unstable();
            prop_assert_eq!(actual, expected);
            let restored = resolution.put(&AnyInstance::Graph(view), &complement,
                &ResolveContext::empty()).unwrap();
            prop_assert!(same_instance(&restored, &AnyInstance::Tree(source)));
        }
    }
}

fn span_payload_source() -> (reference_model::Tree, WInstance) {
    let note = reference_model::Node::new(1, "note")
        .with_field("text", ReferenceValue::Str("abc".to_owned()))
        .with_field("start", ReferenceValue::Int(0))
        .with_field("end", ReferenceValue::Int(1));
    let mut actual_note = Node::new(1, "note");
    for (name, value) in &note.fields {
        actual_note
            .extra_fields
            .insert(name.clone(), syntax_literal(value));
    }
    (
        reference_model::Tree {
            nodes: BTreeMap::from([(0, reference_model::Node::new(0, "doc")), (1, note)]),
            arcs: vec![(
                0,
                1,
                reference_model::SEdge::new("doc", "note", "prop", Some("note")),
            )],
            fans: Vec::new(),
            root: 0,
        },
        WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (1, actual_note)]),
            vec![(0, 1, prop("doc", "note", "note"))],
            Vec::new(),
            0,
            "doc".into(),
        ),
    )
}

fn edit_span_payload(
    reference: &mut reference_model::Graph,
    production: &mut GInstance,
    reference_span: u32,
    production_span: u32,
    case: u8,
) {
    match case {
        0 => {}
        1 | 4 => {
            reference.edges.retain(|_, edge| edge.tgt != reference_span);
            let edges: Vec<_> = production.in_edges(production_span).to_vec();
            for edge in edges {
                production.remove_edge(edge).unwrap();
            }
            for name in ["start", "end"] {
                reference.nodes.get_mut(&1).unwrap().fields.remove(name);
                production.node_mut(1).unwrap().extra_fields.remove(name);
            }
            if case == 4 {
                reference.nodes.remove(&reference_span);
                production.remove_node(production_span).unwrap();
            }
        }
        2 => {
            reference
                .nodes
                .get_mut(&reference_span)
                .unwrap()
                .fields
                .insert(
                    "audit".to_owned(),
                    ReferenceValue::Str("visible".to_owned()),
                );
            production
                .node_mut(production_span)
                .unwrap()
                .extra_fields
                .insert("audit".to_owned(), text("visible"));
        }
        3 => {
            "note".clone_into(&mut reference.nodes.get_mut(&reference_span).unwrap().anchor);
            production.node_mut(production_span).unwrap().anchor = "note".into();
        }
        _ => unreachable!("five span controls"),
    }
}

#[test]
fn materialized_span_nodes_preserve_visible_payload_and_presence() {
    for case in 0..5 {
        let (resolution, schema) = syntax_standoff_resolution();
        let (mut source, mut actual_source) = span_payload_source();
        let (mut view, complement) = reference_get(&schema, &source, false).unwrap();
        let (actual_view, actual_complement) = resolution
            .get(
                &AnyInstance::Tree(actual_source.clone()),
                &ResolveOptions::default(),
                &ResolveContext::empty(),
            )
            .unwrap();
        let AnyInstance::Graph(mut actual_view) = actual_view else {
            panic!("graph expected")
        };
        let span = *complement.span_nodes.keys().next().unwrap();
        let actual_span = *actual_complement.span_nodes.keys().next().unwrap();
        edit_span_payload(&mut view, &mut actual_view, span, actual_span, case);
        let violations = production_violations(&resolution, &actual_view, &actual_complement);
        let expected = reference_put(&schema, &view, &complement);
        let actual = resolution.put(
            &AnyInstance::Graph(actual_view),
            &actual_complement,
            &ResolveContext::empty(),
        );
        if matches!(case, 0 | 4) {
            if case == 4 {
                for name in ["start", "end"] {
                    source.nodes.get_mut(&1).unwrap().fields.remove(name);
                    actual_source
                        .nodes
                        .get_mut(&1)
                        .unwrap()
                        .extra_fields
                        .remove(name);
                }
            }
            assert_eq!(
                expected,
                Ok(source),
                "supported span case {case} preserves exact source"
            );
            assert!(
                same_instance(&actual.unwrap(), &AnyInstance::Tree(actual_source)),
                "supported span case {case} preserves production source"
            );
        } else {
            assert_eq!(
                expected,
                Err(
                    panproto_reference::resolution::ResolveError::UnrepresentableReference {
                        carrier: panproto_reference::lens::ViewRef::Node(span),
                        reference: None,
                    }
                ),
                "reference refuses span data loss in case {case}"
            );
            let edited = ResolveDomainError::SynthesizedEdited {
                carrier: ViewRef::Node(actual_span),
            };
            assert!(
                violations.contains(&edited),
                "the complete report retains the edited span in case {case}"
            );
            // A payload or anchor mismatch also leaves a re-read span on the
            // right. Its holder's diagnostic sorts before the span's own ID.
            let first = if case == 1 {
                edited
            } else {
                ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Node(1),
                    reference: "mention".into(),
                }
            };
            assert_eq!(violations.first(), Some(&first));
            assert!(
                matches!(&actual, Err(ResolveError::PutDomain(error)) if *error == first),
                "production returns the least span-data-loss violation in case {case}: {actual:?}"
            );
        }
    }
}
