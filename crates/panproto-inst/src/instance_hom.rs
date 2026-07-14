//! First-class instance homomorphisms.
//!
//! An instance homomorphism is a structure-preserving map between two
//! instances of the *same* schema. [`WInstanceHom`] maps the nodes of one
//! [`WInstance`] to the nodes of another; [`FInstanceHom`] maps the rows of
//! one [`FInstance`] table by table to the rows of another. Both carry a
//! `check` that verifies the map respects instance structure (anchors, arcs,
//! fans, and root for W-types; foreign keys for F-instances), plus
//! [`identity`](WInstanceHom::identity), [`compose`](WInstanceHom::compose),
//! and [`is_isomorphism`](WInstanceHom::is_isomorphism).
//!
//! These homomorphisms are the morphisms of the category of instances over a
//! fixed schema. They are the arrows the Sigma/Delta adjunction transposes and
//! the equality witnesses that compare instances when a migration square is
//! checked for commutativity.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;

use crate::fan::Fan;
use crate::functor::FInstance;
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
        /// The image of the source row index.
        mapped_src: usize,
        /// The image of the target row index.
        mapped_tgt: usize,
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

/// A homomorphism between two set-valued functor instances of the same schema.
///
/// For each table (schema vertex), a row map sends each row index of the
/// domain to a row index of the codomain. A well-formed homomorphism preserves
/// foreign keys; [`check`](Self::check) verifies this.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FInstanceHom {
    /// Per-table row maps: table name to a vector indexed by domain row index,
    /// whose entry is the corresponding codomain row index.
    pub row_maps: HashMap<String, Vec<usize>>,
}

impl FInstanceHom {
    /// Build a homomorphism from explicit per-table row maps.
    #[must_use]
    pub const fn new(row_maps: HashMap<String, Vec<usize>>) -> Self {
        Self { row_maps }
    }

    /// The identity homomorphism on `instance` (each row maps to itself).
    #[must_use]
    pub fn identity(instance: &FInstance) -> Self {
        Self {
            row_maps: instance
                .tables
                .iter()
                .map(|(name, rows)| (name.clone(), (0..rows.len()).collect()))
                .collect(),
        }
    }

    /// Verify that this homomorphism from `dom` to `cod` preserves the
    /// relational structure.
    ///
    /// Checks totality (every domain table has a row map of the right length,
    /// present in the codomain, whose entries are in range) and foreign-key
    /// naturality (each domain foreign-key pair maps to a codomain pair).
    ///
    /// # Errors
    ///
    /// Returns the first [`HomError`] encountered.
    pub fn check(&self, dom: &FInstance, cod: &FInstance) -> Result<(), HomError> {
        for (table, rows) in &dom.tables {
            let map = self
                .row_maps
                .get(table)
                .ok_or_else(|| HomError::RowMapMalformed {
                    table: table.clone(),
                    detail: "no row map for this table".to_owned(),
                })?;
            if map.len() != rows.len() {
                return Err(HomError::RowMapMalformed {
                    table: table.clone(),
                    detail: format!(
                        "row map has {} entries but the table has {} rows",
                        map.len(),
                        rows.len()
                    ),
                });
            }
            let cod_rows = cod.tables.get(table).map_or(&[][..], Vec::as_slice);
            for (i, (&j, row)) in map.iter().zip(rows).enumerate() {
                if j >= cod_rows.len() {
                    return Err(HomError::RowMapMalformed {
                        table: table.clone(),
                        detail: format!(
                            "image row {j} out of range (codomain has {} rows)",
                            cod_rows.len()
                        ),
                    });
                }
                // Attribute preservation: the image row equals the domain row.
                if *row != cod_rows[j] {
                    return Err(HomError::AttributeMismatch {
                        src: u32::try_from(i).unwrap_or(u32::MAX),
                        tgt: u32::try_from(j).unwrap_or(u32::MAX),
                    });
                }
            }
        }

        for (edge, pairs) in &dom.foreign_keys {
            let src_map =
                self.row_maps
                    .get(edge.src.as_ref())
                    .ok_or_else(|| HomError::RowMapMalformed {
                        table: edge.src.to_string(),
                        detail: "foreign-key source table has no row map".to_owned(),
                    })?;
            let tgt_map =
                self.row_maps
                    .get(edge.tgt.as_ref())
                    .ok_or_else(|| HomError::RowMapMalformed {
                        table: edge.tgt.to_string(),
                        detail: "foreign-key target table has no row map".to_owned(),
                    })?;
            let cod_pairs: HashSet<(usize, usize)> = cod
                .foreign_keys
                .get(edge)
                .into_iter()
                .flatten()
                .copied()
                .collect();
            for &(si, ti) in pairs {
                let mapped = (src_map[si], tgt_map[ti]);
                if !cod_pairs.contains(&mapped) {
                    return Err(HomError::ForeignKeyNotPreserved {
                        edge: format!("{}->{}", edge.src, edge.tgt),
                        mapped_src: mapped.0,
                        mapped_tgt: mapped.1,
                    });
                }
            }
        }

        Ok(())
    }

    /// Compose two homomorphisms: `self` from `A` to `B` followed by `other`
    /// from `B` to `C`.
    ///
    /// # Errors
    ///
    /// Returns [`HomError::ComposeRowMismatch`] if the tables or row indices do
    /// not line up.
    pub fn compose(&self, other: &Self) -> Result<Self, HomError> {
        let mut row_maps = HashMap::with_capacity(self.row_maps.len());
        for (table, map) in &self.row_maps {
            let next = other
                .row_maps
                .get(table)
                .ok_or_else(|| HomError::ComposeRowMismatch {
                    table: table.clone(),
                    detail: "second map has no row map for this table".to_owned(),
                })?;
            let mut composed = Vec::with_capacity(map.len());
            for &j in map {
                let &k = next.get(j).ok_or_else(|| HomError::ComposeRowMismatch {
                    table: table.clone(),
                    detail: format!("intermediate row {j} is outside the second map's domain"),
                })?;
                composed.push(k);
            }
            row_maps.insert(table.clone(), composed);
        }
        Ok(Self { row_maps })
    }

    /// Returns `true` iff this homomorphism from `dom` to `cod` is an
    /// isomorphism: each row map is a bijection and the inverse preserves
    /// foreign keys.
    #[must_use]
    pub fn is_isomorphism(&self, dom: &FInstance, cod: &FInstance) -> bool {
        if self.check(dom, cod).is_err() {
            return false;
        }
        if self.row_maps.len() != cod.tables.len() {
            return false;
        }
        let mut inverse = HashMap::with_capacity(self.row_maps.len());
        for (table, map) in &self.row_maps {
            let cod_rows = cod.tables.get(table).map_or(0, Vec::len);
            if map.len() != cod_rows {
                return false;
            }
            let mut inv = vec![usize::MAX; cod_rows];
            for (i, &j) in map.iter().enumerate() {
                if inv[j] != usize::MAX {
                    return false; // not injective
                }
                inv[j] = i;
            }
            if inv.contains(&usize::MAX) {
                return false; // not surjective
            }
            inverse.insert(table.clone(), inv);
        }
        Self { row_maps: inverse }.check(cod, dom).is_ok()
    }
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
            .with_table("user", users)
            .with_foreign_key(edge("post", "user"), vec![(0, 0), (1, 0)])
    }

    #[test]
    fn finstance_identity_and_fk_naturality() {
        let inst = f_pair();
        let id = FInstanceHom::identity(&inst);
        assert!(id.check(&inst, &inst).is_ok());
        assert!(id.is_isomorphism(&inst, &inst));

        // A codomain whose foreign key omits (1,0) breaks FK naturality under
        // the identity row maps (the rows themselves are unchanged, so
        // attribute preservation still holds).
        let mut broken = f_pair();
        broken
            .foreign_keys
            .insert(edge("post", "user"), vec![(0, 0)]);
        let err = id
            .check(&inst, &broken)
            .expect_err("FK naturality must fail");
        assert!(matches!(err, HomError::ForeignKeyNotPreserved { .. }));
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
        altered.tables.get_mut("post").expect("post table")[0]
            .insert("id".to_owned(), Value::Int(99));
        let err = id
            .check(&inst, &altered)
            .expect_err("differing row must fail check");
        assert!(matches!(err, HomError::AttributeMismatch { src: 0, .. }));
    }
}
