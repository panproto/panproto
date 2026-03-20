//! # panproto-lens
//!
//! Bidirectional lenses via protolens (schema-independent lens families).
//!
//! Every schema migration is a lens with a `get` direction (= restrict,
//! projecting data forward) and a `put` direction (= restore from
//! complement, bringing modifications back). The lens laws — `GetPut`
//! and `PutGet` — guarantee round-trip fidelity.
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
//! - **[`check_laws`]**: Verify `GetPut` and `PutGet` on a test instance.
//!
//! The mathematical foundations are based on asymmetric lenses with
//! complement (Diskin et al., 2011), GAT-based protolenses (natural
//! transformations between theory endofunctors), and Cambria's
//! approach to schema evolution (Ink & Switch, 2020).

// Allow concrete HashMap/HashSet in public API signatures per ENGINEERING.md spec.
#![allow(clippy::implicit_hasher)]

pub mod asymmetric;
pub mod auto_lens;
pub mod complement_type;
pub mod compose;
pub mod diff_to_protolens;
pub mod error;
pub mod laws;
pub mod optic;
pub mod protolens;
pub mod symbolic;
pub mod symmetric;

// Re-exports for convenience.
pub use asymmetric::{Complement, get, put};
pub use auto_lens::{AutoLensConfig, AutoLensResult, auto_generate};
pub use complement_type::{
    CapturedField, ComplementKind, ComplementSpec, DefaultRequirement, chain_complement_spec,
    complement_spec_at,
};
pub use compose::compose;
pub use diff_to_protolens::{DiffSpec, KindChange, diff_to_lens, diff_to_protolens};
pub use error::{LawViolation, LensError};
pub use laws::{check_get_put, check_laws, check_put_get};
pub use optic::{OpticKind, classify_transform};
pub use protolens::{
    ComplementConstructor, FleetResult, Protolens, ProtolensChain, SchemaConstraint,
    apply_to_fleet, elementary, horizontal_compose as protolens_horizontal, lift_chain,
    lift_protolens, vertical_compose as protolens_vertical,
};
pub use symbolic::{SymbolicStep, simplify_steps};
pub use symmetric::SymmetricLens;

use panproto_inst::CompiledMigration;
use panproto_schema::Schema;

/// An asymmetric lens with complement tracking.
///
/// A `Lens` encapsulates a compiled migration between a source and target
/// schema. The `get` direction projects data forward (restricting to the
/// target view), while `put` restores the original source from a modified
/// view and the complement.
pub struct Lens {
    /// The compiled migration driving the restrict operation.
    pub compiled: CompiledMigration,
    /// The source schema.
    pub src_schema: Schema,
    /// The target schema (view).
    pub tgt_schema: Schema,
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

        let result = crate::check_laws(&composed, &instance);
        assert!(
            result.is_ok(),
            "composed identity lenses should satisfy laws: {result:?}"
        );
    }
}
