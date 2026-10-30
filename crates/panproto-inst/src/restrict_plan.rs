//! The compiled plan of a restrict, shared by the tree, table and graph
//! engines.
//!
//! A [`RestrictPlan`] is a pure function of the source schema, the target
//! schema and the compiled migration (and, for its boundary layer, of the
//! two protocols). It holds the image of every source schema edge
//! ([`EdgeImage`]), the contraction plan ([`ContractionPlan`]) and, when
//! the protocols classify both schemas into tables and columns, the cell
//! layer ([`CellPlan`]) and, for tables, the table functor of the migration
//! ([`EntityFunctor`]). The three representation engines
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
    CompiledMigration, KeyTrack, TransformContext, apply_transforms_tracked, check_expansion,
    direct_source_edge_image,
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
    /// image; consecutive anchors of the path for a label), and
    /// [`RestrictError::ExpansionTooLong`] for the least one expanded into a
    /// path of more than 65536 edges, more than a synthesized key numbers.
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
        if let PairResolution::Expand { labels, .. } = &resolution {
            check_expansion(a, b, labels)?;
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
///
/// Besides its public fields, it holds privately the two schemas and the
/// migration it was compiled from, which the engines read, through
/// accessors private to the crate, for value transforms and survival
/// predicates; outside the crate the cell layer and the table functor are
/// read from [`Self::boundary`]. [`Self::compile`], [`Self::compile_with_protocols`],
/// [`Self::compile_for_tables`] and [`Self::compile_for_table_extension`] are
/// its only constructors, so the
/// private fields are exactly the inputs of the compilation that built
/// the plan (the public fields stay writable). It is `Clone` so that a
/// lens may hold it, and `Debug` for diagnostics.
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

    /// The table plan for a left Kan extension.
    ///
    /// Unlike [`Self::compile_for_tables`], this constructor does not require
    /// the restrict survival rule to agree with the table functor. Sigma is
    /// defined by the functor itself: a source relation with no functor image
    /// is an unmapped element, which total extension refuses and partial
    /// extension reports. Requiring agreement here would reject precisely the
    /// partial migrations those two APIs are meant to distinguish.
    ///
    /// # Errors
    ///
    /// As [`Self::compile_for_tables`] through table representability, but
    /// never [`RestrictError::SurvivalRulesDisagree`].
    pub fn compile_for_table_extension(
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
    pub(crate) const fn source(&self) -> &Schema {
        &self.src
    }

    /// The target schema the plan was compiled for.
    #[must_use]
    pub(crate) const fn target(&self) -> &Schema {
        &self.tgt
    }

    /// The migration the plan was compiled from.
    #[must_use]
    pub(crate) const fn migration(&self) -> &CompiledMigration {
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
    ///
    /// It is the plan computation that defines a lens's
    /// `preserves_tree_shape`, public so that panproto-lens can define that
    /// method by it; when it is false, the tree engine's view of a tree
    /// differs from the graph engine's view of the tree's graph form.
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
    pub(crate) fn cells(&self) -> Option<&CellPlan> {
        self.boundary.as_ref().and_then(|b| b.cells.as_ref())
    }

    /// The table functor, when the plan was compiled for tables.
    #[must_use]
    pub(crate) fn table_functor(&self) -> Option<&EntityFunctor> {
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

/// The source anchor a view anchor maps back to, read off the total map on
/// the source's carrier anchors: its unique preimage, when it has one.
pub(crate) struct ReverseAnchors<'s> {
    preimages: HashMap<Name, Vec<&'s Name>>,
}

impl<'s> ReverseAnchors<'s> {
    /// Build the inverse on graph anchors. Ordinary vertices use the total
    /// vertex map, including implicit identities. Hyper-edges use every
    /// resolver target, with identity also possible unless exactly one
    /// resolver entry fixes the image.
    pub(crate) fn for_graph(source: &'s Schema, migration: &CompiledMigration) -> Self {
        let mut reverse = Self::from_vertices(source.vertices.keys(), migration);
        for source_anchor in source.hyper_edges.keys() {
            let images: Vec<&Name> = migration
                .hyper_resolver
                .iter()
                .filter(|((hyper_edge, _), _)| hyper_edge == source_anchor)
                .map(|(_, (target, _))| target)
                .collect();
            for image in &images {
                reverse.insert((*image).clone(), source_anchor);
            }
            if images.len() != 1 {
                reverse.insert(source_anchor.clone(), source_anchor);
            }
        }
        reverse.sort();
        reverse
    }

    /// Build the inverse on the table objects of an attributed relational
    /// schema. Value vertices are not table-carrier preimages.
    pub(crate) fn for_tables(source: &'s RelSchema, migration: &CompiledMigration) -> Self {
        let mut reverse = Self::from_vertices(source.tables(), migration);
        reverse.sort();
        reverse
    }

    fn from_vertices<'a>(
        vertices: impl IntoIterator<Item = &'s Name>,
        migration: &'a CompiledMigration,
    ) -> Self {
        let mut reverse = Self {
            preimages: HashMap::new(),
        };
        for source in vertices {
            if survives(migration, source) {
                reverse.insert(remap(migration, source).clone(), source);
            }
        }
        reverse
    }

    fn insert(&mut self, image: Name, source: &'s Name) {
        let candidates = self.preimages.entry(image).or_default();
        if !candidates.contains(&source) {
            candidates.push(source);
        }
    }

    fn sort(&mut self) {
        for candidates in self.preimages.values_mut() {
            candidates.sort_unstable();
        }
    }

    /// Whether `view_anchor` maps back to exactly `source_anchor`.
    pub(crate) fn recovers(&self, view_anchor: &Name, source_anchor: &Name) -> bool {
        self.source_of(view_anchor) == Some(source_anchor)
    }

    /// The source anchor `view_anchor` maps back to, exactly when its
    /// preimage among the source's carrier anchors is a singleton.
    pub(crate) fn source_of<'a>(&'a self, view_anchor: &Name) -> Option<&'a Name> {
        match self.preimages.get(view_anchor).map(Vec::as_slice) {
            Some([only]) => Some(*only),
            _ => None,
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
    let mut allocator =
        crate::segment::IdAllocator::from_floor(crate::segment::IdRange::FULL, floor);
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

/// What a survival predicate gives at one element.
pub(crate) enum PredicateOutcome {
    /// A verdict: `true` keeps the element, `false` drops it.
    Verdict(bool),
    /// The predicate reads what cannot be bound, so it is not evaluated.
    Unreadable(crate::wtype::Unreadable),
    /// The evaluation fails.
    Failed(panproto_expr::ExprError),
    /// The predicate evaluates to a literal of this type, not a boolean.
    NotBoolean(String),
}

impl PredicateOutcome {
    /// The verdict, or the refusal it is as a [`PredicateRefusal`].
    pub(crate) fn into_refusal(self) -> Result<bool, crate::error_detail::PredicateRefusal> {
        use crate::error_detail::PredicateRefusal;
        use crate::wtype::Unreadable;
        match self {
            Self::Verdict(verdict) => Ok(verdict),
            Self::Unreadable(Unreadable::AmbiguousChildField { field, .. }) => {
                Err(PredicateRefusal::AmbiguousChildField { field })
            }
            Self::Unreadable(Unreadable::StructuralChildNotATree { field, .. }) => {
                Err(PredicateRefusal::StructuralChildNotATree { field })
            }
            Self::Unreadable(Unreadable::ValueNotExpressible {
                field, value_type, ..
            }) => Err(PredicateRefusal::ValueNotExpressible { field, value_type }),
            Self::Failed(source) => Err(PredicateRefusal::EvaluationFailed {
                message: source.to_string(),
            }),
            Self::NotBoolean(got) => Err(PredicateRefusal::NotBoolean { got }),
        }
    }
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
        let node = ctx.element();
        match self.predicate_outcome(anchor, ctx, fields) {
            PredicateOutcome::Verdict(verdict) => Ok(verdict),
            PredicateOutcome::Unreadable(refusal) => Err(refusal.into()),
            PredicateOutcome::Failed(source) => {
                Err(RestrictError::SurvivalPredicateFailed { node, source })
            }
            PredicateOutcome::NotBoolean(got) => Err(RestrictError::SurvivalPredicateFailed {
                node,
                source: panproto_expr::ExprError::TypeError {
                    expected: "bool".to_owned(),
                    got,
                },
            }),
        }
    }

    /// What the survival predicate of `anchor` gives at the element `ctx`
    /// is at, over its original `fields`: a verdict (`true` without a
    /// predicate), or why it gives none, in the order
    /// [`passes_predicate`](Self::passes_predicate) reports.
    pub(crate) fn predicate_outcome(
        &self,
        anchor: &Name,
        ctx: &TransformContext<'_>,
        fields: &HashMap<String, Value>,
    ) -> PredicateOutcome {
        let Some(predicate) = self.migration.conditional_survival.get(anchor) else {
            return PredicateOutcome::Verdict(true);
        };
        let env = match ctx.environment(predicate, fields) {
            Ok(env) => env,
            Err(refusal) => return PredicateOutcome::Unreadable(refusal),
        };
        let config = panproto_expr::EvalConfig::default();
        match ctx.eval(predicate, &env, &config) {
            Ok(panproto_expr::Literal::Bool(verdict)) => PredicateOutcome::Verdict(verdict),
            Ok(other) => PredicateOutcome::NotBoolean(other.type_name().to_owned()),
            Err(source) => PredicateOutcome::Failed(source),
        }
    }

    /// Transform a surviving element, already re-anchored to its image: its
    /// vertex's value transforms (over the children bound in `ctx`), then,
    /// when `cells` is set and the plan has a cell layer, the cell fates of
    /// its source table, then the coercion of every key a transform wrote
    /// that is a column of its target table to that column's spec.
    ///
    /// # Errors
    ///
    /// The errors of the transforms ([`apply_field_transforms`]), and
    /// [`RestrictError::ValueFidelity`] for the least written key whose
    /// value the column cannot hold exactly.
    pub(crate) fn transform(
        &self,
        node: &mut Node,
        source_anchor: &Name,
        ctx: &TransformContext<'_>,
        cells: bool,
    ) -> Result<(), RestrictError> {
        let transforms = self.migration.value_transforms(source_anchor);
        let layer = self.cells();
        let fates = layer
            .filter(|_| cells)
            .and_then(|layer| layer.fates.get(source_anchor));
        if transforms.is_empty() && fates.is_none() {
            // Nothing moves, writes or drops a key, so nothing is coerced.
            return Ok(());
        }
        let mut track = KeyTrack::default();
        if !transforms.is_empty() {
            apply_transforms_tracked(node, &transforms, ctx, &mut track)?;
        }
        let Some(layer) = layer else {
            return Ok(());
        };
        if let Some(fates) = fates {
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
        Ok(())
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
/// carries whether a transform wrote it.
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
                    moved.push((to, value, track.take(from)));
                }
            }
        }
    }
    for (to, value, state) in moved {
        fields.insert(to.clone(), value);
        track.put(to, state);
    }
}

impl RestrictError {
    /// The error's position in the graph engine's precedence order:
    /// `Some` exactly on [`RestrictError::At`], the form in which the graph
    /// engine returns every error it raises while evaluating an instance,
    /// and `None` on every other error (the tree and table engines, plan
    /// compilation, the boundary refusals and the graph engine's fan-domain
    /// refusal, [`RestrictError::MalformedFan`], locate nothing).
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
    ///
    /// It equals the variant's position in the declaration of
    /// [`RestrictError`], and it is stable across versions because that
    /// enum only grows by appending variants. Tests that state two engines
    /// raise the same error compare it.
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
            Self::ExpansionTooLong { .. } => 32,
            Self::NonConforming { .. } => 33,
            Self::UnmappedEdge { .. } => 34,
            Self::SigmaConstantClash { .. } => 35,
            Self::NullIdsExhausted => 36,
            Self::MalformedFan { .. } => 37,
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

#[cfg(test)]
mod tests {
    use super::allocate_synthesized;
    use crate::error::RestrictError;
    use crate::ginstance::EdgeId;
    use crate::segment::IdKind;
    use crate::view_ref::SynthKey;

    /// At every floor in the last four ids, `count` synthesized ids are the
    /// ids from the floor up when they all lie below `u32::MAX`, and the
    /// typed exhaustion error of their kind otherwise.
    #[test]
    fn synthesized_ids_are_exhaustive_at_the_top() {
        for kind in [IdKind::Node, IdKind::Edge] {
            for floor in (u32::MAX - 3)..=u32::MAX {
                for count in 0..=4_u32 {
                    let keys = (0..count).map(|i| SynthKey::Expand {
                        source_edge: EdgeId(i),
                        hop: 0,
                    });
                    let ids = allocate_synthesized(keys, floor, kind);
                    if count <= u32::MAX - floor {
                        let ids: Vec<u32> = ids.unwrap_or_default().into_values().collect();
                        assert_eq!(ids, (floor..floor + count).collect::<Vec<_>>());
                    } else {
                        assert!(
                            matches!(ids, Err(RestrictError::IdsExhausted { kind: found }) if found == kind),
                            "{count} {kind} ids at floor {floor}: {ids:?}"
                        );
                    }
                }
            }
        }
    }
}
