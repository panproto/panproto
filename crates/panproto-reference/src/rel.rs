//! Relation-ified schemas, the migration's entity functor, and the three
//! Kan extensions on finite attributed presheaves.
//!
//! A schema's relation-ified category has one object per table and one per
//! relation (an edge between tables) with its two projections; attribute
//! arrows (edges from a table to a value vertex) are columns into a free,
//! discrete value side. An instance is a presheaf on that category with
//! cells: rows at tables, pairs at relations, and per row one value per
//! column. Delta is precomposition; Sigma is the quotient of the disjoint
//! union; Pi enumerates compatible families over the comma category.

use std::collections::{BTreeMap, BTreeSet};

use crate::migration::Migration;
use crate::model::{Name, SEdge, Table};
use crate::schema::Schema;
use crate::value::Value;

/// A relation-ified schema.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RelSchema {
    /// Tables.
    pub tables: BTreeSet<Name>,
    /// Relation objects: edges between two tables.
    pub relations: BTreeSet<SEdge>,
    /// Per table, column name to attribute arrow.
    pub columns: BTreeMap<Name, BTreeMap<String, SEdge>>,
    /// Value vertices.
    pub value_vertices: BTreeSet<Name>,
    /// Edges out of value vertices (inner structure of one cell).
    pub inner_edges: BTreeSet<SEdge>,
    /// The kind of every vertex.
    pub kinds: BTreeMap<Name, Name>,
}

/// Why a schema cannot be relation-ified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelSchemaError {
    /// A kind is both a value kind and a non-value kind.
    AmbiguousKind {
        /// The kind.
        kind: Name,
    },
    /// An undeclared vertex whose kind is in neither set.
    UnclassifiedKind {
        /// The vertex.
        vertex: Name,
        /// Its kind.
        kind: Name,
    },
    /// The value closure reaches a table.
    ValueReachesTable {
        /// The least offending edge.
        edge: SEdge,
    },
    /// Two attribute arrows of one table share a column name.
    DuplicateColumn {
        /// The table.
        table: Name,
        /// The column.
        column: String,
    },
}

/// Classify a schema (strict classification).
///
/// Definition: with K the value kinds and N the non-value kinds, a kind in
/// both is `AmbiguousKind` (the least such kind); the declared tables D are
/// the vertices with a key and the holders and targets of references; an
/// undeclared vertex with a kind in neither set is `UnclassifiedKind` (the
/// least vertex); the value vertices are the vertices reachable along edges
/// from the undeclared vertices of a value kind, and an edge of that closure
/// reaching a declared table or a vertex of a non-value kind is
/// `ValueReachesTable` (the least edge); the tables are D and the vertices
/// of a non-value kind; relations are the edges between tables; attribute
/// arrows are the edges from a table to a value vertex, named by the edge
/// name else the target, a repeated name in one table being
/// `DuplicateColumn`; edges out of value vertices are inner edges.
///
/// # Errors
///
/// The first of the errors above, in that order.
pub fn classify(
    schema: &Schema,
    value_kinds: &BTreeSet<Name>,
    non_value_kinds: &BTreeSet<Name>,
) -> Result<RelSchema, RelSchemaError> {
    if let Some(kind) = value_kinds.intersection(non_value_kinds).next() {
        return Err(RelSchemaError::AmbiguousKind { kind: kind.clone() });
    }
    let mut declared: BTreeSet<Name> = schema
        .keys
        .iter()
        .filter(|(_, k)| !k.is_empty())
        .map(|(v, _)| v.clone())
        .collect();
    for r in schema.references.values() {
        declared.insert(r.holder.clone());
        declared.insert(r.target_vertex.clone());
    }
    for (v, kind) in &schema.vertices {
        if !declared.contains(v) && !value_kinds.contains(kind) && !non_value_kinds.contains(kind) {
            return Err(RelSchemaError::UnclassifiedKind {
                vertex: v.clone(),
                kind: kind.clone(),
            });
        }
    }
    let is_table = |v: &str| {
        declared.contains(v)
            || schema
                .vertices
                .get(v)
                .is_some_and(|k| non_value_kinds.contains(k))
    };
    let mut values: BTreeSet<Name> = schema
        .vertices
        .iter()
        .filter(|(v, k)| !declared.contains(*v) && value_kinds.contains(*k))
        .map(|(v, _)| v.clone())
        .collect();
    loop {
        let grow: Vec<Name> = schema
            .edges
            .iter()
            .filter(|e| values.contains(&e.src) && !values.contains(&e.tgt) && !is_table(&e.tgt))
            .map(|e| e.tgt.clone())
            .collect();
        if grow.is_empty() {
            break;
        }
        values.extend(grow);
    }
    if let Some(bad) = schema
        .edges
        .iter()
        .find(|e| values.contains(&e.src) && is_table(&e.tgt))
    {
        return Err(RelSchemaError::ValueReachesTable { edge: bad.clone() });
    }
    let tables: BTreeSet<Name> = schema
        .vertices
        .keys()
        .filter(|v| is_table(v))
        .cloned()
        .collect();
    let mut columns: BTreeMap<Name, BTreeMap<String, SEdge>> = tables
        .iter()
        .map(|t| (t.clone(), BTreeMap::new()))
        .collect();
    let mut relations = BTreeSet::new();
    let mut inner_edges = BTreeSet::new();
    for e in &schema.edges {
        if tables.contains(&e.src) && tables.contains(&e.tgt) {
            relations.insert(e.clone());
        } else if tables.contains(&e.src) && values.contains(&e.tgt) {
            let cols = columns.entry(e.src.clone()).or_default();
            let name = e.column().to_owned();
            if cols.insert(name.clone(), e.clone()).is_some() {
                return Err(RelSchemaError::DuplicateColumn {
                    table: e.src.clone(),
                    column: name,
                });
            }
        } else if values.contains(&e.src) {
            inner_edges.insert(e.clone());
        }
    }
    Ok(RelSchema {
        tables,
        relations,
        columns,
        value_vertices: values,
        inner_edges,
        kinds: schema.vertices.clone(),
    })
}

/// A functor between relation-ified schemas, restricted to its kept part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelFunctor {
    /// The source schema.
    pub src: RelSchema,
    /// The target schema.
    pub tgt: RelSchema,
    /// Kept source table to target table.
    pub tables: BTreeMap<Name, Name>,
    /// Kept source relation to target relation.
    pub relations: BTreeMap<SEdge, SEdge>,
    /// Kept `(source table, column)` to target column of the image table.
    pub columns: BTreeMap<(Name, String), String>,
    /// Remap entries on inner value edges.
    pub inner_edges: BTreeMap<SEdge, SEdge>,
}

/// Why a migration has no entity functor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctorError {
    /// A kept table's image is not a target table.
    VertexImageNotATable {
        /// The source table.
        vertex: Name,
        /// Its image.
        image: Name,
    },
    /// A kept relation's image is not a target relation between the images.
    IncoherentEdgeImage {
        /// The source edge.
        edge: Box<SEdge>,
        /// Its image.
        image: Box<SEdge>,
    },
    /// An attribute arrow's image is not a target attribute arrow of the
    /// image table.
    IncoherentAttributeImage {
        /// The source arrow.
        edge: Box<SEdge>,
        /// Its image, when it has one.
        image: Option<Box<SEdge>>,
    },
    /// An inner-edge remap changes the value structure.
    ValueStructureRemap {
        /// The source inner edge.
        edge: Box<SEdge>,
    },
}

fn is_image<'a, K, V: PartialEq + 'a>(map: impl IntoIterator<Item = (K, &'a V)>, v: &V) -> bool {
    map.into_iter().any(|(_, w)| w == v)
}

/// The entity functor of a migration between two relation-ified schemas.
///
/// Steps, each reporting its least offender: (1) a source table is kept iff
/// `vertex_remap` maps it, or it is listed in `surviving_verts` and is no
/// remap image; its image (the remap, else itself) must be a target table;
/// (2) a relation between kept tables is kept iff `edge_remap` maps it, or
/// it is listed in `surviving_edges` and is no remap image; its image must
/// be a target relation between the endpoint images; (3) an attribute arrow
/// of a kept table that `edge_remap` maps must go to a target attribute
/// arrow from the image table (ending at the value vertex's remap when it
/// has one); one listed in `surviving_edges` and no image must have a
/// target arrow from the image table with its kind and name, ending at its
/// value vertex's image; an unmentioned column is dropped; (4) remaps of
/// inner value edges are recorded, and one whose image is no target inner
/// edge, or changes the name, the kind, or an endpoint's kind, is
/// `ValueStructureRemap`.
///
/// # Errors
///
/// The first failing step's [`FunctorError`].
pub fn entity_functor(
    src: &RelSchema,
    tgt: &RelSchema,
    m: &Migration,
) -> Result<RelFunctor, FunctorError> {
    let mut tables = BTreeMap::new();
    for s in &src.tables {
        let kept = m.vertex_remap.contains_key(s)
            || (m.surviving_verts.contains(s) && !is_image(&m.vertex_remap, s));
        if !kept {
            continue;
        }
        let image = m.vertex_image(s).to_owned();
        if !tgt.tables.contains(&image) {
            return Err(FunctorError::VertexImageNotATable {
                vertex: s.clone(),
                image,
            });
        }
        tables.insert(s.clone(), image);
    }
    let kept_edge = |e: &SEdge| {
        m.edge_remap.contains_key(e)
            || (m.surviving_edges.contains(e) && !is_image(&m.edge_remap, e))
    };
    let mut relations = BTreeMap::new();
    for e in src
        .relations
        .iter()
        .filter(|e| tables.contains_key(&e.src) && tables.contains_key(&e.tgt))
    {
        if !kept_edge(e) {
            continue;
        }
        let image = m.edge_remap.get(e).cloned().unwrap_or_else(|| e.clone());
        let coherent = tgt.relations.contains(&image)
            && image.src == tables[&e.src]
            && image.tgt == tables[&e.tgt];
        if !coherent {
            return Err(FunctorError::IncoherentEdgeImage {
                edge: Box::new(e.clone()),
                image: Box::new(image),
            });
        }
        relations.insert(e.clone(), image);
    }
    let columns = functor_columns(src, tgt, m, &tables)?;
    let mut inner_edges = BTreeMap::new();
    for e in &src.inner_edges {
        if let Some(image) = m.edge_remap.get(e) {
            let kind_of = |schema: &RelSchema, v: &str| schema.kinds.get(v).cloned();
            let ok = tgt.inner_edges.contains(image)
                && image.name == e.name
                && image.kind == e.kind
                && kind_of(tgt, &image.src) == kind_of(src, &e.src)
                && kind_of(tgt, &image.tgt) == kind_of(src, &e.tgt);
            if !ok {
                return Err(FunctorError::ValueStructureRemap {
                    edge: Box::new(e.clone()),
                });
            }
            inner_edges.insert(e.clone(), image.clone());
        }
    }
    Ok(RelFunctor {
        src: src.clone(),
        tgt: tgt.clone(),
        tables,
        relations,
        columns,
        inner_edges,
    })
}

fn functor_columns(
    src: &RelSchema,
    tgt: &RelSchema,
    m: &Migration,
    tables: &BTreeMap<Name, Name>,
) -> Result<BTreeMap<(Name, String), String>, FunctorError> {
    let mut out = BTreeMap::new();
    let mut arrows: Vec<(&Name, &String, &SEdge)> = Vec::new();
    for (s, cols) in &src.columns {
        if tables.contains_key(s) {
            arrows.extend(cols.iter().map(|(c, a)| (s, c, a)));
        }
    }
    arrows.sort_by(|x, y| x.2.cmp(y.2));
    for (s, c, a) in arrows {
        let image_table = &tables[s];
        let target_cols = tgt.columns.get(image_table);
        let target_of = |e: &SEdge| {
            target_cols.and_then(|cols| {
                cols.iter()
                    .find(|(_, arrow)| *arrow == e)
                    .map(|(n, _)| n.clone())
            })
        };
        if let Some(image) = m.edge_remap.get(a) {
            let end_ok = m.vertex_remap.get(&a.tgt).is_none_or(|v| *v == image.tgt);
            match target_of(image) {
                Some(col) if end_ok && image.src == *image_table => {
                    out.insert((s.clone(), c.clone()), col);
                }
                _ => {
                    return Err(FunctorError::IncoherentAttributeImage {
                        edge: Box::new(a.clone()),
                        image: Some(Box::new(image.clone())),
                    });
                }
            }
        } else if m.surviving_edges.contains(a) && !is_image(&m.edge_remap, a) {
            let wanted = SEdge {
                src: image_table.clone(),
                tgt: m.vertex_image(&a.tgt).to_owned(),
                kind: a.kind.clone(),
                name: a.name.clone(),
            };
            match target_of(&wanted) {
                Some(col) => {
                    out.insert((s.clone(), c.clone()), col);
                }
                None => {
                    return Err(FunctorError::IncoherentAttributeImage {
                        edge: Box::new(a.clone()),
                        image: None,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// A side: tables with their columns, and relations.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Side {
    /// Table to columns.
    pub tables: BTreeMap<Name, BTreeSet<String>>,
    /// Relations.
    pub relations: BTreeSet<SEdge>,
}

impl RelSchema {
    /// The whole schema as a side.
    #[must_use]
    pub fn side(&self) -> Side {
        Side {
            tables: self
                .tables
                .iter()
                .map(|t| {
                    (
                        t.clone(),
                        self.columns
                            .get(t)
                            .map(|c| c.keys().cloned().collect())
                            .unwrap_or_default(),
                    )
                })
                .collect(),
            relations: self.relations.clone(),
        }
    }
}

impl RelFunctor {
    /// The kept part S' as a side: kept tables with their kept columns, and
    /// kept relations.
    #[must_use]
    pub fn kept_side(&self) -> Side {
        let mut tables: BTreeMap<Name, BTreeSet<String>> = self
            .tables
            .keys()
            .map(|t| (t.clone(), BTreeSet::new()))
            .collect();
        for (s, c) in self.columns.keys() {
            tables.entry(s.clone()).or_default().insert(c.clone());
        }
        Side {
            tables,
            relations: self.relations.keys().cloned().collect(),
        }
    }

    /// The kept source tables mapped to the target table `d`, ascending.
    #[must_use]
    pub fn fibre(&self, d: &str) -> Vec<Name> {
        self.tables
            .iter()
            .filter(|(_, t)| *t == d)
            .map(|(s, _)| s.clone())
            .collect()
    }
}

/// Why an instance does not conform to a side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conformance {
    /// A row at a table outside the side.
    TableOutside,
    /// A column of the row's table without a cell.
    MissingCell,
    /// A cell outside the row's table's columns.
    StrayCell,
    /// A labelled null below the top level of a cell.
    NestedLabeledNull,
    /// A pair at a relation outside the side.
    RelationOutside(Box<SEdge>),
    /// A pair whose rows are not at its relation's endpoint tables.
    IncoherentPair(Box<SEdge>),
}

/// The first violation of conformance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonConforming {
    /// The row (for a pair, its source row).
    pub row: u32,
    /// The column, for a cell violation.
    pub column: Option<String>,
    /// The reason.
    pub reason: Conformance,
}

/// Check that an instance conforms to a side.
///
/// Rows at its tables with exactly its columns and no nested labelled null, pairs at its relations
/// between rows of the relation's endpoint tables. The first violation is the least row; within a
/// row, the least column; a row's violation comes before a pair whose source row is not smaller.
///
/// # Errors
///
/// The first [`NonConforming`] violation.
pub fn conform(inst: &Table, side: &Side) -> Result<(), Box<NonConforming>> {
    let mut first: Option<NonConforming> = None;
    for (id, (table, cells)) in &inst.rows {
        let v = row_violation(*id, table, cells, side);
        if v.is_some() {
            first = v;
            break;
        }
    }
    let pair = inst
        .pairs
        .iter()
        .filter_map(|(e, s, t)| {
            if !side.relations.contains(e) {
                return Some((*s, Conformance::RelationOutside(Box::new(e.clone()))));
            }
            let at = |r: &u32| inst.rows.get(r).map(|(tb, _)| tb.clone());
            (at(s).as_deref() != Some(e.src.as_str()) || at(t).as_deref() != Some(e.tgt.as_str()))
                .then(|| (*s, Conformance::IncoherentPair(Box::new(e.clone()))))
        })
        .min_by_key(|(s, _)| *s);
    match (first, pair) {
        (Some(r), Some((s, _))) if r.row <= s => Err(Box::new(r)),
        (_, Some((row, reason))) => Err(Box::new(NonConforming {
            row,
            column: None,
            reason,
        })),
        (Some(r), None) => Err(Box::new(r)),
        (None, None) => Ok(()),
    }
}

fn row_violation(
    id: u32,
    table: &str,
    cells: &BTreeMap<String, Value>,
    side: &Side,
) -> Option<NonConforming> {
    let Some(columns) = side.tables.get(table) else {
        return Some(NonConforming {
            row: id,
            column: None,
            reason: Conformance::TableOutside,
        });
    };
    let missing = columns
        .iter()
        .find(|c| !cells.contains_key(*c))
        .map(|c| (c.clone(), Conformance::MissingCell));
    let stray = cells
        .keys()
        .find(|k| !columns.contains(*k))
        .map(|k| (k.clone(), Conformance::StrayCell));
    let keyed = match (missing, stray) {
        (Some(m), Some(s)) => Some(if s.0 < m.0 { s } else { m }),
        (m, s) => m.or(s),
    };
    if let Some((column, reason)) = keyed {
        return Some(NonConforming {
            row: id,
            column: Some(column),
            reason,
        });
    }
    cells
        .iter()
        .find(|(_, v)| v.has_nested_labeled_null())
        .map(|(c, _)| NonConforming {
            row: id,
            column: Some(c.clone()),
            reason: Conformance::NestedLabeledNull,
        })
}

/// The restriction of a conforming instance to the kept part: rows at kept
/// tables with their kept cells, pairs at kept relations.
#[must_use]
pub fn restrict_to_kept(inst: &Table, f: &RelFunctor) -> Table {
    let side = f.kept_side();
    let rows = inst
        .rows
        .iter()
        .filter_map(|(id, (t, cells))| {
            let cols = side.tables.get(t)?;
            let kept = cells
                .iter()
                .filter(|(c, _)| cols.contains(*c))
                .map(|(c, v)| (c.clone(), v.clone()))
                .collect();
            Some((*id, (t.clone(), kept)))
        })
        .collect();
    let pairs = inst
        .pairs
        .iter()
        .filter(|(e, _, _)| side.relations.contains(e))
        .cloned()
        .collect();
    Table { rows, pairs }
}

/// Why a Kan extension refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KanError {
    /// The input does not conform.
    NonConforming(Box<NonConforming>),
    /// Two constants were equated in a Sigma cell class.
    SigmaConstantClash {
        /// The output row.
        row: u32,
        /// The target column.
        column: String,
        /// The class's constant before the joining equation.
        left: Box<Value>,
        /// The other constant.
        right: Box<Value>,
    },
    /// A target column is not the image of a kept column of its fibre.
    AttributeOutsideImage {
        /// The target table.
        table: Name,
        /// The column.
        column: String,
    },
    /// A fresh row id would pass `u32::MAX`.
    RowIdsExhausted,
    /// A fresh labelled null would pass `u64::MAX`.
    NullIdsExhausted,
}

/// The fresh ids above a set of used ids: one above the greatest, 0 for
/// none, handed out ascending; `None` once the next would pass the type's
/// greatest value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fresh<T> {
    next: Option<T>,
}

impl Fresh<u32> {
    /// The fresh row ids above `used`.
    #[must_use]
    pub fn above(used: impl Iterator<Item = u32>) -> Self {
        Self {
            next: used.max().map_or(Some(0), |m| m.checked_add(1)),
        }
    }

    /// The next fresh id.
    pub fn take(&mut self) -> Option<u32> {
        let id = self.next?;
        self.next = id.checked_add(1);
        Some(id)
    }
}

impl Fresh<u64> {
    /// The fresh null ids above `used`.
    #[must_use]
    pub fn above_nulls(used: impl Iterator<Item = u64>) -> Self {
        Self {
            next: used.max().map_or(Some(0), |m| m.checked_add(1)),
        }
    }

    /// The next fresh null id.
    pub fn take_null(&mut self) -> Option<u64> {
        let id = self.next?;
        self.next = id.checked_add(1);
        Some(id)
    }
}

/// The provenance of a Delta row: the source table and target row copied.
pub type DeltaProvenance = BTreeMap<u32, (Name, u32)>;

/// Delta along `f` (precomposition).
///
/// For each kept source table s and each row r of `j` at `F(s)`, a copy row at s holding, at each
/// kept column c of s, r's cell at `F(s, c)`; for each kept relation e and each pair `(F(e), a, b)`
/// of `j`, the pair `(e, copy of a at e.src, copy of b at e.tgt)`. Ids: copies are made in (source
/// table, target row) order; the first copy of a row keeps its id, every other copy takes the next
/// id above every id of `j`. No null is created.
///
/// # Errors
///
/// [`KanError::NonConforming`] when `j` does not conform to the target;
/// [`KanError::RowIdsExhausted`] when a fresh id would pass `u32::MAX`.
pub fn delta(f: &RelFunctor, j: &Table) -> Result<(Table, DeltaProvenance), KanError> {
    conform(j, &f.tgt.side()).map_err(KanError::NonConforming)?;
    let mut fresh = Fresh::above(j.rows.keys().copied());
    let mut used: BTreeSet<u32> = BTreeSet::new();
    let mut copy: BTreeMap<(Name, u32), u32> = BTreeMap::new();
    let mut out = Table::default();
    let mut prov = DeltaProvenance::new();
    for (s, t) in &f.tables {
        for (r, (table, cells)) in &j.rows {
            if table != t {
                continue;
            }
            let id = if used.insert(*r) {
                *r
            } else {
                fresh.take().ok_or(KanError::RowIdsExhausted)?
            };
            let kept: BTreeMap<String, Value> = f
                .columns
                .iter()
                .filter(|((src, _), _)| src == s)
                .filter_map(|((_, c), tc)| cells.get(tc).map(|v| (c.clone(), v.clone())))
                .collect();
            out.rows.insert(id, (s.clone(), kept));
            copy.insert((s.clone(), *r), id);
            prov.insert(id, (s.clone(), *r));
        }
    }
    for (e, image) in &f.relations {
        for (pe, a, b) in &j.pairs {
            if pe == image
                && let (Some(x), Some(y)) = (
                    copy.get(&(e.src.clone(), *a)),
                    copy.get(&(e.tgt.clone(), *b)),
                )
            {
                out.pairs.insert((e.clone(), *x, *y));
            }
        }
    }
    Ok((out, prov))
}

/// The provenance of Sigma: each fresh null with its `(row, column)`, and
/// each input null that another value represents, with that value.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SigmaProvenance {
    /// Fresh null id to the output cell it was made for.
    pub fresh: BTreeMap<u64, (u32, String)>,
    /// Input null id to the value representing its class, for each input
    /// null whose class the equations join with a constant or a lesser
    /// null.
    pub merged: BTreeMap<u64, Value>,
}

/// A term of Sigma's cell presentation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Term {
    Slot(u32, String),
    Null(u64),
    Const(Vec<u8>),
}

fn term_of(v: &Value) -> Term {
    match v {
        Value::LabeledNull(n) => Term::Null(*n),
        other => Term::Const(crate::canonical::value_bytes(other).unwrap_or_default()),
    }
}

fn max_null(t: &Table) -> Option<u64> {
    t.rows
        .values()
        .flat_map(|(_, c)| c.values())
        .filter_map(|v| {
            if let Value::LabeledNull(n) = v {
                Some(*n)
            } else {
                None
            }
        })
        .max()
}

/// Sigma along `f`: the quotient of the disjoint union.
///
/// Rows: each row of the kept part keeps its id at the image of its table;
/// pairs: the set of images `(F(e), a, b)`. Cells: each output slot
/// `(r, β)` is equated with `r`'s value at every kept column α of r's
/// source table with `F(α) = β` (ascending), and a slot with no preimage
/// column is a fresh labelled null ("fresh null ids start at max(nulls of
/// the input) + 1 (0 when none), in ascending (`RowId`, target column)
/// order, checked": the input is the whole input, before its restriction
/// to the kept part). The equations generate classes over slots, nulls and
/// constants (constants by canonical bytes); processed in `(r, β, α)`
/// order, the first equation that joins two classes holding different
/// constants is `SigmaConstantClash`. A class is represented by its
/// constant, else its least null. The input is checked against the whole
/// source schema and then restricted to the kept part. The provenance
/// records each fresh null with its cell, and in `merged` each null of the
/// restricted input whose class is represented by another value (a
/// constant, or a lesser null), with that value; a null that represents
/// its own class is not recorded.
///
/// # Errors
///
/// [`KanError::NonConforming`], [`KanError::SigmaConstantClash`], or
/// [`KanError::NullIdsExhausted`] when a fresh null would pass `u64::MAX`.
pub fn sigma(f: &RelFunctor, i: &Table) -> Result<(Table, SigmaProvenance), KanError> {
    conform(i, &f.src.side()).map_err(KanError::NonConforming)?;
    let mut fresh = Fresh::above_nulls(max_null(i).into_iter());
    let i = restrict_to_kept(i, f);
    let mut prov = SigmaProvenance::default();
    // Component label of every term; merging relabels one component.
    let mut label: BTreeMap<Term, usize> = BTreeMap::new();
    let mut next_label = 0usize;
    let mut slots: Vec<(u32, String, Name)> = Vec::new();
    for (r, (s, cells)) in &i.rows {
        let t = &f.tables[s];
        for beta in f
            .tgt
            .columns
            .get(t)
            .map(|c| c.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            slots.push((*r, beta.clone(), t.clone()));
            let slot = Term::Slot(*r, beta.clone());
            label.insert(slot.clone(), next_label);
            next_label += 1;
            let preimages: Vec<&String> = f
                .columns
                .iter()
                .filter(|((src, _), b)| src == s && **b == beta)
                .map(|((_, a), _)| a)
                .collect();
            if preimages.is_empty() {
                let fresh_id = fresh.take_null().ok_or(KanError::NullIdsExhausted)?;
                prov.fresh.insert(fresh_id, (*r, beta.clone()));
                join(&mut label, &mut next_label, &slot, &Term::Null(fresh_id))
                    .map_err(|(l, rt)| clash(*r, &beta, &l, &rt))?;
            }
            for a in preimages {
                let Some(v) = cells.get(a) else { continue };
                join(&mut label, &mut next_label, &slot, &term_of(v))
                    .map_err(|(l, rt)| clash(*r, &beta, &l, &rt))?;
            }
        }
    }
    let value_of = |lbl: usize| -> Value {
        let members: Vec<&Term> = label
            .iter()
            .filter(|(_, l)| **l == lbl)
            .map(|(t, _)| t)
            .collect();
        members
            .iter()
            .find_map(|t| {
                if let Term::Const(b) = t {
                    crate::canonical::decode_value(b)
                } else {
                    None
                }
            })
            .or_else(|| {
                members.iter().find_map(|t| {
                    if let Term::Null(n) = t {
                        Some(Value::LabeledNull(*n))
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(Value::Null)
    };
    let mut out = Table::default();
    for (r, (s, _)) in &i.rows {
        out.rows.insert(*r, (f.tables[s].clone(), BTreeMap::new()));
    }
    for (r, beta, _) in &slots {
        let v = value_of(label[&Term::Slot(*r, beta.clone())]);
        if let Some(row) = out.rows.get_mut(r) {
            row.1.insert(beta.clone(), v);
        }
    }
    for (t, l) in &label {
        if let Term::Null(n) = t
            && !prov.fresh.contains_key(n)
        {
            let representative = value_of(*l);
            if representative != Value::LabeledNull(*n) {
                prov.merged.insert(*n, representative);
            }
        }
    }
    for (e, a, b) in &i.pairs {
        out.pairs.insert((f.relations[e].clone(), *a, *b));
    }
    Ok((out, prov))
}

fn clash(row: u32, column: &str, left: &[u8], right: &[u8]) -> KanError {
    let dec = |b: &[u8]| crate::canonical::decode_value(b).unwrap_or(Value::Null);
    KanError::SigmaConstantClash {
        row,
        column: column.to_owned(),
        left: Box::new(dec(left)),
        right: Box::new(dec(right)),
    }
}

/// Join the classes of `a` and `b`; refuse when both hold a constant and the
/// constants differ, returning `(a's constant, b's constant)`.
fn join(
    label: &mut BTreeMap<Term, usize>,
    next: &mut usize,
    a: &Term,
    b: &Term,
) -> Result<(), (Vec<u8>, Vec<u8>)> {
    for t in [a, b] {
        if !label.contains_key(t) {
            label.insert(t.clone(), *next);
            *next += 1;
        }
    }
    let (la, lb) = (label[a], label[b]);
    if la == lb {
        return Ok(());
    }
    let constant = |l: usize, label: &BTreeMap<Term, usize>| {
        label.iter().find_map(|(t, x)| {
            if *x == l {
                if let Term::Const(c) = t {
                    Some(c.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
    };
    if let (Some(ca), Some(cb)) = (constant(la, label), constant(lb, label)) {
        return Err((ca, cb));
    }
    for v in label.values_mut() {
        if *v == lb {
            *v = la;
        }
    }
    Ok(())
}

/// The provenance of Pi: each output row with its target table and its
/// tuple of component rows, one per fibre table in ascending order.
pub type PiProvenance = BTreeMap<u32, (Name, Vec<u32>)>;

/// Whether every column of every target table is the image of a kept column
/// of its fibre; the least `(table, column)` that is not, otherwise.
///
/// # Errors
///
/// [`KanError::AttributeOutsideImage`] naming the least such column.
pub fn attribute_surjective(f: &RelFunctor) -> Result<(), KanError> {
    for (t, cols) in &f.tgt.columns {
        for c in cols.keys() {
            let covered = f
                .columns
                .iter()
                .any(|((s, _), tc)| tc == c && f.tables.get(s) == Some(t));
            if !covered {
                return Err(KanError::AttributeOutsideImage {
                    table: t.clone(),
                    column: c.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Every tuple choosing one row of `inst` at each table of `fibre`, in
/// lexicographic order of the chosen row ids.
fn tuples(inst: &Table, fibre: &[Name]) -> Vec<Vec<u32>> {
    let mut out: Vec<Vec<u32>> = vec![Vec::new()];
    for s in fibre {
        let rows: Vec<u32> = inst
            .rows
            .iter()
            .filter(|(_, (t, _))| t == s)
            .map(|(id, _)| *id)
            .collect();
        out = out
            .into_iter()
            .flat_map(|prefix| {
                rows.iter().map(move |r| {
                    let mut p = prefix.clone();
                    p.push(*r);
                    p
                })
            })
            .collect();
    }
    out
}

/// Pi along `f`: compatible families over the comma categories.
///
/// For a target table d the comma category under d holds the kept source
/// tables over d and no arrow between them, so a family is a tuple of one
/// row per fibre table; the attributed limit keeps the tuples whose values
/// at the preimages of each column of d are equal (by canonical bytes),
/// that value being the cell (a table outside the image has one empty
/// tuple). For a target relation ρ the families are pairs `(u, v)` of
/// rows over `ρ.src` and `ρ.tgt` with `(u at x.src, v at x.tgt)` a pair of
/// the input at every kept relation x carried to ρ (every pair when there
/// is none). Requires attribute surjectivity; the input is checked against
/// the whole source schema and restricted to the kept part.
///
/// Row ids ("Fresh `RowIds` come from `max(RowId in input) + 1` (0 for an
/// empty input), checked, in a fixed order"): a row of a target table
/// whose fibre is one table keeps the id of the row it projects to, so Pi
/// along the identity returns its input exactly; every other row takes
/// the next fresh id, the target tables in the image first, ordered by the
/// least table of their fibre, each table's tuples ascending, then the
/// tables outside the image by name.
///
/// # Errors
///
/// [`KanError::AttributeOutsideImage`] or [`KanError::NonConforming`];
/// [`KanError::RowIdsExhausted`] when a fresh id would pass `u32::MAX`.
pub fn pi(f: &RelFunctor, i: &Table) -> Result<(Table, PiProvenance), KanError> {
    attribute_surjective(f)?;
    conform(i, &f.src.side()).map_err(KanError::NonConforming)?;
    let mut fresh = Fresh::above(i.rows.keys().copied());
    let i = restrict_to_kept(i, f);
    let mut out = Table::default();
    let mut prov = PiProvenance::new();
    let mut ids: BTreeMap<(Name, Vec<u32>), u32> = BTreeMap::new();
    let mut order: Vec<(Option<Name>, Name)> = f
        .tgt
        .tables
        .iter()
        .map(|d| (f.fibre(d).first().cloned(), d.clone()))
        .collect();
    order.sort_by(|(fa, da), (fb, db)| match (fa, fb) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => da.cmp(db),
    });
    for (_, d) in order {
        let fibre = f.fibre(&d);
        for tuple in tuples(&i, &fibre) {
            let Some(cells) = joined_cells(f, &i, &d, &fibre, &tuple) else {
                continue;
            };
            let id = match tuple.as_slice() {
                [only] => *only,
                _ => fresh.take().ok_or(KanError::RowIdsExhausted)?,
            };
            out.rows.insert(id, (d.clone(), cells));
            prov.insert(id, (d.clone(), tuple.clone()));
            ids.insert((d.clone(), tuple), id);
        }
    }
    for rho in &f.tgt.relations {
        let carried: Vec<&SEdge> = f
            .relations
            .iter()
            .filter(|(_, img)| *img == rho)
            .map(|(x, _)| x)
            .collect();
        let (fs, ft) = (f.fibre(&rho.src), f.fibre(&rho.tgt));
        let rows_at = |t: &Name| {
            ids.iter()
                .filter(move |((d, _), _)| d == t)
                .map(|((_, tu), id)| (tu.clone(), *id))
                .collect::<Vec<_>>()
        };
        for (u, uid) in rows_at(&rho.src) {
            for (v, vid) in rows_at(&rho.tgt) {
                let ok = carried.iter().all(|x| {
                    let a = fs.iter().position(|s| *s == x.src).map(|k| u[k]);
                    let b = ft.iter().position(|s| *s == x.tgt).map(|k| v[k]);
                    matches!((a, b), (Some(a), Some(b)) if i.pairs.contains(&((*x).clone(), a, b)))
                });
                if ok {
                    out.pairs.insert((rho.clone(), uid, vid));
                }
            }
        }
    }
    Ok((out, prov))
}

fn joined_cells(
    func: &RelFunctor,
    inst: &Table,
    target: &str,
    fibre: &[Name],
    tuple: &[u32],
) -> Option<BTreeMap<String, Value>> {
    let mut cells = BTreeMap::new();
    for beta in func
        .tgt
        .columns
        .get(target)
        .map(|c| c.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let mut found: Option<Value> = None;
        for (pos, table) in fibre.iter().enumerate() {
            let row = &inst.rows.get(&tuple[pos])?.1;
            for ((src, col), image) in &func.columns {
                if src == table && *image == beta {
                    let val = row.get(col)?;
                    match &found {
                        None => found = Some(val.clone()),
                        Some(prior)
                            if crate::canonical::value_bytes(prior)
                                == crate::canonical::value_bytes(val) => {}
                        Some(_) => return None,
                    }
                }
            }
        }
        cells.insert(beta, found?);
    }
    Some(cells)
}

/// A structure-preserving bijection between two attributed instances.
///
/// A bijection of rows preserving tables, pairs and cells, with a bijection of labelled nulls under
/// which cells agree (every other value compared by canonical bytes).
///
/// Found by naive backtracking over row bijections; `None` when none
/// exists.
#[must_use]
pub fn row_bijection(a: &Table, b: &Table) -> Option<BTreeMap<u32, u32>> {
    if a.rows.len() != b.rows.len() || a.pairs.len() != b.pairs.len() {
        return None;
    }
    let order: Vec<u32> = a.rows.keys().copied().collect();
    let mut map = BTreeMap::new();
    let mut nulls = BTreeMap::new();
    search_bijection(a, b, &order, 0, &mut map, &mut nulls).then_some(map)
}

fn search_bijection(
    left: &Table,
    right: &Table,
    order: &[u32],
    pos: usize,
    map: &mut BTreeMap<u32, u32>,
    nulls: &mut BTreeMap<u64, u64>,
) -> bool {
    let Some(&row) = order.get(pos) else {
        return left
            .pairs
            .iter()
            .all(|(edge, from, to)| right.pairs.contains(&(edge.clone(), map[from], map[to])));
    };
    let (ta, ca) = &left.rows[&row];
    for (cand, (tb, cb)) in &right.rows {
        if tb != ta || map.values().any(|val| val == cand) || ca.keys().ne(cb.keys()) {
            continue;
        }
        let saved = nulls.clone();
        if ca
            .iter()
            .all(|(col, val)| match_values(val, &cb[col], nulls))
        {
            map.insert(row, *cand);
            if search_bijection(left, right, order, pos + 1, map, nulls) {
                return true;
            }
            map.remove(&row);
        }
        *nulls = saved;
    }
    false
}

fn match_values(left: &Value, right: &Value, nulls: &mut BTreeMap<u64, u64>) -> bool {
    match (left, right) {
        (Value::LabeledNull(lnull), Value::LabeledNull(rnull)) => {
            if let Some(mapped) = nulls.get(lnull) {
                return mapped == rnull;
            }
            if nulls.values().any(|mapped| mapped == rnull) {
                return false;
            }
            nulls.insert(*lnull, *rnull);
            true
        }
        (Value::LabeledNull(_), _) | (_, Value::LabeledNull(_)) => false,
        _ => crate::canonical::value_bytes(left) == crate::canonical::value_bytes(right),
    }
}

/// Every homomorphism between two instances of one schema.
///
/// A map of rows preserving tables and pairs, under which each cell of the domain equals the image
/// row's cell, a labelled null of the domain being sent to the value it first meets (consistently),
/// every other value compared by canonical bytes. Enumerated naively over all row maps.
#[must_use]
pub fn homs(a: &Table, b: &Table) -> Vec<BTreeMap<u32, u32>> {
    let order: Vec<u32> = a.rows.keys().copied().collect();
    let mut out = Vec::new();
    hom_search(
        a,
        b,
        &order,
        0,
        &mut BTreeMap::new(),
        &BTreeMap::new(),
        &mut out,
    );
    out
}

fn hom_search(
    dom: &Table,
    cod: &Table,
    order: &[u32],
    pos: usize,
    map: &mut BTreeMap<u32, u32>,
    nulls: &BTreeMap<u64, Vec<u8>>,
    out: &mut Vec<BTreeMap<u32, u32>>,
) {
    let Some(&row) = order.get(pos) else {
        if dom
            .pairs
            .iter()
            .all(|(edge, from, to)| cod.pairs.contains(&(edge.clone(), map[from], map[to])))
        {
            out.push(map.clone());
        }
        return;
    };
    let (ta, ca) = &dom.rows[&row];
    for (cand, (tb, cb)) in &cod.rows {
        if tb != ta {
            continue;
        }
        let mut fixed = nulls.clone();
        let ok = ca.iter().all(|(col, val)| {
            let Some(img) = cb.get(col) else { return false };
            let wb = crate::canonical::value_bytes(img).unwrap_or_default();
            match val {
                Value::LabeledNull(id) => {
                    if let Some(prev) = fixed.get(id) {
                        return *prev == wb;
                    }
                    fixed.insert(*id, wb);
                    true
                }
                other => crate::canonical::value_bytes(other).unwrap_or_default() == wb,
            }
        });
        if ok {
            map.insert(row, *cand);
            hom_search(dom, cod, order, pos + 1, map, &fixed, out);
            map.remove(&row);
        }
    }
}
