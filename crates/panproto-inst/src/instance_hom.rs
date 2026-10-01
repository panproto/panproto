//! First-class instance homomorphisms.
//!
//! An instance homomorphism is a structure-preserving map between two
//! instances of the *same* schema. [`WInstanceHom`] maps the nodes of one
//! [`WInstance`] to the nodes of another; [`FInstanceHom`] maps the rows of
//! one [`FInstance`] to the rows of another by [`RowId`]. Both carry a
//! `check` that verifies the map respects instance structure (anchors, arcs,
//! fans, and root for W-types; foreign keys for F-instances), plus
//! [`identity`](WInstanceHom::identity), [`compose`](WInstanceHom::compose),
//! and [`is_isomorphism`](WInstanceHom::is_isomorphism).
//!
//! These homomorphisms are the morphisms of the category of instances over a
//! fixed schema. They are the arrows the Sigma/Delta adjunction transposes and
//! the equality witnesses that compare instances when a migration square is
//! checked for commutativity.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::Edge;

use crate::error::InstError;
use crate::fan::Fan;
use crate::functor::{FInstance, same_value};
use crate::value::Value;
use crate::view_ref::RowId;
use crate::wtype::WInstance;

/// Error raised when an instance homomorphism fails its structural checks or
/// when two homomorphisms cannot be composed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HomError {
    /// The node map is not total: a domain node has no image.
    #[error("node map is not total: domain node {0} has no image")]
    NodeNotMapped(u32),

    /// The node map sends a domain node to a codomain node that does not exist.
    #[error("node map sends domain node {src} to nonexistent codomain node {tgt}")]
    ImageNodeMissing {
        /// The domain node.
        src: u32,
        /// The claimed image, absent from the codomain.
        tgt: u32,
    },

    /// A domain node and its image sit over different schema vertices, so the
    /// map does not preserve anchors.
    #[error(
        "anchor not preserved: node {src} anchored at `{src_anchor}` \
         maps to node {tgt} anchored at `{tgt_anchor}`"
    )]
    AnchorMismatch {
        /// The domain node.
        src: u32,
        /// Its image in the codomain.
        tgt: u32,
        /// The domain node's anchor.
        src_anchor: Name,
        /// The image node's anchor.
        tgt_anchor: Name,
    },

    /// A domain arc has no corresponding arc in the codomain under the map, so
    /// the map does not preserve the tree structure.
    #[error(
        "arc ({parent},{child}) is not preserved: \
         no codomain arc ({mapped_parent},{mapped_child}) of the same edge"
    )]
    ArcNotPreserved {
        /// The domain arc's parent node.
        parent: u32,
        /// The domain arc's child node.
        child: u32,
        /// The image of the parent.
        mapped_parent: u32,
        /// The image of the child.
        mapped_child: u32,
    },

    /// A domain fan has no corresponding fan in the codomain under the map.
    #[error("fan `{hyper_edge_id}` at parent {parent} is not preserved")]
    FanNotPreserved {
        /// The hyper-edge the fan instantiates.
        hyper_edge_id: String,
        /// The domain fan's parent node.
        parent: u32,
    },

    /// A domain node (or row) and its image carry different attribute values,
    /// so the map does not preserve the attribute assignment.
    #[error("attributes not preserved: node {src} and its image {tgt} carry different values")]
    AttributeMismatch {
        /// The domain node id (W-type) or row index (F-instance).
        src: u32,
        /// The image node id or row index.
        tgt: u32,
    },

    /// The map does not send the domain root to the codomain root.
    #[error("root not preserved: domain root {src} maps to {mapped}, but codomain root is {cod}")]
    RootNotPreserved {
        /// The domain root.
        src: u32,
        /// Its image.
        mapped: u32,
        /// The codomain root.
        cod: u32,
    },

    /// A table present in the domain has no row map, or the row map disagrees
    /// with the table's row count, or the codomain lacks the table.
    #[error("row map for table `{table}` is missing or malformed: {detail}")]
    RowMapMalformed {
        /// The table (schema vertex) whose row map is malformed.
        table: String,
        /// What is wrong.
        detail: String,
    },

    /// A foreign key of the domain is not preserved by the row maps.
    #[error(
        "foreign key `{edge}` is not preserved: mapped pair \
         ({mapped_src},{mapped_tgt}) absent from the codomain"
    )]
    ForeignKeyNotPreserved {
        /// A rendering of the edge whose foreign key is broken.
        edge: String,
        /// The image of the source row.
        mapped_src: u32,
        /// The image of the target row.
        mapped_tgt: u32,
    },

    /// Two homomorphisms cannot be composed because the image of the first is
    /// not in the domain of the second.
    #[error("cannot compose: intermediate node {0} is outside the second map's domain")]
    ComposeNodeMismatch(u32),

    /// Two F-instance homomorphisms cannot be composed because their table
    /// row maps do not line up.
    #[error("cannot compose F-instance homs on table `{table}`: {detail}")]
    ComposeRowMismatch {
        /// The table where composition fails.
        table: String,
        /// What is wrong.
        detail: String,
    },
}

/// A homomorphism between two W-type instances of the same schema.
///
/// The map sends each node of the domain instance to a node of the codomain
/// instance. A well-formed homomorphism preserves anchors, arcs, fans, and the
/// root; [`check`](Self::check) verifies these conditions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WInstanceHom {
    /// Node map: domain node id to codomain node id.
    pub node_map: HashMap<u32, u32>,
}

impl WInstanceHom {
    /// Build a homomorphism from an explicit node map.
    #[must_use]
    pub const fn new(node_map: HashMap<u32, u32>) -> Self {
        Self { node_map }
    }

    /// The identity homomorphism on `instance` (each node maps to itself).
    #[must_use]
    pub fn identity(instance: &WInstance) -> Self {
        Self {
            node_map: instance.nodes.keys().map(|&id| (id, id)).collect(),
        }
    }

    /// Verify that this homomorphism from `dom` to `cod` preserves the W-type
    /// structure.
    ///
    /// Checks, in order: totality of the node map over `dom`'s nodes;
    /// existence of every image in `cod`; anchor preservation; arc naturality
    /// (each domain arc maps to a codomain arc of the same edge); fan
    /// naturality; and root preservation.
    ///
    /// # Errors
    ///
    /// Returns the first [`HomError`] encountered.
    pub fn check(&self, dom: &WInstance, cod: &WInstance) -> Result<(), HomError> {
        // Totality, image existence, and anchor preservation.
        for (&src, node) in &dom.nodes {
            let &tgt = self
                .node_map
                .get(&src)
                .ok_or(HomError::NodeNotMapped(src))?;
            let image = cod
                .nodes
                .get(&tgt)
                .ok_or(HomError::ImageNodeMissing { src, tgt })?;
            if node.anchor != image.anchor {
                return Err(HomError::AnchorMismatch {
                    src,
                    tgt,
                    src_anchor: node.anchor.clone(),
                    tgt_anchor: image.anchor.clone(),
                });
            }
            // Attribute preservation: a morphism of attributed C-sets acts as
            // the identity on attribute values, so a node and its image must
            // agree on their leaf value and extra fields.
            if node.value != image.value || node.extra_fields != image.extra_fields {
                return Err(HomError::AttributeMismatch { src, tgt });
            }
        }

        // Arc naturality: each domain arc has an image arc of the same edge.
        let cod_arcs: HashSet<(u32, u32, &panproto_schema::Edge)> =
            cod.arcs.iter().map(|(p, c, e)| (*p, *c, e)).collect();
        for (parent, child, edge) in &dom.arcs {
            let mapped_parent = self.node_map[parent];
            let mapped_child = self.node_map[child];
            if !cod_arcs.contains(&(mapped_parent, mapped_child, edge)) {
                return Err(HomError::ArcNotPreserved {
                    parent: *parent,
                    child: *child,
                    mapped_parent,
                    mapped_child,
                });
            }
        }

        // Fan naturality: each domain fan maps to a codomain fan. Fans hold a
        // label map, so they are not hashable; compare by equality instead.
        for fan in &dom.fans {
            let mapped = Fan {
                hyper_edge_id: fan.hyper_edge_id.clone(),
                parent: self.node_map[&fan.parent],
                children: fan
                    .children
                    .iter()
                    .map(|(label, id)| (label.clone(), self.node_map[id]))
                    .collect(),
            };
            if !cod.fans.contains(&mapped) {
                return Err(HomError::FanNotPreserved {
                    hyper_edge_id: fan.hyper_edge_id.clone(),
                    parent: fan.parent,
                });
            }
        }

        // Root preservation.
        let mapped_root = self.node_map[&dom.root];
        if mapped_root != cod.root {
            return Err(HomError::RootNotPreserved {
                src: dom.root,
                mapped: mapped_root,
                cod: cod.root,
            });
        }

        Ok(())
    }

    /// Compose two homomorphisms: `self` from `A` to `B` followed by `other`
    /// from `B` to `C`, yielding a homomorphism from `A` to `C`.
    ///
    /// # Errors
    ///
    /// Returns [`HomError::ComposeNodeMismatch`] if some image of `self` is
    /// outside the domain of `other`.
    pub fn compose(&self, other: &Self) -> Result<Self, HomError> {
        let mut node_map = HashMap::with_capacity(self.node_map.len());
        for (&a, &b) in &self.node_map {
            let &c = other
                .node_map
                .get(&b)
                .ok_or(HomError::ComposeNodeMismatch(b))?;
            node_map.insert(a, c);
        }
        Ok(Self { node_map })
    }

    /// Returns `true` iff this homomorphism from `dom` to `cod` is an
    /// isomorphism: a structure-preserving bijection whose inverse is also
    /// structure preserving.
    #[must_use]
    pub fn is_isomorphism(&self, dom: &WInstance, cod: &WInstance) -> bool {
        if self.check(dom, cod).is_err() {
            return false;
        }
        // The map must be a bijection onto the codomain's nodes.
        if self.node_map.len() != cod.nodes.len() {
            return false;
        }
        let mut inverse = HashMap::with_capacity(self.node_map.len());
        for (&src, &tgt) in &self.node_map {
            // Injectivity: no two domain nodes share an image.
            if inverse.insert(tgt, src).is_some() {
                return false;
            }
        }
        // Surjectivity onto cod is now implied by len equality + injectivity,
        // provided every image is a genuine codomain node (checked above).
        Self { node_map: inverse }.check(cod, dom).is_ok()
    }
}

/// A homomorphism between two set-valued functor instances, possibly of
/// different schemas.
///
/// One row map sends each [`RowId`] of the domain to a [`RowId`] of the
/// codomain. Row ids are unique across a whole instance, so one map serves
/// every table. The map travels along a map of schemas: `table_map` names
/// the image table of each domain table it renames, `edge_map` the image
/// foreign key of each domain edge it sends elsewhere, and `null_map` the
/// value each labeled null of the domain takes in the codomain. An
/// unlisted table maps to the codomain table of its own name, an unlisted
/// edge to the edge of its own kind and name between the images of its
/// endpoints, and an unlisted null to itself, so a map between instances of
/// one schema lists nothing but rows.
///
/// A well-formed map sends each row into the image table of its own table,
/// sends its cells (labeled nulls replaced along `null_map`) to the image
/// row's cells and sends each foreign-key pair to a pair of the image edge.
/// [`check`](Self::check) verifies all of this.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FInstanceHom {
    /// The image of each domain row.
    pub row_map: BTreeMap<RowId, RowId>,
    /// The image table of each domain table the map renames.
    pub table_map: BTreeMap<Name, Name>,
    /// The image foreign key of each domain edge the map does not send to
    /// the edge of its own kind and name between its endpoints' images.
    pub edge_map: BTreeMap<Edge, Edge>,
    /// The value each labeled null of the domain takes in the codomain.
    pub null_map: BTreeMap<u64, Value>,
}

impl FInstanceHom {
    /// Build a homomorphism between instances of one schema from an explicit
    /// row map.
    #[must_use]
    pub const fn new(row_map: BTreeMap<RowId, RowId>) -> Self {
        Self {
            row_map,
            table_map: BTreeMap::new(),
            edge_map: BTreeMap::new(),
            null_map: BTreeMap::new(),
        }
    }

    /// The same homomorphism travelling along `table_map` (entries sending a
    /// table to itself are dropped). The listed edges keep their images; an
    /// unlisted edge follows the new table map.
    #[must_use]
    pub fn with_table_map(mut self, table_map: BTreeMap<Name, Name>) -> Self {
        let listed = std::mem::take(&mut self.edge_map);
        self.table_map = table_map.into_iter().filter(|(a, b)| a != b).collect();
        self.with_edge_map(listed)
    }

    /// The same homomorphism sending edges along `edge_map`, replacing the
    /// edge map it had (entries agreeing with the default image under the
    /// table map are dropped).
    #[must_use]
    pub fn with_edge_map(mut self, edge_map: BTreeMap<Edge, Edge>) -> Self {
        self.edge_map.clear();
        let kept = edge_map
            .into_iter()
            .filter(|(edge, image)| &self.image_edge(edge) != image)
            .collect();
        self.edge_map = kept;
        self
    }

    /// The domain tables this map sends to `table`, those it lists and
    /// `table` itself when unlisted, ascending.
    fn table_preimages(&self, table: &Name) -> Vec<Name> {
        let mut preimages: Vec<Name> = self
            .table_map
            .iter()
            .filter(|(_, image)| *image == table)
            .map(|(source, _)| source.clone())
            .collect();
        if !self.table_map.contains_key(table) {
            preimages.push(table.clone());
        }
        preimages.sort_unstable();
        preimages
    }

    /// The same homomorphism replacing labeled nulls along `null_map`
    /// (entries sending a null to itself are dropped).
    #[must_use]
    pub fn with_null_map(mut self, null_map: BTreeMap<u64, Value>) -> Self {
        self.null_map = null_map
            .into_iter()
            .filter(|(null, value)| *value != Value::LabeledNull(*null))
            .collect();
        self
    }

    /// Build a homomorphism between instances of one schema from a row map,
    /// checking that every mapped row lands in the codomain table of its own
    /// table's name.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::HomTableMismatch`] naming the least row that is
    /// not a domain row, has no image in the codomain, or lands in another
    /// table.
    pub fn from_row_map(
        row_map: BTreeMap<RowId, RowId>,
        dom: &FInstance,
        cod: &FInstance,
    ) -> Result<Self, InstError> {
        Self::from_row_map_along(row_map, BTreeMap::new(), dom, cod)
    }

    /// Build a homomorphism from a row map travelling along `table_map`,
    /// checking that every mapped row lands in the image table of its own
    /// table.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::HomTableMismatch`] naming the least row that is
    /// not a domain row, has no image in the codomain, or lands outside the
    /// image of its table.
    pub fn from_row_map_along(
        row_map: BTreeMap<RowId, RowId>,
        table_map: BTreeMap<Name, Name>,
        dom: &FInstance,
        cod: &FInstance,
    ) -> Result<Self, InstError> {
        let hom = Self::new(row_map).with_table_map(table_map);
        hom.check_tables(dom, cod)?;
        Ok(hom)
    }

    /// The codomain table a domain table maps into.
    #[must_use]
    pub fn image_table<'a>(&'a self, table: &'a Name) -> &'a Name {
        self.table_map.get(table).unwrap_or(table)
    }

    /// The codomain foreign key a domain edge maps to: its `edge_map` entry,
    /// else the edge of its own kind and name between the images of its
    /// endpoints.
    #[must_use]
    pub fn image_edge(&self, edge: &Edge) -> Edge {
        self.edge_map.get(edge).cloned().unwrap_or_else(|| Edge {
            src: self.image_table(&edge.src).clone(),
            tgt: self.image_table(&edge.tgt).clone(),
            kind: edge.kind.clone(),
            name: edge.name.clone(),
        })
    }

    /// The codomain value of a domain cell: a labeled null listed in
    /// `null_map` takes its entry; every other value is itself.
    #[must_use]
    pub fn image_value<'a>(&'a self, value: &'a Value) -> &'a Value {
        match value {
            Value::LabeledNull(null) => self.null_map.get(null).unwrap_or(value),
            _ => value,
        }
    }

    /// Check that every mapped row lands in the image table of its own
    /// table, read through each instance's row index.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::HomTableMismatch`] naming the least offending
    /// row.
    pub fn check_tables(&self, dom: &FInstance, cod: &FInstance) -> Result<(), InstError> {
        for (&row, &image) in &self.row_map {
            match (dom.table_of(row), cod.table_of(image)) {
                (Some(a), Some(b)) if self.image_table(a) == b => {}
                _ => return Err(InstError::HomTableMismatch { row }),
            }
        }
        Ok(())
    }

    /// The identity homomorphism on `instance` (each row maps to itself).
    #[must_use]
    pub fn identity(instance: &FInstance) -> Self {
        Self::new(instance.row_ids().into_iter().map(|id| (id, id)).collect())
    }

    /// The image of a row, if the map has one.
    #[must_use]
    pub fn image(&self, row: RowId) -> Option<RowId> {
        self.row_map.get(&row).copied()
    }

    /// Verify that this homomorphism from `dom` to `cod` preserves the
    /// relational structure.
    ///
    /// Checks totality (every domain row has an image), table membership
    /// (the image is a codomain row of the image of its table), attribute
    /// preservation (the image row carries the domain row's cells, each
    /// labeled null replaced along `null_map`, compared by canonical bytes so
    /// a `NaN` cell is preserved by itself and `0.0` is not by `-0.0`) and
    /// foreign-key naturality
    /// (each domain pair maps to a pair of the image edge).
    ///
    /// # Errors
    ///
    /// Returns the first [`HomError`] encountered, rows taken in ascending id.
    pub fn check(&self, dom: &FInstance, cod: &FInstance) -> Result<(), HomError> {
        for row in dom.row_ids() {
            let table = dom.table_of(row).cloned().unwrap_or_default();
            let image = self.image(row).ok_or_else(|| HomError::RowMapMalformed {
                table: table.to_string(),
                detail: format!("row {} has no image", row.0),
            })?;
            let image_table = self.image_table(&table);
            if cod.table_of(image) != Some(image_table) {
                return Err(HomError::RowMapMalformed {
                    table: table.to_string(),
                    detail: format!(
                        "row {} maps to row {}, which is not a row of the image table {image_table}",
                        row.0, image.0
                    ),
                });
            }
            let preserved = match (dom.row(row), cod.row(image)) {
                (Some(cells), Some(image_cells)) => {
                    cells.len() == image_cells.len()
                        && cells.iter().all(|(column, value)| {
                            image_cells.get(column).is_some_and(|image_cell| {
                                same_value(image_cell, self.image_value(value))
                            })
                        })
                }
                _ => false,
            };
            if !preserved {
                return Err(HomError::AttributeMismatch {
                    src: row.0,
                    tgt: image.0,
                });
            }
        }

        for (edge, pairs) in dom.foreign_keys() {
            let image_edge = self.image_edge(edge);
            for &(s, t) in pairs {
                let (Some(ms), Some(mt)) = (self.image(s), self.image(t)) else {
                    continue;
                };
                if !cod.contains_pair(&image_edge, ms, mt) {
                    return Err(HomError::ForeignKeyNotPreserved {
                        edge: format!("{}->{}", image_edge.src, image_edge.tgt),
                        mapped_src: ms.0,
                        mapped_tgt: mt.0,
                    });
                }
            }
        }

        Ok(())
    }

    /// Compose two homomorphisms: `self` from `A` to `B` followed by `other`
    /// from `B` to `C`. The row, table, edge and null maps compose
    /// pointwise.
    ///
    /// # Errors
    ///
    /// Returns [`HomError::ComposeRowMismatch`] if an intermediate row is
    /// outside the second map's domain.
    pub fn compose(&self, other: &Self) -> Result<Self, HomError> {
        let mut row_map = BTreeMap::new();
        for (&row, &mid) in &self.row_map {
            let image = other
                .image(mid)
                .ok_or_else(|| HomError::ComposeRowMismatch {
                    table: String::new(),
                    detail: format!(
                        "intermediate row {} is outside the second map's domain",
                        mid.0
                    ),
                })?;
            row_map.insert(row, image);
        }
        let table_map = self
            .table_map
            .keys()
            .chain(other.table_map.keys())
            .map(|table| {
                let image = other.image_table(self.image_table(table)).clone();
                (table.clone(), image)
            })
            .collect();
        // An edge the first map does not list goes to its default image `d`;
        // when the second map lists `d`, every such preimage of `d` must be
        // listed in the composite, since its default there differs.
        let mut edges: BTreeSet<Edge> = self.edge_map.keys().cloned().collect();
        edges.extend(other.edge_map.keys().cloned());
        for image in other.edge_map.keys() {
            for src in self.table_preimages(&image.src) {
                for tgt in self.table_preimages(&image.tgt) {
                    let edge = Edge {
                        src: src.clone(),
                        tgt,
                        kind: image.kind.clone(),
                        name: image.name.clone(),
                    };
                    if !self.edge_map.contains_key(&edge) {
                        edges.insert(edge);
                    }
                }
            }
        }
        let edge_map = edges
            .into_iter()
            .map(|edge| {
                let image = other.image_edge(&self.image_edge(&edge));
                (edge, image)
            })
            .collect();
        let null_map = self
            .null_map
            .keys()
            .chain(other.null_map.keys())
            .map(|&null| {
                let own = Value::LabeledNull(null);
                (null, other.image_value(self.image_value(&own)).clone())
            })
            .collect();
        let composed = Self::new(row_map).with_table_map(table_map);
        Ok(composed.with_edge_map(edge_map).with_null_map(null_map))
    }

    /// Returns `true` iff this homomorphism from `dom` to `cod` is an
    /// isomorphism: the row map is a bijection onto the codomain's rows,
    /// the table, edge and null maps are invertible, and the inverse is a
    /// homomorphism from `cod` to `dom`.
    #[must_use]
    pub fn is_isomorphism(&self, dom: &FInstance, cod: &FInstance) -> bool {
        if self.check(dom, cod).is_err() {
            return false;
        }
        if self.row_map.len() != cod.total_row_count() {
            return false;
        }
        let Some(row_map) = invert(self.row_map.iter().map(|(a, b)| (*a, *b))) else {
            return false;
        };
        let Some(table_map) = invert(
            dom.table_names()
                .into_iter()
                .map(|table| (table.clone(), self.image_table(table).clone())),
        ) else {
            return false;
        };
        let Some(edge_map) = invert(
            dom.foreign_keys()
                .into_iter()
                .map(|(edge, _)| (edge.clone(), self.image_edge(edge))),
        ) else {
            return false;
        };
        let mut null_map = BTreeMap::new();
        for (&null, value) in &self.null_map {
            let Value::LabeledNull(image) = value else {
                return false;
            };
            if null_map.insert(*image, Value::LabeledNull(null)).is_some() {
                return false;
            }
        }
        Self::new(row_map)
            .with_table_map(table_map)
            .with_edge_map(edge_map)
            .with_null_map(null_map)
            .check(cod, dom)
            .is_ok()
    }
}

/// The inverse of a finite map given as its graph, or `None` when two
/// arguments share an image.
fn invert<K: Ord, V: Ord>(graph: impl IntoIterator<Item = (K, V)>) -> Option<BTreeMap<V, K>> {
    let mut inverse = BTreeMap::new();
    for (argument, image) in graph {
        if inverse.insert(image, argument).is_some() {
            return None;
        }
    }
    Some(inverse)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_schema::Edge;

    use super::*;
    use crate::metadata::Node;
    use crate::value::Value;

    fn edge(src: &str, tgt: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: None,
        }
    }

    /// A two-node W-instance: root `r` (id 0) with one child (id 1) via edge.
    fn w_pair(root_id: u32, child_id: u32) -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(root_id, Node::new(root_id, "root"));
        nodes.insert(child_id, Node::new(child_id, "leaf"));
        WInstance::new(
            nodes,
            vec![(root_id, child_id, edge("root", "leaf"))],
            vec![],
            root_id,
            "root".into(),
        )
    }

    #[test]
    fn identity_checks_and_composes() {
        let inst = w_pair(0, 1);
        let id = WInstanceHom::identity(&inst);
        assert!(id.check(&inst, &inst).is_ok());

        // A relabeling hom sending {0->10, 1->11} into a renumbered copy.
        let renamed = w_pair(10, 11);
        let h = WInstanceHom::new(HashMap::from([(0, 10), (1, 11)]));
        assert!(h.check(&inst, &renamed).is_ok());

        // identity ∘ h == h (apply h, then identity on the codomain).
        let id_cod = WInstanceHom::identity(&renamed);
        let composed = h.compose(&id_cod).expect("compose with identity");
        assert_eq!(composed, h);

        assert!(h.is_isomorphism(&inst, &renamed));
    }

    #[test]
    fn arc_breaking_node_map_fails_check() {
        // Domain: 0 -> 1. Codomain has nodes but no arc between the images.
        let dom = w_pair(0, 1);
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "leaf"));
        nodes.insert(2, Node::new(2, "leaf"));
        // Root arc goes to node 2, not node 1.
        let cod = WInstance::new(
            nodes,
            vec![(0, 2, edge("root", "leaf"))],
            vec![],
            0,
            "root".into(),
        );
        // Map the child (1) to the non-adjacent node 2's sibling... map 1->1,
        // which has no incoming arc in cod, breaking arc naturality.
        let h = WInstanceHom::new(HashMap::from([(0, 0), (1, 1)]));
        let err = h.check(&dom, &cod).expect_err("arc naturality must fail");
        assert!(matches!(err, HomError::ArcNotPreserved { .. }));
    }

    #[test]
    fn anchor_mismatch_fails_check() {
        let dom = w_pair(0, 1);
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        // Image of node 1 sits over the wrong vertex.
        nodes.insert(1, Node::new(1, "other"));
        let cod = WInstance::new(
            nodes,
            vec![(0, 1, edge("root", "leaf"))],
            vec![],
            0,
            "root".into(),
        );
        let h = WInstanceHom::identity(&dom);
        let err = h.check(&dom, &cod).expect_err("anchor mismatch must fail");
        assert!(matches!(err, HomError::AnchorMismatch { .. }));
    }

    #[test]
    fn incompatible_compose_errors() {
        // self maps 1 -> 99, but other is only defined on {0,1}.
        let h1 = WInstanceHom::new(HashMap::from([(0, 0), (1, 99)]));
        let h2 = WInstanceHom::new(HashMap::from([(0, 0), (1, 1)]));
        let err = h1
            .compose(&h2)
            .expect_err("compose must reject dangling image");
        assert_eq!(err, HomError::ComposeNodeMismatch(99));
    }

    #[test]
    fn non_root_preserving_map_fails() {
        let dom = w_pair(0, 1);
        let renamed = w_pair(10, 11);
        // Send the root to a non-root node.
        let h = WInstanceHom::new(HashMap::from([(0, 11), (1, 10)]));
        let err = h.check(&dom, &renamed).expect_err("root must be preserved");
        // Anchors differ too, so either error is acceptable; assert it fails.
        assert!(matches!(
            err,
            HomError::RootNotPreserved { .. } | HomError::AnchorMismatch { .. }
        ));
    }

    fn f_pair() -> FInstance {
        let posts = vec![
            HashMap::from([("id".to_owned(), Value::Int(1))]),
            HashMap::from([("id".to_owned(), Value::Int(2))]),
        ];
        let users = vec![HashMap::from([("id".to_owned(), Value::Int(10))])];
        FInstance::new()
            .with_table("post", posts)
            .and_then(|f| f.with_table("user", users))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("post", "user"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(2))],
                )
            })
            .expect("the fixture is well formed")
    }

    #[test]
    fn finstance_identity_and_fk_naturality() {
        let inst = f_pair();
        let id = FInstanceHom::identity(&inst);
        assert!(id.check(&inst, &inst).is_ok());
        assert!(id.is_isomorphism(&inst, &inst));

        // A codomain whose foreign key omits (1, 2) breaks FK naturality under
        // the identity row map (the rows themselves are unchanged, so
        // attribute preservation still holds).
        let mut broken = f_pair();
        assert!(broken.remove_foreign_key(&edge("post", "user"), RowId(1), RowId(2)));
        let err = id
            .check(&inst, &broken)
            .expect_err("FK naturality must fail");
        assert!(matches!(err, HomError::ForeignKeyNotPreserved { .. }));
    }

    #[test]
    fn a_row_map_into_the_wrong_table_is_refused() {
        let inst = f_pair();
        // Post row 0 sent to the user row: a row of another table.
        let row_map = BTreeMap::from([
            (RowId(0), RowId(2)),
            (RowId(1), RowId(1)),
            (RowId(2), RowId(2)),
        ]);
        let err = FInstanceHom::from_row_map(row_map.clone(), &inst, &inst)
            .expect_err("a row mapped into another table is refused");
        assert!(matches!(err, InstError::HomTableMismatch { row: RowId(0) }));
        assert!(FInstanceHom::new(row_map).check(&inst, &inst).is_err());
        assert!(
            FInstanceHom::from_row_map(FInstanceHom::identity(&inst).row_map, &inst, &inst).is_ok()
        );
    }

    #[test]
    fn winstance_attribute_change_fails_check() {
        // Domain node 1 carries a value; the image carries a different value.
        let mut dom_nodes = HashMap::new();
        dom_nodes.insert(0, Node::new(0, "root"));
        dom_nodes.insert(
            1,
            Node::new(1, "leaf").with_extra_field("weight", Value::Int(1)),
        );
        let dom = WInstance::new(
            dom_nodes,
            vec![(0, 1, edge("root", "leaf"))],
            vec![],
            0,
            "root".into(),
        );
        let mut cod_nodes = HashMap::new();
        cod_nodes.insert(0, Node::new(0, "root"));
        cod_nodes.insert(
            1,
            Node::new(1, "leaf").with_extra_field("weight", Value::Int(2)),
        );
        let cod = WInstance::new(
            cod_nodes,
            vec![(0, 1, edge("root", "leaf"))],
            vec![],
            0,
            "root".into(),
        );
        let h = WInstanceHom::identity(&dom);
        let err = h
            .check(&dom, &cod)
            .expect_err("differing attribute must fail check");
        assert!(matches!(
            err,
            HomError::AttributeMismatch { src: 1, tgt: 1 }
        ));
    }

    #[test]
    fn finstance_attribute_change_fails_check() {
        let inst = f_pair();
        let id = FInstanceHom::identity(&inst);
        // A codomain whose first post row carries a different id breaks
        // attribute preservation under the identity row map.
        let mut altered = f_pair();
        altered
            .row_mut(RowId(0))
            .expect("post row")
            .insert("id".to_owned(), Value::Int(99));
        let err = id
            .check(&inst, &altered)
            .expect_err("differing row must fail check");
        assert!(matches!(err, HomError::AttributeMismatch { src: 0, .. }));
    }

    fn fk(src: &str, tgt: &str, name: Option<&str>) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: name.map(Into::into),
        }
    }

    /// One row per table and one foreign-key pair along `key`.
    fn linked(key: &Edge, src_row: u32, tgt_row: u32, cell: Value) -> FInstance {
        let cells = HashMap::from([("v".to_owned(), cell)]);
        FInstance::new()
            .with_rows(key.src.clone(), [(RowId(src_row), cells.clone())])
            .and_then(|f| f.with_rows(key.tgt.clone(), [(RowId(tgt_row), cells)]))
            .and_then(|f| f.with_foreign_key(key.clone(), [(RowId(src_row), RowId(tgt_row))]))
            .expect("the fixture is well formed")
    }

    #[test]
    fn a_composite_lists_the_edges_a_renaming_sends_into_the_second_edge_map() {
        // A over `s -> s2`, B over `t -> t2` (the default image of A's key
        // under the table renaming), C over `t -> t2` under another name.
        let a = linked(&fk("s", "s2", None), 0, 1, Value::Int(1));
        let b = linked(&fk("t", "t2", None), 10, 11, Value::Int(1));
        let c = linked(&fk("t", "t2", Some("renamed")), 20, 21, Value::Int(1));
        let first = FInstanceHom::new(BTreeMap::from([
            (RowId(0), RowId(10)),
            (RowId(1), RowId(11)),
        ]))
        .with_table_map(BTreeMap::from([
            (Name::from("s"), Name::from("t")),
            (Name::from("s2"), Name::from("t2")),
        ]));
        let second = FInstanceHom::new(BTreeMap::from([
            (RowId(10), RowId(20)),
            (RowId(11), RowId(21)),
        ]))
        .with_edge_map(BTreeMap::from([(
            fk("t", "t2", None),
            fk("t", "t2", Some("renamed")),
        )]));
        first.check(&a, &b).expect("the renaming is a homomorphism");
        second
            .check(&b, &c)
            .expect("the edge renaming is a homomorphism");
        let composite = first.compose(&second).expect("the row maps compose");
        assert_eq!(
            composite.image_edge(&fk("s", "s2", None)),
            fk("t", "t2", Some("renamed"))
        );
        composite
            .check(&a, &c)
            .expect("the composite sends A's key to C's renamed key");
        assert!(composite.is_isomorphism(&a, &c));
    }

    #[test]
    fn setting_the_table_map_keeps_the_listed_edge_images() {
        let key = fk("s", "s2", None);
        let target = fk("u", "u2", Some("k"));
        let hom = FInstanceHom::new(BTreeMap::new())
            .with_edge_map(BTreeMap::from([(key.clone(), target.clone())]))
            .with_table_map(BTreeMap::from([
                (Name::from("s"), Name::from("u")),
                (Name::from("s2"), Name::from("u2")),
            ]));
        assert_eq!(hom.image_edge(&key), target);
        // An entry that the new table map makes the default is dropped.
        let default = FInstanceHom::new(BTreeMap::new())
            .with_edge_map(BTreeMap::from([(key.clone(), fk("u", "u2", None))]))
            .with_table_map(BTreeMap::from([
                (Name::from("s"), Name::from("u")),
                (Name::from("s2"), Name::from("u2")),
            ]));
        assert!(default.edge_map.is_empty());
        assert_eq!(default.image_edge(&key), fk("u", "u2", None));
    }

    #[test]
    fn a_row_map_along_a_renaming_checks_the_image_table() {
        let a = linked(&fk("s", "s2", None), 0, 1, Value::Int(1));
        let b = linked(&fk("t", "t2", None), 10, 11, Value::Int(1));
        let rows = BTreeMap::from([(RowId(0), RowId(10)), (RowId(1), RowId(11))]);
        let renaming = BTreeMap::from([
            (Name::from("s"), Name::from("t")),
            (Name::from("s2"), Name::from("t2")),
        ]);
        // Without the renaming, the rows land in tables of other names.
        assert!(matches!(
            FInstanceHom::from_row_map(rows.clone(), &a, &b),
            Err(InstError::HomTableMismatch { row: RowId(0) })
        ));
        let hom = FInstanceHom::from_row_map_along(rows, renaming.clone(), &a, &b)
            .expect("each row lands in the image of its table");
        hom.check(&a, &b).expect("the renaming is a homomorphism");
        // Crossing the renamed tables is refused.
        let crossed = BTreeMap::from([(RowId(0), RowId(11)), (RowId(1), RowId(10))]);
        assert!(matches!(
            FInstanceHom::from_row_map_along(crossed, renaming, &a, &b),
            Err(InstError::HomTableMismatch { row: RowId(0) })
        ));
    }

    #[test]
    fn a_null_map_replaces_labeled_nulls_in_the_image() {
        let key = fk("t", "t2", None);
        let open = linked(&key, 0, 1, Value::LabeledNull(3));
        let closed = linked(&key, 0, 1, Value::Str("x".into()));
        let identity = FInstanceHom::identity(&open);
        assert!(matches!(
            identity.check(&open, &closed),
            Err(HomError::AttributeMismatch { src: 0, tgt: 0 })
        ));
        let forced = identity
            .clone()
            .with_null_map(BTreeMap::from([(3, Value::Str("x".into()))]));
        forced
            .check(&open, &closed)
            .expect("the null reads as the value forced on it");
        assert!(
            !forced.is_isomorphism(&open, &closed),
            "a null sent to a constant has no inverse"
        );
        // Renaming one null to another is invertible.
        let renamed = linked(&key, 0, 1, Value::LabeledNull(4));
        let rename = identity.with_null_map(BTreeMap::from([(3, Value::LabeledNull(4))]));
        assert!(rename.is_isomorphism(&open, &renamed));
        // A null sent to itself is dropped from the map.
        let trivial = FInstanceHom::new(BTreeMap::new())
            .with_null_map(BTreeMap::from([(3, Value::LabeledNull(3))]));
        assert!(trivial.null_map.is_empty());
    }

    /// One row `0` in table `a` holding `x = value`.
    fn one_cell(value: Value) -> FInstance {
        FInstance::new()
            .with_rows("a", [(RowId(0), HashMap::from([("x".to_owned(), value)]))])
            .expect("one row")
    }

    /// Cells are compared exactly, by canonical bytes: the identity on an
    /// instance with a `NaN` cell is a homomorphism and an isomorphism, as
    /// the identity law of the category asks.
    #[test]
    fn the_identity_preserves_a_nan_cell() {
        let inst = one_cell(Value::Float(f64::NAN));
        let identity = FInstanceHom::identity(&inst);
        assert_eq!(identity.check(&inst, &inst), Ok(()));
        assert!(identity.is_isomorphism(&inst, &inst));
    }

    /// `0.0` and `-0.0` are different cells (their canonical bytes and the
    /// instances' digests differ), so the map between them that keeps the
    /// row is no homomorphism, let alone an isomorphism.
    #[test]
    fn a_row_map_does_not_carry_zero_to_negative_zero() {
        let positive = one_cell(Value::Float(0.0));
        let negative = one_cell(Value::Float(-0.0));
        assert_ne!(positive.digest(), negative.digest());
        let map = FInstanceHom::identity(&positive);
        assert_eq!(
            map.check(&positive, &negative),
            Err(HomError::AttributeMismatch { src: 0, tgt: 0 })
        );
        assert!(!map.is_isomorphism(&positive, &negative));
    }
}
