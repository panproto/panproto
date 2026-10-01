//! Set-valued functor instance representation.
//!
//! An [`FInstance`] represents relational (tabular) data as a set-valued
//! functor: each schema vertex maps to a table (a set of rows), and each
//! foreign-key edge maps to a relation between the rows of its endpoint
//! tables.
//!
//! Every row carries a [`RowId`] that is unique across the whole instance,
//! not per table, so a row is addressed by its id alone and keeps that id
//! when an operation moves it to another table. Foreign keys are sets of
//! `(source row, target row)` pairs, stored sorted and indexed by both ends,
//! so the pairs at one row are read without a scan. There is no positional
//! row reference anywhere: a row's place in a table is its id, and a table
//! enumerates its rows in ascending id.
//!
//! Both migration operations here carry an `S`-instance forward along a
//! migration `F: S -> T`, and they differ in what they do with the structure
//! `F` does not name. [`functor_restrict`] keeps the surviving fragment: a
//! vertex's rows land in its image's table, a vertex the migration drops
//! contributes none, and only the edges it remaps or lets survive carry their
//! foreign keys across. [`functor_extend`] keeps everything: a vertex the
//! migration says nothing about travels under its own name and merged rows are
//! padded to a common column set, and rows at a vertex the migration names
//! only as another vertex's image are refused rather than dropped. Both keep
//! every row's id, and both read the migration's source vertices as
//! [`crate::adjunction`] does.
//!
//! Neither is precomposition. Both take the coproduct over a fibre when the
//! vertex map merges two vertices, which is what `Sigma_F` does. `Delta_F`
//! runs the other way, from a `T`-instance to an `S`-instance, and lives in
//! [`crate::adjunction::f_delta`] with the rest of the adjunction.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{InstError, RestrictError};
use crate::identity::{CanonicalSink, Digest, TABLE_INSTANCE_CONTEXT, write_value_map};
use crate::segment::{IdAllocator, IdKind, IdRange};
use crate::value::Value;
use crate::view_ref::RowId;
use crate::wtype::CompiledMigration;

/// A row: its cells, by column name.
pub type Row = HashMap<String, Value>;

/// A foreign-key pair together with the schema edge it belongs to.
pub type ForeignKeyPair = (Edge, RowId, RowId);

/// The pairs of a foreign key: a set of `(source row, target row)`.
pub type PairSet = BTreeSet<(RowId, RowId)>;

/// The pairs of an edge that has none.
static NO_PAIRS: PairSet = BTreeSet::new();

/// The pairs of one foreign key, held by source and again by target, each
/// ascending, so that the pairs at either end of a row are one range read
/// and adding or removing a pair costs logarithmic time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ForeignKey {
    /// The pairs `(src, tgt)`, ascending.
    pairs: PairSet,
    /// The same pairs as `(tgt, src)`, ascending.
    by_target: PairSet,
}

impl ForeignKey {
    /// Add a pair; returns whether it was absent.
    fn insert(&mut self, src: RowId, tgt: RowId) -> bool {
        let added = self.pairs.insert((src, tgt));
        if added {
            self.by_target.insert((tgt, src));
        }
        added
    }

    /// Remove a pair; returns whether it was present.
    fn remove(&mut self, src: RowId, tgt: RowId) -> bool {
        let removed = self.pairs.remove(&(src, tgt));
        if removed {
            self.by_target.remove(&(tgt, src));
        }
        removed
    }

    /// The targets of the pairs whose source is `src`, ascending.
    fn targets(&self, src: RowId) -> impl DoubleEndedIterator<Item = RowId> + '_ {
        self.pairs
            .range((src, RowId(0))..=(src, RowId(u32::MAX)))
            .map(|&(_, tgt)| tgt)
    }

    /// The sources of the pairs whose target is `tgt`, ascending.
    fn sources(&self, tgt: RowId) -> impl DoubleEndedIterator<Item = RowId> + '_ {
        self.by_target
            .range((tgt, RowId(0))..=(tgt, RowId(u32::MAX)))
            .map(|&(_, src)| src)
    }
}

/// Whether two cells hold the same value: equal canonical bytes. Unlike
/// `Value`'s `==`, this is reflexive on `NaN` and tells `0.0` from `-0.0`,
/// so it agrees with the canonical bytes and digests of an instance.
pub(crate) fn same_value(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

/// Whether two rows hold the same columns with the same values
/// ([`same_value`]).
pub(crate) fn same_row(a: &Row, b: &Row) -> bool {
    a.len() == b.len()
        && a.iter().all(|(column, value)| {
            b.get(column)
                .is_some_and(|theirs| same_value(value, theirs))
        })
}

/// One table of an [`FInstance`]: its rows, by [`RowId`].
///
/// A table enumerates its rows in ascending id; nothing about a row depends
/// on the order in which it was inserted. Two tables are equal when they
/// hold the same ids with the same cells compared by canonical bytes, so
/// equality is an equivalence (a `NaN` cell equals itself) and agrees with
/// the canonical bytes (`0.0` and `-0.0` differ).
#[derive(Clone, Debug, Default)]
pub struct Table {
    rows: HashMap<RowId, Row>,
}

impl PartialEq for Table {
    fn eq(&self, other: &Self) -> bool {
        self.rows.len() == other.rows.len()
            && self.rows.iter().all(|(id, row)| {
                other
                    .rows
                    .get(id)
                    .is_some_and(|theirs| same_row(row, theirs))
            })
    }
}

impl Eq for Table {}

impl Table {
    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The row with this id, if the table holds it.
    #[must_use]
    pub fn get(&self, id: RowId) -> Option<&Row> {
        self.rows.get(&id)
    }

    /// Whether the table holds a row with this id.
    #[must_use]
    pub fn contains(&self, id: RowId) -> bool {
        self.rows.contains_key(&id)
    }

    /// The ids of the rows, ascending.
    #[must_use]
    pub fn ids(&self) -> Vec<RowId> {
        let mut ids: Vec<RowId> = self.rows.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// The rows with their ids, in ascending id.
    #[must_use]
    pub fn rows(&self) -> Vec<(RowId, &Row)> {
        let mut rows: Vec<(RowId, &Row)> = self.rows.iter().map(|(id, row)| (*id, row)).collect();
        rows.sort_unstable_by_key(|(id, _)| *id);
        rows
    }
}

/// A set-valued functor instance (relational data).
///
/// Tables are keyed by schema vertex; each row has a [`RowId`] unique across
/// the whole instance; each foreign key is keyed by its schema edge and holds
/// a sorted set of `(source row, target row)` pairs, the source row in the
/// edge's source table and the target row in its target table. Each set is
/// also indexed by target, so the pairs at a row are found in time
/// logarithmic in the pairs, and removing or moving a row reads only its
/// own pairs. A vertex
/// with no rows sends the functor to the empty set, so the instance holds a
/// table only while it has a row: an empty table and an absent one are the
/// same instance, with the same wire form, canonical bytes and graph form.
/// Equality compares cells by canonical bytes, as [`Table`]'s does, so it
/// is an equivalence and two instances are equal exactly when their
/// canonical bytes are.
/// The fields are private so that these invariants hold for every value:
/// the constructors and the decoder check them, and a private index from
/// row to table is rebuilt whenever an instance is decoded.
///
/// Serialized through a wire form with sorted maps (tables by name, rows by
/// id, cells by column, foreign keys as a list in ascending edge order), so
/// the bytes of one instance are the same in every process. Decoding fails
/// with [`InstError::DuplicateRowId`] when an id appears in two tables, with
/// [`InstError::DanglingForeignKey`] when a pair names a row its edge's
/// endpoint table does not hold, and with [`InstError::DuplicateForeignKey`]
/// when a pair is listed twice.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::{FInstance, RowId, Value};
/// use panproto_schema::Edge;
///
/// let author = Edge { src: "post".into(), tgt: "user".into(), kind: "fk".into(), name: None };
/// let mut tables = FInstance::new();
/// tables.insert_row("user", RowId(0), HashMap::from([("name".to_owned(), Value::Str("ada".into()))]))?;
/// tables.insert_row("post", RowId(1), HashMap::new())?;
/// tables.insert_foreign_key(author.clone(), RowId(1), RowId(0))?;
/// assert_eq!(tables.table_of(RowId(1)).map(AsRef::as_ref), Some("post"));
/// assert!(tables.pairs(&author).iter().eq(&[(RowId(1), RowId(0))]));
/// assert_eq!(tables.fresh_row_id()?, RowId(2));
/// # Ok::<(), panproto_inst::InstError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FInstance {
    tables: HashMap<Name, Table>,
    /// The pairs of each foreign key with at least one pair.
    foreign_keys: HashMap<Edge, ForeignKey>,
    /// The table of each row, ordered by id so that the largest id in use is
    /// read in logarithmic time, however rows are added or removed.
    row_table: BTreeMap<RowId, Name>,
}

/// The allocator of fresh row ids above every id in `ids`: its floor is
/// `max(ids) + 1`, and `0` when `ids` is empty.
pub(crate) fn row_allocator(ids: impl IntoIterator<Item = RowId>) -> IdAllocator {
    IdAllocator::from_source(IdRange::FULL, ids.into_iter().map(|id| id.0))
}

/// Draw one fresh row id from `allocator`.
///
/// # Errors
///
/// Returns [`InstError::NodeIdsExhausted`] when no row id is free above the
/// floor (rows are the nodes of a table's graph form).
pub(crate) fn allocate_row(allocator: &mut IdAllocator) -> Result<RowId, InstError> {
    allocator
        .allocate()
        .map(RowId)
        .map_err(|e| e.whole_state(IdKind::Node))
}

impl FInstance {
    /// Create a new empty functor instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build an instance from its tables and foreign keys, checking every
    /// invariant; this is also the decoder's path. A table listed without
    /// rows contributes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DuplicateRowId`] when one id appears in two
    /// tables (or twice in one), [`InstError::DanglingForeignKey`] when a pair
    /// names a row its edge's endpoint table does not hold, and
    /// [`InstError::DuplicateForeignKey`] when a pair is listed twice.
    pub fn from_parts(
        tables: impl IntoIterator<Item = (Name, Vec<(RowId, Row)>)>,
        foreign_keys: impl IntoIterator<Item = (Edge, Vec<(RowId, RowId)>)>,
    ) -> Result<Self, InstError> {
        let mut holders: BTreeMap<RowId, Vec<Name>> = BTreeMap::new();
        let mut staged: Vec<(Name, Vec<(RowId, Row)>)> = Vec::new();
        for (name, rows) in tables {
            for (id, _) in &rows {
                holders.entry(*id).or_default().push(name.clone());
            }
            staged.push((name, rows));
        }
        if let Some((row, mut tables)) = holders.into_iter().find(|(_, t)| t.len() > 1) {
            tables.sort_unstable();
            return Err(InstError::DuplicateRowId { row, tables });
        }
        let mut instance = Self::new();
        for (name, rows) in staged {
            for (id, row) in rows {
                instance.place_row(name.clone(), id, row);
            }
        }
        let mut keys: Vec<(Edge, Vec<(RowId, RowId)>)> = foreign_keys.into_iter().collect();
        keys.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (edge, pairs) in keys {
            instance.insert_foreign_keys(edge, pairs)?;
        }
        Ok(instance)
    }

    /// Add fresh rows to the given vertex's table, numbering them from the
    /// least id above every row already in the instance, in the order given.
    /// With no rows the instance is unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdsExhausted`] when the ids run out.
    pub fn with_table(
        mut self,
        vertex: impl Into<Name>,
        rows: impl IntoIterator<Item = Row>,
    ) -> Result<Self, InstError> {
        let vertex = vertex.into();
        for row in rows {
            self.push_row(vertex.clone(), row)?;
        }
        Ok(self)
    }

    /// Add rows with the given ids to the given vertex's table.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DuplicateRowId`] when an id is already in use.
    pub fn with_rows(
        mut self,
        vertex: impl Into<Name>,
        rows: impl IntoIterator<Item = (RowId, Row)>,
    ) -> Result<Self, InstError> {
        let vertex = vertex.into();
        for (id, row) in rows {
            self.insert_row(vertex.clone(), id, row)?;
        }
        Ok(self)
    }

    /// Add the given pairs to the foreign key of `edge`.
    ///
    /// # Errors
    ///
    /// As [`Self::insert_foreign_key`].
    pub fn with_foreign_key(
        mut self,
        edge: Edge,
        pairs: impl IntoIterator<Item = (RowId, RowId)>,
    ) -> Result<Self, InstError> {
        let pairs: Vec<(RowId, RowId)> = pairs.into_iter().collect();
        if let Some((&(src, tgt), rest)) = pairs.split_last() {
            for &(s, t) in rest {
                self.insert_foreign_key(edge.clone(), s, t)?;
            }
            self.insert_foreign_key(edge, src, tgt)?;
        }
        Ok(self)
    }

    /// Returns the number of tables, that is of vertices with at least one
    /// row.
    #[must_use]
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    /// Returns the number of rows in a specific table.
    #[must_use]
    pub fn row_count(&self, vertex: &str) -> usize {
        self.tables.get(vertex).map_or(0, Table::len)
    }

    /// Returns the number of rows in the whole instance.
    #[must_use]
    pub fn total_row_count(&self) -> usize {
        self.row_table.len()
    }

    /// Returns the number of foreign-key pairs in the whole instance.
    #[must_use]
    pub fn pair_count(&self) -> usize {
        self.foreign_keys.values().map(|fk| fk.pairs.len()).sum()
    }

    /// The table names, ascending.
    #[must_use]
    pub fn table_names(&self) -> Vec<&Name> {
        let mut names: Vec<&Name> = self.tables.keys().collect();
        names.sort_unstable();
        names
    }

    /// The tables with their names, in ascending name order.
    #[must_use]
    pub fn tables(&self) -> Vec<(&Name, &Table)> {
        let mut tables: Vec<(&Name, &Table)> = self.tables.iter().collect();
        tables.sort_unstable_by(|a, b| a.0.cmp(b.0));
        tables
    }

    /// The table of the given vertex, if the instance has one.
    #[must_use]
    pub fn table(&self, vertex: &str) -> Option<&Table> {
        self.tables.get(vertex)
    }

    /// Whether the vertex has a table, that is at least one row.
    #[must_use]
    pub fn has_table(&self, vertex: &str) -> bool {
        self.tables.contains_key(vertex)
    }

    /// The rows of the given vertex's table with their ids, in ascending id;
    /// empty when there is no such table.
    #[must_use]
    pub fn rows(&self, vertex: &str) -> Vec<(RowId, &Row)> {
        self.tables.get(vertex).map_or_else(Vec::new, Table::rows)
    }

    /// The row with this id, in whichever table holds it.
    #[must_use]
    pub fn row(&self, id: RowId) -> Option<&Row> {
        let table = self.row_table.get(&id)?;
        self.tables.get(table)?.get(id)
    }

    /// The row with this id, mutably.
    pub fn row_mut(&mut self, id: RowId) -> Option<&mut Row> {
        let table = self.row_table.get(&id)?;
        self.tables.get_mut(table)?.rows.get_mut(&id)
    }

    /// The table holding the row with this id.
    #[must_use]
    pub fn table_of(&self, id: RowId) -> Option<&Name> {
        self.row_table.get(&id)
    }

    /// Every row id of the instance, ascending.
    #[must_use]
    pub fn row_ids(&self) -> Vec<RowId> {
        self.row_table.keys().copied().collect()
    }

    /// The largest row id in use, if any.
    #[must_use]
    pub fn max_row_id(&self) -> Option<RowId> {
        self.row_table.last_key_value().map(|(&id, _)| id)
    }

    /// The least row id above every row id in use: `max + 1`, and `0` for an
    /// instance without rows.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdsExhausted`] when the largest id in use is
    /// `u32::MAX`.
    pub fn fresh_row_id(&self) -> Result<RowId, InstError> {
        allocate_row(&mut row_allocator(self.max_row_id()))
    }

    /// Insert a row with the given id into the given vertex's table.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DuplicateRowId`] when the id is already in use,
    /// naming every table involved.
    pub fn insert_row(
        &mut self,
        vertex: impl Into<Name>,
        id: RowId,
        row: Row,
    ) -> Result<(), InstError> {
        let vertex = vertex.into();
        if let Some(existing) = self.row_table.get(&id) {
            let mut tables = vec![existing.clone(), vertex];
            tables.sort_unstable();
            return Err(InstError::DuplicateRowId { row: id, tables });
        }
        self.place_row(vertex, id, row);
        Ok(())
    }

    /// Insert a row under a fresh id (`max + 1`) and return the id.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdsExhausted`] when the ids run out.
    pub fn push_row(&mut self, vertex: impl Into<Name>, row: Row) -> Result<RowId, InstError> {
        let id = self.fresh_row_id()?;
        self.place_row(vertex.into(), id, row);
        Ok(id)
    }

    /// Insert a row whose id the caller knows to be unused.
    pub(crate) fn place_row(&mut self, vertex: Name, id: RowId, row: Row) {
        debug_assert!(
            !self.row_table.contains_key(&id),
            "place_row: row {} is already in table {:?}",
            id.0,
            self.row_table.get(&id)
        );
        self.row_table.insert(id, vertex.clone());
        self.tables.entry(vertex).or_default().rows.insert(id, row);
    }

    /// Take a row out of its table, dropping the table when it empties, and
    /// out of the row index. Pairs are left to the caller.
    fn take_row(&mut self, id: RowId) -> Option<(Name, Row)> {
        let table_name = self.row_table.remove(&id)?;
        let table = self.tables.get_mut(&table_name)?;
        let row = table.rows.remove(&id)?;
        if table.rows.is_empty() {
            self.tables.remove(&table_name);
        }
        Some((table_name, row))
    }

    /// Remove a row together with every foreign-key pair incident to it.
    /// Returns the row's table, its cells and the removed pairs (ascending),
    /// or `None` when no row has this id. A table whose last row is removed
    /// is no longer listed.
    ///
    /// Reads only the pairs incident to the row, so its cost is logarithmic
    /// in the instance's rows and pairs, linear in its own pairs, and linear
    /// in the foreign keys that have a pair.
    pub fn remove_row(&mut self, id: RowId) -> Option<(Name, Row, Vec<ForeignKeyPair>)> {
        let incident = self.incident_pairs(id);
        let (table, row) = self.take_row(id)?;
        for (edge, src, tgt) in &incident {
            self.remove_foreign_key(edge, *src, *tgt);
        }
        Some((table, row, incident))
    }

    /// Move a row to another table, keeping its id and cells, and return the
    /// table it left, which is no longer listed when the row was its last.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeNotFound`] with the id when no row has it
    /// (rows are the nodes of a table's graph form), and
    /// [`InstError::RowHasForeignKeys`] naming the first incident pair when
    /// the row takes part in a foreign key, since the pair would no longer
    /// join its edge's endpoint tables; the instance is then unchanged.
    pub fn move_row(&mut self, id: RowId, vertex: impl Into<Name>) -> Result<Name, InstError> {
        let vertex = vertex.into();
        let old = self
            .row_table
            .get(&id)
            .cloned()
            .ok_or(InstError::NodeNotFound(id.0))?;
        if old == vertex {
            return Ok(old);
        }
        if let Some((edge, src, tgt)) = self.incident_pairs(id).into_iter().next() {
            return Err(InstError::RowHasForeignKeys {
                row: id,
                from: old,
                to: vertex,
                edge,
                src,
                tgt,
            });
        }
        let (_, row) = self.take_row(id).ok_or(InstError::NodeNotFound(id.0))?;
        self.place_row(vertex, id, row);
        Ok(old)
    }
}

impl FInstance {
    /// Add the pair `(src, tgt)` to the foreign key of `edge`, keeping the
    /// pair list sorted.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DanglingForeignKey`] when `src` is not a row of
    /// `edge.src`'s table or `tgt` is not a row of `edge.tgt`'s table, and
    /// [`InstError::DuplicateForeignKey`] when the pair is already present
    /// (a foreign key is a set of pairs).
    pub fn insert_foreign_key(
        &mut self,
        edge: Edge,
        src: RowId,
        tgt: RowId,
    ) -> Result<(), InstError> {
        let dangling = [(src, &edge.src), (tgt, &edge.tgt)]
            .into_iter()
            .find(|(row, table)| self.row_table.get(row) != Some(*table))
            .map(|(row, _)| row);
        if let Some(row) = dangling {
            return Err(InstError::DanglingForeignKey { edge, row });
        }
        if self.contains_pair(&edge, src, tgt) {
            return Err(InstError::DuplicateForeignKey { edge, src, tgt });
        }
        self.foreign_keys.entry(edge).or_default().insert(src, tgt);
        Ok(())
    }

    /// Add every pair of `pairs` to the foreign key of `edge`, in time
    /// `O(n log (n + p))` in the `n` pairs given and the `p` present,
    /// whatever their order.
    ///
    /// # Errors
    ///
    /// As [`Self::insert_foreign_key`], naming the least offending pair; on
    /// an error no pair is added.
    pub fn insert_foreign_keys(
        &mut self,
        edge: Edge,
        mut pairs: Vec<(RowId, RowId)>,
    ) -> Result<(), InstError> {
        pairs.sort_unstable();
        for &(src, tgt) in &pairs {
            if self.row_table.get(&src) != Some(&edge.src) {
                return Err(InstError::DanglingForeignKey { edge, row: src });
            }
            if self.row_table.get(&tgt) != Some(&edge.tgt) {
                return Err(InstError::DanglingForeignKey { edge, row: tgt });
            }
        }
        if let Some(pair) = pairs.windows(2).find(|w| w[0] == w[1]).map(|w| w[0]) {
            return Err(InstError::DuplicateForeignKey {
                edge,
                src: pair.0,
                tgt: pair.1,
            });
        }
        if let Some(&(src, tgt)) = pairs
            .iter()
            .find(|(src, tgt)| self.contains_pair(&edge, *src, *tgt))
        {
            return Err(InstError::DuplicateForeignKey { edge, src, tgt });
        }
        if pairs.is_empty() {
            return Ok(());
        }
        let key = self.foreign_keys.entry(edge).or_default();
        for (src, tgt) in pairs {
            key.insert(src, tgt);
        }
        Ok(())
    }

    /// Add a pair whose endpoints the caller knows to be rows of the edge's
    /// endpoint tables; a pair already present is left as it is. Returns
    /// whether the pair was added.
    pub(crate) fn place_pair(&mut self, edge: Edge, src: RowId, tgt: RowId) -> bool {
        self.foreign_keys.entry(edge).or_default().insert(src, tgt)
    }

    /// Whether `(src, tgt)` would join the endpoint tables of `edge`.
    pub(crate) fn joins(&self, edge: &Edge, src: RowId, tgt: RowId) -> bool {
        self.row_table.get(&src) == Some(&edge.src) && self.row_table.get(&tgt) == Some(&edge.tgt)
    }

    /// Remove the pair `(src, tgt)` from the foreign key of `edge`. Returns
    /// whether it was present.
    pub fn remove_foreign_key(&mut self, edge: &Edge, src: RowId, tgt: RowId) -> bool {
        let Some(key) = self.foreign_keys.get_mut(edge) else {
            return false;
        };
        if !key.remove(src, tgt) {
            return false;
        }
        if key.pairs.is_empty() {
            self.foreign_keys.remove(edge);
        }
        true
    }

    /// Whether the foreign key of `edge` holds `(src, tgt)`.
    #[must_use]
    pub fn contains_pair(&self, edge: &Edge, src: RowId, tgt: RowId) -> bool {
        self.foreign_keys
            .get(edge)
            .is_some_and(|key| key.pairs.contains(&(src, tgt)))
    }

    /// The pairs of the foreign key of `edge`, a set iterating in ascending
    /// order; empty when the edge has none.
    #[must_use]
    pub fn pairs(&self, edge: &Edge) -> &PairSet {
        self.foreign_keys
            .get(edge)
            .map_or(&NO_PAIRS, |key| &key.pairs)
    }

    /// The targets of the pairs of `edge` whose source is `src`, ascending,
    /// read in time logarithmic in the edge's pairs.
    #[must_use]
    pub fn targets_of(
        &self,
        edge: &Edge,
        src: RowId,
    ) -> impl DoubleEndedIterator<Item = RowId> + '_ {
        self.foreign_keys
            .get(edge)
            .into_iter()
            .flat_map(move |key| key.targets(src))
    }

    /// The sources of the pairs of `edge` whose target is `tgt`, ascending,
    /// read in time logarithmic in the edge's pairs.
    #[must_use]
    pub fn sources_of(
        &self,
        edge: &Edge,
        tgt: RowId,
    ) -> impl DoubleEndedIterator<Item = RowId> + '_ {
        self.foreign_keys
            .get(edge)
            .into_iter()
            .flat_map(move |key| key.sources(tgt))
    }

    /// The foreign keys with their pairs, in ascending edge order. An edge
    /// without pairs is not listed.
    #[must_use]
    pub fn foreign_keys(&self) -> Vec<(&Edge, &PairSet)> {
        let mut keys: Vec<(&Edge, &PairSet)> = self
            .foreign_keys
            .iter()
            .filter(|(_, key)| !key.pairs.is_empty())
            .map(|(edge, key)| (edge, &key.pairs))
            .collect();
        keys.sort_unstable_by(|a, b| a.0.cmp(b.0));
        keys
    }

    /// Every foreign-key pair with its edge, in ascending `(edge, src, tgt)`.
    #[must_use]
    pub fn all_pairs(&self) -> Vec<ForeignKeyPair> {
        self.foreign_keys()
            .into_iter()
            .flat_map(|(edge, pairs)| pairs.iter().map(move |&(s, t)| (edge.clone(), s, t)))
            .collect()
    }

    /// The foreign-key pairs with the given row at either end, in ascending
    /// `(edge, src, tgt)`; empty when no row has this id.
    ///
    /// A pair joins its edge's endpoint tables, so only the foreign keys
    /// leaving or entering the row's table are read, each by one range of
    /// its source or target index: the cost is linear in the foreign keys
    /// with a pair and in the row's own pairs, and logarithmic in the rest.
    #[must_use]
    pub fn incident_pairs(&self, id: RowId) -> Vec<ForeignKeyPair> {
        let Some(table) = self.row_table.get(&id) else {
            return Vec::new();
        };
        let mut incident: Vec<ForeignKeyPair> = Vec::new();
        for (edge, key) in &self.foreign_keys {
            if edge.src == *table {
                incident.extend(key.targets(id).map(|tgt| (edge.clone(), id, tgt)));
            }
            if edge.tgt == *table {
                incident.extend(key.sources(id).map(|src| (edge.clone(), src, id)));
            }
        }
        incident.sort_unstable();
        incident.dedup();
        incident
    }

    /// The single row of `vertex`'s table whose cells agree with every
    /// `(column, value)` of `key`, cells compared by canonical bytes (so a
    /// `NaN` key finds a row holding `NaN`, and `0.0` does not find `-0.0`).
    ///
    /// A key is data: this is a lookup helper, and every operation addresses
    /// rows by [`RowId`].
    ///
    /// # Errors
    ///
    /// Returns [`KeyLookupError::Missing`] when no row matches (a table
    /// without rows included) and
    /// [`KeyLookupError::Duplicate`] (with every matching id, ascending) when
    /// more than one does.
    pub fn find_row_by_key(
        &self,
        vertex: &str,
        key: &[(&str, &Value)],
    ) -> Result<RowId, KeyLookupError> {
        let matches: Vec<RowId> = self
            .rows(vertex)
            .into_iter()
            .filter(|(_, row)| {
                key.iter()
                    .all(|(col, val)| row.get(*col).is_some_and(|cell| same_value(cell, val)))
            })
            .map(|(id, _)| id)
            .collect();
        match matches.as_slice() {
            [] => Err(KeyLookupError::Missing {
                table: Name::from(vertex),
            }),
            [id] => Ok(*id),
            _ => Err(KeyLookupError::Duplicate {
                table: Name::from(vertex),
                rows: matches,
            }),
        }
    }
}

/// Why a key lookup ([`FInstance::find_row_by_key`]) found no single row.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum KeyLookupError {
    /// No row of the table carries the key.
    #[error("no row of table `{table}` carries the key")]
    Missing {
        /// The table searched.
        table: Name,
    },
    /// More than one row of the table carries the key.
    #[error("rows {rows:?} of table `{table}` all carry the key")]
    Duplicate {
        /// The table searched.
        table: Name,
        /// Every row carrying the key, ascending.
        rows: Vec<RowId>,
    },
}

/// The wire form of an [`FInstance`], borrowed for encoding.
#[derive(Serialize)]
struct WireOut<'a> {
    tables: BTreeMap<&'a Name, BTreeMap<RowId, BTreeMap<&'a String, &'a Value>>>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    foreign_keys: BTreeMap<&'a Edge, &'a PairSet>,
}

/// The wire form of an [`FInstance`], owned for decoding.
#[derive(Deserialize)]
struct WireIn {
    tables: Entries<Name, Entries<RowId, Entries<String, Value>>>,
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    foreign_keys: BTreeMap<Edge, Vec<(RowId, RowId)>>,
}

/// The entries of a serialized map in the order they were read, repeated
/// keys included, so that the decoder can refuse a repeat instead of letting
/// the last entry win.
struct Entries<K, V>(Vec<(K, V)>);

impl<'de, K: Deserialize<'de>, V: Deserialize<'de>> Deserialize<'de> for Entries<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EntriesVisitor<K, V>(std::marker::PhantomData<(K, V)>);

        impl<'de, K: Deserialize<'de>, V: Deserialize<'de>> serde::de::Visitor<'de>
            for EntriesVisitor<K, V>
        {
            type Value = Entries<K, V>;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a map")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }

        deserializer.deserialize_map(EntriesVisitor(std::marker::PhantomData))
    }
}

/// The first key listed twice among `keys`, if any.
fn repeated_key<'a, K: Ord + 'a>(keys: impl IntoIterator<Item = &'a K>) -> Option<&'a K> {
    let mut seen = std::collections::BTreeSet::new();
    keys.into_iter().find(|key| !seen.insert(*key))
}

impl Serialize for FInstance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let tables = self
            .tables
            .iter()
            .map(|(name, table)| {
                let rows = table
                    .rows
                    .iter()
                    .map(|(id, row)| (*id, row.iter().collect()))
                    .collect();
                (name, rows)
            })
            .collect();
        let foreign_keys = self.foreign_keys().into_iter().collect();
        WireOut {
            tables,
            foreign_keys,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FInstance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireIn::deserialize(deserializer)?;
        if let Some(name) = repeated_key(wire.tables.0.iter().map(|(name, _)| name)) {
            return Err(serde::de::Error::custom(format!(
                "table `{name}` is listed twice"
            )));
        }
        let mut tables = Vec::with_capacity(wire.tables.0.len());
        for (name, rows) in wire.tables.0 {
            let mut decoded = Vec::with_capacity(rows.0.len());
            for (id, cells) in rows.0 {
                if let Some(column) = repeated_key(cells.0.iter().map(|(column, _)| column)) {
                    return Err(serde::de::Error::custom(format!(
                        "column `{column}` is listed twice in row {}",
                        id.0
                    )));
                }
                decoded.push((id, cells.0.into_iter().collect()));
            }
            tables.push((name, decoded));
        }
        Self::from_parts(tables, wire.foreign_keys).map_err(serde::de::Error::custom)
    }
}

/// Write the record of one row: its id, its table and its cells in ascending
/// column order. See [`canonical_finstance_bytes`] for the layout.
pub fn write_row_record<S: CanonicalSink + ?Sized>(
    sink: &mut S,
    id: RowId,
    table: &Name,
    row: &Row,
) {
    sink.u32(id.0);
    sink.name(table);
    write_value_map(sink, row);
}

/// Write the record of one foreign-key pair: its edge, then its source and
/// target rows. See [`canonical_finstance_bytes`] for the layout.
pub fn write_pair_record<S: CanonicalSink + ?Sized>(
    sink: &mut S,
    edge: &Edge,
    src: RowId,
    tgt: RowId,
) {
    sink.edge(edge);
    sink.u32(src.0);
    sink.u32(tgt.0);
}

/// Write a table instance's canonical bytes into `sink`: the header, then
/// every row record in ascending [`RowId`], then every pair record in
/// ascending `(edge, src, tgt)`.
pub fn write_finstance<S: CanonicalSink + ?Sized>(sink: &mut S, instance: &FInstance) {
    sink.len(instance.total_row_count());
    sink.len(instance.pair_count());
    for id in instance.row_ids() {
        if let (Some(table), Some(row)) = (instance.table_of(id), instance.row(id)) {
            write_row_record(sink, id, table, row);
        }
    }
    for (edge, pairs) in instance.foreign_keys() {
        for &(src, tgt) in pairs {
            write_pair_record(sink, edge, src, tgt);
        }
    }
}

/// The canonical bytes of a table instance, as [`write_finstance`] writes
/// them.
///
/// The bytes are a header and then self-contained records:
///
/// ```text
/// header = row_count: u64, pair_count: u64
/// row    = id: u32, table, cells            (ascending RowId)
/// pair   = edge, src: u32, tgt: u32         (ascending (edge, src, tgt))
/// ```
///
/// Every field is written through a [`CanonicalSink`] with the sink's own
/// encoders: integers little-endian, names length-prefixed, cells as their
/// count and then each `(column, value)` in ascending column order with the
/// value by [`CanonicalSink::value`], an edge as source, target, kind and
/// optional name. No record refers to a position: a row is named by its id,
/// which the instance carries as data, so the digest can be computed by
/// streaming the records into a [`DigestHasher`](crate::DigestHasher). A
/// table without rows writes nothing: the bytes are those of the rows and
/// pairs.
#[must_use]
pub fn canonical_finstance_bytes(instance: &FInstance) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    write_finstance(&mut writer, instance);
    writer.into_bytes()
}

impl FInstance {
    /// The digest of this table instance: its canonical bytes
    /// ([`canonical_finstance_bytes`]) under [`TABLE_INSTANCE_CONTEXT`],
    /// computed by streaming the records into the hasher.
    ///
    /// ```
    /// use panproto_inst::identity::{Digest, TABLE_INSTANCE_CONTEXT};
    /// use panproto_inst::{FInstance, canonical_finstance_bytes};
    ///
    /// let tables = FInstance::new().with_table("item", [Default::default()])?;
    /// assert_eq!(
    ///     tables.digest(),
    ///     Digest::derive(TABLE_INSTANCE_CONTEXT, &canonical_finstance_bytes(&tables)),
    /// );
    /// # Ok::<(), panproto_inst::InstError>(())
    /// ```
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut hasher = Digest::hasher(TABLE_INSTANCE_CONTEXT);
        write_finstance(&mut hasher, self);
        hasher.finalize()
    }
}

/// Carry a set-valued functor instance forward along a migration, keeping
/// the fragment that survives.
///
/// The source vertices are the migration's domain, read as every table
/// operation reads it: the vertices `vertex_remap` renames, together with
/// those `surviving_verts` lists that it neither renames nor names as some
/// other vertex's image. A vertex outside the domain contributes no rows; in
/// particular a vertex another vertex is renamed onto is a target, so the
/// rows it holds under its own name are not carried. Each source vertex
/// sends its rows to its image, the vertex `vertex_remap` names for it or
/// the vertex itself when it is not renamed, and they are kept exactly when
/// that image is in `surviving_verts` (the rule
/// [`crate::ginstance::GInstance`]'s restrict applies to nodes). A kept
/// row keeps its [`RowId`] and is rewritten by its source vertex's
/// op-to-term assignments. So every row lands at most once, even when a
/// renamed vertex's old name is itself listed as surviving (as the
/// migrations `panproto-lens` builds for a rename list it). The source edges
/// are read the same way: the edges `edge_remap` remaps, together with the
/// surviving edges it neither remaps nor names as another edge's image. Each
/// travels to its image, the edge
/// `edge_remap` names or, for a surviving edge, the edge of its kind and name
/// between the images of its endpoints; the pairs of every other edge are not
/// carried. A pair survives exactly when its edge's image joins surviving
/// vertices and both of its rows landed in that image's endpoint tables.
/// Pairs from source edges the migration identifies are unioned (a foreign
/// key is a set).
///
/// This is a pushforward, not precomposition: over a vertex map that merges
/// two vertices it returns the fibre's disjoint union, which is `Sigma_F`'s
/// answer; the row ids keep the union disjoint without any renumbering. On
/// an instance whose rows all lie in the domain it agrees with
/// [`crate::adjunction::f_sigma`] whenever every image survives; where
/// `f_sigma` refuses rows or pairs outside the domain, this drops them, as it
/// drops every row whose image does not survive. For `Delta_F` see
/// [`crate::adjunction::f_delta`].
///
/// # Errors
///
/// Returns [`RestrictError::NoEdgeFound`], naming the images of the edge's
/// endpoints, when the migration carries an edge to an image that does not
/// join them (it states no functor, and the edge's pairs would otherwise be
/// dropped unasked), and `RestrictError` if a row's op-to-term assignment
/// fails to evaluate.
pub fn functor_restrict(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    crate::adjunction::check_edge_endpoints(migration)?;
    let domain = crate::adjunction::source_vertices(migration);
    let mut out = FInstance::new();
    for (src_vertex, table) in instance.tables() {
        if !domain.contains(src_vertex) {
            continue;
        }
        let tgt_vertex = migration.vertex_remap.get(src_vertex).unwrap_or(src_vertex);
        if !migration.surviving_verts.contains(tgt_vertex) {
            continue;
        }
        let assignments = migration.op_term_assignments.get(src_vertex);
        for (id, row) in table.rows() {
            let mut new_row = row.clone();
            if let Some(assignments) = assignments {
                crate::wtype::apply_term_assignments_to_row(&mut new_row, assignments)?;
            }
            // A row id is in exactly one source table, and each source
            // table goes to one image, so the id is unused in `out`.
            out.place_row(tgt_vertex.clone(), id, new_row);
        }
    }

    let edge_images = crate::adjunction::edge_images(migration);
    for (edge, pairs) in instance.foreign_keys() {
        let Some(new_edge) = edge_images.get(edge) else {
            continue;
        };
        if !migration.surviving_verts.contains(&new_edge.src)
            || !migration.surviving_verts.contains(&new_edge.tgt)
        {
            continue;
        }
        for &(s, t) in pairs {
            if out.joins(new_edge, s, t) {
                out.place_pair(new_edge.clone(), s, t);
            }
        }
    }

    Ok(out)
}

/// The extend operation for set-valued functor instances (`Sigma_F`).
///
/// This is the left Kan extension: given an instance of the source schema
/// and a migration mapping (source -> target), produce an instance of the
/// target schema by moving every table to its image. A vertex in the
/// migration's domain (read as [`functor_restrict`] and every other table
/// operation read it) goes to the image `vertex_remap` names, or keeps its
/// name when it survives unrenamed; a vertex the migration does not mention
/// at all keeps its name. A vertex the migration names only as a target,
/// the image of some other vertex that it does not rename itself, has no
/// image of its own, and extend refuses rows held there rather than merge
/// them into that fibre unasked; a surviving target vertex no row reaches
/// is sent to the empty set, so it has no table. Rows keep their [`RowId`]s,
/// so the coproduct over a merging fibre is a disjoint union with no
/// renumbering, and the operation creates no rows. Within each target table
/// the rows are padded to the union of their columns with [`Value::Null`].
/// Edges are read the same way: a source edge (as [`functor_restrict`] reads
/// them) travels to its image, an edge the migration does not mention keeps
/// its kind and name between the images of its endpoints, and an edge the
/// migration names only as a target, the image of some other edge that it
/// does not remap itself, has no image of its own, so extend refuses pairs
/// held there. Relabelling an unmentioned edge can land it on an edge that
/// something else already reaches: the image of a source edge (whether or
/// not that source edge holds pairs), or the relabelling of another
/// unmentioned edge the instance holds pairs on. Extend refuses such an edge
/// too rather than merge its pairs there unasked; the only merges it makes
/// are those of source edges that `edge_remap` sends to one image. An
/// unmentioned edge whose relabelling is a source edge the migration remaps
/// away is no merge, so it travels. A pair is kept when its rows lie in the
/// carried edge's endpoint tables.
///
/// # Errors
///
/// Returns [`RestrictError::VertexOutsideDomain`] when a row lies at a
/// vertex the migration names only as a target,
/// [`RestrictError::EdgeOutsideDomain`] when a pair lies on an edge the
/// migration names only as a target, or on an unmentioned edge whose
/// relabelling some other edge already reaches (of two unmentioned edges
/// that collide, the later in edge order is named),
/// [`RestrictError::NoEdgeFound`], naming the images of the edge's
/// endpoints, when the migration carries an edge to an image that does not
/// join them (it states no functor), and `RestrictError` if a row's
/// op-to-term assignment fails to evaluate.
pub fn functor_extend(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    crate::adjunction::check_edge_endpoints(migration)?;
    let domain = crate::adjunction::source_vertices(migration);
    let images: std::collections::HashSet<&Name> = migration.vertex_remap.values().collect();
    let mut out = FInstance::new();
    for (src_vertex, table) in instance.tables() {
        if !domain.contains(src_vertex) && images.contains(src_vertex) {
            return Err(RestrictError::VertexOutsideDomain {
                vertex: src_vertex.clone(),
            });
        }
        let tgt_vertex = migration
            .vertex_remap
            .get(src_vertex)
            .cloned()
            .unwrap_or_else(|| src_vertex.clone());
        let assignments = migration.op_term_assignments.get(src_vertex);
        for (id, row) in table.rows() {
            let mut new_row = row.clone();
            if let Some(assignments) = assignments {
                crate::wtype::apply_term_assignments_to_row(&mut new_row, assignments)?;
            }
            out.place_row(tgt_vertex.clone(), id, new_row);
        }
    }

    for table in out.tables.values_mut() {
        let columns: std::collections::BTreeSet<String> = table
            .rows
            .values()
            .flat_map(|row| row.keys().cloned())
            .collect();
        for row in table.rows.values_mut() {
            for column in &columns {
                row.entry(column.clone()).or_insert(Value::Null);
            }
        }
    }

    let edge_images = crate::adjunction::edge_images(migration);
    let target_only = crate::adjunction::target_only_edges(migration);
    // Every edge the migration reaches is claimed by the migration itself,
    // whether or not the source edges reaching it hold pairs; an unmentioned
    // edge claims its relabelling only when that edge is still free.
    let mut claimed: HashMap<Edge, Edge> = edge_images
        .iter()
        .map(|(source, image)| (image.clone(), source.clone()))
        .collect();
    for edge in &target_only {
        claimed
            .entry((*edge).clone())
            .or_insert_with(|| (*edge).clone());
    }
    let foreign_keys = instance.foreign_keys();
    let mut carried: Vec<(Edge, &PairSet)> = Vec::with_capacity(foreign_keys.len());
    for (edge, pairs) in foreign_keys {
        let new_edge = match edge_images.get(edge) {
            Some(image) => image.clone(),
            None if target_only.contains(edge) => {
                return Err(RestrictError::EdgeOutsideDomain { edge: edge.clone() });
            }
            None => {
                let relabelled = crate::adjunction::map_edge(migration, edge);
                if claimed.contains_key(&relabelled) {
                    return Err(RestrictError::EdgeOutsideDomain { edge: edge.clone() });
                }
                claimed.insert(relabelled.clone(), edge.clone());
                relabelled
            }
        };
        carried.push((new_edge, pairs));
    }
    for (new_edge, pairs) in carried {
        for &(s, t) in pairs {
            if out.joins(&new_edge, s, t) {
                out.place_pair(new_edge.clone(), s, t);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::wtype::{TermAssignment, TermScope};

    /// A `full = first ++ " " ++ last` computed-column term.
    fn full_name_term() -> panproto_expr::Expr {
        use panproto_expr::{BuiltinOp, Expr, Literal};
        use std::sync::Arc;
        Expr::Builtin(
            BuiltinOp::Concat,
            vec![
                Expr::Var(Arc::from("first")),
                Expr::Builtin(
                    BuiltinOp::Concat,
                    vec![
                        Expr::Lit(Literal::Str(" ".into())),
                        Expr::Var(Arc::from("last")),
                    ],
                ),
            ],
        )
    }

    fn person_row() -> Row {
        let mut row = HashMap::new();
        row.insert("first".to_string(), Value::Str("Ada".into()));
        row.insert("last".to_string(), Value::Str("Lovelace".into()));
        row
    }

    fn computed_full_migration() -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert("person".into());
        migration.op_term_assignments.insert(
            "person".into(),
            vec![TermAssignment::Compute {
                target: "full".into(),
                scope: TermScope::Row,
                term: full_name_term(),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            }],
        );
        migration
    }

    fn cell(column: &str, value: &str) -> Row {
        HashMap::from([(column.to_owned(), Value::Str(value.into()))])
    }

    fn fk(src: &str, tgt: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some("link".into()),
        }
    }

    #[test]
    fn sigma_acts_by_substitution() {
        // Sigma (left Kan extension) computes the migrated `full` column by
        // substituting each source row's `first`/`last` values into the term.
        let inst = FInstance::new()
            .with_table("person", vec![person_row()])
            .unwrap();
        let extended = functor_extend(&inst, &computed_full_migration())
            .expect("functor_extend should succeed");
        let rows = extended.rows("person");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].1.get("full"),
            Some(&Value::Str("Ada Lovelace".into())),
            "Sigma must compute the derived column by substitution",
        );
        // Source columns pass through unchanged.
        assert_eq!(rows[0].1.get("first"), Some(&Value::Str("Ada".into())));
    }

    #[test]
    fn computed_column_migration_e2e() {
        // The surviving-fragment form acts on values by substitution too: the
        // migrated `person` rows gain the computed `full` column end-to-end.
        let inst = FInstance::new()
            .with_table("person", vec![person_row()])
            .unwrap();
        let restricted = functor_restrict(&inst, &computed_full_migration())
            .expect("functor_restrict should succeed");
        let rows = restricted.rows("person");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].1.get("full"),
            Some(&Value::Str("Ada Lovelace".into()))
        );
        assert_eq!(rows[0].1.get("last"), Some(&Value::Str("Lovelace".into())));
    }

    #[test]
    fn empty_functor_instance() {
        let inst = FInstance::new();
        assert_eq!(inst.table_count(), 0);
    }

    #[test]
    fn functor_with_tables() {
        let inst = FInstance::new()
            .with_table("users", vec![cell("name", "Alice")])
            .unwrap();
        assert_eq!(inst.table_count(), 1);
        assert_eq!(inst.row_count("users"), 1);
        assert_eq!(inst.row_count("posts"), 0);
    }

    #[test]
    fn functor_restrict_drops_table() {
        let inst = FInstance::new()
            .with_table("users", vec![cell("name", "Alice")])
            .and_then(|f| f.with_table("posts", vec![cell("title", "Hello")]))
            .and_then(|f| f.with_foreign_key(fk("posts", "users"), [(RowId(1), RowId(0))]))
            .unwrap();

        // Migration that only keeps "users"
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("users")]),
            ..CompiledMigration::default()
        };

        let restricted = functor_restrict(&inst, &migration).unwrap();
        assert_eq!(restricted.table_count(), 1);
        assert!(restricted.has_table("users"));
        assert!(!restricted.has_table("posts"));
        assert_eq!(restricted.pair_count(), 0);
    }

    /// Two source tables merged into one target: rows keep their ids, so
    /// each foreign-key pair still names the rows it came from.
    #[test]
    fn functor_restrict_keeps_the_row_ids_of_merged_foreign_keys() {
        let edge = fk("a", "b");
        let merged_edge = fk("t", "t");
        // `a` holds rows 0 and 1, `b` rows 2 and 3; a1 references b0.
        let inst = FInstance::new()
            .with_table("a", vec![cell("n", "a0"), cell("n", "a1")])
            .and_then(|f| f.with_table("b", vec![cell("n", "b0"), cell("n", "b1")]))
            .and_then(|f| f.with_foreign_key(edge.clone(), [(RowId(1), RowId(2))]))
            .unwrap();

        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert("t".into());
        migration.vertex_remap.insert("a".into(), "t".into());
        migration.vertex_remap.insert("b".into(), "t".into());
        migration.edge_remap.insert(edge, merged_edge.clone());

        let restricted = functor_restrict(&inst, &migration).unwrap();
        assert_eq!(
            restricted.row_count("t"),
            4,
            "the merged table is the disjoint union"
        );
        assert_eq!(
            restricted.pairs(&merged_edge),
            &std::collections::BTreeSet::from([(RowId(1), RowId(2))])
        );
        assert_eq!(
            restricted.row(RowId(1)).and_then(|r| r.get("n")),
            Some(&Value::Str("a1".into()))
        );
        assert_eq!(
            restricted.row(RowId(2)).and_then(|r| r.get("n")),
            Some(&Value::Str("b0".into()))
        );
    }

    /// Distinct source edges identified by `edge_remap` contribute a union of
    /// pairs, not a last-writer-wins overwrite, and the result is independent
    /// of hash iteration order.
    #[test]
    fn functor_restrict_unions_collided_edges_deterministically() {
        let build = || {
            let inst = FInstance::new()
                .with_table("a", vec![cell("n", "a0"), cell("n", "a1")])
                .and_then(|f| f.with_table("b", vec![cell("n", "b0"), cell("n", "b1")]))
                .and_then(|f| f.with_foreign_key(fk("a", "a"), [(RowId(0), RowId(1))]))
                .and_then(|f| f.with_foreign_key(fk("b", "b"), [(RowId(2), RowId(3))]))
                .unwrap();
            let mut migration = CompiledMigration::default();
            migration.surviving_verts.insert("t".into());
            migration.vertex_remap.insert("a".into(), "t".into());
            migration.vertex_remap.insert("b".into(), "t".into());
            migration.edge_remap.insert(fk("a", "a"), fk("t", "t"));
            migration.edge_remap.insert(fk("b", "b"), fk("t", "t"));
            functor_restrict(&inst, &migration).unwrap()
        };

        let first = build();
        assert_eq!(
            first.pairs(&fk("t", "t")),
            &std::collections::BTreeSet::from([(RowId(0), RowId(1)), (RowId(2), RowId(3))]),
            "both source edges contribute their own pairs",
        );
        for _ in 0..16 {
            assert_eq!(
                build(),
                first,
                "the result must not depend on hash iteration order"
            );
        }
    }

    /// A rename whose migration also lists the old name among the survivors
    /// (the shape `panproto-lens` builds for every vertex rename): each row
    /// lands once, in its source vertex's image, and the result decodes.
    #[test]
    fn functor_restrict_places_each_row_once_under_a_rename() {
        let inst = FInstance::new()
            .with_table("a", vec![cell("n", "a0")])
            .and_then(|f| f.with_foreign_key(fk("a", "a"), [(RowId(0), RowId(0))]))
            .unwrap();
        let mut migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("a"), Name::from("b")]),
            ..CompiledMigration::default()
        };
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("b"));
        migration.edge_remap.insert(fk("a", "a"), fk("b", "b"));

        let restricted = functor_restrict(&inst, &migration).unwrap();
        assert_eq!(restricted.total_row_count(), 1);
        assert_eq!(restricted.row_count("b"), 1);
        assert!(!restricted.has_table("a"), "a renamed vertex keeps no copy");
        assert_eq!(restricted.table_of(RowId(0)).map(|n| &**n), Some("b"));
        assert_eq!(
            restricted.pairs(&fk("b", "b")),
            &std::collections::BTreeSet::from([(RowId(0), RowId(0))])
        );
        let bytes = serde_json::to_vec(&restricted).unwrap();
        let back: FInstance = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, restricted);
        let extended = functor_extend(&inst, &migration).unwrap();
        assert_eq!(
            restricted, extended,
            "restrict agrees with extend on a rename"
        );
    }

    /// A migration merging `a` and `b` onto `t`.
    fn merging_migration() -> CompiledMigration {
        let mut migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("t")]),
            ..CompiledMigration::default()
        };
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("b"), Name::from("t"));
        migration
    }

    /// A row carrying a single integer under `v`.
    fn numbered(value: i64) -> Row {
        HashMap::from([("v".to_owned(), Value::Int(value))])
    }

    /// What the direction of `functor_restrict` actually is, since its name
    /// says only how much of the schema it keeps.
    ///
    /// Over a merging vertex map it returns the fibre's coproduct in the
    /// target -- `Sigma_F`'s answer -- and not the source-indexed tables
    /// precomposition would return.
    #[test]
    fn functor_restrict_pushes_forward_rather_than_precomposing() {
        let m = merging_migration();
        let x = FInstance::new()
            .with_table("a", vec![numbered(1), numbered(2)])
            .and_then(|f| f.with_table("b", vec![numbered(3)]))
            .unwrap();

        let restricted = functor_restrict(&x, &m).expect("restrict");
        assert_eq!(restricted.row_count("t"), 3);
        assert_eq!(restricted.table_count(), 1);

        // The functor carries each table's column `v` to `t`'s.
        let column = |table: &str| Edge {
            src: table.into(),
            tgt: format!("{table}.v").into(),
            kind: "prop".into(),
            name: Some("v".into()),
        };
        let columns = |tables: &[&str]| {
            tables
                .iter()
                .map(|t| {
                    (
                        Name::from(*t),
                        std::collections::BTreeMap::from([("v".to_owned(), column(t))]),
                    )
                })
                .collect()
        };
        let mut carried = m.clone();
        for table in ["a", "b"] {
            carried.edge_remap.insert(column(table), column("t"));
        }
        let along = crate::MigrationFunctor::between(
            crate::RelSchema::from_parts(
                [Name::from("a"), Name::from("b")],
                [],
                columns(&["a", "b"]),
            )
            .unwrap(),
            crate::RelSchema::from_parts([Name::from("t")], [], columns(&["t"])).unwrap(),
            &carried,
        )
        .unwrap();
        let pushed = crate::adjunction::f_sigma(&x, &along).expect("sigma");
        assert_eq!(
            restricted, pushed,
            "restrict agrees with Sigma_F on the surviving fragment"
        );

        // Precomposition reads its input as a `t`-instance: `x` holds rows
        // at `a` and `b`, which are no tables of the target, so it refuses.
        assert!(matches!(
            crate::adjunction::f_delta(&x, &along),
            Err(crate::AdjunctionError::NonConforming {
                reason: crate::NonConformance::TableOutside { .. },
                ..
            })
        ));
    }

    /// One row `0` in table `a` holding `x = value`.
    fn one_cell(value: Value) -> FInstance {
        FInstance::new()
            .with_rows("a", [(RowId(0), HashMap::from([("x".to_owned(), value)]))])
            .unwrap()
    }

    /// A key matches what a row holds exactly: a `NaN` key cell finds its
    /// row, and `0.0` does not find a row holding `-0.0`.
    #[test]
    fn a_key_matches_cells_exactly() {
        let nan = one_cell(Value::Float(f64::NAN));
        assert_eq!(
            nan.find_row_by_key("a", &[("x", &Value::Float(f64::NAN))]),
            Ok(RowId(0))
        );
        let negative = one_cell(Value::Float(-0.0));
        assert_eq!(
            negative.find_row_by_key("a", &[("x", &Value::Float(0.0))]),
            Err(KeyLookupError::Missing {
                table: Name::from("a")
            })
        );
        assert_eq!(
            negative.find_row_by_key("a", &[("x", &Value::Float(-0.0))]),
            Ok(RowId(0))
        );
    }

    /// Two instances are equal exactly when they hold the same cells by
    /// canonical bytes, so an instance with a `NaN` cell equals itself and
    /// `0.0` differs from `-0.0`, in agreement with their digests.
    #[test]
    fn instance_equality_is_exact_on_cells() {
        let nan = one_cell(Value::Float(f64::NAN));
        assert_eq!(nan, nan.clone());
        let positive = one_cell(Value::Float(0.0));
        let negative = one_cell(Value::Float(-0.0));
        assert_ne!(positive.digest(), negative.digest());
        assert_ne!(positive, negative);
    }

    /// A vertex that survives under its own name but is also another
    /// vertex's image is a target of the migration, not a source: restrict
    /// keeps only the rows that reach it from its sources, and extend
    /// refuses the rows it holds itself.
    #[test]
    fn an_image_vertex_is_not_a_source() {
        let inst = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new())])
            .and_then(|f| f.with_rows("b", [(RowId(1), Row::new())]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.vertex_remap.insert("a".into(), "b".into());
        migration.surviving_verts.insert("b".into());

        let restricted = functor_restrict(&inst, &migration).unwrap();
        let ids: Vec<RowId> = restricted.rows("b").into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![RowId(0)]);
        assert_eq!(restricted.total_row_count(), 1);

        let err = functor_extend(&inst, &migration).unwrap_err();
        assert!(
            matches!(&err, RestrictError::VertexOutsideDomain { vertex } if vertex == "b"),
            "{err:?}"
        );
    }

    /// An edge of kind `kind` named `name` from `src` to `tgt`.
    fn named(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: Some(name.into()),
        }
    }

    /// Rows `a:0`, `b:1`, `c:2`, with the given pairs.
    fn abc_with(pairs: &[(Edge, (u32, u32))]) -> FInstance {
        let mut inst = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new())])
            .and_then(|f| f.with_rows("b", [(RowId(1), Row::new())]))
            .and_then(|f| f.with_rows("c", [(RowId(2), Row::new())]))
            .unwrap();
        for (edge, (s, t)) in pairs {
            inst = inst
                .with_foreign_key(edge.clone(), [(RowId(*s), RowId(*t))])
                .unwrap();
        }
        inst
    }

    /// `c` merges onto `a`, and `a`, `b` survive under their own names.
    fn c_onto_a() -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        migration.vertex_remap.insert("a".into(), "a".into());
        migration.vertex_remap.insert("c".into(), "a".into());
        migration.vertex_remap.insert("b".into(), "b".into());
        migration.surviving_verts.insert("a".into());
        migration.surviving_verts.insert("b".into());
        migration
    }

    fn refuses_edge(result: Result<FInstance, RestrictError>, expected: &Edge) {
        match result {
            Err(RestrictError::EdgeOutsideDomain { edge }) => assert_eq!(&edge, expected),
            other => panic!("expected EdgeOutsideDomain on {expected:?}, got {other:?}"),
        }
    }

    /// An unmentioned edge relabelled onto the image of a remapped source
    /// edge that holds pairs is refused, not merged into that image.
    #[test]
    fn extend_refuses_an_unmentioned_edge_relabelled_onto_a_remapped_image() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        migration.surviving_edges.insert(named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "y");
        let inst = abc_with(&[(named("a", "b", "fk", "x"), (0, 1)), (z.clone(), (2, 1))]);
        refuses_edge(functor_extend(&inst, &migration), &z);
    }

    /// An unmentioned edge relabelled onto the image of a source edge that
    /// survives under its own name is refused.
    #[test]
    fn extend_refuses_an_unmentioned_edge_relabelled_onto_a_surviving_edge() {
        let mut migration = c_onto_a();
        migration.surviving_edges.insert(named("a", "b", "fk", "w"));
        let z = named("c", "b", "fk", "w");
        let inst = abc_with(&[(named("a", "b", "fk", "w"), (0, 1)), (z.clone(), (2, 1))]);
        refuses_edge(functor_extend(&inst, &migration), &z);
    }

    /// An unmentioned edge relabelled onto a target-only edge is refused
    /// even when the edge remapped onto it holds no pairs: the claim comes
    /// from the migration, not from the instance.
    #[test]
    fn extend_refuses_an_unmentioned_edge_relabelled_onto_a_target_only_edge() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "y");
        let inst = abc_with(&[(z.clone(), (2, 1))]);
        refuses_edge(functor_extend(&inst, &migration), &z);
    }

    /// Two unmentioned edges whose relabellings coincide are refused: the
    /// later one in edge order is named.
    #[test]
    fn extend_refuses_two_unmentioned_edges_relabelled_onto_one_edge() {
        let migration = c_onto_a();
        let z1 = named("c", "b", "k", "n");
        let z2 = named("a", "b", "k", "n");
        let inst = abc_with(&[(z1.clone(), (2, 1)), (z2, (0, 1))]);
        refuses_edge(functor_extend(&inst, &migration), &z1);
    }

    /// An unmentioned edge whose relabelling is free travels with its pairs
    /// between the images of its endpoints, also when it lands on a source
    /// edge the migration remaps away.
    #[test]
    fn extend_carries_an_unmentioned_edge_onto_a_free_relabelling() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "free");
        let onto_key = named("c", "b", "fk", "x");
        let inst = abc_with(&[
            (named("a", "b", "fk", "x"), (0, 1)),
            (z, (2, 1)),
            (onto_key, (2, 1)),
        ]);
        let out = functor_extend(&inst, &migration).unwrap();
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "free")),
            &BTreeSet::from([(RowId(2), RowId(1))])
        );
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "y")),
            &BTreeSet::from([(RowId(0), RowId(1))])
        );
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "x")),
            &BTreeSet::from([(RowId(2), RowId(1))])
        );
        assert_eq!(out.pair_count(), 3);
    }

    /// Two source edges that `edge_remap` sends to one image merge, as the
    /// migration states.
    #[test]
    fn extend_merges_source_edges_the_migration_merges() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        migration
            .edge_remap
            .insert(named("c", "b", "fk", "x2"), named("a", "b", "fk", "y"));
        let inst = abc_with(&[
            (named("a", "b", "fk", "x"), (0, 1)),
            (named("c", "b", "fk", "x2"), (2, 1)),
        ]);
        let out = functor_extend(&inst, &migration).unwrap();
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "y")),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(2), RowId(1))])
        );
    }

    /// Extend merges two foreign keys only when the migration says so.
    mod extend_merges_only_what_the_migration_states {
        use proptest::prelude::*;

        use super::*;

        const VERTS: [&str; 4] = ["a", "b", "c", "d"];
        const NAMES: [&str; 2] = ["p", "q"];

        /// The edge with index `i` of the 32-edge universe over [`VERTS`]
        /// and [`NAMES`].
        fn universe(i: usize) -> Edge {
            Edge {
                src: VERTS[i % 4].into(),
                tgt: VERTS[(i / 4) % 4].into(),
                kind: "fk".into(),
                name: Some(NAMES[(i / 16) % 2].into()),
            }
        }

        prop_compose! {
            fn migration()(
                vertex_map in prop::collection::vec(prop::option::of(0usize..4), 4),
                fix_images in any::<bool>(),
                surviving in prop::collection::vec(any::<bool>(), 4),
                edge_map in prop::collection::vec((0usize..32, 0usize..32), 0..4),
                surviving_edges in prop::collection::vec(0usize..32, 0..4),
            ) -> CompiledMigration {
                let mut migration = CompiledMigration::default();
                for (v, image) in vertex_map.iter().enumerate() {
                    if let Some(t) = image {
                        migration.vertex_remap.insert(VERTS[v].into(), VERTS[*t].into());
                    }
                }
                if fix_images {
                    let images: Vec<Name> = migration.vertex_remap.values().cloned().collect();
                    for image in images {
                        migration.vertex_remap.entry(image.clone()).or_insert(image);
                    }
                }
                for (v, keep) in surviving.iter().enumerate() {
                    if *keep {
                        migration.surviving_verts.insert(VERTS[v].into());
                    }
                }
                for (from, to) in edge_map {
                    migration.edge_remap.insert(universe(from), universe(to));
                }
                for e in surviving_edges {
                    migration.surviving_edges.insert(universe(e));
                }
                migration
            }
        }

        /// One row per vertex (`RowId(i)` at `VERTS[i]`), and the pair of
        /// the endpoint rows on each chosen edge.
        fn instance(edges: &[usize]) -> FInstance {
            let mut inst = FInstance::new();
            for (i, v) in VERTS.iter().enumerate() {
                inst = inst
                    .with_rows(*v, [(RowId(u32::try_from(i).unwrap()), Row::new())])
                    .unwrap();
            }
            for &i in edges {
                let e = universe(i);
                let s = u32::try_from(VERTS.iter().position(|v| e.src == *v).unwrap()).unwrap();
                let t = u32::try_from(VERTS.iter().position(|v| e.tgt == *v).unwrap()).unwrap();
                inst = inst.with_foreign_key(e, [(RowId(s), RowId(t))]).unwrap();
            }
            inst
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1024))]

            #[test]
            fn extend_merges_only_source_edges(
                migration in migration(),
                edges in prop::collection::btree_set(0usize..32, 0..8),
            ) {
                let edges: Vec<usize> = edges.into_iter().collect();
                let inst = instance(&edges);
                let images = crate::adjunction::edge_images(&migration);
                let target_only = crate::adjunction::target_only_edges(&migration);
                // Every target-only edge is the image of the edge remapped
                // onto it, so it is also an image value.
                for e in &target_only {
                    prop_assert!(images.values().any(|image| image == *e));
                }
                let Ok(out) = functor_extend(&inst, &migration) else {
                    return Ok(());
                };
                // Which input edges sent pairs to each output edge.
                let mut contributors: HashMap<Edge, Vec<Edge>> = HashMap::new();
                for (edge, _) in inst.foreign_keys() {
                    prop_assert!(!target_only.contains(edge));
                    let carried = images
                        .get(edge)
                        .cloned()
                        .unwrap_or_else(|| crate::adjunction::map_edge(&migration, edge));
                    contributors.entry(carried).or_default().push(edge.clone());
                }
                for (edge, pairs) in out.foreign_keys() {
                    let from = contributors.get(edge).cloned().unwrap_or_default();
                    prop_assert!(!from.is_empty(), "{edge:?} has pairs but no preimage");
                    let mut expected = BTreeSet::new();
                    for source in &from {
                        expected.extend(inst.pairs(source).iter().copied());
                    }
                    prop_assert!(pairs.is_subset(&expected));
                    let unmentioned: Vec<&Edge> =
                        from.iter().filter(|e| !images.contains_key(*e)).collect();
                    if !unmentioned.is_empty() {
                        prop_assert_eq!(
                            from.len(), 1,
                            "{:?} merges {:?} unasked", edge, from
                        );
                        prop_assert!(
                            !images.values().any(|image| image == edge),
                            "unmentioned {:?} lands on the image {:?}", unmentioned, edge
                        );
                    }
                }
            }
        }
    }
}
