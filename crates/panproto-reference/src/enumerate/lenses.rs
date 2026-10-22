//! Complete finite scopes for the lens-law page.

use std::collections::{BTreeMap, BTreeSet};

use crate::migration::Migration;
use crate::model::{GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::path::{FusionCondition, FusionInputs, FusionSemantics};
use crate::schema::Schema;
use crate::value::Value;

/// Every two-object schema with at most one labelled edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Every vertex-only migration between the first and last bounded schemas.
#[must_use]
pub fn migrations() -> Vec<Migration> {
    let schemas = schemas();
    super::migrations(
        &schemas[0],
        &schemas[schemas.len() - 1],
        super::MigrationScope::VERTICES,
    )
}

/// Labelled graphs with at most two nodes and one edge.
#[must_use]
pub fn graphs() -> Vec<Graph> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 2,
        max_edges: 1,
    })
}

/// Labelled one-table instances with at most one row.
#[must_use]
pub fn tables() -> Vec<Table> {
    super::tables(
        &["t"],
        &BTreeMap::from([("t", vec!["c"])]),
        &[Value::Int(0)],
        &[],
        1,
    )
}

/// Labelled two-node rooted trees with no fans.
#[must_use]
pub fn trees() -> Vec<Tree> {
    super::trees(&super::TreeScope {
        nodes: 2,
        anchors: &["a"],
        values: &[],
        arc_names: &["e"],
        hyper_edges: &[],
        max_fans: 0,
    })
}

/// Number of source states in the finite partial-lens carrier.
pub const PARTIAL_LENS_SOURCES: usize = 2;
/// Number of view states in the finite partial-lens carrier.
pub const PARTIAL_LENS_VIEWS: usize = 2;
/// Number of complement states in the finite partial-lens carrier.
pub const PARTIAL_LENS_COMPLEMENTS: usize = 3;
/// Exact alphabet of one partial-put cell: undefined, source zero, source one.
pub const PARTIAL_PUT_ALPHABET: [Option<usize>; 3] = [None, Some(0), Some(1)];

/// One finite partial lens used by the C11 law census.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialLens {
    get: [(usize, usize); PARTIAL_LENS_SOURCES],
    put: [Option<usize>; PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS],
}

impl PartialLens {
    const fn cell(view: usize, complement: usize) -> usize {
        view * PARTIAL_LENS_COMPLEMENTS + complement
    }

    /// The ordered injective total get table.
    #[must_use]
    pub const fn get_table(&self) -> &[(usize, usize); PARTIAL_LENS_SOURCES] {
        &self.get
    }

    /// The partial put table in view-major, then complement-major order.
    #[must_use]
    pub const fn put_table(
        &self,
    ) -> &[Option<usize>; PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS] {
        &self.put
    }

    /// Read one partial put cell.
    #[must_use]
    pub const fn put(&self, view: usize, complement: usize) -> Option<usize> {
        self.put[Self::cell(view, complement)]
    }

    /// Whether this finite lens satisfies `GetPut`.
    #[must_use]
    pub fn get_put(&self) -> bool {
        self.get
            .iter()
            .enumerate()
            .all(|(source, &(view, complement))| self.put(view, complement) == Some(source))
    }

    /// Whether this finite lens satisfies complement-form `WeakPutGet`.
    #[must_use]
    pub fn weak_put_get(&self) -> bool {
        (0..PARTIAL_LENS_VIEWS).all(|view| {
            (0..PARTIAL_LENS_COMPLEMENTS).all(|complement| {
                self.put(view, complement).is_none_or(|source| {
                    let (observed, _) = self.get[source];
                    self.put(observed, complement) == Some(source)
                })
            })
        })
    }

    /// Whether every defined put re-reads the supplied view exactly.
    #[must_use]
    pub fn exact_put_get(&self) -> bool {
        (0..PARTIAL_LENS_VIEWS).all(|view| {
            (0..PARTIAL_LENS_COMPLEMENTS).all(|complement| {
                self.put(view, complement)
                    .is_none_or(|source| self.get[source].0 == view)
            })
        })
    }

    /// Whether this finite lens satisfies raw `PutTwice`.
    #[must_use]
    pub fn put_twice(&self) -> bool {
        (0..PARTIAL_LENS_VIEWS).all(|view| {
            (0..PARTIAL_LENS_COMPLEMENTS).all(|complement| {
                self.put(view, complement).is_none_or(|source| {
                    let next_complement = self.get[source].1;
                    self.put(view, next_complement) == Some(source)
                })
            })
        })
    }
}

fn decode_partial_put(mut code: usize) -> [Option<usize>; 6] {
    let mut put = [None; 6];
    for cell in &mut put {
        *cell = PARTIAL_PUT_ALPHABET[code % PARTIAL_PUT_ALPHABET.len()];
        code /= PARTIAL_PUT_ALPHABET.len();
    }
    put
}

/// Stream all ordered-injective-get/partial-put lenses of the C11 carrier.
pub fn for_each_partial_lens(mut visit: impl FnMut(PartialLens)) {
    let get_cells = PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS;
    for first in 0..get_cells {
        for second in (0..get_cells).filter(|second| *second != first) {
            let get = [
                (
                    first / PARTIAL_LENS_COMPLEMENTS,
                    first % PARTIAL_LENS_COMPLEMENTS,
                ),
                (
                    second / PARTIAL_LENS_COMPLEMENTS,
                    second % PARTIAL_LENS_COMPLEMENTS,
                ),
            ];
            for code in 0..PARTIAL_PUT_ALPHABET.len().pow(6) {
                visit(PartialLens {
                    get,
                    put: decode_partial_put(code),
                });
            }
        }
    }
}

const fn power(mut base: usize, mut exponent: usize) -> usize {
    let mut result = 1;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result *= base;
        }
        exponent >>= 1;
        base *= base;
    }
    result
}

const fn falling_factorial(n: usize, k: usize) -> usize {
    let mut result = 1;
    let mut offset = 0;
    while offset < k {
        result *= n - offset;
        offset += 1;
    }
    result
}

fn bounded_id(value: usize) -> u32 {
    let Ok(id) = u32::try_from(value) else {
        unreachable!("the finite reference bounds fit in a u32 identifier");
    };
    id
}

/// Independent closed-form count `P(2*3, 2) * 3^(2*3)`.
#[must_use]
pub const fn partial_lens_count_certificate() -> usize {
    let get_cells = PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS;
    falling_factorial(get_cells, PARTIAL_LENS_SOURCES)
        * power(
            PARTIAL_PUT_ALPHABET.len(),
            PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS,
        )
}

/// Exact law histogram of the C11 partial-lens carrier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartialLensHistogram {
    /// Every lens.
    pub total: usize,
    /// Lenses satisfying `GetPut`.
    pub get_put: usize,
    /// Lenses satisfying `WeakPutGet`.
    pub weak_put_get: usize,
    /// Lenses satisfying exact-view `PutGet`.
    pub exact_put_get: usize,
    /// Lenses satisfying raw `PutTwice`.
    pub put_twice: usize,
    /// Lenses satisfying both `GetPut` and `WeakPutGet`.
    pub get_put_and_weak_put_get: usize,
    /// `GetPut` and `WeakPutGet` lenses failing `PutTwice`.
    pub get_put_and_weak_put_get_without_put_twice: usize,
    /// `GetPut` and exact `PutGet` lenses failing `PutTwice`.
    pub get_put_and_exact_put_get_without_put_twice: usize,
}

fn coded_put(code: usize, view: usize, complement: usize) -> Option<usize> {
    match (code / power(3, view * 3 + complement)) % 3 {
        0 => None,
        source @ 1..=2 => Some(source - 1),
        _ => unreachable!("a base-three digit is in 0..=2"),
    }
}

/// Independently walk the integer code space and derive the exact law
/// histogram without invoking [`for_each_partial_lens`] or its table decoder.
#[must_use]
pub fn partial_lens_histogram() -> PartialLensHistogram {
    let mut histogram = PartialLensHistogram::default();
    for first in 0..6 {
        for second in (0..6).filter(|second| *second != first) {
            let get = [(first / 3, first % 3), (second / 3, second % 3)];
            for code in 0..power(3, 6) {
                let gp = get.iter().enumerate().all(|(source, &(view, complement))| {
                    coded_put(code, view, complement) == Some(source)
                });
                let weak = (0..2).all(|view| {
                    (0..3).all(|complement| {
                        coded_put(code, view, complement).is_none_or(|source| {
                            let (observed, _) = get[source];
                            coded_put(code, observed, complement) == Some(source)
                        })
                    })
                });
                let exact = (0..2).all(|view| {
                    (0..3).all(|complement| {
                        coded_put(code, view, complement).is_none_or(|source| get[source].0 == view)
                    })
                });
                let twice = (0..2).all(|view| {
                    (0..3).all(|complement| {
                        coded_put(code, view, complement).is_none_or(|source| {
                            coded_put(code, view, get[source].1) == Some(source)
                        })
                    })
                });
                histogram.total += 1;
                histogram.get_put += usize::from(gp);
                histogram.weak_put_get += usize::from(weak);
                histogram.exact_put_get += usize::from(exact);
                histogram.put_twice += usize::from(twice);
                histogram.get_put_and_weak_put_get += usize::from(gp && weak);
                histogram.get_put_and_weak_put_get_without_put_twice +=
                    usize::from(gp && weak && !twice);
                histogram.get_put_and_exact_put_get_without_put_twice +=
                    usize::from(gp && exact && !twice);
            }
        }
    }
    histogram
}

/// The one refusal of a finite partial lens: put is undefined on a
/// view/complement pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialPutUndefined {
    /// The view.
    pub view: usize,
    /// The complement.
    pub complement: usize,
}

/// The finite partial-lens law shape.
///
/// States are the sources and views of a [`PartialLens`], complements its complements, and the Single-Step Basis
/// of a view is the other view (one value edit), always admitted, so an
/// undefined put on it fails Totality.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartialLensShape;

impl crate::lens::LensShape for PartialLensShape {
    type Instance = usize;
    type Complement = usize;

    fn identical(left: &usize, right: &usize) -> bool {
        left == right
    }

    fn complements_identical(left: &usize, right: &usize) -> bool {
        left == right
    }

    fn synth_index(_: &usize, _: &usize) -> crate::lens::SynthIndex {
        crate::lens::SynthIndex::default()
    }

    fn equivalent(
        left: &usize,
        _: &crate::lens::SynthIndex,
        right: &usize,
        _: &crate::lens::SynthIndex,
    ) -> bool {
        left == right
    }

    fn divergence(
        left: &usize,
        _: &crate::lens::SynthIndex,
        right: &usize,
        _: &crate::lens::SynthIndex,
    ) -> Option<crate::lens::Divergence> {
        (left != right).then_some(crate::lens::Divergence {
            kind: crate::lens::DivergenceKind::Instance,
            detail: "partial-lens states differ",
        })
    }

    fn complement_divergence(left: &usize, right: &usize) -> Option<crate::lens::Divergence> {
        (left != right).then_some(crate::lens::Divergence {
            kind: crate::lens::DivergenceKind::Complement,
            detail: "partial-lens complements differ",
        })
    }
}

impl crate::lens::LawShape<PartialLens> for PartialLensShape {
    type Refusal = PartialPutUndefined;
    type Context = ();

    fn get(
        lens: &PartialLens,
        source: &usize,
        (): &(),
    ) -> Result<(usize, usize), PartialPutUndefined> {
        Ok(lens.get[*source])
    }

    fn put(
        lens: &PartialLens,
        view: &usize,
        complement: &usize,
        (): &(),
    ) -> Result<usize, PartialPutUndefined> {
        lens.put(*view, *complement).ok_or(PartialPutUndefined {
            view: *view,
            complement: *complement,
        })
    }

    fn check_put_domain(
        _: &PartialLens,
        _: &usize,
        _: &usize,
        (): &(),
    ) -> Result<(), PartialPutUndefined> {
        Ok(())
    }

    fn single_step_basis(
        _: &PartialLens,
        _: &usize,
        view: &usize,
        _: &usize,
        (): &(),
    ) -> Vec<crate::lens::BasisCase<usize, PartialPutUndefined>> {
        vec![crate::lens::BasisCase {
            ordinal: 0,
            mutation: crate::lens::Mutation::ValueEdit,
            view: PARTIAL_LENS_VIEWS - 1 - view,
            domain: crate::lens::DomainExpectation::Admitted,
        }]
    }

    fn put_get_mode() -> crate::lens::PutGetMode {
        crate::lens::PutGetMode::Strict
    }

    fn rekey_complement(complement: &usize, _: &usize, _: &usize, _: &usize) -> usize {
        *complement
    }

    fn congruence_cases(
        _: &PartialLens,
        _: &usize,
        _: &usize,
        (): &(),
    ) -> Vec<crate::lens::CongruenceCase<usize, usize>> {
        Vec::new()
    }

    fn locality_divergence(
        _: &PartialLens,
        _: &usize,
        _: &usize,
        _: &usize,
        _: &usize,
        _: &usize,
        (): &(),
    ) -> Option<crate::lens::Divergence> {
        None
    }
}

/// The five-gate report of one finite partial lens at one source, by the
/// generic reference checker over [`PartialLensShape`].
#[must_use]
pub fn partial_lens_law_report(
    lens: &PartialLens,
    source: usize,
) -> crate::lens::LawReport<PartialPutUndefined> {
    crate::lens::check_laws_report::<PartialLensShape, PartialLens>(lens, &source, &())
}

/// Largest column and canonical-row cardinalities of the C11 table carrier.
pub const TABLE_FIXTURE_BOUND: usize = 2;

/// Exact table-cell alphabet: absence, Null, F, T, Int(0), Int(1), Str("").
#[must_use]
pub const fn table_cell_alphabet() -> [Option<Value>; 7] {
    [
        None,
        Some(Value::Null),
        Some(Value::Bool(false)),
        Some(Value::Bool(true)),
        Some(Value::Int(0)),
        Some(Value::Int(1)),
        Some(Value::Str(String::new())),
    ]
}

/// One labelled table source paired with one projection lens, represented by
/// its retained-column bit mask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableFixture {
    /// Number of source columns `c0..c{columns-1}`.
    pub columns: usize,
    /// Retained source-column bits.
    pub retained: usize,
    /// Canonical rows `0..rows`, all anchored at `row`.
    pub source: Table,
}

/// Value kind of every fixture column vertex.
pub const TABLE_FIXTURE_VALUE_KIND: &str = "any";
/// Kind of the one fixture table vertex.
pub const TABLE_FIXTURE_TABLE_KIND: &str = "record";

fn table_fixture_schema(columns: usize, retained: usize) -> Schema {
    let mut schema = Schema::default().vertex("row", TABLE_FIXTURE_TABLE_KIND);
    for column in 0..columns {
        if retained & (1 << column) == 0 {
            continue;
        }
        let name = format!("c{column}");
        let vertex = format!("row.{name}");
        schema = schema.vertex(&vertex, TABLE_FIXTURE_VALUE_KIND).edge(
            "row",
            &vertex,
            "prop",
            Some(&name),
        );
    }
    schema
}

impl TableFixture {
    /// The source schema: table `row` with columns `c0..c{columns-1}`, each
    /// a value vertex `row.c<i>` of kind `any` reached by the `prop` edge
    /// named `c<i>`.
    #[must_use]
    pub fn source_schema(&self) -> Schema {
        table_fixture_schema(self.columns, power(2, self.columns) - 1)
    }

    /// The target schema: the table and its retained columns.
    #[must_use]
    pub fn target_schema(&self) -> Schema {
        table_fixture_schema(self.columns, self.retained)
    }

    /// The projection migration: every target vertex and edge survives as
    /// itself.
    #[must_use]
    pub fn migration(&self) -> Migration {
        let target = self.target_schema();
        Migration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges,
            ..Migration::default()
        }
    }
}

/// Every one-step edit of a table-fixture view.
///
/// The edits are each cell of each view row
/// set to each other member of [`table_cell_alphabet`] (absence included),
/// each row removed, one fresh empty row at the least id not in the view and
/// not reserved, and one empty row at each reserved id. Rows are taken in
/// ascending id, columns in ascending name.
#[must_use]
pub fn table_view_edits(view: &Table, columns: &[String], reserved: &[u32]) -> Vec<Table> {
    let mut out = Vec::new();
    for (id, (table, cells)) in &view.rows {
        for column in columns {
            for value in table_cell_alphabet() {
                if cells.get(column) == value.as_ref() {
                    continue;
                }
                let mut edited = view.clone();
                let mut row = cells.clone();
                match value {
                    Some(value) => {
                        row.insert(column.clone(), value);
                    }
                    None => {
                        row.remove(column);
                    }
                }
                edited.rows.insert(*id, (table.clone(), row));
                out.push(edited);
            }
        }
    }
    for id in view.rows.keys() {
        let mut edited = view.clone();
        edited.rows.remove(id);
        edited.pairs.retain(|(_, s, t)| s != id && t != id);
        out.push(edited);
    }
    let fresh = (0..u32::MAX)
        .find(|id| !view.rows.contains_key(id) && !reserved.contains(id))
        .unwrap_or(u32::MAX);
    for id in std::iter::once(fresh).chain(reserved.iter().copied()) {
        let mut edited = view.clone();
        edited.rows.insert(id, ("row".to_owned(), BTreeMap::new()));
        out.push(edited);
    }
    out
}

/// The count of [`table_view_edits`].
///
/// For a view of `rows` rows over `columns` columns with `reserved` reserved
/// ids, every cell has `6` alternatives, each row one removal,
/// and `1 + reserved` row insertions.
#[must_use]
pub fn table_view_edits_count(rows: usize, columns: usize, reserved: usize) -> usize {
    rows * columns * (table_cell_alphabet().len() - 1) + rows + 1 + reserved
}

/// Source column names of the table-writer carrier.
pub const WRITER_SOURCES: [&str; 2] = ["x", "y"];
/// Image choices of one writer source column: dropped, `a` or `b`.
pub const WRITER_IMAGES: [Option<&str>; 3] = [None, Some("a"), Some("b")];
/// Source cell values of the writer carrier.
pub const WRITER_VALUES: [i64; 3] = [-1, 0, 1];
/// Edited view cell values of the writer carrier.
pub const WRITER_EDITS: [i64; 2] = [-2, 2];

/// One writer case: a column mapping, a one-row source and, when present,
/// one edited view cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriterFixture {
    /// Image of `x` and of `y`.
    pub images: [Option<&'static str>; 2],
    /// Source values of `x` and `y`.
    pub values: [i64; 2],
}

impl WriterFixture {
    /// The source schema: table `t` with int columns `t.x`, `t.y`.
    #[must_use]
    pub fn source_schema(&self) -> Schema {
        writer_schema(&WRITER_SOURCES)
    }

    /// The target columns, ascending and without repetition.
    #[must_use]
    pub fn target_columns(&self) -> Vec<&'static str> {
        let set: std::collections::BTreeSet<&'static str> =
            self.images.iter().flatten().copied().collect();
        set.into_iter().collect()
    }

    /// The target schema: table `t` with the image columns.
    #[must_use]
    pub fn target_schema(&self) -> Schema {
        writer_schema(&self.target_columns())
    }

    /// The column mapping: `t.<s>` to `t.<image>` and the column edge to the
    /// image column edge; a source column with no image is dropped.
    #[must_use]
    pub fn migration(&self) -> Migration {
        let target = self.target_schema();
        let mut migration = Migration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges,
            ..Migration::default()
        };
        for (source, image) in WRITER_SOURCES.iter().zip(self.images) {
            if let Some(image) = image
                && image != *source
            {
                migration
                    .vertex_remap
                    .insert(format!("t.{source}"), format!("t.{image}"));
                migration
                    .edge_remap
                    .insert(writer_edge(source), writer_edge(image));
            }
        }
        migration
    }

    /// The one-row source: row 0 of `t` with `x` and `y`.
    #[must_use]
    pub fn source(&self) -> Table {
        let mut table = Table::default();
        table.rows.insert(
            0,
            (
                "t".to_owned(),
                BTreeMap::from([
                    ("x".to_owned(), Value::Int(self.values[0])),
                    ("y".to_owned(), Value::Int(self.values[1])),
                ]),
            ),
        );
        table
    }

    /// The writer fixed by this fixture without consulting either table
    /// implementation: the last explicitly renamed source, else the
    /// undisplaced same-named source column.
    #[must_use]
    pub fn expected_writer(&self, target: &str) -> Option<&'static str> {
        let explicit = WRITER_SOURCES
            .iter()
            .zip(self.images)
            .filter(|(source, image)| {
                image.is_some_and(|image| image == target && image != **source)
            })
            .map(|(source, _)| *source)
            .next_back();
        explicit.or_else(|| {
            WRITER_SOURCES
                .iter()
                .zip(self.images)
                .find(|(source, _)| **source == target)
                .and_then(|(source, image)| (image == Some(*source)).then_some(*source))
        })
    }
}

/// Kind of the writer carrier's column vertices.
pub const WRITER_VALUE_KIND: &str = "int";

fn writer_edge(column: &str) -> SEdge {
    SEdge::new("t", &format!("t.{column}"), "prop", Some(column))
}

fn writer_schema(columns: &[&str]) -> Schema {
    let mut schema = Schema::default().vertex("t", TABLE_FIXTURE_TABLE_KIND);
    for column in columns {
        schema = schema
            .vertex(&format!("t.{column}"), WRITER_VALUE_KIND)
            .edge("t", &format!("t.{column}"), "prop", Some(column));
    }
    schema
}

/// Stream every writer mapping and source row.
pub fn for_each_writer_fixture(mut visit: impl FnMut(WriterFixture)) {
    for x in WRITER_IMAGES {
        for y in WRITER_IMAGES {
            for x_value in WRITER_VALUES {
                for y_value in WRITER_VALUES {
                    visit(WriterFixture {
                        images: [x, y],
                        values: [x_value, y_value],
                    });
                }
            }
        }
    }
}

/// Independent writer counts `(mappings, source rows, edited views)`.
///
/// There are `3^2`
/// mappings, `3^2` value pairs per mapping, and per mapping and source the
/// number of distinct image columns times `|WRITER_EDITS|`. Over the nine
/// mappings the distinct image columns number `0 + 4 + 2 + 2 * 2 = 10`, so
/// the edited views number `9 * 10 * 2 = 180`.
#[must_use]
pub const fn writer_count_certificate() -> (usize, usize, usize) {
    let mappings = WRITER_IMAGES.len() * WRITER_IMAGES.len();
    let values = WRITER_VALUES.len() * WRITER_VALUES.len();
    // Distinct image columns over the nine mappings: (None, None) has 0;
    // the four with exactly one `None` have 1; (a, a) and (b, b) have 1;
    // (a, b) and (b, a) have 2.
    let distinct_images = 4 + 2 + 2 * 2;
    (
        mappings,
        mappings * values,
        values * distinct_images * WRITER_EDITS.len(),
    )
}

/// Exact number of edited writer cases with two explicit writers.
///
/// The two colliding mappings are `(a, a)` and `(b, b)`; each has nine
/// source rows and two edited values.
#[must_use]
pub const fn last_explicit_writer_count_certificate() -> usize {
    2 * WRITER_VALUES.len() * WRITER_VALUES.len() * WRITER_EDITS.len()
}

/// A target-only column with no source writer, edited to one carrier value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivedWriterFixture {
    /// The edited value inserted at target column `z`.
    pub edit: i64,
}

impl DerivedWriterFixture {
    /// Source schema with columns `x` and `y`.
    #[must_use]
    pub fn source_schema(self) -> Schema {
        writer_schema(&WRITER_SOURCES)
    }

    /// Target schema with the two source columns and derived column `z`.
    #[must_use]
    pub fn target_schema(self) -> Schema {
        writer_schema(&["x", "y", "z"])
    }

    /// Identity migration on the source columns into the larger target.
    #[must_use]
    pub fn migration(self) -> Migration {
        let target = self.target_schema();
        Migration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges,
            ..Migration::default()
        }
    }

    /// One source row used by every derived-writer refusal.
    #[must_use]
    pub fn source(self) -> Table {
        WriterFixture {
            images: [Some("x"), Some("y")],
            values: [0, 1],
        }
        .source()
    }
}

/// Stream every edit of the target-only column in the writer alphabet.
pub fn for_each_derived_writer_fixture(mut visit: impl FnMut(DerivedWriterFixture)) {
    for edit in WRITER_EDITS {
        visit(DerivedWriterFixture { edit });
    }
}

/// Exact number of target-only-column refusal cases.
#[must_use]
pub const fn derived_writer_count_certificate() -> usize {
    WRITER_EDITS.len()
}

/// One named construction witnessing table-complement coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableComplementConstruction {
    /// Stable construction name used in certificate diagnostics.
    pub name: &'static str,
    /// Source schema.
    pub source_schema: Schema,
    /// Target schema.
    pub target_schema: Schema,
    /// Migration from source to target.
    pub migration: Migration,
    /// Source table.
    pub source: Table,
}

fn table_construction_schema(tables: &[&str], relations: &[SEdge]) -> Schema {
    let mut schema = Schema::default();
    for table in tables {
        schema = schema.vertex(table, TABLE_FIXTURE_TABLE_KIND);
    }
    for relation in relations {
        schema = schema.edge(
            &relation.src,
            &relation.tgt,
            &relation.kind,
            relation.name.as_deref(),
        );
    }
    schema
}

const fn table_construction(
    name: &'static str,
    source_schema: Schema,
    target_schema: Schema,
    migration: Migration,
    source: Table,
) -> TableComplementConstruction {
    TableComplementConstruction {
        name,
        source_schema,
        target_schema,
        migration,
        source,
    }
}

fn dropped_table_construction() -> TableComplementConstruction {
    table_construction(
        "dropped table row",
        table_construction_schema(&["a", "b"], &[]),
        table_construction_schema(&["a"], &[]),
        Migration {
            surviving_verts: ["a".to_owned()].into(),
            ..Migration::default()
        },
        Table {
            rows: BTreeMap::from([(0, ("b".to_owned(), BTreeMap::new()))]),
            pairs: BTreeSet::new(),
        },
    )
}

fn dropped_column_construction() -> TableComplementConstruction {
    let dropped_column = WriterFixture {
        images: [None, Some("y")],
        values: [1, 2],
    };
    table_construction(
        "dropped column",
        dropped_column.source_schema(),
        dropped_column.target_schema(),
        dropped_column.migration(),
        dropped_column.source(),
    )
}

fn swapped_columns_construction() -> TableComplementConstruction {
    let swapped_columns = WriterFixture {
        images: [Some("y"), Some("x")],
        values: [1, 2],
    };
    table_construction(
        "swapped column originals",
        swapped_columns.source_schema(),
        swapped_columns.target_schema(),
        swapped_columns.migration(),
        swapped_columns.source(),
    )
}

fn dropped_pair_construction() -> TableComplementConstruction {
    let dropped = SEdge::new("a", "b", "foreign-key", Some("dropped"));
    table_construction(
        "dropped foreign-key pair",
        table_construction_schema(&["a", "b"], std::slice::from_ref(&dropped)),
        table_construction_schema(&["a", "b"], &[]),
        Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            ..Migration::default()
        },
        Table {
            rows: BTreeMap::from([
                (0, ("a".to_owned(), BTreeMap::new())),
                (1, ("b".to_owned(), BTreeMap::new())),
            ]),
            pairs: [(dropped, 0, 1)].into(),
        },
    )
}

fn merged_pair_construction() -> TableComplementConstruction {
    let first = SEdge::new("a", "b", "foreign-key", Some("first"));
    let second = SEdge::new("a", "b", "foreign-key", Some("second"));
    let joined = SEdge::new("a", "b", "foreign-key", Some("joined"));
    table_construction(
        "merged foreign-key origins",
        table_construction_schema(&["a", "b"], &[first.clone(), second.clone()]),
        table_construction_schema(&["a", "b"], std::slice::from_ref(&joined)),
        Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            surviving_edges: [joined.clone()].into(),
            edge_remap: [(first.clone(), joined.clone()), (second.clone(), joined)].into(),
            ..Migration::default()
        },
        Table {
            rows: BTreeMap::from([
                (0, ("a".to_owned(), BTreeMap::new())),
                (1, ("b".to_owned(), BTreeMap::new())),
            ]),
            pairs: [(first, 0, 1), (second, 0, 1)].into(),
        },
    )
}

fn merged_table_construction() -> TableComplementConstruction {
    table_construction(
        "merged table origins",
        table_construction_schema(&["a", "b"], &[]),
        table_construction_schema(&["t"], &[]),
        Migration {
            surviving_verts: ["t".to_owned()].into(),
            vertex_remap: [
                ("a".to_owned(), "t".to_owned()),
                ("b".to_owned(), "t".to_owned()),
            ]
            .into(),
            ..Migration::default()
        },
        Table {
            rows: BTreeMap::from([
                (0, ("a".to_owned(), BTreeMap::new())),
                (1, ("b".to_owned(), BTreeMap::new())),
            ]),
            pairs: BTreeSet::new(),
        },
    )
}

fn contracted_pair_construction() -> TableComplementConstruction {
    let left = SEdge::new("a", "m", "foreign-key", Some("left"));
    let right = SEdge::new("m", "b", "foreign-key", Some("right"));
    let contracted = SEdge::new("a", "b", "foreign-key", Some("contracted"));
    table_construction(
        "contracted foreign-key pair",
        table_construction_schema(&["a", "m", "b"], &[left.clone(), right.clone()]),
        table_construction_schema(&["a", "b"], std::slice::from_ref(&contracted)),
        Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            surviving_edges: [contracted.clone()].into(),
            resolver: [(("a".to_owned(), "b".to_owned()), contracted)].into(),
            ..Migration::default()
        },
        Table {
            rows: BTreeMap::from([
                (0, ("a".to_owned(), BTreeMap::new())),
                (1, ("m".to_owned(), BTreeMap::new())),
                (2, ("b".to_owned(), BTreeMap::new())),
            ]),
            pairs: [(left, 0, 1), (right, 1, 2)].into(),
        },
    )
}

/// Stream the seven named constructions whose union witnesses every T.3
/// residue coordinate, table provenance and synthesized foreign-key pairs.
pub fn for_each_table_complement_construction(mut visit: impl FnMut(TableComplementConstruction)) {
    for construction in [
        dropped_table_construction(),
        dropped_column_construction(),
        swapped_columns_construction(),
        dropped_pair_construction(),
        merged_pair_construction(),
        merged_table_construction(),
        contracted_pair_construction(),
    ] {
        visit(construction);
    }
}

/// Exact cardinality of [`for_each_table_complement_construction`].
#[must_use]
pub const fn table_complement_construction_count_certificate() -> usize {
    7
}

/// A named refusal in the finite outside-domain certificate for table get.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableGetRefusalCase {
    /// An edge expands to a path, which one relational pair cannot encode.
    NotTableRepresentable,
    /// Graph survival and relational-functor survival disagree on a table.
    SurvivalRulesDisagree,
    /// A kept relation is mapped to no coherent target relation.
    IncoherentEdgeImage,
    /// Contracting through a hidden table finds no target relation.
    NoEdgeFound,
    /// Contracting through a hidden table finds two target relations.
    AmbiguousEdge,
    /// A contraction resolver names no coherent target relation.
    IncoherentPairImage,
    /// A source row names a table outside the relational boundary.
    NonConforming,
    /// A survival predicate refuses with a typed error.
    PredicateRefusal,
    /// A row transform refuses with a typed error.
    TransformRefusal,
}

impl TableGetRefusalCase {
    /// Every named refusal, in the order claimed by the law statement.
    pub const ALL: [Self; 9] = [
        Self::NotTableRepresentable,
        Self::SurvivalRulesDisagree,
        Self::IncoherentEdgeImage,
        Self::NoEdgeFound,
        Self::AmbiguousEdge,
        Self::IncoherentPairImage,
        Self::NonConforming,
        Self::PredicateRefusal,
        Self::TransformRefusal,
    ];

    /// Stable diagnostic name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NotTableRepresentable => "NotTableRepresentable",
            Self::SurvivalRulesDisagree => "SurvivalRulesDisagree",
            Self::IncoherentEdgeImage => "IncoherentEdgeImage",
            Self::NoEdgeFound => "NoEdgeFound",
            Self::AmbiguousEdge => "AmbiguousEdge",
            Self::IncoherentPairImage => "IncoherentPairImage",
            Self::NonConforming => "NonConforming",
            Self::PredicateRefusal => "PredicateRefusal",
            Self::TransformRefusal => "TransformRefusal",
        }
    }

    /// Reference-owned schemas, migration and source for this refusal.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn fixture(self) -> TableGetRefusalFixture {
        let relation =
            |src: &str, tgt: &str, name: &str| SEdge::new(src, tgt, "foreign-key", Some(name));
        let table = |names: &[&str], edges: &[SEdge]| table_construction_schema(names, edges);
        match self {
            Self::NotTableRepresentable => {
                let source_schema = table(&["a", "b"], &[relation("a", "b", "ab")]);
                let target_schema = table(
                    &["a", "m", "b"],
                    &[relation("a", "m", "am"), relation("m", "b", "mb")],
                );
                TableGetRefusalFixture {
                    source_schema,
                    target_schema,
                    migration: Migration {
                        surviving_verts: ["a".to_owned(), "m".to_owned(), "b".to_owned()].into(),
                        expansion_path: BTreeMap::from([(
                            ("a".to_owned(), "b".to_owned()),
                            vec!["m".to_owned()],
                        )]),
                        ..Migration::default()
                    },
                    source: Table::default(),
                }
            }
            Self::SurvivalRulesDisagree => TableGetRefusalFixture {
                source_schema: table(&["a", "b"], &[]),
                target_schema: table(&["b"], &[]),
                migration: Migration {
                    surviving_verts: ["b".to_owned()].into(),
                    vertex_remap: [("a".to_owned(), "b".to_owned())].into(),
                    ..Migration::default()
                },
                source: Table::default(),
            },
            Self::IncoherentEdgeImage => {
                let source_edge = relation("a", "b", "source");
                let target_edge = relation("a", "b", "target");
                let incoherent = relation("a", "b", "absent");
                TableGetRefusalFixture {
                    source_schema: table(&["a", "b"], std::slice::from_ref(&source_edge)),
                    target_schema: table(&["a", "b"], &[target_edge]),
                    migration: Migration {
                        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                        edge_remap: [(source_edge, incoherent)].into(),
                        ..Migration::default()
                    },
                    source: Table::default(),
                }
            }
            Self::NoEdgeFound | Self::AmbiguousEdge | Self::IncoherentPairImage => {
                let left = relation("a", "m", "left");
                let right = relation("m", "b", "right");
                let target_edges = match self {
                    Self::NoEdgeFound => Vec::new(),
                    Self::AmbiguousEdge => {
                        vec![relation("a", "b", "first"), relation("a", "b", "second")]
                    }
                    Self::IncoherentPairImage => vec![relation("a", "b", "only")],
                    _ => unreachable!("grouped contraction refusal"),
                };
                let resolver = (self == Self::IncoherentPairImage).then(|| {
                    (
                        ("a".to_owned(), "b".to_owned()),
                        relation("a", "b", "absent"),
                    )
                });
                TableGetRefusalFixture {
                    source_schema: table(&["a", "m", "b"], &[left.clone(), right.clone()]),
                    target_schema: table(&["a", "b"], &target_edges),
                    migration: Migration {
                        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                        resolver: resolver.into_iter().collect(),
                        ..Migration::default()
                    },
                    source: Table {
                        rows: [
                            (0, ("a".to_owned(), BTreeMap::new())),
                            (1, ("m".to_owned(), BTreeMap::new())),
                            (2, ("b".to_owned(), BTreeMap::new())),
                        ]
                        .into(),
                        pairs: [(left, 0, 1), (right, 1, 2)].into(),
                    },
                }
            }
            Self::NonConforming => TableGetRefusalFixture {
                source_schema: table(&["a"], &[]),
                target_schema: table(&["a"], &[]),
                migration: Migration {
                    surviving_verts: ["a".to_owned()].into(),
                    ..Migration::default()
                },
                source: Table {
                    rows: [(0, ("outside".to_owned(), BTreeMap::new()))].into(),
                    pairs: BTreeSet::new(),
                },
            },
            Self::PredicateRefusal | Self::TransformRefusal => {
                let source_schema = writer_schema(&["x"]);
                TableGetRefusalFixture {
                    target_schema: source_schema.clone(),
                    migration: Migration {
                        surviving_verts: source_schema.vertices.keys().cloned().collect(),
                        surviving_edges: source_schema.edges.clone(),
                        conditional: (self == Self::PredicateRefusal)
                            .then(|| "t".to_owned())
                            .into_iter()
                            .collect(),
                        transformed: (self == Self::TransformRefusal)
                            .then(|| "t".to_owned())
                            .into_iter()
                            .collect(),
                        ..Migration::default()
                    },
                    source_schema,
                    source: Table {
                        rows: [(
                            0,
                            (
                                "t".to_owned(),
                                BTreeMap::from([("x".to_owned(), Value::Int(1))]),
                            ),
                        )]
                        .into(),
                        pairs: BTreeSet::new(),
                    },
                }
            }
        }
    }
}

/// Reference-owned data for one table-get refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableGetRefusalFixture {
    /// Source schema.
    pub source_schema: Schema,
    /// Target schema.
    pub target_schema: Schema,
    /// Structural migration and reference value-layer markers.
    pub migration: Migration,
    /// Source table used when the refusal occurs during evaluation.
    pub source: Table,
}

/// Stream every named outside-domain table-get refusal.
pub fn for_each_table_get_refusal(mut visit: impl FnMut(TableGetRefusalCase)) {
    for case in TableGetRefusalCase::ALL {
        visit(case);
    }
}

/// Exact number of named outside-domain table-get refusals.
#[must_use]
pub const fn table_get_refusal_count_certificate() -> usize {
    TableGetRefusalCase::ALL.len()
}

/// Stream every expressible table fixture in the exact C11 alphabet.
pub fn for_each_table_fixture(mut visit: impl FnMut(TableFixture)) {
    for columns in 0..=TABLE_FIXTURE_BOUND {
        for retained in 0..power(2, columns) {
            for rows in 0..=TABLE_FIXTURE_BOUND {
                let assignments = power(7, rows * columns);
                for mut code in 0..assignments {
                    let mut source = Table::default();
                    for row in 0..rows {
                        let mut cells = BTreeMap::new();
                        for column in 0..columns {
                            if let Some(value) = table_cell_alphabet()[code % 7].clone() {
                                cells.insert(format!("c{column}"), value);
                            }
                            code /= 7;
                        }
                        source
                            .rows
                            .insert(bounded_id(row), ("row".to_owned(), cells));
                    }
                    visit(TableFixture {
                        columns,
                        retained,
                        source,
                    });
                }
            }
        }
    }
}

/// Exact number of sources in the bounded two-table relational carrier.
pub const RELATIONAL_TABLE_SOURCE_COUNT: usize = 1_831;

/// Stream every source in the bounded two-table relational carrier.
///
/// Tables `a` and `b` each have two reserved ids. Every subset of those ids
/// is present; every present row's scalar cell is absent, zero, or one; and
/// every subset of the possible `a.ab -> b` relation pairs is present.
pub fn for_each_relational_table_source(mut visit: impl FnMut(Table)) {
    const A_IDS: [u32; 2] = [0, 1];
    const B_IDS: [u32; 2] = [2, 3];

    for a_mask in 0_u8..4 {
        let a_ids: Vec<u32> = A_IDS
            .into_iter()
            .enumerate()
            .filter_map(|(bit, id)| (a_mask & (1 << bit) != 0).then_some(id))
            .collect();
        for b_mask in 0_u8..4 {
            let b_ids: Vec<u32> = B_IDS
                .into_iter()
                .enumerate()
                .filter_map(|(bit, id)| (b_mask & (1 << bit) != 0).then_some(id))
                .collect();
            let row_count = a_ids.len() + b_ids.len();
            for mut cell_code in 0..power(3, row_count) {
                let mut base = Table::default();
                for (table, ids, column) in [("a", &a_ids, "x"), ("b", &b_ids, "y")] {
                    for id in ids {
                        let mut cells = BTreeMap::new();
                        match cell_code % 3 {
                            0 => {}
                            1 => {
                                cells.insert(column.to_owned(), Value::Int(0));
                            }
                            2 => {
                                cells.insert(column.to_owned(), Value::Int(1));
                            }
                            _ => unreachable!("the cell alphabet has size three"),
                        }
                        cell_code /= 3;
                        base.rows.insert(*id, (table.to_owned(), cells));
                    }
                }
                let pairs: Vec<(u32, u32)> = a_ids
                    .iter()
                    .flat_map(|source| b_ids.iter().map(move |target| (*source, *target)))
                    .collect();
                for pair_mask in 0..power(2, pairs.len()) {
                    let mut source = base.clone();
                    for (bit, &(source_id, target_id)) in pairs.iter().enumerate() {
                        if pair_mask & (1 << bit) != 0 {
                            source.pairs.insert((
                                SEdge::new("a", "b", "relation", Some("ab")),
                                source_id,
                                target_id,
                            ));
                        }
                    }
                    visit(source);
                }
            }
        }
    }
}

const fn relational_choose_two(selected: usize) -> usize {
    match selected {
        0 | 2 => 1,
        1 => 2,
        _ => 0,
    }
}

/// Derive the relational carrier size independently as
/// `sum C(2,a) C(2,b) 3^(a+b) 2^(a*b)`.
#[must_use]
pub const fn relational_table_source_count_certificate() -> usize {
    let mut count = 0;
    let mut a = 0;
    while a <= 2 {
        let mut b = 0;
        while b <= 2 {
            count += relational_choose_two(a)
                * relational_choose_two(b)
                * power(3, a + b)
                * power(2, a * b);
            b += 1;
        }
        a += 1;
    }
    count
}

/// Independent closed-form counts for every `(columns, rows)` cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableFixtureCounts {
    /// Source assignments indexed by `[columns][rows]`.
    pub sources_by_columns_and_rows: [[usize; 3]; 3],
    /// Projection lenses indexed by `columns`.
    pub lenses_by_columns: [usize; 3],
    /// Projection-lens/source cases indexed by `[columns][rows]`.
    pub by_columns_and_rows: [[usize; 3]; 3],
    /// Sums for zero, one and two columns.
    pub by_columns: [usize; 3],
    /// Sum of all nine cells.
    pub total: usize,
}

/// Derive table case counts from `2^columns * 7^(columns*rows)`, independently
/// of [`for_each_table_fixture`].
#[must_use]
pub const fn table_fixture_count_certificate() -> TableFixtureCounts {
    let mut sources = [[0; 3]; 3];
    let mut cases = [[0; 3]; 3];
    let mut lenses = [0; 3];
    let mut columns_totals = [0; 3];
    let mut total = 0;
    let mut columns = 0;
    while columns <= TABLE_FIXTURE_BOUND {
        lenses[columns] = power(2, columns);
        let mut rows = 0;
        while rows <= TABLE_FIXTURE_BOUND {
            let source_count = power(7, columns * rows);
            let count = lenses[columns] * source_count;
            sources[columns][rows] = source_count;
            cases[columns][rows] = count;
            columns_totals[columns] += count;
            total += count;
            rows += 1;
        }
        columns += 1;
    }
    TableFixtureCounts {
        sources_by_columns_and_rows: sources,
        lenses_by_columns: lenses,
        by_columns_and_rows: cases,
        by_columns: columns_totals,
        total,
    }
}

/// One case in the complete finite certificate for the table-complement law.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableComplementLawCase {
    /// One projection-lens/source case from the broad table carrier.
    Fixture(TableFixture),
    /// One named construction isolating a complement coordinate.
    Construction(Box<TableComplementConstruction>),
    /// One named outside-domain refusal.
    Refusal(TableGetRefusalCase),
}

/// Stream the broad table carrier followed by the seven coordinate-isolating
/// table-complement constructions and every named outside-domain refusal.
pub fn for_each_table_complement_law_case(mut visit: impl FnMut(TableComplementLawCase)) {
    for_each_table_fixture(|fixture| visit(TableComplementLawCase::Fixture(fixture)));
    for_each_table_complement_construction(|construction| {
        visit(TableComplementLawCase::Construction(Box::new(construction)));
    });
    for_each_table_get_refusal(|case| visit(TableComplementLawCase::Refusal(case)));
}

/// Exact number of cases in [`for_each_table_complement_law_case`].
#[must_use]
pub const fn table_complement_law_count_certificate() -> usize {
    table_fixture_count_certificate().total
        + table_complement_construction_count_certificate()
        + table_get_refusal_count_certificate()
}

fn table_fixture_view(fixture: &TableFixture) -> Table {
    let mut view = fixture.source.clone();
    for (_, cells) in view.rows.values_mut() {
        cells.retain(|column, _| {
            column
                .strip_prefix('c')
                .and_then(|suffix| suffix.parse::<usize>().ok())
                .is_some_and(|index| fixture.retained & (1 << index) != 0)
        });
    }
    view
}

/// One projection-lens get/put observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableFixturePutCase {
    /// Lens and source fixture.
    pub fixture: TableFixture,
    /// Unedited view or one member of its Single-Step Basis.
    pub view: Table,
}

/// One concrete edit in the last-writer carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableWriterEditCase {
    /// Writer mapping and source values.
    pub fixture: WriterFixture,
    /// Edited target column.
    pub target: &'static str,
    /// Replacement value.
    pub edit: i64,
}

/// The five relation-pair observations needed by the table put theorem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TablePairPutCase {
    /// Insert a pair whose only possible source would be a contraction.
    InsertContracted,
    /// Put an unedited synthesized pair.
    SynthesizedUnedited,
    /// Remove a synthesized pair.
    SynthesizedRemoved,
    /// Put an unedited pair with one direct and one contracted origin.
    TwoOriginUnedited,
    /// Remove a pair with one direct and one contracted origin.
    TwoOriginRemoved,
}

/// A named refusal required by the table-put domain theorem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TablePutRefusalCase {
    /// A complement stamped by another lens is rejected before restoration.
    Identity,
    /// Reusing a row id reserved by a dropped row is rejected.
    IdCollision,
    /// An inserted row with several source-table preimages is rejected.
    AmbiguousReverseAnchor,
    /// An edit through a transform without a valid inverse is rejected.
    TransformNotInvertible,
}

impl TablePutRefusalCase {
    /// Every named table-put refusal, in theorem order.
    pub const ALL: [Self; 4] = [
        Self::Identity,
        Self::IdCollision,
        Self::AmbiguousReverseAnchor,
        Self::TransformNotInvertible,
    ];

    /// Stable diagnostic name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Identity => "Identity",
            Self::IdCollision => "IdCollision",
            Self::AmbiguousReverseAnchor => "AmbiguousReverseAnchor",
            Self::TransformNotInvertible => "TransformNotInvertible",
        }
    }
}

/// Stream every named refusal required by the table-put domain theorem.
pub fn for_each_table_put_refusal(mut visit: impl FnMut(TablePutRefusalCase)) {
    for case in TablePutRefusalCase::ALL {
        visit(case);
    }
}

/// Exact number of named table-put refusals.
#[must_use]
pub const fn table_put_refusal_count_certificate() -> usize {
    TablePutRefusalCase::ALL.len()
}

impl TablePairPutCase {
    /// Every relation-pair observation in theorem order.
    pub const ALL: [Self; 5] = [
        Self::InsertContracted,
        Self::SynthesizedUnedited,
        Self::SynthesizedRemoved,
        Self::TwoOriginUnedited,
        Self::TwoOriginRemoved,
    ];

    /// Whether the source has the contracted `a -> m -> b` chain.
    #[must_use]
    pub const fn has_chain(self) -> bool {
        !matches!(self, Self::InsertContracted)
    }

    /// Whether the source also has the direct `a -> b` pair.
    #[must_use]
    pub const fn has_direct(self) -> bool {
        matches!(self, Self::TwoOriginUnedited | Self::TwoOriginRemoved)
    }

    /// Whether the edited view contains the target pair.
    #[must_use]
    pub const fn view_has_pair(self) -> bool {
        matches!(
            self,
            Self::InsertContracted | Self::SynthesizedUnedited | Self::TwoOriginUnedited
        )
    }

    /// Reference-owned schemas, migration and source for this observation.
    #[must_use]
    pub fn fixture(self) -> TablePairPutFixture {
        let left = SEdge::new("a", "m", "foreign-key", Some("left"));
        let right = SEdge::new("m", "b", "foreign-key", Some("right"));
        let direct = SEdge::new("a", "b", "foreign-key", Some("direct"));
        let image = SEdge::new("a", "b", "foreign-key", Some("image"));
        let mut source_edges = vec![left.clone(), right.clone()];
        if self.has_direct() {
            source_edges.push(direct.clone());
        }
        let mut pairs = BTreeSet::new();
        if self.has_chain() {
            pairs.extend([(left, 0, 1), (right, 1, 2)]);
        }
        if self.has_direct() {
            pairs.insert((direct.clone(), 0, 2));
        }
        TablePairPutFixture {
            source_schema: table_construction_schema(&["a", "m", "b"], &source_edges),
            target_schema: table_construction_schema(&["a", "b"], std::slice::from_ref(&image)),
            migration: Migration {
                surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                surviving_edges: self
                    .has_direct()
                    .then(|| direct.clone())
                    .into_iter()
                    .collect(),
                edge_remap: self
                    .has_direct()
                    .then(|| (direct, image.clone()))
                    .into_iter()
                    .collect(),
                resolver: BTreeMap::from([(("a".to_owned(), "b".to_owned()), image.clone())]),
                ..Migration::default()
            },
            source: Table {
                rows: [
                    (0, ("a".to_owned(), BTreeMap::new())),
                    (1, ("m".to_owned(), BTreeMap::new())),
                    (2, ("b".to_owned(), BTreeMap::new())),
                ]
                .into(),
                pairs,
            },
            image,
        }
    }
}

/// Reference-owned data for one relation-pair put observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePairPutFixture {
    /// Source schema.
    pub source_schema: Schema,
    /// Target schema.
    pub target_schema: Schema,
    /// Migration that keeps the direct edge when present and contracts the chain.
    pub migration: Migration,
    /// Source table.
    pub source: Table,
    /// Target relation label.
    pub image: SEdge,
}

/// One case in the complete finite certificate for table get and put.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableGetPutLawCase {
    /// One unedited or one-step-edited projection-lens view.
    Fixture(TableFixturePutCase),
    /// One last-writer edit.
    Writer(TableWriterEditCase),
    /// One edit to a target-only derived column.
    Derived(DerivedWriterFixture),
    /// One synthesized or two-origin relation-pair observation.
    Pair(TablePairPutCase),
    /// One named identity or put-domain refusal.
    Refusal(TablePutRefusalCase),
}

/// Stream every projection observation, every writer edit, both target-only
/// derived-column edits, all relation-pair observations and every named
/// identity or put-domain refusal.
pub fn for_each_table_get_put_law_case(mut visit: impl FnMut(TableGetPutLawCase)) {
    for_each_table_fixture(|fixture| {
        let view = table_fixture_view(&fixture);
        visit(TableGetPutLawCase::Fixture(TableFixturePutCase {
            fixture: fixture.clone(),
            view: view.clone(),
        }));
        let columns: Vec<String> = (0..fixture.columns)
            .filter(|column| fixture.retained & (1 << column) != 0)
            .map(|column| format!("c{column}"))
            .collect();
        for edited in table_view_edits(&view, &columns, &[]) {
            visit(TableGetPutLawCase::Fixture(TableFixturePutCase {
                fixture: fixture.clone(),
                view: edited,
            }));
        }
    });
    for_each_writer_fixture(|fixture| {
        for target in fixture.target_columns() {
            for edit in WRITER_EDITS {
                visit(TableGetPutLawCase::Writer(TableWriterEditCase {
                    fixture: fixture.clone(),
                    target,
                    edit,
                }));
            }
        }
    });
    for_each_derived_writer_fixture(|fixture| visit(TableGetPutLawCase::Derived(fixture)));
    for case in TablePairPutCase::ALL {
        visit(TableGetPutLawCase::Pair(case));
    }
    for_each_table_put_refusal(|case| visit(TableGetPutLawCase::Refusal(case)));
}

/// Independently derive the number of projection get/put observations.
///
/// For `c` columns, `r` rows and a retained-column mask with `k` bits, each
/// source assignment contributes one unedited view, `6*r*k` cell edits, `r`
/// row deletions and one row insertion. The sum ranges over all masks.
#[must_use]
pub const fn table_fixture_get_put_count_certificate() -> usize {
    let mut total = 0;
    let mut columns = 0;
    while columns <= TABLE_FIXTURE_BOUND {
        let mut rows = 0;
        while rows <= TABLE_FIXTURE_BOUND {
            let assignments = power(7, columns * rows);
            let mut retained = 0;
            while retained < power(2, columns) {
                let kept = retained.count_ones() as usize;
                total += assignments * (2 + rows + 6 * rows * kept);
                retained += 1;
            }
            rows += 1;
        }
        columns += 1;
    }
    total
}

/// Exact number of relation-pair observations.
#[must_use]
pub const fn table_pair_put_count_certificate() -> usize {
    TablePairPutCase::ALL.len()
}

/// Exact number of observations in [`for_each_table_get_put_law_case`].
#[must_use]
pub const fn table_get_put_law_count_certificate() -> usize {
    table_fixture_get_put_count_certificate()
        + writer_count_certificate().2
        + derived_writer_count_certificate()
        + table_pair_put_count_certificate()
        + table_put_refusal_count_certificate()
}

/// One of the two exact finite Delta-fragment fixtures of GF5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gf5Fixture {
    /// `a -> b`, with only `a` visible.
    DropTarget,
    /// `a -visible-> c -hidden-> b`, with `a`, `c` and `visible` visible.
    VisibleThenHidden,
}

impl Gf5Fixture {
    /// The source schema of this exact fixture.
    #[must_use]
    pub fn source_schema(self) -> Schema {
        match self {
            Self::DropTarget => Schema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .edge("a", "b", "rel", Some("b")),
            Self::VisibleThenHidden => Schema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .vertex("c", "record")
                .edge("a", "c", "rel", Some("visible"))
                .edge("c", "b", "rel", Some("hidden")),
        }
    }

    /// The target schema of this exact fixture.
    #[must_use]
    pub fn target_schema(self) -> Schema {
        match self {
            Self::DropTarget => Schema::default().vertex("a", "record"),
            Self::VisibleThenHidden => Schema::default()
                .vertex("a", "record")
                .vertex("c", "record")
                .edge("a", "c", "rel", Some("visible")),
        }
    }

    /// The monic deletion migration of this exact fixture.
    #[must_use]
    pub fn migration(self) -> Migration {
        let target = self.target_schema();
        let surviving_verts = target.vertices.keys().cloned().collect();
        Migration {
            surviving_verts,
            surviving_edges: target.edges,
            ..Migration::default()
        }
    }
}

#[derive(Clone, Copy)]
struct Gf5Edge {
    src: usize,
    tgt: usize,
    name: &'static str,
}

const DROP_TARGET_ANCHORS: [&str; 2] = ["a", "b"];
const DROP_TARGET_EDGES: [Gf5Edge; 1] = [Gf5Edge {
    src: 0,
    tgt: 1,
    name: "b",
}];
const VISIBLE_HIDDEN_ANCHORS: [&str; 3] = ["a", "b", "c"];
const VISIBLE_HIDDEN_EDGES: [Gf5Edge; 2] = [
    Gf5Edge {
        src: 0,
        tgt: 2,
        name: "visible",
    },
    Gf5Edge {
        src: 2,
        tgt: 1,
        name: "hidden",
    },
];

const fn gf5_carrier(fixture: Gf5Fixture) -> (&'static [&'static str], &'static [Gf5Edge]) {
    match fixture {
        Gf5Fixture::DropTarget => (&DROP_TARGET_ANCHORS, &DROP_TARGET_EDGES),
        Gf5Fixture::VisibleThenHidden => (&VISIBLE_HIDDEN_ANCHORS, &VISIBLE_HIDDEN_EDGES),
    }
}

fn gf5_keeps_anchor(fixture: Gf5Fixture, anchor: &str) -> bool {
    match fixture {
        Gf5Fixture::DropTarget => anchor == "a",
        Gf5Fixture::VisibleThenHidden => matches!(anchor, "a" | "c"),
    }
}

fn gf5_keeps_edge(fixture: Gf5Fixture, edge: &SEdge) -> bool {
    fixture == Gf5Fixture::VisibleThenHidden
        && *edge == SEdge::new("a", "c", "rel", Some("visible"))
}

fn for_each_gf5_source_of(fixture: Gf5Fixture, mut visit: impl FnMut(&Graph)) {
    let (anchors, edges) = gf5_carrier(fixture);
    for nodes in 0..=3 {
        for mut assignment in 0..power(anchors.len(), nodes) {
            let mut source = Graph::default();
            let mut node_anchors = Vec::with_capacity(nodes);
            for id in 0..nodes {
                let anchor = anchors[assignment % anchors.len()];
                assignment /= anchors.len();
                node_anchors.push(anchor);
                let id = bounded_id(id);
                source.nodes.insert(id, Node::new(id, anchor));
            }
            let mut candidates = Vec::new();
            for src in 0..nodes {
                for tgt in 0..nodes {
                    for (edge_index, edge) in edges.iter().enumerate() {
                        if anchors[edge.src] == node_anchors[src]
                            && anchors[edge.tgt] == node_anchors[tgt]
                        {
                            candidates.push((src, tgt, edge_index));
                        }
                    }
                }
            }
            for mask in 0..power(2, candidates.len()) {
                if mask.count_ones() > 3 {
                    continue;
                }
                let mut graph = source.clone();
                for (index, &(src, tgt, edge_index)) in candidates.iter().enumerate() {
                    if mask & (1 << index) == 0 {
                        continue;
                    }
                    let edge = edges[edge_index];
                    graph.edges.insert(
                        bounded_id(index),
                        GEdge {
                            src: bounded_id(src),
                            tgt: bounded_id(tgt),
                            label: SEdge::new(
                                anchors[edge.src],
                                anchors[edge.tgt],
                                "rel",
                                Some(edge.name),
                            ),
                            position: None,
                        },
                    );
                }
                visit(&graph);
            }
        }
    }
}

fn gf5_restrict(fixture: Gf5Fixture, source: &Graph) -> Graph {
    let mut view = Graph::default();
    for (id, node) in &source.nodes {
        if gf5_keeps_anchor(fixture, &node.anchor) {
            view.nodes.insert(*id, node.clone());
        }
    }
    for (id, edge) in &source.edges {
        if view.nodes.contains_key(&edge.src)
            && view.nodes.contains_key(&edge.tgt)
            && gf5_keeps_edge(fixture, &edge.label)
        {
            view.edges.insert(*id, edge.clone());
        }
    }
    view
}

fn for_each_subinstance(graph: &Graph, mut visit: impl FnMut(&Graph)) {
    let nodes: Vec<_> = graph.nodes.iter().collect();
    let edges: Vec<_> = graph.edges.iter().collect();
    for node_mask in 0..power(2, nodes.len()) {
        let mut base = Graph::default();
        for (index, (id, node)) in nodes.iter().enumerate() {
            if node_mask & (1 << index) != 0 {
                base.nodes.insert(**id, (*node).clone());
            }
        }
        let eligible: Vec<_> = edges
            .iter()
            .filter(|(_, edge)| {
                base.nodes.contains_key(&edge.src) && base.nodes.contains_key(&edge.tgt)
            })
            .collect();
        for edge_mask in 0..power(2, eligible.len()) {
            let mut subinstance = base.clone();
            for (index, (id, edge)) in eligible.iter().enumerate() {
                if edge_mask & (1 << index) != 0 {
                    subinstance.edges.insert(**id, (*edge).clone());
                }
            }
            visit(&subinstance);
        }
    }
}

/// Stream the exact 133 GF5 source graphs.
pub fn for_each_gf5_source(mut visit: impl FnMut(Gf5Fixture, &Graph)) {
    for fixture in [Gf5Fixture::DropTarget, Gf5Fixture::VisibleThenHidden] {
        for_each_gf5_source_of(fixture, |source| visit(fixture, source));
    }
}

/// Stream every source and deletion-only edited view of the GF5 carrier.
pub fn for_each_gf5_deletion_edit(mut visit: impl FnMut(Gf5Fixture, &Graph, &Graph)) {
    for_each_gf5_source(|fixture, source| {
        let view = gf5_restrict(fixture, source);
        for_each_subinstance(&view, |edited_view| visit(fixture, source, edited_view));
    });
}

/// Stream every GF5 source/deletion-edit/source-subinstance probe.
pub fn for_each_gf5_source_subinstance_probe(
    mut visit: impl FnMut(Gf5Fixture, &Graph, &Graph, &Graph),
) {
    for_each_gf5_deletion_edit(|fixture, source, edited_view| {
        for_each_subinstance(source, |candidate| {
            visit(fixture, source, edited_view, candidate);
        });
    });
}

/// Independent counts for the two exact GF5 fixtures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gf5Counts {
    /// Source graphs, by fixture order.
    pub sources: [usize; 2],
    /// Deletion-only edited views, by fixture order.
    pub deletion_edits: [usize; 2],
    /// Source-subinstance probes, by fixture order.
    pub source_subinstance_probes: [usize; 2],
}

const fn certificate_anchor(fixture: Gf5Fixture, word: usize, node: usize) -> usize {
    let alphabet = match fixture {
        Gf5Fixture::DropTarget => 2,
        Gf5Fixture::VisibleThenHidden => 3,
    };
    (word / power(alphabet, node)) % alphabet
}

fn certificate_edge_at(
    fixture: Gf5Fixture,
    word: usize,
    nodes: usize,
    wanted: usize,
) -> Option<(usize, usize, bool)> {
    let mut found = 0;
    for src in 0..nodes {
        for tgt in 0..nodes {
            let src_anchor = certificate_anchor(fixture, word, src);
            let tgt_anchor = certificate_anchor(fixture, word, tgt);
            let possibilities: &[(usize, usize, bool)] = match fixture {
                Gf5Fixture::DropTarget => &[(0, 1, false)],
                Gf5Fixture::VisibleThenHidden => &[(0, 2, true), (2, 1, false)],
            };
            for &(expected_src, expected_tgt, visible) in possibilities {
                if src_anchor == expected_src && tgt_anchor == expected_tgt {
                    if found == wanted {
                        return Some((src, tgt, visible));
                    }
                    found += 1;
                }
            }
        }
    }
    None
}

fn certificate_edge_count(fixture: Gf5Fixture, word: usize, nodes: usize) -> usize {
    let mut count = 0;
    while certificate_edge_at(fixture, word, nodes, count).is_some() {
        count += 1;
    }
    count
}

fn certificate_subinstance_count(
    fixture: Gf5Fixture,
    word: usize,
    nodes: usize,
    selected_edges: usize,
    allowed_nodes: usize,
    visible_edges_only: bool,
) -> usize {
    let edge_count = certificate_edge_count(fixture, word, nodes);
    let mut count = 0;
    for kept_nodes in 0..power(2, nodes) {
        if kept_nodes & !allowed_nodes != 0 {
            continue;
        }
        let eligible_edges = (0..edge_count)
            .filter(|edge| selected_edges & (1 << edge) != 0)
            .filter(|edge| {
                let Some((src, tgt, visible)) = certificate_edge_at(fixture, word, nodes, *edge)
                else {
                    return false;
                };
                (!visible_edges_only || visible)
                    && kept_nodes & (1 << src) != 0
                    && kept_nodes & (1 << tgt) != 0
            })
            .count();
        count += power(2, eligible_edges);
    }
    count
}

fn gf5_counts_for(fixture: Gf5Fixture) -> (usize, usize, usize) {
    let anchor_count = match fixture {
        Gf5Fixture::DropTarget => 2,
        Gf5Fixture::VisibleThenHidden => 3,
    };
    let mut sources = 0;
    let mut edits = 0;
    let mut probes = 0;
    for nodes in 0..=3 {
        for word in 0..power(anchor_count, nodes) {
            let edge_count = certificate_edge_count(fixture, word, nodes);
            for edge_mask in 0..power(2, edge_count) {
                if edge_mask.count_ones() > 3 {
                    continue;
                }
                sources += 1;
                let visible_nodes = (0..nodes).fold(0, |mask, node| {
                    let anchor = certificate_anchor(fixture, word, node);
                    let visible = match fixture {
                        Gf5Fixture::DropTarget => anchor == 0,
                        Gf5Fixture::VisibleThenHidden => matches!(anchor, 0 | 2),
                    };
                    if visible { mask | (1 << node) } else { mask }
                });
                let source_subinstances = certificate_subinstance_count(
                    fixture,
                    word,
                    nodes,
                    edge_mask,
                    power(2, nodes) - 1,
                    false,
                );
                let deletion_edits = certificate_subinstance_count(
                    fixture,
                    word,
                    nodes,
                    edge_mask,
                    visible_nodes,
                    true,
                );
                edits += deletion_edits;
                probes += deletion_edits * source_subinstances;
            }
        }
    }
    (sources, edits, probes)
}

/// Derive the GF5 counts from anchor assignments, edge-subset masks and the
/// endpoint-closed subinstance formula, without constructing a [`Graph`] or
/// invoking the streamed enumerators.
#[must_use]
pub fn gf5_count_certificate() -> Gf5Counts {
    let first = gf5_counts_for(Gf5Fixture::DropTarget);
    let second = gf5_counts_for(Gf5Fixture::VisibleThenHidden);
    Gf5Counts {
        sources: [first.0, second.0],
        deletion_edits: [first.1, second.1],
        source_subinstance_probes: [first.2, second.2],
    }
}

/// Number of endpoint states in the closed path-contract subcarrier.
///
/// This reuses the two-state source/view alphabet of [`PartialLens`]. It
/// does not select production graph or tree schemas.
pub const PATH_CONTRACT_ENDPOINTS: usize = PARTIAL_LENS_SOURCES;

/// Maximum word length in the closed path-contract subcarrier.
pub const PATH_CONTRACT_MAX_LENGTH: usize = 3;

/// Atomic behaviours in the closed two-state path-contract subcarrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathContractKind {
    /// Preserve the state.
    Identity,
    /// Exchange zero and one.
    Toggle,
    /// Return zero and retain the source state in the complement.
    Clear,
}

impl PathContractKind {
    /// Contract order of the three behaviours.
    pub const ALL: [Self; 3] = [Self::Identity, Self::Toggle, Self::Clear];

    const fn ordinal(self) -> usize {
        match self {
            Self::Identity => 0,
            Self::Toggle => 1,
            Self::Clear => 2,
        }
    }

    /// Execute get over the two-state carrier. Complement `2` is the
    /// complement-free marker already present in the three-state alphabet.
    #[must_use]
    pub const fn get(self, source: usize) -> (usize, usize) {
        match self {
            Self::Identity => (source, 2),
            Self::Toggle => (1 - source, 2),
            Self::Clear => (0, source),
        }
    }

    /// Execute put over the exact three-state complement alphabet.
    #[must_use]
    pub const fn put(self, view: usize, complement: usize) -> Option<usize> {
        if view >= PARTIAL_LENS_VIEWS || complement >= PARTIAL_LENS_COMPLEMENTS {
            return None;
        }
        match self {
            Self::Identity => {
                if complement == 2 {
                    Some(view)
                } else {
                    None
                }
            }
            Self::Toggle => {
                if complement == 2 {
                    Some(1 - view)
                } else {
                    None
                }
            }
            Self::Clear => {
                if complement < 2 {
                    Some(complement)
                } else {
                    None
                }
            }
        }
    }

    /// Exact composition when the result is represented by this closed
    /// atomic behaviour alphabet.
    ///
    /// # Errors
    ///
    /// `Clear` followed by `Toggle` is constant one, which is deliberately
    /// absent from the fixed alphabet. The sequence must therefore remain
    /// explicit.
    pub const fn fuse(self, next: Self) -> Result<Self, PathContractFusionObstruction> {
        match (self, next) {
            (Self::Identity, right) => Ok(right),
            (left, Self::Identity) => Ok(left),
            (Self::Toggle, Self::Toggle) => Ok(Self::Identity),
            (Self::Toggle | Self::Clear, Self::Clear) => Ok(Self::Clear),
            (Self::Clear, Self::Toggle) => {
                Err(PathContractFusionObstruction::ConstantOneOutsideAlphabet)
            }
        }
    }
}

/// Independently computed premise results for one proposed scalar fusion.
///
/// This is deliberately not a complete L.6 certificate. The scalar carrier
/// exercises migration/value compatibility nonvacuously; five structural
/// premises have empty alphabets and the context premise is supplied by the
/// caller. [`path_contract_fusion_coverage`] records that limitation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractFusionPremises {
    values: [bool; 8],
    context_applicable: bool,
}

impl PathContractFusionPremises {
    /// Result for the premise at its L.6 contract ordinal.
    ///
    /// Ordinals are contraction, migration, edge image, empty plans, value
    /// layer, resolver, keep/drop compilation, and context.
    #[must_use]
    pub const fn holds(&self, ordinal: usize) -> bool {
        self.values[ordinal]
    }

    /// Whether the boundary-context premise applies.
    #[must_use]
    pub const fn context_applicable(&self) -> bool {
        self.context_applicable
    }
}

/// Nonvacuity ledger for the scalar fusion-premise subcarrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractFusionCoverage {
    /// Independently evaluated observations by premise ordinal.
    pub observations: [usize; 8],
}

impl PathContractFusionCoverage {
    /// Whether a premise has at least one independently evaluated witness.
    #[must_use]
    pub const fn nonvacuous(&self, ordinal: usize) -> bool {
        self.observations[ordinal] != 0
    }
}

/// Evaluate a proposed fusion independently of [`PathContractKind::fuse`].
///
/// The operation check exhausts both source states and both possible edited
/// views. Structural premises evaluate true only because this explicitly
/// scalar carrier has no graph paths, edges, restrict plans, or fan resolvers;
/// they are not counted as witnesses for the full C11/L.6 claim.
#[must_use]
pub fn path_contract_fusion_premises(
    first: PathContractKind,
    second: PathContractKind,
    fused: PathContractKind,
    context_compiles: Option<bool>,
) -> PathContractFusionPremises {
    // These cardinalities are part of the scalar carrier definition above,
    // not conclusions drawn from the proposed fused atom.
    const CONTRACTED_PATHS: usize = 0;
    const EDGE_IMAGE_DISAGREEMENTS: usize = 0;
    const RESTRICT_PLAN_STEPS: usize = 0;
    const FAN_RESOLVERS: usize = 0;
    const NON_KEEP_DROP_FATES: usize = 0;

    let mut migrations_compose = true;
    let mut value_layers_compatible = true;
    for source in 0..PARTIAL_LENS_SOURCES {
        let (middle, first_complement) = first.get(source);
        let (sequential_view, second_complement) = second.get(middle);
        let (fused_view, fused_complement) = fused.get(source);
        migrations_compose &= sequential_view == fused_view;
        for edited_view in 0..PARTIAL_LENS_VIEWS {
            let sequential_put = second
                .put(edited_view, second_complement)
                .and_then(|middle_source| first.put(middle_source, first_complement));
            let fused_put = fused.put(edited_view, fused_complement);
            value_layers_compatible &= sequential_put == fused_put;
        }
    }
    PathContractFusionPremises {
        values: [
            CONTRACTED_PATHS == 0,
            migrations_compose,
            EDGE_IMAGE_DISAGREEMENTS == 0,
            RESTRICT_PLAN_STEPS == 0,
            value_layers_compatible,
            FAN_RESOLVERS == 0,
            NON_KEEP_DROP_FATES == 0,
            context_compiles.unwrap_or(true),
        ],
        context_applicable: context_compiles.is_some(),
    }
}

/// Exact nonvacuity ledger for [`path_contract_fusion_premises`].
///
/// Only migration composition and value-layer compatibility are exercised on
/// nonempty alphabets. Thus this subcarrier cannot discharge the full C11
/// fusion obligation by itself.
#[must_use]
pub const fn path_contract_fusion_coverage() -> PathContractFusionCoverage {
    PathContractFusionCoverage {
        observations: [
            0,
            PARTIAL_LENS_SOURCES,
            0,
            0,
            PARTIAL_LENS_SOURCES * PARTIAL_LENS_VIEWS,
            0,
            0,
            0,
        ],
    }
}

fn derive_path_contract_fusion(
    condition: FusionCondition,
    inputs: &FusionInputs<'_, PathContractKind>,
) -> bool {
    let context = inputs.context().map(|bytes| bytes == [1]);
    path_contract_fusion_premises(
        *inputs.left().stage().payload(),
        *inputs.right().stage().payload(),
        *inputs.payload(),
        context,
    )
    .holds(condition.ordinal())
}

/// Trusted bounded semantics used to derive scalar fusion evidence.
///
/// The returned value is opaque. Its constructor is unavailable outside the
/// crate, so a caller cannot substitute an all-true premise function.
#[must_use]
pub const fn path_contract_fusion_semantics() -> FusionSemantics<PathContractKind> {
    FusionSemantics::trusted(derive_path_contract_fusion)
}

/// Independent exact domain oracle for [`PathContractKind::put`].
#[must_use]
pub const fn path_contract_put_domain(
    kind: PathContractKind,
    view: usize,
    complement: usize,
) -> bool {
    if view >= PARTIAL_LENS_VIEWS || complement >= PARTIAL_LENS_COMPLEMENTS {
        return false;
    }
    match kind {
        PathContractKind::Identity | PathContractKind::Toggle => complement == 2,
        PathContractKind::Clear => complement < 2,
    }
}

/// Typed reason the closed contract algebra cannot fuse an atomic pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathContractFusionObstruction {
    /// `Clear ; Toggle` is constant one, not an atomic behaviour in this
    /// explicitly fixed subcarrier.
    ConstantOneOutsideAlphabet,
}

/// One typed atomic arrow in the path-contract subcarrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathContractStage {
    /// Source endpoint.
    pub source: usize,
    /// Target endpoint.
    pub target: usize,
    /// Two-state behaviour.
    pub kind: PathContractKind,
}

/// One composable word in the exact path-contract subcarrier.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathContractWord {
    /// Source of the empty word or first stage.
    pub source: usize,
    /// Composable atomic stages.
    pub stages: Vec<PathContractStage>,
}

fn path_contract_byte(value: usize) -> u8 {
    let Ok(byte) = u8::try_from(value) else {
        unreachable!("the closed path-contract carrier fits in one byte")
    };
    byte
}

impl PathContractWord {
    /// Target endpoint after every stage.
    #[must_use]
    pub fn target(&self) -> usize {
        self.stages.last().map_or(self.source, |stage| stage.target)
    }

    /// Ordinal within words of this exact length.
    #[must_use]
    pub fn ordinal(&self) -> usize {
        let radix = PATH_CONTRACT_ENDPOINTS * PathContractKind::ALL.len();
        let mut place = 1;
        let mut digits = 0;
        for stage in &self.stages {
            let digit = stage.kind.ordinal() * PATH_CONTRACT_ENDPOINTS + stage.target;
            digits += digit * place;
            place *= radix;
        }
        self.source * place + digits
    }

    /// Canonical injective byte word used only by the finite enumerator.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.stages.len() * 2);
        bytes.push(path_contract_byte(self.stages.len()));
        bytes.push(path_contract_byte(self.source));
        for stage in &self.stages {
            bytes.push(path_contract_byte(stage.kind.ordinal()));
            bytes.push(path_contract_byte(stage.target));
        }
        bytes
    }
}

/// Count words of one length from `2 * (3 * 2)^length`.
#[must_use]
pub const fn path_contract_count(length: usize) -> usize {
    PATH_CONTRACT_ENDPOINTS
        * power(
            PathContractKind::ALL.len() * PATH_CONTRACT_ENDPOINTS,
            length,
        )
}

/// Independent counts for lengths zero through three and their sum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractCounts {
    /// Counts by word length.
    pub by_length: [usize; PATH_CONTRACT_MAX_LENGTH + 1],
    /// Total number of typed words.
    pub total: usize,
}

/// Derive exact path counts without constructing a path.
#[must_use]
pub const fn path_contract_count_certificate() -> PathContractCounts {
    let mut by_length = [0; PATH_CONTRACT_MAX_LENGTH + 1];
    let mut total = 0;
    let mut length = 0;
    while length <= PATH_CONTRACT_MAX_LENGTH {
        by_length[length] = path_contract_count(length);
        total += by_length[length];
        length += 1;
    }
    PathContractCounts { by_length, total }
}

/// Decode one length-local ordinal into a typed composable word.
#[must_use]
pub fn path_contract_word(length: usize, ordinal: usize) -> Option<PathContractWord> {
    if length > PATH_CONTRACT_MAX_LENGTH || ordinal >= path_contract_count(length) {
        return None;
    }
    let radix = PATH_CONTRACT_ENDPOINTS * PathContractKind::ALL.len();
    let word_radix = power(radix, length);
    let source = ordinal / word_radix;
    let mut code = ordinal % word_radix;
    let mut current = source;
    let mut stages = Vec::with_capacity(length);
    for _ in 0..length {
        let digit = code % radix;
        code /= radix;
        let target = digit % PATH_CONTRACT_ENDPOINTS;
        let kind = PathContractKind::ALL[digit / PATH_CONTRACT_ENDPOINTS];
        stages.push(PathContractStage {
            source: current,
            target,
            kind,
        });
        current = target;
    }
    Some(PathContractWord { source, stages })
}

/// Stream every path-contract word through length three in
/// `(length, ordinal)` order.
pub fn for_each_path_contract_word(mut visit: impl FnMut(usize, usize, PathContractWord)) {
    for length in 0..=PATH_CONTRACT_MAX_LENGTH {
        for ordinal in 0..path_contract_count(length) {
            let Some(word) = path_contract_word(length, ordinal) else {
                unreachable!("ordinal is inside the certified length-local bound")
            };
            visit(length, ordinal, word);
        }
    }
}

/// Five-gate classification of one atomic behaviour and source state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractLawClass {
    /// Truth values in `GetPut`, `PutGet`, Totality, `PutTwice`, Congruence order.
    pub gates: [bool; 5],
}

/// Classify one atom with a one-element Single-Step Basis: the other view.
#[must_use]
pub const fn path_contract_law_class(
    kind: PathContractKind,
    source: usize,
) -> PathContractLawClass {
    let (view, complement) = kind.get(source);
    let edited = 1 - view;
    let get_put = match kind.put(view, complement) {
        Some(restored) => restored == source,
        None => false,
    };
    let restored = kind.put(edited, complement);
    let put_get = match restored {
        Some(next_source) => kind.get(next_source).0 == edited,
        None => false,
    };
    let put_twice = match restored {
        Some(next_source) => {
            let next_complement = kind.get(next_source).1;
            match kind.put(edited, next_complement) {
                Some(repeated) => repeated == next_source,
                None => false,
            }
        }
        None => false,
    };
    PathContractLawClass {
        gates: [
            get_put,
            put_get,
            restored.is_some(),
            put_twice,
            restored.is_some(),
        ],
    }
}

/// Independently derived aggregate for all twelve typed atoms and both
/// source states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractLawCounts {
    /// Typed atoms (`2 * 2 * 3`).
    pub stages: usize,
    /// Atom/source cases.
    pub source_cases: usize,
    /// SSB cases (one alternate view per source case).
    pub basis_cases: usize,
    /// Passing cases by gate order.
    pub gates: [usize; 5],
    /// Cases passing all five gates.
    pub all_five: usize,
}

/// Closed-form law counts, independent of the streamed evaluator.
#[must_use]
pub const fn path_contract_law_count_certificate() -> PathContractLawCounts {
    let stages = PATH_CONTRACT_ENDPOINTS * PATH_CONTRACT_ENDPOINTS * PathContractKind::ALL.len();
    let source_cases = stages * PARTIAL_LENS_SOURCES;
    let strict_put_get =
        PATH_CONTRACT_ENDPOINTS * PATH_CONTRACT_ENDPOINTS * 2 * PARTIAL_LENS_SOURCES;
    PathContractLawCounts {
        stages,
        source_cases,
        basis_cases: source_cases,
        gates: [
            source_cases,
            strict_put_get,
            source_cases,
            source_cases,
            source_cases,
        ],
        all_five: strict_put_get,
    }
}

/// Every finite C11 alphabet now has a binding owner ruling.
pub const C11_SCOPE_COMPLETE: bool = true;

/// Dynamic execution remains on hold until the amended reference and
/// production tests build and reproduce the independent count certificate.
pub const C11_DYNAMIC_VERIFICATION_HOLD: &str =
    "HOLD: build the amended C11 census and reproduce its independent counts before execution";

/// Vertex names of the C11 schema alphabet.
pub const C11_VERTICES: [&str; 2] = ["a", "b"];
/// Kind of every C11 schema vertex.
pub const C11_VERTEX_KIND: &str = "obj";
/// Kind of every C11 schema edge.
pub const C11_EDGE_KIND: &str = "prop";
/// Name of every C11 schema edge.
pub const C11_EDGE_NAME: &str = "e";
/// Most edges of a C11 schema.
pub const C11_MAX_SCHEMA_EDGES: usize = 2;
/// Most nodes of a C11 state.
pub const C11_MAX_NODES: usize = 3;
/// Most edges of a C11 graph state.
pub const C11_MAX_EDGES: usize = 3;
/// Most edges of a C11 fan state.
pub const C11_MAX_FAN_EDGES: usize = 2;
/// The one value a C11 node may hold (or hold none).
pub const C11_VALUE: Value = Value::Int(0);
/// The one field key of the C11 edit alphabet.
pub const C11_FIELD: &str = "f";

/// The C11 schema edge from `src` to `tgt`.
#[must_use]
pub fn c11_edge(src: &str, tgt: &str) -> SEdge {
    SEdge::new(src, tgt, C11_EDGE_KIND, Some(C11_EDGE_NAME))
}

/// The C11 schema on `vertices` with `edges` given as vertex pairs.
#[must_use]
pub fn c11_schema(vertices: &[&str], edges: &[(&str, &str)]) -> Schema {
    let mut schema = Schema::default();
    for vertex in vertices {
        schema = schema.vertex(vertex, C11_VERTEX_KIND);
    }
    for (src, tgt) in edges {
        schema.edges.insert(c11_edge(src, tgt));
    }
    schema
}

/// Every schema of the C11 alphabet: each subset of `{a, b}` as vertices,
/// with each set of at most two of its ordered vertex pairs as edges, in
/// ascending vertex-subset mask and then edge-subset order.
#[must_use]
pub fn sigma2_schemas() -> Vec<Schema> {
    let mut out = Vec::new();
    for mask in 0..power(2, C11_VERTICES.len()) {
        let vertices: Vec<&str> = C11_VERTICES
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, vertex)| *vertex)
            .collect();
        let pairs: Vec<(&str, &str)> = vertices
            .iter()
            .flat_map(|src| vertices.iter().map(move |tgt| (*src, *tgt)))
            .collect();
        for edges in super::subsets(&pairs, C11_MAX_SCHEMA_EDGES) {
            out.push(c11_schema(&vertices, &edges));
        }
    }
    out
}

/// Independent count of [`sigma2_schemas`]:
/// `sum_{V subset of {a,b}} sum_{j <= 2} C(|V|^2, j)`.
#[must_use]
pub fn sigma2_count() -> usize {
    let mut total = 0;
    for size in 0..=C11_VERTICES.len() {
        let subsets =
            usize::try_from(super::choose(2, u64::try_from(size).unwrap_or(0))).unwrap_or(0);
        let pairs = u64::try_from(size * size).unwrap_or(0);
        let edge_sets: u64 = (0..=2).map(|j| super::choose(pairs, j)).sum();
        total += subsets * usize::try_from(edge_sets).unwrap_or(0);
    }
    total
}

fn c11_node(id: usize, anchor: &str, valued: bool) -> Node {
    let node = Node::new(bounded_id(id), anchor);
    if valued {
        node.with_value(C11_VALUE)
    } else {
        node
    }
}

/// The typed edge candidates over nodes anchored at `anchors`: each
/// `(src, tgt, label)` whose label is a schema edge from the source node's
/// anchor to the target node's, in ascending source, target and label.
fn typed_candidates(schema: &Schema, anchors: &[&str]) -> Vec<(usize, usize, SEdge)> {
    let mut out = Vec::new();
    for (src, src_anchor) in anchors.iter().enumerate() {
        for (tgt, tgt_anchor) in anchors.iter().enumerate() {
            for label in &schema.edges {
                if label.src == *src_anchor && label.tgt == *tgt_anchor {
                    out.push((src, tgt, label.clone()));
                }
            }
        }
    }
    out
}

/// Every C11 graph state over `schema`.
///
/// The states have nodes `0..n-1` (`n <= 3`) anchored
/// at the schema's vertices, each with no value or [`C11_VALUE`], and edges
/// `0..m-1` (`m <= 3`), each one of the typed candidates (parallel edges,
/// self-loops and two-cycles included). Labelled and unpruned.
#[must_use]
pub fn c11_graph_states(schema: &Schema) -> Vec<Graph> {
    let vertices: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
    let mut out = Vec::new();
    for nodes in 0..=C11_MAX_NODES {
        for mut anchor_code in 0..power(vertices.len(), nodes) {
            let anchors: Vec<&str> = (0..nodes)
                .map(|_| {
                    let anchor = vertices[anchor_code % vertices.len()];
                    anchor_code /= vertices.len();
                    anchor
                })
                .collect();
            let candidates = typed_candidates(schema, &anchors);
            for values in 0..power(2, nodes) {
                let mut base = Graph::default();
                for (id, anchor) in anchors.iter().enumerate() {
                    base.nodes.insert(
                        bounded_id(id),
                        c11_node(id, anchor, values & (1 << id) != 0),
                    );
                }
                for edges in 0..=C11_MAX_EDGES {
                    for mut code in 0..power(candidates.len(), edges) {
                        let mut graph = base.clone();
                        for id in 0..edges {
                            let (src, tgt, label) = &candidates[code % candidates.len()];
                            code /= candidates.len();
                            graph.edges.insert(
                                bounded_id(id),
                                GEdge {
                                    src: bounded_id(*src),
                                    tgt: bounded_id(*tgt),
                                    label: label.clone(),
                                    position: None,
                                },
                            );
                        }
                        out.push(graph);
                    }
                }
            }
        }
    }
    out
}

/// Independent count of [`c11_graph_states`] by anchor multiplicities:
/// `sum_n sum_{k_a + k_b = n} n! / (k_a! k_b!) * 2^n * sum_{m <= 3} c^m`
/// with `c = sum_{(x, y) in E} k_x k_y`.
#[must_use]
pub fn c11_graph_state_count(schema: &Schema) -> usize {
    let vertices: Vec<&String> = schema.vertices.keys().collect();
    let mut total = 0;
    for nodes in 0..=C11_MAX_NODES {
        for split in multiplicities(vertices.len(), nodes) {
            let arrangements = multinomial(nodes, &split);
            let typed: usize = schema
                .edges
                .iter()
                .map(|edge| {
                    let index = |v: &str| vertices.iter().position(|x| x.as_str() == v);
                    match (index(&edge.src), index(&edge.tgt)) {
                        (Some(s), Some(t)) => split[s] * split[t],
                        _ => 0,
                    }
                })
                .sum();
            let edge_sets: usize = (0..=C11_MAX_EDGES).map(|m| power(typed, m)).sum();
            total += arrangements * power(2, nodes) * edge_sets;
        }
    }
    total
}

/// Every C11 table state over `schema`.
///
/// The states have rows `0..n-1` (`n <= 3`) assigned to schema vertices,
/// each with either no cells or the single cell `f = 0`, and at most three
/// distinct typed relation pairs. Unlike the graph carrier, relation pairs
/// form a set, so a typed `(label, source row, target row)` occurs at most
/// once.
#[must_use]
pub fn c11_table_states(schema: &Schema) -> Vec<Table> {
    let vertices: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
    let mut out = Vec::new();
    for rows in 0..=C11_MAX_NODES {
        for mut anchor_code in 0..power(vertices.len(), rows) {
            let anchors: Vec<&str> = (0..rows)
                .map(|_| {
                    let anchor = vertices[anchor_code % vertices.len()];
                    anchor_code /= vertices.len();
                    anchor
                })
                .collect();
            let candidates = typed_candidates(schema, &anchors);
            for values in 0..power(2, rows) {
                let mut base = Table::default();
                for (id, anchor) in anchors.iter().enumerate() {
                    let cells = if values & (1 << id) != 0 {
                        BTreeMap::from([(C11_FIELD.to_owned(), C11_VALUE)])
                    } else {
                        BTreeMap::new()
                    };
                    base.rows
                        .insert(bounded_id(id), ((*anchor).to_owned(), cells));
                }
                for pairs in super::subsets(&candidates, C11_MAX_EDGES) {
                    let mut table = base.clone();
                    table.pairs.extend(
                        pairs
                            .into_iter()
                            .map(|(src, tgt, edge)| (edge, bounded_id(src), bounded_id(tgt))),
                    );
                    out.push(table);
                }
            }
        }
    }
    out
}

/// Independent count of [`c11_table_states`] by row-anchor multiplicities:
///
/// `sum_n sum_{k_1 + ... + k_v = n} multinomial(n; k) * 2^n`, multiplied by
/// `sum_{m <= 3} choose(c, m)`, where
/// `c = sum_{(x,y) in E} k_x k_y` is the number of distinct typed pairs.
#[must_use]
pub fn c11_table_state_count(schema: &Schema) -> usize {
    let vertices: Vec<&String> = schema.vertices.keys().collect();
    let mut total = 0;
    for rows in 0..=C11_MAX_NODES {
        for split in multiplicities(vertices.len(), rows) {
            let arrangements = multinomial(rows, &split);
            let typed: usize = schema
                .edges
                .iter()
                .map(|edge| {
                    let index = |v: &str| vertices.iter().position(|x| x.as_str() == v);
                    match (index(&edge.src), index(&edge.tgt)) {
                        (Some(src), Some(tgt)) => split[src] * split[tgt],
                        _ => 0,
                    }
                })
                .sum();
            let pair_sets: usize = (0..=C11_MAX_EDGES.min(typed))
                .map(|pairs| choose_usize(typed, pairs))
                .sum();
            total += arrangements * power(2, rows) * pair_sets;
        }
    }
    total
}

fn choose_usize(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    (0..k).fold(1, |coefficient, offset| {
        coefficient * (n - offset) / (offset + 1)
    })
}

/// Every vector of `parts` naturals summing to `total`.
fn multiplicities(parts: usize, total: usize) -> Vec<Vec<usize>> {
    if parts == 0 {
        return if total == 0 {
            vec![Vec::new()]
        } else {
            Vec::new()
        };
    }
    let mut out = Vec::new();
    for first in 0..=total {
        for mut rest in multiplicities(parts - 1, total - first) {
            rest.insert(0, first);
            out.push(rest);
        }
    }
    out
}

const fn factorial(n: usize) -> usize {
    let mut result = 1;
    let mut k = 2;
    while k <= n {
        result *= k;
        k += 1;
    }
    result
}

fn multinomial(total: usize, split: &[usize]) -> usize {
    split
        .iter()
        .fold(factorial(total), |acc, part| acc / factorial(*part))
}

/// Every C11 tree state over `schema`.
///
/// The states are the ordered rooted trees on nodes
/// `0..n-1` (`1 <= n <= 3`) with every root, every sibling order, every
/// anchoring in the schema's vertices, every value choice and, for each arc,
/// every schema edge from the parent's anchor to the child's. Arcs are listed
/// by ascending parent and, within one parent, in sibling order. No fans.
#[must_use]
pub fn c11_tree_states(schema: &Schema) -> Vec<Tree> {
    let vertices: Vec<&str> = schema.vertices.keys().map(String::as_str).collect();
    let mut out = Vec::new();
    for nodes in 1..=C11_MAX_NODES {
        for shape in plane_trees(nodes) {
            for mut anchor_code in 0..power(vertices.len(), nodes) {
                let anchors: Vec<&str> = (0..nodes)
                    .map(|_| {
                        let anchor = vertices[anchor_code % vertices.len()];
                        anchor_code /= vertices.len();
                        anchor
                    })
                    .collect();
                let labels: Vec<Vec<SEdge>> = shape
                    .arcs
                    .iter()
                    .map(|(parent, child)| {
                        schema
                            .edges
                            .iter()
                            .filter(|edge| {
                                edge.src == anchors[*parent] && edge.tgt == anchors[*child]
                            })
                            .cloned()
                            .collect()
                    })
                    .collect();
                let label_choices: usize = labels.iter().map(Vec::len).product();
                for mut label_code in 0..label_choices {
                    let mut arcs = Vec::new();
                    for ((parent, child), choices) in shape.arcs.iter().zip(&labels) {
                        arcs.push((
                            bounded_id(*parent),
                            bounded_id(*child),
                            choices[label_code % choices.len()].clone(),
                        ));
                        label_code /= choices.len();
                    }
                    for values in 0..power(2, nodes) {
                        let mut tree = Tree {
                            nodes: BTreeMap::new(),
                            arcs: arcs.clone(),
                            fans: Vec::new(),
                            root: bounded_id(shape.root),
                        };
                        for (id, anchor) in anchors.iter().enumerate() {
                            tree.nodes.insert(
                                bounded_id(id),
                                c11_node(id, anchor, values & (1 << id) != 0),
                            );
                        }
                        out.push(tree);
                    }
                }
            }
        }
    }
    out
}

/// A labelled plane rooted tree: a root and its arcs, listed by ascending
/// parent and, within one parent, in sibling order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PlaneTree {
    root: usize,
    arcs: Vec<(usize, usize)>,
}

/// Every labelled plane rooted tree on `0..n-1`.
fn plane_trees(nodes: usize) -> Vec<PlaneTree> {
    let mut out = Vec::new();
    for root in 0..nodes {
        let others: Vec<usize> = (0..nodes).filter(|node| *node != root).collect();
        for mut code in 0..power(nodes, others.len()) {
            let parent: BTreeMap<usize, usize> = others
                .iter()
                .map(|child| {
                    let p = code % nodes;
                    code /= nodes;
                    (*child, p)
                })
                .collect();
            let reaches_root = |start: usize| {
                let mut at = start;
                for _ in 0..nodes {
                    if at == root {
                        return true;
                    }
                    at = parent[&at];
                }
                at == root
            };
            if others
                .iter()
                .any(|child| parent[child] == *child || !reaches_root(*child))
            {
                continue;
            }
            let children: Vec<Vec<usize>> = (0..nodes)
                .map(|p| others.iter().copied().filter(|c| parent[c] == p).collect())
                .collect();
            for orders in sibling_orders(&children) {
                let arcs = orders
                    .iter()
                    .enumerate()
                    .flat_map(|(p, kids)| kids.iter().map(move |c| (p, *c)))
                    .collect();
                out.push(PlaneTree { root, arcs });
            }
        }
    }
    out
}

/// Every choice of a permutation of each parent's children.
fn sibling_orders(children: &[Vec<usize>]) -> Vec<Vec<Vec<usize>>> {
    let mut out = vec![Vec::new()];
    for kids in children {
        let mut next = Vec::new();
        for prefix in &out {
            for order in permutations(kids) {
                let mut extended: Vec<Vec<usize>> = prefix.clone();
                extended.push(order);
                next.push(extended);
            }
        }
        out = next;
    }
    out
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(index);
        for mut tail in permutations(&rest) {
            tail.insert(0, *item);
            out.push(tail);
        }
    }
    out
}

/// Independent count of [`c11_tree_states`] by tree shape.
///
/// With `w(x, y)`
/// the number of schema edges from `x` to `y`:
/// `2 |V| + 2 * 4 sum w(x,y) + 8 (6 sum w(x,y) w(y,z) + 6 sum w(x,y) w(x,z))`
/// (one node; root and child; a path; a root with two ordered children,
/// over every labelling of the nodes).
#[must_use]
pub fn c11_tree_state_count(schema: &Schema) -> usize {
    let vertices: Vec<&String> = schema.vertices.keys().collect();
    let w = |x: &str, y: &str| {
        schema
            .edges
            .iter()
            .filter(|edge| edge.src == x && edge.tgt == y)
            .count()
    };
    let mut pairs = 0;
    let mut paths = 0;
    let mut cherries = 0;
    for x in &vertices {
        for y in &vertices {
            pairs += w(x, y);
            for z in &vertices {
                paths += w(x, y) * w(y, z);
                cherries += w(x, y) * w(x, z);
            }
        }
    }
    2 * vertices.len() + 2 * 4 * pairs + 8 * (6 * paths + 6 * cherries)
}

/// The fan schema: vertices `a`, `b` and the hyper-edge `h` of kind `fan`
/// with signature `{p -> a, l -> b}` and parent label `p`.
#[must_use]
pub fn c11_fan_schema() -> Schema {
    let mut schema = c11_schema(&["a", "b"], &[]);
    schema.hyper_edges.insert(
        "h".to_owned(),
        crate::schema::HyperEdge {
            kind: "fan".to_owned(),
            signature: BTreeMap::from([
                ("l".to_owned(), "b".to_owned()),
                ("p".to_owned(), "a".to_owned()),
            ]),
            parent_label: "p".to_owned(),
        },
    );
    schema
}

/// The two reserved fan labels of the fan states: the parent edge of `h`
/// into `a`, and the child edge `l` of `h` into `b`.
#[must_use]
pub fn c11_fan_labels() -> [SEdge; 2] {
    [
        SEdge::new("h", "a", crate::restrict::HYPER_PARENT, Some("p")),
        SEdge::new("h", "b", crate::restrict::HYPER_LABEL, Some("l")),
    ]
}

/// Every C11 fan state.
///
/// The states have nodes `0..n-1` (`n <= 3`) anchored at `a`, `b` or
/// `h` (an `h` node has the hyper-edge shape), no values, and edges
/// `0..m-1` (`m <= 2`) with any endpoints and either fan label: every
/// well-formed one-child fan and every malformed fan of the bound.
#[must_use]
pub fn c11_fan_states() -> Vec<Graph> {
    let anchors = ["a", "b", "h"];
    let labels = c11_fan_labels();
    let mut out = Vec::new();
    for nodes in 0..=C11_MAX_NODES {
        for mut code in 0..power(anchors.len(), nodes) {
            let mut base = Graph::default();
            for id in 0..nodes {
                let anchor = anchors[code % anchors.len()];
                code /= anchors.len();
                let mut node = Node::new(bounded_id(id), anchor);
                if anchor == "h" {
                    node.shape = NodeShape::HyperEdge;
                }
                base.nodes.insert(bounded_id(id), node);
            }
            let choices = nodes * nodes * labels.len();
            for edges in 0..=C11_MAX_FAN_EDGES {
                for mut edge_code in 0..power(choices, edges) {
                    let mut graph = base.clone();
                    for id in 0..edges {
                        let choice = edge_code % choices;
                        edge_code /= choices;
                        graph.edges.insert(
                            bounded_id(id),
                            GEdge {
                                src: bounded_id(choice / (nodes * labels.len())),
                                tgt: bounded_id((choice / labels.len()) % nodes),
                                label: labels[choice % labels.len()].clone(),
                                position: None,
                            },
                        );
                    }
                    out.push(graph);
                }
            }
        }
    }
    out
}

/// Independent count of [`c11_fan_states`]:
/// `sum_{n <= 3} 3^n sum_{m <= 2} (2 n^2)^m`.
#[must_use]
pub const fn c11_fan_state_count() -> usize {
    let mut total = 0;
    let mut n = 0;
    while n <= C11_MAX_NODES {
        let mut edge_sets = 0;
        let mut m = 0;
        while m <= C11_MAX_FAN_EDGES {
            edge_sets += power(2 * n * n, m);
            m += 1;
        }
        total += power(3, n) * edge_sets;
        n += 1;
    }
    total
}

/// The value layer of a C11 stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C11Layer {
    /// No survival predicate and no transform.
    None,
    /// The survival predicate `false` on vertex `a`.
    SurvivalFalse,
    /// The transform adding field [`C11_FIELD`] with [`C11_VALUE`] on `a`.
    AddField,
    /// The transform dropping field [`C11_FIELD`] on `a`.
    DropField,
    /// The invertible transform reading [`C11_FIELD`] on `a`.
    ReadField,
    /// The op-term assignment computing [`C11_FIELD`] on `a`.
    OpTerm,
}

/// One atomic stage of the C11 stage alphabet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C11Stage {
    /// Stable stage name.
    pub name: String,
    /// Source schema.
    pub source: Schema,
    /// Target schema.
    pub target: Schema,
    /// Migration.
    pub migration: Migration,
    /// Value layer.
    pub layer: C11Layer,
}

fn identity_migration(schema: &Schema) -> Migration {
    Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        ..Migration::default()
    }
}

fn stage(
    name: &str,
    source: Schema,
    target: Schema,
    migration: Migration,
    layer: C11Layer,
) -> C11Stage {
    C11Stage {
        name: name.to_owned(),
        source,
        target,
        migration,
        layer,
    }
}

/// The 31 atomic stages of the C11 stage alphabet.
///
/// These are the identity on every schema of [`sigma2_schemas`] (16) and on
/// [`c11_fan_schema`], then `P`, `D`, `R`, `W`, `C`, `X`, `H`, `Q`, `T`,
/// `DropF`, `ReadF`, `Op`, `Ext` and `B`.
#[must_use]
pub fn c11_stages() -> Vec<C11Stage> {
    let mut out: Vec<C11Stage> = sigma2_schemas()
        .into_iter()
        .enumerate()
        .map(|(index, schema)| {
            let migration = identity_migration(&schema);
            stage(
                &format!("Id{index}"),
                schema.clone(),
                schema,
                migration,
                C11Layer::None,
            )
        })
        .collect();
    let fan = c11_fan_schema();
    out.push(stage(
        "Id_h",
        fan.clone(),
        fan.clone(),
        identity_migration(&fan),
        C11Layer::None,
    ));
    out.extend(c11_structural_stages());
    let ab = c11_schema(&["a", "b"], &[("a", "b")]);
    let a = c11_schema(&["a"], &[]);
    let bare = c11_schema(&["a", "b"], &[]);
    let mut retarget = identity_migration(&fan);
    retarget.hyper_resolver.insert(
        ("h".to_owned(), ["l".to_owned()].into()),
        ("h".to_owned(), [("l".to_owned(), "l".to_owned())].into()),
    );
    out.push(stage("H", fan.clone(), fan, retarget, C11Layer::None));
    let mut predicate = identity_migration(&a);
    predicate.conditional.insert("a".to_owned());
    out.push(stage(
        "Q",
        a.clone(),
        a.clone(),
        predicate,
        C11Layer::SurvivalFalse,
    ));
    let mut transform = identity_migration(&a);
    transform.transformed.insert("a".to_owned());
    out.push(stage(
        "T",
        a.clone(),
        a.clone(),
        transform,
        C11Layer::AddField,
    ));
    let mut drop_field = identity_migration(&a);
    drop_field.transformed.insert("a".to_owned());
    out.push(stage(
        "DropF",
        a.clone(),
        a.clone(),
        drop_field,
        C11Layer::DropField,
    ));
    let mut read_field = identity_migration(&a);
    read_field.transformed.insert("a".to_owned());
    out.push(stage(
        "ReadF",
        a.clone(),
        a.clone(),
        read_field,
        C11Layer::ReadField,
    ));
    let mut op_term = identity_migration(&a);
    op_term.transformed.insert("a".to_owned());
    out.push(stage("Op", a.clone(), a, op_term, C11Layer::OpTerm));
    let bare_identity = identity_migration(&bare);
    out.push(stage(
        "Ext",
        bare,
        ab.clone(),
        bare_identity,
        C11Layer::None,
    ));
    let mut incoherent = identity_migration(&ab);
    incoherent
        .edge_remap
        .insert(c11_edge("a", "b"), c11_edge("b", "a"));
    out.push(stage("B", ab.clone(), ab, incoherent, C11Layer::None));
    out
}

/// One schema-level case in the L.19 protolens certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtolensSchemaCase {
    /// One of the four elementary protolenses at one of the sixteen
    /// [`sigma2_schemas`].
    Elementary {
        /// Index in the elementary protolens alphabet.
        protolens: usize,
        /// Index in [`sigma2_schemas`].
        schema: usize,
    },
    /// An ordered two-clause false precondition whose transforms must not run.
    FalsePrecondition,
    /// A reachable 65,537-hop plan-compilation refusal.
    ExpansionTooLong,
}

/// Stream the 66 schema-level cases of the L.19 protolens certificate.
pub fn for_each_protolens_schema_case(mut visit: impl FnMut(ProtolensSchemaCase)) {
    for protolens in 0..4 {
        for schema in 0..sigma2_count() {
            visit(ProtolensSchemaCase::Elementary { protolens, schema });
        }
    }
    visit(ProtolensSchemaCase::FalsePrecondition);
    visit(ProtolensSchemaCase::ExpansionTooLong);
}

/// Independent arithmetic count of [`for_each_protolens_schema_case`].
#[must_use]
pub const fn protolens_schema_case_count_certificate() -> usize {
    4 * 16 + 2
}

/// How one compatible complement triple is bound to its lens.
///
/// The reference carrier distinguishes the two admissible provenance
/// regimes. Production constructs the corresponding fingerprint or envelope;
/// the reference model only chooses the row of the identity-composition table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplementMonoidBinding {
    /// All three complements carry the same legacy fingerprint.
    Legacy,
    /// All three complements carry the same verified identity envelope.
    Verified,
}

/// One ordered triple in the finite compatible-complement carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComplementMonoidTriple {
    /// Provenance regime shared by the three operands.
    pub binding: ComplementMonoidBinding,
    /// Three-bit mask selecting canonical resource bundles in the left operand.
    pub left: u8,
    /// Three-bit mask selecting canonical resource bundles in the middle operand.
    pub middle: u8,
    /// Three-bit mask selecting canonical resource bundles in the right operand.
    pub right: u8,
    /// Reference union of the three masks.
    pub union: u8,
}

/// One coordinate on which complement composition is deliberately undefined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplementMonoidRefusal {
    /// Distinct nodes occupy the same dropped-node id.
    DroppedNodes,
    /// Distinct edge choices occupy the same contraction pair.
    ContractionChoices,
    /// Distinct parents are recorded for the same node.
    OriginalParent,
    /// Distinct field snapshots are recorded for the same node.
    OriginalExtraFields,
    /// Distinct edge labels are recorded for the same arc.
    ArcEdges,
    /// Distinct value snapshots are recorded for the same node.
    OriginalValues,
    /// Distinct surviving ancestors are recorded for the same node.
    ContractedInto,
    /// Distinct source anchors are recorded for the same node.
    OriginalAnchors,
    /// One synthesized id is assigned two distinct structural keys.
    SynthesizedKeyById,
    /// One structural key is assigned to two distinct synthesized ids.
    SynthesizedIdByKey,
    /// Two nonempty global arc orders disagree.
    ArcOrder,
    /// A data-bearing complement carries no provenance.
    UnboundData,
    /// Two legacy complements carry different fingerprints.
    LegacyFingerprint,
    /// A verified complement is paired with a legacy complement.
    MixedProvenance,
    /// Two verified complements carry different envelopes.
    EnvelopeMismatch,
}

impl ComplementMonoidRefusal {
    /// Every named off-domain coordinate in declaration order.
    pub const ALL: [Self; 15] = [
        Self::DroppedNodes,
        Self::ContractionChoices,
        Self::OriginalParent,
        Self::OriginalExtraFields,
        Self::ArcEdges,
        Self::OriginalValues,
        Self::ContractedInto,
        Self::OriginalAnchors,
        Self::SynthesizedKeyById,
        Self::SynthesizedIdByKey,
        Self::ArcOrder,
        Self::UnboundData,
        Self::LegacyFingerprint,
        Self::MixedProvenance,
        Self::EnvelopeMismatch,
    ];
}

/// One case in the complete finite complement-monoid certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplementMonoidCase {
    /// A triple from the two provenance regimes and the three-atom powerset.
    Compatible(ComplementMonoidTriple),
    /// A named witness outside the partial operation's domain.
    Refusal(ComplementMonoidRefusal),
}

/// Stream every ordered compatible triple, then every named refusal witness.
///
/// The compatible carrier contains two provenance regimes and three operands,
/// each ranging over all eight subsets of three canonical resource bundles:
/// `2 * 8^3 = 1024` triples. The union oracle is bitwise set union and is
/// independent of the production merge implementation.
pub fn for_each_complement_monoid_case(mut visit: impl FnMut(ComplementMonoidCase)) {
    for binding in [
        ComplementMonoidBinding::Legacy,
        ComplementMonoidBinding::Verified,
    ] {
        for left in 0_u8..8 {
            for middle in 0_u8..8 {
                for right in 0_u8..8 {
                    visit(ComplementMonoidCase::Compatible(ComplementMonoidTriple {
                        binding,
                        left,
                        middle,
                        right,
                        union: left | middle | right,
                    }));
                }
            }
        }
    }
    for refusal in ComplementMonoidRefusal::ALL {
        visit(ComplementMonoidCase::Refusal(refusal));
    }
}

/// Independent arithmetic count of [`for_each_complement_monoid_case`].
#[must_use]
pub const fn complement_monoid_count_certificate() -> usize {
    2 * 8 * 8 * 8 + ComplementMonoidRefusal::ALL.len()
}

/// One shape in the independent law-equivalence carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawEquivalenceShape {
    /// Graph instances with explicit node and edge presentation ids.
    Graph,
    /// Tree instances whose arc ids arise from their graph embedding.
    Tree,
    /// Table instances whose row and foreign-key ids arise from their graph embedding.
    Table,
}

impl LawEquivalenceShape {
    /// Every certified shape in declaration order.
    pub const ALL: [Self; 3] = [Self::Graph, Self::Tree, Self::Table];
}

/// One way a synthesized index can fail to be a partial bijection into an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawEquivalenceIncoherence {
    /// Two present nodes carry the same synthesis key.
    DuplicateNodeKey,
    /// Two present edges carry the same synthesis key.
    DuplicateEdgeKey,
    /// The index names a node absent from the instance.
    MissingNode,
    /// The index names an edge absent from the instance.
    MissingEdge,
}

impl LawEquivalenceIncoherence {
    /// Every incoherent-index family rejected by the production comparison.
    pub const ALL: [Self; 4] = [
        Self::DuplicateNodeKey,
        Self::DuplicateEdgeKey,
        Self::MissingNode,
        Self::MissingEdge,
    ];
}

/// One case in the independent normalization certificate for law equivalence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawEquivalenceOracleCase {
    /// Compare two coherent presentations. `changed` changes one semantic cell,
    /// not a presentation id, so the reference answer is `!changed`.
    Pair {
        /// Shape under comparison.
        shape: LawEquivalenceShape,
        /// First of three pairwise-disjoint presentation-id assignments.
        left: u8,
        /// Second presentation-id assignment.
        right: u8,
        /// Whether the right instance has a changed semantic cell.
        changed: bool,
    },
    /// Check reflexivity, symmetry and both transitive bracketings on three
    /// coherent presentations of one normalized value.
    Triple {
        /// Shape under comparison.
        shape: LawEquivalenceShape,
        /// First presentation-id assignment.
        first: u8,
        /// Second presentation-id assignment.
        second: u8,
        /// Third presentation-id assignment.
        third: u8,
    },
    /// Check one complete family of incoherent synthesized metadata.
    Incoherent {
        /// Shape under comparison.
        shape: LawEquivalenceShape,
        /// Failed partial-bijection condition.
        family: LawEquivalenceIncoherence,
    },
}

/// Stream the complete finite carrier for the independent normalization oracle.
///
/// Each shape has `3 * 3 * 2 = 18` ordered coherent pairs, `3^3 = 27`
/// coherent triples and four incoherent-index families. Thus the carrier has
/// `3 * (18 + 27 + 4) = 147` cases.
pub fn for_each_law_equivalence_oracle_case(mut visit: impl FnMut(LawEquivalenceOracleCase)) {
    for shape in LawEquivalenceShape::ALL {
        for left in 0..3 {
            for right in 0..3 {
                for changed in [false, true] {
                    visit(LawEquivalenceOracleCase::Pair {
                        shape,
                        left,
                        right,
                        changed,
                    });
                }
            }
        }
        for first in 0..3 {
            for second in 0..3 {
                for third in 0..3 {
                    visit(LawEquivalenceOracleCase::Triple {
                        shape,
                        first,
                        second,
                        third,
                    });
                }
            }
        }
        for family in LawEquivalenceIncoherence::ALL {
            visit(LawEquivalenceOracleCase::Incoherent { shape, family });
        }
    }
}

/// Independent arithmetic count of [`for_each_law_equivalence_oracle_case`].
#[must_use]
pub const fn law_equivalence_oracle_case_count_certificate() -> usize {
    LawEquivalenceShape::ALL.len() * (3 * 3 * 2 + 3 * 3 * 3 + LawEquivalenceIncoherence::ALL.len())
}

/// One boundary checked by the graph put-contract certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphPutContractCase {
    /// A noncompiling migration must be reported as the first domain clause.
    PlanCompilation,
    /// An empty taken set returns the floor, including the last usable id.
    EmptyAtFloor(u32),
    /// A taken id below the floor does not advance the allocator.
    TakenBelowFloor {
        /// Recorded allocation floor.
        floor: u32,
        /// Occupied presentation id strictly below the floor.
        taken: u32,
    },
    /// A taken id at or above the floor advances to its successor.
    TakenAtOrAboveFloor {
        /// Recorded allocation floor.
        floor: u32,
        /// Occupied presentation id at or above the floor.
        taken: u32,
    },
    /// No fresh presentation id exists when the required successor is reserved.
    ExhaustedByTaken(u32),
}

/// Stream all named plan-compilation and fresh-id boundary cases for L.3.
pub fn for_each_graph_put_contract_case(mut visit: impl FnMut(GraphPutContractCase)) {
    visit(GraphPutContractCase::PlanCompilation);
    for floor in [0, 1, u32::MAX - 1] {
        visit(GraphPutContractCase::EmptyAtFloor(floor));
    }
    visit(GraphPutContractCase::TakenBelowFloor { floor: 7, taken: 3 });
    visit(GraphPutContractCase::TakenAtOrAboveFloor { floor: 7, taken: 7 });
    visit(GraphPutContractCase::TakenAtOrAboveFloor {
        floor: 7,
        taken: 11,
    });
    for taken in [u32::MAX - 1, u32::MAX] {
        visit(GraphPutContractCase::ExhaustedByTaken(taken));
    }
}

/// Independent arithmetic count of [`for_each_graph_put_contract_case`].
#[must_use]
pub const fn graph_put_contract_case_count_certificate() -> usize {
    1 + 3 + 1 + 2 + 2
}

/// A validation boundary in the graph/tree put-domain square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeValidationCase {
    /// Deleting a required child reports the missing cell.
    RequiredDeletion {
        /// Node that loses the required cell.
        elem: u32,
        /// Required field name.
        field: &'static str,
    },
    /// Editing one key value to equal another reports both colliding nodes.
    DuplicateKeyAfterEdit {
        /// Vertex whose key is duplicated.
        table: &'static str,
        /// Single key-part field name.
        field: &'static str,
        /// First source key value and the value copied by the edit.
        repeated_value: i64,
        /// Distinct second source key value before the edit.
        replaced_value: i64,
        /// Colliding node ids, in report order.
        elems: [u32; 2],
    },
    /// A native tree view whose fan names an absent child reports a malformed fan.
    MalformedFan {
        /// Parent node named by the typed refusal.
        node: u32,
        /// Fan label whose child is absent.
        missing_label: &'static str,
    },
}

/// Stream every independent validation witness for L.5 and L.9.
pub fn for_each_tree_validation_case(mut visit: impl FnMut(TreeValidationCase)) {
    visit(TreeValidationCase::RequiredDeletion {
        elem: 0,
        field: "b",
    });
    visit(TreeValidationCase::DuplicateKeyAfterEdit {
        table: "a",
        field: "b",
        repeated_value: 1,
        replaced_value: 2,
        elems: [1, 2],
    });
    visit(TreeValidationCase::MalformedFan {
        node: 0,
        missing_label: "x",
    });
}

/// Independent arithmetic count of [`for_each_tree_validation_case`].
#[must_use]
pub const fn tree_validation_case_count_certificate() -> usize {
    3
}

/// Exact heterogeneous scope of the L.10 construction certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LensConstructionCountCertificate {
    /// Atomic C11 stages classified by both graph and tree classifiers.
    pub stage_classifications: usize,
    /// Graph stages classified as complement-free isomorphisms.
    pub graph_positive_stages: usize,
    /// Successful graph complement-free restorations.
    pub graph_restorations: usize,
    /// Tree stages classified as complement-free isomorphisms.
    pub tree_positive_stages: usize,
    /// Positive nonfan tree stages with a nonempty finite state carrier.
    pub tree_state_bearing_stages: usize,
    /// Successful tree complement-free restorations.
    pub tree_restorations: usize,
    /// Distinct table projection lenses.
    pub table_projection_lenses: usize,
    /// Table projection source/lens cases.
    pub table_projection_cases: usize,
    /// Distinct table writer lenses.
    pub table_writer_lenses: usize,
    /// Table writer source/lens cases.
    pub table_writer_cases: usize,
    /// Focused symmetric-span acceptance and refusal assertions.
    pub span_assertions: usize,
}

/// Independently pinned arithmetic metadata for the L.10 executable umbrella.
#[must_use]
pub const fn lens_construction_count_certificate() -> LensConstructionCountCertificate {
    LensConstructionCountCertificate {
        stage_classifications: 31,
        graph_positive_stages: 18,
        graph_restorations: 117_004,
        tree_positive_stages: 18,
        tree_state_bearing_stages: 17,
        tree_restorations: 2_082,
        table_projection_lenses: 7,
        table_projection_cases: 9_921,
        table_writer_lenses: 9,
        table_writer_cases: 81,
        span_assertions: 4,
    }
}

/// One base case in the cross-shape law-equivalence certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LensEquivalenceCase {
    /// One C11 stage applied to one bounded graph source.
    Graph {
        /// Index in [`c11_stages`].
        stage: usize,
        /// Bounded graph over the stage's source schema.
        source: Graph,
    },
    /// One nonfan C11 stage applied to one bounded tree source.
    Tree {
        /// Index in [`c11_stages`].
        stage: usize,
        /// Bounded nonfan tree over the stage's source schema.
        source: Tree,
    },
    /// One bounded table projection/source fixture.
    Table(TableFixture),
}

/// Stream the graph, common-domain tree, and table bases used to certify
/// observational equivalence across all three lens shapes.
pub fn for_each_lens_equivalence_case(mut visit: impl FnMut(LensEquivalenceCase)) {
    let stages = c11_stages();
    for (stage, descriptor) in stages.iter().enumerate() {
        let graph_sources = if descriptor.source == c11_fan_schema() {
            c11_fan_states()
        } else {
            c11_graph_states(&descriptor.source)
        };
        for source in graph_sources {
            visit(LensEquivalenceCase::Graph { stage, source });
        }
        if descriptor.source != c11_fan_schema() {
            for source in c11_tree_states(&descriptor.source) {
                visit(LensEquivalenceCase::Tree { stage, source });
            }
        }
    }
    for_each_table_fixture(|fixture| visit(LensEquivalenceCase::Table(fixture)));
}

/// The six structural stages `P`, `D`, `R`, `W`, `C` and `X` of
/// [`c11_stages`].
fn c11_structural_stages() -> Vec<C11Stage> {
    let mut stages = Vec::new();
    let ab = c11_schema(&["a", "b"], &[("a", "b")]);
    let a = c11_schema(&["a"], &[]);
    stages.push(stage(
        "P",
        ab.clone(),
        a.clone(),
        Migration {
            surviving_verts: ["a".to_owned()].into(),
            ..Migration::default()
        },
        C11Layer::None,
    ));
    let bare = c11_schema(&["a", "b"], &[]);
    stages.push(stage(
        "D",
        ab.clone(),
        bare,
        Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            ..Migration::default()
        },
        C11Layer::None,
    ));
    stages.push(stage(
        "R",
        a,
        c11_schema(&["b"], &[]),
        Migration {
            surviving_verts: ["b".to_owned()].into(),
            vertex_remap: [("a".to_owned(), "b".to_owned())].into(),
            ..Migration::default()
        },
        C11Layer::None,
    ));
    stages.push(stage(
        "W",
        ab.clone(),
        c11_schema(&["a", "b"], &[("b", "a")]),
        Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            surviving_edges: [c11_edge("b", "a")].into(),
            vertex_remap: [
                ("a".to_owned(), "b".to_owned()),
                ("b".to_owned(), "a".to_owned()),
            ]
            .into(),
            edge_remap: [(c11_edge("a", "b"), c11_edge("b", "a"))].into(),
            ..Migration::default()
        },
        C11Layer::None,
    ));
    stages.push(stage(
        "C",
        c11_schema(&["a", "b"], &[("a", "b"), ("b", "a")]),
        c11_schema(&["a"], &[("a", "a")]),
        Migration {
            surviving_verts: ["a".to_owned()].into(),
            resolver: [(("a".to_owned(), "a".to_owned()), c11_edge("a", "a"))].into(),
            ..Migration::default()
        },
        C11Layer::None,
    ));
    stages.push(stage(
        "X",
        ab,
        c11_schema(&["a", "b"], &[("a", "b"), ("b", "b")]),
        Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            surviving_edges: [c11_edge("a", "b")].into(),
            expansion_path: [(("a".to_owned(), "b".to_owned()), vec!["b".to_owned()])].into(),
            ..Migration::default()
        },
        C11Layer::None,
    ));
    stages
}

/// The survival-predicate value layer of stage `Q`: every node anchored at
/// a vertex with a predicate fails it.
#[derive(Clone, Copy, Debug, Default)]
pub struct C11SurvivalFalse;

impl crate::restrict::ValueLayer for C11SurvivalFalse {
    fn canonical_semantics(&self) -> crate::restrict::ValueLayerSemantics {
        crate::restrict::ValueLayerSemantics {
            domain: "panproto-reference/c11-survival-false",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, crate::restrict::PredicateRefusal> {
        Ok(false)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::LayerRefusal,
    > {
        Ok((node.fields.clone(), node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<crate::value::Presence>>,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::PutFieldRefusal,
    > {
        Ok((
            original_fields
                .cloned()
                .unwrap_or_else(|| view.fields.clone()),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

/// The transform value layer of stage `T`.
///
/// Get defaults an absent [`C11_FIELD`] to [`C11_VALUE`]. Put requires an inserted default to remain
/// unedited and removes it, while a field already present in the source
/// remains an ordinary editable coordinate.
#[derive(Clone, Copy, Debug, Default)]
pub struct C11AddField;

impl crate::restrict::ValueLayer for C11AddField {
    fn canonical_semantics(&self) -> crate::restrict::ValueLayerSemantics {
        crate::restrict::ValueLayerSemantics {
            domain: "panproto-reference/c11-add-field",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, crate::restrict::PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::LayerRefusal,
    > {
        let mut fields = node.fields.clone();
        fields.entry(C11_FIELD.to_owned()).or_insert(C11_VALUE);
        Ok((fields, node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<crate::value::Presence>>,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::PutFieldRefusal,
    > {
        if let Some(original_fields) = original_fields {
            if !original_fields.contains_key(C11_FIELD)
                && view.fields.get(C11_FIELD) != Some(&C11_VALUE)
            {
                return Err(crate::restrict::PutFieldRefusal::DerivedCellEdited {
                    field: C11_FIELD.to_owned(),
                });
            }
            return Ok((
                original_fields.clone(),
                original_value
                    .cloned()
                    .unwrap_or_else(|| view.value.clone()),
            ));
        }
        if !view.fields.contains_key(C11_FIELD) {
            return Err(crate::restrict::PutFieldRefusal::DerivedCellEdited {
                field: C11_FIELD.to_owned(),
            });
        }
        // Get records a node's source fields exactly when its view fields
        // differ from them. An unrecorded present field was therefore already
        // present, or belongs to a new view node, and remains independently
        // editable. An absent field was handled above because the next get
        // would add it and violate PutGet.
        Ok((
            view.fields.clone(),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

/// The dropping value layer of stage `DropF`: get removes [`C11_FIELD`]
/// and put restores a captured source row.
///
/// With no captured row, put keeps the view, as the production graph lens
/// does for an externally supplied complement.
#[derive(Clone, Copy, Debug, Default)]
pub struct C11DropField;

impl crate::restrict::ValueLayer for C11DropField {
    fn canonical_semantics(&self) -> crate::restrict::ValueLayerSemantics {
        crate::restrict::ValueLayerSemantics {
            domain: "panproto-reference/c11-drop-field",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, crate::restrict::PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::LayerRefusal,
    > {
        let mut fields = node.fields.clone();
        fields.remove(C11_FIELD);
        Ok((fields, node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<crate::value::Presence>>,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::PutFieldRefusal,
    > {
        Ok((
            original_fields
                .cloned()
                .unwrap_or_else(|| view.fields.clone()),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

/// The invertible field-read layer of stage `ReadF`. Its forward and inverse
/// maps are both the identity on [`C11_FIELD`].
#[derive(Clone, Copy, Debug, Default)]
pub struct C11ReadField;

impl crate::restrict::ValueLayer for C11ReadField {
    fn canonical_semantics(&self) -> crate::restrict::ValueLayerSemantics {
        crate::restrict::ValueLayerSemantics {
            domain: "panproto-reference/c11-read-field",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, crate::restrict::PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::LayerRefusal,
    > {
        Ok((node.fields.clone(), node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        _: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<crate::value::Presence>>,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::PutFieldRefusal,
    > {
        Ok((
            view.fields.clone(),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

/// The op-term layer of stage `Op`: get computes [`C11_FIELD`] as
/// [`C11_VALUE`], and put restores the captured source row.
///
/// This separate implementation keeps the op-term execution path distinct
/// from `T` even though the two stages agree on the bounded carrier's forward
/// value.
#[derive(Clone, Copy, Debug, Default)]
pub struct C11OpTerm;

impl crate::restrict::ValueLayer for C11OpTerm {
    fn canonical_semantics(&self) -> crate::restrict::ValueLayerSemantics {
        crate::restrict::ValueLayerSemantics {
            domain: "panproto-reference/c11-op-term",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, crate::restrict::PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::LayerRefusal,
    > {
        let mut fields = node.fields.clone();
        fields.insert(C11_FIELD.to_owned(), C11_VALUE);
        Ok((fields, node.value.clone()))
    }

    /// The op term computes [`C11_FIELD`] and has no inverse: a view that
    /// shows another value (or none) has edited a derived field. Otherwise
    /// the source fields are those recorded, or else the view's.
    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<crate::value::Presence>>,
    ) -> Result<
        (BTreeMap<String, Value>, Option<crate::value::Presence>),
        crate::restrict::PutFieldRefusal,
    > {
        if view.fields.get(C11_FIELD) != Some(&C11_VALUE) {
            return Err(crate::restrict::PutFieldRefusal::DerivedCellEdited {
                field: C11_FIELD.to_owned(),
            });
        }
        // Get records a node's source fields exactly when its view fields
        // differ from them, so an unrecorded node's source fields are its
        // view fields.
        Ok((
            original_fields
                .cloned()
                .unwrap_or_else(|| view.fields.clone()),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

/// Independent cardinalities of the C11 stage and path carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C11PathCounts {
    /// Atomic stages.
    pub stages: usize,
    /// All words of lengths one, two and three.
    pub words_by_length: [usize; 3],
    /// Boundary-matched words of lengths one, two and three.
    pub composable_by_length: [usize; 3],
    /// `(stage, graph state)` pairs.
    pub single_stage_states: usize,
    /// `(word, graph state)` pairs for boundary-matched words, by length.
    pub word_states_by_length: [usize; 3],
    /// Non-fan atomic stages.
    pub tree_stages: usize,
    /// `(stage, tree state)` pairs.
    pub stage_tree_states: usize,
    /// `(word, tree state)` pairs for boundary-matched words, by length.
    pub tree_word_states_by_length: [usize; 3],
    /// `(word, table state)` pairs for boundary-matched words, by length.
    pub table_word_states_by_length: [usize; 3],
}

/// Independently pinned cardinalities of the L.17 graph/tree square carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C11TreeSquareCounts {
    /// Atomic-stage tree sources on which both routes are attempted.
    pub bases: usize,
    /// One-step edited views generated from successful tree gets.
    pub edited_views: usize,
    /// Base and edited observations together.
    pub observations: usize,
}

/// Exact finite bound of the L.17 graph/tree square certificate.
///
/// The base count is independently derived by [`c11_path_count_certificate`].
/// The edited-view count is the frozen Single-Step Basis census over those
/// bases; the exhaustive differential checks both coordinates separately.
#[must_use]
pub const fn c11_tree_square_count_certificate() -> C11TreeSquareCounts {
    C11TreeSquareCounts {
        bases: 2_548,
        edited_views: 28_922,
        observations: 31_470,
    }
}

/// Which categorical or operational path form a certificate case observes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathLawForm {
    /// The atomic path itself, used as the fusion baseline.
    Sequential,
    /// The identity at the source followed by the path.
    LeftIdentity,
    /// The path followed by the identity at the target.
    RightIdentity,
}

/// Carrier on which a path law is evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathLawCarrier {
    /// General graph instances.
    Graph,
    /// Trees in the common graph/tree reference domain.
    Tree,
    /// Finite relational instances.
    Table,
}

/// A named boundary excluded by the table fusion predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableFusionBoundaryWitness {
    /// A noninjective second-stage column map changes the collision winner.
    ColumnMapInjectivity,
    /// An absent cell receives a protocol-dependent typed placeholder.
    TypedPlaceholder,
    /// The middle schema imposes required-value validation.
    RequiredValueValidation,
    /// A target-only table, relation or column has no reverse source carrier.
    TargetOnlyRelationalCarrier,
}

/// One case in the combined path-category and fusion certificate.
///
/// Words and source states are represented by their stable indices in the
/// C11 enumeration. This keeps the carrier streaming: enumerating two million
/// cases does not clone their graph, tree and table payloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathLawCase {
    /// One boundary-matched word under left identity.
    LeftIdentity {
        /// Boundary-matched word in atomic-stage index coordinates.
        word: Vec<usize>,
    },
    /// One boundary-matched word under right identity.
    RightIdentity {
        /// Boundary-matched word in atomic-stage index coordinates.
        word: Vec<usize>,
    },
    /// One ordered triple, including triples with mismatched boundaries.
    Associativity {
        /// Ordered atomic-stage indices.
        stages: [usize; 3],
    },
    /// One operational path/source case.
    Operational {
        /// The path form being observed.
        form: PathLawForm,
        /// The carrier being observed.
        carrier: PathLawCarrier,
        /// A boundary-matched word.
        word: Vec<usize>,
        /// Stable index in the bounded source enumeration for the first stage.
        source: usize,
    },
    /// One independent named table-fusion boundary witness.
    TableFusionBoundary(TableFusionBoundaryWitness),
}

fn c11_composable_words(stages: &[C11Stage]) -> Vec<Vec<usize>> {
    let mut words = Vec::new();
    let mut layer: Vec<Vec<usize>> = vec![Vec::new()];
    for _ in 0..3 {
        layer = layer
            .iter()
            .flat_map(|prefix| {
                (0..stages.len()).map(move |stage| {
                    let mut word = prefix.clone();
                    word.push(stage);
                    word
                })
            })
            .collect();
        words.extend(
            layer
                .iter()
                .filter(|word| {
                    word.windows(2)
                        .all(|pair| stages[pair[0]].target == stages[pair[1]].source)
                })
                .cloned(),
        );
    }
    words
}

/// Stream the combined finite carrier for L.13 through L.16.
///
/// The categorical component contains every boundary-matched word for each
/// unit law and every ordered triple for associativity. The operational
/// component contains the sequential, left-unit and right-unit graph paths,
/// plus the common-domain tree and table fusion paths. Four named witnesses
/// cover table-fusion boundaries that the column-free C11 tables cannot
/// express.
pub fn for_each_path_law_case(mut visit: impl FnMut(PathLawCase)) {
    let stages = c11_stages();
    let words = c11_composable_words(&stages);
    for word in &words {
        visit(PathLawCase::LeftIdentity { word: word.clone() });
        visit(PathLawCase::RightIdentity { word: word.clone() });
    }
    for first in 0..stages.len() {
        for second in 0..stages.len() {
            for third in 0..stages.len() {
                visit(PathLawCase::Associativity {
                    stages: [first, second, third],
                });
            }
        }
    }
    for word in &words {
        let first = &stages[word[0]];
        let graph_sources = if first.source == c11_fan_schema() {
            c11_fan_state_count()
        } else {
            c11_graph_state_count(&first.source)
        };
        for source in 0..graph_sources {
            for form in [
                PathLawForm::Sequential,
                PathLawForm::LeftIdentity,
                PathLawForm::RightIdentity,
            ] {
                visit(PathLawCase::Operational {
                    form,
                    carrier: PathLawCarrier::Graph,
                    word: word.clone(),
                    source,
                });
            }
        }
        if first.source != c11_fan_schema() {
            for source in 0..c11_tree_state_count(&first.source) {
                visit(PathLawCase::Operational {
                    form: PathLawForm::Sequential,
                    carrier: PathLawCarrier::Tree,
                    word: word.clone(),
                    source,
                });
            }
            for source in 0..c11_table_state_count(&first.source) {
                visit(PathLawCase::Operational {
                    form: PathLawForm::Sequential,
                    carrier: PathLawCarrier::Table,
                    word: word.clone(),
                    source,
                });
            }
        }
    }
    for witness in [
        TableFusionBoundaryWitness::ColumnMapInjectivity,
        TableFusionBoundaryWitness::TypedPlaceholder,
        TableFusionBoundaryWitness::RequiredValueValidation,
        TableFusionBoundaryWitness::TargetOnlyRelationalCarrier,
    ] {
        visit(PathLawCase::TableFusionBoundary(witness));
    }
}

/// Independent arithmetic cardinality of [`for_each_path_law_case`].
#[must_use]
pub fn path_law_case_count_certificate() -> usize {
    let counts = c11_path_count_certificate();
    2 * counts.composable()
        + counts.stages.pow(3)
        + 3 * counts.word_states()
        + counts.tree_word_states()
        + counts.table_word_states()
        + 4
}

impl C11PathCounts {
    /// Total number of words.
    #[must_use]
    pub const fn words(self) -> usize {
        self.words_by_length[0] + self.words_by_length[1] + self.words_by_length[2]
    }

    /// Total number of boundary-matched words.
    #[must_use]
    pub const fn composable(self) -> usize {
        self.composable_by_length[0] + self.composable_by_length[1] + self.composable_by_length[2]
    }

    /// Total number of boundary-matched `(word, graph state)` pairs.
    #[must_use]
    pub const fn word_states(self) -> usize {
        self.word_states_by_length[0]
            + self.word_states_by_length[1]
            + self.word_states_by_length[2]
    }

    /// Total number of boundary-matched `(word, tree state)` pairs.
    #[must_use]
    pub const fn tree_word_states(self) -> usize {
        self.tree_word_states_by_length[0]
            + self.tree_word_states_by_length[1]
            + self.tree_word_states_by_length[2]
    }

    /// Total number of boundary-matched `(word, table state)` pairs.
    #[must_use]
    pub const fn table_word_states(self) -> usize {
        self.table_word_states_by_length[0]
            + self.table_word_states_by_length[1]
            + self.table_word_states_by_length[2]
    }
}

/// Compute the C11 path counts from endpoint products and the independent
/// graph/tree state formulae. This does not enumerate words.
#[must_use]
pub fn c11_path_count_certificate() -> C11PathCounts {
    let stages = c11_stages();
    let count = stages.len();
    let mut paths = vec![1_usize; count];
    let state_weights: Vec<usize> = stages
        .iter()
        .map(|stage| {
            if stage.source == c11_fan_schema() {
                c11_fan_state_count()
            } else {
                c11_graph_state_count(&stage.source)
            }
        })
        .collect();
    let mut weighted = state_weights.clone();
    let tree_weights: Vec<usize> = stages
        .iter()
        .map(|stage| {
            if stage.source == c11_fan_schema() {
                0
            } else {
                c11_tree_state_count(&stage.source)
            }
        })
        .collect();
    let table_weights: Vec<usize> = stages
        .iter()
        .map(|stage| {
            if stage.source == c11_fan_schema() {
                0
            } else {
                c11_table_state_count(&stage.source)
            }
        })
        .collect();
    let mut tree_weighted = tree_weights.clone();
    let mut table_weighted = table_weights.clone();
    let mut composable_by_length = [count, 0, 0];
    let mut word_states_by_length = [state_weights.iter().sum(), 0, 0];
    let mut tree_word_states_by_length = [tree_weights.iter().sum(), 0, 0];
    let mut table_word_states_by_length = [table_weights.iter().sum(), 0, 0];
    for length in 1..3 {
        let mut next_paths = vec![0_usize; count];
        let mut next_weighted = vec![0_usize; count];
        let mut next_tree_weighted = vec![0_usize; count];
        let mut next_table_weighted = vec![0_usize; count];
        for (earlier, left) in stages.iter().enumerate() {
            for (later, right) in stages.iter().enumerate() {
                if left.target == right.source {
                    next_paths[later] += paths[earlier];
                    next_weighted[later] += weighted[earlier];
                    next_tree_weighted[later] += tree_weighted[earlier];
                    next_table_weighted[later] += table_weighted[earlier];
                }
            }
        }
        composable_by_length[length] = next_paths.iter().sum();
        word_states_by_length[length] = next_weighted.iter().sum();
        tree_word_states_by_length[length] = next_tree_weighted.iter().sum();
        table_word_states_by_length[length] = next_table_weighted.iter().sum();
        paths = next_paths;
        weighted = next_weighted;
        tree_weighted = next_tree_weighted;
        table_weighted = next_table_weighted;
    }
    let tree_stages: Vec<&C11Stage> = stages
        .iter()
        .filter(|stage| stage.source != c11_fan_schema())
        .collect();
    C11PathCounts {
        stages: count,
        words_by_length: [count, count * count, count * count * count],
        composable_by_length,
        single_stage_states: state_weights.iter().sum(),
        word_states_by_length,
        tree_stages: tree_stages.len(),
        stage_tree_states: tree_stages
            .iter()
            .map(|stage| c11_tree_state_count(&stage.source))
            .sum(),
        tree_word_states_by_length,
        table_word_states_by_length,
    }
}

/// One one-step edit of a C11 view: its family and the edited view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct C11Edit {
    /// The Single-Step Basis family.
    pub mutation: crate::lens::Mutation,
    /// The edited view.
    pub view: Graph,
}

/// The fresh-id rule: `max(floor, 1 + max(taken))`, the floor when nothing
/// is taken, saturating at `u32::MAX` (which the caller then cannot use).
fn fresh_id<'a>(taken: impl Iterator<Item = &'a u32>, floor: u32) -> u32 {
    taken
        .max()
        .map_or(floor, |largest| floor.max(largest.saturating_add(1)))
}

/// Every one-step edit of a C11 graph view.
///
/// The edits of `view` over `target`, against `complement`, are, in
/// Single-Step Basis family order: value
/// toggles, field toggles, a fresh node at each target vertex, node
/// removals (with incident edges), node relabels, a fresh edge for each
/// typed candidate, edge removals, reconnections, edge relabels, then the
/// negative families (synthesized removals and value edits, a node and an
/// edge at each reserved id, and for fan states each fan edge removed).
///
/// A fresh node or edge takes the fresh-id rule's id over the view,
/// reserved and synthesized ids and the complement's floor.
#[must_use]
pub fn c11_graph_edits(
    view: &Graph,
    target: &Schema,
    complement: &crate::restrict::Complement,
) -> Vec<C11Edit> {
    let mut out = Vec::new();
    c11_node_edits(view, target, complement, &mut out);
    c11_edge_edits(view, target, complement, &mut out);
    c11_negative_edits(view, complement, &mut out);
    out
}

/// The node families of [`c11_graph_edits`]: value and field toggles,
/// fresh nodes, removals and relabels.
fn c11_node_edits(
    view: &Graph,
    target: &Schema,
    complement: &crate::restrict::Complement,
    out: &mut Vec<C11Edit>,
) {
    use crate::lens::Mutation as M;
    let mut push = |mutation: M, view: Graph| out.push(C11Edit { mutation, view });
    let anchors: Vec<&str> = target.vertices.keys().map(String::as_str).collect();
    let synthesized = |id: &u32| complement.synthesized_nodes.contains_key(id);
    for (id, node) in &view.nodes {
        if synthesized(id) {
            continue;
        }
        let mut edited = view.clone();
        let target_node = edited.nodes.get_mut(id).unwrap_or_else(|| unreachable!());
        target_node.value = if node.value.is_some() {
            None
        } else {
            Some(crate::value::Presence::Present(C11_VALUE))
        };
        push(M::ValueEdit, edited);
    }
    for (id, node) in &view.nodes {
        if synthesized(id) {
            continue;
        }
        let mut edited = view.clone();
        let fields = &mut edited
            .nodes
            .get_mut(id)
            .unwrap_or_else(|| unreachable!())
            .fields;
        if node.fields.contains_key(C11_FIELD) {
            fields.remove(C11_FIELD);
        } else {
            fields.insert(C11_FIELD.to_owned(), C11_VALUE);
        }
        push(M::FieldEdit, edited);
    }
    let fresh_node = fresh_id(
        view.nodes
            .keys()
            .chain(complement.dropped_nodes.keys())
            .chain(complement.synthesized_nodes.keys()),
        complement.floor.0,
    );
    for anchor in &anchors {
        let mut edited = view.clone();
        edited
            .nodes
            .insert(fresh_node, Node::new(fresh_node, anchor));
        push(M::AddNode, edited);
    }
    for id in view.nodes.keys() {
        if synthesized(id) {
            continue;
        }
        let mut edited = view.clone();
        edited.nodes.remove(id);
        edited
            .edges
            .retain(|_, edge| edge.src != *id && edge.tgt != *id);
        push(M::RemoveNode, edited);
    }
    for (id, node) in &view.nodes {
        if synthesized(id) {
            continue;
        }
        for anchor in &anchors {
            if *anchor == node.anchor {
                continue;
            }
            let mut edited = view.clone();
            (*anchor).clone_into(
                &mut edited
                    .nodes
                    .get_mut(id)
                    .unwrap_or_else(|| unreachable!())
                    .anchor,
            );
            push(M::RelabelNode, edited);
        }
    }
}

/// The edge families of [`c11_graph_edits`]: fresh edges, removals,
/// reconnections and relabels.
fn c11_edge_edits(
    view: &Graph,
    target: &Schema,
    complement: &crate::restrict::Complement,
    out: &mut Vec<C11Edit>,
) {
    use crate::lens::Mutation as M;
    let mut push = |mutation: M, view: Graph| out.push(C11Edit { mutation, view });
    let fresh_edge = fresh_id(
        view.edges
            .keys()
            .chain(complement.dropped_edges.keys())
            .chain(complement.synthesized_edges.keys()),
        complement.floor.1,
    );
    let typed = |src: u32, tgt: u32| -> Vec<SEdge> {
        let (Some(s), Some(t)) = (view.nodes.get(&src), view.nodes.get(&tgt)) else {
            return Vec::new();
        };
        target
            .edges
            .iter()
            .filter(|edge| edge.src == s.anchor && edge.tgt == t.anchor)
            .cloned()
            .collect()
    };
    for src in view.nodes.keys() {
        for tgt in view.nodes.keys() {
            for label in typed(*src, *tgt) {
                let mut edited = view.clone();
                edited.edges.insert(
                    fresh_edge,
                    GEdge {
                        src: *src,
                        tgt: *tgt,
                        label,
                        position: None,
                    },
                );
                push(M::AddEdge, edited);
            }
        }
    }
    let synthesized_edge = |id: &u32| complement.synthesized_edges.contains_key(id);
    for id in view.edges.keys() {
        if synthesized_edge(id) {
            continue;
        }
        let mut edited = view.clone();
        edited.edges.remove(id);
        push(M::RemoveEdge, edited);
    }
    for (id, edge) in &view.edges {
        if synthesized_edge(id) {
            continue;
        }
        for src in view.nodes.keys() {
            for tgt in view.nodes.keys() {
                if (*src, *tgt) == (edge.src, edge.tgt) || !typed(*src, *tgt).contains(&edge.label)
                {
                    continue;
                }
                let mut edited = view.clone();
                let moved = edited.edges.get_mut(id).unwrap_or_else(|| unreachable!());
                moved.src = *src;
                moved.tgt = *tgt;
                push(M::Reconnect, edited);
            }
        }
    }
    for (id, edge) in &view.edges {
        if synthesized_edge(id) {
            continue;
        }
        for label in typed(edge.src, edge.tgt) {
            if label == edge.label {
                continue;
            }
            let mut edited = view.clone();
            edited
                .edges
                .get_mut(id)
                .unwrap_or_else(|| unreachable!())
                .label = label;
            push(M::RelabelEdge, edited);
        }
    }
}

/// The negative families of [`c11_graph_edits`].
fn c11_negative_edits(
    view: &Graph,
    complement: &crate::restrict::Complement,
    out: &mut Vec<C11Edit>,
) {
    use crate::lens::Mutation as M;
    let mut push = |mutation: M, view: Graph| out.push(C11Edit { mutation, view });
    for id in complement.synthesized_nodes.keys() {
        if view.nodes.contains_key(id) {
            let mut removed = view.clone();
            removed.nodes.remove(id);
            removed
                .edges
                .retain(|_, edge| edge.src != *id && edge.tgt != *id);
            push(M::RemoveSynthesized, removed);
        }
    }
    for id in complement.synthesized_edges.keys() {
        if view.edges.contains_key(id) {
            let mut removed = view.clone();
            removed.edges.remove(id);
            push(M::RemoveSynthesized, removed);
        }
    }
    for id in complement.synthesized_nodes.keys() {
        if let Some(node) = view.nodes.get(id) {
            let mut edited = view.clone();
            edited
                .nodes
                .get_mut(id)
                .unwrap_or_else(|| unreachable!())
                .value = if node.value.is_some() {
                None
            } else {
                Some(crate::value::Presence::Present(C11_VALUE))
            };
            push(M::EditSynthesized, edited);
        }
    }
    for (id, dropped) in &complement.dropped_nodes {
        let mut edited = view.clone();
        edited.nodes.insert(*id, Node::new(*id, &dropped.anchor));
        push(M::ReservedIdReuse, edited);
    }
    for (id, dropped) in &complement.dropped_edges {
        if view.nodes.contains_key(&dropped.src) && view.nodes.contains_key(&dropped.tgt) {
            let mut edited = view.clone();
            edited.edges.insert(*id, dropped.clone());
            push(M::ReservedIdReuse, edited);
        }
    }
    for (id, edge) in &view.edges {
        if crate::restrict::is_fan_kind(&edge.label.kind) {
            let mut edited = view.clone();
            edited.edges.remove(id);
            push(M::MalformedFan, edited);
        }
    }
}

/// Independent per-view count of [`c11_graph_edits`].
///
/// The count is computed from the sizes of the view and complement: for `n` non-synthesized nodes, `t` target vertices,
/// typed candidate count `c`, and so on, the sum of the family sizes.
#[must_use]
pub fn c11_graph_edit_count(
    view: &Graph,
    target: &Schema,
    complement: &crate::restrict::Complement,
) -> usize {
    let plain_nodes: Vec<&Node> = view
        .nodes
        .iter()
        .filter(|(id, _)| !complement.synthesized_nodes.contains_key(id))
        .map(|(_, node)| node)
        .collect();
    let n = plain_nodes.len();
    let t = target.vertices.len();
    let typed_count = |src: &Node, tgt: &Node| {
        target
            .edges
            .iter()
            .filter(|edge| edge.src == src.anchor && edge.tgt == tgt.anchor)
            .count()
    };
    let all_nodes: Vec<&Node> = view.nodes.values().collect();
    let candidates: usize = all_nodes
        .iter()
        .map(|s| all_nodes.iter().map(|t| typed_count(s, t)).sum::<usize>())
        .sum();
    let plain_edges: Vec<&GEdge> = view
        .edges
        .iter()
        .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
        .map(|(_, edge)| edge)
        .collect();
    let mut reconnects = 0;
    let mut relabels = 0;
    for edge in &plain_edges {
        let pairs = all_nodes
            .iter()
            .flat_map(|s| all_nodes.iter().map(move |t| (*s, *t)))
            .filter(|(s, t)| {
                s.anchor == edge.label.src
                    && t.anchor == edge.label.tgt
                    && target.edges.contains(&edge.label)
            })
            .count();
        reconnects += pairs.saturating_sub(1);
        let (Some(s), Some(t)) = (view.nodes.get(&edge.src), view.nodes.get(&edge.tgt)) else {
            continue;
        };
        relabels +=
            typed_count(s, t).saturating_sub(usize::from(target.edges.contains(&edge.label)));
    }
    let relabel_nodes: usize = plain_nodes
        .iter()
        .map(|node| t - usize::from(target.vertices.contains_key(&node.anchor)))
        .sum();
    let synth_nodes = complement
        .synthesized_nodes
        .keys()
        .filter(|id| view.nodes.contains_key(id))
        .count();
    let synth_edges = complement
        .synthesized_edges
        .keys()
        .filter(|id| view.edges.contains_key(id))
        .count();
    let reserved_edges = complement
        .dropped_edges
        .values()
        .filter(|edge| view.nodes.contains_key(&edge.src) && view.nodes.contains_key(&edge.tgt))
        .count();
    let fan_edges = view
        .edges
        .values()
        .filter(|edge| crate::restrict::is_fan_kind(&edge.label.kind))
        .count();
    2 * n
        + t
        + n
        + relabel_nodes
        + candidates
        + plain_edges.len()
        + reconnects
        + relabels
        + 2 * synth_nodes
        + synth_edges
        + complement.dropped_nodes.len()
        + reserved_edges
        + fan_edges
}

#[cfg(test)]
mod tests {
    use super::{
        C11_FIELD, C11AddField, LensEquivalenceCase, Node, PartialLens, c11_path_count_certificate,
        c11_stages, derived_writer_count_certificate, for_each_derived_writer_fixture,
        for_each_lens_equivalence_case, for_each_table_complement_construction,
        last_explicit_writer_count_certificate, partial_lens_count_certificate,
        partial_lens_histogram, table_complement_construction_count_certificate,
        writer_count_certificate,
    };
    use crate::restrict::{PutFieldRefusal, ValueLayer};

    /// `GetPut` and `WeakPutGet` do not imply `PutTwice`: put `(1, 2)` gives
    /// source 0, whose complement is 0, and put `(1, 0)` is undefined.
    #[test]
    fn put_twice_is_not_implied_by_get_put_and_weak_put_get() {
        let counterexample = PartialLens {
            get: [(0, 0), (0, 1)],
            put: [Some(0), Some(1), Some(0), None, Some(1), Some(0)],
        };
        assert!(counterexample.get_put());
        assert!(counterexample.weak_put_get());
        assert!(!counterexample.put_twice());
        assert_eq!(counterexample.put(1, 2), Some(0));
        assert_eq!(counterexample.get_table()[0], (0, 0));
        assert_eq!(counterexample.put(1, 0), None);
    }

    #[test]
    fn the_partial_lens_histogram_is_exact() {
        let histogram = partial_lens_histogram();
        assert_eq!(histogram.total, partial_lens_count_certificate());
        assert_eq!(histogram.total, 21_870);
        assert_eq!(histogram.get_put, 2_430);
        assert_eq!(histogram.weak_put_get, 5_388);
        assert_eq!(histogram.exact_put_get, 1_476);
        assert_eq!(histogram.put_twice, 3_942);
        assert_eq!(histogram.get_put_and_weak_put_get, 1_104);
        assert_eq!(histogram.get_put_and_weak_put_get_without_put_twice, 312);
        assert_eq!(histogram.get_put_and_exact_put_get_without_put_twice, 0);
    }

    #[test]
    fn the_writer_carrier_has_its_certified_counts() {
        assert_eq!(writer_count_certificate(), (9, 81, 180));
        assert_eq!(last_explicit_writer_count_certificate(), 36);
        let mut derived = 0;
        for_each_derived_writer_fixture(|_| derived += 1);
        assert_eq!(derived, derived_writer_count_certificate());
        assert_eq!(derived, 2);
    }

    #[test]
    fn the_table_complement_construction_certificate_is_exact() {
        let mut names = Vec::new();
        for_each_table_complement_construction(|construction| names.push(construction.name));
        assert_eq!(
            names.len(),
            table_complement_construction_count_certificate()
        );
        assert_eq!(names.len(), 7);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 7, "construction names are unique");
    }

    #[test]
    fn add_field_refuses_an_inserted_node_that_omits_the_derived_field() {
        let node = Node::new(0, "a");
        assert_eq!(
            C11AddField.restore(&node, None, None),
            Err(PutFieldRefusal::DerivedCellEdited {
                field: C11_FIELD.to_owned(),
            })
        );
    }

    #[test]
    fn the_amended_c11_stage_and_path_census_is_exact() {
        let stages = c11_stages();
        let names: Vec<&str> = stages.iter().map(|stage| stage.name.as_str()).collect();
        assert_eq!(stages.len(), 31);
        assert_eq!(names.iter().filter(|name| **name == "DropF").count(), 1);
        assert_eq!(names.iter().filter(|name| **name == "ReadF").count(), 1);
        assert_eq!(names.iter().filter(|name| **name == "Op").count(), 1);
        let certificate = c11_path_count_certificate();
        assert_eq!(certificate.stages, 31);
        assert_eq!(certificate.words_by_length, [31, 961, 29_791]);
        assert_eq!(certificate.words(), 30_783);
        assert_eq!(certificate.composable_by_length, [31, 93, 413]);
        assert_eq!(certificate.composable(), 537);
        assert_eq!(certificate.single_stage_states, 144_397);
        assert_eq!(
            certificate.word_states_by_length,
            [144_397, 182_833, 291_373]
        );
        assert_eq!(certificate.word_states(), 618_603);
        assert_eq!(certificate.tree_stages, 29);
        assert_eq!(certificate.stage_tree_states, 2_548);
        assert_eq!(
            certificate.tree_word_states_by_length,
            [2_548, 3_876, 9_172]
        );
        assert_eq!(certificate.tree_word_states(), 15_596);
        assert_eq!(
            certificate.table_word_states_by_length,
            [21_701, 27_561, 51_549]
        );
        assert_eq!(certificate.table_word_states(), 100_811);

        let mut lens_equivalence_bases = [0_usize; 3];
        for_each_lens_equivalence_case(|case| match case {
            LensEquivalenceCase::Graph { .. } => lens_equivalence_bases[0] += 1,
            LensEquivalenceCase::Tree { .. } => lens_equivalence_bases[1] += 1,
            LensEquivalenceCase::Table(_) => lens_equivalence_bases[2] += 1,
        });
        assert_eq!(lens_equivalence_bases, [144_397, 2_548, 9_921]);
    }
}
