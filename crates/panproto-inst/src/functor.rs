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
//! `(source row, target row)` pairs, stored sorted. There is no positional
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
//! padded to a common column set. Both keep every row's id.
//!
//! Neither is precomposition. Both take the coproduct over a fibre when the
//! vertex map merges two vertices, which is what `Sigma_F` does. `Delta_F`
//! runs the other way, from a `T`-instance to an `S`-instance, and lives in
//! [`crate::adjunction::f_delta`] with the rest of the adjunction.

use std::collections::{BTreeMap, HashMap};

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

/// One table of an [`FInstance`]: its rows, by [`RowId`].
///
/// A table enumerates its rows in ascending id; nothing about a row depends
/// on the order in which it was inserted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    rows: HashMap<RowId, Row>,
}

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
/// edge's source table and the target row in its target table. A vertex
/// with no rows sends the functor to the empty set, so the instance holds a
/// table only while it has a row: an empty table and an absent one are the
/// same instance, with the same wire form, canonical bytes and graph form.
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
/// assert_eq!(tables.pairs(&author), &[(RowId(1), RowId(0))]);
/// assert_eq!(tables.fresh_row_id()?, RowId(2));
/// # Ok::<(), panproto_inst::InstError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FInstance {
    tables: HashMap<Name, Table>,
    foreign_keys: HashMap<Edge, Vec<(RowId, RowId)>>,
    row_table: HashMap<RowId, Name>,
    /// The largest key of `row_table`, kept exact so that allocating a fresh
    /// id costs no scan; a function of `row_table`, so equality is unchanged.
    max_row: Option<RowId>,
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
        self.foreign_keys.values().map(Vec::len).sum()
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
        let mut ids: Vec<RowId> = self.row_table.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// The largest row id in use, if any.
    #[must_use]
    pub fn max_row_id(&self) -> Option<RowId> {
        self.max_row
    }

    /// The least row id above every row id in use: `max + 1`, and `0` for an
    /// instance without rows.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NodeIdsExhausted`] when the largest id in use is
    /// `u32::MAX`.
    pub fn fresh_row_id(&self) -> Result<RowId, InstError> {
        allocate_row(&mut row_allocator(self.max_row))
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
        self.row_table.insert(id, vertex.clone());
        self.max_row = self.max_row.max(Some(id));
        self.tables.entry(vertex).or_default().rows.insert(id, row);
    }

    /// Take a row out of its table, dropping the table when it empties, and
    /// out of the row index. Pairs are left to the caller.
    fn take_row(&mut self, id: RowId) -> Option<(Name, Row)> {
        let table_name = self.row_table.remove(&id)?;
        if self.max_row == Some(id) {
            self.max_row = self.row_table.keys().copied().max();
        }
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
    pub fn remove_row(&mut self, id: RowId) -> Option<(Name, Row, Vec<ForeignKeyPair>)> {
        let (table, row) = self.take_row(id)?;
        let incident = self.incident_pairs(id);
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
    /// [`InstError::DanglingForeignKey`] naming the first incident
    /// pair when the row takes part in a foreign key, since the pair would no
    /// longer join its edge's endpoint tables.
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
        if let Some((edge, _, _)) = self.incident_pairs(id).into_iter().next() {
            return Err(InstError::DanglingForeignKey { edge, row: id });
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
        let pairs = self.foreign_keys.entry(edge.clone()).or_default();
        match pairs.binary_search(&(src, tgt)) {
            Ok(_) => Err(InstError::DuplicateForeignKey { edge, src, tgt }),
            Err(at) => {
                pairs.insert(at, (src, tgt));
                Ok(())
            }
        }
    }

    /// Add every pair of `pairs` to the foreign key of `edge`, in time
    /// `O(n log n)` in the pairs whatever their order.
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
        if pairs.is_empty() {
            return Ok(());
        }
        let existing = self.foreign_keys.remove(&edge).unwrap_or_default();
        let mut merged = Vec::with_capacity(existing.len() + pairs.len());
        let (mut old, mut new) = (existing.iter().peekable(), pairs.iter().peekable());
        loop {
            let next = match (old.peek(), new.peek()) {
                (Some(a), Some(b)) if a == b => {
                    let (src, tgt) = **a;
                    self.foreign_keys.insert(edge.clone(), existing.clone());
                    return Err(InstError::DuplicateForeignKey { edge, src, tgt });
                }
                (Some(a), Some(b)) if a < b => old.next(),
                (Some(_), Some(_)) | (None, Some(_)) => new.next(),
                (Some(_), None) => old.next(),
                (None, None) => break,
            };
            merged.extend(next.copied());
        }
        self.foreign_keys.insert(edge, merged);
        Ok(())
    }

    /// Add a pair whose endpoints the caller knows to be rows of the edge's
    /// endpoint tables; a pair already present is left as it is. Returns
    /// whether the pair was added.
    pub(crate) fn place_pair(&mut self, edge: Edge, src: RowId, tgt: RowId) -> bool {
        let pairs = self.foreign_keys.entry(edge).or_default();
        match pairs.binary_search(&(src, tgt)) {
            Ok(_) => false,
            Err(at) => {
                pairs.insert(at, (src, tgt));
                true
            }
        }
    }

    /// Whether `(src, tgt)` would join the endpoint tables of `edge`.
    pub(crate) fn joins(&self, edge: &Edge, src: RowId, tgt: RowId) -> bool {
        self.row_table.get(&src) == Some(&edge.src) && self.row_table.get(&tgt) == Some(&edge.tgt)
    }

    /// Remove the pair `(src, tgt)` from the foreign key of `edge`. Returns
    /// whether it was present.
    pub fn remove_foreign_key(&mut self, edge: &Edge, src: RowId, tgt: RowId) -> bool {
        let Some(pairs) = self.foreign_keys.get_mut(edge) else {
            return false;
        };
        let Ok(at) = pairs.binary_search(&(src, tgt)) else {
            return false;
        };
        pairs.remove(at);
        if pairs.is_empty() {
            self.foreign_keys.remove(edge);
        }
        true
    }

    /// Whether the foreign key of `edge` holds `(src, tgt)`.
    #[must_use]
    pub fn contains_pair(&self, edge: &Edge, src: RowId, tgt: RowId) -> bool {
        self.foreign_keys
            .get(edge)
            .is_some_and(|pairs| pairs.binary_search(&(src, tgt)).is_ok())
    }

    /// The pairs of the foreign key of `edge`, ascending; empty when the
    /// edge has none.
    #[must_use]
    pub fn pairs(&self, edge: &Edge) -> &[(RowId, RowId)] {
        self.foreign_keys.get(edge).map_or(&[], Vec::as_slice)
    }

    /// The foreign keys with their pairs, in ascending edge order. An edge
    /// without pairs is not listed.
    #[must_use]
    pub fn foreign_keys(&self) -> Vec<(&Edge, &[(RowId, RowId)])> {
        let mut keys: Vec<(&Edge, &[(RowId, RowId)])> = self
            .foreign_keys
            .iter()
            .filter(|(_, pairs)| !pairs.is_empty())
            .map(|(edge, pairs)| (edge, pairs.as_slice()))
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
    /// `(edge, src, tgt)`.
    #[must_use]
    pub fn incident_pairs(&self, id: RowId) -> Vec<ForeignKeyPair> {
        let mut incident: Vec<ForeignKeyPair> = self
            .foreign_keys
            .iter()
            .flat_map(|(edge, pairs)| {
                pairs
                    .iter()
                    .filter(move |(s, t)| *s == id || *t == id)
                    .map(move |&(s, t)| (edge.clone(), s, t))
            })
            .collect();
        incident.sort_unstable();
        incident
    }

    /// The single row of `vertex`'s table whose cells agree with every
    /// `(column, value)` of `key`.
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
            .filter(|(_, row)| key.iter().all(|(col, val)| row.get(*col) == Some(*val)))
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
    foreign_keys: BTreeMap<&'a Edge, &'a Vec<(RowId, RowId)>>,
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
        let foreign_keys = self
            .foreign_keys
            .iter()
            .filter(|(_, pairs)| !pairs.is_empty())
            .collect();
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

/// The edge a source edge travels to under a migration: its remap when the
/// migration names one, itself when it survives, and `None` otherwise.
fn carried_edge(migration: &CompiledMigration, edge: &Edge) -> Option<Edge> {
    migration.edge_remap.get(edge).cloned().or_else(|| {
        migration
            .surviving_edges
            .contains(edge)
            .then(|| edge.clone())
    })
}

/// Carry a set-valued functor instance forward along a migration, keeping
/// the fragment that survives.
///
/// Each surviving target vertex takes the rows of every source vertex the
/// migration sends to it, each row keeping its [`RowId`] and rewritten by
/// its source vertex's op-to-term assignments; a target no source reaches
/// keeps a same-named source table when the instance has one. Foreign keys
/// travel for the edges the migration remaps between surviving vertices and
/// for those it lets survive: a pair survives exactly when both of its rows
/// landed in the carried edge's endpoint tables. Pairs from source edges the
/// migration identifies are unioned (a foreign key is a set).
///
/// This is a pushforward, not precomposition: over a vertex map that merges
/// two vertices it returns the fibre's disjoint union, which is `Sigma_F`'s
/// answer; the row ids keep the union disjoint without any renumbering. For
/// `Delta_F` see [`crate::adjunction::f_delta`].
///
/// # Errors
///
/// Returns `RestrictError` if a row's op-to-term assignment fails to
/// evaluate.
pub fn functor_restrict(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    let mut out = FInstance::new();
    let mut targets: Vec<&Name> = migration.surviving_verts.iter().collect();
    targets.sort_unstable();
    for tgt_vertex in targets {
        let mut sources: Vec<&Name> = migration
            .vertex_remap
            .iter()
            .filter(|(_, v)| *v == tgt_vertex)
            .map(|(k, _)| k)
            .collect();
        sources.sort_unstable();
        if sources.is_empty() {
            sources.push(tgt_vertex);
        }
        for src_vertex in sources {
            let Some(table) = instance.tables.get(src_vertex) else {
                continue;
            };
            let assignments = migration.op_term_assignments.get(src_vertex);
            for (id, row) in table.rows() {
                let mut new_row = row.clone();
                if let Some(assignments) = assignments {
                    crate::wtype::apply_term_assignments_to_row(&mut new_row, assignments)?;
                }
                out.place_row(tgt_vertex.clone(), id, new_row);
            }
        }
    }

    for (edge, pairs) in instance.foreign_keys() {
        let Some(new_edge) = carried_edge(migration, edge) else {
            continue;
        };
        if !migration.surviving_verts.contains(&new_edge.src)
            || !migration.surviving_verts.contains(&new_edge.tgt)
        {
            continue;
        }
        for &(s, t) in pairs {
            if out.joins(&new_edge, s, t) {
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
/// target schema by moving every table to its image (a vertex the migration
/// does not remap keeps its name); a surviving target vertex no row reaches
/// is sent to the empty set, so it has no table. Rows keep their [`RowId`]s,
/// so the coproduct over a merging fibre is a disjoint union with no
/// renumbering, and the operation creates no rows. Within each target table
/// the rows are padded to the union of their columns with [`Value::Null`].
/// Foreign keys travel along remapped and surviving edges; a pair is kept
/// when its rows lie in the carried edge's endpoint tables.
///
/// # Errors
///
/// Returns `RestrictError` if a row's op-to-term assignment fails to
/// evaluate.
pub fn functor_extend(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    let mut out = FInstance::new();
    for (src_vertex, table) in instance.tables() {
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

    for (edge, pairs) in instance.foreign_keys() {
        let Some(new_edge) = carried_edge(migration, edge) else {
            continue;
        };
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
        assert_eq!(restricted.pairs(&merged_edge), &[(RowId(1), RowId(2))]);
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
            &[(RowId(0), RowId(1)), (RowId(2), RowId(3))],
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

        let pushed = crate::adjunction::f_sigma(&x, &m).expect("sigma");
        assert_eq!(
            restricted, pushed,
            "restrict agrees with Sigma_F on the surviving fragment"
        );

        // Precomposition reads `x` as a `t`-instance: `a` and `b` pull back
        // the rows of `t`, which `x` does not have, so both are empty.
        let precomposed = crate::adjunction::f_delta(&x, &m).expect("delta");
        assert_eq!(precomposed.total_row_count(), 0);
        assert!(!precomposed.has_table("t"));
    }
}
