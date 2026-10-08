//! # panproto-lens
//!
//! Bidirectional lenses via protolens (schema-independent lens families).
//!
//! Every schema migration is a lens with a `get` direction (= restrict,
//! projecting data forward) and a `put` direction (= restore from
//! complement, bringing modifications back). The strict law gate checks
//! `GetPut`, `PutGet`, totality, `PutTwice`, and presentation congruence.
//!
//! This crate provides:
//!
//! - **[`Lens`]**: An asymmetric lens backed by a compiled migration.
//! - **[`get`]** / **[`put`]**: Forward and backward lens directions.
//! - **[`Complement`]**: Data discarded by `get`, needed by `put`.
//! - **[`Protolens`]**: A dependent function from schemas to lenses
//!   (`Π(S : Schema | P(S)). Lens(F(S), G(S))`).
//! - **[`ProtolensChain`]**: Composable sequence of protolenses.
//! - **[`auto_generate`]**: Automatic protolens generation from two schemas.
//! - **[`compose()`]**: Compose two lenses sequentially.
//! - **[`check_laws`]**: Run the five strict gates on a test instance.
//!
//! The mathematical foundations are based on asymmetric lenses with
//! complement (Diskin et al., 2011), GAT-based protolenses (natural
//! transformations between theory endofunctors), and Cambria's
//! approach to schema evolution (Ink & Switch, 2020).

// Allow concrete HashMap/HashSet in public API signatures per ENGINEERING.md spec.
#![allow(clippy::implicit_hasher)]

pub mod asymmetric;
pub mod auto_lens;
pub mod candidate;
pub mod coercion_laws;
pub mod complement_type;
pub mod compose;
pub mod cost;
mod default_synthesis;
pub mod derived;
pub mod diff_to_protolens;
pub mod dispatch;
pub mod edit_error;
pub mod edit_laws;
pub mod edit_lens;
pub mod edit_pipeline;
pub mod edit_provenance;
pub mod enrichment_registry;
pub mod error;
pub mod fibration;
pub mod graph_lens;
pub mod graph_protolens;
pub mod graph_symmetric;
pub mod hint;
pub mod identity;
pub mod laws;
pub mod layout_complement;
pub mod lens_graph;
pub mod lens_path;
pub mod lens_shape;
pub mod optic;
pub mod protolens;
pub mod protolens_nat;
pub mod symbolic;
pub mod symmetric;
pub mod table_lens;
pub mod table_protolens;
pub mod table_shape;
pub mod table_symmetric;

// Re-exports for convenience.
pub use asymmetric::{
    Complement, ComplementCompose, get, put, put_checked, put_without_complement,
};
pub use auto_lens::{
    AutoLensConfig, AutoLensResult, Stringency, auto_generate, auto_generate_candidates,
    auto_generate_candidates_with_hints, auto_generate_graph, auto_generate_table,
    auto_generate_with_hints,
};
pub use candidate::{CandidateStep, LensCandidate};
pub use complement_type::{
    CapturedField, ComplementKind, ComplementSpec, DefaultRequirement, GraphComplementSpec,
    GraphResidueKind, TableComplementSpec, TableResidueKind, chain_complement_spec,
    complement_spec_at, graph_chain_complement_spec, graph_complement_spec_at,
    table_chain_complement_spec, table_complement_spec_at,
};
pub use compose::compose;
pub use cost::{chain_cost, complement_cost, verify_identity_cost, verify_subadditivity};
pub use derived::{DerivedFiber, DerivedMap, collect_derived_fields};
pub use diff_to_protolens::{DiffSpec, KindChange, diff_to_lens, diff_to_protolens};
pub use dispatch::{
    graph_complement_to_table, graph_complement_to_tree, graph_get_via_table, graph_get_via_tree,
    graph_put_via_table, graph_put_via_tree, table_complement_to_graph, tree_complement_to_graph,
};
pub use edit_error::EditLensError;
pub use edit_lens::EditLens;
pub use edit_pipeline::EditPipeline;
pub use edit_provenance::EditProvenance;
pub use error::{LawViolation, LensError, NotTableComplementReason, NotTreeComplementReason};
pub use graph_lens::{
    AlignError, Aligner, Alignment, GraphComposite, GraphCompositeComplement, IdAligner,
    SequentialGraphLens,
};
pub use graph_protolens::AppliedGraphProtolens;
pub use identity::ComplementIdentity;
pub use laws::{
    FamilyReport, LawReport, LawShape, LensClassification, Mutation, MutationCases, SkipReason,
    SmallScope, check_congruence, check_congruence_for_shape, check_get_put,
    check_get_put_for_shape, check_laws, check_laws_exhaustive, check_laws_report, check_locality,
    check_locality_for_shape, check_put_get, check_put_get_for_shape, check_put_injectivity,
    check_put_injectivity_for_shape, check_put_put, check_put_twice, check_put_twice_for_shape,
    check_totality, check_totality_for_shape, check_weak_put_get, check_weak_put_get_for_shape,
    instances_equivalent,
};
pub use lens_graph::LensGraph;
pub use lens_path::{LensPath, NormalGroup, PathComplement, PathComplementEntry, StageIndex};
pub use lens_shape::{GraphShape, LensShape, TreeShape};
pub use optic::{OpticKind, classify_transform, refine_scoped_optic};
pub use panproto_inst::{ErrorDetail, PutDomainError};
pub use protolens::{
    ComplementConstructor, FleetResult, Protolens, ProtolensChain, SchemaConstraint,
    apply_to_fleet, combinators, elementary, horizontal_compose as protolens_horizontal,
    lift_chain, lift_protolens, vertical_compose as protolens_vertical,
};
pub use protolens_nat::{
    ProtolensNaturalitySquare, check_protolens_naturality, protolens_naturality_square,
};
pub use symbolic::{
    SymbolicStep, cancels_to_identity, elementary_get_steps, proves_get_put, proves_put_get,
    simplify_steps,
};
pub use symmetric::{CoherenceViolation, SymmetricLens};
pub use table_protolens::AppliedTableProtolens;
pub use table_shape::{TableContext, TableShape};
pub use table_symmetric::TableSymmetricContext;

use panproto_inst::CompiledMigration;
use panproto_schema::Schema;

/// An asymmetric lens with complement tracking.
///
/// A `Lens` encapsulates a compiled migration between a source and target
/// schema. The `get` direction projects data forward (restricting to the
/// target view), while `put` restores the original source from a modified
/// view and the complement.
#[derive(Debug)]
pub struct Lens {
    /// The compiled migration driving the restrict operation.
    pub compiled: CompiledMigration,
    /// The source schema.
    pub src_schema: Schema,
    /// The target schema (view).
    pub tgt_schema: Schema,
}

impl Lens {
    /// Compute the composite coercion class of this lens's migration.
    ///
    /// Delegates to the underlying `CompiledMigration::coercion_class()`.
    #[must_use]
    pub fn coercion_class(&self) -> panproto_gat::CoercionClass {
        self.compiled.coercion_class()
    }

    /// Whether this lens is an **isomorphism**, so that its complement is
    /// terminal and a source is determined by its view alone.
    ///
    /// A lens with complement decomposes its source as `S ≅ V × C`. It is
    /// invertible on the nose exactly when `C ≅ 1`, since `S ≅ V × 1 ≅ V`,
    /// that is when every complement `get` produces has trivial residue
    /// ([`Complement::residue_is_trivial`](panproto_inst::Complement::residue_is_trivial)).
    /// Survival is read as `get` reads it, through the vertex remap, so an
    /// injective rename is not a drop. The conditions, each one way the
    /// complement acquires residue:
    ///
    /// 1. Every source vertex survives, and none survives only under a
    ///    predicate, so no node's data is set aside.
    /// 2. No two source vertices share an image, so a view anchor
    ///    determines the source anchor.
    /// 3. Every source edge has an image in the target, and no two source
    ///    edges share one, so a view arc determines its source edge.
    /// 4. Every value-level transform is invertible, i.e. the composite
    ///    [`coercion_class`](Self::coercion_class) is
    ///    [`Iso`](panproto_gat::CoercionClass::Iso). A `Retraction`,
    ///    `Projection`, or `Opaque` transform is not injective, so the
    ///    fibre over a view value is not a point and the source cannot be
    ///    recovered from it.
    ///
    /// This is a *static* property of the lens, so it holds for every
    /// source or for none. It is exactly the test
    /// [`put_without_complement`] makes before it reconstructs a source,
    /// so the two agree on every lens, and
    /// [`obstruction_to_isomorphism`](Self::obstruction_to_isomorphism)
    /// names the first condition that fails.
    #[must_use]
    pub fn is_isomorphism(&self) -> bool {
        self.obstruction_to_isomorphism().is_none()
    }

    /// The first reason this lens is not an isomorphism, or `None` when it
    /// is one. See [`is_isomorphism`](Self::is_isomorphism).
    #[must_use]
    pub fn obstruction_to_isomorphism(&self) -> Option<String> {
        asymmetric::obstruction_to_tree_isomorphism(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;

    use panproto_gat::Name;
    use panproto_inst::value::{FieldPresence, Value};
    use panproto_inst::{CompiledMigration, Node, WInstance};
    use panproto_schema::{Edge, Schema, Vertex};
    use smallvec::SmallVec;

    use crate::Lens;

    /// Build a 3-vertex schema: `post:body` (object) with two children
    /// `post:body.text` (string) and `post:body.createdAt` (string).
    pub fn three_node_schema() -> Schema {
        let mut vertices = HashMap::new();
        vertices.insert(
            Name::from("post:body"),
            Vertex {
                id: "post:body".into(),
                kind: "object".into(),
                nsid: None,
            },
        );
        vertices.insert(
            Name::from("post:body.text"),
            Vertex {
                id: "post:body.text".into(),
                kind: "string".into(),
                nsid: None,
            },
        );
        vertices.insert(
            Name::from("post:body.createdAt"),
            Vertex {
                id: "post:body.createdAt".into(),
                kind: "string".into(),
                nsid: None,
            },
        );

        let edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_created = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };

        let mut edges = HashMap::new();
        edges.insert(edge_text.clone(), Name::from("prop"));
        edges.insert(edge_created.clone(), Name::from("prop"));

        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        outgoing
            .entry("post:body".into())
            .or_default()
            .push(edge_text.clone());
        outgoing
            .entry("post:body".into())
            .or_default()
            .push(edge_created.clone());

        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        incoming
            .entry("post:body.text".into())
            .or_default()
            .push(edge_text.clone());
        incoming
            .entry("post:body.createdAt".into())
            .or_default()
            .push(edge_created.clone());

        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
        between
            .entry((Name::from("post:body"), Name::from("post:body.text")))
            .or_default()
            .push(edge_text);
        between
            .entry((Name::from("post:body"), Name::from("post:body.createdAt")))
            .or_default()
            .push(edge_created);

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
            ..Default::default()
        }
    }

    /// Build a 3-node W-type instance matching [`three_node_schema`].
    pub fn three_node_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "post:body"));
        nodes.insert(
            1,
            Node::new(1, "post:body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "post:body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );

        let arcs = vec![
            (
                0,
                1,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            ),
            (
                0,
                2,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.createdAt".into(),
                    kind: "prop".into(),
                    name: Some("createdAt".into()),
                },
            ),
        ];

        WInstance::new(nodes, arcs, vec![], 0, Name::from("post:body"))
    }

    /// Build an identity lens for the given schema.
    pub fn identity_lens(schema: &Schema) -> Lens {
        let surviving_verts = schema.vertices.keys().cloned().collect();
        let surviving_edges = schema.edges.keys().cloned().collect();

        let compiled = CompiledMigration {
            surviving_verts,
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        Lens {
            compiled,
            src_schema: schema.clone(),
            tgt_schema: schema.clone(),
        }
    }

    /// Build a projection lens that removes a single field from the schema.
    pub fn projection_lens(schema: &Schema, field_to_remove: &str) -> Lens {
        let mut tgt_schema = schema.clone();

        // Find and remove the edge + target vertex for this field
        let edges_to_remove: Vec<Edge> = tgt_schema
            .edges
            .keys()
            .filter(|e| e.name.as_deref() == Some(field_to_remove))
            .cloned()
            .collect();

        let mut removed_vertices = Vec::new();
        for edge in &edges_to_remove {
            tgt_schema.edges.remove(edge);
            tgt_schema.vertices.remove(&edge.tgt);
            removed_vertices.push(edge.tgt.clone());
        }

        // Rebuild indices
        crate::protolens::rebuild_indices(&mut tgt_schema);

        let mut surviving_verts: std::collections::HashSet<Name> =
            schema.vertices.keys().cloned().collect();
        let mut surviving_edges: std::collections::HashSet<Edge> =
            schema.edges.keys().cloned().collect();

        for v in &removed_vertices {
            surviving_verts.remove(v);
        }
        for e in &edges_to_remove {
            surviving_edges.remove(e);
        }

        let compiled = CompiledMigration {
            surviving_verts,
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        Lens {
            compiled,
            src_schema: schema.clone(),
            tgt_schema,
        }
    }

    // -----------------------------------------------------------------------
    // Test 1: Identity lens satisfies laws (covered in laws.rs)
    // Test 2: Compose rename + add_field laws (below)
    // Test 3: Round-trip get/put (below)
    // Test 4: Modified view propagation (below)
    // -----------------------------------------------------------------------

    #[test]
    fn round_trip_get_then_put_recovers_original() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let (view, complement) =
            crate::get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));
        let restored =
            crate::put(&lens, &view, &complement).unwrap_or_else(|e| panic!("put failed: {e}"));

        assert_eq!(restored.node_count(), instance.node_count());
        assert_eq!(restored.root, instance.root);
        assert_eq!(restored.schema_root, instance.schema_root);

        // Verify all node anchors match
        for (&id, node) in &instance.nodes {
            let restored_node = restored
                .nodes
                .get(&id)
                .unwrap_or_else(|| panic!("node {id} missing from restored instance"));
            assert_eq!(
                node.anchor, restored_node.anchor,
                "anchor mismatch for node {id}"
            );
        }
    }

    #[test]
    fn modified_view_propagates_changes() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        // Get the view
        let (mut view, complement) =
            crate::get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));

        // Modify a field in the view
        if let Some(node) = view.nodes.get_mut(&1) {
            node.value = Some(FieldPresence::Present(Value::Str("modified".into())));
        }

        // Put back
        let restored =
            crate::put(&lens, &view, &complement).unwrap_or_else(|e| panic!("put failed: {e}"));

        // Verify the modification propagated
        let node = restored
            .nodes
            .get(&1)
            .unwrap_or_else(|| panic!("node 1 missing"));
        assert_eq!(
            node.value,
            Some(FieldPresence::Present(Value::Str("modified".into()))),
            "modification should be preserved"
        );
    }

    #[test]
    fn projection_lens_drops_field() {
        let schema = three_node_schema();
        let lens = projection_lens(&schema, "createdAt");
        let instance = three_node_instance();

        let (view, complement) =
            crate::get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));

        assert_eq!(view.node_count(), 2, "projection should drop one node");
        assert!(
            !complement.dropped_nodes.is_empty(),
            "complement should have dropped node"
        );
    }

    #[test]
    fn projection_get_then_put_restores_with_complement() {
        let schema = three_node_schema();
        let lens = projection_lens(&schema, "createdAt");
        let instance = three_node_instance();

        let (view, complement) =
            crate::get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));

        let restored =
            crate::put(&lens, &view, &complement).unwrap_or_else(|e| panic!("put failed: {e}"));

        assert_eq!(
            restored.node_count(),
            instance.node_count(),
            "restoration should bring back all nodes"
        );
    }

    #[test]
    fn compose_rename_then_identity_preserves_laws() {
        let schema = three_node_schema();
        let l1 = identity_lens(&schema);
        let l2 = identity_lens(&schema);

        let composed = crate::compose(&l1, &l2).unwrap_or_else(|e| panic!("compose failed: {e}"));
        let instance = three_node_instance();

        let (view, complement) = composed
            .get(&instance)
            .unwrap_or_else(|error| panic!("path get failed: {error}"));
        let restored = composed
            .put(&view, &complement)
            .unwrap_or_else(|error| panic!("path put failed: {error}"));
        assert!(crate::instances_equivalent(&restored, &instance));
    }
}
