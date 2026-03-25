//! Incremental migration via edit lenses.
//!
//! Translates sequences of [`TreeEdit`] values through an [`EditLens`],
//! producing the corresponding edit sequence in the target schema. This
//! enables incremental migration: instead of re-migrating entire data
//! sets, individual edits flow through the lens.

use panproto_inst::TreeEdit;
use panproto_lens::EditLens;

use crate::error::VcsError;

/// Translate a sequence of source edits through an edit lens.
///
/// Each edit is passed to [`EditLens::get_edit`]. Non-identity results
/// are collected into the output sequence.
///
/// # Errors
///
/// Returns [`VcsError::DataMigrationFailed`] if any edit translation fails.
pub fn incremental_migrate(
    edits: &[TreeEdit],
    lens: &mut EditLens,
) -> Result<Vec<TreeEdit>, VcsError> {
    let mut translated = Vec::with_capacity(edits.len());
    for edit in edits {
        let result = lens
            .get_edit(edit.clone())
            .map_err(|e| VcsError::DataMigrationFailed {
                reason: e.to_string(),
            })?;
        if !result.is_identity() {
            translated.push(result);
        }
    }
    Ok(translated)
}

/// Encode an edit sequence as `MessagePack` bytes.
///
/// # Errors
///
/// Returns [`VcsError::Serialization`] if encoding fails.
pub fn encode_edit_log(edits: &[TreeEdit]) -> Result<Vec<u8>, VcsError> {
    Ok(rmp_serde::to_vec(edits)?)
}

/// Decode an edit sequence from `MessagePack` bytes.
///
/// # Errors
///
/// Returns [`VcsError::Serialization`] if decoding fails.
pub fn decode_edit_log(bytes: &[u8]) -> Result<Vec<TreeEdit>, VcsError> {
    Ok(rmp_serde::from_slice(bytes)?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_gat::Name;
    use panproto_inst::{CompiledMigration, Node, TreeEdit, WInstance};
    use panproto_schema::{Edge, Protocol, Schema, Vertex};
    use smallvec::SmallVec;

    use panproto_lens::{EditLens, Lens};

    use super::*;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: false,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        }
    }

    fn simple_schema() -> Schema {
        let mut vertices = HashMap::new();
        vertices.insert(
            Name::from("root"),
            Vertex {
                id: "root".into(),
                kind: "object".into(),
                nsid: None,
            },
        );
        vertices.insert(
            Name::from("child"),
            Vertex {
                id: "child".into(),
                kind: "string".into(),
                nsid: None,
            },
        );

        let edge = Edge {
            src: "root".into(),
            tgt: "child".into(),
            kind: "prop".into(),
            name: Some("name".into()),
        };

        let mut edges = HashMap::new();
        edges.insert(edge.clone(), Name::from("prop"));

        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        outgoing
            .entry("root".into())
            .or_default()
            .push(edge.clone());

        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        incoming
            .entry("child".into())
            .or_default()
            .push(edge.clone());

        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
        between
            .entry((Name::from("root"), Name::from("child")))
            .or_default()
            .push(edge);

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

    fn identity_lens(schema: &Schema) -> Lens {
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

    fn simple_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "child"));
        let arcs = vec![(
            0,
            1,
            Edge {
                src: "root".into(),
                tgt: "child".into(),
                kind: "prop".into(),
                name: Some("name".into()),
            },
        )];
        WInstance::new(nodes, arcs, vec![], 0, Name::from("root"))
    }

    #[test]
    fn incremental_migrate_basic() {
        let schema = simple_schema();
        let lens = identity_lens(&schema);
        let instance = simple_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edits = vec![
            TreeEdit::SetField {
                node_id: 1,
                field: Name::from("value"),
                value: panproto_inst::Value::Str("hello".into()),
            },
            TreeEdit::Identity,
            TreeEdit::SetField {
                node_id: 0,
                field: Name::from("tag"),
                value: panproto_inst::Value::Int(42),
            },
        ];

        let result = incremental_migrate(&edits, &mut edit_lens).unwrap();
        // Identity edits are filtered out.
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn encode_decode_edit_log() {
        let edits = vec![
            TreeEdit::SetField {
                node_id: 1,
                field: Name::from("x"),
                value: panproto_inst::Value::Int(10),
            },
            TreeEdit::DeleteNode { id: 99 },
        ];

        let bytes = encode_edit_log(&edits).unwrap();
        let decoded = decode_edit_log(&bytes).unwrap();
        assert_eq!(decoded.len(), 2);

        // Verify structure is preserved.
        match &decoded[0] {
            TreeEdit::SetField { node_id, field, .. } => {
                assert_eq!(*node_id, 1);
                assert_eq!(field, &Name::from("x"));
            }
            other => panic!("expected SetField, got {other:?}"),
        }
        match &decoded[1] {
            TreeEdit::DeleteNode { id } => assert_eq!(*id, 99),
            other => panic!("expected DeleteNode, got {other:?}"),
        }
    }
}
