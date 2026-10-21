//! The functor a compiled migration states between attributed relation-ified
//! schemas.
//!
//! A migration states a functor `F: S' -> T` from the part `S'` of the
//! source [`RelSchema`] it keeps to the target one: a map on tables, one on
//! relations, and one on the columns of each kept table. Two types carry it.
//! [`EntityFunctor`] is the functor together with the migration's remaps of
//! edges inside column values; the table restrict reads it, and applies
//! those remaps to the values it copies. [`MigrationFunctor`] is the functor
//! the Kan extensions (`Delta_F`, `Sigma_F`, `Pi_F`, their units, counits,
//! transposes and actions on homomorphisms) read; it is obtained only from
//! an entity functor whose value remaps change no value's structure
//! ([`EntityFunctor::into_kan`]), so the Kan extensions never see one that
//! does.
//!
//! One rule reads the migration's lists of survivors: a name listed in
//! `surviving_verts` or `surviving_edges` that `vertex_remap` or `edge_remap`
//! names as the image of something is a target element (as
//! `panproto_mig::compile` lists the images there), not a source one; any
//! other listed name is read against the source schema.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Protocol, Schema};

use crate::error::RestrictError;
use crate::rel_schema::{RelSchema, RelSchemaError};
use crate::wtype::CompiledMigration;

/// The maps of a functor between relation-ified schemas, shared by
/// [`EntityFunctor`] and [`MigrationFunctor`].
#[derive(Clone, Debug, PartialEq, Eq)]
struct Maps {
    /// The relation-ified source schema `S` (of which `S'` is a part).
    source: RelSchema,
    /// The relation-ified target schema `T`.
    target: RelSchema,
    /// The image of each table of `S'`.
    tables: BTreeMap<Name, Name>,
    /// The image of each relation of `S'`.
    relations: BTreeMap<Edge, Edge>,
    /// The image column of each kept column `(source table, column)`: a
    /// column of the image of the table.
    columns: BTreeMap<(Name, String), String>,
}

/// The functor a migration states between relation-ified schemas, with the
/// migration's remaps of the edges inside column values.
///
/// Built by [`MigrationFunctor::entity_part`] or [`EntityFunctor::between`],
/// which run every check of the functor except the one on value remaps;
/// [`EntityFunctor::into_kan`] runs that one and yields the
/// [`MigrationFunctor`] the Kan extensions take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityFunctor {
    /// The maps on tables, relations and columns.
    maps: Maps,
    /// Each `edge_remap` entry whose key is an inner value edge of the
    /// source, with its image.
    inner_edges: BTreeMap<Edge, Edge>,
    /// Each `vertex_remap` entry whose key is a value vertex of the source.
    value_remap: BTreeMap<Name, Name>,
}

/// The functor `F: S' -> T` the Kan extensions of table instances take.
///
/// `S'` holds the source tables the migration keeps: those `vertex_remap`
/// maps, and those it lets survive under their own name. Of the relations
/// between kept tables it holds those `edge_remap` maps or lets survive, and
/// of the columns (attribute arrows) of kept tables those `edge_remap` maps
/// or lets survive. A kept table goes to its `vertex_remap` entry or to
/// itself; a kept relation to its `edge_remap` entry or to the edge of its
/// kind and name between the images of its endpoints; a kept column to the
/// column of its `edge_remap` entry, or of the target attribute arrow of its
/// kind and name from the image of its table. The arrows `src_e` and `tgt_e`
/// of a kept relation go to those of its image, which is what makes `F` a
/// functor; on the value side `F` is the identity. The functor also keeps
/// the migration's remaps of the vertices and edges inside column values
/// ([`Self::inner_edges`], [`Self::value_vertices`]), which no table function
/// reads and along which the graph Kan extensions move the layout of a
/// structured cell. An edge or column the migration does not mention is
/// outside `S'`: no operation reads it, and none is constrained by it. The
/// column map may send two columns of one table to one column, and may miss
/// columns of the target.
///
/// ```
/// use std::collections::{BTreeMap, BTreeSet, HashMap};
///
/// use panproto_inst::{CompiledMigration, MigrationFunctor};
/// use panproto_schema::{Edge, Protocol, ScalarKind, Schema, Vertex};
///
/// let vertex = |id: &str, kind: &str| {
///     (id.into(), Vertex { id: id.into(), kind: kind.into(), nsid: None })
/// };
/// let column = |table: &str, name: &str| Edge {
///     src: table.into(),
///     tgt: format!("{table}.{name}").into(),
///     kind: "prop".into(),
///     name: Some(name.into()),
/// };
/// let schema = |name: &str| Schema {
///     vertices: HashMap::from([
///         vertex("post", "record"),
///         vertex(&format!("post.{name}"), "string"),
///     ]),
///     edges: HashMap::from([(column("post", name), "prop".into())]),
///     ..Schema::default()
/// };
/// let protocol = Protocol {
///     value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
///     non_value_kinds: BTreeSet::from(["record".to_owned()]),
///     ..Protocol::default()
/// };
/// let mut rename = CompiledMigration::default();
/// rename.vertex_remap.insert("post".into(), "post".into());
/// rename.vertex_remap.insert("post.title".into(), "post.name".into());
/// rename.edge_remap.insert(column("post", "title"), column("post", "name"));
/// let f =
///     MigrationFunctor::new(&schema("title"), &protocol, &schema("name"), &protocol, &rename)?;
/// assert_eq!(f.column_image("post", "title"), Some("name"));
/// f.attribute_surjective()?;
/// # Ok::<(), panproto_inst::RestrictError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationFunctor {
    /// The maps on tables, relations and columns.
    maps: Maps,
    /// Each `edge_remap` entry whose key is an inner value edge of the
    /// source, with its image: an inner value edge of the target of the same
    /// kind and name. No table function reads it; the graph Kan extensions
    /// move the inner edges of a cell's layout along it.
    inner_edges: BTreeMap<Edge, Edge>,
    /// Each `vertex_remap` entry whose key is a value vertex of the source.
    /// No table function reads it; the graph Kan extensions move the inner
    /// nodes of a cell's layout along it.
    value_vertices: BTreeMap<Name, Name>,
}

/// Build the maps of the functor `migration` states between `source` and
/// `target`, and the remaps of inner value edges: steps 1 to 4a.
fn entity_maps(
    source: RelSchema,
    target: RelSchema,
    migration: &CompiledMigration,
) -> Result<EntityFunctor, RestrictError> {
    let vertex_images: BTreeSet<&Name> = migration.vertex_remap.values().collect();
    let edge_images: BTreeSet<&Edge> = migration.edge_remap.values().collect();
    let survives_vertex =
        |v: &Name| migration.surviving_verts.contains(v) && !vertex_images.contains(v);
    let survives_edge =
        |e: &Edge| migration.surviving_edges.contains(e) && !edge_images.contains(e);

    // Step 1: tables.
    let mut tables = BTreeMap::new();
    for table in source.tables() {
        let image = match migration.vertex_remap.get(table) {
            Some(image) => image,
            None if survives_vertex(table) => table,
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

    // Step 2: relations between kept tables.
    let mut relations = BTreeMap::new();
    for edge in source.relations() {
        let (Some(src), Some(tgt)) = (tables.get(&edge.src), tables.get(&edge.tgt)) else {
            continue;
        };
        let image = match migration.edge_remap.get(edge) {
            Some(image) => image.clone(),
            None if survives_edge(edge) => Edge {
                src: src.clone(),
                tgt: tgt.clone(),
                kind: edge.kind.clone(),
                name: edge.name.clone(),
            },
            None => continue,
        };
        if &image.src != src || &image.tgt != tgt || !target.is_relation(&image) {
            return Err(RestrictError::IncoherentEdgeImage {
                source_edge: Box::new(edge.clone()),
                image: Box::new(image),
            });
        }
        relations.insert(edge.clone(), image);
    }

    // Step 3: the columns of kept tables.
    let columns = column_maps(&source, &target, migration, &tables, &survives_edge)?;

    // Step 4a: no remap crosses the divide between entities and values, and
    // the remaps of inner value edges are recorded.
    let mut entries: Vec<(&Edge, &Edge)> = migration.edge_remap.iter().collect();
    entries.sort();
    let mut inner_edges = BTreeMap::new();
    for (edge, image) in entries {
        if source.is_relation(edge) && target.attribute_column(image).is_some() {
            return Err(RestrictError::IncoherentEdgeImage {
                source_edge: Box::new(edge.clone()),
                image: Box::new(image.clone()),
            });
        }
        if source.attribute_column(edge).is_some() && target.is_relation(image) {
            return Err(RestrictError::IncoherentAttributeImage {
                edge: Box::new(edge.clone()),
                image: Some(Box::new(image.clone())),
            });
        }
        if source.is_inner_edge(edge) {
            inner_edges.insert(edge.clone(), image.clone());
        }
    }
    let value_remap = migration
        .vertex_remap
        .iter()
        .filter(|(vertex, _)| source.value_kind(vertex).is_some())
        .map(|(vertex, image)| (vertex.clone(), image.clone()))
        .collect();
    Ok(EntityFunctor {
        maps: Maps {
            source,
            target,
            tables,
            relations,
            columns,
        },
        inner_edges,
        value_remap,
    })
}

/// Step 3 of [`entity_maps`]: the image column of each kept column of each
/// kept table, or the least column whose image is incoherent.
fn column_maps(
    source: &RelSchema,
    target: &RelSchema,
    migration: &CompiledMigration,
    tables: &BTreeMap<Name, Name>,
    survives_edge: &dyn Fn(&Edge) -> bool,
) -> Result<BTreeMap<(Name, String), String>, RestrictError> {
    let mut columns = BTreeMap::new();
    for (table, image_table) in tables {
        for (column, arrow) in source.columns(table) {
            let value_image = migration.vertex_remap.get(&arrow.tgt);
            let image = if let Some(image) = migration.edge_remap.get(arrow) {
                let coherent = target.attribute_column(image).is_some()
                    && image.src == *image_table
                    && value_image.is_none_or(|v| image.tgt == *v);
                if !coherent {
                    return Err(RestrictError::IncoherentAttributeImage {
                        edge: Box::new(arrow.clone()),
                        image: Some(Box::new(image.clone())),
                    });
                }
                image.clone()
            } else if survives_edge(arrow) {
                let image = Edge {
                    src: image_table.clone(),
                    tgt: value_image.unwrap_or(&arrow.tgt).clone(),
                    kind: arrow.kind.clone(),
                    name: arrow.name.clone(),
                };
                if target.attribute_column(&image).is_none() {
                    return Err(RestrictError::IncoherentAttributeImage {
                        edge: Box::new(arrow.clone()),
                        image: None,
                    });
                }
                image
            } else {
                continue;
            };
            let image_column = target.attribute_column(&image).unwrap_or_default();
            columns.insert((table.clone(), column.clone()), image_column.to_owned());
        }
    }
    Ok(columns)
}

impl EntityFunctor {
    /// The entity functor `migration` states between relation-ified schemas
    /// built already.
    ///
    /// # Errors
    ///
    /// As [`MigrationFunctor::entity_part`], without its classification
    /// step.
    pub fn between(
        source: RelSchema,
        target: RelSchema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        entity_maps(source, target, migration)
    }

    /// The functor the Kan extensions take: this one, once no remap of an
    /// edge inside a column's value changes that value's structure.
    ///
    /// A remap of an inner value edge `e` to `e'` is accepted when `e'` is an
    /// inner value edge of the target with the name and kind of `e`, and the
    /// kinds of its endpoints are those of the images of `e`'s endpoints
    /// under `vertex_remap` (an endpoint the migration does not remap keeps
    /// its own kind). Such a remap only renames vertices, which a value does
    /// not record, so every cell keeps its structure.
    ///
    /// The functor keeps the remaps of inner value edges and of value
    /// vertices unchanged, as [`MigrationFunctor::inner_edges`] and
    /// [`MigrationFunctor::value_vertices`].
    ///
    /// # Errors
    ///
    /// [`RestrictError::ValueStructureRemap`] for the least inner value edge
    /// whose remap is not accepted.
    pub fn into_kan(self) -> Result<MigrationFunctor, RestrictError> {
        let (source, target) = (&self.maps.source, &self.maps.target);
        let expected = |vertex: &Name| {
            self.value_remap.get(vertex).map_or_else(
                || source.value_kind(vertex),
                |image| target.value_kind(image),
            )
        };
        for (edge, image) in &self.inner_edges {
            let keeps_structure = target.is_inner_edge(image)
                && image.name == edge.name
                && image.kind == edge.kind
                && target.value_kind(&image.src).is_some()
                && target.value_kind(&image.src) == expected(&edge.src)
                && target.value_kind(&image.tgt).is_some()
                && target.value_kind(&image.tgt) == expected(&edge.tgt);
            if !keeps_structure {
                return Err(RestrictError::ValueStructureRemap { edge: edge.clone() });
            }
        }
        Ok(MigrationFunctor {
            maps: self.maps,
            inner_edges: self.inner_edges,
            value_vertices: self.value_remap,
        })
    }

    /// The relation-ified source schema `S`.
    #[must_use]
    pub const fn source(&self) -> &RelSchema {
        &self.maps.source
    }

    /// The relation-ified target schema `T`.
    #[must_use]
    pub const fn target(&self) -> &RelSchema {
        &self.maps.target
    }

    /// The tables of `S'` with their images, ascending.
    #[must_use]
    pub const fn table_map(&self) -> &BTreeMap<Name, Name> {
        &self.maps.tables
    }

    /// The relations of `S'` with their images, ascending.
    #[must_use]
    pub const fn relation_map(&self) -> &BTreeMap<Edge, Edge> {
        &self.maps.relations
    }

    /// The kept columns `(source table, column)` with their image columns,
    /// ascending.
    #[must_use]
    pub const fn column_map(&self) -> &BTreeMap<(Name, String), String> {
        &self.maps.columns
    }

    /// The migration's remaps of inner value edges of the source, ascending.
    #[must_use]
    pub const fn inner_edges(&self) -> &BTreeMap<Edge, Edge> {
        &self.inner_edges
    }
}

impl MigrationFunctor {
    /// The entity functor `migration` states from `src` (under
    /// `src_protocol`) to `tgt` (under `tgt_protocol`): every check of the
    /// functor but the one on value remaps.
    ///
    /// # Errors
    ///
    /// Each step reports its least offending element, and the steps run in
    /// this order: [`RestrictError::RelSchema`] when either schema has no
    /// relation-ified form (the source first);
    /// [`RestrictError::VertexImageNotATable`] when a kept table's image is
    /// no table of the target; [`RestrictError::IncoherentEdgeImage`] when a
    /// kept relation's image is no relation of the target between the
    /// images of its endpoints; [`RestrictError::IncoherentAttributeImage`]
    /// when a kept column's image is no column of the image of its table
    /// (ending at the image of its vertex, when `vertex_remap` maps that);
    /// and then, over every `edge_remap` entry, `IncoherentEdgeImage` for a
    /// relation sent to an attribute arrow and `IncoherentAttributeImage` for
    /// an attribute arrow sent to a relation.
    pub fn entity_part(
        src: &Schema,
        src_protocol: &Protocol,
        tgt: &Schema,
        tgt_protocol: &Protocol,
        migration: &CompiledMigration,
    ) -> Result<EntityFunctor, RestrictError> {
        let source = RelSchema::new(src, src_protocol)?;
        let target = RelSchema::new(tgt, tgt_protocol)?;
        EntityFunctor::between(source, target, migration)
    }

    /// The functor `migration` states from `src` to `tgt`:
    /// [`Self::entity_part`] followed by [`EntityFunctor::into_kan`].
    ///
    /// # Errors
    ///
    /// As those two, in that order.
    pub fn new(
        src: &Schema,
        src_protocol: &Protocol,
        tgt: &Schema,
        tgt_protocol: &Protocol,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        Self::entity_part(src, src_protocol, tgt, tgt_protocol, migration)?.into_kan()
    }

    /// [`Self::new`] over relation-ified schemas built already.
    ///
    /// # Errors
    ///
    /// As [`EntityFunctor::between`] and [`EntityFunctor::into_kan`].
    pub fn between(
        source: RelSchema,
        target: RelSchema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        EntityFunctor::between(source, target, migration)?.into_kan()
    }

    /// The identity functor on the relation-ified schema of `schema`.
    ///
    /// # Errors
    ///
    /// As [`RelSchema::new`].
    pub fn identity(schema: &Schema, protocol: &Protocol) -> Result<Self, RelSchemaError> {
        let rel = RelSchema::new(schema, protocol)?;
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
        let columns = rel
            .all_columns()
            .iter()
            .flat_map(|(table, columns)| {
                columns
                    .keys()
                    .map(move |column| ((table.clone(), column.clone()), column.clone()))
            })
            .collect();
        Ok(Self {
            maps: Maps {
                source: rel.clone(),
                target: rel,
                tables,
                relations,
                columns,
            },
            inner_edges: BTreeMap::new(),
            value_vertices: BTreeMap::new(),
        })
    }

    /// The relation-ified source schema `S`.
    #[must_use]
    pub const fn source(&self) -> &RelSchema {
        &self.maps.source
    }

    /// The relation-ified target schema `T`.
    #[must_use]
    pub const fn target(&self) -> &RelSchema {
        &self.maps.target
    }

    /// The tables of `S'` with their images, ascending.
    #[must_use]
    pub const fn table_map(&self) -> &BTreeMap<Name, Name> {
        &self.maps.tables
    }

    /// The relations of `S'` with their images, ascending.
    #[must_use]
    pub const fn relation_map(&self) -> &BTreeMap<Edge, Edge> {
        &self.maps.relations
    }

    /// The kept columns `(source table, column)` with their image columns,
    /// ascending.
    #[must_use]
    pub const fn column_map(&self) -> &BTreeMap<(Name, String), String> {
        &self.maps.columns
    }

    /// The migration's remaps of inner value edges of the source: each
    /// `edge_remap` entry whose key is an edge inside a column's value, with
    /// its image, an inner value edge of the target of the same kind and
    /// name. Empty for [`Self::identity`]. No table function reads it.
    ///
    /// ```
    /// use std::collections::{BTreeMap, BTreeSet, HashMap};
    ///
    /// use panproto_inst::{CompiledMigration, MigrationFunctor};
    /// use panproto_schema::{Edge, Protocol, ScalarKind, Schema, Vertex};
    ///
    /// let vertex = |id: &str, kind: &str| {
    ///     (id.into(), Vertex { id: id.into(), kind: kind.into(), nsid: None })
    /// };
    /// let tags = Edge {
    ///     src: "post".into(),
    ///     tgt: "post.tags".into(),
    ///     kind: "prop".into(),
    ///     name: Some("tags".into()),
    /// };
    /// let element = |vertex: &str| Edge {
    ///     src: "post.tags".into(),
    ///     tgt: vertex.into(),
    ///     kind: "items".into(),
    ///     name: Some("item".into()),
    /// };
    /// let schema = |element_vertex: &str| Schema {
    ///     vertices: HashMap::from([
    ///         vertex("post", "record"),
    ///         vertex("post.tags", "list"),
    ///         vertex(element_vertex, "string"),
    ///     ]),
    ///     edges: HashMap::from([
    ///         (tags.clone(), "prop".into()),
    ///         (element(element_vertex), "items".into()),
    ///     ]),
    ///     ..Schema::default()
    /// };
    /// let protocol = Protocol {
    ///     value_kinds: BTreeMap::from([
    ///         ("list".to_owned(), ScalarKind::Any),
    ///         ("string".to_owned(), ScalarKind::Str),
    ///     ]),
    ///     non_value_kinds: BTreeSet::from(["record".to_owned()]),
    ///     ..Protocol::default()
    /// };
    /// // Rename the element vertex of the list column `tags`.
    /// let mut rename = CompiledMigration::default();
    /// rename.vertex_remap.insert("post".into(), "post".into());
    /// rename.vertex_remap.insert("post.tags".into(), "post.tags".into());
    /// rename.vertex_remap.insert("post.tags.tag".into(), "post.tags.label".into());
    /// rename.edge_remap.insert(tags.clone(), tags.clone());
    /// rename.edge_remap.insert(element("post.tags.tag"), element("post.tags.label"));
    /// let source = schema("post.tags.tag");
    /// let target = schema("post.tags.label");
    /// let f = MigrationFunctor::new(&source, &protocol, &target, &protocol, &rename)?;
    /// assert_eq!(
    ///     f.inner_edges(),
    ///     &BTreeMap::from([(element("post.tags.tag"), element("post.tags.label"))])
    /// );
    /// assert_eq!(f.value_vertices().len(), 2);
    /// assert_eq!(f.value_vertices()["post.tags.tag"], "post.tags.label");
    /// let identity = MigrationFunctor::identity(&source, &protocol)?;
    /// assert!(identity.inner_edges().is_empty() && identity.value_vertices().is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub const fn inner_edges(&self) -> &BTreeMap<Edge, Edge> {
        &self.inner_edges
    }

    /// The migration's remaps of value vertices of the source: each
    /// `vertex_remap` entry whose key is a value vertex of the source. A
    /// value vertex without an entry stands for its namesake. Empty for
    /// [`Self::identity`]. No table function reads it; see
    /// [`Self::inner_edges`] for an example.
    #[must_use]
    pub const fn value_vertices(&self) -> &BTreeMap<Name, Name> {
        &self.value_vertices
    }

    /// The image of a table of `S'`, or `None` outside `S'`.
    #[must_use]
    pub fn table_image(&self, table: &str) -> Option<&Name> {
        self.maps.tables.get(table)
    }

    /// The image of a relation of `S'`, or `None` outside `S'`.
    #[must_use]
    pub fn relation_image(&self, edge: &Edge) -> Option<&Edge> {
        self.maps.relations.get(edge)
    }

    /// The image column of a kept column, or `None` outside `S'`.
    #[must_use]
    pub fn column_image(&self, table: &str, column: &str) -> Option<&str> {
        self.maps
            .columns
            .get(&(Name::from(table), column.to_owned()))
            .map(String::as_str)
    }

    /// The kept columns of a table of `S'`, ascending.
    #[must_use]
    pub fn kept_columns(&self, table: &str) -> Vec<&String> {
        self.maps
            .columns
            .keys()
            .filter(|(t, _)| t.as_ref() == table)
            .map(|(_, column)| column)
            .collect()
    }

    /// The kept columns of `table` sent to `column`, ascending.
    #[must_use]
    pub fn column_preimage(&self, table: &str, column: &str) -> Vec<&String> {
        self.maps
            .columns
            .iter()
            .filter(|((t, _), image)| t.as_ref() == table && image.as_str() == column)
            .map(|((_, c), _)| c)
            .collect()
    }

    /// The fibre of a target table: the tables of `S'` sent to it, ascending.
    #[must_use]
    pub fn table_fibre(&self, table: &str) -> Vec<&Name> {
        self.maps
            .tables
            .iter()
            .filter(|(_, image)| image.as_ref() == table)
            .map(|(source, _)| source)
            .collect()
    }

    /// The fibre of a target relation: the relations of `S'` sent to it,
    /// ascending.
    #[must_use]
    pub fn relation_fibre(&self, edge: &Edge) -> Vec<&Edge> {
        self.maps
            .relations
            .iter()
            .filter(|(_, image)| *image == edge)
            .map(|(source, _)| source)
            .collect()
    }

    /// The fibres of every target table, including the empty fibres of the
    /// tables outside the image, by target table ascending.
    pub(crate) fn table_fibres(&self) -> BTreeMap<&Name, Vec<&Name>> {
        let mut fibres: BTreeMap<&Name, Vec<&Name>> = self
            .maps
            .target
            .tables()
            .iter()
            .map(|t| (t, Vec::new()))
            .collect();
        for (source, image) in &self.maps.tables {
            fibres.entry(image).or_default().push(source);
        }
        fibres
    }

    /// The fibres of every target relation, including the empty ones, by
    /// target relation ascending.
    pub(crate) fn relation_fibres(&self) -> BTreeMap<&Edge, Vec<&Edge>> {
        let mut fibres: BTreeMap<&Edge, Vec<&Edge>> = self
            .maps
            .target
            .relations()
            .iter()
            .map(|e| (e, Vec::new()))
            .collect();
        for (source, image) in &self.maps.relations {
            fibres.entry(image).or_default().push(source);
        }
        fibres
    }

    /// The tables of `T` outside the image of `F`, ascending.
    #[must_use]
    pub fn tables_outside_image(&self) -> BTreeSet<&Name> {
        let image: BTreeSet<&Name> = self.maps.tables.values().collect();
        self.maps
            .target
            .tables()
            .iter()
            .filter(|t| !image.contains(t))
            .collect()
    }

    /// Whether `F` is surjective on attributes: every column of every target
    /// table is the image of a kept column of some table of its fibre. A
    /// target table outside the image passes only when it has no columns.
    /// Along such a functor the right Kan extension exists; along any other
    /// it does not, since a column outside the image would have to hold
    /// every value at once.
    ///
    /// # Errors
    ///
    /// [`RestrictError::AttributeOutsideImage`] for the least target table,
    /// and column, that is the image of no kept column.
    pub fn attribute_surjective(&self) -> Result<(), RestrictError> {
        let hit: BTreeSet<(&Name, &str)> = self
            .maps
            .columns
            .iter()
            .filter_map(|((table, _), image)| Some((self.maps.tables.get(table)?, image.as_str())))
            .collect();
        for (table, columns) in self.maps.target.all_columns() {
            if let Some(column) = columns
                .keys()
                .find(|column| !hit.contains(&(table, column.as_str())))
            {
                return Err(RestrictError::AttributeOutsideImage {
                    table: table.clone(),
                    column: column.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use panproto_schema::{ScalarKind, Vertex};

    use super::*;
    use crate::functor::{FInstance, Row};
    use crate::view_ref::RowId;

    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    /// The attribute arrow of column `column` of `table`, to the vertex
    /// `table.column`.
    fn column(table: &str, column: &str) -> Edge {
        Edge {
            src: table.into(),
            tgt: format!("{table}.{column}").into(),
            kind: "prop".into(),
            name: Some(column.into()),
        }
    }

    /// A schema of tables (kind `record`) with integer columns, and the given
    /// relations and further edges.
    fn schema(tables: &[(&str, &[&str])], edges: &[Edge]) -> Schema {
        let mut vertices = HashMap::new();
        let mut all = HashMap::new();
        for (table, columns) in tables {
            vertices.insert(
                Name::from(*table),
                Vertex {
                    id: (*table).into(),
                    kind: "record".into(),
                    nsid: None,
                },
            );
            for c in *columns {
                let arrow = column(table, c);
                vertices.insert(
                    arrow.tgt.clone(),
                    Vertex {
                        id: arrow.tgt.clone(),
                        kind: "integer".into(),
                        nsid: None,
                    },
                );
                all.insert(arrow, Name::from("prop"));
            }
        }
        for e in edges {
            all.insert(e.clone(), e.kind.clone());
        }
        Schema {
            vertices,
            edges: all,
            ..Schema::default()
        }
    }

    /// The protocol of [`schema`]: `record` tables, `integer`, `list` and
    /// `string` values.
    fn protocol() -> Protocol {
        Protocol {
            value_kinds: BTreeMap::from([
                (
                    "integer".to_owned(),
                    ScalarKind::Int {
                        bits: 64,
                        signed: true,
                    },
                ),
                ("list".to_owned(), ScalarKind::Any),
                ("string".to_owned(), ScalarKind::Str),
            ]),
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            ..Protocol::default()
        }
    }

    fn functor(
        src: &Schema,
        tgt: &Schema,
        m: &CompiledMigration,
    ) -> Result<MigrationFunctor, RestrictError> {
        MigrationFunctor::new(src, &protocol(), tgt, &protocol(), m)
    }

    fn row(cells: &[(&str, i64)]) -> Row {
        cells
            .iter()
            .map(|(c, v)| ((*c).to_owned(), crate::Value::Int(*v)))
            .collect()
    }

    /// `panproto_mig::compile` lists the surviving image edges, which are
    /// target edges; one that the source does not have is not read as a
    /// source edge, so the functor's domain holds no relation and Delta
    /// copies none of its pairs back.
    #[test]
    fn a_target_only_edge_is_not_a_source_edge() -> Result<(), Box<dyn std::error::Error>> {
        let e = fk("a", "b", "e");
        let src = schema(&[("a", &[]), ("b", &[])], &[]);
        let tgt = schema(&[("a", &[]), ("b", &[])], std::slice::from_ref(&e));
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        m.surviving_edges.insert(e.clone());
        let f = functor(&src, &tgt, &m)?;
        assert!(f.relation_map().is_empty());
        assert_eq!(f.relation_fibre(&e), Vec::<&Edge>::new());
        let y = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new())])
            .and_then(|i| i.with_rows("b", [(RowId(1), Row::new())]))
            .and_then(|i| i.with_foreign_key(e.clone(), [(RowId(0), RowId(1))]))?;
        let delta = crate::f_delta(&y, &f)?;
        assert_eq!(delta.pair_count(), 0, "{delta:?}");
        assert_eq!(delta.total_row_count(), 2);
        Ok(())
    }

    #[test]
    fn an_incoherent_edge_image_is_refused() {
        let e = fk("a", "b", "e");
        let src = schema(
            &[("a", &[]), ("b", &[]), ("c", &[])],
            std::slice::from_ref(&e),
        );
        let tgt = schema(
            &[("a", &[]), ("b", &[]), ("c", &[])],
            &[fk("a", "c", "E"), fk("a", "b", "E")],
        );
        let mut m = CompiledMigration::default();
        m.surviving_verts
            .extend(["a".into(), "b".into(), "c".into()]);
        m.edge_remap.insert(e.clone(), fk("a", "c", "E"));
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::IncoherentEdgeImage { source_edge: edge, image })
                if *edge == e && *image == fk("a", "c", "E")
        ));
        // An image between the right tables that the target lacks.
        m.edge_remap.insert(e.clone(), fk("a", "b", "missing"));
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::IncoherentEdgeImage { .. })
        ));
        m.edge_remap.insert(e, fk("a", "b", "E"));
        assert!(functor(&src, &tgt, &m).is_ok());
        // A table sent to a vertex that is no table of the target; tables
        // are checked before relations.
        m.vertex_remap.insert("c".into(), "nowhere".into());
        m.edge_remap.insert(fk("a", "b", "e"), fk("a", "c", "E"));
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::VertexImageNotATable { vertex, image })
                if vertex == "c" && image == "nowhere"
        ));
    }

    #[test]
    fn unmentioned_edges_are_outside_the_domain() -> Result<(), RestrictError> {
        let kept = fk("a", "b", "kept");
        let unmentioned = fk("b", "a", "back");
        let to_dropped = fk("a", "gone", "dangling");
        let src = schema(
            &[("a", &["x"]), ("b", &[]), ("gone", &[])],
            &[kept.clone(), unmentioned.clone(), to_dropped.clone()],
        );
        let tgt = schema(&[("m", &[])], &[fk("m", "m", "K")]);
        let mut m = CompiledMigration::default();
        m.vertex_remap.insert("a".into(), "m".into());
        m.vertex_remap.insert("b".into(), "m".into());
        m.surviving_verts.insert("m".into());
        m.edge_remap.insert(kept.clone(), fk("m", "m", "K"));
        // Carried, but its target table is outside the domain.
        m.edge_remap.insert(to_dropped.clone(), fk("m", "m", "K"));
        let f = functor(&src, &tgt, &m)?;
        assert_eq!(f.relation_map().keys().collect::<Vec<_>>(), [&kept]);
        assert!(f.relation_image(&unmentioned).is_none());
        assert!(f.relation_image(&to_dropped).is_none());
        assert_eq!(f.table_fibre("m"), [&Name::from("a"), &Name::from("b")]);
        assert!(f.table_image("gone").is_none());
        // The unmentioned column `x` is dropped, not an error.
        assert!(f.column_map().is_empty());
        Ok(())
    }

    /// `panproto_mig::compile` lists the images of the vertex map as
    /// surviving vertices. A source table that the vertex map leaves out but
    /// whose name is the image of another table is dropped, not kept.
    #[test]
    fn a_surviving_image_is_not_a_source_table() -> Result<(), RestrictError> {
        let src = schema(&[("a", &[]), ("b", &[])], &[]);
        let tgt = schema(&[("b", &[])], &[]);
        let mut m = CompiledMigration::default();
        m.vertex_remap.insert("a".into(), "b".into());
        m.surviving_verts.insert("b".into());
        let f = functor(&src, &tgt, &m)?;
        assert_eq!(f.table_map().keys().collect::<Vec<_>>(), [&Name::from("a")]);
        m.vertex_remap.insert("b".into(), "b".into());
        let f = functor(&src, &tgt, &m)?;
        assert_eq!(f.table_fibre("b"), [&Name::from("a"), &Name::from("b")]);
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        let f = functor(&src, &schema(&[("a", &[]), ("b", &[])], &[]), &m)?;
        assert_eq!(f.table_map().len(), 2);
        Ok(())
    }

    /// The edge twin: a source relation that the edge map leaves out but
    /// that is the image of another relation is a listed target edge, so it
    /// is dropped, and Pi does not intersect with its pairs.
    #[test]
    fn a_surviving_image_is_not_a_source_relation() -> Result<(), Box<dyn std::error::Error>> {
        let (carried, listed) = (fk("a", "b", "p"), fk("a", "b", "q"));
        let src = schema(
            &[("a", &[]), ("b", &[])],
            &[carried.clone(), listed.clone()],
        );
        let tgt = schema(&[("a", &[]), ("b", &[])], std::slice::from_ref(&listed));
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.extend(["a".into(), "b".into()]);
        migration.edge_remap.insert(carried.clone(), listed.clone());
        migration.surviving_edges.insert(listed.clone());
        let f = functor(&src, &tgt, &migration)?;
        assert_eq!(f.relation_map().keys().collect::<Vec<_>>(), [&carried]);
        let inst = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new())])
            .and_then(|i| i.with_rows("b", [(RowId(1), Row::new())]))
            .and_then(|i| i.with_foreign_key(carried.clone(), [(RowId(0), RowId(1))]))?;
        let pi = crate::functor_pi_along(&inst, &f, 1000)?;
        assert_eq!(pi.pairs(&listed).len(), 1, "{pi:?}");
        migration.edge_remap.insert(listed.clone(), listed.clone());
        let f = functor(&src, &tgt, &migration)?;
        assert_eq!(f.relation_fibre(&listed), [&carried, &listed]);
        Ok(())
    }

    /// A column remapped by `edge_remap` goes to the column of its image; a
    /// surviving column to the target column of its kind and name.
    #[test]
    fn a_column_rename_is_carried() -> Result<(), RestrictError> {
        let src = schema(&[("post", &["title", "body"])], &[]);
        let tgt = schema(&[("post", &["name", "body"])], &[]);
        let mut m = CompiledMigration::default();
        m.vertex_remap.insert("post".into(), "post".into());
        m.vertex_remap
            .insert("post.title".into(), "post.name".into());
        m.edge_remap
            .insert(column("post", "title"), column("post", "name"));
        m.surviving_edges.insert(column("post", "body"));
        let f = functor(&src, &tgt, &m)?;
        assert_eq!(f.column_image("post", "title"), Some("name"));
        assert_eq!(f.column_image("post", "body"), Some("body"));
        assert_eq!(f.column_preimage("post", "name"), [&"title".to_owned()]);
        f.attribute_surjective()?;
        Ok(())
    }

    #[test]
    fn an_incoherent_attribute_image_is_refused() {
        let src = schema(&[("a", &["x"]), ("b", &["y"])], &[]);
        let tgt = schema(&[("a", &["x", "z"]), ("b", &["y"])], &[]);
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        // An image leaving another table than the image of the column's.
        m.edge_remap.insert(column("a", "x"), column("b", "y"));
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::IncoherentAttributeImage { edge, image: Some(image) })
                if *edge == column("a", "x") && *image == column("b", "y")
        ));
        // An image ending elsewhere than the image of the column's vertex.
        m.edge_remap.insert(column("a", "x"), column("a", "z"));
        m.vertex_remap.insert("a.x".into(), "a.x".into());
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::IncoherentAttributeImage { image: Some(_), .. })
        ));
        // A surviving column with no column of its kind and name.
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        m.surviving_edges.insert(column("b", "y"));
        let tgt = schema(&[("a", &["x"]), ("b", &["w"])], &[]);
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::IncoherentAttributeImage { edge, image: None })
                if *edge == column("b", "y")
        ));
    }

    /// No remap sends a relation to an attribute arrow or an attribute arrow
    /// to a relation, whether or not its tables are kept.
    #[test]
    fn a_remap_across_the_entity_value_divide_is_refused() {
        let e = fk("a", "b", "e");
        let src = schema(
            &[("a", &["x"]), ("b", &[]), ("c", &[])],
            std::slice::from_ref(&e),
        );
        let tgt = schema(&[("a", &["x"]), ("b", &[])], &[fk("a", "b", "E")]);
        // `c` is not kept, so its relation is outside the domain, but its
        // remap still crosses the divide.
        let src2 = schema(
            &[("a", &["x"]), ("b", &[]), ("c", &[])],
            &[fk("c", "b", "f")],
        );
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        m.edge_remap.insert(fk("c", "b", "f"), column("a", "x"));
        assert!(matches!(
            functor(&src2, &tgt, &m),
            Err(RestrictError::IncoherentEdgeImage { source_edge: edge, image })
                if *edge == fk("c", "b", "f") && *image == column("a", "x")
        ));
        // A kept relation sent to a column.
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        m.edge_remap.insert(e, column("a", "x"));
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::IncoherentEdgeImage { .. })
        ));
        // A column sent to a relation.
        let mut m = CompiledMigration::default();
        m.surviving_verts.extend(["a".into(), "b".into()]);
        m.edge_remap.insert(column("a", "x"), fk("a", "b", "E"));
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::IncoherentAttributeImage { image: Some(image), .. })
                if *image == fk("a", "b", "E")
        ));
    }

    /// A table `name` with one list column `tags` whose element vertex is
    /// reached by an inner value edge `edge`, of the given element kind.
    fn with_list_column(name: &str, element_edge: &str, element_kind: &str) -> Schema {
        let mut s = schema(&[(name, &[])], &[]);
        let list: Name = format!("{name}.tags").into();
        let items: Name = format!("{name}.tags.items").into();
        s.vertices.insert(
            list.clone(),
            Vertex {
                id: list.clone(),
                kind: "list".into(),
                nsid: None,
            },
        );
        s.vertices.insert(
            items.clone(),
            Vertex {
                id: items,
                kind: element_kind.into(),
                nsid: None,
            },
        );
        s.edges.insert(
            Edge {
                src: name.into(),
                tgt: list,
                kind: "prop".into(),
                name: Some("tags".into()),
            },
            "prop".into(),
        );
        s.edges.insert(inner(name, element_edge), "items".into());
        s
    }

    fn inner(table: &str, name: &str) -> Edge {
        Edge {
            src: format!("{table}.tags").into(),
            tgt: format!("{table}.tags.items").into(),
            kind: "items".into(),
            name: Some(name.into()),
        }
    }

    /// The rename `posts -> articles` of a table with a list column.
    fn rename_posts(element_edge: &str) -> CompiledMigration {
        let mut m = CompiledMigration::default();
        m.vertex_remap.insert("posts".into(), "articles".into());
        m.vertex_remap
            .insert("posts.tags".into(), "articles.tags".into());
        m.vertex_remap
            .insert("posts.tags.items".into(), "articles.tags.items".into());
        m.edge_remap
            .insert(column("posts", "tags"), column("articles", "tags"));
        m.edge_remap
            .insert(inner("posts", "item"), inner("articles", element_edge));
        m
    }

    /// Renaming a table renames the ids of its list column's vertices too;
    /// the cells (a list of strings) keep their structure, so the functor is
    /// accepted.
    #[test]
    fn renaming_a_table_with_a_list_column_is_accepted() -> Result<(), RestrictError> {
        let src = with_list_column("posts", "item", "string");
        let tgt = with_list_column("articles", "item", "string");
        let f = functor(&src, &tgt, &rename_posts("item"))?;
        assert_eq!(f.column_image("posts", "tags"), Some("tags"));
        f.attribute_surjective()?;
        Ok(())
    }

    #[test]
    fn entity_part_accepts_an_inner_value_edge_rename() -> Result<(), RestrictError> {
        let src = with_list_column("posts", "item", "string");
        let tgt = with_list_column("articles", "element", "string");
        let entity = MigrationFunctor::entity_part(
            &src,
            &protocol(),
            &tgt,
            &protocol(),
            &rename_posts("element"),
        )?;
        assert_eq!(
            entity.inner_edges(),
            &BTreeMap::from([(inner("posts", "item"), inner("articles", "element"))])
        );
        assert_eq!(entity.column_map().len(), 1);
        Ok(())
    }

    #[test]
    fn into_kan_refuses_what_the_entity_part_accepts() -> Result<(), RestrictError> {
        let src = with_list_column("posts", "item", "string");
        let tgt = with_list_column("articles", "element", "string");
        let entity = MigrationFunctor::entity_part(
            &src,
            &protocol(),
            &tgt,
            &protocol(),
            &rename_posts("element"),
        )?;
        assert!(matches!(
            entity.into_kan(),
            Err(RestrictError::ValueStructureRemap { edge }) if edge == inner("posts", "item")
        ));
        Ok(())
    }

    /// `into_kan` moves the remaps of inner value edges and of value vertices
    /// into the functor unchanged, the identity holds none, and the refusals
    /// of a remap that changes a value's structure are those of the entity
    /// part's `into_kan`.
    #[test]
    fn migration_functor_keeps_the_value_remaps_of_its_migration()
    -> Result<(), Box<dyn std::error::Error>> {
        let src = with_list_column("posts", "item", "string");
        let tgt = with_list_column("articles", "item", "string");
        let m = rename_posts("item");
        let entity = MigrationFunctor::entity_part(&src, &protocol(), &tgt, &protocol(), &m)?;
        let recorded = entity.inner_edges().clone();
        let f = entity.into_kan()?;
        assert_eq!(f.inner_edges(), &recorded);
        assert_eq!(
            f.inner_edges(),
            &BTreeMap::from([(inner("posts", "item"), inner("articles", "item"))])
        );
        // The table `posts` is not a value vertex, so its remap is not kept.
        assert_eq!(
            f.value_vertices(),
            &BTreeMap::from([
                (Name::from("posts.tags"), Name::from("articles.tags")),
                (
                    Name::from("posts.tags.items"),
                    Name::from("articles.tags.items")
                ),
            ])
        );
        assert_eq!(f, functor(&src, &tgt, &m)?);
        let between = MigrationFunctor::between(
            RelSchema::new(&src, &protocol())?,
            RelSchema::new(&tgt, &protocol())?,
            &m,
        )?;
        assert_eq!(between, f);
        // A value vertex the migration leaves alone has no entry.
        let mut unmapped = m;
        unmapped.vertex_remap.remove("posts.tags.items");
        let f = functor(&src, &tgt, &unmapped)?;
        assert_eq!(f.inner_edges().len(), 1);
        assert_eq!(
            f.value_vertices(),
            &BTreeMap::from([(Name::from("posts.tags"), Name::from("articles.tags"))])
        );
        // The identity keeps no remap.
        let identity = MigrationFunctor::identity(&src, &protocol())?;
        assert!(identity.inner_edges().is_empty());
        assert!(identity.value_vertices().is_empty());
        // A remap that renames an inner value edge is still refused, by
        // `new` as by `into_kan`.
        let renamed = with_list_column("articles", "element", "string");
        let refusal = |result: Result<MigrationFunctor, RestrictError>| {
            matches!(
                result,
                Err(RestrictError::ValueStructureRemap { edge }) if edge == inner("posts", "item")
            )
        };
        assert!(refusal(functor(&src, &renamed, &rename_posts("element"))));
        assert!(refusal(
            MigrationFunctor::entity_part(
                &src,
                &protocol(),
                &renamed,
                &protocol(),
                &rename_posts("element"),
            )?
            .into_kan()
        ));
        Ok(())
    }

    /// An inner value edge's remap that renames it, changes its kind, changes
    /// an endpoint's kind, or lands on no inner value edge is refused by the
    /// Kan functor.
    #[test]
    fn an_inner_value_edge_remap_is_refused() {
        let src = with_list_column("posts", "item", "string");
        let refused = |tgt: &Schema, m: &CompiledMigration| {
            matches!(
                functor(&src, tgt, m),
                Err(RestrictError::ValueStructureRemap { edge }) if edge == inner("posts", "item")
            )
        };
        // Renamed.
        assert!(refused(
            &with_list_column("articles", "element", "string"),
            &rename_posts("element")
        ));
        // An endpoint of another kind than the image of the source endpoint
        // under the vertex map.
        let mut tgt = with_list_column("articles", "item", "string");
        tgt.vertices.insert(
            "articles.tags.count".into(),
            Vertex {
                id: "articles.tags.count".into(),
                kind: "integer".into(),
                nsid: None,
            },
        );
        let elsewhere = Edge {
            tgt: "articles.tags.count".into(),
            ..inner("articles", "item")
        };
        tgt.edges.insert(elsewhere.clone(), "items".into());
        let mut m = rename_posts("item");
        m.edge_remap.insert(inner("posts", "item"), elsewhere);
        assert!(refused(&tgt, &m));
        // An endpoint the vertex map leaves alone keeps its own kind.
        let mut m = rename_posts("item");
        m.vertex_remap.remove("posts.tags.items");
        assert!(refused(
            &with_list_column("articles", "item", "integer"),
            &m
        ));
        // Another edge kind.
        let mut tgt = with_list_column("articles", "item", "string");
        let other_kind = Edge {
            kind: "elements".into(),
            ..inner("articles", "item")
        };
        tgt.edges.insert(other_kind.clone(), "elements".into());
        let mut m = rename_posts("item");
        m.edge_remap.insert(inner("posts", "item"), other_kind);
        assert!(refused(&tgt, &m));
        // No inner value edge of the target.
        let mut m = rename_posts("item");
        m.edge_remap
            .insert(inner("posts", "item"), fk("articles", "articles", "loop"));
        assert!(refused(&with_list_column("articles", "item", "string"), &m));
    }

    #[test]
    fn attribute_surjectivity_names_the_least_missing_column() -> Result<(), RestrictError> {
        let src = schema(&[("a", &["x"])], &[]);
        let tgt = schema(&[("a", &["x", "y", "w"]), ("z", &["v"]), ("e", &[])], &[]);
        let mut m = CompiledMigration::default();
        m.surviving_verts.insert("a".into());
        m.surviving_edges.insert(column("a", "x"));
        let f = functor(&src, &tgt, &m)?;
        assert!(matches!(
            f.attribute_surjective(),
            Err(RestrictError::AttributeOutsideImage { table, column })
                if table == "a" && column == "w"
        ));
        // A table outside the image passes only with no columns: `e` passes,
        // `z` does not.
        let tgt = schema(&[("a", &["x"]), ("z", &["v"]), ("e", &[])], &[]);
        let f = functor(&src, &tgt, &m)?;
        assert_eq!(
            f.tables_outside_image()
                .into_iter()
                .map(AsRef::as_ref)
                .collect::<Vec<&str>>(),
            ["e", "z"]
        );
        assert!(matches!(
            f.attribute_surjective(),
            Err(RestrictError::AttributeOutsideImage { table, column })
                if table == "z" && column == "v"
        ));
        let tgt = schema(&[("a", &["x"]), ("e", &[])], &[]);
        let f = functor(&src, &tgt, &m)?;
        assert_eq!(
            f.tables_outside_image()
                .into_iter()
                .map(AsRef::as_ref)
                .collect::<Vec<&str>>(),
            ["e"]
        );
        f.attribute_surjective()?;
        Ok(())
    }

    /// A schema with no relation-ified form under its protocol is a restrict
    /// error, the source reported first.
    #[test]
    fn a_classification_failure_is_a_restrict_error() {
        let mut src = schema(&[("a", &[])], &[]);
        src.vertices.insert(
            "s".into(),
            Vertex {
                id: "s".into(),
                kind: "mystery".into(),
                nsid: None,
            },
        );
        let mut tgt = schema(&[("a", &[])], &[]);
        tgt.vertices.insert(
            "t".into(),
            Vertex {
                id: "t".into(),
                kind: "enigma".into(),
                nsid: None,
            },
        );
        let m = CompiledMigration::default();
        assert!(matches!(
            functor(&src, &tgt, &m),
            Err(RestrictError::RelSchema(RelSchemaError::UnclassifiedKind { vertex, .. }))
                if vertex == "s"
        ));
        assert!(matches!(
            functor(&schema(&[("a", &[])], &[]), &tgt, &m),
            Err(RestrictError::RelSchema(RelSchemaError::UnclassifiedKind { vertex, .. }))
                if vertex == "t"
        ));
    }

    #[test]
    fn the_identity_functor_refuses_an_unclassified_kind() -> Result<(), RelSchemaError> {
        let e = fk("a", "a", "self");
        let s = schema(&[("a", &["x"]), ("b", &[])], std::slice::from_ref(&e));
        let f = MigrationFunctor::identity(&s, &protocol())?;
        assert_eq!(f.table_image("b"), Some(&Name::from("b")));
        assert_eq!(f.relation_image(&e), Some(&e));
        assert_eq!(f.column_image("a", "x"), Some("x"));
        assert!(f.tables_outside_image().is_empty());
        let mut unclassified = s;
        unclassified.vertices.insert(
            "q".into(),
            Vertex {
                id: "q".into(),
                kind: "mystery".into(),
                nsid: None,
            },
        );
        assert_eq!(
            MigrationFunctor::identity(&unclassified, &protocol()),
            Err(RelSchemaError::UnclassifiedKind {
                vertex: "q".into(),
                kind: "mystery".into()
            })
        );
        Ok(())
    }

    /// Every restrict error names its variant in its structured detail, in
    /// snake case, and no two variants share a name.
    /// The restrict errors that are not about the migration functor, each
    /// with its kind.
    fn other_restrict_errors() -> Vec<(RestrictError, &'static str)> {
        vec![
            (
                RestrictError::NoEdgeFound {
                    src: "a".into(),
                    tgt: "b".into(),
                },
                "no_edge_found",
            ),
            (
                RestrictError::AmbiguousEdge {
                    src: "a".into(),
                    tgt: "b".into(),
                    count: 2,
                },
                "ambiguous_edge",
            ),
            (RestrictError::RootPruned, "root_pruned"),
            (
                RestrictError::FanReconstructionFailed {
                    hyper_edge_id: "h".into(),
                    detail: "d".into(),
                },
                "fan_reconstruction_failed",
            ),
            (
                RestrictError::ProductSizeExceeded {
                    vertex: "v".into(),
                    actual: 2,
                    limit: 1,
                },
                "product_size_exceeded",
            ),
            (
                RestrictError::MultiElementFiber {
                    vertex: "v".into(),
                    count: 2,
                },
                "multi_element_fiber",
            ),
            (
                RestrictError::UnmappedAnchor {
                    anchor: "a".into(),
                    node_id: 0,
                },
                "unmapped_anchor",
            ),
            (
                RestrictError::AttributeCollision {
                    vertex: "v".into(),
                    column: "c".into(),
                },
                "attribute_collision",
            ),
            (
                RestrictError::NonInjectiveVertexMap {
                    target: "t".into(),
                    sources: Vec::new(),
                },
                "non_injective_vertex_map",
            ),
            (
                RestrictError::FieldTransformFailed {
                    key: "k".into(),
                    source: panproto_expr::ExprError::StepLimitExceeded(1),
                },
                "field_transform_failed",
            ),
            (RestrictError::RowIdsExhausted, "row_ids_exhausted"),
            (
                RestrictError::VertexOutsideDomain { vertex: "v".into() },
                "vertex_outside_domain",
            ),
            (
                RestrictError::EdgeOutsideDomain {
                    edge: fk("a", "b", "e"),
                },
                "edge_outside_domain",
            ),
        ]
    }

    /// The restrict errors about the migration functor, each with its kind.
    fn functor_restrict_errors() -> Vec<(RestrictError, &'static str)> {
        let e = || fk("a", "b", "e");
        vec![
            (
                RestrictError::IncoherentEdgeImage {
                    source_edge: Box::new(e()),
                    image: Box::new(e()),
                },
                "incoherent_edge_image",
            ),
            (
                RestrictError::VertexImageNotATable {
                    vertex: "v".into(),
                    image: "w".into(),
                },
                "vertex_image_not_a_table",
            ),
            (
                RestrictError::IncoherentAttributeImage {
                    edge: Box::new(e()),
                    image: None,
                },
                "incoherent_attribute_image",
            ),
            (
                RestrictError::AttributeOutsideImage {
                    table: "t".into(),
                    column: "c".into(),
                },
                "attribute_outside_image",
            ),
            (
                RestrictError::ValueStructureRemap { edge: e() },
                "value_structure_remap",
            ),
            (
                RestrictError::RelSchema(RelSchemaError::AmbiguousKind { kind: "k".into() }),
                "rel_schema",
            ),
        ]
    }

    /// The coherence errors carry their edges under the names the
    /// contract gives them: the incoherent edge image's source edge as
    /// `source_edge`. The errors with one edge carry it as a plain `Edge`;
    /// the two with two edges box both, so that `RestrictError` stays within
    /// the size `clippy::result_large_err` allows.
    #[test]
    fn coherence_errors_carry_their_edges_under_their_names() {
        let e = fk("a", "b", "e");
        let edges: Vec<Edge> = [
            RestrictError::IncoherentEdgeImage {
                source_edge: Box::new(e.clone()),
                image: Box::new(e.clone()),
            },
            RestrictError::IncoherentAttributeImage {
                edge: Box::new(e.clone()),
                image: Some(Box::new(e.clone())),
            },
            RestrictError::ValueStructureRemap { edge: e.clone() },
            RestrictError::RelSchema(RelSchemaError::ValueReachesTable { edge: e.clone() }),
            RestrictError::RelSchema(RelSchemaError::RelationEndpointNotATable { edge: e.clone() }),
        ]
        .into_iter()
        .flat_map(|error| match error {
            RestrictError::IncoherentEdgeImage { source_edge, image } => {
                vec![*source_edge, *image]
            }
            RestrictError::IncoherentAttributeImage { edge, image } => {
                std::iter::once(*edge).chain(image.map(|i| *i)).collect()
            }
            RestrictError::ValueStructureRemap { edge }
            | RestrictError::RelSchema(
                RelSchemaError::ValueReachesTable { edge }
                | RelSchemaError::RelationEndpointNotATable { edge },
            ) => vec![edge],
            other => panic!("{other:?}"),
        })
        .collect();
        assert_eq!(edges, vec![e; 7]);
    }

    #[test]
    fn every_restrict_error_has_a_kind() {
        let all = other_restrict_errors()
            .into_iter()
            .chain(functor_restrict_errors());
        let mut seen = BTreeSet::new();
        for (error, kind) in all {
            match error.detail() {
                crate::ErrorDetail::Restrict {
                    kind: named,
                    message,
                    ..
                } => {
                    assert_eq!(named, kind);
                    assert_eq!(message, error.to_string());
                    assert!(seen.insert(named), "{kind} twice");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn pi_errors_are_adjunction_errors() -> Result<(), Box<dyn std::error::Error>> {
        let src = schema(&[("a", &["x"])], &[]);
        let tgt = schema(&[("a", &["x"]), ("z", &["v"])], &[]);
        let mut m = CompiledMigration::default();
        m.surviving_verts.insert("a".into());
        m.surviving_edges.insert(column("a", "x"));
        let x = FInstance::new().with_rows("a", [(RowId(0), row(&[("x", 1)]))])?;
        let p = protocol();
        assert!(matches!(
            crate::functor_pi(&x, &src, &p, &tgt, &p, &m, 10),
            Err(crate::AdjunctionError::Pi(RestrictError::AttributeOutsideImage { table, column }))
                if table == "z" && column == "v"
        ));
        // A construction error of the schema-taking form is wrapped too.
        m.vertex_remap.insert("a".into(), "nowhere".into());
        assert!(matches!(
            crate::functor_pi(&x, &src, &p, &tgt, &p, &m, 10),
            Err(crate::AdjunctionError::Pi(
                RestrictError::VertexImageNotATable { .. }
            ))
        ));
        // So is an exceeded product bound.
        let tgt = schema(&[("m", &[])], &[]);
        let src = schema(&[("a", &[]), ("b", &[])], &[]);
        let mut m = CompiledMigration::default();
        m.vertex_remap.insert("a".into(), "m".into());
        m.vertex_remap.insert("b".into(), "m".into());
        let x = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new()), (RowId(1), Row::new())])
            .and_then(|i| i.with_rows("b", [(RowId(2), Row::new()), (RowId(3), Row::new())]))?;
        assert!(matches!(
            crate::functor_pi(&x, &src, &p, &tgt, &p, &m, 3),
            Err(crate::AdjunctionError::Pi(
                RestrictError::ProductSizeExceeded {
                    actual: 4,
                    limit: 3,
                    ..
                }
            ))
        ));
        assert_eq!(
            crate::functor_pi(&x, &src, &p, &tgt, &p, &m, 4)?.row_count("m"),
            4
        );
        Ok(())
    }
}
