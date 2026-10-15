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
