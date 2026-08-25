//! The morphism tower: Theory → Schema → Instance.
//!
//! Theory morphisms induce schema morphisms by renaming sorts and
//! operations. The schema morphisms are then compiled into the tables
//! used by panproto's forward, restrict-based instance projection. This
//! module provides the cascade functions that connect those levels.

use std::collections::HashMap;

use panproto_gat::{Name, NameSite, SiteRename, TheoryMorphism};
use panproto_inst::CompiledMigration;
use panproto_schema::{Edge, Schema, SchemaMorphism};

/// Induce a schema morphism from a theory morphism.
///
/// Given a theory morphism F: T1 → T2 and a schema built from T1,
/// produce the corresponding schema morphism that:
/// - Renames vertex kinds via `sort_map` (sorts map to vertex kinds)
/// - Renames edge kinds via `op_map` (operations map to edge kinds)
/// - Preserves vertex IDs (identity on structure)
///
/// The induced morphism is the functorial action of F on the category
/// of schemas.
#[must_use]
pub fn induce_schema_morphism(
    theory_morph: &TheoryMorphism,
    src_schema: &Schema,
) -> SchemaMorphism {
    let renames = theory_morph.induce_schema_renames();

    // Build vertex map: all vertices survive with same IDs
    let vertex_map: HashMap<Name, Name> = src_schema
        .vertices
        .keys()
        .map(|id| (id.clone(), id.clone()))
        .collect();

    // Build edge map: rename edge kinds via op_map
    let mut edge_map: HashMap<Edge, Edge> = HashMap::new();
    for edge in src_schema.edges.keys() {
        let mut new_edge = edge.clone();
        if let Some(new_kind) = theory_morph
            .op_map
            .get(edge.kind.as_ref())
            .and_then(panproto_gat::OpAssignment::as_op)
        {
            new_edge.kind = Name::from(&**new_kind);
        }
        edge_map.insert(edge.clone(), new_edge);
    }

    SchemaMorphism {
        name: format!("induced_{}", theory_morph.name),
        src_protocol: theory_morph.domain.to_string(),
        tgt_protocol: theory_morph.codomain.to_string(),
        vertex_map,
        edge_map,
        renames,
    }
}

/// Induce a data migration from a schema morphism.
///
/// The result records the surviving target elements, remappings, and
/// contraction resolvers needed by the restrict pipeline. Applying it
/// through `lift_wtype` or `lift_functor` carries source data forward;
/// those operations are not categorical precomposition `Delta_F`.
#[must_use]
pub fn induce_data_migration(
    schema_morph: &SchemaMorphism,
    tgt_schema: &Schema,
) -> CompiledMigration {
    compile_schema_morphism(schema_morph, tgt_schema)
}

/// Lower a schema morphism to a `CompiledMigration`.
///
/// Computes surviving vertex/edge sets, remapping tables, and the
/// resolver for ancestor contraction.
#[must_use]
fn compile_schema_morphism(
    schema_morph: &SchemaMorphism,
    tgt_schema: &Schema,
) -> CompiledMigration {
    let surviving_verts: std::collections::HashSet<Name> =
        schema_morph.vertex_map.values().cloned().collect();

    let surviving_edges: std::collections::HashSet<Edge> =
        schema_morph.edge_map.values().cloned().collect();

    let mut vertex_remap = HashMap::new();
    for (src, tgt) in &schema_morph.vertex_map {
        if src != tgt {
            vertex_remap.insert(src.clone(), tgt.clone());
        }
    }

    let mut edge_remap = HashMap::new();
    for (src_e, tgt_e) in &schema_morph.edge_map {
        if src_e != tgt_e {
            edge_remap.insert(src_e.clone(), tgt_e.clone());
        }
    }

    // Build resolver for edges between surviving vertices in the target
    let mut resolver = HashMap::new();
    for edge in tgt_schema.edges.keys() {
        if surviving_verts.contains(&edge.src) && surviving_verts.contains(&edge.tgt) {
            resolver.insert((edge.src.clone(), edge.tgt.clone()), edge.clone());
        }
    }

    CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    }
}

/// Induce a complete migration pipeline from a theory morphism.
///
/// Convenience function that chains `induce_schema_morphism` and
/// `induce_data_migration`.
#[must_use]
pub fn induce_migration_from_theory(
    theory_morph: &TheoryMorphism,
    src_schema: &Schema,
    tgt_schema: &Schema,
) -> (SchemaMorphism, CompiledMigration) {
    let schema_morph = induce_schema_morphism(theory_morph, src_schema);
    let compiled = induce_data_migration(&schema_morph, tgt_schema);
    (schema_morph, compiled)
}

/// Collect all site renames from a theory morphism.
///
/// This is a convenience re-export of
/// [`TheoryMorphism::induce_schema_renames`].
#[must_use]
pub fn theory_renames(theory_morph: &TheoryMorphism) -> Vec<SiteRename> {
    theory_morph.induce_schema_renames()
}

/// Check if a site rename affects a specific naming site.
#[must_use]
pub fn rename_affects_site(rename: &SiteRename, site: &NameSite) -> bool {
    rename.site == *site
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_morphism() -> TheoryMorphism {
        TheoryMorphism::new(
            "test",
            "ThGraph",
            "ThRenamedGraph",
            HashMap::from([
                (Arc::from("Vertex"), Arc::from("Node")),
                (Arc::from("Edge"), Arc::from("Arrow")),
            ]),
            HashMap::from([
                (Arc::from("src"), Arc::from("source")),
                (Arc::from("tgt"), Arc::from("target")),
            ]),
        )
    }

    fn simple_schema() -> Schema {
        use panproto_schema::{Protocol, SchemaBuilder};

        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };

        SchemaBuilder::new(&protocol)
            .vertex("root", "record", None::<&str>)
            .unwrap()
            .vertex("root.name", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.name", "src", Some("name"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// A `SiteRename` is matched by its `NameSite`, not by its display
    /// name. Two renames that share an `old` string but target
    /// different sites are distinct values and `rename_affects_site`
    /// returns `true` only when the sites agree.
    #[test]
    fn site_rename_does_not_cross_sites() {
        let vertex_rename = SiteRename::new(NameSite::VertexKind, "x", "y");
        let edge_rename = SiteRename::new(NameSite::EdgeKind, "x", "y");

        // Each rename affects only its declared site.
        assert!(rename_affects_site(&vertex_rename, &NameSite::VertexKind));
        assert!(!rename_affects_site(&vertex_rename, &NameSite::EdgeKind));
        assert!(rename_affects_site(&edge_rename, &NameSite::EdgeKind));
        assert!(!rename_affects_site(&edge_rename, &NameSite::VertexKind));

        // Same display name but different site: not equal, so filters
        // that key on `site` keep them apart.
        assert_ne!(vertex_rename, edge_rename);
        assert_ne!(vertex_rename.site, edge_rename.site);
    }

    #[test]
    fn induce_schema_morphism_renames_edge_kinds() {
        let morph = test_morphism();
        let schema = simple_schema();
        let schema_morph = induce_schema_morphism(&morph, &schema);

        // Edge kind "src" should be mapped to "source"
        for (src_e, tgt_e) in &schema_morph.edge_map {
            if src_e.kind == "src" {
                assert_eq!(tgt_e.kind, "source");
            }
        }
    }

    #[test]
    fn induce_schema_morphism_preserves_vertex_ids() {
        let morph = test_morphism();
        let schema = simple_schema();
        let schema_morph = induce_schema_morphism(&morph, &schema);

        for (src_id, tgt_id) in &schema_morph.vertex_map {
            assert_eq!(src_id, tgt_id, "vertex IDs should be unchanged");
        }
    }

    #[test]
    fn induce_schema_morphism_records_renames() {
        let morph = test_morphism();
        let schema = simple_schema();
        let schema_morph = induce_schema_morphism(&morph, &schema);

        // Should have renames for the sort and op changes
        assert!(
            !schema_morph.renames.is_empty(),
            "renames should be non-empty"
        );

        let has_vertex_kind_rename = schema_morph
            .renames
            .iter()
            .any(|r| r.site == NameSite::VertexKind);
        let has_edge_kind_rename = schema_morph
            .renames
            .iter()
            .any(|r| r.site == NameSite::EdgeKind);
        assert!(has_vertex_kind_rename, "should have vertex kind renames");
        assert!(has_edge_kind_rename, "should have edge kind renames");
    }

    #[test]
    fn induce_data_migration_produces_compiled() {
        let morph = test_morphism();
        let schema = simple_schema();
        let schema_morph = induce_schema_morphism(&morph, &schema);
        let compiled = induce_data_migration(&schema_morph, &schema);

        // All vertices should survive (no structural changes)
        assert_eq!(compiled.surviving_verts.len(), schema.vertices.len());
    }
}
