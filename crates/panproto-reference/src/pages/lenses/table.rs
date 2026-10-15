//! The table lens: rows and pairs restricted as graph nodes and edges, cells
//! through the cell fates of the attribute arrows, the table complement,
//! and put by the baseline-and-pullback rule.
//!
//! The value layer ([`ValueLayer`]) supplies survival predicates and the
//! transforms of transformed tables: get reads cells after the transforms,
//! and put pulls an edited cell back through its writer and then through
//! the value layer's restoration; a cell with no writer is derived.

use std::collections::{BTreeMap, BTreeSet};

use super::domain::{
    PutDomainError, ViewRef, key_part_present, part_column, required_value_violations,
    uniqueness_role,
};
use crate::canonical::Writer;
use crate::encode::{TableEncodeError, table_to_graph};
use crate::keys::{KeyEquality, SqlCollation};
use crate::migration::Migration;
use crate::model::{Graph, Name, SEdge, Table};
use crate::pages::representation::canonical::{ComplementShape, IdentityEnvelope};
use crate::rel::{FunctorError, RelFunctor, RelSchema, entity_functor};
use crate::restrict::{
    self, EdgeFate, EngineError, ErrorKey, LayerRefusal, PairResolution, Plan, PlanError,
    RestrictError, SynthKey, ValueLayer, ValueLayerSemantics,
};
use crate::schema::{KeyRole, KeyScope, Schema};
use crate::value::Value;

const SCHEMA_CONTEXT: &str = "panproto 2026-09 schema identity v2";
const TABLE_LENS_CONTEXT: &str = "panproto 2026-09 compiled table lens identity v2";
const FUNCTOR_COMPLEMENT_FORMAT_VERSION: u16 = 1;

/// Canonical bytes of every semantic field in the finite reference
/// migration.
///
/// This is the reference model's analogue of production's compiled-
/// migration encoding. Field tags fix the record layout, maps and sets are
/// already in key order, and vectors retain their semantic order.
#[must_use]
pub fn canonical_migration_bytes(migration: &Migration) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.raw(b"panproto-reference/restrict-plan-migration/v1");

    writer.tag(0);
    writer.len(migration.surviving_verts.len());
    for vertex in &migration.surviving_verts {
        writer.str(vertex);
    }

    writer.tag(1);
    writer.len(migration.surviving_edges.len());
    for edge in &migration.surviving_edges {
        writer.edge(edge);
    }

    writer.tag(2);
    writer.len(migration.vertex_remap.len());
    for (source, target) in &migration.vertex_remap {
        writer.str(source);
        writer.str(target);
    }

    writer.tag(3);
    writer.len(migration.edge_remap.len());
    for (source, target) in &migration.edge_remap {
        writer.edge(source);
        writer.edge(target);
    }

    writer.tag(4);
    writer.len(migration.resolver.len());
    for ((source, target), edge) in &migration.resolver {
        writer.str(source);
        writer.str(target);
        writer.edge(edge);
    }

    writer.tag(5);
    writer.len(migration.expansion_path.len());
    for ((source, target), intermediates) in &migration.expansion_path {
        writer.str(source);
        writer.str(target);
        writer.len(intermediates.len());
        for intermediate in intermediates {
            writer.str(intermediate);
        }
    }

    writer.tag(6);
    writer.len(migration.hyper_resolver.len());
    for ((source, labels), (target, label_remap)) in &migration.hyper_resolver {
        writer.str(source);
        writer.len(labels.len());
        for label in labels {
            writer.str(label);
        }
        writer.str(target);
        writer.len(label_remap.len());
        for (source_label, target_label) in label_remap {
            writer.str(source_label);
            writer.str(target_label);
        }
    }

    writer.tag(7);
    writer.len(migration.conditional.len());
    for vertex in &migration.conditional {
        writer.str(vertex);
    }

    writer.tag(8);
    writer.len(migration.transformed.len());
    for vertex in &migration.transformed {
        writer.str(vertex);
    }

    writer.bytes
}

/// Canonical bytes of an exact relation-ified schema boundary.
///
/// Each field has a fixed tag, variable-length components are framed, ordered
/// collections retain their `BTree` order, and every field of [`RelSchema`]
/// participates.
#[must_use]
pub fn canonical_rel_schema_bytes(schema: &RelSchema) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.raw(b"panproto-reference/relation-schema/v1");

    writer.tag(0);
    writer.len(schema.tables.len());
    for table in &schema.tables {
        writer.str(table);
    }

    writer.tag(1);
    writer.len(schema.relations.len());
    for relation in &schema.relations {
        writer.edge(relation);
    }

    writer.tag(2);
    writer.len(schema.columns.len());
    for (table, columns) in &schema.columns {
        writer.str(table);
        writer.len(columns.len());
        for (column, arrow) in columns {
            writer.str(column);
            writer.edge(arrow);
        }
    }

    writer.tag(3);
    writer.len(schema.value_vertices.len());
    for vertex in &schema.value_vertices {
        writer.str(vertex);
    }

    writer.tag(4);
    writer.len(schema.inner_edges.len());
    for edge in &schema.inner_edges {
        writer.edge(edge);
    }

    writer.tag(5);
    writer.len(schema.kinds.len());
    for (vertex, kind) in &schema.kinds {
        writer.str(vertex);
        writer.str(kind);
    }
    writer.bytes
}

fn canonical_value_layer_bytes(semantics: &ValueLayerSemantics) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.tag(0);
    writer.str(semantics.domain);
    writer.tag(1);
    writer
        .bytes
        .extend_from_slice(&semantics.version.to_le_bytes());
    writer.tag(2);
    writer.raw(&semantics.canonical_payload);
    writer.bytes
}

/// The exact identity stamped on a table complement made by this lens.
///
/// The compiled-lens digest binds the two schema digests, the exact two
/// relation-ified endpoint schemas, the complete canonical migration, and
/// the value layer's canonical semantic descriptor. Constraint semantics are
/// put-admission context, so they deliberately do not participate.
#[must_use]
pub fn table_lens_identity(
    source: &Schema,
    target: &Schema,
    source_boundary: &RelSchema,
    target_boundary: &RelSchema,
    migration: &Migration,
    value_layer: &ValueLayerSemantics,
) -> IdentityEnvelope {
    let source_schema =
        crate::blake3_ref::derive_key(SCHEMA_CONTEXT, &crate::schema::canonical_bytes(source));
    let target_schema =
        crate::blake3_ref::derive_key(SCHEMA_CONTEXT, &crate::schema::canonical_bytes(target));
    let mut writer = Writer::default();
    writer.raw(b"panproto-reference/table-lens-identity/v2");
    writer.tag(0);
    writer.raw(&source_schema);
    writer.tag(1);
    writer.raw(&target_schema);
    writer.tag(2);
    writer.raw(&canonical_rel_schema_bytes(source_boundary));
    writer.tag(3);
    writer.raw(&canonical_rel_schema_bytes(target_boundary));
    writer.tag(4);
    writer.raw(&canonical_migration_bytes(migration));
    writer.tag(5);
    writer.raw(&canonical_value_layer_bytes(value_layer));
    let lens = crate::blake3_ref::derive_key(TABLE_LENS_CONTEXT, &writer.bytes);
    IdentityEnvelope {
        source_schema,
        target_schema,
        lens,
        shape: ComplementShape::Functor,
        format_version: FUNCTOR_COMPLEMENT_FORMAT_VERSION,
    }
}

/// What happens to a source column of a kept table.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CellFate {
    /// The column is dropped.
    Drop(String),
    /// The column is renamed.
    Rename(String, String),
}

/// The element on which the two survival rules disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurvivalElement {
    /// A table.
    Table(Name),
    /// A relation or attribute arrow.
    Edge(SEdge),
}

/// A table plan refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TablePlanError {
    /// The restrict plan refused.
    Plan(Box<PlanError>),
    /// The entity functor refused.
    Functor(Box<FunctorError>),
    /// An expansion or hyper-edge resolution.
    NotTableRepresentable,
    /// Graph restrict's survival and the entity functor disagree.
    SurvivalRulesDisagree(Box<SurvivalElement>),
    /// A bound lens supplied another source relational boundary.
    SourceBoundaryMismatch,
    /// A bound lens supplied another target relational boundary.
    TargetBoundaryMismatch,
}

/// The compiled table plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePlan {
    /// The restrict plan.
    pub(crate) plan: Plan,
    /// The entity functor.
    pub(crate) functor: RelFunctor,
    /// Per kept source table, the fates of its columns in column order.
    pub(crate) fates: BTreeMap<Name, Vec<CellFate>>,
}

impl TablePlan {
    /// The validated restrict plan underlying this table plan.
    #[must_use]
    pub const fn restrict_plan(&self) -> &Plan {
        &self.plan
    }

    /// The compiled entity functor.
    #[must_use]
    pub const fn functor(&self) -> &RelFunctor {
        &self.functor
    }

    /// Source relational boundary through which this plan was compiled.
    #[must_use]
    pub const fn source_boundary(&self) -> &RelSchema {
        &self.functor.src
    }

    /// Target relational boundary through which this plan was compiled.
    #[must_use]
    pub const fn target_boundary(&self) -> &RelSchema {
        &self.functor.tgt
    }

    /// Per kept source table, the fates of its columns in column order.
    #[must_use]
    pub const fn fates(&self) -> &BTreeMap<Name, Vec<CellFate>> {
        &self.fates
    }

    /// Definition: The writer of target column `t` of rows of source table `s` is the
    /// source of the last `Rename(_, t)` in fate order, else `t` itself
    /// when `t` has no fate, else none (a derived cell).
    ///
    /// Book anchors: `L-lenses-12`.
    #[must_use]
    pub fn writer(&self, s: &str, t: &str) -> Option<String> {
        let fates = self.fates.get(s).map(Vec::as_slice).unwrap_or_default();
        if let Some(CellFate::Rename(from, _)) = fates
            .iter()
            .rev()
            .find(|f| matches!(f, CellFate::Rename(_, to) if to == t))
        {
            return Some(from.clone());
        }
        let has_fate = fates
            .iter()
            .any(|f| matches!(f, CellFate::Drop(c) | CellFate::Rename(c, _) if c == t));
        (!has_fate).then(|| t.to_owned())
    }

    /// The view cells of a source row of table `s`: each target column with
    /// a writer present in the row takes the writer's cell (all fates
    /// simultaneous).
    #[must_use]
    pub fn apply(&self, s: &str, row: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
        let mut targets: BTreeSet<String> = row.keys().cloned().collect();
        for f in self.fates.get(s).into_iter().flatten() {
            if let CellFate::Rename(_, to) = f {
                targets.insert(to.clone());
            }
        }
        targets
            .into_iter()
            .filter_map(|t| {
                self.writer(s, &t)
                    .and_then(|w| row.get(&w).cloned())
                    .map(|v| (t, v))
            })
            .collect()
    }
}

/// Definition: Compile the table plan by the restrict plan, entity functor,
/// table-representability and survival-agreement checks, then cell fates.
///
/// The restrict plan, the entity functor, the refusal of expansions and hyper-edge resolution, the
/// agreement check of the two survival rules (tables ascending, then relations and attribute arrows
/// of tables both keep, ascending), and the cell fates.
///
/// # Errors
///
/// The first [`TablePlanError`] in that order.
///
/// Book anchors: `L-lenses-12`.
pub fn compile_for_tables(
    src: &Schema,
    tgt: &Schema,
    rs: &RelSchema,
    rt: &RelSchema,
    m: &Migration,
) -> Result<TablePlan, TablePlanError> {
    let plan = restrict::compile(src, tgt, m).map_err(|e| TablePlanError::Plan(Box::new(e)))?;
    let entity = entity_functor(rs, rt, m).map_err(|e| TablePlanError::Functor(Box::new(e)))?;
    let functor = entity.table_part().clone();
    let expands = plan
        .fates
        .values()
        .any(|f| matches!(f, EdgeFate::Expand { .. }))
        || plan
            .pairs
            .values()
            .any(|p| matches!(p.resolution, PairResolution::Expand { .. }));
    if expands || !m.hyper_resolver.is_empty() {
        return Err(TablePlanError::NotTableRepresentable);
    }
    for t in &rs.tables {
        let fr = m.survives(t).then(|| m.vertex_image(t).to_owned());
        if fr != functor.tables.get(t).cloned() {
            return Err(TablePlanError::SurvivalRulesDisagree(Box::new(
                SurvivalElement::Table(t.clone()),
            )));
        }
    }
    let kept = |t: &str| functor.tables.contains_key(t);
    let mut edges: Vec<(SEdge, Option<SEdge>)> = Vec::new();
    for r in rs.relations.iter().filter(|r| kept(&r.src) && kept(&r.tgt)) {
        edges.push((r.clone(), functor.relations.get(r).cloned()));
    }
    for (t, cols) in rs.columns.iter().filter(|(t, _)| kept(t)) {
        for (c, a) in cols {
            let image = functor.columns.get(&(t.clone(), c.clone())).and_then(|tc| {
                rt.columns
                    .get(&functor.tables[t])
                    .and_then(|cs| cs.get(tc))
                    .cloned()
            });
            edges.push((a.clone(), image));
        }
    }
    edges.sort();
    for (e, ef) in edges {
        let agree = match (plan.fates.get(&e), &ef) {
            (Some(EdgeFate::Keep(i)), Some(j)) => i == j,
            (Some(EdgeFate::Drop), None) => true,
            _ => false,
        };
        if !agree {
            return Err(TablePlanError::SurvivalRulesDisagree(Box::new(
                SurvivalElement::Edge(e),
            )));
        }
    }
    let mut fates = BTreeMap::new();
    for t in functor.tables.keys() {
        let image = &functor.tables[t];
        let mut v = Vec::new();
        for (c, a) in rs.columns.get(t).into_iter().flatten() {
            let target_col = match plan.fates.get(a) {
                Some(EdgeFate::Keep(e)) => rt
                    .columns
                    .get(image)
                    .and_then(|cs| cs.iter().find(|(_, x)| *x == e).map(|(n, _)| n.clone())),
                _ => None,
            };
            match target_col {
                None => v.push(CellFate::Drop(c.clone())),
                Some(c2) if c2 != *c => v.push(CellFate::Rename(c.clone(), c2)),
                Some(_) => {}
            }
        }
        fates.insert(t.clone(), v);
    }
    Ok(TablePlan {
        plan,
        functor,
        fates,
    })
}

/// A source row absent from the view, paired with its source table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedRow {
    /// The source table of the row.
    pub table: Name,
    /// The source row's cells.
    pub row: BTreeMap<String, Value>,
}

/// The table complement, in the exact T.3 field order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableComplement {
    /// Dropped rows with their table and cells.
    pub dropped_rows: BTreeMap<u32, DroppedRow>,
    /// Source cells of surviving rows that the view holds under no cell of
    /// their name.
    pub dropped_cells: BTreeMap<(u32, String), Value>,
    /// Dropped pairs by source edge.
    pub dropped_fk_pairs: BTreeMap<SEdge, BTreeSet<(u32, u32)>>,
    /// View pairs whose source edges the preimage rule would not return.
    pub original_fk_edges: BTreeMap<(SEdge, u32, u32), BTreeSet<SEdge>>,
    /// Rows whose source table the reverse remap would not return.
    pub original_tables: BTreeMap<u32, Name>,
    /// Source cells of surviving rows whose same-named view cell differs.
    pub original_cells: BTreeMap<(u32, String), Value>,
    /// Contracted pairs.
    pub synthesized_fk_pairs: BTreeSet<(SEdge, u32, u32)>,
    /// The row id floor.
    pub id_floor: u32,
    /// The envelope binding the complement to its lens.
    pub identity: Option<IdentityEnvelope>,
}

impl TableComplement {
    /// Canonical T.3 field order.
    ///
    /// The reference carrier is not serialized, so this list is the public
    /// layout contract against which differential tests pin the declaration.
    pub const FIELD_ORDER: [&'static str; 9] = [
        "dropped_rows",
        "dropped_cells",
        "dropped_fk_pairs",
        "original_fk_edges",
        "original_tables",
        "original_cells",
        "synthesized_fk_pairs",
        "id_floor",
        "identity",
    ];

    /// The complement of nothing: empty residue and bookkeeping, floor zero,
    /// and no lens envelope.
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

    /// Whether this is exactly [`Self::empty`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::empty()
    }

    /// Whether no source residue is recorded.
    ///
    /// Synthesized pairs, the allocation floor and the identity envelope are
    /// bookkeeping rather than residue.
    #[must_use]
    pub fn residue_is_trivial(&self) -> bool {
        self.dropped_rows.is_empty()
            && self.dropped_cells.is_empty()
            && self.dropped_fk_pairs.is_empty()
            && self.original_fk_edges.is_empty()
            && self.original_tables.is_empty()
            && self.original_cells.is_empty()
    }

    /// Row ids unavailable for new view rows because they name hidden rows.
    #[must_use]
    pub fn reserved_row_ids(&self) -> BTreeSet<u32> {
        self.dropped_rows.keys().copied().collect()
    }

    /// Whether this complement carries no residue or allocation bookkeeping.
    fn records_nothing(&self) -> bool {
        self.residue_is_trivial() && self.synthesized_fk_pairs.is_empty() && self.id_floor == 0
    }
}

impl Default for TableComplement {
    fn default() -> Self {
        Self::empty()
    }
}

/// How a table complement is bound to the lens supplied to checked put.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityStatus {
    /// The complement carries the lens's exact envelope.
    Verified,
    /// The complement is unstamped and records nothing.
    Unbound,
}

/// The compatibility policy for table-complement identity.
///
/// The finite reference model has no legacy fingerprint field, so both
/// policies accept exactly verified complements and empty unbound
/// complements. The distinction is retained so differential callers can use
/// the production call shape without weakening the reference check.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IdentityPolicy {
    /// Require a verified envelope except on an empty unbound complement.
    Strict,
    /// Production's default policy. There is no legacy table encoding in the
    /// reference carrier, so its accepted set is the same as [`Self::Strict`].
    #[default]
    AllowLegacy,
}

/// A table complement cannot be used with the supplied lens identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// A stamped complement names another endpoint, migration, shape or
    /// complement version.
    Mismatch {
        /// The lens's envelope.
        expected: Box<IdentityEnvelope>,
        /// The complement's envelope.
        found: Box<IdentityEnvelope>,
    },
    /// A complement records state but carries no lens envelope.
    UnboundComplement,
}

/// A checked table-put refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TablePutError {
    /// Complement identity is checked before restoration.
    Identity(IdentityError),
    /// The first violation of the table put domain.
    Domain(PutDomainError),
    /// Evaluation could not complete, so no complete domain report exists.
    Operational(TableOperationalError),
    /// The reference model cannot decide text equality under these named
    /// database collations without a caller-supplied oracle.
    NamedCollationUnresolved {
        /// Collation names in ascending order.
        names: BTreeSet<String>,
    },
    /// The lens is not statically invertible without a complement.
    NotAnIsomorphism {
        /// The first obstructing table coordinate.
        detail: String,
    },
}

/// A complete checked table-domain outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableCheckError {
    /// Every known violation in deterministic report order, together with
    /// every unresolved named collation applicable to a feasible restoration
    /// candidate.
    Domain {
        /// Complete ordered put-domain violations.
        violations: Vec<PutDomainError>,
        /// Candidate-local unresolved collation names in ascending order.
        unresolved_collations: BTreeSet<String>,
    },
    /// No known violation was found, but named-collation equality remains
    /// outside the finite reference evaluator.
    NamedCollationUnresolved {
        /// Collation names in ascending order.
        names: BTreeSet<String>,
    },
    /// Evaluation could not complete, so a complete domain report would be
    /// unsound.
    Operational(TableOperationalError),
}

/// An operational failure that prevents complete table-domain evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableOperationalError {
    /// The baseline or restored source table has no graph encoding.
    BaselineEncoding(TableEncodeError),
    /// The value layer refused to transform a baseline row.
    Transform {
        /// The affected row.
        elem: ViewRef,
        /// The transform-layer refusal.
        refusal: LayerRefusal,
    },
    /// The independent forward get could not be evaluated.
    ForwardGet(RestrictError),
}

/// No row id below the reserved exhaustion sentinel remains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowIdsExhausted;

enum CellRestoreError {
    Domain(Vec<PutDomainError>),
    Operational(TableOperationalError),
}

struct TableRestoration {
    source: Table,
    violations: Vec<PutDomainError>,
    unresolved_collations: BTreeSet<String>,
}

/// Definition: Table get: rows and pairs restricted as graph nodes and edges (graph restrict and
/// contraction), then the cell fates of each surviving row's source table,
/// and the table complement.
///
/// # Errors
///
/// The engine's [`RestrictError`].
///
/// Book anchors: `L-lenses-12`.
fn get_with_identity(
    src: &Schema,
    tgt: &Schema,
    mig: &Migration,
    tp: &TablePlan,
    vl: &dyn ValueLayer,
    source: &Table,
    identity: IdentityEnvelope,
) -> Result<(Table, TableComplement), RestrictError> {
    let (graph, _) = table_to_graph(source).map_err(|_| RestrictError::At {
        key: ErrorKey {
            stage: 5,
            id: 1,
            id2: 0,
        },
        error: EngineError::IdsExhausted { nodes: false },
    })?;
    let (vg, gc) = restrict::get_with_plan(src, tgt, mig, &tp.plan, vl, &graph)?;
    let mut view = Table::default();
    let mut comp = TableComplement {
        id_floor: gc.floor.0,
        identity: Some(identity),
        ..TableComplement::default()
    };
    for (id, node) in &vg.nodes {
        let (stable, source_cells) = &source.rows[id];
        view.rows
            .insert(*id, (node.anchor.clone(), tp.apply(stable, &node.fields)));
        if table_preimages(tp, &node.anchor) != vec![stable.clone()] {
            comp.original_tables.insert(*id, stable.clone());
        }
        let cells = &view.rows[id].1;
        for (col, old) in source_cells {
            match cells.get(col) {
                None => {
                    comp.dropped_cells.insert((*id, col.clone()), old.clone());
                }
                Some(seen) if seen != old => {
                    comp.original_cells.insert((*id, col.clone()), old.clone());
                }
                Some(_) => {}
            }
        }
    }
    for (id, node) in &gc.dropped_nodes {
        comp.dropped_rows.insert(
            *id,
            DroppedRow {
                table: node.anchor.clone(),
                row: node.fields.clone(),
            },
        );
    }
    for edge in gc.dropped_edges.values() {
        comp.dropped_fk_pairs
            .entry(edge.label.clone())
            .or_default()
            .insert((edge.src, edge.tgt));
    }
    for (id, edge) in &vg.edges {
        let pair = (edge.label.clone(), edge.src, edge.tgt);
        if gc.synthesized_edges.contains_key(id) {
            comp.synthesized_fk_pairs.insert(pair.clone());
        }
        view.pairs.insert(pair);
    }
    let mut sources: BTreeMap<(SEdge, u32, u32), BTreeSet<SEdge>> = BTreeMap::new();
    for (id, edge) in &vg.edges {
        if !gc.synthesized_edges.contains_key(id) {
            sources
                .entry((edge.label.clone(), edge.src, edge.tgt))
                .or_default()
                .insert(graph.edges[id].label.clone());
        }
    }
    for (pair, srcs) in sources {
        let unique = preimage(src, mig, &tp.plan, &view, &pair).ok();
        let back = unique.map(|edge| BTreeSet::from([edge]));
        if comp.synthesized_fk_pairs.contains(&pair) || back.as_ref() != Some(&srcs) {
            comp.original_fk_edges.insert(pair, srcs);
        }
    }
    Ok((view, comp))
}

/// Definition: Table get through a compiled table plan.
///
/// The free operation computes the representation-specific identity once for
/// this invocation. Bound callers use [`TableLens::get`], whose identity was
/// computed once after construction-time validation.
///
/// # Errors
///
/// The engine's [`RestrictError`].
pub fn get(
    src: &Schema,
    tgt: &Schema,
    mig: &Migration,
    tp: &TablePlan,
    vl: &dyn ValueLayer,
    source: &Table,
) -> Result<(Table, TableComplement), RestrictError> {
    let semantics = vl.canonical_semantics();
    let identity = table_lens_identity(
        src,
        tgt,
        tp.source_boundary(),
        tp.target_boundary(),
        mig,
        &semantics,
    );
    get_with_identity(src, tgt, mig, tp, vl, source, identity)
}

/// The unique source edge whose fate keeps a view pair's label between
/// preimages of its rows' view tables; `Err` with the candidate count.
fn preimage(
    src: &Schema,
    mig: &Migration,
    plan: &Plan,
    view: &Table,
    (label, src_row, tgt_row): &(SEdge, u32, u32),
) -> Result<SEdge, usize> {
    let (Some((ts, _)), Some((tt, _))) = (view.rows.get(src_row), view.rows.get(tgt_row)) else {
        return Err(0);
    };
    let cands: Vec<&SEdge> = src
        .edges
        .iter()
        .filter(|cand| {
            matches!(plan.fates.get(*cand), Some(EdgeFate::Keep(i)) if i == label)
                && mig.vertex_image(&cand.src) == ts
                && mig.vertex_image(&cand.tgt) == tt
        })
        .collect();
    match cands.as_slice() {
        [one] => Ok((*one).clone()),
        other => Err(other.len()),
    }
}

fn table_preimages(plan: &TablePlan, view_table: &str) -> Vec<Name> {
    plan.functor
        .tables
        .iter()
        .filter(|(_, target)| target.as_str() == view_table)
        .map(|(source, _)| source.clone())
        .collect()
}

/// The table lens.
pub struct TableLens<'a> {
    /// Source schema.
    src: &'a Schema,
    /// Target schema.
    tgt: &'a Schema,
    /// Source relation-ified schema.
    rs: &'a RelSchema,
    /// Migration.
    m: &'a Migration,
    /// Compiled table plan.
    tp: &'a TablePlan,
    /// Source protocol meanings for declared constraint sorts.
    constraint_semantics: BTreeMap<Name, crate::validation::Semantics>,
    /// Value layer (survival predicates).
    vl: &'a dyn ValueLayer,
    /// Exact identity of the validated endpoints, migration, and value layer.
    identity: IdentityEnvelope,
}

/// The relational endpoint context through which a table plan was compiled.
#[derive(Clone, Copy, Debug)]
pub struct TableBoundaries<'a> {
    /// Source relational boundary.
    pub source: &'a RelSchema,
    /// Target relational boundary.
    pub target: &'a RelSchema,
}

impl<'a> TableBoundaries<'a> {
    /// Pair the two relational endpoint contexts.
    #[must_use]
    pub const fn new(source: &'a RelSchema, target: &'a RelSchema) -> Self {
        Self { source, target }
    }
}

impl<'a> TableLens<'a> {
    /// Build a table lens only when its table plan was compiled for these
    /// exact source and target schemas and migration.
    ///
    /// # Errors
    ///
    /// [`PlanError::SourceSchemaMismatch`] or
    /// [`PlanError::TargetSchemaMismatch`] when a plan is reused across a
    /// different schema endpoint, or [`PlanError::MigrationMismatch`] when
    /// it is reused with a different migration.
    pub fn new(
        src: &'a Schema,
        tgt: &'a Schema,
        boundaries: TableBoundaries<'a>,
        m: &'a Migration,
        tp: &'a TablePlan,
        constraint_semantics: &BTreeMap<Name, crate::validation::Semantics>,
        vl: &'a dyn ValueLayer,
    ) -> Result<Self, TablePlanError> {
        tp.plan
            .validate_for(src, tgt, m)
            .map_err(|error| TablePlanError::Plan(Box::new(error)))?;
        if &tp.functor.src != boundaries.source {
            return Err(TablePlanError::SourceBoundaryMismatch);
        }
        if &tp.functor.tgt != boundaries.target {
            return Err(TablePlanError::TargetBoundaryMismatch);
        }
        let value_layer = vl.canonical_semantics();
        let identity = table_lens_identity(
            src,
            tgt,
            boundaries.source,
            boundaries.target,
            m,
            &value_layer,
        );
        Ok(Self {
            src,
            tgt,
            rs: boundaries.source,
            m,
            tp,
            constraint_semantics: constraint_semantics.clone(),
            vl,
            identity,
        })
    }

    /// Build a table lens only for the exact source and target relational
    /// boundaries through which its plan compiled.
    ///
    /// # Errors
    ///
    /// The endpoint or migration mismatch from [`PlanError`], followed by a
    /// source or target boundary mismatch.
    pub fn new_at_boundary(
        src: &'a Schema,
        tgt: &'a Schema,
        boundaries: TableBoundaries<'a>,
        m: &'a Migration,
        tp: &'a TablePlan,
        constraint_semantics: &BTreeMap<Name, crate::validation::Semantics>,
        vl: &'a dyn ValueLayer,
    ) -> Result<Self, TablePlanError> {
        Self::new(src, tgt, boundaries, m, tp, constraint_semantics, vl)
    }

    /// The exact lens identity expected on a checked put's complement.
    #[must_use]
    pub const fn identity(&self) -> &IdentityEnvelope {
        &self.identity
    }

    /// The compiled table plan this lens executes.
    pub(super) const fn plan(&self) -> &TablePlan {
        self.tp
    }

    /// Forward table-lens direction under this exact compiled boundary.
    ///
    /// # Errors
    ///
    /// The first restrict refusal. The returned complement is stamped with
    /// [`Self::identity`].
    pub fn get(&self, source: &Table) -> Result<(Table, TableComplement), RestrictError> {
        get_with_identity(
            self.src,
            self.tgt,
            self.m,
            self.tp,
            self.vl,
            source,
            self.identity,
        )
    }
}

impl TableLens<'_> {
    /// The source table of a view row.
    fn source_table(
        &self,
        c: &TableComplement,
        r: u32,
        view_table: &str,
    ) -> Result<Name, PutDomainError> {
        if let Some(t) = c.original_tables.get(&r)
            && self
                .tp
                .functor
                .tables
                .get(t)
                .is_some_and(|target| target == view_table)
        {
            return Ok(t.clone());
        }
        let pre = table_preimages(self.tp, view_table);
        match pre.as_slice() {
            [one] => Ok(one.clone()),
            _ => Err(PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Row(r),
                candidates: pre,
            }),
        }
    }

    /// The baseline source cells of a row: each column of its source table
    /// from the complement's dropped or original cell, else the same-named
    /// view cell.
    fn baseline(
        &self,
        comp: &TableComplement,
        row: u32,
        table: &str,
        view: &BTreeMap<String, Value>,
    ) -> BTreeMap<String, Value> {
        let mut s0 = BTreeMap::new();
        for col in self
            .rs
            .columns
            .get(table)
            .into_iter()
            .flat_map(|cs| cs.keys())
        {
            let key = (row, col.clone());
            if let Some(val) = comp
                .dropped_cells
                .get(&key)
                .or_else(|| comp.original_cells.get(&key))
                .or_else(|| view.get(col))
            {
                s0.insert(col.clone(), val.clone());
            }
        }
        s0
    }

    /// The baseline-and-pullback rule for one row.
    ///
    /// The baseline source cells `s0` give the baseline view: the cell
    /// fates applied to `s0`, or, for a row of a transformed table, to the
    /// value layer's transform of `s0` on the baseline source `g0`. Each
    /// target cell where the view differs is written to its writer (a cell
    /// with no writer is `DerivedCellEdited`). For a transformed row the
    /// written fields are the transformed ones, and the value layer
    /// restores the source fields from them (with the baseline as the
    /// recorded original when the transform changed it), its refusal being
    /// the row's `TransformNotInvertible` or `DerivedCellEdited`.
    fn cells(
        &self,
        row: u32,
        table: &str,
        s0: &BTreeMap<String, Value>,
        g0: &Graph,
        view: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, CellRestoreError> {
        let transformed = self.m.transformed.contains(table);
        let mut node = crate::model::Node::new(row, table);
        node.fields.clone_from(s0);
        let f0 = if transformed {
            match self.vl.transform(g0, &node) {
                Ok((f, _)) => f,
                Err(refusal) => {
                    return Err(CellRestoreError::Operational(
                        TableOperationalError::Transform {
                            elem: ViewRef::Row(row),
                            refusal,
                        },
                    ));
                }
            }
        } else {
            s0.clone()
        };
        let mut fields = f0.clone();
        let v0 = self.tp.apply(table, &f0);
        let changed: BTreeSet<&String> = view
            .keys()
            .chain(v0.keys())
            .filter(|col| view.get(*col) != v0.get(*col))
            .collect();
        if changed.is_empty() && !transformed {
            return Ok(s0.clone());
        }
        let mut errs = Vec::new();
        for target in changed {
            match self.tp.writer(table, target) {
                None => errs.push(PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Row(row),
                    field: target.clone(),
                }),
                Some(writer)
                    if !self
                        .rs
                        .columns
                        .get(table)
                        .is_some_and(|columns| columns.contains_key(&writer)) =>
                {
                    errs.push(PutDomainError::DerivedCellEdited {
                        elem: ViewRef::Row(row),
                        field: target.clone(),
                    });
                }
                Some(writer) => match view.get(target) {
                    Some(val) => {
                        fields.insert(writer, val.clone());
                    }
                    None => {
                        fields.remove(&writer);
                    }
                },
            }
        }
        if !errs.is_empty() {
            return Err(CellRestoreError::Domain(errs));
        }
        if !transformed {
            return Ok(fields);
        }
        let original = (f0 != *s0).then_some(s0);
        let mut written = crate::model::Node::new(row, table);
        written.fields = fields;
        self.vl
            .restore(&written, original, None)
            .map(|(restored, _)| restored)
            .map_err(|refusal| {
                CellRestoreError::Domain(vec![match refusal {
                    crate::restrict::PutFieldRefusal::DerivedCellEdited { field } => {
                        PutDomainError::DerivedCellEdited {
                            elem: ViewRef::Row(row),
                            field,
                        }
                    }
                    crate::restrict::PutFieldRefusal::TransformNotInvertible { field } => {
                        PutDomainError::TransformNotInvertible {
                            elem: ViewRef::Row(row),
                            field,
                        }
                    }
                }])
            })
    }

    /// Restore by the baseline-and-pullback rule, collecting every violation.
    ///
    /// # Errors
    ///
    /// Every violation of the put domain, sorted by report order, or an
    /// operational failure before a complete report exists.
    ///
    fn restore(
        &self,
        view: &Table,
        c: &TableComplement,
    ) -> Result<TableRestoration, TableOperationalError> {
        let mut errs = Vec::new();
        let mut out = Table::default();
        for id in view.rows.keys().filter(|r| c.dropped_rows.contains_key(r)) {
            errs.push(PutDomainError::IdCollision {
                elem: ViewRef::Row(*id),
            });
        }
        for (e, s, t) in &c.synthesized_fk_pairs {
            if !view.pairs.contains(&(e.clone(), *s, *t))
                && view.rows.contains_key(s)
                && view.rows.contains_key(t)
            {
                errs.push(PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Pair(Box::new(e.clone()), *s, *t),
                });
            }
        }
        let mut baselines: BTreeMap<u32, (Name, BTreeMap<String, Value>)> = BTreeMap::new();
        for (r, (vt, cells)) in &view.rows {
            match self.source_table(c, *r, vt) {
                Err(e) => errs.push(e),
                Ok(s) => {
                    let s0 = self.baseline(c, *r, &s, cells);
                    baselines.insert(*r, (s, s0));
                }
            }
        }
        self.restore_view_pairs(view, c, &baselines, &mut out, &mut errs);
        let removed: BTreeSet<u32> = c
            .dropped_fk_pairs
            .values()
            .flatten()
            .flat_map(|(s, t)| [*s, *t])
            .filter(|r| !c.dropped_rows.contains_key(r) && !view.rows.contains_key(r))
            .collect();
        out.rows.extend(
            c.dropped_rows
                .iter()
                .map(|(id, dropped)| (*id, (dropped.table.clone(), dropped.row.clone()))),
        );
        for (e, ps) in &c.dropped_fk_pairs {
            for (s, t) in ps {
                if !removed.contains(s) && !removed.contains(t) {
                    out.pairs.insert((e.clone(), *s, *t));
                }
            }
        }
        // The baseline source: every view row at its baseline cells, the
        // dropped rows and the restored pairs; the value layer's transforms
        // read it.
        let mut source0 = out.clone();
        source0.rows.extend(baselines.clone());
        let baseline_rows = &source0.rows;
        source0.pairs.retain(|(_, source, target)| {
            baseline_rows.contains_key(source) && baseline_rows.contains_key(target)
        });
        let (g0, _) = table_to_graph(&source0).map_err(TableOperationalError::BaselineEncoding)?;
        for (r, (s, s0)) in baselines {
            match self.cells(r, &s, &s0, &g0, &view.rows[&r].1) {
                Ok(restored) => {
                    out.rows.insert(r, (s, restored));
                }
                Err(CellRestoreError::Domain(errors)) => errs.extend(errors),
                Err(CellRestoreError::Operational(error)) => return Err(error),
            }
        }
        let mut evaluable = out.clone();
        let evaluable_rows = &evaluable.rows;
        evaluable.pairs.retain(|(_, source, target)| {
            evaluable_rows.contains_key(source) && evaluable_rows.contains_key(target)
        });
        let source_errors = self.source_violations(view, &evaluable)?;
        let predicate_refused = source_errors
            .iter()
            .any(|error| matches!(error, PutDomainError::SurvivalPredicateRefused { .. }));
        errs.extend(source_errors);
        let has_every_view_row = view.rows.keys().all(|row| out.rows.contains_key(row));
        if has_every_view_row && !predicate_refused {
            errs.extend(self.forward_violations(view, c, &out)?);
        }
        errs.sort_by(PutDomainError::report_cmp);
        errs.dedup();
        let unresolved_collations = self.candidate_named_collations(view, c);
        Ok(TableRestoration {
            source: out,
            violations: errs,
            unresolved_collations,
        })
    }

    fn restore_view_pairs(
        &self,
        view: &Table,
        c: &TableComplement,
        baselines: &BTreeMap<u32, (Name, BTreeMap<String, Value>)>,
        out: &mut Table,
        errs: &mut Vec<PutDomainError>,
    ) {
        for pair in &view.pairs {
            let candidates: Vec<SEdge> = baselines
                .get(&pair.1)
                .zip(baselines.get(&pair.2))
                .map_or_else(Vec::new, |((source_table, _), (target_table, _))| {
                    self.src
                        .edges
                        .iter()
                        .filter(|candidate| {
                            candidate.src == *source_table
                                && candidate.tgt == *target_table
                                && matches!(
                                    self.tp.plan.fates.get(*candidate),
                                    Some(EdgeFate::Keep(image)) if image == &pair.0
                                )
                        })
                        .cloned()
                        .collect()
                });
            let recorded = c.original_fk_edges.get(pair).filter(|sources| {
                !sources.is_empty() && sources.iter().all(|source| candidates.contains(source))
            });
            if let Some(srcs) = recorded {
                out.pairs
                    .extend(srcs.iter().map(|e| (e.clone(), pair.1, pair.2)));
                continue;
            }
            if c.synthesized_fk_pairs.contains(pair) {
                continue;
            }
            match candidates.as_slice() {
                [e] => {
                    out.pairs.insert((e.clone(), pair.1, pair.2));
                }
                [] if self.contracted(view, pair) => {
                    errs.push(PutDomainError::ContractedEdgeInserted {
                        elem: ViewRef::Pair(Box::new(pair.0.clone()), pair.1, pair.2),
                    });
                }
                _ => errs.push(PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Pair(Box::new(pair.0.clone()), pair.1, pair.2),
                }),
            }
        }
    }

    /// Definition: The complete table put-domain report.
    ///
    /// Complement identity is a precondition of checked put rather than a
    /// put-domain variant, and is checked separately by [`put_checked`].
    ///
    /// # Errors
    ///
    /// Every put-domain violation in deterministic report order.
    pub fn check_put_domain(
        &self,
        view: &Table,
        complement: &TableComplement,
    ) -> Result<(), TableCheckError> {
        check_put_domain(self, view, complement)
    }

    /// Compatibility entrypoint returning the complete table put-domain
    /// report.
    ///
    /// New checked callers use the module-level [`put`], whose error is the
    /// first ordered violation after identity verification.
    ///
    /// # Errors
    ///
    /// Every put-domain violation in report order.
    pub fn put(&self, view: &Table, complement: &TableComplement) -> Result<Table, TablePutError> {
        put(self, view, complement)
    }

    fn contracted(&self, view: &Table, (label, src_row, tgt_row): &(SEdge, u32, u32)) -> bool {
        let anchor = |row: &u32| {
            view.rows
                .get(row)
                .map(|(table, _)| table_preimages(self.tp, table))
        };
        let (Some(from), Some(to)) = (anchor(src_row), anchor(tgt_row)) else {
            return false;
        };
        from.iter().any(|one| {
            to.iter().any(|two| {
                self.tp
                    .plan
                    .pairs
                    .get(&(one.clone(), two.clone()))
                    .is_some_and(|pair| pair.resolution == PairResolution::Label(label.clone()))
            })
        })
    }

    /// Survival predicates of restored view rows and declared keys, read on
    /// the restored source as a graph.
    fn source_violations(
        &self,
        view: &Table,
        out: &Table,
    ) -> Result<Vec<PutDomainError>, TableOperationalError> {
        let (g, _) = table_to_graph(out).map_err(TableOperationalError::BaselineEncoding)?;
        let mut errs = Vec::new();
        for (r, n) in g.nodes.iter().filter(|(r, _)| view.rows.contains_key(r)) {
            if self.m.conditional.contains(&n.anchor) {
                match self.vl.survives(&g, n) {
                    Ok(true) => {}
                    Ok(false) => errs.push(PutDomainError::SurvivalPredicateFails {
                        elem: ViewRef::Row(*r),
                    }),
                    Err(refusal) => errs.push(PutDomainError::SurvivalPredicateRefused {
                        elem: ViewRef::Row(*r),
                        refusal,
                    }),
                }
            }
        }
        errs.extend(required_value_violations(
            self.src,
            &self.constraint_semantics,
            &g,
            ViewRef::Row,
        ));
        let graph_lens_errs = key_violations(self.src, &g, view);
        errs.extend(graph_lens_errs);
        errs.sort_by(PutDomainError::report_cmp);
        errs.dedup();
        Ok(errs)
    }

    /// Recompute get on the restored source. This is the independent final
    /// guard for transformed coordinates whose inverses are locally defined
    /// but whose other outputs change when one source cell changes.
    fn forward_violations(
        &self,
        view: &Table,
        complement: &TableComplement,
        out: &Table,
    ) -> Result<Vec<PutDomainError>, TableOperationalError> {
        let regenerated = match self.get(out) {
            Ok((regenerated, _)) => regenerated,
            Err(error) => return Err(TableOperationalError::ForwardGet(error)),
        };
        let mut errors = Vec::new();
        for (row, (view_table, view_cells)) in &view.rows {
            let Some((regenerated_table, regenerated_cells)) = regenerated.rows.get(row) else {
                errors.push(PutDomainError::SurvivalPredicateFails {
                    elem: ViewRef::Row(*row),
                });
                continue;
            };
            if regenerated_table != view_table {
                continue;
            }
            let Some((source_table, _)) = out.rows.get(row) else {
                continue;
            };
            let fields: BTreeSet<&String> = view_cells
                .keys()
                .chain(regenerated_cells.keys())
                .filter(|field| view_cells.get(*field) != regenerated_cells.get(*field))
                .collect();
            for field in fields {
                let writer = self.tp.writer(source_table, field);
                let is_source_column = writer.as_ref().is_some_and(|writer| {
                    self.rs
                        .columns
                        .get(source_table)
                        .is_some_and(|columns| columns.contains_key(writer))
                });
                errors.push(if is_source_column {
                    PutDomainError::TransformNotInvertible {
                        elem: ViewRef::Row(*row),
                        field: field.clone(),
                    }
                } else {
                    PutDomainError::DerivedCellEdited {
                        elem: ViewRef::Row(*row),
                        field: field.clone(),
                    }
                });
            }
        }
        for pair in &view.pairs {
            if !regenerated.pairs.contains(pair) && !complement.synthesized_fk_pairs.contains(pair)
            {
                errors.push(PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Pair(Box::new(pair.0.clone()), pair.1, pair.2),
                });
            }
        }
        Ok(errors)
    }

    /// Named collations applicable to at least one coherent restoration
    /// candidate. Source-table choices are enumerated jointly, so incident
    /// pairs, scoped keys, and structural paths are evaluated on one graph.
    /// Each candidate runs the ordinary baseline, transform, writer, restore,
    /// and forward-stability rules. A refusal or mismatch discards that
    /// candidate rather than contributing a collation name.
    fn candidate_named_collations(
        &self,
        view: &Table,
        complement: &TableComplement,
    ) -> BTreeSet<String> {
        let rows: Vec<(u32, Vec<Name>)> = view
            .rows
            .iter()
            .map(|(row, (view_table, _))| {
                let choices = complement
                    .original_tables
                    .get(row)
                    .filter(|table| {
                        self.tp
                            .functor
                            .tables
                            .get(*table)
                            .is_some_and(|target| target == view_table)
                    })
                    .map_or_else(
                        || table_preimages(self.tp, view_table),
                        |table| vec![table.clone()],
                    );
                (*row, choices)
            })
            .collect();
        if rows.iter().any(|(_, choices)| choices.is_empty()) {
            return BTreeSet::new();
        }
        let mut names = BTreeSet::new();
        let mut indices = vec![0; rows.len()];
        'candidates: loop {
            let assignment: BTreeMap<u32, Name> = rows
                .iter()
                .zip(&indices)
                .map(|((row, choices), index)| (*row, choices[*index].clone()))
                .collect();
            if let Some(candidate) = self.restore_collation_candidate(view, complement, &assignment)
            {
                names.extend(named_collations_unresolved(self, &candidate));
            }
            for at in (0..rows.len()).rev() {
                indices[at] += 1;
                if indices[at] < rows[at].1.len() {
                    continue 'candidates;
                }
                indices[at] = 0;
            }
            break;
        }
        names
    }

    /// Restore one complete source-table assignment for collation analysis.
    /// `None` means that the assignment is not a feasible put candidate.
    fn restore_collation_candidate(
        &self,
        view: &Table,
        complement: &TableComplement,
        assignment: &BTreeMap<u32, Name>,
    ) -> Option<Table> {
        let baselines: BTreeMap<u32, (Name, BTreeMap<String, Value>)> = assignment
            .iter()
            .map(|(row, table)| {
                let view_cells = &view.rows.get(row)?.1;
                Some((
                    *row,
                    (
                        table.clone(),
                        self.baseline(complement, *row, table, view_cells),
                    ),
                ))
            })
            .collect::<Option<_>>()?;

        let mut candidate = Table::default();
        let mut pair_errors = Vec::new();
        self.restore_view_pairs(
            view,
            complement,
            &baselines,
            &mut candidate,
            &mut pair_errors,
        );
        if !pair_errors.is_empty() {
            return None;
        }

        let removed: BTreeSet<u32> = complement
            .dropped_fk_pairs
            .values()
            .flatten()
            .flat_map(|(source, target)| [*source, *target])
            .filter(|row| {
                !complement.dropped_rows.contains_key(row) && !view.rows.contains_key(row)
            })
            .collect();
        candidate.rows.extend(
            complement
                .dropped_rows
                .iter()
                .map(|(row, dropped)| (*row, (dropped.table.clone(), dropped.row.clone()))),
        );
        for (edge, pairs) in &complement.dropped_fk_pairs {
            for (source, target) in pairs {
                if !removed.contains(source) && !removed.contains(target) {
                    candidate.pairs.insert((edge.clone(), *source, *target));
                }
            }
        }

        let mut baseline_source = candidate.clone();
        baseline_source.rows.extend(baselines.clone());
        let (baseline_graph, _) = table_to_graph(&baseline_source).ok()?;
        for (row, (table, baseline)) in baselines {
            let view_cells = &view.rows.get(&row)?.1;
            let restored = self
                .cells(row, &table, &baseline, &baseline_graph, view_cells)
                .ok()?;
            candidate.rows.insert(row, (table, restored));
        }
        table_to_graph(&candidate).ok()?;
        match self.forward_violations(view, complement, &candidate) {
            Ok(errors) if errors.is_empty() => Some(candidate),
            Ok(_) | Err(_) => None,
        }
    }
}

fn verify_identity(
    lens: &TableLens<'_>,
    complement: &TableComplement,
    _policy: IdentityPolicy,
) -> Result<IdentityStatus, IdentityError> {
    match &complement.identity {
        Some(found) if found == lens.identity() => Ok(IdentityStatus::Verified),
        Some(found) => Err(IdentityError::Mismatch {
            expected: Box::new(*lens.identity()),
            found: Box::new(*found),
        }),
        None if complement.records_nothing() => Ok(IdentityStatus::Unbound),
        None => Err(IdentityError::UnboundComplement),
    }
}

fn named_collations_unresolved(lens: &TableLens<'_>, source: &Table) -> BTreeSet<String> {
    let Ok((graph, _)) = table_to_graph(source) else {
        return BTreeSet::new();
    };
    let mut names = BTreeSet::new();
    for (vertex, keys) in &lens.src.keys {
        for key in keys.iter().filter(|key| uniqueness_role(key.role)) {
            if key.equality.len() != key.parts.len() {
                continue;
            }
            for (id, node) in &graph.nodes {
                if node.anchor != *vertex
                    || node.shape == crate::model::NodeShape::HyperEdge
                    || matches!(
                        &key.scope,
                        KeyScope::Within(ancestor)
                            if crate::keys::nearest_ancestor(&graph, *id, ancestor).is_none()
                    )
                {
                    continue;
                }
                let Some(values) = crate::keys::key_value(&graph, *id, key) else {
                    continue;
                };
                for (equality, value) in key.equality.iter().zip(values) {
                    if let (
                        KeyEquality::Sql {
                            collation: SqlCollation::Named(name),
                            ..
                        },
                        Value::Str(_),
                    ) = (equality, value)
                    {
                        names.insert(name.clone());
                    }
                }
            }
        }
    }
    names
}

/// Definition: Every known violation of the table put domain, in complete
/// deterministic order.
///
/// The reference evaluator does not guess the meaning of a named database
/// collation. When no known violation precedes that unresolved equality,
/// the result is [`TableCheckError::NamedCollationUnresolved`].
///
/// # Errors
///
/// Every known violation, or the exact set of unresolved collation names.
pub fn check_put_domain(
    lens: &TableLens<'_>,
    view: &Table,
    complement: &TableComplement,
) -> Result<(), TableCheckError> {
    match lens.restore(view, complement) {
        Err(error) => Err(TableCheckError::Operational(error)),
        Ok(restoration) if !restoration.violations.is_empty() => Err(TableCheckError::Domain {
            violations: restoration.violations,
            unresolved_collations: restoration.unresolved_collations,
        }),
        Ok(restoration) => {
            if restoration.unresolved_collations.is_empty() {
                Ok(())
            } else {
                Err(TableCheckError::NamedCollationUnresolved {
                    names: restoration.unresolved_collations,
                })
            }
        }
    }
}

/// Backward table-lens direction under the default identity policy.
///
/// Identity errors precede restoration. A put-domain failure returns only
/// the first error from [`check_put_domain`]'s complete deterministic vector.
///
/// # Errors
///
/// A complement-identity refusal, first put-domain violation, or unresolved
/// named collation.
pub fn put(
    lens: &TableLens<'_>,
    view: &Table,
    complement: &TableComplement,
) -> Result<Table, TablePutError> {
    put_checked(lens, view, complement, IdentityPolicy::AllowLegacy).map(|(source, _)| source)
}

/// Backward table-lens direction under an explicit identity policy.
///
/// # Errors
///
/// As [`put`], returning the accepted identity status on success.
pub fn put_checked(
    lens: &TableLens<'_>,
    view: &Table,
    complement: &TableComplement,
    policy: IdentityPolicy,
) -> Result<(Table, IdentityStatus), TablePutError> {
    let status = verify_identity(lens, complement, policy).map_err(TablePutError::Identity)?;
    let restoration = lens
        .restore(view, complement)
        .map_err(TablePutError::Operational)?;
    if let Some(first) = restoration.violations.into_iter().next() {
        return Err(TablePutError::Domain(first));
    }
    if !restoration.unresolved_collations.is_empty() {
        return Err(TablePutError::NamedCollationUnresolved {
            names: restoration.unresolved_collations,
        });
    }
    Ok((restoration.source, status))
}

/// A deterministic row id for a single writer adding to a table view.
///
/// Allocation starts at the complement's floor and selects the least id not
/// used by a visible or hidden row. `u32::MAX` is the exhaustion sentinel and
/// is never allocated.
///
/// # Errors
///
/// [`RowIdsExhausted`] when no allocatable id remains.
pub fn fresh_view_row_id(
    view: &Table,
    complement: &TableComplement,
) -> Result<u32, RowIdsExhausted> {
    let mut candidate = complement.id_floor;
    loop {
        if candidate == u32::MAX {
            return Err(RowIdsExhausted);
        }
        if !view.rows.contains_key(&candidate) && !complement.dropped_rows.contains_key(&candidate)
        {
            return Ok(candidate);
        }
        candidate = candidate.checked_add(1).ok_or(RowIdsExhausted)?;
    }
}

/// The first static obstruction to reconstructing a table source without a
/// complement.
///
/// This is complete for the finite reference table language: tables,
/// columns and relations must form bijections, and no conditional or
/// transformed coordinate may require source residue.
#[must_use]
pub fn obstruction_to_isomorphism(lens: &TableLens<'_>) -> Option<String> {
    if let Some(table) = lens.m.conditional.iter().next() {
        return Some(format!(
            "table `{table}` survives conditionally, so some rows are dropped"
        ));
    }
    if let Some(table) = lens.m.transformed.iter().next() {
        return Some(format!(
            "table `{table}` has a value transform whose inverse belongs to the boundary layer"
        ));
    }

    let source_tables = &lens.tp.functor.src.tables;
    let target_tables = &lens.tp.functor.tgt.tables;
    if lens.tp.functor.tables.len() != source_tables.len() {
        let dropped = source_tables
            .iter()
            .find(|table| !lens.tp.functor.tables.contains_key(*table))?;
        return Some(format!("table `{dropped}` is dropped"));
    }
    let table_images: BTreeSet<&Name> = lens.tp.functor.tables.values().collect();
    if table_images.len() != lens.tp.functor.tables.len()
        || table_images.len() != target_tables.len()
    {
        return Some("the table map is not a bijection".to_owned());
    }

    for source_table in source_tables {
        let target_table = &lens.tp.functor.tables[source_table];
        let source_columns = lens
            .tp
            .functor
            .src
            .columns
            .get(source_table)
            .map_or_else(BTreeSet::new, |columns| columns.keys().cloned().collect());
        let mut outputs = BTreeSet::new();
        for source_column in source_columns {
            let output = lens
                .tp
                .fates
                .get(source_table)
                .into_iter()
                .flatten()
                .find_map(|fate| match fate {
                    CellFate::Drop(source) if source == &source_column => Some(None),
                    CellFate::Rename(source, target) if source == &source_column => {
                        Some(Some(target.clone()))
                    }
                    CellFate::Drop(_) | CellFate::Rename(_, _) => None,
                })
                .unwrap_or_else(|| Some(source_column.clone()));
            let Some(output) = output else {
                return Some(format!(
                    "column `{source_table}.{source_column}` is dropped"
                ));
            };
            if lens.tp.writer(source_table, &output).as_deref() != Some(source_column.as_str()) {
                return Some(format!(
                    "column `{source_table}.{source_column}` is not the last writer of `{output}`"
                ));
            }
            if !outputs.insert(output) {
                return Some(format!(
                    "columns of table `{source_table}` have a shared output"
                ));
            }
        }
        let target_columns: BTreeSet<String> = lens
            .tp
            .functor
            .tgt
            .columns
            .get(target_table)
            .map_or_else(BTreeSet::new, |columns| columns.keys().cloned().collect());
        if outputs != target_columns {
            return Some(format!(
                "columns of table `{source_table}` do not cover target table `{target_table}`"
            ));
        }
    }

    let relation_images: BTreeSet<&SEdge> = lens.tp.functor.relations.values().collect();
    if lens.tp.functor.relations.len() != lens.tp.functor.src.relations.len()
        || relation_images.len() != lens.tp.functor.relations.len()
        || relation_images.len() != lens.tp.functor.tgt.relations.len()
    {
        return Some("the foreign-key map is not a bijection".to_owned());
    }
    if !lens.tp.plan.pairs.is_empty() {
        return Some("the table plan synthesizes contracted pairs".to_owned());
    }
    None
}

/// Whether every schema-supported table source is determined by its view.
#[must_use]
pub fn is_isomorphism(lens: &TableLens<'_>) -> bool {
    obstruction_to_isomorphism(lens).is_none()
}

/// Reconstruct through a table isomorphism without a stored complement.
///
/// # Errors
///
/// [`TablePutError::NotAnIsomorphism`] for a lossy lens, or an ordinary
/// checked-put refusal.
pub fn put_without_complement(lens: &TableLens<'_>, view: &Table) -> Result<Table, TablePutError> {
    if let Some(detail) = obstruction_to_isomorphism(lens) {
        return Err(TablePutError::NotAnIsomorphism { detail });
    }
    let mut complement = TableComplement::empty();
    complement.identity = Some(*lens.identity());
    put(lens, view, &complement)
}

fn key_violations(src: &Schema, g: &Graph, view: &Table) -> Vec<PutDomainError> {
    let mut errs = Vec::new();
    for (vertex, ks) in &src.keys {
        for key in ks {
            if uniqueness_role(key.role) {
                let (buckets, _) = crate::keys::buckets(g, vertex, key).unwrap_or_default();
                for (values, elems) in buckets.into_values().filter(|(_, e)| e.len() > 1) {
                    let key_cols = key.parts.iter().map(part_column).zip(values).collect();
                    errs.push(PutDomainError::DuplicateKey {
                        table: vertex.clone(),
                        key: key_cols,
                        elems: elems.into_iter().map(ViewRef::Row).collect(),
                    });
                }
            }
            if key.role == KeyRole::Primary {
                for id in g
                    .nodes
                    .iter()
                    .filter(|(i, n)| n.anchor == *vertex && view.rows.contains_key(i))
                    .map(|(i, _)| i)
                {
                    for p in &key.parts {
                        if !key_part_present(g, *id, p) {
                            errs.push(PutDomainError::MissingRequiredCell {
                                elem: ViewRef::Row(*id),
                                field: part_column(p),
                            });
                        }
                    }
                }
            }
        }
    }
    errs
}

/// Whether a synthesized key names a contracted pair (tables synthesize no
/// node).
#[must_use]
pub const fn is_pair_key(k: &SynthKey) -> bool {
    matches!(k, SynthKey::Contract { .. })
}
