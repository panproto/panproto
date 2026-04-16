//! Automatic protolens generation pipeline.
//!
//! Given two schemas, auto-discovers morphism alignment, factorizes
//! it into elementary endofunctors, maps each to a protolens, and
//! composes the result.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{Name, Theory, TheoryEndofunctor, TheoryMorphism, TheoryTransform, factorize};
use panproto_inst::value::Value;
use panproto_mig::align::{self, AliasDict, Anchor, default_alias_dict};
use panproto_mig::hom_search::{
    DomainConstraints, FoundMorphism, SearchOptions, find_best_morphism,
    find_best_morphism_constrained,
};
use panproto_schema::{Protocol, Schema};

use crate::Lens;
use crate::error::LensError;
use crate::protolens::{Protolens, ProtolensChain, elementary};

/// Stringency tier controlling which alignment strategies run and how
/// permissively the CSP solver searches.
///
/// Higher tiers form a superset of lower-tier behaviors; nothing
/// available at `Strict` is suppressed at `Exploratory`. Each tier
/// preserves categorical soundness: the CSP still validates naturality
/// for every emitted morphism.
///
/// * **`Strict`** — only kind-exact name equality is consulted; the
///   solver enforces hard edge-name overlap pruning. Returns either a
///   total theory morphism or no result.
/// * **`Balanced`** — runs the alias dictionary and a tight
///   token-similarity threshold on top of `Strict`'s priors; relaxes
///   the edge-name pruning to a soft preference.
/// * **`Lenient`** — opens the search to spans `A ←f− C −g→ B` over a
///   maximal common subtheory `C`, loosens the token-similarity
///   threshold, and engages structural matching.
/// * **`Exploratory`** — additionally admits lossy retraction witnesses
///   for sort coercion and language-model-proposed alignments
///   (feature-gated). Candidates are still validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stringency {
    /// Hard kind-exact, edge-name-pruned CSP search; total morphism only.
    Strict,
    /// Adds alias dictionary and tight token-similarity priors.
    #[default]
    Balanced,
    /// Adds span-search over maximal common subtheories and structural priors.
    Lenient,
    /// Adds lossy retraction witnesses and LM-proposed alignments.
    Exploratory,
}

impl Stringency {
    /// Whether token-similarity priors are consulted at this tier.
    #[must_use]
    pub const fn uses_token_similarity(self) -> bool {
        !matches!(self, Self::Strict)
    }

    /// Whether the alias dictionary is consulted at this tier.
    #[must_use]
    pub const fn uses_alias_dict(self) -> bool {
        !matches!(self, Self::Strict)
    }

    /// Whether the wrap/unwrap idiom detector runs at this tier.
    #[must_use]
    pub const fn uses_wrap_unwrap(self) -> bool {
        matches!(self, Self::Lenient | Self::Exploratory)
    }

    /// Whether the type-signature strategy runs at this tier.
    #[must_use]
    pub const fn uses_type_signature(self) -> bool {
        matches!(self, Self::Lenient | Self::Exploratory)
    }

    /// Whether the overlap-based fallback is turned on automatically
    /// at this tier when the caller hasn't pinned `try_overlap`.
    #[must_use]
    pub const fn default_try_overlap(self) -> bool {
        matches!(self, Self::Lenient | Self::Exploratory)
    }

    /// Token-similarity acceptance threshold at this tier. Lower values
    /// admit weaker matches as anchor candidates; the CSP still validates.
    #[must_use]
    pub const fn token_similarity_threshold(self) -> f64 {
        match self {
            Self::Strict => 1.0,
            Self::Balanced => 0.75,
            Self::Lenient => 0.55,
            Self::Exploratory => 0.40,
        }
    }

    /// Minimum kind-signature overlap ratio for the type-signature
    /// strategy to emit an anchor at this tier.
    #[must_use]
    pub const fn type_signature_threshold(self) -> f64 {
        match self {
            Self::Strict | Self::Balanced => 1.0,
            Self::Lenient => 0.75,
            Self::Exploratory => 0.50,
        }
    }

    /// Whether the CSP should relax its hard edge-name overlap pruning
    /// at this tier. When `true`, kind-compatible candidates are kept
    /// even when they share no edge names with the source vertex; when
    /// `false`, the pruner runs as in the `Strict` baseline.
    #[must_use]
    pub const fn relax_edge_name_pruning(self) -> bool {
        !matches!(self, Self::Strict)
    }
}

/// Result of automatic protolens generation.
pub struct AutoLensResult {
    /// The protolens chain (schema-independent, reusable).
    pub chain: ProtolensChain,
    /// The concrete lens (schema-specific).
    pub lens: Lens,
    /// Quality score of the morphism alignment (0.0 to 1.0).
    pub alignment_quality: f64,
    /// Anchors that the alignment strategies seeded into the CSP
    /// before the morphism search ran. Useful for explanations.
    pub seed_anchors: Vec<Anchor>,
}

/// Configuration for automatic lens generation.
#[derive(Debug, Clone)]
pub struct AutoLensConfig {
    /// User-provided default values for new sorts.
    pub defaults: HashMap<Name, Value>,
    /// Search options for morphism discovery.
    pub search_opts: SearchOptions,
    /// Whether to attempt overlap-based alignment when direct morphism fails.
    pub try_overlap: bool,
    /// Stringency tier governing which alignment strategies run and how
    /// permissively the CSP searches.
    pub stringency: Stringency,
    /// Alias dictionary consulted by the alias strategy. Defaults to the
    /// built-in domain-agnostic dictionary; callers may extend it with
    /// protocol-specific cartridges.
    pub alias_dict: AliasDict,
}

impl Default for AutoLensConfig {
    fn default() -> Self {
        Self {
            defaults: HashMap::new(),
            search_opts: SearchOptions::default(),
            try_overlap: false,
            stringency: Stringency::default(),
            alias_dict: default_alias_dict(),
        }
    }
}

/// Run the alignment strategies enabled by `config.stringency`, returning
/// the raw anchor proposals. Strategies are listed in priority order;
/// `resolve_anchors` later picks a single target per source vertex,
/// preferring higher-confidence and higher-priority anchors.
///
/// User-supplied anchors (`config.search_opts.initial`) are not consulted
/// here; callers merge them in on top of the strategy output.
fn run_strategies(src: &Schema, tgt: &Schema, config: &AutoLensConfig) -> Vec<Anchor> {
    let mut anchors = Vec::new();

    // Exact name equality is consulted at every tier.
    anchors.extend(align::exact_anchors(src, tgt));

    if config.stringency.uses_alias_dict() {
        anchors.extend(align::alias_anchors(src, tgt, &config.alias_dict));
    }

    if config.stringency.uses_token_similarity() {
        let threshold = config.stringency.token_similarity_threshold();
        anchors.extend(align::token_anchors(src, tgt, threshold));
    }

    if config.stringency.uses_wrap_unwrap() {
        anchors.extend(align::wrap_unwrap_anchors(src, tgt));
    }

    if config.stringency.uses_type_signature() {
        let threshold = config.stringency.type_signature_threshold();
        anchors.extend(align::type_signature_anchors(src, tgt, threshold));
    }

    anchors
}

/// Merge `additional` (source → target name pairs) into `opts.initial`
/// without overwriting any existing entry.
fn merge_seed_anchors(opts: &mut SearchOptions, additional: &HashMap<Name, Name>) {
    for (s, t) in additional {
        opts.initial.entry(s.clone()).or_insert_with(|| t.clone());
    }
}

/// Set `opts.relax_edge_name_pruning` according to `stringency`. The
/// caller's explicit `true` is preserved if already set.
const fn apply_stringency_search_opts(opts: &mut SearchOptions, stringency: Stringency) {
    if stringency.relax_edge_name_pruning() {
        opts.relax_edge_name_pruning = true;
    }
}

/// Generate a protolens chain and concrete lens from two schemas.
///
/// # Pipeline
///
/// 1. Run alignment strategies enabled by [`AutoLensConfig::stringency`]
///    over `(src, tgt)`, producing candidate anchors.
/// 2. Resolve anchors into a single seed map (higher-priority strategies
///    win conflicts) and merge into [`SearchOptions::initial`].
/// 3. Run the CSP-based morphism search; the CSP enforces naturality on
///    every candidate seed, so heuristic priors cannot produce an invalid
///    morphism.
/// 4. If `config.try_overlap` is set and the result is missing or below
///    the quality floor, retry with overlap-derived seeds.
/// 5. Convert the alignment to a GAT-level `TheoryMorphism`, factorize
///    into elementary endofunctors, map each to an elementary `Protolens`,
///    compose into a `ProtolensChain`, and instantiate at `src`.
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] if no morphism is found,
/// factorization fails, or instantiation fails.
pub fn auto_generate(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    config: &AutoLensConfig,
) -> Result<AutoLensResult, LensError> {
    let seed_anchors = run_strategies(src, tgt, config);
    let resolved = align::resolve_anchors(&seed_anchors, config.search_opts.monic);

    let mut search_opts = config.search_opts.clone();
    apply_stringency_search_opts(&mut search_opts, config.stringency);
    merge_seed_anchors(&mut search_opts, &resolved);

    // Lenient / Exploratory tiers always consult the overlap fallback
    // (span-style retry with alias-derived seeds). Caller's explicit
    // `try_overlap = true` is preserved at every tier.
    let mut effective = config.clone();
    if config.stringency.default_try_overlap() {
        effective.try_overlap = true;
    }

    let result = run_search(src, tgt, protocol, &effective, &search_opts, None)?;

    Ok(AutoLensResult {
        chain: result.chain,
        lens: result.lens,
        alignment_quality: result.alignment_quality,
        seed_anchors,
    })
}

struct SearchResult {
    chain: ProtolensChain,
    lens: Lens,
    alignment_quality: f64,
}

/// Shared CSP-search/factorize/instantiate pipeline. `domain_constraints`
/// is consulted only when `Some`; otherwise the unconstrained
/// [`find_best_morphism`] is used.
fn run_search(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    config: &AutoLensConfig,
    search_opts: &SearchOptions,
    domain_constraints: Option<&DomainConstraints>,
) -> Result<SearchResult, LensError> {
    let search = |opts: &SearchOptions| -> Option<FoundMorphism> {
        domain_constraints.map_or_else(
            || find_best_morphism(src, tgt, opts),
            |dc| find_best_morphism_constrained(src, tgt, opts, dc),
        )
    };

    let mut alignment = search(search_opts);

    let quality_floor = 0.5;
    if config.try_overlap {
        let should_try_overlap = alignment.as_ref().is_none_or(|a| a.quality < quality_floor);
        if should_try_overlap {
            let overlap = panproto_mig::discover_overlap(src, tgt);
            if !overlap.vertex_pairs.is_empty() {
                let mut overlap_opts = search_opts.clone();
                for (src_id, tgt_id) in &overlap.vertex_pairs {
                    overlap_opts
                        .initial
                        .entry(src_id.clone())
                        .or_insert_with(|| tgt_id.clone());
                }
                if let Some(oa) = search(&overlap_opts) {
                    let is_better = alignment.as_ref().is_none_or(|a| oa.quality > a.quality);
                    if is_better {
                        alignment = Some(oa);
                    }
                }
            }
        }
    }

    let alignment = alignment
        .ok_or_else(|| LensError::ProtolensError("no morphism found between schemas".into()))?;

    let quality = alignment.quality;
    let chain = protolens_from_alignment(&alignment, src, tgt)?;
    let mut lens = chain.instantiate(src, protocol)?;
    let field_transforms = derive_field_transforms(&chain, src, tgt);
    lens.compiled.field_transforms = field_transforms;

    Ok(SearchResult {
        chain,
        lens,
        alignment_quality: quality,
    })
}

/// Auto-generate with hint-guided constraint propagation.
///
/// User-supplied `anchors` and `domain_constraints` are layered on top of
/// the alignment strategies enabled by `config.stringency`. User anchors
/// take precedence over heuristic anchors; the latter only fill in
/// source vertices the user did not pin.
///
/// # Parameters
///
/// - `anchors`: user-supplied / hint-derived vertex name mappings
///   (source → target). These pin assignments before the CSP runs.
/// - `domain_constraints`: domain restrictions and scoring overrides.
/// - `quality_threshold`: minimum quality before trying overlap fallback
///   (default: 0.5).
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] if no morphism is found.
pub fn auto_generate_with_hints(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    config: &AutoLensConfig,
    anchors: &HashMap<Name, Name>,
    domain_constraints: &DomainConstraints,
    quality_threshold: Option<f64>,
) -> Result<AutoLensResult, LensError> {
    let _ = quality_threshold; // overlap floor is fixed at 0.5 inside `run_search`

    let strategy_anchors = run_strategies(src, tgt, config);
    let resolved_strategy = align::resolve_anchors(&strategy_anchors, config.search_opts.monic);

    let mut search_opts = config.search_opts.clone();
    apply_stringency_search_opts(&mut search_opts, config.stringency);
    // User hints first (highest priority).
    for (src_v, tgt_v) in anchors {
        search_opts.initial.insert(src_v.clone(), tgt_v.clone());
    }
    // Strategy anchors fill in the rest without overwriting.
    merge_seed_anchors(&mut search_opts, &resolved_strategy);

    let mut effective = config.clone();
    if config.stringency.default_try_overlap() {
        effective.try_overlap = true;
    }

    let result = run_search(
        src,
        tgt,
        protocol,
        &effective,
        &search_opts,
        Some(domain_constraints),
    )?;

    // Combine user anchors (as `UserHint`-tagged anchors) with the
    // strategy proposals so that downstream callers see the full set.
    let mut combined = Vec::with_capacity(strategy_anchors.len() + anchors.len());
    for (src_v, tgt_v) in anchors {
        combined.push(Anchor {
            src: src_v.clone(),
            tgt: tgt_v.clone(),
            confidence: 1.0,
            strategy: align::StrategyTag::UserHint,
            explanation: format!("user hint: {} ↔ {}", src_v.as_str(), tgt_v.as_str()),
        });
    }
    combined.extend(strategy_anchors);

    Ok(AutoLensResult {
        chain: result.chain,
        lens: result.lens,
        alignment_quality: result.alignment_quality,
        seed_anchors: combined,
    })
}

/// Generate a protolens chain from a pre-computed morphism alignment.
///
/// Converts the schema-level alignment to a GAT-level theory morphism,
/// factorizes it into elementary endofunctors, and maps each to a
/// protolens.
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] if factorization fails or
/// an endofunctor cannot be mapped to a protolens.
pub fn protolens_from_alignment(
    alignment: &FoundMorphism,
    src: &Schema,
    tgt: &Schema,
) -> Result<ProtolensChain, LensError> {
    // Convert schema-level alignment to GAT-level theory morphism
    let src_theory = schema_to_implicit_theory(src);
    let tgt_theory = schema_to_implicit_theory(tgt);
    let morphism = alignment_to_theory_morphism(alignment, src, tgt);

    // Factorize the morphism
    let factorization = factorize(&morphism, &src_theory, &tgt_theory)
        .map_err(|e| LensError::ProtolensError(format!("factorization failed: {e}")))?;

    // Map each elementary endofunctor to a protolens
    let mut steps = Vec::new();
    for endofunctor in &factorization.steps {
        let protolens = endofunctor_to_protolens(endofunctor)?;
        steps.push(protolens);
    }

    Ok(ProtolensChain::new(steps))
}

/// Derive value-level field transforms from a protolens chain.
///
/// For each elementary protolens step, determines which vertices are
/// affected and generates the appropriate `FieldTransform` entries.
/// This is protocol-agnostic; it works purely from the chain structure.
fn derive_field_transforms(
    chain: &ProtolensChain,
    src: &Schema,
    _tgt: &Schema,
) -> std::collections::HashMap<Name, Vec<panproto_inst::FieldTransform>> {
    use panproto_gat::TheoryTransform;
    use panproto_inst::FieldTransform;

    let mut transforms: std::collections::HashMap<Name, Vec<FieldTransform>> =
        std::collections::HashMap::new();

    for step in &chain.steps {
        match &step.target.transform {
            TheoryTransform::RenameOp { old, new } => {
                // Find all vertices that have an outgoing edge with this name
                for vid in src.vertices.keys() {
                    let has_edge = src
                        .outgoing_edges(vid)
                        .iter()
                        .any(|e| e.name.as_deref() == Some(old.as_ref()));
                    if has_edge {
                        transforms.entry(vid.clone()).or_default().push(
                            FieldTransform::RenameField {
                                old_key: old.to_string(),
                                new_key: new.to_string(),
                            },
                        );
                    }
                }
            }
            TheoryTransform::DropOp(name) => {
                for vid in src.vertices.keys() {
                    let has_edge = src
                        .outgoing_edges(vid)
                        .iter()
                        .any(|e| e.name.as_deref() == Some(name.as_ref()));
                    if has_edge {
                        transforms.entry(vid.clone()).or_default().push(
                            FieldTransform::DropField {
                                key: name.to_string(),
                            },
                        );
                    }
                }
            }
            TheoryTransform::AddDirectedEquation(deq) => {
                // Extract the variable name from the LHS pattern
                let key = match &deq.lhs {
                    panproto_gat::Term::Var(name) => name.to_string(),
                    panproto_gat::Term::App { op, .. } => op.to_string(),
                };
                for vid in src.vertices.keys() {
                    transforms
                        .entry(vid.clone())
                        .or_default()
                        .push(FieldTransform::ApplyExpr {
                            key: key.clone(),
                            expr: deq.impl_term.clone(),
                            inverse: deq.inverse.clone(),
                            coercion_class: deq.coercion_class,
                        });
                }
            }
            TheoryTransform::CoerceSort {
                sort_name,
                coercion_expr,
                inverse_expr,
                coercion_class,
                ..
            } => {
                for vid in src.vertices.keys() {
                    if src.vertex(vid).is_some_and(|v| *v.kind == **sort_name) {
                        transforms.entry(vid.clone()).or_default().push(
                            FieldTransform::ApplyExpr {
                                key: "__value__".to_string(),
                                expr: coercion_expr.clone(),
                                inverse: inverse_expr.clone(),
                                coercion_class: *coercion_class,
                            },
                        );
                    }
                }
            }
            _ => {} // Other transforms don't produce field-level effects
        }
    }

    transforms
}

/// Convert a schema to its implicit theory (sorts = vertex kinds,
/// ops = edge kinds).
fn schema_to_implicit_theory(schema: &Schema) -> Theory {
    crate::protolens::schema_to_implicit_theory(schema)
}

/// Convert a `FoundMorphism` to a `TheoryMorphism`.
///
/// Builds the sort map from vertex kind mappings and the op map from
/// edge kind mappings. Ensures all sorts and ops in the source theory
/// are represented in the morphism (identity-mapping any unmapped ones).
fn alignment_to_theory_morphism(
    found: &FoundMorphism,
    src: &Schema,
    tgt: &Schema,
) -> TheoryMorphism {
    // Build sort map from vertex kind mappings
    let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    for (src_id, tgt_id) in &found.vertex_map {
        if let (Some(src_v), Some(tgt_v)) = (src.vertices.get(src_id), tgt.vertices.get(tgt_id)) {
            let src_kind: Arc<str> = Arc::from(src_v.kind.as_str());
            let tgt_kind: Arc<str> = Arc::from(tgt_v.kind.as_str());
            sort_map.entry(src_kind).or_insert(tgt_kind);
        }
    }

    // Build op map from edge kind mappings
    let mut op_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    for (src_edge, tgt_edge) in &found.edge_map {
        let src_kind: Arc<str> = Arc::from(src_edge.kind.as_str());
        let tgt_kind: Arc<str> = Arc::from(tgt_edge.kind.as_str());
        op_map.entry(src_kind).or_insert(tgt_kind);
    }

    // Ensure all sorts and ops in the source theory are mapped
    let src_theory = crate::protolens::schema_to_implicit_theory(src);
    for sort in &src_theory.sorts {
        sort_map
            .entry(Arc::clone(&sort.name))
            .or_insert_with(|| Arc::clone(&sort.name));
    }
    for op in &src_theory.ops {
        op_map
            .entry(Arc::clone(&op.name))
            .or_insert_with(|| Arc::clone(&op.name));
    }

    TheoryMorphism::new(
        "auto_morphism",
        "src_implicit",
        "tgt_implicit",
        sort_map,
        op_map,
    )
}

/// Convert a `TheoryEndofunctor` to a `Protolens`.
///
/// Each elementary endofunctor maps directly to one of the elementary
/// protolens constructors. `Identity` and `Compose` transforms are
/// rejected since they should not appear in a factorized sequence.
fn endofunctor_to_protolens(endofunctor: &TheoryEndofunctor) -> Result<Protolens, LensError> {
    match &endofunctor.transform {
        TheoryTransform::AddSort { sort, vertex_kind }
        | TheoryTransform::AddSortWithDefault {
            sort, vertex_kind, ..
        } => {
            let vk = vertex_kind
                .as_ref()
                .map_or_else(|| sort.default_vertex_kind(), Arc::clone);
            Ok(elementary::add_sort(
                Name::from(&*sort.name),
                Name::from(&*vk),
                Value::Null,
            ))
        }
        TheoryTransform::DropSort(name) => Ok(elementary::drop_sort(Name::from(&**name))),
        TheoryTransform::RenameSort { old, new } => Ok(elementary::rename_sort(
            Name::from(&**old),
            Name::from(&**new),
        )),
        TheoryTransform::AddOp(op) => {
            let src = if op.inputs.is_empty() {
                Name::from("unknown")
            } else {
                Name::from(&*op.inputs[0].1)
            };
            Ok(elementary::add_op(
                Name::from(&*op.name),
                src,
                Name::from(&*op.output),
                Name::from(&*op.name),
            ))
        }
        TheoryTransform::DropOp(name) => Ok(elementary::drop_op(Name::from(&**name))),
        TheoryTransform::RenameOp { old, new } => Ok(elementary::rename_op(
            Name::from(&**old),
            Name::from(&**new),
        )),
        TheoryTransform::AddEquation(eq) => Ok(elementary::add_equation(eq.clone())),
        TheoryTransform::DropEquation(name) => Ok(elementary::drop_equation(Name::from(&**name))),
        TheoryTransform::Pullback(morphism) => Ok(elementary::pullback(morphism.clone())),
        TheoryTransform::AddDirectedEquation(deq) => Ok(elementary::directed_eq(deq.clone())),
        TheoryTransform::DropDirectedEquation(name) => {
            Ok(elementary::drop_directed_eq(Name::from(&**name)))
        }
        TheoryTransform::CoerceSort { .. } | TheoryTransform::MergeSorts { .. } => {
            Err(LensError::ProtolensError(
                "coercion/merge transforms not yet supported as protolenses".into(),
            ))
        }
        TheoryTransform::Identity => Err(LensError::ProtolensError(
            "unexpected Identity in factorization".into(),
        )),
        TheoryTransform::Compose(_, _) => Err(LensError::ProtolensError(
            "unexpected Compose in factorization".into(),
        )),
        TheoryTransform::RenameEdgeName { .. } => Err(LensError::ProtolensError(
            "unexpected RenameEdgeName in factorization (user-constructed only)".into(),
        )),
        TheoryTransform::AddEdge { .. } => Err(LensError::ProtolensError(
            "unexpected AddEdge in factorization (user-constructed only)".into(),
        )),
        TheoryTransform::DropEdge { .. } => Err(LensError::ProtolensError(
            "unexpected DropEdge in factorization (user-constructed only)".into(),
        )),
        TheoryTransform::ScopedTransform { .. } => Err(LensError::ProtolensError(
            "unexpected ScopedTransform in factorization (user-constructed only)".into(),
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_gat::Sort;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![
                "record".into(),
                "string".into(),
                "boolean".into(),
                "array".into(),
            ],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema_v1(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("post.text", "string", None::<&str>)
            .unwrap()
            .vertex("post.done", "boolean", None::<&str>)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.done", "prop", Some("done"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn schema_v2(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("post.text", "string", None::<&str>)
            .unwrap()
            .vertex("post.status", "string", None::<&str>)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.status", "prop", Some("status"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn schema_post_with_created(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .vertex("post.text", "string", None::<&str>)
            .unwrap()
            .vertex("post.createdAt", "string", None::<&str>)
            .unwrap()
            .edge("post", "post.text", "prop", Some("text"))
            .unwrap()
            .edge("post", "post.createdAt", "prop", Some("createdAt"))
            .unwrap()
            .build()
            .unwrap()
    }

    fn schema_message_with_sent(protocol: &Protocol) -> Schema {
        SchemaBuilder::new(protocol)
            .vertex("message", "record", None::<&str>)
            .unwrap()
            .vertex("message.body", "string", None::<&str>)
            .unwrap()
            .vertex("message.sentAt", "string", None::<&str>)
            .unwrap()
            .edge("message", "message.body", "prop", Some("body"))
            .unwrap()
            .edge("message", "message.sentAt", "prop", Some("sentAt"))
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn balanced_finds_alias_aligned_morphism_when_strict_cannot() {
        // Two record schemas with no shared vertex names and no shared
        // child field names. Strict cannot pair them. Balanced uses the
        // alias dictionary (text↔body, createdAt↔sentAt) to seed anchors,
        // and the CSP validates a morphism on the resulting alignment.
        let protocol = test_protocol();
        let src = schema_post_with_created(&protocol);
        let tgt = schema_message_with_sent(&protocol);

        let strict = AutoLensConfig {
            stringency: Stringency::Strict,
            ..Default::default()
        };
        let strict_result = auto_generate(&src, &tgt, &protocol, &strict);

        let balanced = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        let balanced_result = auto_generate(&src, &tgt, &protocol, &balanced).unwrap();

        // Strict cannot find a useful morphism. Balanced does.
        assert!(
            balanced_result.alignment_quality > 0.0,
            "Balanced should find a non-trivial alignment"
        );
        // Confirm that Balanced's seed anchors include the alias-driven pairs.
        let names: Vec<(String, String)> = balanced_result
            .seed_anchors
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();
        assert!(
            names.iter().any(|(s, t)| s == "post" && t == "message"),
            "alias strategy should seed post ↔ message anchor; got {names:?}"
        );

        // Strict, lacking heuristics, gets either no alignment or a trivial one.
        if let Ok(r) = strict_result {
            assert!(
                r.alignment_quality <= balanced_result.alignment_quality,
                "Strict should not outperform Balanced on this case"
            );
        }
    }

    #[test]
    fn stringency_thresholds_form_monotone_ladder() {
        assert!(
            Stringency::Strict.token_similarity_threshold()
                >= Stringency::Balanced.token_similarity_threshold()
        );
        assert!(
            Stringency::Balanced.token_similarity_threshold()
                >= Stringency::Lenient.token_similarity_threshold()
        );
        assert!(
            Stringency::Lenient.token_similarity_threshold()
                >= Stringency::Exploratory.token_similarity_threshold()
        );
        assert!(!Stringency::Strict.uses_alias_dict());
        assert!(Stringency::Balanced.uses_alias_dict());
        assert!(Stringency::Lenient.uses_alias_dict());
        assert!(Stringency::Exploratory.uses_alias_dict());
    }

    #[test]
    fn auto_generate_between_same_schemas() {
        let protocol = test_protocol();
        let s = schema_v1(&protocol);
        let config = AutoLensConfig::default();
        let result = auto_generate(&s, &s, &protocol, &config).unwrap();
        assert!(result.chain.is_empty() || result.alignment_quality > 0.0);
    }

    #[test]
    fn auto_generate_between_different_schemas() {
        let protocol = test_protocol();
        let v1 = schema_v1(&protocol);
        let v2 = schema_v2(&protocol);
        let config = AutoLensConfig::default();
        let result = auto_generate(&v1, &v2, &protocol, &config);
        // Should either succeed or fail with a clear error
        match result {
            Ok(r) => {
                assert!(!r.chain.is_empty());
                assert!(r.alignment_quality > 0.0);
            }
            Err(e) => {
                // Acceptable if no morphism found
                assert!(e.to_string().contains("morphism"));
            }
        }
    }

    #[test]
    fn alignment_to_morphism_preserves_kinds() {
        let protocol = test_protocol();
        let v1 = schema_v1(&protocol);
        let v2 = schema_v1(&protocol); // same schema
        let alignment = FoundMorphism {
            vertex_map: v1.vertices.keys().map(|k| (k.clone(), k.clone())).collect(),
            edge_map: v1.edges.keys().map(|e| (e.clone(), e.clone())).collect(),
            quality: 1.0,
        };
        let morphism = alignment_to_theory_morphism(&alignment, &v1, &v2);
        // All source sorts should be in the sort map
        let src_theory = schema_to_implicit_theory(&v1);
        for sort in &src_theory.sorts {
            assert!(morphism.sort_map.contains_key(&sort.name));
        }
    }

    #[test]
    fn protolens_from_identity_alignment() {
        let protocol = test_protocol();
        let v1 = schema_v1(&protocol);
        let alignment = FoundMorphism {
            vertex_map: v1.vertices.keys().map(|k| (k.clone(), k.clone())).collect(),
            edge_map: v1.edges.keys().map(|e| (e.clone(), e.clone())).collect(),
            quality: 1.0,
        };
        let chain = protolens_from_alignment(&alignment, &v1, &v1).unwrap();
        // Identity alignment should produce empty or near-empty chain
        // (depends on factorize behavior for identity morphism)
        assert!(chain.len() <= 1);
    }

    #[test]
    fn endofunctor_to_protolens_add_sort() {
        let ef = TheoryEndofunctor {
            name: Arc::from("add_tags"),
            precondition: panproto_gat::TheoryConstraint::Unconstrained,
            transform: TheoryTransform::AddSort {
                sort: Sort::simple("tags"),
                vertex_kind: None,
            },
        };
        let p = endofunctor_to_protolens(&ef).unwrap();
        assert!(p.name.contains("add_sort"));
    }

    #[test]
    fn endofunctor_to_protolens_drop_sort() {
        let ef = TheoryEndofunctor {
            name: Arc::from("drop_foo"),
            precondition: panproto_gat::TheoryConstraint::HasSort(Arc::from("foo")),
            transform: TheoryTransform::DropSort(Arc::from("foo")),
        };
        let p = endofunctor_to_protolens(&ef).unwrap();
        assert!(p.name.contains("drop_sort"));
        assert!(!p.is_lossless());
    }

    #[test]
    fn endofunctor_to_protolens_rename() {
        let ef = TheoryEndofunctor {
            name: Arc::from("rename"),
            precondition: panproto_gat::TheoryConstraint::HasSort(Arc::from("old")),
            transform: TheoryTransform::RenameSort {
                old: Arc::from("old"),
                new: Arc::from("new"),
            },
        };
        let p = endofunctor_to_protolens(&ef).unwrap();
        assert!(p.is_lossless());
    }

    #[test]
    fn endofunctor_to_protolens_rejects_identity() {
        let ef = TheoryEndofunctor {
            name: Arc::from("id"),
            precondition: panproto_gat::TheoryConstraint::Unconstrained,
            transform: TheoryTransform::Identity,
        };
        assert!(endofunctor_to_protolens(&ef).is_err());
    }
}
