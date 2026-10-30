//! The complement of a table restrict.
//!
//! [`TableComplement`] is what
//! [`functor_restrict_with_plan`](crate::functor_restrict_with_plan) leaves
//! out of its view: the source's dropped rows, cells and foreign-key pairs,
//! the source data of kept rows and pairs that the view does not determine,
//! and the contracted pairs it synthesized.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::functor::Row;
use crate::ginstance::TableEmbedding;
use crate::identity::{ComplementShape, IdentityCarrier, IdentityEnvelope};
use crate::value::Value;
use crate::view_ref::{RowId, SynthIndex, SynthKey};

/// A row the view does not hold, with the table it was in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DroppedRow {
    /// The source table of the row.
    pub table: Name,
    /// The row's cells.
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    pub row: Row,
}

/// The complement of a table restrict, the table form of
/// [`GraphComplement`](crate::GraphComplement): rows for nodes, pairs for
/// edges, cells for fields.
///
/// It is a complement in the sense of bancilhonspyratos1981update (Def 4.4,
/// stated): get paired with it is injective on the sources the table
/// restrict accepts, whose rows are total. Selection keeps the
/// rows it hides here, but a view deletion cascades into the pairs held here,
/// so it is not a constant complement (inferred).
///
/// Every map is ordered; maps keyed by tuples are written as sequences of
/// entries in key order. The fields are serialized in declaration order with
/// no defaults, so a complement has one encoding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableComplement {
    /// The rows the view does not hold: rows of dropped tables and rows a
    /// survival predicate drops.
    pub dropped_rows: BTreeMap<RowId, DroppedRow>,
    /// The source cells of kept rows whose column the view row holds no
    /// cell of that name for, by row and source column, with their source
    /// value. A column the migration renames or drops is one: the view
    /// holds no cell under its name, whatever name its value now has.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub dropped_cells: BTreeMap<(RowId, String), Value>,
    /// The pairs the view does not hold, by source foreign key.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub dropped_fk_pairs: BTreeMap<Edge, BTreeSet<(RowId, RowId)>>,
    /// The source foreign keys of each view pair whose source key the view
    /// does not determine: the pair maps back to its key only when exactly
    /// one source key has its view key as image and only that key carried
    /// it. More than one key is recorded exactly when the migration merged
    /// the pairs of several keys into one.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub original_fk_edges: BTreeMap<(Edge, RowId, RowId), BTreeSet<Edge>>,
    /// The source table of each kept row whose view table does not map back
    /// to it (the vertex map has several preimages for the view table, or
    /// one that is not the source table). An injective rename records
    /// nothing.
    pub original_tables: BTreeMap<RowId, Name>,
    /// The source cells of kept rows whose column the view row holds a
    /// cell of that name for with another value, by row and source column,
    /// with their source value.
    ///
    /// With [`dropped_cells`](Self::dropped_cells) it names every source
    /// column on which the view row and the source row disagree, so a put
    /// rebuilds the source row, column by column of its source table, from
    /// `dropped_cells`, else `original_cells`, else the view's cell of that
    /// name. Since the table restrict refuses a row whose cells are not
    /// exactly its table's columns, get paired with the complement is
    /// injective on the sources it accepts, value-dependent renames
    /// included.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub original_cells: BTreeMap<(RowId, String), Value>,
    /// The pairs contraction synthesized, each keyed in the graph form by
    /// `SynthKey::Contract { src: s.0, tgt: t.0, edge }`.
    pub synthesized_fk_pairs: BTreeSet<(Edge, RowId, RowId)>,
    /// The row id floor: one above the largest source row id, saturating
    /// at `u32::MAX`, and 0 for an empty source.
    pub id_floor: u32,
    /// The envelope binding the complement to a lens, once stamped.
    pub identity: Option<IdentityEnvelope>,
}

impl TableComplement {
    /// The complement of nothing: every map empty, floor zero, no envelope.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            dropped_rows: BTreeMap::new(),
            dropped_cells: BTreeMap::new(),
            dropped_fk_pairs: BTreeMap::new(),
            original_fk_edges: BTreeMap::new(),
            original_tables: BTreeMap::new(),
            original_cells: BTreeMap::new(),
            synthesized_fk_pairs: BTreeSet::new(),
            id_floor: 0,
            identity: None,
        }
    }

    /// Whether the complement is [`Self::empty`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::empty()
    }

    /// Whether the complement holds no residue: nothing dropped and no
    /// source data recorded. The synthesized pairs, the floor and the
    /// envelope are not residue.
    #[must_use]
    pub fn residue_is_trivial(&self) -> bool {
        self.dropped_rows.is_empty()
            && self.dropped_cells.is_empty()
            && self.dropped_fk_pairs.is_empty()
            && self.original_fk_edges.is_empty()
            && self.original_tables.is_empty()
            && self.original_cells.is_empty()
    }

    /// The row ids the complement reserves: those of the dropped rows.
    #[must_use]
    pub fn reserved_row_ids(&self) -> BTreeSet<RowId> {
        self.dropped_rows.keys().copied().collect()
    }

    /// The graph edge ids under which `embedding` presents the synthesized
    /// foreign-key pairs recorded by this complement.
    #[must_use]
    pub fn synth_index(&self, embedding: &TableEmbedding) -> SynthIndex {
        let edges = self
            .synthesized_fk_pairs
            .iter()
            .filter_map(|(edge, src, tgt)| {
                embedding
                    .pairs
                    .get(&(edge.clone(), *src, *tgt))
                    .copied()
                    .map(|id| {
                        (
                            id,
                            SynthKey::Contract {
                                src: src.0,
                                tgt: tgt.0,
                                edge: edge.clone(),
                            },
                        )
                    })
            })
            .collect();
        SynthIndex {
            nodes: BTreeMap::new(),
            edges,
        }
    }
}

impl Default for TableComplement {
    fn default() -> Self {
        Self::empty()
    }
}

impl IdentityCarrier for TableComplement {
    const SHAPE: ComplementShape = ComplementShape::Functor;

    fn identity(&self) -> Option<&IdentityEnvelope> {
        self.identity.as_ref()
    }

    fn set_identity(&mut self, envelope: Option<IdentityEnvelope>) {
        self.identity = envelope;
    }

    fn legacy_fingerprint(&self) -> u64 {
        0
    }

    fn set_legacy_fingerprint(&mut self, _fingerprint: u64) {}

    fn records_nothing(&self) -> bool {
        self.residue_is_trivial() && self.synthesized_fk_pairs.is_empty() && self.id_floor == 0
    }
}
