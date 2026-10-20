//! Complete finite scopes for the Kan-extension page.
//!
//! There are two named scopes. [`family`] is the ten-case table-Kan family
//! from O4: nine structural cases over integers and labelled nulls, plus the
//! column-merge case over exact binary64 bit patterns. [`encoding_graphs`]
//! is O7's graph-encoding family. Both enumerate labelled objects. Row, node
//! and edge identifiers are therefore part of an object; no object is removed
//! merely because an identifier permutation makes it isomorphic to another.

use std::collections::{BTreeMap, BTreeSet};

use crate::migration::Migration;
use crate::model::{GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::schema::Schema;
use crate::value::{Presence, Value};

/// The small product bound and the larger product bound required by P2-O205.
pub const PRODUCT_BOUNDS: [usize; 2] = [4, 81];

/// Which side of a Kan-family case an object inhabits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CaseSide {
    /// The source of the functor.
    Source,
    /// The target of the functor.
    Target,
}

/// A finite relational side: ordered tables with ordered column names, and
/// relation arrows. The order is part of the enumerator's labelled layout,
/// not part of the mathematical schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Side {
    /// Tables and their columns.
    pub tables: Vec<(&'static str, Vec<&'static str>)>,
    /// Relation arrows.
    pub relations: Vec<SEdge>,
}

impl Side {
    /// Its graph schema. Tables have kind `record`, value vertices kind
    /// `value`, columns kind `prop`, and relations kind `fk`.
    #[must_use]
    pub fn schema(&self) -> Schema {
        let mut schema = Schema::default();
        for (table, columns) in &self.tables {
            schema
                .vertices
                .insert((*table).to_owned(), "record".to_owned());
            for column in columns {
                let value = format!("{table}.{column}");
                schema.vertices.insert(value.clone(), "value".to_owned());
                schema
                    .edges
                    .insert(SEdge::new(table, &value, "prop", Some(column)));
            }
        }
        schema.edges.extend(self.relations.iter().cloned());
        schema
    }

    fn table_index(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|(table, _)| *table == name)
    }
}

/// A migration of the complete finite Kan family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// Stable descriptive name.
    pub name: &'static str,
    /// Source relational side.
    pub source: Side,
    /// Target relational side.
    pub target: Side,
    /// Source-table to target-table map.
    pub tables: Vec<(&'static str, &'static str)>,
    /// Source-relation to target-relation map.
    pub relations: Vec<(SEdge, SEdge)>,
    /// `(source table, source column, target column)` map.
    pub columns: Vec<(&'static str, &'static str, &'static str)>,
    /// Cell alphabet for this case.
    pub cells: Vec<Value>,
    /// Whether Pi is defined (a target column outside the image makes it
    /// undefined independently of the product bound).
    pub pi: bool,
}

impl Case {
    /// The graph-schema source.
    #[must_use]
    pub fn source_schema(&self) -> Schema {
        self.source.schema()
    }

    /// The graph-schema target.
    #[must_use]
    pub fn target_schema(&self) -> Schema {
        self.target.schema()
    }

    /// The labelled migration represented by the three maps.
    #[must_use]
    pub fn migration(&self) -> Migration {
        let mut migration = Migration::default();
        for (source, target) in &self.tables {
            migration.surviving_verts.insert((*target).to_owned());
            if source != target {
                migration
                    .vertex_remap
                    .insert((*source).to_owned(), (*target).to_owned());
            }
        }
        for (source, target) in &self.relations {
            migration.surviving_edges.insert(target.clone());
            if source != target {
                migration.edge_remap.insert(source.clone(), target.clone());
            }
        }
        for (table, column, image) in &self.columns {
            let Some((_, target_table)) = self.tables.iter().find(|(source, _)| source == table)
            else {
                continue;
            };
            let source_value = format!("{table}.{column}");
            let target_value = format!("{target_table}.{image}");
            migration.surviving_verts.insert(target_value.clone());
            if source_value != target_value {
                migration
                    .vertex_remap
                    .insert(source_value.clone(), target_value.clone());
            }
            let source_edge = SEdge::new(table, &source_value, "prop", Some(column));
            let target_edge = SEdge::new(target_table, &target_value, "prop", Some(image));
            migration.surviving_edges.insert(target_edge.clone());
            if source_edge != target_edge {
                migration.edge_remap.insert(source_edge, target_edge);
            }
        }
        migration
    }

    /// Select one side.
    #[must_use]
    pub const fn side(&self, side: CaseSide) -> &Side {
        match side {
            CaseSide::Source => &self.source,
            CaseSide::Target => &self.target,
        }
    }
}

fn fk(source: &str, target: &str, name: &str) -> SEdge {
    SEdge::new(source, target, "fk", Some(name))
}

fn side(tables: &[(&'static str, &[&'static str])], relations: &[SEdge]) -> Side {
    Side {
        tables: tables
            .iter()
            .map(|(table, columns)| (*table, columns.to_vec()))
            .collect(),
        relations: relations.to_vec(),
    }
}

fn tiny_cells() -> Vec<Value> {
    vec![
        Value::Int(0),
        Value::Int(1),
        Value::LabeledNull(0),
        Value::LabeledNull(1),
    ]
}

fn float_cells() -> Vec<Value> {
    vec![
        Value::Float(0),
        Value::Float(1u64 << 63),
        Value::Float(0x7ff8_0000_0000_0000),
        Value::Float(0xfff8_0000_0000_0000),
    ]
}

fn float_variant(case: &Case) -> Case {
    let mut float = case.clone();
    float.name = "columns-merged-float-bits";
    float.cells = float_cells();
    float
}

/// The ten-case exhaustive family: merges, renames, inclusions,
/// non-surjective maps, relation maps, a partial self relation, a column
/// merge, identity, and the column merge again over exact float bits.
#[must_use]
pub fn family() -> Vec<Case> {
    let l = fk("a", "b", "l");
    let big_l = fk("m", "m", "L");
    let k = fk("e", "e", "k");
    let r = fk("a", "b", "r");
    let cases = vec![
        Case {
            name: "merge-disjoint-columns",
            source: side(&[("a", &["x"]), ("b", &["y"])], &[]),
            target: side(&[("m", &["x", "y"])], &[]),
            tables: vec![("a", "m"), ("b", "m")],
            relations: vec![],
            columns: vec![("a", "x", "x"), ("b", "y", "y")],
            cells: tiny_cells(),
            pi: true,
        },
        Case {
            name: "merge-shared-column",
            source: side(&[("a", &["x"]), ("b", &["x"])], &[]),
            target: side(&[("m", &["x"])], &[]),
            tables: vec![("a", "m"), ("b", "m")],
            relations: vec![],
            columns: vec![("a", "x", "x"), ("b", "x", "x")],
            cells: tiny_cells(),
            pi: true,
        },
        Case {
            name: "column-rename",
            source: side(&[("post", &["title"])], &[]),
            target: side(&[("post", &["name"])], &[]),
            tables: vec![("post", "post")],
            relations: vec![],
            columns: vec![("post", "title", "name")],
            cells: tiny_cells(),
            pi: true,
        },
        Case {
            name: "target-column-outside-image",
            source: side(&[("a", &["x"])], &[]),
            target: side(&[("a", &["x", "y"])], &[]),
            tables: vec![("a", "a")],
            relations: vec![],
            columns: vec![("a", "x", "x")],
            cells: tiny_cells(),
            pi: false,
        },
        Case {
            name: "target-table-outside-image",
            source: side(&[("a", &["x"])], &[]),
            target: side(&[("a", &["x"]), ("z", &[])], &[]),
            tables: vec![("a", "a")],
            relations: vec![],
            columns: vec![("a", "x", "x")],
            cells: tiny_cells(),
            pi: true,
        },
        Case {
            name: "relation-carried-to-loop",
            source: side(&[("a", &["x"]), ("b", &[])], std::slice::from_ref(&l)),
            target: side(&[("m", &["x"])], std::slice::from_ref(&big_l)),
            tables: vec![("a", "m"), ("b", "m")],
            relations: vec![(l, big_l)],
            columns: vec![("a", "x", "x")],
            cells: tiny_cells(),
            pi: true,
        },
        Case {
            name: "partial-self-relation",
            source: side(&[("e", &["x"])], std::slice::from_ref(&k)),
            target: side(&[("e", &["x"])], std::slice::from_ref(&k)),
            tables: vec![("e", "e")],
            relations: vec![(k.clone(), k)],
            columns: vec![("e", "x", "x")],
            cells: tiny_cells(),
            pi: true,
        },
        Case {
            name: "columns-merged",
            source: side(&[("t", &["a1", "a2"])], &[]),
            target: side(&[("t", &["b"])], &[]),
            tables: vec![("t", "t")],
            relations: vec![],
            columns: vec![("t", "a1", "b"), ("t", "a2", "b")],
            cells: tiny_cells(),
            pi: true,
        },
        Case {
            name: "identity-with-relation",
            source: side(&[("a", &["x"]), ("b", &[])], std::slice::from_ref(&r)),
            target: side(&[("a", &["x"]), ("b", &[])], std::slice::from_ref(&r)),
            tables: vec![("a", "a"), ("b", "b")],
            relations: vec![(r.clone(), r)],
            columns: vec![("a", "x", "x")],
            cells: tiny_cells(),
            pi: true,
        },
    ];
    // O14 names this duplicate structural case `8′`, immediately before
    // case 9. Keep that order observable so printed certificates can be
    // compared line-for-line with the contract table.
    let mut cases = cases;
    let float = float_variant(&cases[7]);
    cases.insert(8, float);
    cases
}

/// Every distinct schema named by [`family`], in first-occurrence order.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    let mut out = Vec::new();
    for case in family() {
        for schema in [case.source_schema(), case.target_schema()] {
            if !out.contains(&schema) {
                out.push(schema);
            }
        }
    }
    out
}

/// Every migration named by [`family`]. Cases with the same structural map
/// but a different value alphabet remain separate in [`family`]; this list is
/// the set of migration values.
#[must_use]
pub fn migrations() -> Vec<Migration> {
    let mut out = Vec::new();
    for migration in family().iter().map(Case::migration) {
        if !out.contains(&migration) {
            out.push(migration);
        }
    }
    out
}

fn product<T: Clone>(choices: &[Vec<T>]) -> Vec<Vec<T>> {
    let mut out = vec![Vec::new()];
    for options in choices {
        out = out
            .into_iter()
            .flat_map(|prefix| {
                options.iter().map(move |option| {
                    let mut next = prefix.clone();
                    next.push(option.clone());
                    next
                })
            })
            .collect();
    }
    out
}

fn row_options(
    columns: &[&str],
    count: usize,
    cells: &[Value],
) -> Vec<Vec<BTreeMap<String, Value>>> {
    let row_choices = product(
        &columns
            .iter()
            .map(|column| {
                cells
                    .iter()
                    .map(|value| ((*column).to_owned(), value.clone()))
                    .collect()
            })
            .collect::<Vec<Vec<(String, Value)>>>(),
    )
    .into_iter()
    .map(|entries| entries.into_iter().collect())
    .collect::<Vec<BTreeMap<String, Value>>>();
    product(&vec![row_choices; count])
}

fn row_count_vectors(tables: usize, max_per_table: usize, max_total: usize) -> Vec<Vec<usize>> {
    product(&vec![(0..=max_per_table).collect::<Vec<_>>(); tables])
        .into_iter()
        .filter(|counts| counts.iter().sum::<usize>() <= max_total)
        .collect()
}

/// Every labelled instance of `side`.
///
/// Rows are numbered from zero in table order, each table has at most
/// `max_per_table` rows, the total is at most `max_total`, cells range over
/// `cells`, and every relation contains every subset of its possible labelled
/// row pairs.
#[must_use]
pub fn instances(
    side: &Side,
    max_per_table: usize,
    max_total: usize,
    cells: &[Value],
) -> Vec<Table> {
    let mut out = Vec::new();
    for counts in row_count_vectors(side.tables.len(), max_per_table, max_total) {
        let table_rows: Vec<Vec<Vec<BTreeMap<String, Value>>>> = side
            .tables
            .iter()
            .zip(&counts)
            .map(|((_, columns), count)| row_options(columns, *count, cells))
            .collect();
        for selected_rows in product(&table_rows) {
            let mut table = Table::default();
            let mut ids: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
            let mut next = 0u32;
            for ((name, _), rows) in side.tables.iter().zip(selected_rows) {
                for cells in rows {
                    table.rows.insert(next, ((*name).to_owned(), cells));
                    ids.entry(name).or_default().push(next);
                    let Some(fresh) = next.checked_add(1) else {
                        break;
                    };
                    next = fresh;
                }
            }
            let relation_choices: Vec<Vec<Vec<(SEdge, u32, u32)>>> = side
                .relations
                .iter()
                .map(|relation| {
                    let possible = ids
                        .get(relation.src.as_str())
                        .into_iter()
                        .flatten()
                        .flat_map(|source| {
                            ids.get(relation.tgt.as_str())
                                .into_iter()
                                .flatten()
                                .map(move |target| (relation.clone(), *source, *target))
                        })
                        .collect::<Vec<_>>();
                    super::subsets(&possible, possible.len())
                })
                .collect();
            for pairs in product(&relation_choices) {
                let mut candidate = table.clone();
                candidate.pairs = pairs.into_iter().flatten().collect();
                out.push(candidate);
            }
        }
    }
    out
}

fn instance_count(side: &Side, max_per_table: usize, max_total: usize, cell_count: u64) -> u64 {
    // This counter deliberately does not call `row_count_vectors`, `product`,
    // `row_options`, or `instances`: it is the independent product proof for
    // the literal generator. At each table it chooses a row count, and only at
    // a leaf applies |C|^(cell slots) 2^(relation-pair slots).
    fn visit(
        side: &Side,
        cell_count: u64,
        max_per_table: usize,
        max_total: usize,
        table: usize,
        used: usize,
        counts: &mut [usize],
    ) -> u64 {
        if table != side.tables.len() {
            let available = max_total.saturating_sub(used).min(max_per_table);
            return (0..=available)
                .map(|count| {
                    counts[table] = count;
                    visit(
                        side,
                        cell_count,
                        max_per_table,
                        max_total,
                        table + 1,
                        used + count,
                        counts,
                    )
                })
                .sum();
        }
        let cell_slots = side
            .tables
            .iter()
            .enumerate()
            .map(|(index, (_, columns))| columns.len() * counts[index])
            .sum::<usize>();
        let pair_slots = side
            .relations
            .iter()
            .map(|relation| {
                let source = side
                    .table_index(&relation.src)
                    .map_or(0, |index| counts[index]);
                let target = side
                    .table_index(&relation.tgt)
                    .map_or(0, |index| counts[index]);
                source * target
            })
            .sum::<usize>();
        let (Ok(cell_exponent), Ok(pair_exponent)) =
            (u32::try_from(cell_slots), u32::try_from(pair_slots))
        else {
            return 0;
        };
        cell_count.pow(cell_exponent) * 2u64.pow(pair_exponent)
    }

    visit(
        side,
        cell_count,
        max_per_table,
        max_total,
        0,
        0,
        &mut vec![0; side.tables.len()],
    )
}

/// Independent `(source, target)` object counts for every O14 case, in
/// contract order `1..8, 8′, 9`.
#[must_use]
pub fn case_object_counts(max_per_table: usize, max_total: usize) -> Vec<(u64, u64)> {
    family()
        .iter()
        .map(|case| {
            (
                instance_count(
                    &case.source,
                    max_per_table,
                    max_total,
                    case.cells.len() as u64,
                ),
                instance_count(
                    &case.target,
                    max_per_table,
                    max_total,
                    case.cells.len() as u64,
                ),
            )
        })
        .collect()
}

/// One labelled occurrence of an object in the family. Equal table values
/// in different cases or on different sides are separate occurrences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    /// Index in [`family`].
    pub case: usize,
    /// Side of that case.
    pub side: CaseSide,
    /// Labelled table instance.
    pub table: Table,
}

/// The complete row-relabeling profile of one labelled object.
///
/// The case and side retain the functor context; `canonical` retains every
/// table name, constant byte, labelled-null equality class, and relation edge.
/// Only row identifiers within one table and labelled-null identifiers are
/// quotiented.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HomProfile {
    /// Index in [`family`].
    pub case: usize,
    /// Source or target side of that case.
    pub side: CaseSide,
    /// Least canonical table bytes over every within-table row permutation.
    pub canonical: Vec<u8>,
}

/// An explicit structure-preserving row relabelling into a
/// [`HomProfile`]'s canonical representative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileRelabeling {
    /// Labelled row id to the canonical row id in the same table.
    pub rows: BTreeMap<u32, u32>,
    /// Finite labelled-null-id permutation into the representative. Constants
    /// remain byte-literal: independently renaming them would not preserve
    /// cross-object hom sets.
    pub nulls: BTreeMap<u64, u64>,
}

impl ProfileRelabeling {
    /// Apply this relabelling to rows, relation endpoints and labelled-null
    /// identifiers. Constants, table names and relation labels are unchanged.
    #[must_use]
    pub fn apply(&self, table: &Table) -> Option<Table> {
        let rows = table
            .rows
            .iter()
            .map(|(row, (owner, cells))| {
                Some((
                    *self.rows.get(row)?,
                    (
                        owner.clone(),
                        cells
                            .iter()
                            .map(|(column, value)| {
                                let image = match value {
                                    Value::LabeledNull(id) => self
                                        .nulls
                                        .get(id)
                                        .copied()
                                        .map_or_else(|| value.clone(), Value::LabeledNull),
                                    _ => value.clone(),
                                };
                                (column.clone(), image)
                            })
                            .collect(),
                    ),
                ))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        let pairs = table
            .pairs
            .iter()
            .map(|(relation, source, target)| {
                Some((
                    relation.clone(),
                    *self.rows.get(source)?,
                    *self.rows.get(target)?,
                ))
            })
            .collect::<Option<BTreeSet<_>>>()?;
        Some(Table { rows, pairs })
    }

    /// The explicit row bijection from `self`'s labelled object to `other`'s
    /// labelled object when both relabel into the same canonical table.
    #[must_use]
    pub fn between(&self, other: &Self) -> Option<ProfileTransport> {
        let inverse: BTreeMap<u32, u32> = other
            .rows
            .iter()
            .map(|(labelled, canonical)| (*canonical, *labelled))
            .collect();
        if inverse.len() != other.rows.len() {
            return None;
        }
        let rows = self
            .rows
            .iter()
            .map(|(labelled, canonical)| Some((*labelled, *inverse.get(canonical)?)))
            .collect::<Option<_>>()?;
        let inverse_nulls: BTreeMap<u64, u64> = other
            .nulls
            .iter()
            .map(|(labelled, canonical)| (*canonical, *labelled))
            .collect();
        let nulls = self
            .nulls
            .iter()
            .map(|(labelled, canonical)| Some((*labelled, *inverse_nulls.get(canonical)?)))
            .collect::<Option<_>>()?;
        Some(ProfileTransport { rows, nulls })
    }
}

/// The explicit structure-preserving transport between two members of one
/// profile class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileTransport {
    /// Domain row id to codomain row id.
    pub rows: BTreeMap<u32, u32>,
    /// Domain labelled-null id to codomain labelled-null id.
    pub nulls: BTreeMap<u64, u64>,
}

impl ProfileTransport {
    /// Apply the transport to rows, relation endpoints, and every top-level
    /// labelled-null cell. Constants remain byte-literal.
    #[must_use]
    pub fn apply(&self, table: &Table) -> Option<Table> {
        let rows = table
            .rows
            .iter()
            .map(|(row, (owner, cells))| {
                Some((
                    *self.rows.get(row)?,
                    (
                        owner.clone(),
                        cells
                            .iter()
                            .map(|(column, value)| {
                                let value = match value {
                                    Value::LabeledNull(id) => {
                                        Value::LabeledNull(*self.nulls.get(id)?)
                                    }
                                    _ => value.clone(),
                                };
                                Some((column.clone(), value))
                            })
                            .collect::<Option<BTreeMap<_, _>>>()?,
                    ),
                ))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        let pairs = table
            .pairs
            .iter()
            .map(|(relation, source, target)| {
                Some((
                    relation.clone(),
                    *self.rows.get(source)?,
                    *self.rows.get(target)?,
                ))
            })
            .collect::<Option<BTreeSet<_>>>()?;
        Some(Table { rows, pairs })
    }

    /// Invert both finite bijections.
    #[must_use]
    pub fn inverse(&self) -> Option<Self> {
        let rows: BTreeMap<u32, u32> = self
            .rows
            .iter()
            .map(|(source, target)| (*target, *source))
            .collect();
        let nulls: BTreeMap<u64, u64> = self
            .nulls
            .iter()
            .map(|(source, target)| (*target, *source))
            .collect();
        (rows.len() == self.rows.len() && nulls.len() == self.nulls.len())
            .then_some(Self { rows, nulls })
    }

    /// This transport as an attributed-instance homomorphism.
    #[must_use]
    pub fn as_hom(&self) -> crate::rel::Hom {
        crate::rel::Hom {
            rows: self.rows.clone(),
            nulls: self
                .nulls
                .iter()
                .map(|(source, target)| (*source, Value::LabeledNull(*target)))
                .collect(),
        }
    }

    /// Check exact structural transport and both inverse composites.
    #[must_use]
    pub fn check(&self, source: &Table, target: &Table) -> bool {
        let Some(inverse) = self.inverse() else {
            return false;
        };
        self.apply(source).as_ref() == Some(target)
            && inverse.apply(target).as_ref() == Some(source)
            && self.as_hom().check(source, target).is_ok()
            && inverse.as_hom().check(target, source).is_ok()
            && self
                .as_hom()
                .compose(&inverse.as_hom())
                .is_ok_and(|hom| hom.same_as(&identity_hom(source), source))
            && inverse
                .as_hom()
                .compose(&self.as_hom())
                .is_ok_and(|hom| hom.same_as(&identity_hom(target), target))
    }
}

fn identity_hom(table: &Table) -> crate::rel::Hom {
    crate::rel::Hom {
        rows: table.rows.keys().map(|row| (*row, *row)).collect(),
        nulls: BTreeMap::new(),
    }
}

/// Conjugate `hom: A -> B` along isomorphisms `A -> A'` and `B -> B'`:
/// `source^-1 ; hom ; target`. Composition also transports a null-valued
/// image through the codomain null bijection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileTransportError {
    /// A stored row/null map is not bijective.
    NotBijection,
    /// Hom composition is not total on the intermediate row carrier.
    Hom(crate::rel::HomError),
}

/// Conjugate a homomorphism under two profile transports. The caller must
/// first establish both transports with [`ProfileTransport::check`] on their
/// respective domain/codomain object pairs.
///
/// # Errors
///
/// Returns [`ProfileTransportError::NotBijection`] if either transport is not
/// invertible, or [`ProfileTransportError::Hom`] if either composition is not
/// a total homomorphism.
pub fn conjugate_hom(
    hom: &crate::rel::Hom,
    source: &ProfileTransport,
    target: &ProfileTransport,
) -> Result<crate::rel::Hom, ProfileTransportError> {
    source
        .inverse()
        .ok_or(ProfileTransportError::NotBijection)?
        .as_hom()
        .compose(hom)
        .map_err(ProfileTransportError::Hom)?
        .compose(&target.as_hom())
        .map_err(ProfileTransportError::Hom)
}

fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    fn visit<T: Clone>(remaining: &mut Vec<T>, prefix: &mut Vec<T>, out: &mut Vec<Vec<T>>) {
        if remaining.is_empty() {
            out.push(prefix.clone());
            return;
        }
        for index in 0..remaining.len() {
            let item = remaining.remove(index);
            prefix.push(item.clone());
            visit(remaining, prefix, out);
            prefix.pop();
            remaining.insert(index, item);
        }
    }
    let mut out = Vec::new();
    visit(&mut items.to_vec(), &mut Vec::new(), &mut out);
    out
}

fn profile_relabelings(table: &Table, side: &Side) -> Vec<ProfileRelabeling> {
    let mut groups = Vec::new();
    let mut next = 0u32;
    for (name, _) in &side.tables {
        let rows: Vec<u32> = table
            .rows
            .iter()
            .filter_map(|(row, (owner, _))| (owner == name).then_some(*row))
            .collect();
        let targets: Vec<u32> = (next..next + u32::try_from(rows.len()).unwrap_or(0)).collect();
        next += u32::try_from(rows.len()).unwrap_or(0);
        let choices = permutations(&rows)
            .into_iter()
            .map(|ordered| {
                ordered
                    .into_iter()
                    .zip(targets.iter().copied())
                    .collect::<BTreeMap<_, _>>()
            })
            .collect::<Vec<_>>();
        groups.push(choices);
    }
    let mut nulls = table
        .rows
        .values()
        .flat_map(|(_, cells)| cells.values())
        .filter_map(|value| match value {
            Value::LabeledNull(id) => Some(*id),
            _ => None,
        })
        .collect::<Vec<_>>();
    nulls.sort_unstable();
    nulls.dedup();
    let canonical_nulls: Vec<u64> = (0..u64::try_from(nulls.len()).unwrap_or(0)).collect();
    product(&groups)
        .into_iter()
        .map(|parts| parts.into_iter().flatten().collect::<BTreeMap<_, _>>())
        .flat_map(|rows| {
            permutations(&nulls).into_iter().map({
                let canonical_nulls = canonical_nulls.clone();
                move |source| ProfileRelabeling {
                    rows: rows.clone(),
                    nulls: source.into_iter().zip(canonical_nulls.clone()).collect(),
                }
            })
        })
        .collect()
}

/// Classify an object and return its relabelling to the representative.
///
/// The minimization ranges over every permutation of rows within each table
/// and the finite labelled-null alphabet. Constants remain byte-literal.
#[must_use]
pub fn hom_profile(object: &Object) -> Option<(HomProfile, ProfileRelabeling)> {
    let case = family().into_iter().nth(object.case)?;
    let side = case.side(object.side);
    profile_relabelings(&object.table, side)
        .into_iter()
        .filter_map(|relabeling| {
            let table = relabeling.apply(&object.table)?;
            let bytes = crate::canonical::table_bytes(&table)?;
            Some((bytes, relabeling))
        })
        .min_by(|(left_bytes, left_map), (right_bytes, right_map)| {
            left_bytes
                .cmp(right_bytes)
                .then_with(|| left_map.rows.cmp(&right_map.rows))
        })
        .map(|(canonical, relabeling)| {
            (
                HomProfile {
                    case: object.case,
                    side: object.side,
                    canonical,
                },
                relabeling,
            )
        })
}

/// One hom-profile class: its canonical representative and exact labelled
/// multiplicity in the finite scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomProfileClass {
    /// Canonical representative with canonical row ids.
    pub representative: Table,
    /// Number of labelled occurrences in this class.
    pub multiplicity: u64,
}

/// Independent orbit–stabilizer certificate for one profile class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrbitCertificate {
    /// Order of the full row-permutation × finite-null-permutation group.
    pub group_order: u64,
    /// Number of group elements fixing the representative structurally.
    pub stabilizer_order: u64,
    /// Exact orbit size `group_order / stabilizer_order`.
    pub multiplicity: u64,
}

fn certificate_permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(index);
        for suffix in certificate_permutations(&rest) {
            let mut permutation = Vec::with_capacity(items.len());
            permutation.push(item.clone());
            permutation.extend(suffix);
            out.push(permutation);
        }
    }
    out
}

fn certificate_action(
    table: &Table,
    row_map: &BTreeMap<u32, u32>,
    null_map: &BTreeMap<u64, u64>,
) -> Option<Table> {
    let rows = table
        .rows
        .iter()
        .map(|(row, (owner, cells))| {
            Some((
                *row_map.get(row)?,
                (
                    owner.clone(),
                    cells
                        .iter()
                        .map(|(column, value)| {
                            let value = match value {
                                Value::LabeledNull(id) => Value::LabeledNull(*null_map.get(id)?),
                                _ => value.clone(),
                            };
                            Some((column.clone(), value))
                        })
                        .collect::<Option<BTreeMap<_, _>>>()?,
                ),
            ))
        })
        .collect::<Option<BTreeMap<_, _>>>()?;
    let pairs = table
        .pairs
        .iter()
        .map(|(relation, source, target)| {
            Some((
                relation.clone(),
                *row_map.get(source)?,
                *row_map.get(target)?,
            ))
        })
        .collect::<Option<BTreeSet<_>>>()?;
    Some(Table { rows, pairs })
}

fn certificate_row_maps(
    groups: &[Vec<BTreeMap<u32, u32>>],
    index: usize,
    current: &mut BTreeMap<u32, u32>,
    out: &mut Vec<BTreeMap<u32, u32>>,
) {
    let Some(group) = groups.get(index) else {
        out.push(current.clone());
        return;
    };
    for part in group {
        current.extend(part);
        certificate_row_maps(groups, index + 1, current, out);
        for row in part.keys() {
            current.remove(row);
        }
    }
}

/// Compute a profile multiplicity without using the object generator,
/// canonicalizer, stored relabelling, or canonical table bytes. This is the
/// executable orbit–stabilizer proof for O14's weighted total.
#[must_use]
pub fn profile_multiplicity_certificate(
    case_index: usize,
    side_name: CaseSide,
    representative: &Table,
) -> Option<OrbitCertificate> {
    let case = family().into_iter().nth(case_index)?;
    let side = case.side(side_name);
    let mut row_groups = Vec::new();
    for (name, _) in &side.tables {
        let rows = representative
            .rows
            .iter()
            .filter_map(|(row, (owner, _))| (owner == name).then_some(*row))
            .collect::<Vec<_>>();
        row_groups.push(
            certificate_permutations(&rows)
                .into_iter()
                .map(|images| rows.iter().copied().zip(images).collect::<BTreeMap<_, _>>())
                .collect::<Vec<_>>(),
        );
    }
    let available_nulls = case
        .cells
        .iter()
        .filter_map(|value| match value {
            Value::LabeledNull(id) => Some(*id),
            _ => None,
        })
        .collect::<Vec<_>>();
    let null_maps: Vec<BTreeMap<u64, u64>> = certificate_permutations(&available_nulls)
        .into_iter()
        .map(|images| {
            available_nulls
                .iter()
                .copied()
                .zip(images)
                .collect::<BTreeMap<_, _>>()
        })
        .collect();
    let mut row_maps = Vec::new();
    certificate_row_maps(&row_groups, 0, &mut BTreeMap::new(), &mut row_maps);
    let group_order = u64::try_from(row_maps.len().checked_mul(null_maps.len())?).ok()?;
    let stabilizer_order = row_maps
        .iter()
        .flat_map(|rows| null_maps.iter().map(move |nulls| (rows, nulls)))
        .filter(|(rows, nulls)| {
            certificate_action(representative, rows, nulls).as_ref() == Some(representative)
        })
        .count() as u64;
    (stabilizer_order != 0 && group_order % stabilizer_order == 0).then_some(OrbitCertificate {
        group_order,
        stabilizer_order,
        multiplicity: group_order / stabilizer_order,
    })
}

/// Stable FNV-1a label for one profile key, used only to make exhaustive
/// certificate output compact and independently checkable.
#[must_use]
pub fn hom_profile_hash(profile: &HomProfile) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in profile
        .case
        .to_le_bytes()
        .into_iter()
        .chain([match profile.side {
            CaseSide::Source => 0,
            CaseSide::Target => 1,
        }])
        .chain(profile.canonical.iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Extend an input profile transport to Sigma's output nulls.
///
/// Input null classes follow their recorded representatives; fresh nulls
/// follow their `(row,column)` slots. In particular this never reuses an
/// irrelevant permutation of an unused input-alphabet id.
#[must_use]
pub fn sigma_output_null_transport(
    input: &ProfileTransport,
    left: &crate::rel::SigmaProvenance,
    right: &crate::rel::SigmaProvenance,
) -> Option<BTreeMap<u64, u64>> {
    let mut out = BTreeMap::new();
    for (left_id, right_id) in &input.nulls {
        let left_rep = left
            .merged
            .get(left_id)
            .cloned()
            .unwrap_or(Value::LabeledNull(*left_id));
        let right_rep = right
            .merged
            .get(right_id)
            .cloned()
            .unwrap_or(Value::LabeledNull(*right_id));
        match (left_rep, right_rep) {
            (Value::LabeledNull(source), Value::LabeledNull(target)) => {
                if out
                    .insert(source, target)
                    .is_some_and(|prior| prior != target)
                {
                    return None;
                }
            }
            (source, target)
                if crate::canonical::value_bytes(&source)
                    == crate::canonical::value_bytes(&target) => {}
            _ => return None,
        }
    }
    let right_fresh: BTreeMap<(u32, String), u64> = right
        .fresh
        .iter()
        .map(|(id, slot)| (slot.clone(), *id))
        .collect();
    for (source, slot) in &left.fresh {
        let mapped_slot = (*input.rows.get(&slot.0)?, slot.1.clone());
        let target = *right_fresh.get(&mapped_slot)?;
        if out
            .insert(*source, target)
            .is_some_and(|prior| prior != target)
        {
            return None;
        }
    }
    Some(out)
}

/// Transport Delta output rows from the copied target-row provenance.
#[must_use]
pub fn delta_output_row_transport(
    input: &ProfileTransport,
    left: &crate::rel::DeltaProvenance,
    right: &crate::rel::DeltaProvenance,
) -> Option<BTreeMap<u32, u32>> {
    let right_index: BTreeMap<(String, u32), u32> = right
        .iter()
        .map(|(output, origin)| (origin.clone(), *output))
        .collect();
    left.iter()
        .map(|(output, (table, origin))| {
            let mapped = *input.rows.get(origin)?;
            Some((*output, *right_index.get(&(table.clone(), mapped))?))
        })
        .collect()
}

/// Transport Pi output rows from the target table and ordered component-row
/// tuple recorded by Pi provenance.
#[must_use]
pub fn pi_output_row_transport(
    input: &ProfileTransport,
    left: &crate::rel::PiProvenance,
    right: &crate::rel::PiProvenance,
) -> Option<BTreeMap<u32, u32>> {
    let right_index: BTreeMap<(String, Vec<u32>), u32> = right
        .iter()
        .map(|(output, origin)| (origin.clone(), *output))
        .collect();
    left.iter()
        .map(|(output, (table, tuple))| {
            let tuple = tuple
                .iter()
                .map(|row| input.rows.get(row).copied())
                .collect::<Option<Vec<_>>>()?;
            Some((*output, *right_index.get(&(table.clone(), tuple))?))
        })
        .collect()
}

/// Partition every object in the requested scope into complete row-
/// relabelling profiles.
#[must_use]
pub fn hom_profile_classes(
    max_per_table: usize,
    max_total: usize,
) -> Option<BTreeMap<HomProfile, HomProfileClass>> {
    let mut classes = BTreeMap::new();
    for object in objects(max_per_table, max_total) {
        let (profile, relabeling) = hom_profile(&object)?;
        let representative = relabeling.apply(&object.table)?;
        let class = classes.entry(profile).or_insert_with(|| HomProfileClass {
            representative: representative.clone(),
            multiplicity: 0,
        });
        if class.representative != representative {
            return None;
        }
        class.multiplicity += 1;
    }
    Some(classes)
}

/// Every family object at the stated row bounds.
#[must_use]
pub fn objects(max_per_table: usize, max_total: usize) -> Vec<Object> {
    family()
        .into_iter()
        .enumerate()
        .flat_map(|(case_index, case)| {
            [CaseSide::Source, CaseSide::Target]
                .into_iter()
                .flat_map(move |side_name| {
                    instances(case.side(side_name), max_per_table, max_total, &case.cells)
                        .into_iter()
                        .map(move |table| Object {
                            case: case_index,
                            side: side_name,
                            table,
                        })
                })
        })
        .collect()
}

/// Independently computed labelled object-occurrence count for [`objects`].
#[must_use]
pub fn objects_count(max_per_table: usize, max_total: usize) -> u64 {
    family()
        .iter()
        .map(|case| {
            instance_count(
                &case.source,
                max_per_table,
                max_total,
                case.cells.len() as u64,
            ) + instance_count(
                &case.target,
                max_per_table,
                max_total,
                case.cells.len() as u64,
            )
        })
        .sum()
}

/// Independently computed number of ordered source-target object pairs.
///
/// The count is summed case by case. This is the raw labelled hom-law scope;
/// it is not the square of the aggregate object count because objects of
/// different cases do not share a functor.
#[must_use]
pub fn object_pairs_count(max_per_table: usize, max_total: usize) -> u64 {
    family()
        .iter()
        .map(|case| {
            instance_count(
                &case.source,
                max_per_table,
                max_total,
                case.cells.len() as u64,
            ) * instance_count(
                &case.target,
                max_per_table,
                max_total,
                case.cells.len() as u64,
            )
        })
        .sum()
}

/// The P2-O205 object scope: at most three rows per table and six in total.
#[must_use]
pub fn tables() -> Vec<Table> {
    objects(3, 6)
        .into_iter()
        .map(|object| object.table)
        .collect()
}

/// Every map from the rows of `a` to the rows of `b` that keeps each row's
/// table.
///
/// These are the candidate row maps of the homomorphisms `a -> b`: each row
/// of `a`, ascending, chooses among the rows of its table in `b`, ascending,
/// the first row varying slowest.
#[must_use]
pub fn table_row_maps(a: &Table, b: &Table) -> Vec<BTreeMap<u32, u32>> {
    let rows: Vec<u32> = a.rows.keys().copied().collect();
    let choices: Vec<Vec<u32>> = a
        .rows
        .values()
        .map(|(table, _)| {
            b.rows
                .iter()
                .filter(|(_, (other, _))| other == table)
                .map(|(id, _)| *id)
                .collect()
        })
        .collect();
    product(&choices)
        .into_iter()
        .map(|images| rows.iter().copied().zip(images).collect())
        .collect()
}

/// The count of [`table_row_maps`] by its closed formula.
///
/// It is the product, over the tables of `a`, of the row count of that table
/// in `b` raised to its row count in `a`.
#[must_use]
pub fn table_row_maps_count(a: &Table, b: &Table) -> u64 {
    let mut in_a: BTreeMap<&str, u32> = BTreeMap::new();
    for (table, _) in a.rows.values() {
        *in_a.entry(table.as_str()).or_default() += 1;
    }
    in_a.into_iter()
        .map(|(table, count)| {
            let in_b = b.rows.values().filter(|(other, _)| other == table).count() as u64;
            in_b.pow(count)
        })
        .product()
}

/// A graph cell presentation in the O7 encoding scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellForm {
    /// A leaf holding one presence value.
    Leaf(Presence),
    /// An unpositioned record, `(edge name, target vertex, leaf value)`.
    Record(Vec<(&'static str, &'static str, Value)>),
    /// A positioned list with the same child representation.
    List(Vec<(&'static str, &'static str, Value)>),
}

/// O7's graph-encoding schema: `a(x)`, `b`, relation `a -f-> b`, and two
/// possible inner edges below `x`.
#[must_use]
pub fn encoding_schema() -> Schema {
    Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.x", "value")
        .vertex("a.x.k", "value")
        .vertex("a.x.item", "value")
        .edge("a", "a.x", "prop", Some("x"))
        .edge("a", "b", "fk", Some("f"))
        .edge("a.x", "a.x.k", "inner", Some("k"))
        .edge("a.x", "a.x.item", "inner", Some("item"))
}

/// The seven O7 cell forms: a constant, explicit null, value null, record,
/// graph list, leaf list, and labelled null.
#[must_use]
pub fn encoding_forms() -> Vec<CellForm> {
    vec![
        CellForm::Leaf(Presence::Present(Value::Int(0))),
        CellForm::Leaf(Presence::Null),
        CellForm::Leaf(Presence::Present(Value::Null)),
        CellForm::Record(vec![("k", "a.x.k", Value::Str("s".to_owned()))]),
        CellForm::List(vec![
            ("item", "a.x.item", Value::Int(0)),
            ("item", "a.x.item", Value::Int(1)),
        ]),
        CellForm::Leaf(Presence::Present(Value::List(vec![
            Value::Int(0),
            Value::Int(1),
        ]))),
        CellForm::Leaf(Presence::Present(Value::LabeledNull(0))),
    ]
}

fn add_cell(
    graph: &mut Graph,
    next_node: &mut u32,
    next_edge: &mut u32,
    owner: u32,
    form: &CellForm,
) -> Option<()> {
    let root = *next_node;
    *next_node = next_node.checked_add(1)?;
    let (presence, children, positioned) = match form {
        CellForm::Leaf(presence) => (Some(presence.clone()), &[][..], false),
        CellForm::Record(children) => (None, &children[..], false),
        CellForm::List(children) => (None, &children[..], true),
    };
    let mut node = Node::new(root, "a.x");
    node.value = presence;
    graph.nodes.insert(root, node);
    graph.edges.insert(
        *next_edge,
        GEdge {
            src: owner,
            tgt: root,
            label: SEdge::new("a", "a.x", "prop", Some("x")),
            position: None,
        },
    );
    *next_edge = next_edge.checked_add(1)?;
    for (position, (name, vertex, value)) in children.iter().enumerate() {
        let child = *next_node;
        *next_node = next_node.checked_add(1)?;
        graph
            .nodes
            .insert(child, Node::new(child, vertex).with_value(value.clone()));
        graph.edges.insert(
            *next_edge,
            GEdge {
                src: root,
                tgt: child,
                label: SEdge::new("a.x", vertex, "inner", Some(name)),
                position: if positioned {
                    u32::try_from(position).ok()
                } else {
                    None
                },
            },
        );
        *next_edge = next_edge.checked_add(1)?;
    }
    Some(())
}

/// Every labelled graph of O7's encoding family with at most four table nodes.
///
/// Table-node ids run in table order, cell presentations range over
/// [`encoding_forms`], and every relation-pair subset is present.
#[must_use]
pub fn encoding_graphs() -> Vec<Graph> {
    let forms = encoding_forms();
    let relation = SEdge::new("a", "b", "fk", Some("f"));
    let mut out = Vec::new();
    for counts in row_count_vectors(2, 4, 4) {
        let (a_count, b_count) = (counts[0], counts[1]);
        for chosen in product(&vec![forms.clone(); a_count]) {
            let mut base = Graph::default();
            let mut next_node = 0u32;
            let mut a_nodes = Vec::new();
            let mut b_nodes = Vec::new();
            for _ in 0..a_count {
                let id = next_node;
                let Some(fresh) = next_node.checked_add(1) else {
                    continue;
                };
                next_node = fresh;
                base.nodes.insert(id, Node::new(id, "a"));
                a_nodes.push(id);
            }
            for _ in 0..b_count {
                let id = next_node;
                let Some(fresh) = next_node.checked_add(1) else {
                    continue;
                };
                next_node = fresh;
                base.nodes.insert(id, Node::new(id, "b"));
                b_nodes.push(id);
            }
            let mut next_edge = 0u32;
            let mut complete = true;
            for (owner, form) in a_nodes.iter().copied().zip(&chosen) {
                complete &=
                    add_cell(&mut base, &mut next_node, &mut next_edge, owner, form).is_some();
            }
            if !complete {
                continue;
            }
            let possible = a_nodes
                .iter()
                .flat_map(|source| b_nodes.iter().map(move |target| (*source, *target)))
                .collect::<Vec<_>>();
            for pairs in super::subsets(&possible, possible.len()) {
                let mut graph = base.clone();
                let mut edge = next_edge;
                for (source, target) in pairs {
                    graph.edges.insert(
                        edge,
                        GEdge {
                            src: source,
                            tgt: target,
                            label: relation.clone(),
                            position: None,
                        },
                    );
                    let Some(fresh) = edge.checked_add(1) else {
                        complete = false;
                        break;
                    };
                    edge = fresh;
                }
                if complete {
                    out.push(graph);
                }
            }
        }
    }
    out
}

/// Independent product count of [`encoding_graphs`]: the sum over `a+b<=4`
/// of `7^a 2^(a b)`.
#[must_use]
pub fn encoding_graphs_count() -> u64 {
    (0..=4u32)
        .flat_map(|a| (0..=4 - a).map(move |b| (a, b)))
        .map(|(a, b)| 7u64.pow(a) * 2u64.pow(a * b))
        .sum()
}

fn relabel_graph(
    graph: &Graph,
    node_id: &dyn Fn(u32) -> u32,
    edge_id: &dyn Fn(u32) -> u32,
) -> Graph {
    let nodes = graph
        .nodes
        .iter()
        .map(|(id, node)| {
            let id = node_id(*id);
            let mut node = node.clone();
            node.id = id;
            (id, node)
        })
        .collect();
    let edges = graph
        .edges
        .iter()
        .map(|(id, edge)| {
            let mut edge = edge.clone();
            edge.src = node_id(edge.src);
            edge.tgt = node_id(edge.tgt);
            (edge_id(*id), edge)
        })
        .collect();
    Graph { nodes, edges }
}

fn decorate_graph(graph: &Graph) -> Graph {
    let mut out = graph.clone();
    for node in out.nodes.values_mut() {
        node.discriminator = Some(node.anchor.clone());
        node.position = Some(node.id % 5);
        node.annotations
            .insert("seen".to_owned(), Value::Int(i64::from(node.id)));
        node.fields
            .insert("note".to_owned(), Value::Str(format!("n{}", node.id)));
    }
    for (id, edge) in &mut out.edges {
        if edge.label.kind != "inner" {
            edge.position = id.checked_add(2);
        }
    }
    out
}

/// O7's complete labelled presentation scope.
///
/// It contains every canonical base, the same graph with reversed/spread node
/// and edge identifiers, and decorated forms of both. Identifier renamings
/// remain distinct labelled objects.
#[must_use]
pub fn encoding_graph_presentations() -> Vec<Graph> {
    let bases = encoding_graphs();
    let mut out = Vec::with_capacity(bases.len() * 4);
    for graph in bases {
        let top_node = graph.nodes.keys().next_back().copied().unwrap_or(0);
        let top_edge = graph.edges.keys().next_back().copied().unwrap_or(0);
        let spread = relabel_graph(&graph, &|node| 3 * (top_node - node) + 1, &|edge| {
            5 * (top_edge - edge) + 2
        });
        out.push(decorate_graph(&graph));
        out.push(decorate_graph(&spread));
        out.push(graph);
        out.push(spread);
    }
    out
}

/// Independent count of [`encoding_graph_presentations`].
#[must_use]
pub fn encoding_graph_presentations_count() -> u64 {
    4 * encoding_graphs_count()
}

/// A compact tree scope used by the tree-conjugation differential: all
/// labelled rooted two-node trees over anchors `a,b`, cells including a
/// labelled null and exact float bits, and one optional arc name.
#[must_use]
pub fn trees() -> Vec<Tree> {
    super::trees(&super::TreeScope {
        nodes: 2,
        anchors: &["a", "b"],
        values: &[
            Value::Int(0),
            Value::LabeledNull(0),
            Value::Float(0),
            Value::Float(1u64 << 63),
        ],
        arc_names: &["f"],
        hyper_edges: &[],
        max_fans: 0,
    })
}

/// Independent closed count of [`trees`].
#[must_use]
pub fn trees_count() -> u64 {
    super::trees_count(2, 2, 4, 1, &[], 0)
}

/// Graphs that exercise the general carrier outside the encoding image.
///
/// They include parallel edges, loops, exact float bits and labelled nulls and
/// check typed encoding refusals and first-error order.
#[must_use]
pub fn carrier_graphs() -> Vec<Graph> {
    let labels = [
        SEdge::new("a", "a", "fk", Some("loop")),
        SEdge::new("a", "b", "fk", Some("f")),
    ];
    super::graphs(&super::GraphScope {
        anchors: &["a", "b"],
        values: &[
            Value::LabeledNull(0),
            Value::Float(0),
            Value::Float(1u64 << 63),
        ],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None, Some(0)],
        max_nodes: 2,
        max_edges: 2,
    })
}

/// Independent formula count of [`carrier_graphs`].
#[must_use]
pub fn carrier_graphs_count() -> u64 {
    super::graphs_count(2, 3, 1, 2, 2, 2, 2)
}

// ---------------------------------------------------------------------------
// O7's graph-Kan scopes: hand-written relational specs, their labelled
// graphs, the adjunction family with the relation-merge cases, the hom
// families and the encoding refusals.
// ---------------------------------------------------------------------------

/// A relational spec written out by hand.
///
/// Its tables in order, each with its
/// columns `(column, kind of the column's value vertex)`, its relations, and
/// its inner value edges `(source vertex, target vertex, name, target kind)`,
/// an empty name standing for an unnamed edge. The value vertex of column
/// `c` of table `t` is `t.c`; attribute arrows, relations and inner edges
/// have kind `prop`, tables kind `record`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphSpec {
    /// Tables with their `(column, value kind)` columns.
    pub tables: Vec<(&'static str, Vec<(&'static str, &'static str)>)>,
    /// Relation arrows.
    pub relations: Vec<SEdge>,
    /// Inner value edges `(source vertex, target vertex, name, target kind)`.
    pub inner: Vec<(&'static str, &'static str, &'static str, &'static str)>,
}

/// The value kinds of the graph specs, all classified as values.
pub const GRAPH_SPEC_VALUE_KINDS: [&str; 3] = ["int", "str", "value"];

/// The table kind of the graph specs.
pub const GRAPH_SPEC_TABLE_KIND: &str = "record";

/// A relation or attribute arrow of kind `prop` named `name`.
#[must_use]
pub fn prop_edge(src: &str, tgt: &str, name: &str) -> SEdge {
    SEdge::new(src, tgt, "prop", Some(name))
}

/// An inner value edge of kind `prop`; the empty name stands for an unnamed
/// edge.
#[must_use]
pub fn inner_prop_edge(src: &str, tgt: &str, name: &str) -> SEdge {
    SEdge::new(src, tgt, "prop", (!name.is_empty()).then_some(name))
}

impl GraphSpec {
    /// The attribute arrow of column `column` of `table`.
    #[must_use]
    pub fn column_edge(table: &str, column: &str) -> SEdge {
        prop_edge(table, &format!("{table}.{column}"), column)
    }

    /// The inner value edges as schema edges.
    #[must_use]
    pub fn inner_edges(&self) -> Vec<SEdge> {
        self.inner
            .iter()
            .map(|(src, tgt, name, _)| inner_prop_edge(src, tgt, name))
            .collect()
    }

    /// The graph schema of the spec.
    #[must_use]
    pub fn schema(&self) -> Schema {
        let mut schema = Schema::default();
        for (table, columns) in &self.tables {
            schema
                .vertices
                .insert((*table).to_owned(), GRAPH_SPEC_TABLE_KIND.to_owned());
            for (column, kind) in columns {
                schema
                    .vertices
                    .insert(format!("{table}.{column}"), (*kind).to_owned());
                schema.edges.insert(Self::column_edge(table, column));
            }
        }
        for (_, tgt, _, kind) in &self.inner {
            schema
                .vertices
                .entry((*tgt).to_owned())
                .or_insert_with(|| (*kind).to_owned());
        }
        schema.edges.extend(self.relations.iter().cloned());
        schema.edges.extend(self.inner_edges());
        schema
    }

    /// The tables of the spec.
    #[must_use]
    pub fn table_names(&self) -> BTreeSet<String> {
        self.tables
            .iter()
            .map(|(table, _)| (*table).to_owned())
            .collect()
    }

    /// The relations of the spec.
    #[must_use]
    pub fn relation_set(&self) -> BTreeSet<SEdge> {
        self.relations.iter().cloned().collect()
    }

    fn table_index(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|(table, _)| *table == name)
    }
}

/// Every vector of per-table node counts with sum at most `max`, the first
/// table varying slowest.
fn compositions(tables: usize, max: usize) -> Vec<Vec<usize>> {
    product(&vec![(0..=max).collect::<Vec<_>>(); tables])
        .into_iter()
        .filter(|counts| counts.iter().sum::<usize>() <= max)
        .collect()
}

/// The cell slots `(owner, table, column)` of a graph of `spec` with
/// `counts[i]` nodes at its `i`th table, each table's nodes, and the number
/// of table nodes. Table nodes take the ids `0..n` in table order.
type SpecLayout = (
    Vec<(u32, &'static str, &'static str)>,
    BTreeMap<&'static str, Vec<u32>>,
    u32,
);

fn spec_layout(spec: &GraphSpec, counts: &[usize]) -> SpecLayout {
    let mut slots = Vec::new();
    let mut owners: BTreeMap<&'static str, Vec<u32>> = BTreeMap::new();
    let mut id = 0u32;
    for ((table, columns), count) in spec.tables.iter().zip(counts) {
        for _ in 0..*count {
            owners.entry(table).or_default().push(id);
            for (column, _) in columns {
                slots.push((id, *table, *column));
            }
            id += 1;
        }
    }
    (slots, owners, id)
}

/// Every pair between the nodes of `relation`'s endpoint tables, the source
/// node varying slowest.
fn spec_possible_pairs(relation: &SEdge, owners: &BTreeMap<&str, Vec<u32>>) -> Vec<(u32, u32)> {
    let none = Vec::new();
    let sources = owners.get(relation.src.as_str()).unwrap_or(&none);
    let targets = owners.get(relation.tgt.as_str()).unwrap_or(&none);
    sources
        .iter()
        .flat_map(|source| targets.iter().map(move |target| (*source, *target)))
        .collect()
}

fn spec_add_cell(
    graph: &mut Graph,
    next: &mut (u32, u32),
    owner: u32,
    arrow: &SEdge,
    form: &CellForm,
) {
    let root = next.0;
    next.0 += 1;
    let (presence, children, positioned) = match form {
        CellForm::Leaf(presence) => (Some(presence.clone()), &[][..], false),
        CellForm::Record(children) => (None, &children[..], false),
        CellForm::List(children) => (None, &children[..], true),
    };
    let mut node = Node::new(root, &arrow.tgt);
    node.value = presence;
    graph.nodes.insert(root, node);
    graph.edges.insert(
        next.1,
        GEdge {
            src: owner,
            tgt: root,
            label: arrow.clone(),
            position: None,
        },
    );
    next.1 += 1;
    for (position, (name, vertex, value)) in children.iter().enumerate() {
        let child = next.0;
        next.0 += 1;
        graph
            .nodes
            .insert(child, Node::new(child, vertex).with_value(value.clone()));
        graph.edges.insert(
            next.1,
            GEdge {
                src: root,
                tgt: child,
                label: inner_prop_edge(&arrow.tgt, vertex, name),
                position: positioned.then(|| u32::try_from(position).unwrap_or(u32::MAX)),
            },
        );
        next.1 += 1;
    }
}

/// The graph of `spec` with the table nodes of `layout`, the cell of each
/// slot in the form `cells` gives it, and the pairs `pairs` gives each
/// relation. Value nodes and edges take ids in order after the table nodes.
fn spec_assemble(
    spec: &GraphSpec,
    layout: &SpecLayout,
    cells: &[CellForm],
    pairs: &[Vec<(u32, u32)>],
) -> Graph {
    let (slots, owners, nodes) = layout;
    let mut graph = Graph::default();
    for (table, _) in &spec.tables {
        for node in owners.get(table).into_iter().flatten() {
            graph.nodes.insert(*node, Node::new(*node, table));
        }
    }
    let mut next = (*nodes, 0u32);
    for ((owner, table, column), form) in slots.iter().zip(cells) {
        spec_add_cell(
            &mut graph,
            &mut next,
            *owner,
            &GraphSpec::column_edge(table, column),
            form,
        );
    }
    for (relation, chosen) in spec.relations.iter().zip(pairs) {
        for (src, tgt) in chosen {
            graph.edges.insert(
                next.1,
                GEdge {
                    src: *src,
                    tgt: *tgt,
                    label: relation.clone(),
                    position: None,
                },
            );
            next.1 += 1;
        }
    }
    graph
}

/// Every labelled graph of `spec` with at most `max` table nodes.
///
/// The table nodes carry the ids `0..n` in table order, every cell takes
/// every form `forms(table, column)` lists, and every relation takes every
/// subset of the pairs between its tables' nodes (by ascending bit mask over
/// [`spec_possible_pairs`] order). Value nodes and edges take ids in order
/// after them. Two graphs that differ only in their ids are distinct
/// labelled objects.
#[must_use]
pub fn spec_graphs(
    spec: &GraphSpec,
    forms: &dyn Fn(&str, &str) -> Vec<CellForm>,
    max: usize,
) -> Vec<Graph> {
    let mut out = Vec::new();
    for counts in compositions(spec.tables.len(), max) {
        let layout = spec_layout(spec, &counts);
        let (slots, owners, _) = &layout;
        let cell_choices: Vec<Vec<CellForm>> = slots.iter().map(|(_, t, c)| forms(t, c)).collect();
        let pair_choices: Vec<Vec<Vec<(u32, u32)>>> = spec
            .relations
            .iter()
            .map(|relation| {
                let possible = spec_possible_pairs(relation, owners);
                (0u64..(1u64 << possible.len()))
                    .map(|mask| {
                        possible
                            .iter()
                            .enumerate()
                            .filter(|(k, _)| mask & (1u64 << k) != 0)
                            .map(|(_, pair)| *pair)
                            .collect()
                    })
                    .collect()
            })
            .collect();
        for cells in product(&cell_choices) {
            for pairs in product(&pair_choices) {
                out.push(spec_assemble(spec, &layout, &cells, &pairs));
            }
        }
    }
    out
}

/// One labelled graph of `spec`, for sampling beyond the exhaustive bound.
///
/// `counts[i]` gives the node count of the `i`th table (missing entries
/// count zero); the `k`th cell slot, in [`spec_graphs`] slot order, takes
/// `forms[choices[k] % forms.len()]` (the first form when `choices` is
/// short); relation `r` takes the subset of its possible pairs whose bits
/// are set in `masks[r]` (none when `masks` is short). Ids are laid out as
/// [`spec_graphs`] lays them, so every graph `spec_graphs` lists at the same
/// counts is one of these.
#[must_use]
pub fn spec_graph(
    spec: &GraphSpec,
    counts: &[usize],
    forms: &[CellForm],
    choices: &[usize],
    masks: &[u64],
) -> Graph {
    let counts: Vec<usize> = (0..spec.tables.len())
        .map(|i| counts.get(i).copied().unwrap_or(0))
        .collect();
    let layout = spec_layout(spec, &counts);
    let (slots, owners, _) = &layout;
    let cells: Vec<CellForm> = (0..slots.len())
        .map(|k| {
            let choice = choices.get(k).copied().unwrap_or(0);
            forms[choice % forms.len()].clone()
        })
        .collect();
    let pairs: Vec<Vec<(u32, u32)>> = spec
        .relations
        .iter()
        .enumerate()
        .map(|(r, relation)| {
            let mask = masks.get(r).copied().unwrap_or(0);
            spec_possible_pairs(relation, owners)
                .into_iter()
                .enumerate()
                .filter(|(k, _)| *k < 64 && mask & (1u64 << k) != 0)
                .map(|(_, pair)| pair)
                .collect()
        })
        .collect();
    spec_assemble(spec, &layout, &cells, &pairs)
}

/// The labelled count of [`spec_graphs`] by its closed formula.
///
/// Computed without the enumerator: the sum, over every vector `k` of per-table node
/// counts with `|k| <= max`, of `prod_t (prod_c forms(t, c))^(k_t)` times
/// `2^(sum over relations e: s -> t of k_s k_t)`.
#[must_use]
pub fn spec_graphs_count(spec: &GraphSpec, forms: &dyn Fn(&str, &str) -> u64, max: usize) -> u64 {
    fn visit(
        spec: &GraphSpec,
        forms: &dyn Fn(&str, &str) -> u64,
        max: usize,
        table: usize,
        counts: &mut Vec<usize>,
    ) -> u64 {
        if table < spec.tables.len() {
            let used: usize = counts.iter().sum();
            return (0..=max - used)
                .map(|k| {
                    counts.push(k);
                    let total = visit(spec, forms, max, table + 1, counts);
                    counts.pop();
                    total
                })
                .sum();
        }
        let mut weight = 1u64;
        for ((name, columns), k) in spec.tables.iter().zip(counts.iter()) {
            let per_node: u64 = columns
                .iter()
                .map(|(column, _)| forms(name, column))
                .product();
            weight *= per_node.pow(u32::try_from(*k).unwrap_or(u32::MAX));
        }
        let pair_slots: usize = spec
            .relations
            .iter()
            .map(|relation| {
                let at = |vertex: &str| spec.table_index(vertex).map_or(0, |i| counts[i]);
                at(&relation.src) * at(&relation.tgt)
            })
            .sum();
        weight << pair_slots
    }
    visit(spec, forms, max, 0, &mut Vec::new())
}

/// One leaf form per value.
#[must_use]
pub fn leaf_forms(cells: &[Value]) -> Vec<CellForm> {
    cells
        .iter()
        .map(|value| CellForm::Leaf(Presence::Present(value.clone())))
        .collect()
}

/// `graph` with every node id `n` renamed to `node(n)` and every edge id `e`
/// to `edge(e)`.
#[must_use]
pub fn relabel_spec_graph(
    graph: &Graph,
    node: &dyn Fn(u32) -> u32,
    edge: &dyn Fn(u32) -> u32,
) -> Graph {
    relabel_graph(graph, node, edge)
}

/// `graph` with a decoration on every node (discriminator, position,
/// annotation and an extra field that names no column) and a position on
/// every edge that is not one of `spec`'s inner edges.
#[must_use]
pub fn decorate_spec_graph(graph: &Graph, spec: &GraphSpec) -> Graph {
    let inner: BTreeSet<SEdge> = spec.inner_edges().into_iter().collect();
    let mut out = graph.clone();
    for node in out.nodes.values_mut() {
        node.discriminator = Some(node.anchor.clone());
        node.position = Some(node.id % 5);
        node.annotations
            .insert("seen".to_owned(), Value::Int(i64::from(node.id)));
        node.fields
            .insert("note".to_owned(), Value::Str(format!("n{}", node.id)));
    }
    for (id, edge) in &mut out.edges {
        if !inner.contains(&edge.label) {
            edge.position = id.checked_add(2);
        }
    }
    out
}

/// O7's encoding spec: a table `a` with one structured column `x` (kind
/// `value`), a table `b` without columns and a relation `f: a -> b`, with
/// inner edges `x -k-> a.x.k` and `x -item-> a.x.item`.
#[must_use]
pub fn o7_encoding_spec() -> GraphSpec {
    GraphSpec {
        tables: vec![("a", vec![("x", "value")]), ("b", vec![])],
        relations: vec![prop_edge("a", "b", "f")],
        inner: vec![
            ("a.x", "a.x.k", "k", "str"),
            ("a.x", "a.x.item", "item", "int"),
        ],
    }
}

/// The seven forms of the encoding spec's cell: a constant leaf, both null
/// forms, a record subtree, a list subtree, a leaf holding the list that
/// subtree folds to, and a top-level labelled null.
#[must_use]
pub fn o7_encoding_forms() -> Vec<CellForm> {
    vec![
        CellForm::Leaf(Presence::Present(Value::Int(0))),
        CellForm::Leaf(Presence::Null),
        CellForm::Leaf(Presence::Present(Value::Null)),
        CellForm::Record(vec![("k", "a.x.k", Value::Str("s".to_owned()))]),
        CellForm::List(vec![
            ("item", "a.x.item", Value::Int(0)),
            ("item", "a.x.item", Value::Int(1)),
        ]),
        CellForm::Leaf(Presence::Present(Value::List(vec![
            Value::Int(0),
            Value::Int(1),
        ]))),
        CellForm::Leaf(Presence::Present(Value::LabeledNull(0))),
    ]
}

/// Every graph of the encoding spec with at most `max` table nodes.
///
/// Four presentations each: decorated, decorated with node and edge ids reversed
/// and spread out (value nodes then precede table nodes), as enumerated, and
/// spread.
#[must_use]
pub fn o7_encoding_presentations(max: usize) -> Vec<Graph> {
    let spec = o7_encoding_spec();
    let plain = spec_graphs(&spec, &|_, _| o7_encoding_forms(), max);
    let mut out = Vec::with_capacity(plain.len() * 4);
    for graph in plain {
        let top_node = graph.nodes.keys().next_back().copied().unwrap_or(0);
        let top_edge = graph.edges.keys().next_back().copied().unwrap_or(0);
        let spread = relabel_graph(&graph, &|node| 3 * (top_node - node) + 1, &|edge| {
            5 * (top_edge - edge) + 2
        });
        out.push(decorate_spec_graph(&graph, &spec));
        out.push(decorate_spec_graph(&spread, &spec));
        out.push(graph);
        out.push(spread);
    }
    out
}

/// Independent count of [`o7_encoding_presentations`]: four times the sum,
/// over `a + b <= max`, of `7^a 2^(a b)`.
#[must_use]
pub fn o7_encoding_presentations_count(max: u32) -> u64 {
    4 * (0..=max)
        .flat_map(|a| (0..=max - a).map(move |b| (a, b)))
        .map(|(a, b)| 7u64.pow(a) * 2u64.pow(a * b))
        .sum::<u64>()
}

/// The five leaf forms of the encoding spec's conforming tables: a
/// constant, `Null`, a record, a list and a labelled null.
#[must_use]
pub fn o7_table_cells() -> Vec<CellForm> {
    vec![
        CellForm::Leaf(Presence::Present(Value::Int(0))),
        CellForm::Leaf(Presence::Null),
        CellForm::Leaf(Presence::Present(Value::Unknown(BTreeMap::from([(
            "k".to_owned(),
            Value::Str("s".to_owned()),
        )])))),
        CellForm::Leaf(Presence::Present(Value::List(vec![
            Value::Int(0),
            Value::Int(1),
        ]))),
        CellForm::Leaf(Presence::Present(Value::LabeledNull(0))),
    ]
}

/// The first `count` cells of the small alphabet: `0`, `1`, then the
/// labelled null `0`.
#[must_use]
pub fn small_cells(count: usize) -> Vec<Value> {
    [Value::Int(0), Value::Int(1), Value::LabeledNull(0)]
        .into_iter()
        .take(count)
        .collect()
}

/// Two tables without columns and one relation between them.
#[must_use]
pub fn relation_spec() -> GraphSpec {
    GraphSpec {
        tables: vec![("a", vec![]), ("b", vec![])],
        relations: vec![prop_edge("a", "b", "f")],
        inner: vec![],
    }
}

/// Two tables with one integer column each.
#[must_use]
pub fn shared_column_spec() -> GraphSpec {
    GraphSpec {
        tables: vec![("a", vec![("x", "int")]), ("b", vec![("x", "int")])],
        relations: vec![],
        inner: vec![],
    }
}

/// A table `p` with one structured column `x` holding a record `{k}`.
#[must_use]
pub fn record_spec() -> GraphSpec {
    GraphSpec {
        tables: vec![("p", vec![("x", "value")])],
        relations: vec![],
        inner: vec![("p.x", "p.x.k", "k", "str")],
    }
}

/// The record value `{k: "s"}`.
#[must_use]
pub fn record_value() -> Value {
    Value::Unknown(BTreeMap::from([(
        "k".to_owned(),
        Value::Str("s".to_owned()),
    )]))
}

/// The hom families, each over at most four table nodes.
///
/// The record table
/// over two of its cell forms at a time (a constant and a labelled null, two
/// labelled nulls, a constant and a record subtree, a record subtree and a
/// leaf holding the record it folds to), and the relation spec.
#[must_use]
pub fn hom_families() -> Vec<(&'static str, GraphSpec, Vec<CellForm>)> {
    let constant = CellForm::Leaf(Presence::Present(Value::Int(0)));
    let null = |n| CellForm::Leaf(Presence::Present(Value::LabeledNull(n)));
    let subtree = CellForm::Record(vec![("k", "p.x.k", Value::Str("s".to_owned()))]);
    let leaf = CellForm::Leaf(Presence::Present(record_value()));
    vec![
        (
            "constant and null",
            record_spec(),
            vec![constant.clone(), null(0)],
        ),
        ("two nulls", record_spec(), vec![null(0), null(1)]),
        (
            "constant and record subtree",
            record_spec(),
            vec![constant, subtree.clone()],
        ),
        (
            "record subtree and record leaf",
            record_spec(),
            vec![subtree, leaf],
        ),
        ("relation", relation_spec(), vec![]),
    ]
}

/// The unnamed-edge spec: a table `a` with one structured column `x` whose
/// inner edges to `a.x.k` and `a.x.item` carry no name.
#[must_use]
pub fn unnamed_inner_spec() -> GraphSpec {
    GraphSpec {
        tables: vec![("a", vec![("x", "value")])],
        relations: vec![],
        inner: vec![("a.x", "a.x.k", "", "str"), ("a.x", "a.x.item", "", "int")],
    }
}

/// The four forms of the unnamed-edge spec: a record over both targets, a
/// record repeating one target, a list, and a leaf.
#[must_use]
pub fn unnamed_inner_forms() -> Vec<CellForm> {
    vec![
        CellForm::Record(vec![
            ("", "a.x.k", Value::Str("s".to_owned())),
            ("", "a.x.item", Value::Int(1)),
        ]),
        CellForm::Record(vec![
            ("", "a.x.k", Value::Str("s".to_owned())),
            ("", "a.x.k", Value::Str("t".to_owned())),
        ]),
        CellForm::List(vec![
            ("", "a.x.item", Value::Int(0)),
            ("", "a.x.item", Value::Int(1)),
        ]),
        CellForm::Leaf(Presence::Present(Value::Int(0))),
    ]
}

/// One migration of O7's adjunction family.
///
/// The whole source `S`, its kept
/// part `S'` and the target `T`, the maps of `F: S' -> T` on tables,
/// columns `(table, column, image)` and relations, the cell alphabet,
/// whether `F` is surjective on attributes (so Pi exists), and the largest
/// number of table nodes its graphs carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphKanCase {
    /// Stable descriptive name.
    pub name: &'static str,
    /// The whole source.
    pub source: GraphSpec,
    /// The kept part.
    pub kept: GraphSpec,
    /// The target.
    pub target: GraphSpec,
    /// Kept table to target table.
    pub tables: Vec<(&'static str, &'static str)>,
    /// Kept `(table, column)` to target column.
    pub columns: Vec<(&'static str, &'static str, &'static str)>,
    /// Kept relation to target relation.
    pub relations: Vec<(SEdge, SEdge)>,
    /// The cell alphabet.
    pub cells: Vec<Value>,
    /// Whether Pi exists (every target column is an image).
    pub pi: bool,
    /// The largest number of table nodes of an enumerated graph.
    pub bound: usize,
}

impl GraphKanCase {
    fn new(
        name: &'static str,
        (source, kept, target): (GraphSpec, GraphSpec, GraphSpec),
        tables: &[(&'static str, &'static str)],
        columns: &[(&'static str, &'static str, &'static str)],
        relations: &[(SEdge, SEdge)],
    ) -> Self {
        Self {
            name,
            source,
            kept,
            target,
            tables: tables.to_vec(),
            columns: columns.to_vec(),
            relations: relations.to_vec(),
            cells: small_cells(2),
            pi: true,
            bound: 4,
        }
    }

    fn with_cells(mut self, cells: Vec<Value>) -> Self {
        self.cells = cells;
        self
    }

    const fn without_pi(mut self) -> Self {
        self.pi = false;
        self
    }

    const fn with_bound(mut self, bound: usize) -> Self {
        self.bound = bound;
        self
    }

    /// The migration stating `F`: each kept table and column vertex to its
    /// image, each kept column arrow and relation to its image; nothing else
    /// survives.
    #[must_use]
    pub fn migration(&self) -> Migration {
        let mut migration = Migration::default();
        for (source, target) in &self.tables {
            migration
                .vertex_remap
                .insert((*source).to_owned(), (*target).to_owned());
        }
        for (table, column, image) in &self.columns {
            let Some((_, target)) = self.tables.iter().find(|(source, _)| source == table) else {
                continue;
            };
            migration
                .vertex_remap
                .insert(format!("{table}.{column}"), format!("{target}.{image}"));
            migration.edge_remap.insert(
                GraphSpec::column_edge(table, column),
                GraphSpec::column_edge(target, image),
            );
        }
        for (source, target) in &self.relations {
            migration.edge_remap.insert(source.clone(), target.clone());
        }
        migration
    }

    /// Every labelled graph of `spec` (one of this case's three specs) with
    /// at most [`GraphKanCase::bound`] table nodes, every cell a leaf of the
    /// case's alphabet.
    #[must_use]
    pub fn graphs(&self, spec: &GraphSpec) -> Vec<Graph> {
        let forms = leaf_forms(&self.cells);
        spec_graphs(spec, &|_, _| forms.clone(), self.bound)
    }

    /// Independent count of [`GraphKanCase::graphs`].
    #[must_use]
    pub fn graphs_count(&self, spec: &GraphSpec) -> u64 {
        let cells = self.cells.len() as u64;
        spec_graphs_count(spec, &|_, _| cells, self.bound)
    }
}

const fn graph_spec(
    tables: Vec<(&'static str, Vec<(&'static str, &'static str)>)>,
    relations: Vec<SEdge>,
) -> GraphSpec {
    GraphSpec {
        tables,
        relations,
        inner: Vec::new(),
    }
}

/// O7's adjunction family.
///
/// Four merges (a shared column over constants and
/// over a constant and a labelled null, tables carrying two relations to
/// one, two parallel relations merged) and three column cases (a target
/// column outside the image, two columns sent to one, a source table and
/// column outside the kept part), each over at most four table nodes.
#[must_use]
pub fn o7_adjunction_family() -> Vec<GraphKanCase> {
    let mut cases = o7_merge_cases();
    cases.extend(o7_column_cases());
    cases
}

/// The four merges of [`o7_adjunction_family`].
fn o7_merge_cases() -> Vec<GraphKanCase> {
    let merge = || {
        (
            shared_column_spec(),
            shared_column_spec(),
            graph_spec(vec![("t", vec![("x", "int")])], vec![]),
        )
    };
    let (f, h) = (prop_edge("a", "c", "f"), prop_edge("b", "c", "h"));
    let relations = graph_spec(
        vec![("a", vec![]), ("b", vec![]), ("c", vec![])],
        vec![f.clone(), h.clone()],
    );
    let g = prop_edge("A", "C", "g");
    let (f2, h2, g2) = (
        prop_edge("a", "b", "f"),
        prop_edge("a", "b", "h"),
        prop_edge("a", "b", "g"),
    );
    let parallel = graph_spec(
        vec![("a", vec![]), ("b", vec![])],
        vec![f2.clone(), h2.clone()],
    );
    vec![
        GraphKanCase::new(
            "a merge with a shared column",
            merge(),
            &[("a", "t"), ("b", "t")],
            &[("a", "x", "x"), ("b", "x", "x")],
            &[],
        ),
        GraphKanCase::new(
            "a merge with a shared column over a null",
            merge(),
            &[("a", "t"), ("b", "t")],
            &[("a", "x", "x"), ("b", "x", "x")],
            &[],
        )
        .with_cells(vec![Value::Int(0), Value::LabeledNull(0)]),
        GraphKanCase::new(
            "a merge of tables carrying two relations to one",
            (
                relations.clone(),
                relations,
                graph_spec(vec![("A", vec![]), ("C", vec![])], vec![g.clone()]),
            ),
            &[("a", "A"), ("b", "A"), ("c", "C")],
            &[],
            &[(f, g.clone()), (h, g)],
        ),
        GraphKanCase::new(
            "two parallel relations merged",
            (
                parallel.clone(),
                parallel,
                graph_spec(vec![("a", vec![]), ("b", vec![])], vec![g2.clone()]),
            ),
            &[("a", "a"), ("b", "b")],
            &[],
            &[(f2, g2.clone()), (h2, g2)],
        ),
    ]
}

/// The three column cases of [`o7_adjunction_family`].
fn o7_column_cases() -> Vec<GraphKanCase> {
    let one_column = || graph_spec(vec![("a", vec![("x", "int")])], vec![]);
    let two_columns = graph_spec(vec![("t", vec![("x", "int"), ("y", "int")])], vec![]);
    let dropped = graph_spec(
        vec![("a", vec![("x", "int"), ("w", "int")]), ("q", vec![])],
        vec![],
    );
    vec![
        GraphKanCase::new(
            "a target column outside the image",
            (
                one_column(),
                one_column(),
                graph_spec(vec![("a", vec![("x", "int"), ("y", "int")])], vec![]),
            ),
            &[("a", "a")],
            &[("a", "x", "x")],
            &[],
        )
        .without_pi(),
        GraphKanCase::new(
            "two columns sent to one",
            (
                two_columns.clone(),
                two_columns,
                graph_spec(vec![("t", vec![("z", "int")])], vec![]),
            ),
            &[("t", "t")],
            &[("t", "x", "z"), ("t", "y", "z")],
            &[],
        ),
        GraphKanCase::new(
            "a source table and column outside the kept part",
            (
                dropped,
                one_column(),
                graph_spec(vec![("A", vec![("x", "int")])], vec![]),
            ),
            &[("a", "A")],
            &[("a", "x", "x")],
            &[],
        ),
    ]
}

/// The relation-merge cases of the separated reflection.
///
/// Two parallel
/// relations and a third merged into one across a merge of tables (four
/// table nodes), two parallel loops merged into one loop (two table nodes),
/// and a merge of tables with a shared column whose relations merge (four
/// table nodes, over a constant and a labelled null).
#[must_use]
pub fn relation_merge_cases() -> Vec<GraphKanCase> {
    let (f, h, k) = (
        prop_edge("a", "b", "f"),
        prop_edge("a", "b", "h"),
        prop_edge("c", "b", "k"),
    );
    let three = graph_spec(
        vec![("a", vec![]), ("b", vec![]), ("c", vec![])],
        vec![f.clone(), h.clone(), k.clone()],
    );
    let g = prop_edge("A", "B", "g");
    let (lf, lh) = (prop_edge("a", "a", "f"), prop_edge("a", "a", "h"));
    let loops = graph_spec(vec![("a", vec![])], vec![lf.clone(), lh.clone()]);
    let lg = prop_edge("t", "t", "g");
    let (cf, ch) = (prop_edge("a", "c", "f"), prop_edge("b", "c", "h"));
    let shared = graph_spec(
        vec![
            ("a", vec![("x", "int")]),
            ("b", vec![("x", "int")]),
            ("c", vec![]),
        ],
        vec![cf.clone(), ch.clone()],
    );
    let cg = prop_edge("t", "C", "g");
    vec![
        GraphKanCase::new(
            "two parallel relations and a third merged across a merge of tables",
            (
                three.clone(),
                three,
                graph_spec(vec![("A", vec![]), ("B", vec![])], vec![g.clone()]),
            ),
            &[("a", "A"), ("b", "B"), ("c", "A")],
            &[],
            &[(f, g.clone()), (h, g.clone()), (k, g)],
        ),
        GraphKanCase::new(
            "two parallel loops merged",
            (
                loops.clone(),
                loops,
                graph_spec(vec![("t", vec![])], vec![lg.clone()]),
            ),
            &[("a", "t")],
            &[],
            &[(lf, lg.clone()), (lh, lg)],
        )
        .with_bound(2),
        GraphKanCase::new(
            "a merge with a shared column whose relations merge",
            (
                shared.clone(),
                shared,
                graph_spec(
                    vec![("t", vec![("x", "int")]), ("C", vec![])],
                    vec![cg.clone()],
                ),
            ),
            &[("a", "t"), ("b", "t"), ("c", "C")],
            &[("a", "x", "x"), ("b", "x", "x")],
            &[(cf, cg.clone()), (ch, cg)],
        )
        .with_cells(vec![Value::Int(0), Value::LabeledNull(0)]),
    ]
}

/// The separated-reflection family: [`o7_adjunction_family`] followed by
/// [`relation_merge_cases`].
#[must_use]
pub fn reflection_family() -> Vec<GraphKanCase> {
    let mut cases = o7_adjunction_family();
    cases.extend(relation_merge_cases());
    cases
}

/// The number of mutation kinds [`encoding_mutation`] knows.
pub const ENCODING_MUTATIONS: u8 = 18;

/// The reserved fan-parent edge kind.
const HYPER_PARENT_KIND: &str = "hyper_parent";

/// The ids a mutation reads: the node and edge `pick` selects (modulo their
/// counts), the node after that node, and a fresh node and edge id.
struct Picks {
    node: Option<u32>,
    other: Option<u32>,
    edge: Option<u32>,
    fresh_node: u32,
    fresh_edge: u32,
}

impl Picks {
    fn of(graph: &Graph, pick: usize) -> Self {
        let nodes: Vec<u32> = graph.nodes.keys().copied().collect();
        let edges: Vec<u32> = graph.edges.keys().copied().collect();
        Self {
            node: nodes.get(pick % nodes.len().max(1)).copied(),
            other: nodes.get((pick + 1) % nodes.len().max(1)).copied(),
            edge: edges.get(pick % edges.len().max(1)).copied(),
            fresh_node: graph.nodes.keys().next_back().map_or(0, |id| id + 1),
            fresh_edge: graph.edges.keys().next_back().map_or(0, |id| id + 1),
        }
    }
}

fn anchor_of(graph: &Graph, id: u32) -> String {
    graph
        .nodes
        .get(&id)
        .map(|node| node.anchor.clone())
        .unwrap_or_default()
}

/// The mutations that add nodes or edges.
fn mutate_by_adding(out: &mut Graph, graph: &Graph, kind: u8, at: &Picks) {
    let add = |out: &mut Graph, src: u32, tgt: u32, label: SEdge| {
        out.edges.insert(
            at.fresh_edge,
            GEdge {
                src,
                tgt,
                label,
                position: None,
            },
        );
    };
    let leaf = |anchor: &str, value: Value| Node::new(at.fresh_node, anchor).with_value(value);
    match (kind, at.node) {
        (1, Some(n)) => {
            out.nodes
                .insert(at.fresh_node, Node::new(at.fresh_node, "a"));
            let label = SEdge::new(&anchor_of(graph, n), "a", HYPER_PARENT_KIND, None);
            add(out, n, at.fresh_node, label);
        }
        (2, _) => {
            out.nodes
                .insert(at.fresh_node, Node::new(at.fresh_node, "outside"));
        }
        (3, Some(n)) => {
            let m = at.other.unwrap_or(n);
            add(
                out,
                n,
                m,
                prop_edge(&anchor_of(graph, n), &anchor_of(graph, m), "nowhere"),
            );
        }
        (4, Some(n)) => add(out, at.other.unwrap_or(n), n, prop_edge("a", "b", "f")),
        (5, _) => {
            if let Some(edge) = at.edge.and_then(|e| graph.edges.get(&e)) {
                out.edges.insert(at.fresh_edge, edge.clone());
            }
        }
        (8, Some(n)) if anchor_of(graph, n) == "a" => {
            out.nodes.insert(at.fresh_node, leaf("a.x", Value::Int(5)));
            add(out, n, at.fresh_node, prop_edge("a", "a.x", "x"));
        }
        (10, _) => {
            out.nodes.insert(at.fresh_node, leaf("a.x", Value::Int(5)));
        }
        (17, Some(n)) if anchor_of(graph, n) == "a.x" => {
            out.nodes
                .insert(at.fresh_node, leaf("a.x.k", Value::Str("t".to_owned())));
            add(out, n, at.fresh_node, prop_edge("a.x", "a.x.k", "k"));
        }
        _ => {}
    }
}

/// The mutations that change what is there.
fn mutate_in_place(out: &mut Graph, graph: &Graph, kind: u8, at: &Picks) {
    let set_value = |out: &mut Graph, n: u32, value: Option<Presence>| {
        if let Some(node) = out.nodes.get_mut(&n) {
            node.value = value;
        }
    };
    match (kind, at.node, at.edge) {
        (0, Some(n), _) => {
            if let Some(node) = out.nodes.get_mut(&n) {
                node.shape = NodeShape::HyperEdge;
            }
        }
        (6, _, Some(e)) => {
            out.edges.remove(&e);
        }
        (7, _, Some(e)) => {
            if let Some(removed) = out.edges.remove(&e) {
                let name = removed.label.name.clone().unwrap_or_default();
                if let Some(node) = out.nodes.get_mut(&removed.src) {
                    node.fields.insert(name, Value::Int(9));
                }
            }
        }
        (9, _, Some(e)) => {
            if let Some(moved) = graph.edges.get(&e) {
                let shared = graph
                    .edges
                    .values()
                    .find(|other| other.label == moved.label && other.tgt != moved.tgt)
                    .map(|other| other.tgt);
                if let (Some(target), Some(edge)) = (shared, out.edges.get_mut(&e)) {
                    edge.tgt = target;
                }
            }
        }
        (11, Some(n), _) => set_value(out, n, None),
        (12, Some(n), _) => set_value(out, n, Some(Presence::Absent)),
        (13, Some(n), _) => set_value(out, n, Some(Presence::Present(Value::Int(0)))),
        (14, _, Some(e)) => {
            if let Some(edge) = out.edges.get_mut(&e) {
                edge.position = if edge.position.is_some() {
                    None
                } else {
                    Some(7)
                };
            }
        }
        (15, Some(n), _) => set_value(
            out,
            n,
            Some(Presence::Present(Value::List(vec![Value::LabeledNull(3)]))),
        ),
        (16, Some(n), _) => set_value(out, n, Some(Presence::Present(Value::LabeledNull(3)))),
        _ => {}
    }
}

/// `graph` changed by mutation `kind` at the node or edge `pick` selects.
///
/// Kinds: 0 a fan shape, 1 a fan edge, 2 a node outside the side, 3 an edge
/// outside the side, 4 an edge with swapped endpoints, 5 a duplicated edge,
/// 6 a removed edge, 7 a removed edge with an extra field of its name, 8 a
/// second cell, 9 an edge moved onto another edge's value node of the same
/// label, 10 an orphan value node, 11 a cleared value, 12 an absent value,
/// 13 a value on an inner node, 14 a toggled position, 15 a nested labelled
/// null, 16 a top-level labelled null, 17 a second record field of one
/// name. Most kinds move the graph out of the encoding domain; some keep it
/// inside.
#[must_use]
pub fn encoding_mutation(graph: &Graph, kind: u8, pick: usize) -> Graph {
    let at = Picks::of(graph, pick);
    let mut out = graph.clone();
    mutate_by_adding(&mut out, graph, kind, &at);
    mutate_in_place(&mut out, graph, kind, &at);
    out
}

// ---------------------------------------------------------------------------
// Generated table cases: a deterministic generator of functors and
// instances, so that a property-test seed names one whole case.
// ---------------------------------------------------------------------------

/// A small deterministic generator (splitmix64).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix(pub u64);

impl SplitMix {
    /// The next 64 bits.
    pub const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (`0` when `n` is `0`).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        usize::try_from(self.next_u64() % u64::try_from(n).unwrap_or(u64::MAX)).unwrap_or(0)
    }

    /// True with probability `percent / 100`.
    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    /// One of `items`, `None` when there is none.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            items.get(self.below(items.len()))
        }
    }
}

/// One side of a generated functor: tables with their column names, and
/// relations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableSide {
    /// Each table with its column names.
    pub tables: BTreeMap<String, BTreeSet<String>>,
    /// The relations.
    pub relations: BTreeSet<SEdge>,
}

/// The table kind of the generated specs.
pub const TABLE_SPEC_TABLE_KIND: &str = "table";

/// The value kind of the generated specs.
pub const TABLE_SPEC_VALUE_KIND: &str = "integer";

impl TableSide {
    /// Its graph schema: tables of kind `table`, value vertices `t.c` of
    /// kind `integer`, attribute arrows of kind `prop` named by the column,
    /// and the relations as they are.
    #[must_use]
    pub fn schema(&self) -> Schema {
        let mut schema = Schema::default();
        for (table, columns) in &self.tables {
            schema
                .vertices
                .insert(table.clone(), TABLE_SPEC_TABLE_KIND.to_owned());
            for column in columns {
                let value = format!("{table}.{column}");
                schema
                    .vertices
                    .insert(value.clone(), TABLE_SPEC_VALUE_KIND.to_owned());
                schema
                    .edges
                    .insert(SEdge::new(table, &value, "prop", Some(column)));
            }
        }
        schema.edges.extend(self.relations.iter().cloned());
        schema
    }
}

/// A relation `src -> tgt` of kind `fk` named `name`.
#[must_use]
pub fn fk_edge(src: &str, tgt: &str, name: &str) -> SEdge {
    SEdge::new(src, tgt, "fk", Some(name))
}

/// A functor between two table sides by its maps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableSpec {
    /// The whole source.
    pub src: TableSide,
    /// The target.
    pub tgt: TableSide,
    /// Kept table to image.
    pub tables: BTreeMap<String, String>,
    /// Kept relation to image.
    pub relations: BTreeMap<SEdge, SEdge>,
    /// Kept `(table, column)` to image column.
    pub columns: BTreeMap<(String, String), String>,
}

impl TableSpec {
    /// The migration stating the spec's functor: each kept table and column
    /// vertex to its image, each kept attribute arrow and relation to its
    /// image.
    #[must_use]
    pub fn migration(&self) -> Migration {
        let mut migration = Migration::default();
        for (source, target) in &self.tables {
            migration
                .vertex_remap
                .insert(source.clone(), target.clone());
        }
        for ((table, column), image) in &self.columns {
            let Some(target) = self.tables.get(table) else {
                continue;
            };
            migration
                .vertex_remap
                .insert(format!("{table}.{column}"), format!("{target}.{image}"));
            migration.edge_remap.insert(
                SEdge::new(table, &format!("{table}.{column}"), "prop", Some(column)),
                SEdge::new(target, &format!("{target}.{image}"), "prop", Some(image)),
            );
        }
        for (source, target) in &self.relations {
            migration.edge_remap.insert(source.clone(), target.clone());
        }
        migration
    }

    /// The least target `(table, column)` no kept column of the table's
    /// fibre reaches, when there is one.
    #[must_use]
    pub fn missing_column(&self) -> Option<(String, String)> {
        for (table, columns) in &self.tgt.tables {
            for column in columns {
                let hit = self.columns.iter().any(|((source, _), image)| {
                    self.tables.get(source) == Some(table) && image == column
                });
                if !hit {
                    return Some((table.clone(), column.clone()));
                }
            }
        }
        None
    }

    /// The kept part of the source.
    #[must_use]
    pub fn kept(&self) -> TableSide {
        TableSide {
            tables: self
                .tables
                .keys()
                .map(|table| {
                    (
                        table.clone(),
                        self.columns
                            .keys()
                            .filter(|(owner, _)| owner == table)
                            .map(|(_, column)| column.clone())
                            .collect(),
                    )
                })
                .collect(),
            relations: self.relations.keys().cloned().collect(),
        }
    }
}

/// The cells of the generated instances: integers, labelled nulls, both
/// zeros and both quiet NaNs, floats by their bits.
#[must_use]
pub fn generated_cells() -> Vec<Value> {
    vec![
        Value::Int(0),
        Value::Int(1),
        Value::LabeledNull(0),
        Value::LabeledNull(1),
        Value::LabeledNull(2),
        Value::Float(0),
        Value::Float(1u64 << 63),
        Value::Float(0x7ff8_0000_0000_0000),
        Value::Float(0xfff8_0000_0000_0000),
    ]
}

/// A generated side: at most 4 tables with at most 3 columns each, and at
/// most 4 relations (self-references included).
fn generated_side(rng: &mut SplitMix, prefix: &str) -> TableSide {
    let tables: BTreeMap<String, BTreeSet<String>> = (0..=rng.below(4))
        .map(|k| {
            let columns = (0..rng.below(4)).map(|c| format!("c{c}")).collect();
            (format!("{prefix}{k}"), columns)
        })
        .collect();
    let names: Vec<String> = tables.keys().cloned().collect();
    let mut relations = BTreeSet::new();
    for k in 0..rng.below(5) {
        if let (Some(a), Some(b)) = (rng.pick(&names).cloned(), rng.pick(&names).cloned()) {
            relations.insert(fk_edge(&a, &b, &format!("{prefix}e{k}")));
        }
    }
    TableSide { tables, relations }
}

/// A generated spec.
///
/// A table map that may merge tables and drop some,
/// relations carried to target relations between the images of their
/// endpoints (some added to the target for it, up to 4) or dropped, and
/// columns sent to columns of the image table or dropped, so column maps
/// may be non-injective and non-surjective and the target may have objects
/// outside the image; half the specs are made surjective on attributes.
#[must_use]
pub fn generated_spec(rng: &mut SplitMix) -> TableSpec {
    let src = generated_side(rng, "s");
    let mut tgt = generated_side(rng, "d");
    let targets: Vec<String> = tgt.tables.keys().cloned().collect();
    let mut tables = BTreeMap::new();
    for source in src.tables.keys() {
        if rng.chance(85) {
            if let Some(target) = rng.pick(&targets) {
                tables.insert(source.clone(), target.clone());
            }
        }
    }
    let mut relations = BTreeMap::new();
    for (k, edge) in src.relations.iter().enumerate() {
        let (Some(a), Some(b)) = (tables.get(&edge.src), tables.get(&edge.tgt)) else {
            continue;
        };
        if tgt.relations.len() < 4 && rng.chance(50) {
            tgt.relations.insert(fk_edge(a, b, &format!("dk{k}")));
        }
        let candidates: Vec<SEdge> = tgt
            .relations
            .iter()
            .filter(|relation| relation.src == *a && relation.tgt == *b)
            .cloned()
            .collect();
        if rng.chance(80) {
            if let Some(image) = rng.pick(&candidates) {
                relations.insert(edge.clone(), image.clone());
            }
        }
    }
    let mut columns = BTreeMap::new();
    for (source, target) in &tables {
        let image_columns: Vec<String> = tgt.tables[target].iter().cloned().collect();
        for column in &src.tables[source] {
            if rng.chance(80) {
                if let Some(image) = rng.pick(&image_columns) {
                    columns.insert((source.clone(), column.clone()), image.clone());
                }
            }
        }
    }
    if rng.chance(50) {
        let hit: BTreeSet<(String, String)> = columns
            .iter()
            .map(|((source, _), image)| (tables[source].clone(), image.clone()))
            .collect();
        for (target, target_columns) in &mut tgt.tables {
            target_columns.retain(|column| hit.contains(&(target.clone(), column.clone())));
        }
    }
    TableSpec {
        src,
        tgt,
        tables,
        relations,
        columns,
    }
}

/// A generated instance of a side: at most 3 rows per table and 6 in all,
/// cells from [`generated_cells`], and a random set of pairs per relation
/// (partial and many-valued). Row ids start at `first`.
#[must_use]
pub fn generated_instance(rng: &mut SplitMix, side: &TableSide, first: u32) -> Table {
    let cells = generated_cells();
    let mut next = first;
    let mut total = 0usize;
    let mut ids: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut out = Table::default();
    for (table, columns) in &side.tables {
        let n = rng.below(4).min(6 - total);
        total += n;
        for _ in 0..n {
            let row: BTreeMap<String, Value> = columns
                .iter()
                .map(|column| {
                    (
                        column.clone(),
                        rng.pick(&cells).cloned().unwrap_or(Value::Int(0)),
                    )
                })
                .collect();
            out.rows.insert(next, (table.clone(), row));
            ids.entry(table.clone()).or_default().push(next);
            next += 1;
        }
    }
    let empty = Vec::new();
    for edge in &side.relations {
        let from = ids.get(&edge.src).unwrap_or(&empty);
        let to = ids.get(&edge.tgt).unwrap_or(&empty);
        for a in from {
            for b in to {
                if rng.chance(35) {
                    out.pairs.insert((edge.clone(), *a, *b));
                }
            }
        }
    }
    out
}

/// `small` with up to two more rows (within 3 per table and 6 in all) and
/// some more pairs: an instance it is within.
#[must_use]
pub fn generated_growth(rng: &mut SplitMix, side: &TableSide, small: &Table) -> Table {
    let first = small.rows.keys().next_back().map_or(0, |row| row + 1);
    let extra = generated_instance(rng, side, first);
    let mut big = small.clone();
    let mut added = 0;
    for (id, (table, row)) in &extra.rows {
        let in_table = big
            .rows
            .values()
            .filter(|(owner, _)| owner == table)
            .count();
        let room = in_table < 3 && big.rows.len() < 6;
        if room && added < 2 && rng.chance(60) {
            big.rows.insert(*id, (table.clone(), row.clone()));
            added += 1;
        }
    }
    for edge in &side.relations {
        let rows_at = |table: &str| -> Vec<u32> {
            big.rows
                .iter()
                .filter(|(_, (owner, _))| owner == table)
                .map(|(id, _)| *id)
                .collect()
        };
        let (from, to) = (rows_at(&edge.src), rows_at(&edge.tgt));
        for a in &from {
            for b in &to {
                let pair = (edge.clone(), *a, *b);
                if !big.pairs.contains(&pair) && rng.chance(20) {
                    big.pairs.insert(pair);
                }
            }
        }
    }
    big
}

/// A homomorphism out of `a`: `a` with one null replaced by a value and
/// then grown, reached by the identity on rows and that one null entry.
#[must_use]
pub fn generated_hom_out(
    rng: &mut SplitMix,
    side: &TableSide,
    a: &Table,
) -> (Table, crate::rel::Hom) {
    let nulls: Vec<u64> = a
        .rows
        .values()
        .flat_map(|(_, cells)| cells.values())
        .filter_map(|value| match value {
            Value::LabeledNull(id) => Some(*id),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut null_map = BTreeMap::new();
    let mut b = a.clone();
    if let Some(&null) = rng.pick(&nulls) {
        let cells = generated_cells();
        let value = rng.pick(&cells).cloned().unwrap_or(Value::Int(0));
        for (_, cells) in b.rows.values_mut() {
            for cell in cells.values_mut() {
                if *cell == Value::LabeledNull(null) {
                    *cell = value.clone();
                }
            }
        }
        null_map.insert(null, value);
    }
    let b = generated_growth(rng, side, &b);
    let hom = crate::rel::Hom {
        rows: a.rows.keys().map(|row| (*row, *row)).collect(),
        nulls: null_map,
    };
    (b, hom)
}

/// `table` with one row corrupted: a stray cell, a missing cell or a
/// labelled null nested in a list; unchanged when it has no row.
#[must_use]
pub fn generated_corruption(rng: &mut SplitMix, table: &Table) -> Table {
    let mut out = table.clone();
    let rows: Vec<u32> = table.rows.keys().copied().collect();
    let Some(&row) = rng.pick(&rows) else {
        return out;
    };
    if let Some((_, cells)) = out.rows.get_mut(&row) {
        let columns: Vec<String> = cells.keys().cloned().collect();
        match (rng.below(3), rng.pick(&columns)) {
            (0, _) | (_, None) => {
                cells.insert("zz-stray".to_owned(), Value::Int(0));
            }
            (1, Some(column)) => {
                cells.remove(column);
            }
            (_, Some(column)) => {
                cells.insert(column.clone(), Value::List(vec![Value::LabeledNull(9)]));
            }
        }
    }
    out
}

/// One generated case.
///
/// A spec, an instance `x` on the whole source, one `j`
/// on the target (row ids from a random first id below 8), the product
/// bound drawn from `PRODUCT_BOUNDS`, and the generator's state after them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedCase {
    /// The spec.
    pub spec: TableSpec,
    /// An instance on the whole source.
    pub x: Table,
    /// An instance on the target.
    pub j: Table,
    /// The Pi product bound.
    pub max: usize,
    /// The generator after the case, for derived instances.
    pub rng: SplitMix,
}

/// The generated case a seed names.
#[must_use]
pub fn generated_case(seed: u64) -> GeneratedCase {
    let mut rng = SplitMix(seed);
    let spec = generated_spec(&mut rng);
    let x = generated_instance(&mut rng, &spec.src, 0);
    let first = u32::try_from(rng.below(8)).unwrap_or(0);
    let j = generated_instance(&mut rng, &spec.tgt, first);
    let max = if rng.chance(50) {
        PRODUCT_BOUNDS[0]
    } else {
        PRODUCT_BOUNDS[1]
    };
    GeneratedCase {
        spec,
        x,
        j,
        max,
        rng,
    }
}

/// One Pi join case: a fibre of one to four source tables over the target
/// columns `p, q, r, s` all sent to one target table `d`, an instance of it
/// and a product bound from {1, 4, 81}.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiJoinCase {
    /// The functor.
    pub spec: TableSpec,
    /// The instance on the source.
    pub instance: Table,
    /// The product bound.
    pub max: usize,
    /// Whether the fibre holds the triangle `pq, qr, rp`.
    pub cyclic: bool,
}

/// The 3000 Pi join cases.
///
/// Generated fibres of one to four tables over the
/// target columns `p, q, r, s` with shared columns in connected and
/// disconnected, acyclic and cyclic (every fourth case holds the triangle
/// `pq, qr, rp`) patterns, zero to five rows per table with values from
/// three integers, and bounds 1, 4 and 81, by a fixed xorshift sequence.
#[must_use]
pub fn pi_join_cases() -> Vec<PiJoinCase> {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = |n: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % n
    };
    let all = ["p", "q", "r", "s"];
    let mut cases = Vec::with_capacity(3000);
    for case in 0..3000 {
        let mut shapes: Vec<Vec<&str>> = Vec::new();
        let cyclic = case % 4 == 0;
        if cyclic {
            shapes.extend([vec!["p", "q"], vec!["q", "r"], vec!["r", "p"]]);
        }
        for _ in 0..=next(if shapes.is_empty() { 4 } else { 2 }) {
            shapes.push(all.iter().copied().filter(|_| next(2) == 0).collect());
        }
        let names: Vec<String> = (0..shapes.len()).map(|k| format!("a{k}")).collect();
        let used: BTreeSet<&str> = shapes.iter().flatten().copied().collect();
        let mut spec = TableSpec::default();
        for (name, shape) in names.iter().zip(&shapes) {
            spec.src.tables.insert(
                name.clone(),
                shape.iter().map(|column| (*column).to_owned()).collect(),
            );
            spec.tables.insert(name.clone(), "d".to_owned());
            for column in shape {
                spec.columns
                    .insert((name.clone(), (*column).to_owned()), (*column).to_owned());
            }
        }
        spec.tgt.tables.insert(
            "d".to_owned(),
            used.iter().map(|column| (*column).to_owned()).collect(),
        );
        let mut instance = Table::default();
        let mut id = 0u32;
        for (name, shape) in names.iter().zip(&shapes) {
            for _ in 0..next(6) {
                id += 1;
                let row: BTreeMap<String, Value> = shape
                    .iter()
                    .map(|column| {
                        (
                            (*column).to_owned(),
                            Value::Int(i64::try_from(next(3)).unwrap_or(0)),
                        )
                    })
                    .collect();
                instance.rows.insert(id, (name.clone(), row));
            }
        }
        let max = [1, 4, 81][usize::try_from(next(3)).unwrap_or(0)];
        cases.push(PiJoinCase {
            spec,
            instance,
            max,
            cyclic,
        });
    }
    cases
}

// ---------------------------------------------------------------------------
// Classification and entity-functor scopes.
// ---------------------------------------------------------------------------

/// The vertex kinds of the classification scope: a table kind `t`, a value
/// kind `v`, an unclassified kind `u`, and a kind `b` that the ambiguous
/// partition declares both ways.
pub const CLASSIFICATION_KINDS: [&str; 4] = ["t", "v", "u", "b"];

/// The two kind partitions `(value kinds, non-value kinds)` of the
/// classification scope: a strict one, and one declaring `b` both ways.
#[must_use]
pub fn classification_partitions() -> Vec<(BTreeSet<String>, BTreeSet<String>)> {
    let set = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect();
    vec![
        (set(&["v"]), set(&["t"])),
        (set(&["v", "b"]), set(&["t", "b"])),
    ]
}

/// Every schema of the classification scope.
///
/// Vertices `v0, v1, v2`, each
/// with a kind from [`CLASSIFICATION_KINDS`], at most two edges of kind
/// `prop` named `a` over every ordered vertex pair, and every subset of the
/// vertices declared tables by a key `k`.
#[must_use]
pub fn classification_schemas() -> Vec<Schema> {
    let mut out = Vec::new();
    for schema in super::schemas(3, &CLASSIFICATION_KINDS, &["a"], 2) {
        let vertices: Vec<String> = schema.vertices.keys().cloned().collect();
        for declared in super::subsets(&vertices, vertices.len()) {
            let mut with_keys = schema.clone();
            for vertex in declared {
                with_keys.keys.insert(
                    vertex,
                    vec![crate::schema::Key {
                        id: "k".to_owned(),
                        parts: Vec::new(),
                        role: crate::schema::KeyRole::Primary,
                        scope: crate::schema::KeyScope::Document,
                        equality: Vec::new(),
                    }],
                );
            }
            out.push(with_keys);
        }
    }
    out
}

/// Independent count of [`classification_schemas`]:
/// `4^3 (1 + C(9,1) + C(9,2)) 2^3`.
#[must_use]
pub fn classification_schemas_count() -> u64 {
    4u64.pow(3) * (1 + super::choose(9, 1) + super::choose(9, 2)) * 2u64.pow(3)
}

/// The schemas of the entity-functor scope.
///
/// Each over kinds `record` and
/// `value`: one table; one table with a column; two tables and a relation;
/// one table with two columns; a table with a structured column (an inner
/// edge below it); a table with a loop relation.
#[must_use]
pub fn entity_schemas() -> Vec<Schema> {
    vec![
        Schema::default().vertex("a", "record"),
        Schema::default()
            .vertex("a", "record")
            .vertex("a.x", "value")
            .edge("a", "a.x", "prop", Some("x")),
        Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "fk", Some("f")),
        Schema::default()
            .vertex("a", "record")
            .vertex("a.x", "value")
            .vertex("a.y", "value")
            .edge("a", "a.x", "prop", Some("x"))
            .edge("a", "a.y", "prop", Some("y")),
        Schema::default()
            .vertex("a", "record")
            .vertex("a.x", "value")
            .vertex("a.x.k", "value")
            .edge("a", "a.x", "prop", Some("x"))
            .edge("a.x", "a.x.k", "inner", Some("k")),
        Schema::default()
            .vertex("a", "record")
            .edge("a", "a", "fk", Some("l")),
    ]
}

/// Every migration of the entity-functor scope from `src` to `tgt`.
///
/// Every
/// partial vertex map from the source vertices to the target vertices,
/// every partial edge map from the source edges to the target edges, and
/// every subset of the target vertices and of the target edges declared
/// surviving.
#[must_use]
pub fn entity_migrations(src: &Schema, tgt: &Schema) -> Vec<Migration> {
    let source_vertices: Vec<String> = src.vertices.keys().cloned().collect();
    let target_vertices: Vec<String> = tgt.vertices.keys().cloned().collect();
    let source_edges: Vec<SEdge> = src.edges.iter().cloned().collect();
    let target_edges: Vec<SEdge> = tgt.edges.iter().cloned().collect();
    let options = |n: usize| (0..=n).collect::<Vec<usize>>();
    let vertex_maps = product(&vec![options(target_vertices.len()); source_vertices.len()]);
    let edge_maps = product(&vec![options(target_edges.len()); source_edges.len()]);
    let surviving_vertices = super::subsets(&target_vertices, target_vertices.len());
    let surviving_edges = super::subsets(&target_edges, target_edges.len());
    let mut out = Vec::new();
    for vertex_map in &vertex_maps {
        for edge_map in &edge_maps {
            for verts in &surviving_vertices {
                for edges in &surviving_edges {
                    let mut migration = Migration::default();
                    for (source, choice) in source_vertices.iter().zip(vertex_map) {
                        if *choice > 0 {
                            migration
                                .vertex_remap
                                .insert(source.clone(), target_vertices[choice - 1].clone());
                        }
                    }
                    for (source, choice) in source_edges.iter().zip(edge_map) {
                        if *choice > 0 {
                            migration
                                .edge_remap
                                .insert(source.clone(), target_edges[choice - 1].clone());
                        }
                    }
                    migration.surviving_verts = verts.iter().cloned().collect();
                    migration.surviving_edges = edges.iter().cloned().collect();
                    out.push(migration);
                }
            }
        }
    }
    out
}

/// Independent count of [`entity_migrations`]:
/// `(m + 1)^n (f + 1)^e 2^m 2^f` for `n` source and `m` target vertices and
/// `e` source and `f` target edges.
#[must_use]
pub fn entity_migrations_count(src: &Schema, tgt: &Schema) -> u64 {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let (n, m) = (count(src.vertices.len()), count(tgt.vertices.len()));
    let (e, f) = (count(src.edges.len()), count(tgt.edges.len()));
    (u64::from(m) + 1).pow(n) * (u64::from(f) + 1).pow(e) * 2u64.pow(m) * 2u64.pow(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether no two items share a debug rendering.
    fn distinct<T: std::fmt::Debug>(items: &[T]) -> bool {
        let keys: BTreeSet<String> = items.iter().map(|item| format!("{item:?}")).collect();
        keys.len() == items.len()
    }

    fn row_table(entries: &[(u32, &str)]) -> Table {
        Table {
            rows: entries
                .iter()
                .map(|(id, table)| (*id, ((*table).to_owned(), BTreeMap::new())))
                .collect(),
            pairs: BTreeSet::new(),
        }
    }

    fn node(id: u32, anchor: &str, value: Option<Presence>) -> Node {
        let mut node = Node::new(id, anchor);
        node.value = value;
        node
    }

    fn present(value: Value) -> Option<Presence> {
        Some(Presence::Present(value))
    }

    fn gedge(src: u32, tgt: u32, label: SEdge, position: Option<u32>) -> GEdge {
        GEdge {
            src,
            tgt,
            label,
            position,
        }
    }

    fn graph(nodes: Vec<Node>, edges: Vec<(u32, GEdge)>) -> Graph {
        Graph {
            nodes: nodes.into_iter().map(|node| (node.id, node)).collect(),
            edges: edges.into_iter().collect(),
        }
    }

    fn names(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn cell(value: &Value) -> String {
        match value {
            Value::Int(n) => n.to_string(),
            Value::LabeledNull(n) => format!("_{n}"),
            Value::Float(bits) => format!("f{bits:x}"),
            Value::List(items) => {
                format!("[{}]", items.iter().map(cell).collect::<Vec<_>>().join(","))
            }
            other => format!("{other:?}"),
        }
    }

    fn render_table(table: &Table) -> String {
        let rows = table
            .rows
            .iter()
            .map(|(id, (owner, cells))| {
                let cells = cells
                    .iter()
                    .map(|(column, value)| format!("{column}={}", cell(value)))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{id}:{owner}{{{cells}}}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        let pairs = table
            .pairs
            .iter()
            .map(|(edge, a, b)| format!("{}:{a}>{b}", edge.name.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join(" ");
        format!("{rows} / {pairs}")
    }

    fn render_side(side: &TableSide) -> String {
        let tables = side
            .tables
            .iter()
            .map(|(table, columns)| {
                format!(
                    "{table}[{}]",
                    columns.iter().cloned().collect::<Vec<_>>().join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let relations = side
            .relations
            .iter()
            .map(|edge| {
                let name = edge.name.as_deref().unwrap_or("");
                format!("{}>{}:{name}", edge.src, edge.tgt)
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("{tables} / {relations}")
    }

    fn render_case(case: &GeneratedCase) -> Vec<String> {
        let spec = &case.spec;
        let join = |items: Vec<String>| items.join(" ");
        let name = |edge: &SEdge| edge.name.clone().unwrap_or_default();
        vec![
            format!("src {}", render_side(&spec.src)),
            format!("tgt {}", render_side(&spec.tgt)),
            format!(
                "tables {}",
                join(
                    spec.tables
                        .iter()
                        .map(|(a, b)| format!("{a}>{b}"))
                        .collect()
                )
            ),
            format!(
                "rels {}",
                join(
                    spec.relations
                        .iter()
                        .map(|(a, b)| format!("{}>{}", name(a), name(b)))
                        .collect()
                )
            ),
            format!(
                "cols {}",
                join(
                    spec.columns
                        .iter()
                        .map(|((t, c), image)| format!("{t}.{c}>{image}"))
                        .collect()
                )
            ),
            format!("x {}", render_table(&case.x)),
            format!("j {}", render_table(&case.j)),
            format!("max {}", case.max),
            format!("state {:x}", case.rng.0),
        ]
    }

    #[test]
    fn table_row_maps_keep_each_row_table_and_match_their_count() {
        let a = row_table(&[(0, "a"), (1, "a"), (2, "b")]);
        let b = row_table(&[(5, "a"), (7, "a"), (9, "b"), (10, "c")]);
        // Rows 0 and 1 choose among 5 and 7, the first varying slowest, and
        // row 2 must choose 9: 2^2 * 1^1 = 4 maps.
        let expected: Vec<BTreeMap<u32, u32>> = [(5, 5), (5, 7), (7, 5), (7, 7)]
            .into_iter()
            .map(|(x, y)| BTreeMap::from([(0, x), (1, y), (2, 9)]))
            .collect();
        assert_eq!(table_row_maps(&a, &b), expected);
        assert_eq!(table_row_maps_count(&a, &b), 4);
        // Table `c` has a row in `b` and none in `a`: no map back.
        assert!(table_row_maps(&b, &a).is_empty());
        assert_eq!(table_row_maps_count(&b, &a), 0);
        let empty = Table::default();
        assert_eq!(table_row_maps(&empty, &b), vec![BTreeMap::new()]);
        assert_eq!(table_row_maps_count(&empty, &b), 1);
        assert_eq!(table_row_maps_count(&b, &empty), 0);
        // Three `a` rows and two `b` rows into themselves: 3^3 * 2^2 = 108.
        let mixed = row_table(&[(0, "a"), (1, "a"), (2, "a"), (3, "b"), (4, "b")]);
        let maps = table_row_maps(&mixed, &mixed);
        assert_eq!(table_row_maps_count(&mixed, &mixed), 108);
        assert_eq!(maps.len(), 108);
        assert!(distinct(&maps));
        for map in &maps {
            assert_eq!(map.keys().copied().collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
            for (row, image) in map {
                assert_eq!(mixed.rows[row].0, mixed.rows[image].0);
            }
        }
        assert_eq!(
            maps.last(),
            Some(&BTreeMap::from([(0, 2), (1, 2), (2, 2), (3, 4), (4, 4)]))
        );
    }

    #[test]
    fn graph_spec_builds_its_schema_edges_and_names() {
        assert_eq!(
            prop_edge("a", "b", "f"),
            SEdge::new("a", "b", "prop", Some("f"))
        );
        assert_eq!(
            inner_prop_edge("a", "b", ""),
            SEdge::new("a", "b", "prop", None)
        );
        assert_eq!(
            inner_prop_edge("a", "b", "k"),
            SEdge::new("a", "b", "prop", Some("k"))
        );
        assert_eq!(
            fk_edge("a", "b", "f"),
            SEdge::new("a", "b", "fk", Some("f"))
        );
        assert_eq!(
            GraphSpec::column_edge("t", "c"),
            SEdge::new("t", "t.c", "prop", Some("c"))
        );
        let spec = o7_encoding_spec();
        assert_eq!(
            spec.inner_edges(),
            vec![
                SEdge::new("a.x", "a.x.k", "prop", Some("k")),
                SEdge::new("a.x", "a.x.item", "prop", Some("item")),
            ]
        );
        let expected = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("a.x", "value")
            .vertex("a.x.k", "str")
            .vertex("a.x.item", "int")
            .edge("a", "a.x", "prop", Some("x"))
            .edge("a", "b", "prop", Some("f"))
            .edge("a.x", "a.x.k", "prop", Some("k"))
            .edge("a.x", "a.x.item", "prop", Some("item"));
        assert_eq!(spec.schema(), expected);
        assert_eq!(spec.table_names(), names(&["a", "b"]));
        assert_eq!(
            spec.relation_set(),
            BTreeSet::from([prop_edge("a", "b", "f")])
        );
        assert_eq!(spec.table_index("b"), Some(1));
        assert_eq!(spec.table_index("a.x"), None);
        // An inner edge into a column vertex keeps the column's kind.
        let overlap = GraphSpec {
            tables: vec![("p", vec![("x", "value"), ("y", "int")])],
            relations: vec![],
            inner: vec![("p.x", "p.y", "", "str")],
        };
        let expected = Schema::default()
            .vertex("p", "record")
            .vertex("p.x", "value")
            .vertex("p.y", "int")
            .edge("p", "p.x", "prop", Some("x"))
            .edge("p", "p.y", "prop", Some("y"))
            .edge("p.x", "p.y", "prop", None);
        assert_eq!(overlap.schema(), expected);
        assert_eq!(GraphSpec::default().schema(), Schema::default());
        assert!(GraphSpec::default().table_names().is_empty());
        assert!(GraphSpec::default().inner_edges().is_empty());
    }

    #[test]
    fn named_specs_and_alphabets_are_as_written() {
        let relation = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "prop", Some("f"));
        assert_eq!(relation_spec().schema(), relation);
        let shared = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("a.x", "int")
            .vertex("b.x", "int")
            .edge("a", "a.x", "prop", Some("x"))
            .edge("b", "b.x", "prop", Some("x"));
        assert_eq!(shared_column_spec().schema(), shared);
        let record = Schema::default()
            .vertex("p", "record")
            .vertex("p.x", "value")
            .vertex("p.x.k", "str")
            .edge("p", "p.x", "prop", Some("x"))
            .edge("p.x", "p.x.k", "prop", Some("k"));
        assert_eq!(record_spec().schema(), record);
        let unnamed = Schema::default()
            .vertex("a", "record")
            .vertex("a.x", "value")
            .vertex("a.x.k", "str")
            .vertex("a.x.item", "int")
            .edge("a", "a.x", "prop", Some("x"))
            .edge("a.x", "a.x.k", "prop", None)
            .edge("a.x", "a.x.item", "prop", None);
        assert_eq!(unnamed_inner_spec().schema(), unnamed);
        let s = || Value::Str("s".to_owned());
        assert_eq!(
            record_value(),
            Value::Unknown(BTreeMap::from([("k".to_owned(), s())]))
        );
        let small = [Value::Int(0), Value::Int(1), Value::LabeledNull(0)];
        for count in 0..=4 {
            assert_eq!(small_cells(count), small[..count.min(3)].to_vec());
        }
        assert_eq!(
            leaf_forms(&small),
            small
                .iter()
                .map(|value| CellForm::Leaf(Presence::Present(value.clone())))
                .collect::<Vec<_>>()
        );
        assert!(leaf_forms(&[]).is_empty());
        assert_eq!(o7_encoding_forms(), encoding_forms());
        assert_eq!(o7_encoding_forms().len(), 7);
        assert!(distinct(&o7_encoding_forms()));
        let cells = o7_table_cells();
        assert_eq!(cells.len(), 5);
        assert!(distinct(&cells));
        assert_eq!(cells[0], CellForm::Leaf(Presence::Present(Value::Int(0))));
        assert_eq!(cells[1], CellForm::Leaf(Presence::Null));
        assert_eq!(cells[2], CellForm::Leaf(Presence::Present(record_value())));
        assert_eq!(
            cells[4],
            CellForm::Leaf(Presence::Present(Value::LabeledNull(0)))
        );
        let forms = unnamed_inner_forms();
        assert_eq!(forms.len(), 4);
        assert!(distinct(&forms));
        assert_eq!(
            forms[1],
            CellForm::Record(vec![
                ("", "a.x.k", s()),
                ("", "a.x.k", Value::Str("t".to_owned())),
            ])
        );
        assert_eq!(forms[3], CellForm::Leaf(Presence::Present(Value::Int(0))));
    }

    /// Enumerate `spec`, compare with the count and a hand-computed number,
    /// and check every graph is distinct, well formed and inside the schema.
    fn check_scope(
        spec: &GraphSpec,
        forms: &dyn Fn(&str, &str) -> Vec<CellForm>,
        max: usize,
        expected: u64,
    ) {
        let graphs = spec_graphs(spec, forms, max);
        let count = spec_graphs_count(spec, &|t, c| forms(t, c).len() as u64, max);
        assert_eq!((graphs.len() as u64, count), (expected, expected));
        assert!(distinct(&graphs));
        let schema = spec.schema();
        for graph in &graphs {
            assert!(graph.well_formed());
            assert!(
                graph
                    .nodes
                    .values()
                    .all(|n| schema.vertices.contains_key(&n.anchor))
            );
            assert!(
                graph
                    .edges
                    .values()
                    .all(|e| schema.edges.contains(&e.label))
            );
            let tables = graph
                .nodes
                .values()
                .filter(|n| spec.table_index(&n.anchor).is_some())
                .count();
            assert!(tables <= max);
        }
    }

    #[test]
    fn spec_graphs_agree_with_their_count_and_hand_counts() {
        let none = |_: &str, _: &str| Vec::new();
        let two = |_: &str, _: &str| leaf_forms(&small_cells(2));
        // Relation spec: the sum over a + b <= max of 2^(a b).
        for (max, expected) in [(0, 1), (1, 3), (2, 7), (3, 17)] {
            check_scope(&relation_spec(), &none, max, expected);
        }
        // Shared column, two cells: the sum over a + b <= max of 2^(a + b).
        for (max, expected) in [(0, 1), (1, 5), (2, 17), (3, 49)] {
            check_scope(&shared_column_spec(), &two, max, expected);
        }
        // Two columns of 2 and 3 forms: 1 + 6 + 36.
        let columns = GraphSpec {
            tables: vec![("t", vec![("x", "int"), ("y", "int")])],
            relations: vec![],
            inner: vec![],
        };
        let by_column =
            |_: &str, column: &str| leaf_forms(&small_cells(if column == "x" { 2 } else { 3 }));
        check_scope(&columns, &by_column, 2, 43);
        // A loop: the sum over a <= max of 2^(a^2), 1 + 2 + 16 + 512.
        let looped = GraphSpec {
            tables: vec![("a", vec![])],
            relations: vec![prop_edge("a", "a", "l")],
            inner: vec![],
        };
        check_scope(&looped, &none, 3, 531);
        // A relation into a vertex that is no table has no pairs.
        let dangling = GraphSpec {
            tables: vec![("a", vec![])],
            relations: vec![prop_edge("a", "zz", "l")],
            inner: vec![],
        };
        check_scope(&dangling, &none, 3, 4);
        // The encoding spec: the sum over a + b <= max of 7^a 2^(a b).
        let seven = |_: &str, _: &str| o7_encoding_forms();
        check_scope(&o7_encoding_spec(), &seven, 1, 9);
        check_scope(&o7_encoding_spec(), &seven, 2, 73);
        check_scope(&unnamed_inner_spec(), &|_, _| unnamed_inner_forms(), 2, 21);
        // Counts alone beyond the enumerated sizes: 5 + 105 + 1029 + 3087 + 2401.
        assert_eq!(spec_graphs_count(&o7_encoding_spec(), &|_, _| 7, 4), 6627);
        // 1 + 2 + 4 + 10 + 34.
        assert_eq!(spec_graphs_count(&relation_spec(), &|_, _| 0, 4), 51);
        assert_eq!(spec_graphs_count(&GraphSpec::default(), &|_, _| 3, 5), 1);
    }

    #[test]
    fn spec_graphs_lay_out_ids_in_the_documented_order() {
        let f = prop_edge("a", "b", "f");
        let plain = |id, anchor| node(id, anchor, None);
        let expected = vec![
            graph(vec![], vec![]),
            graph(vec![plain(0, "b")], vec![]),
            graph(vec![plain(0, "b"), plain(1, "b")], vec![]),
            graph(vec![plain(0, "a")], vec![]),
            graph(vec![plain(0, "a"), plain(1, "b")], vec![]),
            graph(
                vec![plain(0, "a"), plain(1, "b")],
                vec![(0, gedge(0, 1, f, None))],
            ),
            graph(vec![plain(0, "a"), plain(1, "a")], vec![]),
        ];
        assert_eq!(
            spec_graphs(&relation_spec(), &|_, _| Vec::new(), 2),
            expected
        );
        let cell = |owner: u32, table: &str, value: i64| {
            graph(
                vec![
                    plain(owner, table),
                    node(1, &format!("{table}.x"), present(Value::Int(value))),
                ],
                vec![(0, gedge(0, 1, GraphSpec::column_edge(table, "x"), None))],
            )
        };
        let expected = vec![
            graph(vec![], vec![]),
            cell(0, "b", 0),
            cell(0, "b", 1),
            cell(0, "a", 0),
            cell(0, "a", 1),
        ];
        let forms = leaf_forms(&small_cells(2));
        assert_eq!(
            spec_graphs(&shared_column_spec(), &|_, _| forms.clone(), 1),
            expected
        );
    }

    /// The encoding-spec graph with one `a` row holding the list form and
    /// one `b` row it relates to.
    fn list_graph() -> Graph {
        spec_graph(
            &o7_encoding_spec(),
            &[1, 1],
            &o7_encoding_forms(),
            &[4],
            &[1],
        )
    }

    #[test]
    fn spec_graph_places_cells_children_and_pairs() {
        let item = inner_prop_edge("a.x", "a.x.item", "item");
        let expected = graph(
            vec![
                node(0, "a", None),
                node(1, "b", None),
                node(2, "a.x", None),
                node(3, "a.x.item", present(Value::Int(0))),
                node(4, "a.x.item", present(Value::Int(1))),
            ],
            vec![
                (0, gedge(0, 2, GraphSpec::column_edge("a", "x"), None)),
                (1, gedge(2, 3, item.clone(), Some(0))),
                (2, gedge(2, 4, item, Some(1))),
                (3, gedge(0, 1, prop_edge("a", "b", "f"), None)),
            ],
        );
        assert_eq!(list_graph(), expected);
        let forms = o7_encoding_forms();
        let spec = o7_encoding_spec();
        let record = spec_graph(&spec, &[1, 1], &forms, &[3], &[0]);
        let k = inner_prop_edge("a.x", "a.x.k", "k");
        assert_eq!(record.edges[&1], gedge(2, 3, k, None));
        assert_eq!(
            record.nodes[&3],
            node(3, "a.x.k", present(Value::Str("s".to_owned())))
        );
        assert_eq!(record.edges.len(), 2);
        // Missing counts, choices and masks default to zero; a choice wraps
        // around the forms; extra counts are ignored.
        let leaf = graph(
            vec![node(0, "a", None), node(1, "a.x", present(Value::Int(0)))],
            vec![(0, gedge(0, 1, GraphSpec::column_edge("a", "x"), None))],
        );
        assert_eq!(spec_graph(&spec, &[1], &forms, &[], &[]), leaf);
        assert_eq!(spec_graph(&spec, &[1, 0, 5], &forms, &[7], &[0]), leaf);
        // Only the first 64 of 72 possible pairs can be chosen by a mask.
        let wide = spec_graph(&relation_spec(), &[9, 8], &[], &[], &[u64::MAX]);
        assert_eq!(wide.nodes.len(), 17);
        assert_eq!(wide.edges.len(), 64);
        assert_eq!(
            wide.edges[&63],
            gedge(7, 16, prop_edge("a", "b", "f"), None)
        );
        // Every graph `spec_graphs` lists is the `spec_graph` of its counts,
        // choices and mask, in that order.
        let mut rebuilt = Vec::new();
        for (a, b) in [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (2, 0)] {
            let choices: Vec<Vec<usize>> = match a {
                0 => vec![vec![]],
                1 => (0..7).map(|c| vec![c]).collect(),
                _ => (0..49).map(|c| vec![c / 7, c % 7]).collect(),
            };
            for choice in &choices {
                for mask in 0..(1u64 << (a * b)) {
                    rebuilt.push(spec_graph(&spec, &[a, b], &forms, choice, &[mask]));
                }
            }
        }
        assert_eq!(rebuilt, spec_graphs(&spec, &|_, _| forms.clone(), 2));
    }

    #[test]
    fn relabelling_and_decoration_touch_exactly_what_they_name() {
        let base = list_graph();
        let moved = relabel_spec_graph(&base, &|n| n + 5, &|e| 2 * e + 1);
        assert_eq!(
            moved.nodes.keys().copied().collect::<Vec<_>>(),
            [5, 6, 7, 8, 9]
        );
        assert_eq!(
            moved.edges.keys().copied().collect::<Vec<_>>(),
            [1, 3, 5, 7]
        );
        assert!(moved.well_formed());
        assert_eq!(moved.nodes[&7].anchor, "a.x");
        assert_eq!((moved.edges[&3].src, moved.edges[&3].tgt), (7, 8));
        assert_eq!(moved.edges[&3].position, Some(0));
        let spec = o7_encoding_spec();
        let decorated = decorate_spec_graph(&moved, &spec);
        for (id, n) in &decorated.nodes {
            assert_eq!(n.discriminator.as_ref(), Some(&n.anchor));
            assert_eq!(n.position, Some(id - 5));
            assert_eq!(n.annotations["seen"], Value::Int(i64::from(*id)));
            assert_eq!(n.fields["note"], Value::Str(format!("n{id}")));
            assert_eq!(
                (&n.value, &n.anchor),
                (&moved.nodes[id].value, &moved.nodes[id].anchor)
            );
        }
        // Inner edges keep their list positions; the others take id + 2.
        let positions: Vec<Option<u32>> = decorated.edges.values().map(|e| e.position).collect();
        assert_eq!(positions, [Some(3), Some(0), Some(1), Some(9)]);
        let plain = decorate_spec_graph(&base, &spec);
        let node_positions: Vec<Option<u32>> = plain.nodes.values().map(|n| n.position).collect();
        assert_eq!(
            node_positions,
            [Some(0), Some(1), Some(2), Some(3), Some(4)]
        );
    }

    #[test]
    fn encoding_presentations_list_four_forms_of_each_graph() {
        // Four times 1, 9, 73: the sum over a + b <= max of 7^a 2^(a b).
        for (max, bound, expected) in [(0, 0, 4), (1, 1, 36), (2, 2, 292)] {
            let presentations = o7_encoding_presentations(max);
            assert_eq!(o7_encoding_presentations_count(bound), expected);
            assert_eq!(presentations.len() as u64, expected);
            assert!(presentations.iter().all(Graph::well_formed));
        }
        assert_eq!(o7_encoding_presentations_count(4), 4 * 6627);
        // The third graph at bound 1 is one `a` row holding the constant.
        let all = o7_encoding_presentations(1);
        let column = GraphSpec::column_edge("a", "x");
        let plain = graph(
            vec![node(0, "a", None), node(1, "a.x", present(Value::Int(0)))],
            vec![(0, gedge(0, 1, column.clone(), None))],
        );
        // Spread: node n to 3 (1 - n) + 1, edge e to 5 (0 - e) + 2.
        let spread = graph(
            vec![node(4, "a", None), node(1, "a.x", present(Value::Int(0)))],
            vec![(2, gedge(4, 1, column, None))],
        );
        let decorate = |g: &Graph, edge_position: u32| {
            let mut out = g.clone();
            for (id, n) in &mut out.nodes {
                n.discriminator = Some(n.anchor.clone());
                n.position = Some(*id);
                n.annotations
                    .insert("seen".to_owned(), Value::Int(i64::from(*id)));
                n.fields
                    .insert("note".to_owned(), Value::Str(format!("n{id}")));
            }
            for e in out.edges.values_mut() {
                e.position = Some(edge_position);
            }
            out
        };
        assert_eq!(all[8], decorate(&plain, 2));
        assert_eq!(all[9], decorate(&spread, 4));
        assert_eq!(all[10], plain);
        assert_eq!(all[11], spread);
        assert!(all[..4].iter().all(|g| *g == Graph::default()));
    }

    #[test]
    fn hom_families_and_their_scopes() {
        let families = hom_families();
        let labels: Vec<&str> = families.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(
            labels,
            [
                "constant and null",
                "two nulls",
                "constant and record subtree",
                "record subtree and record leaf",
                "relation",
            ]
        );
        let null = |n| CellForm::Leaf(Presence::Present(Value::LabeledNull(n)));
        let constant = CellForm::Leaf(Presence::Present(Value::Int(0)));
        let subtree = CellForm::Record(vec![("k", "p.x.k", Value::Str("s".to_owned()))]);
        let leaf = CellForm::Leaf(Presence::Present(record_value()));
        let forms: Vec<Vec<CellForm>> = families.iter().map(|(_, _, f)| f.clone()).collect();
        assert_eq!(
            forms,
            [
                vec![constant.clone(), null(0)],
                vec![null(0), null(1)],
                vec![constant, subtree.clone()],
                vec![subtree, leaf],
                vec![],
            ]
        );
        for (name, spec, forms) in &families {
            let expected = if *name == "relation" {
                assert_eq!(*spec, relation_spec());
                51
            } else {
                assert_eq!(*spec, record_spec());
                // The sum over a <= 4 of 2^a.
                31
            };
            check_scope(spec, &|_, _| forms.clone(), 4, expected);
        }
    }

    #[test]
    fn reflection_family_lists_its_cases_in_order() {
        let family = reflection_family();
        let labels: Vec<&str> = family.iter().map(|case| case.name).collect();
        assert_eq!(
            labels,
            [
                "a merge with a shared column",
                "a merge with a shared column over a null",
                "a merge of tables carrying two relations to one",
                "two parallel relations merged",
                "a target column outside the image",
                "two columns sent to one",
                "a source table and column outside the kept part",
                "two parallel relations and a third merged across a merge of tables",
                "two parallel loops merged",
                "a merge with a shared column whose relations merge",
            ]
        );
        let mut joined = o7_adjunction_family();
        assert_eq!(joined.len(), 7);
        let merges = relation_merge_cases();
        assert_eq!(merges.len(), 3);
        joined.extend(merges);
        assert_eq!(joined, family);
        let null_cells = vec![Value::Int(0), Value::LabeledNull(0)];
        for (index, case) in family.iter().enumerate() {
            let cells = if matches!(index, 1 | 9) {
                null_cells.clone()
            } else {
                small_cells(2)
            };
            assert_eq!(case.cells, cells, "{}", case.name);
            assert_eq!(case.pi, index != 4, "{}", case.name);
            assert_eq!(case.bound, if index == 8 { 2 } else { 4 }, "{}", case.name);
            assert_eq!(case.kept == case.source, index != 6, "{}", case.name);
            assert!(case.source.inner.is_empty() && case.target.inner.is_empty());
        }
        let dropped = Schema::default()
            .vertex("a", "record")
            .vertex("a.x", "int")
            .vertex("a.w", "int")
            .vertex("q", "record")
            .edge("a", "a.x", "prop", Some("x"))
            .edge("a", "a.w", "prop", Some("w"));
        assert_eq!(family[6].source.schema(), dropped);
        let three = &family[7];
        assert_eq!(
            three.target.schema(),
            Schema::default()
                .vertex("A", "record")
                .vertex("B", "record")
                .edge("A", "B", "prop", Some("g"))
        );
    }

    fn renames(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn graph_kan_case_migrations_state_the_functor() {
        let family = reflection_family();
        let col = GraphSpec::column_edge;
        let merge = Migration {
            vertex_remap: renames(&[("a", "t"), ("a.x", "t.x"), ("b", "t"), ("b.x", "t.x")]),
            edge_remap: BTreeMap::from([
                (col("a", "x"), col("t", "x")),
                (col("b", "x"), col("t", "x")),
            ]),
            ..Migration::default()
        };
        assert_eq!(family[0].migration(), merge);
        let g = prop_edge("A", "C", "g");
        let relations = Migration {
            vertex_remap: renames(&[("a", "A"), ("b", "A"), ("c", "C")]),
            edge_remap: BTreeMap::from([
                (prop_edge("a", "c", "f"), g.clone()),
                (prop_edge("b", "c", "h"), g),
            ]),
            ..Migration::default()
        };
        assert_eq!(family[2].migration(), relations);
        let columns = Migration {
            vertex_remap: renames(&[("t", "t"), ("t.x", "t.z"), ("t.y", "t.z")]),
            edge_remap: BTreeMap::from([
                (col("t", "x"), col("t", "z")),
                (col("t", "y"), col("t", "z")),
            ]),
            ..Migration::default()
        };
        assert_eq!(family[5].migration(), columns);
        // A column of a table outside the table map is skipped.
        let mut stray = family[0].clone();
        stray.columns.push(("zz", "x", "x"));
        assert_eq!(stray.migration(), merge);
    }

    #[test]
    fn graph_kan_case_graphs_match_their_counts() {
        // Each entry is the sum, over the node-count vectors within the
        // bound, of |cells|^(cell slots) 2^(pair slots), evaluated by hand
        // for (source, kept, target): e.g. the shared-column merge has
        // sum_{n <= 4} (n + 1) 2^n = 129 sources and sum_{a <= 4} 2^a = 31
        // targets, and the loops have 1 + 2^2 + 2^8 = 261 sources.
        let expected: [[u64; 3]; 10] = [
            [129, 129, 31],
            [129, 129, 31],
            [139, 139, 51],
            [429, 429, 51],
            [31, 31, 341],
            [341, 341, 31],
            [453, 31, 31],
            [601, 601, 51],
            [261, 261, 19],
            [685, 685, 207],
        ];
        for (case, expected) in reflection_family().iter().zip(expected) {
            for (spec, want) in [&case.source, &case.kept, &case.target]
                .into_iter()
                .zip(expected)
            {
                let graphs = case.graphs(spec);
                assert_eq!(case.graphs_count(spec), want, "{}", case.name);
                assert_eq!(graphs.len() as u64, want, "{}", case.name);
                assert!(distinct(&graphs));
                for value in graphs.iter().flat_map(|g| g.nodes.values()) {
                    match &value.value {
                        Some(Presence::Present(cell)) => assert!(case.cells.contains(cell)),
                        other => assert_eq!(*other, None),
                    }
                }
            }
        }
    }

    /// The encoding-spec graph with one `a` row holding the record form and
    /// one `b` row it relates to: nodes `0 a`, `1 b`, `2 a.x`, `3 a.x.k`;
    /// edges `0: 0 -> 2`, `1: 2 -> 3`, `2: 0 -> 1`.
    fn record_graph() -> Graph {
        spec_graph(
            &o7_encoding_spec(),
            &[1, 1],
            &o7_encoding_forms(),
            &[3],
            &[1],
        )
    }

    #[test]
    fn encoding_mutations_that_add_nodes_or_edges() {
        let g = record_graph();
        let with = |nodes: Vec<Node>, edges: Vec<(u32, GEdge)>| {
            let mut out = g.clone();
            out.nodes.extend(nodes.into_iter().map(|n| (n.id, n)));
            out.edges.extend(edges);
            out
        };
        let m = |kind, pick| encoding_mutation(&g, kind, pick);
        let fan = |src: &str| SEdge::new(src, "a", "hyper_parent", None);
        // Pick 0 reads node 0, the next node 1 and edge 0; the fresh ids
        // are node 4 and edge 3.
        let fresh_a = || vec![node(4, "a", None)];
        assert_eq!(
            m(1, 0),
            with(fresh_a(), vec![(3, gedge(0, 4, fan("a"), None))])
        );
        assert_eq!(
            m(1, 2),
            with(fresh_a(), vec![(3, gedge(2, 4, fan("a.x"), None))])
        );
        assert_eq!(m(2, 0), with(vec![node(4, "outside", None)], vec![]));
        let nowhere = prop_edge("a", "b", "nowhere");
        assert_eq!(m(3, 0), with(vec![], vec![(3, gedge(0, 1, nowhere, None))]));
        // Pick 3 wraps the next node round to node 0.
        let back = prop_edge("a.x.k", "a", "nowhere");
        assert_eq!(m(3, 3), with(vec![], vec![(3, gedge(3, 0, back, None))]));
        let f = prop_edge("a", "b", "f");
        assert_eq!(m(4, 0), with(vec![], vec![(3, gedge(1, 0, f, None))]));
        assert_eq!(m(5, 1), with(vec![], vec![(3, g.edges[&1].clone())]));
        let five = || vec![node(4, "a.x", present(Value::Int(5)))];
        let x = prop_edge("a", "a.x", "x");
        assert_eq!(m(8, 0), with(five(), vec![(3, gedge(0, 4, x, None))]));
        assert_eq!(m(8, 1), g);
        assert_eq!(m(10, 1), with(five(), vec![]));
        let t = vec![node(4, "a.x.k", present(Value::Str("t".to_owned())))];
        let k = prop_edge("a.x", "a.x.k", "k");
        assert_eq!(m(17, 2), with(t, vec![(3, gedge(2, 4, k, None))]));
        assert_eq!(m(17, 0), g);
        // On the empty graph only the kinds that need no node or edge act.
        let empty = Graph::default();
        for kind in 0..ENCODING_MUTATIONS {
            let out = encoding_mutation(&empty, kind, 0);
            match kind {
                2 => assert_eq!(out, graph(vec![node(0, "outside", None)], vec![])),
                10 => assert_eq!(
                    out,
                    graph(vec![node(0, "a.x", present(Value::Int(5)))], vec![])
                ),
                _ => assert_eq!(out, empty, "kind {kind}"),
            }
        }
    }

    fn set_value(g: &Graph, id: u32, value: Option<Presence>) -> Graph {
        let mut out = g.clone();
        if let Some(n) = out.nodes.get_mut(&id) {
            n.value = value;
        }
        out
    }

    fn set_shape(g: &Graph, id: u32) -> Graph {
        let mut out = g.clone();
        if let Some(n) = out.nodes.get_mut(&id) {
            n.shape = NodeShape::HyperEdge;
        }
        out
    }

    fn edit_edge(g: &Graph, id: u32, edit: &dyn Fn(&mut GEdge)) -> Graph {
        let mut out = g.clone();
        if let Some(e) = out.edges.get_mut(&id) {
            edit(e);
        }
        out
    }

    fn drop_edge(g: &Graph, id: u32, field: Option<(u32, &str)>) -> Graph {
        let mut out = g.clone();
        out.edges.remove(&id);
        if let Some((owner, name)) = field {
            if let Some(n) = out.nodes.get_mut(&owner) {
                n.fields.insert(name.to_owned(), Value::Int(9));
            }
        }
        out
    }

    #[test]
    fn encoding_mutations_that_change_what_is_there() {
        let g = record_graph();
        let m = |kind, pick| encoding_mutation(&g, kind, pick);
        assert_eq!(m(0, 1), set_shape(&g, 1));
        // Pick 6 selects node 6 mod 4 = 2 and edge 6 mod 3 = 0.
        assert_eq!(m(0, 6), set_shape(&g, 2));
        assert_eq!(m(6, 2), drop_edge(&g, 2, None));
        assert_eq!(m(6, 5), drop_edge(&g, 2, None));
        assert_eq!(m(7, 0), drop_edge(&g, 0, Some((0, "x"))));
        assert_eq!(m(7, 1), drop_edge(&g, 1, Some((2, "k"))));
        assert_eq!(m(11, 3), set_value(&g, 3, None));
        // Node 0 holds no value, so clearing it is a no-op.
        assert_eq!(m(11, 0), g);
        assert_eq!(m(12, 2), set_value(&g, 2, Some(Presence::Absent)));
        assert_eq!(m(13, 2), set_value(&g, 2, present(Value::Int(0))));
        assert_eq!(m(14, 0), edit_edge(&g, 0, &|e| e.position = Some(7)));
        let nested = Value::List(vec![Value::LabeledNull(3)]);
        assert_eq!(m(15, 3), set_value(&g, 3, present(nested)));
        assert_eq!(m(16, 0), set_value(&g, 0, present(Value::LabeledNull(3))));
        // Every label of the record graph is distinct, so kind 9 has no
        // edge to move onto; the list graph's two item edges do.
        let list = list_graph();
        for pick in 0..6 {
            assert_eq!(m(9, pick), g);
        }
        assert_eq!(
            encoding_mutation(&list, 9, 1),
            edit_edge(&list, 1, &|e| e.tgt = 4)
        );
        assert_eq!(
            encoding_mutation(&list, 14, 1),
            edit_edge(&list, 1, &|e| e.position = None)
        );
        let two = spec_graph(
            &o7_encoding_spec(),
            &[2],
            &o7_encoding_forms(),
            &[0, 0],
            &[],
        );
        assert_eq!(
            encoding_mutation(&two, 9, 0),
            edit_edge(&two, 0, &|e| e.tgt = 3)
        );
        assert_eq!(
            encoding_mutation(&two, 9, 1),
            edit_edge(&two, 1, &|e| e.tgt = 2)
        );
        // An unnamed edge leaves a field with the empty name.
        let unnamed = spec_graph(
            &unnamed_inner_spec(),
            &[1],
            &unnamed_inner_forms(),
            &[0],
            &[],
        );
        assert_eq!(
            encoding_mutation(&unnamed, 7, 1),
            drop_edge(&unnamed, 1, Some((1, "")))
        );
        // Every kind changes the record graph (kind 9 the list graph) at
        // some pick; kinds past the last are no-ops.
        for kind in 0..ENCODING_MUTATIONS {
            let base = if kind == 9 { &list } else { &g };
            assert!((0..8).any(|pick| encoding_mutation(base, kind, pick) != *base));
        }
        for kind in [ENCODING_MUTATIONS, 40, u8::MAX] {
            assert!((0..8).all(|pick| encoding_mutation(&list, kind, pick) == list));
        }
    }

    #[test]
    fn splitmix_matches_the_reference_sequence() {
        // The published splitmix64 outputs from state 0.
        let mut rng = SplitMix(0);
        assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
        assert_eq!(rng.next_u64(), 0x06c4_5d18_8009_454f);
        assert_eq!(rng, SplitMix(0x9E37_79B9_7F4A_7C15u64.wrapping_mul(3)));
        // These values come from an independent splitmix64 transcription.
        let mut rng = SplitMix(42);
        assert_eq!(
            [1, 2, 3, 10, 100, 7].map(|n| rng.below(n)),
            [0, 1, 0, 4, 50, 4]
        );
        let mut rng = SplitMix(7);
        let draws = [50; 8].map(|p| rng.chance(p));
        assert_eq!(draws, [false, true, true, true, false, true, false, false]);
        // An empty range or slice consumes nothing.
        let mut rng = SplitMix(5);
        assert_eq!(rng.below(0), 0);
        assert_eq!(rng.pick::<u8>(&[]), None);
        assert_eq!(rng, SplitMix(5));
        // At seed 9 the hundredth draw of chance(0) is a zero.
        let mut rng = SplitMix(9);
        for _ in 0..200 {
            assert!(!rng.chance(0));
            assert!(rng.chance(100));
        }
        let items = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];
        let (mut x, mut y) = (SplitMix(11), SplitMix(11));
        for _ in 0..50 {
            assert_eq!(x.pick(&items), items.get(y.below(7)));
        }
    }

    fn table_side(tables: &[(&str, &[&str])], relations: &[SEdge]) -> TableSide {
        TableSide {
            tables: tables
                .iter()
                .map(|(table, columns)| ((*table).to_owned(), names(columns)))
                .collect(),
            relations: relations.iter().cloned().collect(),
        }
    }

    fn column_map(entries: &[(&str, &str, &str)]) -> BTreeMap<(String, String), String> {
        entries
            .iter()
            .map(|(t, c, i)| (((*t).to_owned(), (*c).to_owned()), (*i).to_owned()))
            .collect()
    }

    #[test]
    fn table_spec_states_its_functor() {
        let (e, g) = (fk_edge("s0", "s1", "e"), fk_edge("d0", "d1", "g"));
        let src = table_side(
            &[("s0", &["c0", "c1"]), ("s1", &["c0"]), ("s2", &[])],
            &[e.clone(), fk_edge("s2", "s2", "l")],
        );
        let tgt = table_side(&[("d0", &["c0", "c1"]), ("d1", &["c1"])], &[g.clone()]);
        let tgt_schema = Schema::default()
            .vertex("d0", "table")
            .vertex("d1", "table")
            .vertex("d0.c0", "integer")
            .vertex("d0.c1", "integer")
            .vertex("d1.c1", "integer")
            .edge("d0", "d0.c0", "prop", Some("c0"))
            .edge("d0", "d0.c1", "prop", Some("c1"))
            .edge("d1", "d1.c1", "prop", Some("c1"))
            .edge("d0", "d1", "fk", Some("g"));
        assert_eq!(tgt.schema(), tgt_schema);
        assert_eq!(TableSide::default().schema(), Schema::default());
        let mut spec = TableSpec {
            src,
            tgt,
            tables: renames(&[("s0", "d0"), ("s1", "d1")]),
            relations: BTreeMap::from([(e.clone(), g.clone())]),
            columns: column_map(&[
                ("s0", "c0", "c0"),
                ("s0", "c1", "c0"),
                ("s1", "c0", "c1"),
                ("s2", "c9", "c0"),
            ]),
        };
        let attr = |t: &str, c: &str| SEdge::new(t, &format!("{t}.{c}"), "prop", Some(c));
        let expected = Migration {
            vertex_remap: renames(&[
                ("s0", "d0"),
                ("s1", "d1"),
                ("s0.c0", "d0.c0"),
                ("s0.c1", "d0.c0"),
                ("s1.c0", "d1.c1"),
            ]),
            edge_remap: BTreeMap::from([
                (attr("s0", "c0"), attr("d0", "c0")),
                (attr("s0", "c1"), attr("d0", "c0")),
                (attr("s1", "c0"), attr("d1", "c1")),
                (e.clone(), g),
            ]),
            ..Migration::default()
        };
        assert_eq!(spec.migration(), expected);
        assert_eq!(
            spec.missing_column(),
            Some(("d0".to_owned(), "c1".to_owned()))
        );
        let kept = table_side(&[("s0", &["c0", "c1"]), ("s1", &["c0"])], &[e]);
        assert_eq!(spec.kept(), kept);
        // `c1` of `d1` has a namesake image in `d0`'s fibre, which does not
        // reach it.
        spec.columns = column_map(&[("s0", "c0", "c0"), ("s0", "c1", "c1")]);
        assert_eq!(
            spec.missing_column(),
            Some(("d1".to_owned(), "c1".to_owned()))
        );
        spec.columns
            .insert(("s1".to_owned(), "c0".to_owned()), "c1".to_owned());
        assert_eq!(spec.missing_column(), None);
        assert_eq!(TableSpec::default().missing_column(), None);
        assert_eq!(TableSpec::default().kept(), TableSide::default());
    }

    #[test]
    fn generated_cases_reproduce_an_independent_transcription() {
        // Each line was computed by an independent transcription of the
        // generator (splitmix64, the side, spec and instance draws in their
        // documented order) and is rendered here the same way.
        let pinned: [(u64, [&str; 9]); 4] = [
            (
                0,
                [
                    "src s0[] s1[c0,c1,c2] s2[] s3[c0,c1,c2] / ",
                    "tgt d0[] d1[c0,c1,c2] / ",
                    "tables s0>d0 s1>d1 s2>d1 s3>d0",
                    "rels ",
                    "cols s1.c2>c0",
                    "x 0:s2{} 1:s2{} 2:s3{c0=_2,c1=_0,c2=f8000000000000000} 3:s3{c0=f8000000000000000,c1=1,c2=_2} / ",
                    "j 3:d0{} 4:d1{c0=f8000000000000000,c1=f8000000000000000,c2=_1} 5:d1{c0=f0,c1=_1,c2=_1} 6:d1{c0=1,c1=f7ff8000000000000,c2=0} / ",
                    "max 4",
                    "state 489e4c815d41c005",
                ],
            ),
            (
                1,
                [
                    "src s0[c0,c1,c2] s1[c0,c1] / ",
                    "tgt d0[] d1[c0] / d0>d0:de0 d0>d0:de2 d1>d0:de1",
                    "tables s0>d1 s1>d1",
                    "rels ",
                    "cols s0.c0>c0 s0.c1>c0 s1.c0>c0 s1.c1>c0",
                    "x 0:s0{c0=f0,c1=f7ff8000000000000,c2=0} 1:s0{c0=1,c1=f0,c2=f8000000000000000} 2:s0{c0=_0,c1=_0,c2=_1} / ",
                    "j 4:d0{} 5:d0{} 6:d1{c0=f7ff8000000000000} 7:d1{c0=f0} 8:d1{c0=f7ff8000000000000} / de0:5>4 de0:5>5 de2:4>4 de1:7>5",
                    "max 4",
                    "state 15008779d57514ed",
                ],
            ),
            (
                2,
                [
                    "src s0[c0,c1] s1[c0,c1,c2] s2[] / s0>s2:se0 s1>s2:se3 s2>s0:se1 s2>s2:se2",
                    "tgt d0[c0,c1,c2] d1[c0] d2[c0,c1] / d2>d2:dk2 d2>d2:dk3",
                    "tables s0>d2 s1>d2 s2>d2",
                    "rels se2>dk3",
                    "cols s0.c1>c1 s1.c0>c1 s1.c1>c1 s1.c2>c0",
                    "x 0:s0{c0=f7ff8000000000000,c1=f7ff8000000000000} 1:s0{c0=_2,c1=_1} 2:s0{c0=f8000000000000000,c1=_0} 3:s1{c0=f0,c1=f7ff8000000000000,c2=0} 4:s2{} / se1:4>1",
                    "j 0:d2{c0=0,c1=f8000000000000000} 1:d2{c0=f7ff8000000000000,c1=f8000000000000000} 2:d2{c0=ffff8000000000000,c1=0} / dk2:0>0 dk2:1>0 dk2:2>2 dk3:0>1 dk3:2>1",
                    "max 4",
                    "state dbefbea9bec4978e",
                ],
            ),
            (
                3,
                [
                    "src s0[c0] s1[c0] / s0>s0:se1 s0>s1:se0",
                    "tgt d0[c0,c1] d1[] d2[c0,c1,c2] / d0>d1:de1 d1>d1:de0 d2>d2:dk0 d2>d2:dk1",
                    "tables s0>d2 s1>d2",
                    "rels se0>dk1",
                    "cols s0.c0>c2 s1.c0>c2",
                    "x 0:s0{c0=f8000000000000000} / ",
                    "j 2:d0{c0=_2,c1=ffff8000000000000} 3:d0{c0=_1,c1=f8000000000000000} 4:d0{c0=_1,c1=0} / ",
                    "max 81",
                    "state 6df7df54df624bc9",
                ],
            ),
        ];
        for (seed, lines) in pinned {
            assert_eq!(render_case(&generated_case(seed)), lines, "seed {seed}");
        }
        assert_eq!(
            generated_case(0).spec.missing_column(),
            Some(("d1".to_owned(), "c1".to_owned()))
        );
        assert_eq!(generated_case(1).spec.missing_column(), None);
        assert_eq!(
            generated_case(2).spec.missing_column(),
            Some(("d0".to_owned(), "c0".to_owned()))
        );
        assert_eq!(generated_cells().len(), 9);
        assert!(distinct(&generated_cells()));
    }

    #[test]
    fn derived_instances_reproduce_an_independent_transcription() {
        // Growth, a homomorphism out and two corruptions drawn, in that
        // order, from each case's generator, by the same transcription.
        let pinned: [(u64, [&str; 5], (u64, Value)); 2] = [
            (
                1,
                [
                    "0:s0{c0=f0,c1=f7ff8000000000000,c2=0} 1:s0{c0=1,c1=f0,c2=f8000000000000000} 2:s0{c0=_0,c1=_0,c2=_1} / ",
                    "0:s0{c0=f0,c1=f7ff8000000000000,c2=0} 1:s0{c0=1,c1=f0,c2=f8000000000000000} 2:s0{c0=_0,c1=_0,c2=1} 6:s1{c0=ffff8000000000000,c1=ffff8000000000000} / ",
                    "0:s0{c0=f0,c1=f7ff8000000000000,c2=0} 1:s0{c0=1,c1=f0,c2=f8000000000000000} 2:s0{c0=_0,c1=_0,c2=_1,zz-stray=0} / ",
                    "4:d0{} 5:d0{} 6:d1{c0=f7ff8000000000000} 7:d1{} 8:d1{c0=f7ff8000000000000} / de0:5>4 de0:5>5 de2:4>4 de1:7>5",
                    "54cda58fbbee87e1",
                ],
                (1, Value::Int(1)),
            ),
            (
                2,
                [
                    "0:s0{c0=f7ff8000000000000,c1=f7ff8000000000000} 1:s0{c0=_2,c1=_1} 2:s0{c0=f8000000000000000,c1=_0} 3:s1{c0=f0,c1=f7ff8000000000000,c2=0} 4:s2{} / se0:1>4 se1:4>1 se1:4>2",
                    "0:s0{c0=f7ff8000000000000,c1=f7ff8000000000000} 1:s0{c0=ffff8000000000000,c1=_1} 2:s0{c0=f8000000000000000,c1=_0} 3:s1{c0=f0,c1=f7ff8000000000000,c2=0} 4:s2{} 9:s1{c0=_2,c1=_1,c2=f8000000000000000} / se1:4>0 se1:4>1 se2:4>4",
                    "0:s0{c0=f7ff8000000000000,c1=f7ff8000000000000} 1:s0{c0=_2,c1=_1} 2:s0{c0=f8000000000000000,c1=_0} 3:s1{c0=f0,c1=f7ff8000000000000,c2=0} 4:s2{zz-stray=0} / se1:4>1",
                    "0:d2{c0=0,c1=f8000000000000000} 1:d2{c0=f7ff8000000000000,c1=f8000000000000000} 2:d2{c0=ffff8000000000000,c1=[_9]} / dk2:0>0 dk2:1>0 dk2:2>2 dk3:0>1 dk3:2>1",
                    "abb024451edc673f",
                ],
                (2, Value::Float(0xfff8_0000_0000_0000)),
            ),
        ];
        for (seed, lines, null) in pinned {
            let case = generated_case(seed);
            let mut rng = case.rng.clone();
            let grown = generated_growth(&mut rng, &case.spec.src, &case.x);
            let (b, hom) = generated_hom_out(&mut rng, &case.spec.src, &case.x);
            let x = generated_corruption(&mut rng, &case.x);
            let j = generated_corruption(&mut rng, &case.j);
            let state = format!("{:x}", rng.0);
            let got = [grown, b, x, j].map(|t| render_table(&t));
            assert_eq!(got, lines[..4], "seed {seed}");
            assert_eq!(state, lines[4], "seed {seed}");
            assert_eq!(hom.nulls, BTreeMap::from([null]));
            assert!(hom.rows.iter().all(|(a, b)| a == b));
            assert_eq!(hom.rows.len(), case.x.rows.len());
        }
        let mut rng = SplitMix(3);
        assert_eq!(
            generated_corruption(&mut rng, &Table::default()),
            Table::default()
        );
        assert_eq!(rng, SplitMix(3));
    }

    /// A generated side's shape: tables `prefix0..` with at most three of
    /// the columns `c0, c1, c2`, at most four relations between them.
    fn check_side(side: &TableSide, prefix: &str) {
        assert!((1..=4).contains(&side.tables.len()));
        for (k, (table, columns)) in side.tables.iter().enumerate() {
            assert_eq!(*table, format!("{prefix}{k}"));
            assert!(
                columns
                    .iter()
                    .all(|c| ["c0", "c1", "c2"].contains(&c.as_str()))
            );
        }
        assert!(side.relations.len() <= 4);
        for edge in &side.relations {
            assert!(side.tables.contains_key(&edge.src) && side.tables.contains_key(&edge.tgt));
            assert_eq!(edge.kind, "fk");
        }
    }

    /// An instance of `side`: at most three rows per table and six in all,
    /// the side's columns on every row, cells from the alphabet, and pairs
    /// of the side's relations between rows of their tables.
    fn check_instance(table: &Table, side: &TableSide) {
        let cells = generated_cells();
        assert!(table.rows.len() <= 6);
        for (owner, columns) in &side.tables {
            let rows: Vec<_> = table.rows.values().filter(|(t, _)| t == owner).collect();
            assert!(rows.len() <= 3);
            for (_, row) in rows {
                assert!(row.keys().eq(columns.iter()));
                assert!(row.values().all(|v| cells.contains(v)));
            }
        }
        assert!(
            table
                .rows
                .values()
                .all(|(t, _)| side.tables.contains_key(t))
        );
        for (edge, a, b) in &table.pairs {
            assert!(side.relations.contains(edge));
            assert_eq!(table.rows[a].0, edge.src);
            assert_eq!(table.rows[b].0, edge.tgt);
        }
    }

    fn check_spec(spec: &TableSpec) {
        check_side(&spec.src, "s");
        check_side(&spec.tgt, "d");
        for (source, target) in &spec.tables {
            assert!(spec.src.tables.contains_key(source));
            assert!(spec.tgt.tables.contains_key(target));
        }
        for (edge, image) in &spec.relations {
            assert!(spec.src.relations.contains(edge));
            assert!(spec.tgt.relations.contains(image));
            assert_eq!(image.src, spec.tables[&edge.src]);
            assert_eq!(image.tgt, spec.tables[&edge.tgt]);
        }
        for ((source, column), image) in &spec.columns {
            assert!(spec.src.tables[source].contains(column));
            assert!(spec.tgt.tables[&spec.tables[source]].contains(image));
        }
        let kept = spec.kept();
        assert!(kept.tables.keys().eq(spec.tables.keys()));
        assert!(kept.relations.iter().eq(spec.relations.keys()));
    }

    #[test]
    fn generated_cases_respect_their_bounds() {
        let (mut maxes, mut missing, mut corrupted) = (BTreeSet::new(), [0, 0], 0);
        for seed in 0..300 {
            let case = generated_case(seed);
            check_spec(&case.spec);
            check_instance(&case.x, &case.spec.src);
            check_instance(&case.j, &case.spec.tgt);
            for (offset, id) in (0..).zip(case.x.rows.keys()) {
                assert_eq!(offset, *id);
            }
            let first = case.j.rows.keys().next().copied().unwrap_or(0);
            assert!(first < 8);
            for (offset, id) in (first..).zip(case.j.rows.keys()) {
                assert_eq!(offset, *id);
            }
            assert!(PRODUCT_BOUNDS.contains(&case.max));
            maxes.insert(case.max);
            missing[usize::from(case.spec.missing_column().is_some())] += 1;
            let mut rng = case.rng.clone();
            let big = generated_growth(&mut rng, &case.spec.src, &case.x);
            check_instance(&big, &case.spec.src);
            assert!(
                case.x
                    .rows
                    .iter()
                    .all(|(id, row)| big.rows.get(id) == Some(row))
            );
            assert!(case.x.pairs.is_subset(&big.pairs));
            assert!(big.rows.len() <= case.x.rows.len() + 2);
            let (b, hom) = generated_hom_out(&mut rng, &case.spec.src, &case.x);
            check_instance(&b, &case.spec.src);
            assert_eq!(hom.check(&case.x, &b), Ok(()));
            assert!(hom.nulls.len() <= 1);
            let bad = generated_corruption(&mut rng, &case.x);
            let changed = case
                .x
                .rows
                .iter()
                .filter(|(id, row)| bad.rows[*id] != **row)
                .count();
            assert_eq!(changed, usize::from(!case.x.rows.is_empty()));
            assert_eq!(bad.pairs, case.x.pairs);
            corrupted += changed;
        }
        assert_eq!(maxes, BTreeSet::from(PRODUCT_BOUNDS));
        assert!(missing[0] > 0 && missing[1] > 0);
        assert!(corrupted > 200);
    }

    fn render_pi(case: &PiJoinCase) -> String {
        let tables = case
            .spec
            .src
            .tables
            .iter()
            .map(|(t, columns)| {
                format!(
                    "{t}[{}]",
                    columns.iter().cloned().collect::<Vec<_>>().join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let d = case.spec.tgt.tables["d"]
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        let rows = render_table(&case.instance);
        let rows = rows.trim_end_matches(" / ");
        format!(
            "{tables} | d[{d}] | {rows} | max {} | cyclic {}",
            case.max, case.cyclic
        )
    }

    #[test]
    fn pi_join_cases_reproduce_an_independent_transcription() {
        let cases = pi_join_cases();
        assert_eq!(cases.len(), 3000);
        // Rendered from an independent transcription of the xorshift
        // sequence and the draws in their documented order.
        let pinned = [
            (
                0,
                "a0[p,q] a1[q,r] a2[p,r] a3[p,r,s] a4[q] | d[p,q,r,s] | 1:a0{p=1,q=1} 2:a1{q=0,r=1} 3:a1{q=2,r=0} 4:a1{q=2,r=0} 5:a2{p=2,r=2} 6:a2{p=1,r=1} 7:a4{q=1} 8:a4{q=0} | max 4 | cyclic true",
            ),
            (
                1,
                "a0[r,s] a1[p,q,s] a2[s] | d[p,q,r,s] | 1:a0{r=0,s=0} 2:a0{r=1,s=1} 3:a0{r=1,s=0} 4:a0{r=2,s=2} 5:a0{r=0,s=0} 6:a2{s=2} | max 1 | cyclic false",
            ),
            (
                2,
                "a0[q] a1[p,q,s] a2[p,s] | d[p,q,s] | 1:a0{q=2} 2:a0{q=0} 3:a0{q=0} 4:a1{p=0,q=1,s=1} 5:a1{p=2,q=1,s=0} 6:a2{p=0,s=0} 7:a2{p=0,s=0} | max 1 | cyclic false",
            ),
            (
                2999,
                "a0[p,r,s] a1[p,r,s] a2[p,r,s] | d[p,r,s] | 1:a1{p=1,r=2,s=2} 2:a1{p=2,r=2,s=0} | max 1 | cyclic false",
            ),
        ];
        for (index, line) in pinned {
            assert_eq!(render_pi(&cases[index]), line, "case {index}");
        }
        let count = |f: &dyn Fn(&PiJoinCase) -> usize| cases.iter().map(f).sum::<usize>();
        assert_eq!(count(&|c| usize::from(c.cyclic)), 750);
        assert_eq!(count(&|c| usize::from(c.max == 1)), 962);
        assert_eq!(count(&|c| usize::from(c.max == 4)), 1044);
        assert_eq!(count(&|c| usize::from(c.max == 81)), 994);
        assert_eq!(count(&|c| c.instance.rows.len()), 22_415);
        assert_eq!(count(&|c| c.spec.src.tables.len()), 9014);
        let empty = |c: &PiJoinCase| c.spec.src.tables.values().filter(|s| s.is_empty()).count();
        assert_eq!(count(&empty), 421);
        let cell_sum = |c: &PiJoinCase| -> usize {
            let values = c.instance.rows.values().flat_map(|(_, row)| row.values());
            values
                .map(|v| {
                    if let Value::Int(n) = v {
                        usize::try_from(*n).unwrap_or(99)
                    } else {
                        99
                    }
                })
                .sum()
        };
        assert_eq!(count(&cell_sum), 44_472);
    }

    #[test]
    fn pi_join_cases_are_fibres_over_one_table() {
        let triangle = [names(&["p", "q"]), names(&["q", "r"]), names(&["p", "r"])];
        for (index, case) in pi_join_cases().iter().enumerate() {
            let spec = &case.spec;
            assert_eq!(case.cyclic, index % 4 == 0);
            let tables = spec.src.tables.len();
            assert!(if case.cyclic {
                (4..=5).contains(&tables)
            } else {
                (1..=4).contains(&tables)
            });
            if case.cyclic {
                assert!(spec.src.tables.values().take(3).eq(triangle.iter()));
            }
            let used: BTreeSet<String> = spec.src.tables.values().flatten().cloned().collect();
            assert_eq!(spec.tgt.tables, BTreeMap::from([("d".to_owned(), used)]));
            assert!(spec.tables.values().all(|t| t == "d"));
            assert!(spec.tables.keys().eq(spec.src.tables.keys()));
            for (table, columns) in &spec.src.tables {
                for column in columns {
                    let key = (table.clone(), column.clone());
                    assert_eq!(spec.columns.get(&key), Some(column));
                }
            }
            assert_eq!(
                spec.columns.len(),
                spec.src.tables.values().map(BTreeSet::len).sum::<usize>()
            );
            assert_eq!(spec.missing_column(), None);
            assert!([1, 4, 81].contains(&case.max));
            for (offset, (id, (table, row))) in (1..).zip(&case.instance.rows) {
                assert_eq!(offset, *id);
                assert!(row.keys().eq(spec.src.tables[table].iter()));
                assert!(row.values().all(|v| matches!(v, Value::Int(0..=2))));
            }
            for table in spec.src.tables.keys() {
                let rows = case
                    .instance
                    .rows
                    .values()
                    .filter(|(t, _)| t == table)
                    .count();
                assert!(rows <= 5);
            }
            assert!(case.instance.pairs.is_empty());
        }
    }

    fn key() -> crate::schema::Key {
        crate::schema::Key {
            id: "k".to_owned(),
            parts: Vec::new(),
            role: crate::schema::KeyRole::Primary,
            scope: crate::schema::KeyScope::Document,
            equality: Vec::new(),
        }
    }

    #[test]
    fn classification_scope_matches_its_count() {
        assert_eq!(
            classification_partitions(),
            vec![
                (names(&["v"]), names(&["t"])),
                (names(&["v", "b"]), names(&["t", "b"])),
            ]
        );
        // 4^3 kind assignments, 1 + 9 + 36 sets of at most two of the nine
        // candidate edges, and 2^3 key subsets: 64 * 46 * 8.
        assert_eq!(classification_schemas_count(), 23_552);
        let schemas = classification_schemas();
        assert_eq!(schemas.len(), 23_552);
        assert!(distinct(&schemas));
        let first = Schema::default()
            .vertex("v0", "t")
            .vertex("v1", "t")
            .vertex("v2", "t");
        assert_eq!(schemas[0], first);
        let mut keyed = first;
        keyed.keys.insert("v0".to_owned(), vec![key()]);
        assert_eq!(schemas[1], keyed);
        let mut last = Schema::default()
            .vertex("v0", "b")
            .vertex("v1", "b")
            .vertex("v2", "b")
            .edge("v2", "v1", "prop", Some("a"))
            .edge("v2", "v2", "prop", Some("a"));
        for vertex in ["v0", "v1", "v2"] {
            last.keys.insert(vertex.to_owned(), vec![key()]);
        }
        assert_eq!(schemas.last(), Some(&last));
        for schema in &schemas {
            assert_eq!(schema.vertices.len(), 3);
            assert!(schema.edges.len() <= 2);
            assert!(
                schema
                    .vertices
                    .values()
                    .all(|k| CLASSIFICATION_KINDS.contains(&k.as_str()))
            );
            assert!(schema.keys.keys().all(|v| schema.vertices.contains_key(v)));
        }
    }

    #[test]
    fn entity_scope_migrations_match_their_count() {
        let schemas = entity_schemas();
        let sizes: Vec<(usize, usize)> = schemas
            .iter()
            .map(|s| (s.vertices.len(), s.edges.len()))
            .collect();
        assert_eq!(sizes, [(1, 0), (2, 1), (2, 1), (3, 2), (3, 2), (1, 1)]);
        assert!(distinct(&schemas));
        assert_eq!(
            schemas[5],
            Schema::default()
                .vertex("a", "record")
                .edge("a", "a", "fk", Some("l"))
        );
        assert!(
            schemas[4]
                .edges
                .contains(&SEdge::new("a.x", "a.x.k", "inner", Some("k")))
        );
        for src in &schemas {
            for tgt in &schemas {
                let all = entity_migrations(src, tgt);
                assert_eq!(all.len() as u64, entity_migrations_count(src, tgt));
            }
        }
        // (m + 1)^n (f + 1)^e 2^m 2^f by hand.
        let count = |i: usize, j: usize| entity_migrations_count(&schemas[i], &schemas[j]);
        assert_eq!(count(0, 0), 2 * 2);
        assert_eq!(count(1, 2), 9 * 2 * 4 * 2);
        assert_eq!(count(3, 4), 64 * 9 * 8 * 4);
        assert_eq!(count(5, 5), 2 * 2 * 2 * 2);
        assert_eq!(count(5, 0), 2 * 2);
        let some = entity_migrations(&schemas[1], &schemas[3]);
        assert_eq!(some.len(), 16 * 3 * 8 * 4);
        assert!(distinct(&some));
        for migration in &some {
            assert!(
                migration
                    .vertex_remap
                    .iter()
                    .all(|(a, b)| schemas[1].vertices.contains_key(a)
                        && schemas[3].vertices.contains_key(b))
            );
            assert!(
                migration
                    .edge_remap
                    .iter()
                    .all(|(a, b)| schemas[1].edges.contains(a) && schemas[3].edges.contains(b))
            );
            assert!(
                migration
                    .surviving_verts
                    .iter()
                    .all(|v| schemas[3].vertices.contains_key(v))
            );
            assert!(migration.surviving_edges.is_subset(&schemas[3].edges));
        }
        let a = || "a".to_owned();
        let survives = Migration {
            surviving_verts: BTreeSet::from([a()]),
            ..Migration::default()
        };
        let mapped = Migration {
            vertex_remap: BTreeMap::from([(a(), a())]),
            ..Migration::default()
        };
        let both = Migration {
            vertex_remap: mapped.vertex_remap.clone(),
            ..survives.clone()
        };
        assert_eq!(
            entity_migrations(&schemas[0], &schemas[0]),
            vec![Migration::default(), survives, mapped, both]
        );
        // A schema without edges has the single empty edge map.
        assert!(
            entity_migrations(&schemas[5], &schemas[0])
                .iter()
                .all(|m| m.edge_remap.is_empty())
        );
    }
}
