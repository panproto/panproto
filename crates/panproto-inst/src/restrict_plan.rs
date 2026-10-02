//! The compiled plan of a restrict, shared by the tree, table and graph
//! engines.
//!
//! A [`RestrictPlan`] is a pure function of the source schema, the target
//! schema and the compiled migration (and, for its boundary layer, of the
//! two protocols). It holds the image of every source schema edge
//! ([`EdgeImage`]), the contraction plan ([`ContractionPlan`]) and, when
//! the protocols classify both schemas into tables and columns, the cell
//! layer ([`CellPlan`]) and, for tables, the table functor of the migration
//! ([`EntityFunctor`]). The three engines
//! ([`wtype_restrict_with_plan`](crate::wtype_restrict_with_plan),
//! [`functor_restrict_with_plan`](crate::functor_restrict_with_plan),
//! [`graph_restrict_with_plan`](crate::graph_restrict_with_plan)) read one
//! plan and share the helpers here, so they agree by construction wherever
//! their inputs are the same data.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema, ValueSpec};
use serde::{Deserialize, Serialize};

use crate::contraction_plan::{
    ContractionPlan, PairResolution, UnresolvedReason, failing_hop, incoherent_label, remap,
    resolve_pair, survives,
};
use crate::error::RestrictError;
use crate::identity::{Digest, schema_digest};
use crate::metadata::Node;
use crate::migration_functor::EntityFunctor;
use crate::rel_schema::RelSchema;
use crate::restrict_boundary::BoundaryProtocols;
use crate::segment::{ErrorKey, IdKind};
use crate::value::coerce::coerce_value;
use crate::value::{Value, ValueSite};
use crate::view_ref::SynthKey;
use crate::wtype::{
    CompiledMigration, KeyTrack, TransformContext, apply_transforms_tracked, direct_source_edge_image,
};

/// An element of the source schema on which the survival rule of the
/// restrict and the table functor of a migration disagree.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalElement {
    /// A source table.
    Table(Name),
    /// A source relation or attribute arrow.
    Edge(Edge),
}

impl std::fmt::Display for SurvivalElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Table(table) => write!(f, "table `{table}`"),
            Self::Edge(edge) => write!(f, "edge {edge:?}"),
        }
    }
}

/// What a restrict does with the elements of one source schema edge whose
/// endpoints survive.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeFate {
    /// Each element is kept, under its own id and position, relabelled to
    /// this target edge.
    Keep(Edge),
    /// Each element is replaced by a path of synthesized elements through
    /// `intermediates`, along `labels` (one more label than intermediates).
    Expand {
        /// The anchors of the synthesized inner nodes.
        intermediates: Vec<Name>,
        /// The target edges of the path.
        labels: Vec<Edge>,
    },
    /// Each element is dropped, alone (no subtree goes with it).
    Drop,
    /// The edge's image is not determined: an element of it fails the
    /// restrict.
    Unresolved(UnresolvedReason),
}

/// The fate of every source schema edge.
///
/// Serialized as a sequence of `(edge, fate)` entries in edge order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeImage {
    /// Every source schema edge, with its fate.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub fates: BTreeMap<Edge, EdgeFate>,
}

impl EdgeImage {
    /// Compile the image of every source schema edge.
    ///
    /// An edge with an endpoint vertex that does not survive (its image
    /// under the vertex map is no surviving vertex) is [`EdgeFate::Drop`].
    /// Otherwise, with `a'` and `b'` the images of its endpoints, the fate
    /// is the tree's decision for a direct arc: the edge's direct image
    /// (its `edge_remap` entry; else the target edge between `a'` and `b'`
    /// with its kind and name; else, when the source has no other edge
    /// between its endpoints, the resolver's edge for `(a', b')`) gives
    /// [`EdgeFate::Keep`]; failing that, when the migration records an
    /// expansion path for `(a', b')`, the pair resolution
    /// ([`resolve_pair`]) gives `Keep`, [`EdgeFate::Expand`] or
    /// [`EdgeFate::Unresolved`]; failing that, the edge is `Drop`.
    ///
    /// # Errors
    ///
    /// [`RestrictError::IncoherentEdgeImage`] for the least source edge
    /// whose kept image, or one of whose expansion labels, is not a target
    /// edge joining the anchors it stands between (`a'` to `b'` for a kept
    /// image; consecutive anchors of the path for a label).
    pub fn compile(
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        let mut edges: Vec<&Edge> = src.edges.keys().collect();
        edges.sort_unstable();
        let mut fates = BTreeMap::new();
        for edge in edges {
            let fate = Self::fate_of(src, tgt, migration, edge)?;
            fates.insert(edge.clone(), fate);
        }
        Ok(Self { fates })
    }

    fn fate_of(
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
        edge: &Edge,
    ) -> Result<EdgeFate, RestrictError> {
        if !survives(migration, &edge.src) || !survives(migration, &edge.tgt) {
            return Ok(EdgeFate::Drop);
        }
        let a = remap(migration, &edge.src);
        let b = remap(migration, &edge.tgt);
        let incoherent = |image: &Edge| RestrictError::IncoherentEdgeImage {
            source_edge: Box::new(edge.clone()),
            image: Box::new(image.clone()),
        };
        let resolution =
            if let Some(image) = direct_source_edge_image(src, tgt, migration, a, b, edge) {
                PairResolution::Label(image)
            } else if migration
                .expansion_path
                .contains_key(&(a.clone(), b.clone()))
            {
                resolve_pair(tgt, migration, a, b)
            } else {
                return Ok(EdgeFate::Drop);
            };
        if let Some(image) = incoherent_label(tgt, a, b, &resolution) {
            return Err(incoherent(image));
        }
        Ok(match resolution {
            PairResolution::Label(image) => EdgeFate::Keep(image),
            PairResolution::Expand {
                intermediates,
                labels,
            } => EdgeFate::Expand {
                intermediates,
                labels,
            },
            PairResolution::Unresolved(reason) => EdgeFate::Unresolved(reason),
        })
    }

    /// The fate of a source schema edge, or `None` when the edge is not one.
    #[must_use]
    pub fn fate(&self, edge: &Edge) -> Option<&EdgeFate> {
        self.fates.get(edge)
    }

    /// The source edges whose fate keeps them as `image`, ascending.
    #[must_use]
    pub fn preimages(&self, image: &Edge) -> Vec<&Edge> {
        self.fates
            .iter()
            .filter(|(_, fate)| matches!(fate, EdgeFate::Keep(kept) if kept == image))
            .map(|(edge, _)| edge)
            .collect()
    }
}

/// What a restrict does with one cell of a surviving row (a column carried
/// in a surviving node's `extra_fields`).
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellFate {
    /// The column is dropped.
    Drop(String),
    /// The column is renamed from the first name to the second.
    Rename(String, String),
}

/// The cell layer of a plan: what happens to the columns of each kept
/// source table, and the declared value spec of every target column.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellPlan {
    /// The cell fates of each kept source table, in column order; a column
    /// kept under its own name has none.
    pub fates: BTreeMap<Name, Vec<CellFate>>,
    /// The value spec of each column `(target table, column)`.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub specs: BTreeMap<(Name, String), ValueSpec>,
}

/// The layer of a plan that needs the protocols: cells, and the table
/// functor of the migration.
#[derive(Clone, Debug)]
pub struct BoundaryPlan {
    /// The cell layer, when the protocols classify both schemas.
    pub cells: Option<CellPlan>,
    /// The table functor of the migration, for a plan compiled for tables.
    pub table: Option<EntityFunctor>,
}

/// The compiled plan of a restrict from `src` to `tgt` along a migration.
///
/// Compiled once, read by every engine call; a pure function of the two
/// schemas and the migration (and, for [`Self::boundary`], the two
/// protocols), so it changes no lens identity.
#[derive(Clone, Debug)]
pub struct RestrictPlan {
    /// The digest of the source schema.
    pub src_schema: Digest,
    /// The digest of the target schema.
    pub tgt_schema: Digest,
    /// The fate of every source schema edge.
    pub edge_image: EdgeImage,
    /// The contraction plan.
    pub contraction: ContractionPlan,
    /// The layer that needs the protocols, when the plan was compiled with
    /// them.
    pub boundary: Option<BoundaryPlan>,
    /// The source schema.
    src: Schema,
    /// The target schema.
    tgt: Schema,
    /// The migration.
    migration: CompiledMigration,
}

impl RestrictPlan {
    /// The plan of `migration` from `src` to `tgt`, with no boundary layer.
    ///
    /// # Errors
    ///
    /// [`RestrictError::IncoherentEdgeImage`] from [`EdgeImage::compile`],
    /// then [`RestrictError::IncoherentPairImage`] from
    /// [`ContractionPlan::compile`].
    pub fn compile(
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        Ok(Self {
            src_schema: schema_digest(src),
            tgt_schema: schema_digest(tgt),
            edge_image: EdgeImage::compile(src, tgt, migration)?,
            contraction: ContractionPlan::compile(src, tgt, migration)?,
            boundary: None,
            src: src.clone(),
            tgt: tgt.clone(),
            migration: migration.clone(),
        })
    }

    /// The plan with its boundary layer: the cell layer when
    /// [`RelSchema::new`] classifies both schemas under their protocols
    /// (none otherwise, which is no error), and no table functor.
    ///
    /// # Errors
    ///
    /// As [`Self::compile`].
    pub fn compile_with_protocols(
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
        protocols: BoundaryProtocols<'_>,
    ) -> Result<Self, RestrictError> {
        let mut plan = Self::compile(src, tgt, migration)?;
        let cells = match (
            RelSchema::new(src, protocols.src),
            RelSchema::new(tgt, protocols.tgt),
        ) {
            (Ok(source), Ok(target)) => Some(cell_plan(&plan, &source, &target, protocols)),
            _ => None,
        };
        plan.boundary = Some(BoundaryPlan { cells, table: None });
        Ok(plan)
    }

    /// The plan the table engine runs: the boundary layer with both the cell
    /// layer and the table functor of the migration.
    ///
    /// The table restrict is defined where the survival rule of the restrict
    /// and the table functor agree: every source table is kept by both or by
    /// neither, with one image, and every relation between two kept tables
    /// and every column of a kept table is kept by both with one image (a
    /// column: the same column of the image table) or dropped by both.
    ///
    /// # Errors
    ///
    /// In this order: [`RestrictError::RelSchema`] when a schema has no
    /// relation-ified form (the source first); the errors of the table
    /// functor ([`EntityFunctor::between`]); the errors of
    /// [`Self::compile`]; [`RestrictError::NotTableRepresentable`] for an
    /// edge that expands into a path, then a contracted pair that does,
    /// then a hyper-edge resolution; and
    /// [`RestrictError::SurvivalRulesDisagree`] for the first table, in
    /// ascending name, and then the first relation or column, in ascending
    /// edge order, on which the two rules disagree.
    pub fn compile_for_tables(
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
        protocols: BoundaryProtocols<'_>,
    ) -> Result<Self, RestrictError> {
        let source = RelSchema::new(src, protocols.src)?;
        let target = RelSchema::new(tgt, protocols.tgt)?;
        let functor = EntityFunctor::between(source.clone(), target.clone(), migration)?;
        let mut plan = Self::compile(src, tgt, migration)?;
        plan.refuse_unrepresentable_on_tables()?;
        plan.refuse_disagreement(&functor, &source, &target)?;
        let cells = cell_plan(&plan, &source, &target, protocols);
        plan.boundary = Some(BoundaryPlan {
            cells: Some(cells),
            table: Some(functor),
        });
        Ok(plan)
    }

    /// Refuse a plan that needs a path where a table has one pair.
    fn refuse_unrepresentable_on_tables(&self) -> Result<(), RestrictError> {
        let refuse = |reason: String| Err(RestrictError::NotTableRepresentable { reason });
        if let Some((edge, _)) = self
            .edge_image
            .fates
            .iter()
            .find(|(_, fate)| matches!(fate, EdgeFate::Expand { .. }))
        {
            return refuse(format!("edge {edge:?} expands into a path"));
        }
        if let Some(((a, b), _)) = self
            .contraction
            .pairs
            .iter()
            .find(|(_, pair)| matches!(pair.resolution, PairResolution::Expand { .. }))
        {
            return refuse(format!(
                "the contracted pair ({a}, {b}) expands into a path"
            ));
        }
        if !self.migration.hyper_resolver.is_empty() {
            return refuse("the migration resolves hyper-edges".to_owned());
        }
        Ok(())
    }

    /// Refuse a plan whose survival rule disagrees with the table functor.
    fn refuse_disagreement(
        &self,
        functor: &EntityFunctor,
        source: &RelSchema,
        target: &RelSchema,
    ) -> Result<(), RestrictError> {
        let disagree = |element: SurvivalElement| {
            Err(RestrictError::SurvivalRulesDisagree {
                element: Box::new(element),
            })
        };
        let migration = &self.migration;
        let kept = |table: &'_ Name| -> Option<Name> {
            survives(migration, table).then(|| remap(migration, table).clone())
        };
        for table in source.tables() {
            if kept(table).as_ref() != functor.table_map().get(table) {
                return disagree(SurvivalElement::Table(table.clone()));
            }
        }
        let mut edges: Vec<(&Edge, Option<&str>)> = source
            .relations()
            .iter()
            .filter(|e| kept(&e.src).is_some() && kept(&e.tgt).is_some())
            .map(|e| (e, None))
            .collect();
        for (table, columns) in source.all_columns() {
            if kept(table).is_some() {
                edges.extend(columns.iter().map(|(column, e)| (e, Some(column.as_str()))));
            }
        }
        edges.sort_unstable_by(|a, b| a.0.cmp(b.0));
        for (edge, column) in edges {
            let fate = self.edge_image.fate(edge);
            let functor_column = |column: &str| {
                functor
                    .column_map()
                    .get(&(edge.src.clone(), column.to_owned()))
            };
            let agrees = match (fate, column) {
                (Some(EdgeFate::Keep(image)), None) => {
                    functor.relation_map().get(edge) == Some(image)
                }
                (Some(EdgeFate::Drop), None) => functor.relation_map().get(edge).is_none(),
                (Some(EdgeFate::Keep(image)), Some(column)) => functor_column(column)
                    .is_some_and(|c| target.attribute_column(image) == Some(c.as_str())),
                (Some(EdgeFate::Drop), Some(column)) => functor_column(column).is_none(),
                _ => false,
            };
            if !agrees {
                return disagree(SurvivalElement::Edge(edge.clone()));
            }
        }
        Ok(())
    }

    /// The source schema the plan was compiled from.
    #[must_use]
    pub const fn source(&self) -> &Schema {
        &self.src
    }

    /// The target schema the plan was compiled for.
    #[must_use]
    pub const fn target(&self) -> &Schema {
        &self.tgt
    }

    /// The migration the plan was compiled from.
    #[must_use]
    pub const fn migration(&self) -> &CompiledMigration {
        &self.migration
    }

    /// Whether the plan sends every tree to a tree the graph engine also
    /// gives: false iff some source schema edge between two surviving
    /// vertices has [`EdgeFate::Drop`], where the tree engine drops the
    /// arc's subtree but the graph engine drops the edge alone.
    ///
    /// Contraction does not decide it: on a tree the walk through dropped
    /// nodes into a surviving node is unique, so tree ancestor contraction
    /// and graph contraction join the same pairs.
    #[must_use]
    pub fn preserves_tree_shape(&self) -> bool {
        !self.edge_image.fates.iter().any(|(edge, fate)| {
            matches!(fate, EdgeFate::Drop)
                && self.anchor_survives(&edge.src)
                && self.anchor_survives(&edge.tgt)
        })
    }

    /// The cell layer, when the plan has one.
    #[must_use]
    pub fn cells(&self) -> Option<&CellPlan> {
        self.boundary.as_ref().and_then(|b| b.cells.as_ref())
    }

    /// The table functor, when the plan was compiled for tables.
    #[must_use]
    pub fn table_functor(&self) -> Option<&EntityFunctor> {
        self.boundary.as_ref().and_then(|b| b.table.as_ref())
    }
}

/// The cell layer of `plan` between the relation-ified schemas.
///
/// For each source table the plan keeps and each of its columns `c`, with
/// attribute arrow `α`: [`CellFate::Drop`] unless `α`'s fate is to be kept
/// as an attribute arrow of the target leaving the image table, then
/// [`CellFate::Rename`] when that arrow's column `c'` differs from `c`, and
/// nothing when it is `c`. Each column of each target table gets the spec
/// [`ValueSpec::for_vertex`] gives its value vertex.
fn cell_plan(
    plan: &RestrictPlan,
    source: &RelSchema,
    target: &RelSchema,
    protocols: BoundaryProtocols<'_>,
) -> CellPlan {
    let mut fates = BTreeMap::new();
    for table in source.tables() {
        if !survives(&plan.migration, table) {
            continue;
        }
        let image = remap(&plan.migration, table);
        let mut table_fates = Vec::new();
        for (column, arrow) in source.columns(table) {
            let kept = match plan.edge_image.fate(arrow) {
                Some(EdgeFate::Keep(kept)) if kept.src == *image => target.attribute_column(kept),
                _ => None,
            };
            match kept {
                Some(renamed) if renamed == column => {}
                Some(renamed) => {
                    table_fates.push(CellFate::Rename(column.clone(), renamed.to_owned()));
                }
                None => table_fates.push(CellFate::Drop(column.clone())),
            }
        }
        if !table_fates.is_empty() {
            fates.insert(table.clone(), table_fates);
        }
    }
    let mut specs = BTreeMap::new();
    for table in target.tables() {
        for (column, arrow) in target.columns(table) {
            specs.insert(
                (table.clone(), column.clone()),
                ValueSpec::for_vertex(&plan.tgt, protocols.tgt, &arrow.tgt),
            );
        }
    }
    CellPlan { fates, specs }
}

/// The source anchor a view anchor maps back to, read off the vertex map:
/// its one explicit preimage; itself, when it has none; and no single
/// anchor when it has several.
pub(crate) struct ReverseAnchors<'m> {
    preimages: HashMap<&'m Name, Vec<&'m Name>>,
}

impl<'m> ReverseAnchors<'m> {
    pub(crate) fn new(migration: &'m CompiledMigration) -> Self {
        let mut preimages: HashMap<&Name, Vec<&Name>> = HashMap::new();
        for (source, image) in &migration.vertex_remap {
            preimages.entry(image).or_default().push(source);
        }
        for candidates in preimages.values_mut() {
            candidates.sort_unstable();
        }
        Self { preimages }
    }

    /// Whether `view_anchor` maps back to exactly `source_anchor`.
    pub(crate) fn recovers(&self, view_anchor: &Name, source_anchor: &Name) -> bool {
        match self.preimages.get(view_anchor).map(Vec::as_slice) {
            None | Some([]) => view_anchor == source_anchor,
            Some([only]) => *only == source_anchor,
            Some(_) => false,
        }
    }
}

/// The floor above a source's largest id: one above it, saturating at
/// `u32::MAX` (which no floor-governed allocation hands out), and 0 for an
/// empty source.
#[must_use]
pub(crate) fn floor_above(max: Option<u32>) -> u32 {
    max.map_or(0, |m| m.saturating_add(1))
}

/// Ids for synthesized elements, one per key in ascending key order, from
/// `floor` upward, never `u32::MAX`.
///
/// # Errors
///
/// [`RestrictError::IdsExhausted`] when no id below `u32::MAX` at or above
/// the floor is left for some key.
pub(crate) fn allocate_synthesized(
    keys: impl IntoIterator<Item = SynthKey>,
    floor: u32,
    kind: IdKind,
) -> Result<BTreeMap<SynthKey, u32>, RestrictError> {
    let mut allocator = crate::segment::IdAllocator::from_floor(
        crate::segment::IdRange {
            lo: 0,
            hi: u32::MAX - 1,
        },
        floor,
    );
    let sorted: BTreeSet<SynthKey> = keys.into_iter().collect();
    let mut ids = BTreeMap::new();
    for key in sorted {
        let id = allocator
            .allocate()
            .map_err(|_| RestrictError::IdsExhausted { kind })?;
        ids.insert(key, id);
    }
    Ok(ids)
}

/// The key of a synthesized-id exhaustion in the graph engine's precedence
/// order: stage 5, `id` 0 for nodes and 1 for edges.
pub(crate) const fn exhaustion_key(kind: IdKind, error_kind: u8) -> ErrorKey {
    ErrorKey {
        stage: 5,
        id: match kind {
            IdKind::Node => 0,
            IdKind::Edge => 1,
        },
        id2: 0,
        kind: error_kind,
    }
}

impl RestrictPlan {
    /// The image of a source anchor under the vertex map.
    pub(crate) fn target_anchor<'a>(&'a self, anchor: &'a Name) -> &'a Name {
        remap(&self.migration, anchor)
    }

    /// Whether elements anchored at `anchor` survive the vertex map.
    pub(crate) fn anchor_survives(&self, anchor: &Name) -> bool {
        survives(&self.migration, anchor)
    }

    /// Every name the survival predicate and the value transforms of
    /// `anchor` read.
    pub(crate) fn demand(&self, anchor: &Name) -> crate::wtype::Demand {
        let mut names: BTreeSet<String> =
            crate::wtype::referenced_names(&self.migration.value_transforms(anchor))
                .into_iter()
                .collect();
        if let Some(predicate) = self.migration.conditional_survival.get(anchor) {
            names.extend(
                panproto_expr::free_vars(predicate)
                    .into_iter()
                    .map(|v| v.to_string()),
            );
        }
        crate::wtype::Demand::from_names(names)
    }

    /// Whether an element anchored at `anchor`, whose image survives, passes
    /// its vertex's survival predicate over its original `fields` and the
    /// children bound in `ctx`: true without a predicate, true or false for
    /// a predicate that evaluates to `Bool(true)` or `Bool(false)`.
    ///
    /// # Errors
    ///
    /// In this order: [`RestrictError::AmbiguousChildField`],
    /// [`RestrictError::StructuralChildNotATree`] and
    /// [`RestrictError::ValueNotExpressible`] for a predicate refused before
    /// evaluation, and [`RestrictError::SurvivalPredicateFailed`] for one
    /// that fails to evaluate or evaluates to something other than a
    /// boolean (a type error expecting `bool`).
    pub(crate) fn passes_predicate(
        &self,
        anchor: &Name,
        ctx: &TransformContext<'_>,
        fields: &HashMap<String, Value>,
    ) -> Result<bool, RestrictError> {
        let Some(predicate) = self.migration.conditional_survival.get(anchor) else {
            return Ok(true);
        };
        let node = ctx.element();
        let env = ctx.environment(predicate, fields)?;
        let config = panproto_expr::EvalConfig::default();
        match ctx.eval(predicate, &env, &config) {
            Ok(panproto_expr::Literal::Bool(verdict)) => Ok(verdict),
            Ok(other) => Err(RestrictError::SurvivalPredicateFailed {
                node,
                source: panproto_expr::ExprError::TypeError {
                    expected: "bool".to_owned(),
                    got: other.type_name().to_owned(),
                },
            }),
            Err(source) => Err(RestrictError::SurvivalPredicateFailed { node, source }),
        }
    }

    /// Transform a surviving element, already re-anchored to its image: its
    /// vertex's value transforms (over the children bound in `ctx`), then,
    /// when `cells` is set and the plan has a cell layer, the cell fates of
    /// its source table, then the coercion of every key a transform wrote
    /// that is a column of its target table to that column's spec.
    ///
    /// Returns what the run did to the element's keys, from which each
    /// source key's key in the result is read ([`KeyTrack::image_of`]).
    ///
    /// # Errors
    ///
    /// The errors of the transforms ([`apply_field_transforms`]), and
    /// [`RestrictError::ValueFidelity`] for the least written key whose
    /// value the column cannot hold exactly.
    ///
    /// [`apply_field_transforms`]: crate::wtype::apply_field_transforms
    pub(crate) fn transform(
        &self,
        node: &mut Node,
        source_anchor: &Name,
        ctx: &TransformContext<'_>,
        cells: bool,
    ) -> Result<KeyTrack, RestrictError> {
        let transforms = self.migration.value_transforms(source_anchor);
        let mut track = KeyTrack::over(node.extra_fields.keys());
        if !transforms.is_empty() {
            apply_transforms_tracked(node, &transforms, ctx, &mut track)?;
        }
        let Some(layer) = self.cells() else {
            return Ok(track);
        };
        if cells && let Some(fates) = layer.fates.get(source_anchor) {
            apply_cell_fates(&mut node.extra_fields, fates, &mut track);
        }
        for key in track.written.clone() {
            let Some(spec) = layer.specs.get(&(node.anchor.clone(), key.clone())) else {
                continue;
            };
            let Some(value) = node.extra_fields.get(&key) else {
                continue;
            };
            let site = ValueSite::new(self.column_vertex(&node.anchor, &key), None);
            let coerced =
                coerce_value(value, spec, site).map_err(|error| RestrictError::ValueFidelity {
                    node: ctx.element(),
                    error: Box::new(error),
                })?;
            node.extra_fields.insert(key, coerced);
        }
        Ok(track)
    }

    /// The value vertex of `column` of the target table `table`.
    fn column_vertex(&self, table: &Name, column: &str) -> Name {
        self.tgt
            .outgoing_edges(table)
            .iter()
            .find(|e| crate::rel_schema::column_name(e) == column)
            .map_or_else(|| table.clone(), |e| e.tgt.clone())
    }

    /// The error of an unresolved pair `(a', b')` of target anchors: no
    /// edge or several for the hop that leaves it unresolved.
    pub(crate) fn unresolved(&self, a: &Name, b: &Name, reason: UnresolvedReason) -> RestrictError {
        let (src, tgt) = failing_hop(&self.tgt, &self.migration, a, b);
        match reason {
            UnresolvedReason::NoEdge => RestrictError::NoEdgeFound {
                src: src.to_string(),
                tgt: tgt.to_string(),
            },
            UnresolvedReason::Ambiguous { count } => RestrictError::AmbiguousEdge {
                src: src.to_string(),
                tgt: tgt.to_string(),
                count,
            },
        }
    }
}

/// Apply cell fates to a row's cells all at once: every dropped or renamed
/// column is read from the cells as they were, so a rename onto a column
/// that is itself renamed or dropped moves the right value. A renamed value
/// carries whether a transform wrote it and the key it came from.
fn apply_cell_fates(fields: &mut HashMap<String, Value>, fates: &[CellFate], track: &mut KeyTrack) {
    let mut moved = Vec::new();
    for fate in fates {
        match fate {
            CellFate::Drop(column) => {
                fields.remove(column);
                track.removed(column);
            }
            CellFate::Rename(from, to) => {
                if let Some(value) = fields.remove(from) {
                    let was_written = track.written.remove(from);
                    let origin = track.origin.remove(from);
                    moved.push((to, value, was_written, origin));
                }
            }
        }
    }
    for (to, value, was_written, origin) in moved {
        fields.insert(to.clone(), value);
        if was_written {
            track.written.insert(to.clone());
        } else {
            track.written.remove(to);
        }
        match origin {
            Some(origin) => track.origin.insert(to.clone(), origin),
            None => track.origin.remove(to),
        };
    }
}

impl RestrictError {
    /// The error's position in the graph engine's precedence order:
    /// `Some` exactly on [`RestrictError::At`], the form in which the graph
    /// engine returns every error it raises while evaluating an instance,
    /// and `None` on every other error (the tree and table engines, plan
    /// compilation and the boundary refusals locate nothing).
    #[must_use]
    pub const fn key(&self) -> Option<ErrorKey> {
        match self {
            Self::At { key, .. } => Some(*key),
            _ => None,
        }
    }

    /// The error without its location: the inner error of
    /// [`RestrictError::At`] and the error itself otherwise. Statements that
    /// two engines raise the same error compare this.
    #[must_use]
    pub fn unlocated(&self) -> &Self {
        match self {
            Self::At { error, .. } => error,
            _ => self,
        }
    }

    /// The index of the error's variant in declaration order (the inner
    /// error's, for a located one): the `kind` of its [`ErrorKey`].
    #[must_use]
    pub fn kind_index(&self) -> u8 {
        match self {
            Self::NoEdgeFound { .. } => 0,
            Self::AmbiguousEdge { .. } => 1,
            Self::RootPruned => 2,
            Self::FanReconstructionFailed { .. } => 3,
            Self::ProductSizeExceeded { .. } => 4,
            Self::MultiElementFiber { .. } => 5,
            Self::UnmappedAnchor { .. } => 6,
            Self::AttributeCollision { .. } => 7,
            Self::NonInjectiveVertexMap { .. } => 8,
            Self::FieldTransformFailed { .. } => 9,
            Self::RowIdsExhausted => 10,
            Self::VertexOutsideDomain { .. } => 11,
            Self::EdgeOutsideDomain { .. } => 12,
            Self::IncoherentEdgeImage { .. } => 13,
            Self::VertexImageNotATable { .. } => 14,
            Self::IncoherentAttributeImage { .. } => 15,
            Self::AttributeOutsideImage { .. } => 16,
            Self::ValueStructureRemap { .. } => 17,
            Self::RelSchema(_) => 18,
            Self::Validation { .. } => 19,
            Self::AmbiguousChildField { .. } => 20,
            Self::NotTableRepresentable { .. } => 21,
            Self::ValueNotExpressible { .. } => 22,
            Self::At { error, .. } => error.kind_index(),
            Self::SurvivalRulesDisagree { .. } => 24,
            Self::TablePlanRequired => 25,
            Self::StructuralChildNotATree { .. } => 26,
            Self::IncoherentPairImage { .. } => 27,
            Self::ValueFidelity { .. } => 28,
            Self::IdsExhausted { .. } => 29,
            Self::SurvivalPredicateFailed { .. } => 30,
            Self::PlanSchemaMismatch { .. } => 31,
        }
    }

    /// Locate an error of the graph engine at `stage` and element
    /// `(id, id2)`: [`RestrictError::At`] with that key, the kind its
    /// variant's index. An error already located is returned as it is.
    pub(crate) fn at(self, stage: u8, id: u32, id2: u32) -> Self {
        if matches!(self, Self::At { .. }) {
            return self;
        }
        let key = ErrorKey {
            stage,
            id,
            id2,
            kind: self.kind_index(),
        };
        Self::At {
            key,
            error: Box::new(self),
        }
    }
}
