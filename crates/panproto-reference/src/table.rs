//! The table lens: rows and pairs restricted as graph nodes and edges, cells
//! through the cell fates of the attribute arrows, the table complement,
//! and put by the baseline-and-pullback rule.
//!
//! The value layer ([`ValueLayer`]) supplies survival predicates and the
//! transforms of transformed tables: get reads cells after the transforms,
//! and put pulls an edited cell back through its writer and then through
//! the value layer's restoration; a cell with no writer is derived.

use std::collections::{BTreeMap, BTreeSet};

use crate::encode::table_to_graph;
use crate::lens::{PutDomainError, ViewRef};
use crate::migration::Migration;
use crate::model::{Graph, Name, SEdge, Table};
use crate::rel::{FunctorError, RelFunctor, RelSchema, entity_functor};
use crate::restrict::{
    self, EdgeFate, PairResolution, Plan, PlanError, RestrictError, SynthKey, ValueLayer,
    reverse_anchor,
};
use crate::schema::Schema;
use crate::value::Value;

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
}

/// The compiled table plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePlan {
    /// The restrict plan.
    pub plan: Plan,
    /// The entity functor.
    pub functor: RelFunctor,
    /// Per kept source table, the fates of its columns in column order.
    pub fates: BTreeMap<Name, Vec<CellFate>>,
}

impl TablePlan {
    /// The writer of target column `t` of rows of source table `s`: the
    /// source of the last `Rename(_, t)` in fate order, else `t` itself
    /// when `t` has no fate, else none (a derived cell).
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

/// Compile the table plan.
///
/// The restrict plan, the entity functor, the refusal of expansions and hyper-edge resolution, the
/// agreement check of the two survival rules (tables ascending, then relations and attribute arrows
/// of tables both keep, ascending), and the cell fates.
///
/// # Errors
///
/// The first [`TablePlanError`] in that order.
pub fn compile_for_tables(
    src: &Schema,
    tgt: &Schema,
    rs: &RelSchema,
    rt: &RelSchema,
    m: &Migration,
) -> Result<TablePlan, TablePlanError> {
    let plan = restrict::compile(src, tgt, m).map_err(|e| TablePlanError::Plan(Box::new(e)))?;
    let functor = entity_functor(rs, rt, m).map_err(|e| TablePlanError::Functor(Box::new(e)))?;
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

/// The table complement.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TableComplement {
    /// Dropped rows with their table and cells.
    pub dropped_rows: BTreeMap<u32, (Name, BTreeMap<String, Value>)>,
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
}

/// Table get: rows and pairs restricted as graph nodes and edges (graph restrict and
/// contraction), then the cell fates of each surviving row's source table,
/// and the table complement.
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
    let (graph, _) = table_to_graph(source);
    let (vg, gc) = restrict::get_with_plan(src, tgt, mig, &tp.plan, vl, &graph)?;
    let mut view = Table::default();
    let mut comp = TableComplement {
        id_floor: gc.floor.0,
        ..TableComplement::default()
    };
    for (id, node) in &vg.nodes {
        let (stable, source_cells) = &source.rows[id];
        view.rows
            .insert(*id, (node.anchor.clone(), tp.apply(stable, &node.fields)));
        if reverse_anchor(src, mig, &node.anchor) != vec![stable.clone()] {
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
        comp.dropped_rows
            .insert(*id, (node.anchor.clone(), node.fields.clone()));
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

/// The table lens.
pub struct TableLens<'a> {
    /// Source schema.
    pub src: &'a Schema,
    /// Target schema.
    pub tgt: &'a Schema,
    /// Source relation-ified schema.
    pub rs: &'a RelSchema,
    /// Migration.
    pub m: &'a Migration,
    /// Compiled table plan.
    pub tp: &'a TablePlan,
    /// Value layer (survival predicates).
    pub vl: &'a dyn ValueLayer,
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
            && self.m.vertex_image(t) == view_table
        {
            return Ok(t.clone());
        }
        let pre = reverse_anchor(self.src, self.m, view_table);
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
    ) -> Result<BTreeMap<String, Value>, Vec<PutDomainError>> {
        let transformed = self.m.transformed.contains(table);
        let mut node = crate::model::Node::new(row, table);
        node.fields.clone_from(s0);
        let f0 = if transformed {
            match self.vl.transform(g0, &node) {
                Ok((f, _)) => f,
                Err(refusal) => {
                    return Err(vec![PutDomainError::BaselineRefused {
                        elem: ViewRef::Row(row),
                        refusal,
                    }]);
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
        if changed.is_empty() {
            return Ok(s0.clone());
        }
        let mut errs = Vec::new();
        for target in changed {
            match self.tp.writer(table, target) {
                None => errs.push(PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Row(row),
                    field: target.clone(),
                }),
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
            return Err(errs);
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
                vec![match refusal {
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
                }]
            })
    }

    /// Put by the baseline-and-pullback rule, collecting every violation.
    ///
    /// # Errors
    ///
    /// Every violation of the put domain, sorted by report order.
    pub fn put(&self, view: &Table, c: &TableComplement) -> Result<Table, Vec<PutDomainError>> {
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
        for pair in &view.pairs {
            if let Some(srcs) = c.original_fk_edges.get(pair) {
                out.pairs
                    .extend(srcs.iter().map(|e| (e.clone(), pair.1, pair.2)));
                continue;
            }
            if c.synthesized_fk_pairs.contains(pair) {
                continue;
            }
            match preimage(self.src, self.m, &self.tp.plan, view, pair) {
                Ok(e) => {
                    out.pairs.insert((e, pair.1, pair.2));
                }
                Err(0) if self.contracted(view, pair) => {
                    errs.push(PutDomainError::ContractedEdgeInserted {
                        elem: ViewRef::Pair(Box::new(pair.0.clone()), pair.1, pair.2),
                    });
                }
                Err(_) => errs.push(PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Pair(Box::new(pair.0.clone()), pair.1, pair.2),
                }),
            }
        }
        let removed: BTreeSet<u32> = c
            .dropped_fk_pairs
            .values()
            .flatten()
            .flat_map(|(s, t)| [*s, *t])
            .filter(|r| !c.dropped_rows.contains_key(r) && !view.rows.contains_key(r))
            .collect();
        out.rows
            .extend(c.dropped_rows.iter().map(|(i, r)| (*i, r.clone())));
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
        let (g0, _) = table_to_graph(&source0);
        for (r, (s, s0)) in baselines {
            match self.cells(r, &s, &s0, &g0, &view.rows[&r].1) {
                Ok(restored) => {
                    out.rows.insert(r, (s, restored));
                }
                Err(e) => errs.extend(e),
            }
        }
        errs.extend(self.source_violations(view, &out));
        if errs.is_empty() {
            Ok(out)
        } else {
            errs.sort_by_key(PutDomainError::order_key);
            Err(errs)
        }
    }

    fn contracted(&self, view: &Table, (label, src_row, tgt_row): &(SEdge, u32, u32)) -> bool {
        let anchor = |row: &u32| {
            view.rows
                .get(row)
                .map(|(tb, _)| reverse_anchor(self.src, self.m, tb))
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
    fn source_violations(&self, view: &Table, out: &Table) -> Vec<PutDomainError> {
        let (g, _) = table_to_graph(out);
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
        let graph_lens_errs = key_violations(self.src, &g, view);
        errs.extend(graph_lens_errs);
        errs
    }
}

fn key_violations(src: &Schema, g: &Graph, view: &Table) -> Vec<PutDomainError> {
    let mut errs = Vec::new();
    for (vertex, ks) in &src.keys {
        for key in ks {
            let (buckets, _) = crate::keys::buckets(g, vertex, key);
            for (values, elems) in buckets.into_values().filter(|(_, e)| e.len() > 1) {
                let cols = key.parts.iter().map(|p| match p {
                    crate::schema::KeyPart::Path(path) => path
                        .last()
                        .map(|e| e.column().to_owned())
                        .unwrap_or_default(),
                    crate::schema::KeyPart::Position { within } => within.column().to_owned(),
                });
                let key_cols = cols
                    .zip(
                        values
                            .iter()
                            .map(|v| crate::canonical::value_bytes(v).unwrap_or_default()),
                    )
                    .collect();
                errs.push(PutDomainError::DuplicateKey {
                    table: vertex.clone(),
                    key: key_cols,
                    elems: elems.into_iter().map(ViewRef::Row).collect(),
                });
            }
            if key.role == crate::schema::KeyRole::Primary {
                for id in g
                    .nodes
                    .iter()
                    .filter(|(i, n)| n.anchor == *vertex && view.rows.contains_key(i))
                    .map(|(i, _)| i)
                {
                    for p in &key.parts {
                        if let crate::schema::KeyPart::Path(path) = p
                            && crate::keys::path_value(g, *id, path).is_none()
                        {
                            let field = path
                                .last()
                                .map(|e| e.column().to_owned())
                                .unwrap_or_default();
                            errs.push(PutDomainError::MissingRequiredCell {
                                elem: ViewRef::Row(*id),
                                field,
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
