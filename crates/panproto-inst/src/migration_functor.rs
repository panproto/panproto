//! The functor a compiled migration states between relation-ified schemas.
//!
//! Every table operation along a migration (`Delta_F`, `Sigma_F`, `Pi_F`,
//! their units, counits and transposes) reads one [`MigrationFunctor`], built
//! once from the two schemas and the compiled migration, so they agree on
//! what `F` is.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::rel_schema::RelSchema;
use crate::wtype::CompiledMigration;

/// A functor `F: S' -> T` between relation-ified schemas ([`RelSchema`]).
///
/// `S'` is the part of the source schema the migration keeps: the source
/// tables the migration names an image for (keys of `vertex_remap`) or lets
/// survive under their own name (members of `surviving_verts` that are
/// tables of the source schema and that `vertex_remap` names as no table's
/// image, since `panproto_mig::compile` lists those images as surviving
/// target vertices), and the source foreign keys between two
/// kept tables that the migration carries (keys of `edge_remap`) or lets
/// survive (members of `surviving_edges` that are edges of the source
/// schema). A source table in `vertex_remap` goes to its entry, a surviving
/// one to itself; a carried foreign key goes to its `edge_remap` entry, a
/// surviving one to the edge of its kind and name between the images of its
/// endpoints. The arrows `src_e` and `tgt_e` of a kept foreign key go to the
/// arrows `src_F(e)` and `tgt_F(e)`, which is what makes `F` a functor.
///
/// An edge of the source schema the migration does not mention is outside
/// `S'`: no operation reads its pairs, and none is constrained by it.
/// Members of `surviving_edges` and `surviving_verts` are read against the
/// source schema, so an edge or vertex only the target has (as
/// `panproto_mig::compile` lists the surviving image edges) is never taken
/// for a source one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationFunctor {
    /// The relation-ified source schema `S` (of which `S'` is a part).
    source: RelSchema,
    /// The relation-ified target schema `T`.
    target: RelSchema,
    /// The image of each table of `S'`.
    tables: BTreeMap<Name, Name>,
    /// The image of each relation of `S'`.
    relations: BTreeMap<Edge, Edge>,
}

impl MigrationFunctor {
    /// The functor `migration` states from `src` to `tgt`, read as the type
    /// documents.
    ///
    /// # Errors
    ///
    /// Returns [`RestrictError::VertexImageNotATable`] when a kept source
    /// table's image is not a table of `tgt`, and
    /// [`RestrictError::IncoherentEdgeImage`] when a kept foreign key's image
    /// is not a foreign key of `tgt` running from the image of its source to
    /// the image of its target. Tables are checked before foreign keys, each
    /// in ascending order.
    pub fn new(
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        Self::between(RelSchema::new(src), RelSchema::new(tgt), migration)
    }

    /// [`Self::new`] over relation-ified schemas built already.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn between(
        source: RelSchema,
        target: RelSchema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        // A listed surviving vertex that the vertex map names as some table's
        // image is a target vertex, not a source one.
        let images: BTreeSet<&Name> = migration.vertex_remap.values().collect();
        let mut tables = BTreeMap::new();
        for table in source.tables() {
            let image = match migration.vertex_remap.get(table) {
                Some(image) => image,
                None if migration.surviving_verts.contains(table) && !images.contains(table) => {
                    table
                }
                None => continue,
            };
            if !target.is_table(image) {
                return Err(RestrictError::VertexImageNotATable {
                    vertex: table.clone(),
                    image: image.clone(),
                });
            }
            tables.insert(table.clone(), image.clone());
        }
        let mut relations = BTreeMap::new();
        for edge in source.relations() {
            let (Some(src), Some(tgt)) = (tables.get(&edge.src), tables.get(&edge.tgt)) else {
                continue;
            };
            let image = match migration.edge_remap.get(edge) {
                Some(image) => image.clone(),
                None if migration.surviving_edges.contains(edge) => Edge {
                    src: src.clone(),
                    tgt: tgt.clone(),
                    kind: edge.kind.clone(),
                    name: edge.name.clone(),
                },
                None => continue,
            };
            if &image.src != src || &image.tgt != tgt || !target.is_relation(&image) {
                return Err(RestrictError::IncoherentEdgeImage {
                    edge: Box::new(edge.clone()),
                    image: Box::new(image),
                });
            }
            relations.insert(edge.clone(), image);
        }
        Ok(Self {
            source,
            target,
            tables,
            relations,
        })
    }

    /// The identity functor on the relation-ified schema of `schema`.
    #[must_use]
    pub fn identity(schema: &Schema) -> Self {
        let rel = RelSchema::new(schema);
        let tables = rel
            .tables()
            .iter()
            .map(|t| (t.clone(), t.clone()))
            .collect();
        let relations = rel
            .relations()
            .iter()
            .map(|e| (e.clone(), e.clone()))
            .collect();
        Self {
            source: rel.clone(),
            target: rel,
            tables,
            relations,
        }
    }

    /// The relation-ified source schema `S`.
    #[must_use]
    pub const fn source(&self) -> &RelSchema {
        &self.source
    }

    /// The relation-ified target schema `T`.
    #[must_use]
    pub const fn target(&self) -> &RelSchema {
        &self.target
    }

    /// The tables of `S'` with their images, ascending.
    #[must_use]
    pub const fn table_map(&self) -> &BTreeMap<Name, Name> {
        &self.tables
    }

    /// The relations of `S'` with their images, ascending.
    #[must_use]
    pub const fn relation_map(&self) -> &BTreeMap<Edge, Edge> {
        &self.relations
    }

    /// The image of a table of `S'`, or `None` outside `S'`.
    #[must_use]
    pub fn table_image(&self, table: &str) -> Option<&Name> {
        self.tables.get(table)
    }

    /// The image of a relation of `S'`, or `None` outside `S'`.
    #[must_use]
    pub fn relation_image(&self, edge: &Edge) -> Option<&Edge> {
        self.relations.get(edge)
    }

    /// The fibre of a target table: the tables of `S'` sent to it, ascending.
    #[must_use]
    pub fn table_fibre(&self, table: &str) -> Vec<&Name> {
        self.tables
            .iter()
            .filter(|(_, image)| image.as_ref() == table)
            .map(|(source, _)| source)
            .collect()
    }

    /// The fibre of a target relation: the relations of `S'` sent to it,
    /// ascending.
    #[must_use]
    pub fn relation_fibre(&self, edge: &Edge) -> Vec<&Edge> {
        self.relations
            .iter()
            .filter(|(_, image)| *image == edge)
            .map(|(source, _)| source)
            .collect()
    }

    /// The fibres of every target table, including the empty fibres of the
    /// tables outside the image, by target table ascending.
    pub(crate) fn table_fibres(&self) -> BTreeMap<&Name, Vec<&Name>> {
        let mut fibres: BTreeMap<&Name, Vec<&Name>> = self
            .target
            .tables()
            .iter()
            .map(|t| (t, Vec::new()))
            .collect();
        for (source, image) in &self.tables {
            fibres.entry(image).or_default().push(source);
        }
        fibres
    }

    /// The fibres of every target relation, including the empty ones, by
    /// target relation ascending.
    pub(crate) fn relation_fibres(&self) -> BTreeMap<&Edge, Vec<&Edge>> {
        let mut fibres: BTreeMap<&Edge, Vec<&Edge>> = self
            .target
            .relations()
            .iter()
            .map(|e| (e, Vec::new()))
            .collect();
        for (source, image) in &self.relations {
            fibres.entry(image).or_default().push(source);
        }
        fibres
    }

    /// The tables of `T` outside the image of `F`, ascending.
    #[must_use]
    pub fn tables_outside_image(&self) -> BTreeSet<&Name> {
        let image: BTreeSet<&Name> = self.tables.values().collect();
        self.target
            .tables()
            .iter()
            .filter(|t| !image.contains(t))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use panproto_schema::Vertex;

    use super::*;

    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    fn table_schema(tables: &[&str], relations: &[Edge]) -> Schema {
        Schema {
            vertices: tables
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        Vertex {
                            id: v.into(),
                            kind: "table".into(),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: relations
                .iter()
                .map(|e| (e.clone(), e.kind.clone()))
                .collect(),
            ..Schema::default()
        }
    }

    /// `panproto_mig::compile` lists the surviving image edges, which are
    /// target edges; one that the source does not have is not read as a
    /// source edge, so the functor's domain holds no relation and Delta
    /// copies none of its pairs back.
    #[test]
    fn a_target_only_edge_is_not_a_source_edge() -> Result<(), Box<dyn std::error::Error>> {
        let e = fk("a", "b", "e");
        let src = table_schema(&["a", "b"], &[]);
        let tgt = table_schema(&["a", "b"], std::slice::from_ref(&e));
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        m.surviving_edges.insert(e.clone());
        let f = MigrationFunctor::new(&src, &tgt, &m)?;
        assert!(f.relation_map().is_empty());
        assert_eq!(f.relation_fibre(&e), Vec::<&Edge>::new());
        let y = crate::FInstance::new()
            .with_rows("a", [(crate::RowId(0), crate::Row::new())])
            .and_then(|i| i.with_rows("b", [(crate::RowId(1), crate::Row::new())]))
            .and_then(|i| i.with_foreign_key(e.clone(), [(crate::RowId(0), crate::RowId(1))]))?;
        let delta = crate::f_delta(&y, &f)?;
        assert_eq!(delta.pair_count(), 0, "{delta:?}");
        assert_eq!(delta.total_row_count(), 2);
        Ok(())
    }

    #[test]
    fn an_incoherent_edge_image_is_refused() {
        let e = fk("a", "b", "e");
        let src = table_schema(&["a", "b", "c"], std::slice::from_ref(&e));
        let tgt = table_schema(&["a", "b", "c"], &[fk("a", "c", "E"), fk("a", "b", "E")]);
        let mut m = CompiledMigration::default();
        m.surviving_verts
            .extend(["a".into(), "b".into(), "c".into()]);
        m.edge_remap.insert(e.clone(), fk("a", "c", "E"));
        assert!(matches!(
            MigrationFunctor::new(&src, &tgt, &m),
            Err(RestrictError::IncoherentEdgeImage { edge, image })
                if *edge == e && *image == fk("a", "c", "E")
        ));
        // An image between the right tables that the target lacks is refused
        // too.
        m.edge_remap.insert(e.clone(), fk("a", "b", "missing"));
        assert!(matches!(
            MigrationFunctor::new(&src, &tgt, &m),
            Err(RestrictError::IncoherentEdgeImage { .. })
        ));
        m.edge_remap.insert(e, fk("a", "b", "E"));
        assert!(MigrationFunctor::new(&src, &tgt, &m).is_ok());
        // A table sent to a vertex that is no table of the target.
        m.vertex_remap.insert("c".into(), "nowhere".into());
        assert!(matches!(
            MigrationFunctor::new(&src, &tgt, &m),
            Err(RestrictError::VertexImageNotATable { vertex, image })
                if vertex == "c" && image == "nowhere"
        ));
    }

    #[test]
    fn unmentioned_edges_are_outside_the_domain() -> Result<(), Box<dyn std::error::Error>> {
        let kept = fk("a", "b", "kept");
        let unmentioned = fk("b", "a", "back");
        let to_dropped = fk("a", "gone", "dangling");
        let src = table_schema(
            &["a", "b", "gone"],
            &[kept.clone(), unmentioned.clone(), to_dropped.clone()],
        );
        let tgt = table_schema(&["m"], &[fk("m", "m", "K")]);
        let mut m = CompiledMigration::default();
        m.vertex_remap.insert("a".into(), "m".into());
        m.vertex_remap.insert("b".into(), "m".into());
        m.surviving_verts.insert("m".into());
        m.edge_remap.insert(kept.clone(), fk("m", "m", "K"));
        // Carried, but its target table is outside the domain.
        m.edge_remap.insert(to_dropped.clone(), fk("m", "m", "K"));
        let f = MigrationFunctor::new(&src, &tgt, &m)?;
        assert_eq!(f.relation_map().keys().collect::<Vec<_>>(), [&kept]);
        assert!(f.relation_image(&unmentioned).is_none());
        assert!(f.relation_image(&to_dropped).is_none());
        assert_eq!(f.table_fibre("m"), [&Name::from("a"), &Name::from("b")]);
        assert!(f.table_image("gone").is_none());
        Ok(())
    }

    /// `panproto_mig::compile` lists the images of the vertex map as
    /// surviving vertices. A source table that the vertex map leaves out but
    /// whose name is the image of another table is dropped, not kept: the
    /// listed name is a target vertex, as a listed image edge is a target
    /// edge.
    #[test]
    fn a_surviving_image_is_not_a_source_table() -> Result<(), Box<dyn std::error::Error>> {
        let src = table_schema(&["a", "b"], &[]);
        let tgt = table_schema(&["b"], &[]);
        let mut m = CompiledMigration::default();
        m.vertex_remap.insert("a".into(), "b".into());
        m.surviving_verts.insert("b".into());
        let f = MigrationFunctor::new(&src, &tgt, &m)?;
        assert_eq!(
            f.table_map().keys().collect::<Vec<_>>(),
            [&Name::from("a")],
            "source b is outside the domain"
        );
        assert!(f.table_image("b").is_none());
        // Listed under its own image too, it is kept.
        m.vertex_remap.insert("b".into(), "b".into());
        let f = MigrationFunctor::new(&src, &tgt, &m)?;
        assert_eq!(f.table_fibre("b"), [&Name::from("a"), &Name::from("b")]);
        // A surviving vertex no table is sent to survives under its name.
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        let f = MigrationFunctor::new(&src, &table_schema(&["a", "b"], &[]), &m)?;
        assert_eq!(f.table_map().len(), 2);
        Ok(())
    }

    #[test]
    fn the_identity_functor_keeps_every_table_and_relation() {
        let e = fk("a", "a", "self");
        let schema = table_schema(&["a", "b"], std::slice::from_ref(&e));
        let f = MigrationFunctor::identity(&schema);
        assert_eq!(f.table_image("b"), Some(&Name::from("b")));
        assert_eq!(f.relation_image(&e), Some(&e));
        assert!(f.tables_outside_image().is_empty());
    }
}
