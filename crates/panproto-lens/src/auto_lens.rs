//! Automatic protolens generation pipeline.
//!
//! Given two schemas, auto-discovers morphism alignment, factorizes
//! it into elementary endofunctors, maps each to a protolens, and
//! composes the result.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{Name, Theory, TheoryEndofunctor, TheoryMorphism, TheoryTransform, factorize};
use panproto_inst::value::Value;
use panproto_mig::align::{self, AliasDict, Anchor, CoerceAnchor, default_alias_dict};
use panproto_mig::hom_search::{
    DomainConstraints, FoundMorphism, SearchOptions, find_best_morphism,
    find_best_morphism_constrained, find_morphisms, find_morphisms_constrained,
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

    /// Whether the structural (degree + kind-signature) strategy runs
    /// at this tier. Only Exploratory enables this fallback.
    #[must_use]
    pub const fn uses_structural(self) -> bool {
        matches!(self, Self::Exploratory)
    }

    /// Whether the sort-coercion witness strategy runs at this tier.
    ///
    /// Only Exploratory fires `align::coerce::coerce_anchors`; lower
    /// tiers reject kind-mismatched vertex pairs entirely. When
    /// `Exploratory` is active, the coerce strategy proposes anchors
    /// bridgeable by a library witness (`int ↔ str`, and so on). The
    /// CSP still validates naturality on every proposal.
    #[must_use]
    pub const fn uses_coerce(self) -> bool {
        matches!(self, Self::Exploratory)
    }

    /// Minimum similarity floor for the structural strategy at this tier.
    /// Irrelevant for tiers where structural is disabled.
    #[must_use]
    pub const fn structural_threshold(self) -> f64 {
        match self {
            Self::Exploratory => 0.40,
            _ => 1.0,
        }
    }

    /// Whether the overlap-based fallback is turned on automatically
    /// at this tier when the caller hasn't pinned `try_overlap`.
    #[must_use]
    pub const fn default_try_overlap(self) -> bool {
        matches!(self, Self::Lenient | Self::Exploratory)
    }

    /// Whether the engine emits spans `A ←f− C −g→ B` rather than
    /// identity-filled total morphisms at this tier.
    ///
    /// When enabled, source sorts/ops with no matched counterpart
    /// surface as explicit `DropSort`/`DropOp` endofunctors in the
    /// factorization, and target extensions surface as
    /// `AddSort`/`AddOp`. The resulting lens is genuinely partial on
    /// the source side — round-trip laws hold on `C`, not on `A`.
    #[must_use]
    pub const fn allow_spans(self) -> bool {
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

    /// User-facing lowercase token (`"strict"`, `"balanced"`,
    /// `"lenient"`, `"exploratory"`). Matches the tier names accepted
    /// by the CLI / SDK parsers and by `serde::Serialize` under
    /// `rename_all = "snake_case"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Balanced => "balanced",
            Self::Lenient => "lenient",
            Self::Exploratory => "exploratory",
        }
    }
}

impl std::fmt::Display for Stringency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
    /// Sort-coercion proposals emitted at the Exploratory tier by
    /// [`align::coerce_anchors`]. Each carries the witness name and
    /// [`panproto_gat::CoercionClass`] so downstream code can look the
    /// witness back up in a [`panproto_mig::coerce::WitnessLibrary`]
    /// and emit a `TheoryTransform::CoerceSort` endofunctor. The bare
    /// `.anchor` field of each entry has already been merged into the
    /// CSP seed pool, but the witness metadata does not flow through
    /// the CSP and must be consumed separately.
    ///
    /// Empty at every tier except [`Stringency::Exploratory`], and
    /// empty at Exploratory when no kind pair in the schema matches a
    /// library witness.
    pub coerce_proposals: Vec<CoerceAnchor>,
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
fn run_strategies(
    src: &Schema,
    tgt: &Schema,
    config: &AutoLensConfig,
) -> (Vec<Anchor>, Vec<CoerceAnchor>) {
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

    if config.stringency.uses_structural() {
        let threshold = config.stringency.structural_threshold();
        anchors.extend(align::structural_anchors(src, tgt, threshold));
    }

    let coerce_proposals = if config.stringency.uses_coerce() {
        // Consult the default witness library for kind-bridging
        // proposals. Each proposal contributes its bare `.anchor` to
        // the CSP seed pool and its witness metadata (name + class)
        // to the returned proposal vector so downstream callers can
        // synthesize a `TheoryTransform::CoerceSort` endofunctor.
        //
        // Today, the CSP's `kinds_compatible` filter still rejects
        // kind-mismatched pairs, so bare coerce anchors do not
        // influence the morphism search. The real value of wiring
        // them up is making the witness proposals accessible to
        // downstream code (CLI, DSL) that decides independently
        // whether to emit CoerceSort transforms. A future pass can
        // relax the CSP kind filter conditional on the presence of a
        // registered witness.
        let library = panproto_mig::coerce::default_witness_library();
        let proposals = align::coerce_anchors(src, tgt, &library);
        for ca in &proposals {
            anchors.push(ca.anchor.clone());
        }
        proposals
    } else {
        Vec::new()
    };

    (anchors, coerce_proposals)
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
///
/// Other `SearchOptions` fields are intentionally *not* forced by the
/// tier:
///
/// * `monic` / `epic` / `iso`: these encode categorical properties of
///   the morphism itself, not search aggressiveness; flipping them at
///   `Strict` would reject perfectly valid identity-fill morphisms on
///   partial schemas. Callers who want a monic search pass it in via
///   `AutoLensConfig::search_opts`.
/// * `max_results`: candidate APIs set this per-call; the single-best
///   entry point leaves it at the default.
/// * `initial`: seeded separately via `merge_seed_anchors`.
const fn apply_stringency_search_opts(opts: &mut SearchOptions, stringency: Stringency) {
    if stringency.relax_edge_name_pruning() {
        opts.relax_edge_name_pruning = true;
    }
}

/// Identify source vertices with no kind-compatible target vertex.
///
/// Used at `Lenient`+ to pre-populate `DomainConstraints.excluded_sources`
/// so the CSP can still find a morphism on the shared subschema `C`
/// rather than failing outright when a source sort has no counterpart.
fn sources_without_compatible_targets(src: &Schema, tgt: &Schema) -> Vec<Name> {
    let tgt_kinds: std::collections::HashSet<&str> =
        tgt.vertices.values().map(|v| v.kind.as_str()).collect();
    let mut out: Vec<Name> = src
        .vertices
        .iter()
        .filter_map(|(id, vertex)| {
            if tgt_kinds.contains(vertex.kind.as_str()) {
                None
            } else {
                Some(id.clone())
            }
        })
        .collect();
    // HashMap iteration is nondeterministic; sort so the derived
    // `DomainConstraints.excluded_sources` and any downstream log are
    // stable across runs.
    out.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    out
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
    let (seed_anchors, coerce_proposals) = run_strategies(src, tgt, config);
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

    // Span search: at Lenient+, pre-exclude source vertices with no
    // kind-compatible target. Excluded vertices surface as `DropSort`
    // endofunctors in the factorized chain — the left leg of the span
    // `A ←f− C −g→ B`.
    let span_constraints = span_exclusions_at_lenient(src, tgt, config.stringency);

    let result = run_search(
        src,
        tgt,
        protocol,
        &effective,
        &search_opts,
        span_constraints.as_ref(),
        DEFAULT_QUALITY_FLOOR,
    )?;

    Ok(AutoLensResult {
        chain: result.chain,
        lens: result.lens,
        alignment_quality: result.alignment_quality,
        seed_anchors,
        coerce_proposals,
    })
}

/// Default alignment quality below which `auto_generate` triggers the
/// overlap-fallback retry when `try_overlap` is enabled. `auto_generate_with_hints`
/// accepts a `quality_threshold` override that shadows this constant.
const DEFAULT_QUALITY_FLOOR: f64 = 0.5;

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
    quality_floor: f64,
) -> Result<SearchResult, LensError> {
    let search = |opts: &SearchOptions| -> Option<FoundMorphism> {
        domain_constraints.map_or_else(
            || find_best_morphism(src, tgt, opts),
            |dc| find_best_morphism_constrained(src, tgt, opts, dc),
        )
    };

    let mut alignment = search(search_opts);

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
    let chain =
        protolens_from_alignment_mode(&alignment, src, tgt, config.stringency.allow_spans())?;
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
    // `None` keeps the library default; `Some(x)` overrides the overlap-fallback
    // floor. NaN is rejected — silently coercing it would make overlap either
    // always or never fire depending on comparator quirks.
    let quality_floor = match quality_threshold {
        None => DEFAULT_QUALITY_FLOOR,
        Some(x) if x.is_nan() => {
            return Err(LensError::ProtolensError(
                "quality_threshold must not be NaN".into(),
            ));
        }
        Some(x) => x.clamp(0.0, 1.0),
    };

    let (strategy_anchors, coerce_proposals) = run_strategies(src, tgt, config);
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

    // Span search: at Lenient+, fold source vertices with no
    // kind-compatible target into `excluded_sources` so the CSP
    // searches the shared subschema C instead of failing outright.
    // User-supplied `domain_constraints` are preserved; the span
    // exclusions are UNIONED in rather than replacing them.
    let mut merged_domain = domain_constraints.clone();
    if let Some(span) = span_exclusions_at_lenient(src, tgt, config.stringency) {
        merged_domain.excluded_sources.extend(span.excluded_sources);
    }

    let result = run_search(
        src,
        tgt,
        protocol,
        &effective,
        &search_opts,
        Some(&merged_domain),
        quality_floor,
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
        coerce_proposals,
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
    protolens_from_alignment_mode(alignment, src, tgt, false)
}

/// Generate a protolens chain from an alignment with explicit span
/// emission control.
///
/// When `emit_spans` is `true`, source sorts/ops that the alignment
/// did not witness are left out of the theory morphism; [`factorize`]
/// then emits `DropSort`/`DropOp` endofunctors for them, and
/// `AddSort`/`AddOp` for the target extensions. This realizes the span
/// `A ←f− C −g→ B` described in the plan.
///
/// When `emit_spans` is `false`, unmapped source sorts/ops are
/// identity-filled and no drops are emitted; behaviour matches the
/// classic total-morphism path.
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] if factorization fails or
/// an endofunctor cannot be mapped to a protolens.
pub fn protolens_from_alignment_mode(
    alignment: &FoundMorphism,
    src: &Schema,
    tgt: &Schema,
    emit_spans: bool,
) -> Result<ProtolensChain, LensError> {
    let src_theory = schema_to_implicit_theory(src);
    let tgt_theory = schema_to_implicit_theory(tgt);
    let morphism = alignment_to_theory_morphism_mode(alignment, src, tgt, emit_spans);

    let factorization = factorize(&morphism, &src_theory, &tgt_theory)
        .map_err(|e| LensError::ProtolensError(format!("factorization failed: {e}")))?;

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
                // For a pattern `f(x) ⇒ g(x)` the field being rewritten is
                // the op `f`, not the bound variable `x`. A bare `Var` LHS
                // is not a meaningful rewrite target at the instance
                // level, so we skip it rather than plumb the variable
                // name through as a phantom field key.
                let Some(key) = (match &deq.lhs {
                    panproto_gat::Term::App { op, .. } => Some(op.to_string()),
                    panproto_gat::Term::Var(_) => None,
                }) else {
                    continue;
                };
                // Apply the rewrite only to vertices that actually have
                // an outgoing edge with this name. The previous
                // implementation pushed the transform onto *every*
                // vertex in the schema, including ones that never
                // mention the op — which both ballooned the complement
                // and ran the equation on unrelated field values.
                for vid in src.vertices.keys() {
                    let has_edge = src
                        .outgoing_edges(vid)
                        .iter()
                        .any(|e| e.name.as_deref() == Some(key.as_str()));
                    if !has_edge {
                        continue;
                    }
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

/// Generate up to `top_n` ranked candidate lenses between `src` and `tgt`.
///
/// Runs the alignment strategies enabled by `config.stringency`,
/// seeds the CSP solver, and returns the top-N morphisms sorted by
/// the composite score `quality + 0.5 · coverage + 0.2 · avg_step_confidence`.
///
/// Every candidate is a fully-validated theory morphism: naturality is
/// enforced during CSP backtracking, and each candidate's protolens
/// chain is instantiated at the source schema so the user can run
/// `get`/`put` immediately. The candidate's `steps` vector carries
/// per-step explanations sourced from whichever alignment strategy
/// motivated the rename/add/drop (see [`enrich_steps`]).
///
/// `top_n == 0` is treated as `top_n == 1`. The returned vector is
/// non-empty on success; a [`LensError::ProtolensError`] is returned
/// when no morphism exists.
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] when no morphism exists at
/// the configured stringency tier.
///
/// [`enrich_steps`]: crate::candidate::enrich_steps
pub fn auto_generate_candidates(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    config: &AutoLensConfig,
    top_n: usize,
) -> Result<Vec<crate::candidate::LensCandidate>, LensError> {
    let n = top_n.max(1);
    let (seed_anchors, _coerce_proposals) = run_strategies(src, tgt, config);
    let resolved = align::resolve_anchors(&seed_anchors, config.search_opts.monic);

    let mut search_opts = config.search_opts.clone();
    apply_stringency_search_opts(&mut search_opts, config.stringency);
    merge_seed_anchors(&mut search_opts, &resolved);
    search_opts.max_results = n;

    // Span search: at Lenient+ pre-exclude source vertices with no
    // kind-compatible target so the CSP can still find a morphism on
    // the shared subschema.
    let span_constraints = span_exclusions_at_lenient(src, tgt, config.stringency);

    candidates_from_search(
        src,
        tgt,
        protocol,
        &search_opts,
        span_constraints.as_ref(),
        &seed_anchors,
        n,
        config.stringency.allow_spans(),
    )
}

/// Build `DomainConstraints` with auto-derived `excluded_sources` for
/// span search. Returns `None` at tiers where spans are not allowed or
/// when every source vertex has a compatible target.
fn span_exclusions_at_lenient(
    src: &Schema,
    tgt: &Schema,
    stringency: Stringency,
) -> Option<DomainConstraints> {
    if !stringency.allow_spans() {
        return None;
    }
    let to_drop = sources_without_compatible_targets(src, tgt);
    if to_drop.is_empty() {
        return None;
    }
    let mut dc = DomainConstraints::default();
    dc.excluded_sources.extend(to_drop);
    Some(dc)
}

/// Candidate-API variant accepting caller-supplied anchors and domain
/// constraints.
///
/// User anchors take precedence over strategy anchors; neither
/// overrules the CSP's naturality check.
///
/// # Errors
///
/// Returns [`LensError::ProtolensError`] when no morphism exists under
/// the combined constraints.
pub fn auto_generate_candidates_with_hints(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    config: &AutoLensConfig,
    anchors: &HashMap<Name, Name>,
    domain_constraints: &DomainConstraints,
    top_n: usize,
) -> Result<Vec<crate::candidate::LensCandidate>, LensError> {
    let n = top_n.max(1);
    let (strategy_anchors, _coerce_proposals) = run_strategies(src, tgt, config);
    let resolved_strategy = align::resolve_anchors(&strategy_anchors, config.search_opts.monic);

    let mut search_opts = config.search_opts.clone();
    apply_stringency_search_opts(&mut search_opts, config.stringency);
    for (src_v, tgt_v) in anchors {
        search_opts.initial.insert(src_v.clone(), tgt_v.clone());
    }
    merge_seed_anchors(&mut search_opts, &resolved_strategy);
    search_opts.max_results = n;

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

    candidates_from_search(
        src,
        tgt,
        protocol,
        &search_opts,
        Some(domain_constraints),
        &combined,
        n,
        config.stringency.allow_spans(),
    )
}

/// Concatenation of `chain.steps[i].name` used as a deterministic
/// tiebreak key for candidate ordering.
fn chain_step_names(chain: &ProtolensChain) -> String {
    let mut out = String::new();
    for step in &chain.steps {
        out.push_str(step.name.as_str());
        out.push('|');
    }
    out
}

/// Shared engine for multi-candidate generation. Runs the CSP, builds
/// one candidate per returned morphism, scores them, and truncates to
/// `n` entries sorted by composite score.
#[allow(clippy::too_many_arguments)]
fn candidates_from_search(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    search_opts: &SearchOptions,
    domain_constraints: Option<&DomainConstraints>,
    seed_anchors: &[Anchor],
    n: usize,
    emit_spans: bool,
) -> Result<Vec<crate::candidate::LensCandidate>, LensError> {
    let morphisms = domain_constraints.map_or_else(
        || find_morphisms(src, tgt, search_opts),
        |dc| find_morphisms_constrained(src, tgt, search_opts, dc),
    );

    if morphisms.is_empty() {
        return Err(LensError::ProtolensError(
            "no morphism found between schemas".into(),
        ));
    }

    let mut candidates = Vec::with_capacity(morphisms.len());
    // Track the most recent factorization failure so that, if EVERY
    // morphism fails to realize as a protolens, we can surface the
    // underlying cause rather than the generic "no morphism could be
    // realized" message. Silently swallowing every error made real
    // structural bugs invisible.
    let mut last_failure: Option<LensError> = None;
    for morphism in morphisms {
        match candidate_from_morphism(src, tgt, protocol, &morphism, seed_anchors, emit_spans) {
            Ok(cand) => candidates.push(cand),
            Err(e) => {
                last_failure = Some(e);
                continue;
            }
        }
        if candidates.len() >= n.saturating_mul(2) {
            // Generate at most 2×n raw candidates before scoring; saves
            // instantiation work on the long tail of low-quality results.
            break;
        }
    }

    if candidates.is_empty() {
        return Err(last_failure.map_or_else(
            || LensError::ProtolensError("no morphism could be realized as a protolens".into()),
            |e| {
                LensError::ProtolensError(format!(
                    "no morphism could be realized as a protolens; \
                     last factorization failure: {e}"
                ))
            },
        ));
    }

    // Sort by descending composite score; break ties deterministically
    // by (shorter chain, then lexicographic concatenation of step names).
    // Without this, two equally-scored candidates produced in different
    // CSP iterations could swap order across runs.
    //
    // `f64::total_cmp` gives a total order even in the presence of NaN,
    // so a degenerate score cannot silently swap position with a valid
    // neighbor. NaN values sort to the bottom (after positive infinity).
    candidates.sort_by(|a, b| {
        b.score()
            .total_cmp(&a.score())
            .then_with(|| a.chain.steps.len().cmp(&b.chain.steps.len()))
            .then_with(|| chain_step_names(&a.chain).cmp(&chain_step_names(&b.chain)))
    });
    candidates.truncate(n);
    Ok(candidates)
}

/// Build a single candidate from one CSP result. Factorizes the
/// morphism, instantiates its chain at `src`, and enriches the step
/// list with explanations sourced from the seed anchors.
fn candidate_from_morphism(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    morphism: &FoundMorphism,
    seed_anchors: &[Anchor],
    emit_spans: bool,
) -> Result<crate::candidate::LensCandidate, LensError> {
    let chain = protolens_from_alignment_mode(morphism, src, tgt, emit_spans)?;
    let mut lens = chain.instantiate(src, protocol)?;
    lens.compiled.field_transforms = derive_field_transforms(&chain, src, tgt);

    let coverage = crate::candidate::coverage_ratio(
        src,
        tgt,
        crate::candidate::matched_count(&morphism.vertex_map),
    );
    let steps = crate::candidate::enrich_steps(&chain, seed_anchors);
    let strategies_used = crate::candidate::strategies_used(seed_anchors);

    Ok(crate::candidate::LensCandidate {
        chain,
        lens,
        quality: morphism.quality,
        coverage,
        seed_anchors: seed_anchors.to_vec(),
        steps,
        strategies_used,
    })
}

/// Convert a `FoundMorphism` to a `TheoryMorphism`.
///
/// Builds the sort map from vertex kind mappings and the op map from
/// edge kind mappings. Ensures all sorts and ops in the source theory
/// are represented in the morphism (identity-mapping any unmapped ones).
/// Build the theory morphism corresponding to `found` (total-morphism mode).
#[cfg(test)]
fn alignment_to_theory_morphism(
    found: &FoundMorphism,
    src: &Schema,
    tgt: &Schema,
) -> TheoryMorphism {
    alignment_to_theory_morphism_mode(found, src, tgt, false)
}

/// Build the theory morphism corresponding to `found`.
///
/// When `emit_spans` is `false`, the resulting morphism identity-fills
/// any source sort or op that `found` did not witness — that is, the
/// morphism claims to interpret every source sort by itself, and
/// `factorize` will not emit any `DropSort` / `DropOp` endofunctors.
///
/// When `emit_spans` is `true`, identity-fill is skipped for sorts and
/// ops that never appear in the morphism's vertex/edge maps. The
/// resulting morphism is the left leg of a span `A ←f− C −g→ B` where
/// `C` is the shared subtheory spanned by the matched vertices;
/// `factorize` emits `DropSort(s)` for every source sort with no image
/// in `C` and `DropOp(op)` for every source op likewise, and emits
/// `AddSort`/`AddOp` for the right-leg extensions into `B`. This is
/// the span-search behavior plan §3 describes.
fn alignment_to_theory_morphism_mode(
    found: &FoundMorphism,
    src: &Schema,
    tgt: &Schema,
    emit_spans: bool,
) -> TheoryMorphism {
    // Build sort map from vertex kind mappings. Iterating `vertex_map`
    // (a HashMap) in its native order would let the first `or_insert`
    // winner vary across runs when multiple source vertices share a
    // kind but map to different target kinds. Sort by source vertex id
    // so ties break deterministically.
    let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    let mut vertex_pairs: Vec<(&Name, &Name)> = found.vertex_map.iter().collect();
    vertex_pairs.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    for (src_id, tgt_id) in vertex_pairs {
        if let (Some(src_v), Some(tgt_v)) = (src.vertices.get(src_id), tgt.vertices.get(tgt_id)) {
            let src_kind: Arc<str> = Arc::from(src_v.kind.as_str());
            let tgt_kind: Arc<str> = Arc::from(tgt_v.kind.as_str());
            sort_map.entry(src_kind).or_insert(tgt_kind);
        }
    }

    // Build op map from edge kind mappings (same determinism concern as
    // `sort_map` above).
    let mut op_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    let mut edge_pairs: Vec<(&panproto_schema::Edge, &panproto_schema::Edge)> =
        found.edge_map.iter().collect();
    edge_pairs.sort_by(|a, b| {
        a.0.src
            .as_str()
            .cmp(b.0.src.as_str())
            .then_with(|| a.0.tgt.as_str().cmp(b.0.tgt.as_str()))
            .then_with(|| a.0.kind.as_str().cmp(b.0.kind.as_str()))
    });
    for (src_edge, tgt_edge) in edge_pairs {
        let src_kind: Arc<str> = Arc::from(src_edge.kind.as_str());
        let tgt_kind: Arc<str> = Arc::from(tgt_edge.kind.as_str());
        op_map.entry(src_kind).or_insert(tgt_kind);
    }

    if !emit_spans {
        // Identity-fill any source sort or op the alignment didn't
        // witness. Preserves the classic total-morphism semantics.
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
        TheoryTransform::AddSort { sort, vertex_kind } => {
            let vk = vertex_kind
                .as_ref()
                .map_or_else(|| sort.default_vertex_kind(), Arc::clone);
            Ok(elementary::add_sort(
                Name::from(&*sort.name),
                Name::from(&*vk),
                Value::Null,
            ))
        }
        TheoryTransform::AddSortWithDefault {
            sort,
            vertex_kind,
            default_expr,
        } => {
            // Previously this variant was collapsed into `AddSort`, which
            // discarded `default_expr` — the zero-element of the pushout.
            // Route it through the dedicated elementary so the expression
            // survives to migration-time evaluation.
            let vk = vertex_kind
                .as_ref()
                .map_or_else(|| sort.default_vertex_kind(), Arc::clone);
            Ok(elementary::add_sort_with_default(
                Name::from(&*sort.name),
                Name::from(&*vk),
                default_expr.clone(),
            ))
        }
        TheoryTransform::DropSort(name) => Ok(elementary::drop_sort(Name::from(&**name))),
        TheoryTransform::RenameSort { old, new } => Ok(elementary::rename_sort(
            Name::from(&**old),
            Name::from(&**new),
        )),
        TheoryTransform::AddOp(op) => {
            // A protolens `AddOp` needs a source sort to anchor the edge at.
            // A theory operation with no inputs is a constant; synthesizing
            // a `"unknown"` sentinel produces an ill-formed elementary that
            // silently corrupts downstream factorization. Surface it as a
            // real error so callers can add an explicit input sort or
            // reroute constants through `AddSortWithDefault`.
            let Some((_, input_sort)) = op.inputs.first() else {
                return Err(LensError::ProtolensError(format!(
                    "AddOp '{}' has no inputs; elementary add_op requires a source sort. \
                     Supply an explicit input sort or route constants through AddSortWithDefault.",
                    op.name
                )));
            };
            Ok(elementary::add_op(
                Name::from(&*op.name),
                Name::from(&**input_sort),
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
        TheoryTransform::CoerceSort {
            sort_name,
            target_kind,
            coercion_expr,
            inverse_expr,
            coercion_class,
        } => Ok(elementary::sort_coerce(
            Name::from(&**sort_name),
            *target_kind,
            coercion_expr.clone(),
            inverse_expr.clone(),
            *coercion_class,
        )),
        TheoryTransform::MergeSorts { .. } => Err(LensError::ProtolensError(
            "merge transforms not yet supported as protolenses".into(),
        )),
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use panproto_gat::Sort;
    use panproto_schema::{Protocol, SchemaBuilder};

    #[test]
    fn stringency_as_str_matches_display_and_serde() {
        // `Stringency::as_str` / `Display` / serde (`rename_all =
        // "snake_case"`) all have to render the same four strings, because
        // the CLI, the Python parser, the WASM parser, and the TypeScript
        // `Stringency` union all hardcode this representation.
        for s in [
            Stringency::Strict,
            Stringency::Balanced,
            Stringency::Lenient,
            Stringency::Exploratory,
        ] {
            let as_str = s.as_str();
            let display = format!("{s}");
            let serde = serde_json::to_string(&s).expect("serialize stringency");
            assert_eq!(as_str, display, "Display disagrees with as_str");
            assert_eq!(
                format!("\"{as_str}\""),
                serde,
                "serde wire format disagrees with as_str",
            );
        }
        // Lowercase tier names; snake_case happens to coincide with
        // lowercase for all four variants but lock the exact strings.
        assert_eq!(Stringency::Strict.as_str(), "strict");
        assert_eq!(Stringency::Balanced.as_str(), "balanced");
        assert_eq!(Stringency::Lenient.as_str(), "lenient");
        assert_eq!(Stringency::Exploratory.as_str(), "exploratory");
    }

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
    fn auto_generate_candidates_returns_ranked_non_empty_list() {
        let protocol = test_protocol();
        let src = schema_post_with_created(&protocol);
        let tgt = schema_message_with_sent(&protocol);
        let config = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        let candidates = auto_generate_candidates(&src, &tgt, &protocol, &config, 5)
            .unwrap_or_else(|e| panic!("expected candidates: {e}"));
        assert!(!candidates.is_empty(), "candidates must be non-empty");
        // Scores must be non-increasing by composite score.
        for pair in candidates.windows(2) {
            assert!(
                pair[0].score() >= pair[1].score(),
                "candidates must be sorted by descending composite score"
            );
        }
        // Every candidate carries steps enriched with explanations.
        for cand in &candidates {
            assert!(
                cand.steps.iter().all(|s| !s.explanation.is_empty()),
                "every step needs an explanation; got {:?}",
                cand.steps
            );
        }
    }

    #[test]
    fn auto_generate_candidates_reports_coverage_and_strategies() {
        let protocol = test_protocol();
        let src = schema_post_with_created(&protocol);
        let tgt = schema_message_with_sent(&protocol);
        let config = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        let candidates = auto_generate_candidates(&src, &tgt, &protocol, &config, 1)
            .unwrap_or_else(|e| panic!("candidates: {e}"));
        let top = &candidates[0];
        assert!(
            top.coverage > 0.0 && top.coverage <= 1.0,
            "coverage must be in (0, 1]: {}",
            top.coverage
        );
        assert!(
            !top.strategies_used.is_empty(),
            "Balanced tier should engage at least one strategy"
        );
    }

    #[test]
    fn auto_generate_candidates_errors_when_no_morphism() {
        use panproto_mig::hom_search::SearchOptions;
        // Build two schemas with no kind-compatible vertices; the CSP
        // returns no morphism and `auto_generate_candidates` should
        // surface an error.
        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["alpha".into(), "beta".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let src = SchemaBuilder::new(&protocol)
            .vertex("x", "alpha", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&protocol)
            .vertex("y", "beta", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let config = AutoLensConfig {
            stringency: Stringency::Strict,
            // Require monic to disallow the trivial empty morphism.
            search_opts: SearchOptions {
                monic: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let res = auto_generate_candidates(&src, &tgt, &protocol, &config, 1);
        assert!(
            res.is_err(),
            "expected no-morphism error between disjoint-kind schemas, got {res:?}"
        );
    }

    #[test]
    fn endofunctor_to_protolens_rejects_add_op_with_no_inputs() {
        // Previously this path synthesized a `"unknown"` source sort,
        // which silently produced an ill-formed elementary protolens.
        // The fix converts the case to a real error surfaced by
        // `endofunctor_to_protolens`. Pin the behaviour so a future
        // refactor cannot reintroduce the sentinel.
        use panproto_gat::{Operation, TheoryEndofunctor, TheoryTransform};
        use std::sync::Arc;

        let endo = TheoryEndofunctor {
            name: Arc::from("add_op_constant"),
            precondition: panproto_gat::TheoryConstraint::Unconstrained,
            transform: TheoryTransform::AddOp(Operation {
                name: Arc::from("constant"),
                // Zero inputs — the exact shape the old sentinel accepted.
                inputs: Vec::new(),
                output: Arc::from("int"),
            }),
        };
        let err = endofunctor_to_protolens(&endo)
            .expect_err("AddOp with empty inputs must error, not synthesize 'unknown'");
        let msg = format!("{err}");
        assert!(
            msg.contains("no inputs") && msg.contains("constant"),
            "error must name the op and the reason; got: {msg}"
        );
    }

    #[test]
    fn endofunctor_to_protolens_preserves_add_sort_default_expr() {
        // `AddSortWithDefault` previously collapsed into `AddSort` and
        // discarded `default_expr`. The fix routes it through a dedicated
        // elementary; the resulting protolens's target transform must
        // still be `AddSortWithDefault` carrying the original expression.
        use panproto_expr::Expr;
        use panproto_gat::{Sort, TheoryEndofunctor, TheoryTransform};
        use std::sync::Arc;

        let expr = Expr::Lit(panproto_expr::Literal::Int(42));
        let endo = TheoryEndofunctor {
            name: Arc::from("add_counter"),
            precondition: panproto_gat::TheoryConstraint::Unconstrained,
            transform: TheoryTransform::AddSortWithDefault {
                sort: Sort::simple(Arc::from("counter")),
                vertex_kind: Some(Arc::from("integer")),
                default_expr: expr.clone(),
            },
        };
        let protolens =
            endofunctor_to_protolens(&endo).expect("AddSortWithDefault must produce a protolens");
        match &protolens.target.transform {
            TheoryTransform::AddSortWithDefault { default_expr, .. } => {
                assert_eq!(
                    default_expr, &expr,
                    "default_expr must be forwarded verbatim, not replaced with Value::Null"
                );
            }
            other => panic!("expected AddSortWithDefault in target transform, got {other:?}"),
        }
    }

    #[test]
    fn auto_generate_with_hints_rejects_nan_quality_threshold() {
        // Supplying `NaN` used to be silently absorbed by
        // `partial_cmp(...).unwrap_or(Equal)`; the fix rejects it at the
        // entry point so callers can't accidentally disable the overlap
        // fallback.
        let protocol = test_protocol();
        let src = schema_v1(&protocol);
        let tgt = schema_v2(&protocol);
        let result = auto_generate_with_hints(
            &src,
            &tgt,
            &protocol,
            &AutoLensConfig::default(),
            &HashMap::new(),
            &DomainConstraints::default(),
            Some(f64::NAN),
        );
        // `AutoLensResult` doesn't derive `Debug`, so `.expect_err` would
        // fail to compile; `let…else` gives the same assertion without
        // needing Debug on the Ok variant.
        let Err(err) = result else {
            panic!("NaN quality_threshold must be rejected, but got Ok");
        };
        assert!(
            format!("{err}").contains("NaN"),
            "error must mention NaN; got: {err}"
        );
    }

    #[test]
    fn endofunctor_to_protolens_roundtrips_coerce_sort() {
        // `CoerceSort` carries forward + inverse expressions and a class
        // tag; the elementary wrapper must preserve all three verbatim
        // so that downstream instantiation can run the bridging lens in
        // both directions.
        use panproto_expr::{Expr, Literal};
        use panproto_gat::{CoercionClass, TheoryEndofunctor, TheoryTransform, ValueKind};
        use std::sync::Arc;

        let fwd = Expr::Lit(Literal::Int(1));
        let inv = Expr::Lit(Literal::Int(-1));
        let endo = TheoryEndofunctor {
            name: Arc::from("coerce_counter_to_float"),
            precondition: panproto_gat::TheoryConstraint::HasSort(Arc::from("counter")),
            transform: TheoryTransform::CoerceSort {
                sort_name: Arc::from("counter"),
                target_kind: ValueKind::Float,
                coercion_expr: fwd.clone(),
                inverse_expr: Some(inv.clone()),
                coercion_class: CoercionClass::Retraction,
            },
        };
        let protolens =
            endofunctor_to_protolens(&endo).expect("CoerceSort must produce a protolens");
        match &protolens.target.transform {
            TheoryTransform::CoerceSort {
                sort_name,
                target_kind,
                coercion_expr,
                inverse_expr,
                coercion_class,
            } => {
                assert_eq!(&**sort_name, "counter");
                assert_eq!(*target_kind, ValueKind::Float);
                assert_eq!(coercion_expr, &fwd);
                assert_eq!(inverse_expr, &Some(inv));
                assert_eq!(*coercion_class, CoercionClass::Retraction);
            }
            other => panic!("expected CoerceSort target transform, got {other:?}"),
        }
    }

    #[test]
    fn alignment_to_theory_morphism_is_deterministic_across_hash_iterations() {
        // Build two `FoundMorphism` values whose `vertex_map` and
        // `edge_map` carry the same entries inserted in opposite orders.
        // The resulting `TheoryMorphism.sort_map` / `op_map` must be
        // identical; otherwise downstream factorization can produce
        // different chains on different runs.
        use panproto_mig::FoundMorphism;
        use panproto_schema::Edge;

        let protocol = test_protocol();
        let src = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.a", "string", None::<&str>)
            .unwrap()
            .vertex("r.b", "string", None::<&str>)
            .unwrap()
            .edge("r", "r.a", "prop", Some("a"))
            .unwrap()
            .edge("r", "r.b", "prop", Some("b"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = src.clone();

        let mk = |pairs: &[(&str, &str)]| -> FoundMorphism {
            let mut vm = HashMap::new();
            for (a, b) in pairs {
                vm.insert(Name::from(*a), Name::from(*b));
            }
            let mut em = HashMap::new();
            let e1 = Edge {
                src: Name::from("r"),
                tgt: Name::from("r.a"),
                kind: Name::from("prop"),
                name: Some(Name::from("a")),
            };
            let e2 = Edge {
                src: Name::from("r"),
                tgt: Name::from("r.b"),
                kind: Name::from("prop"),
                name: Some(Name::from("b")),
            };
            em.insert(e1.clone(), e1);
            em.insert(e2.clone(), e2);
            FoundMorphism {
                vertex_map: vm,
                edge_map: em,
                quality: 1.0,
            }
        };
        let fm_a = mk(&[("r", "r"), ("r.a", "r.a"), ("r.b", "r.b")]);
        let fm_b = mk(&[("r.b", "r.b"), ("r.a", "r.a"), ("r", "r")]);

        let ma = alignment_to_theory_morphism_mode(&fm_a, &src, &tgt, false);
        let mb = alignment_to_theory_morphism_mode(&fm_b, &src, &tgt, false);

        // sort_map/op_map are HashMaps, so compare as sorted Vec<(k,v)>.
        let to_sorted = |m: &HashMap<Arc<str>, Arc<str>>| -> Vec<(String, String)> {
            let mut out: Vec<_> = m
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            out.sort();
            out
        };
        assert_eq!(
            to_sorted(&ma.sort_map),
            to_sorted(&mb.sort_map),
            "sort_map must not depend on vertex_map insertion order"
        );
        assert_eq!(
            to_sorted(&ma.op_map),
            to_sorted(&mb.op_map),
            "op_map must not depend on edge_map insertion order"
        );
    }

    #[test]
    fn lenient_span_search_drops_orphan_source_sorts() {
        // Schemas that share `record` sort but differ on `boolean`:
        // the source has an `r.flag` child of kind boolean with no
        // counterpart in the target. Strict cannot find a morphism.
        // Lenient auto-excludes the orphan source vertex and emits a
        // DropSort for the `boolean` sort in the factorization.
        let protocol = test_protocol();
        let src = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.flag", "boolean", None::<&str>)
            .unwrap()
            .edge("r", "r.flag", "prop", Some("flag"))
            .unwrap()
            .build()
            .unwrap();
        // Target lacks any boolean vertex.
        let tgt = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .build()
            .unwrap();

        // Strict: should produce no useful morphism OR a degenerate one
        // — but crucially cannot emit a drop_sort for the orphan.
        let strict = AutoLensConfig {
            stringency: Stringency::Strict,
            ..Default::default()
        };
        let strict_res = auto_generate(&src, &tgt, &protocol, &strict);

        // Lenient: should succeed AND the chain should contain a
        // drop_sort step for `boolean` (the orphan sort in the source).
        let lenient = AutoLensConfig {
            stringency: Stringency::Lenient,
            ..Default::default()
        };
        let lenient_res = auto_generate(&src, &tgt, &protocol, &lenient)
            .unwrap_or_else(|e| panic!("Lenient should find a span: {e}"));

        let has_boolean_drop = lenient_res.chain.steps.iter().any(|step| {
            matches!(
                &step.target.transform,
                TheoryTransform::DropSort(name) if &**name == "boolean"
            )
        });
        assert!(
            has_boolean_drop,
            "Lenient span should emit DropSort(boolean); chain: {:?}",
            lenient_res
                .chain
                .steps
                .iter()
                .map(|s| s.name.to_string())
                .collect::<Vec<_>>()
        );

        // Strict should either error or return a chain without the drop.
        if let Ok(r) = strict_res {
            assert!(
                r.chain.steps.iter().all(|step| !matches!(
                    &step.target.transform,
                    TheoryTransform::DropSort(name) if &**name == "boolean"
                )),
                "Strict must not emit a drop step (drops require span search)"
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

    #[test]
    fn endofunctor_to_protolens_coerce_sort_tags_target_kind_in_name() {
        // Two CoerceSort endofunctors over the same source sort but
        // different target kinds must produce distinct protolens
        // identities. Without tagging the target kind into the
        // protolens name, downstream consumers keying on `name` would
        // conflate them.
        use panproto_expr::{BuiltinOp, Expr};
        let v: Arc<str> = Arc::from("v");
        let to_str = TheoryEndofunctor {
            name: Arc::from("coerce_n_str"),
            precondition: panproto_gat::TheoryConstraint::HasSort(Arc::from("n")),
            transform: TheoryTransform::CoerceSort {
                sort_name: Arc::from("n"),
                target_kind: panproto_gat::ValueKind::Str,
                coercion_expr: Expr::Builtin(BuiltinOp::IntToStr, vec![Expr::Var(Arc::clone(&v))]),
                inverse_expr: None,
                coercion_class: panproto_gat::CoercionClass::Retraction,
            },
        };
        let to_float = TheoryEndofunctor {
            name: Arc::from("coerce_n_float"),
            precondition: panproto_gat::TheoryConstraint::HasSort(Arc::from("n")),
            transform: TheoryTransform::CoerceSort {
                sort_name: Arc::from("n"),
                target_kind: panproto_gat::ValueKind::Float,
                coercion_expr: Expr::Builtin(BuiltinOp::IntToFloat, vec![Expr::Var(v)]),
                inverse_expr: None,
                coercion_class: panproto_gat::CoercionClass::Retraction,
            },
        };
        let p1 = endofunctor_to_protolens(&to_str).unwrap();
        let p2 = endofunctor_to_protolens(&to_float).unwrap();
        assert_ne!(
            p1.name, p2.name,
            "CoerceSort protolens names must distinguish target kinds"
        );
        assert!(
            p1.name.as_str().contains("str"),
            "expected target kind in name; got {}",
            p1.name
        );
        assert!(
            p2.name.as_str().contains("float"),
            "expected target kind in name; got {}",
            p2.name
        );
    }

    #[test]
    fn uses_coerce_is_exploratory_only() {
        assert!(!Stringency::Strict.uses_coerce());
        assert!(!Stringency::Balanced.uses_coerce());
        assert!(!Stringency::Lenient.uses_coerce());
        assert!(Stringency::Exploratory.uses_coerce());
    }

    #[test]
    fn sources_without_compatible_targets_is_sorted() {
        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![
                "record".into(),
                "alpha".into(),
                "beta".into(),
                "gamma".into(),
            ],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let src = SchemaBuilder::new(&protocol)
            .vertex("zeta", "alpha", None::<&str>)
            .unwrap()
            .vertex("aardvark", "beta", None::<&str>)
            .unwrap()
            .vertex("mango", "gamma", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let out = sources_without_compatible_targets(&src, &tgt);
        let names: Vec<&str> = out.iter().map(panproto_gat::Name::as_str).collect();
        assert_eq!(
            names,
            vec!["aardvark", "mango", "zeta"],
            "HashMap iteration order leaked into output"
        );
    }

    #[test]
    fn lenient_partial_kind_coverage_keeps_sort() {
        let protocol = test_protocol();
        let src = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.keep", "string", None::<&str>)
            .unwrap()
            .vertex("r.extra", "string", None::<&str>)
            .unwrap()
            .edge("r", "r.keep", "prop", Some("keep"))
            .unwrap()
            .edge("r", "r.extra", "prop", Some("extra"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.keep", "string", None::<&str>)
            .unwrap()
            .edge("r", "r.keep", "prop", Some("keep"))
            .unwrap()
            .build()
            .unwrap();
        let cfg = AutoLensConfig {
            stringency: Stringency::Lenient,
            ..Default::default()
        };
        let result = auto_generate(&src, &tgt, &protocol, &cfg).unwrap();
        let dropped_string = result.chain.steps.iter().any(|step| {
            matches!(
                &step.target.transform,
                TheoryTransform::DropSort(name) if &**name == "string"
            )
        });
        assert!(
            !dropped_string,
            "Lenient must not drop the `string` sort when at least one \
             target vertex has that kind; chain: {:?}",
            result
                .chain
                .steps
                .iter()
                .map(|s| s.name.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn auto_generate_candidates_ordering_is_stable_on_ties() {
        let protocol = test_protocol();
        let src = schema_post_with_created(&protocol);
        let tgt = schema_message_with_sent(&protocol);
        let config = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        let a = auto_generate_candidates(&src, &tgt, &protocol, &config, 5)
            .expect("candidates should exist");
        let b = auto_generate_candidates(&src, &tgt, &protocol, &config, 5)
            .expect("candidates should exist");
        let key = |cands: &[crate::candidate::LensCandidate]| -> Vec<String> {
            cands
                .iter()
                .map(|c| format!("{:.6}:{}", c.score(), chain_step_names(&c.chain)))
                .collect()
        };
        assert_eq!(key(&a), key(&b), "candidate ordering is not deterministic");
    }

    #[test]
    fn exploratory_surfaces_coerce_proposals_on_result() {
        // Two schemas with a kind mismatch (integer vs string) that
        // the default witness library bridges via `int_to_str`.
        // Exploratory should surface the proposal on AutoLensResult.
        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into(), "integer".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let src = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.n", "integer", None::<&str>)
            .unwrap()
            .edge("r", "r.n", "prop", Some("n"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.n", "string", None::<&str>)
            .unwrap()
            .edge("r", "r.n", "prop", Some("n"))
            .unwrap()
            .build()
            .unwrap();

        // Balanced: never consults coerce; the proposals vec is empty.
        let balanced = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        // Use auto_generate_with_hints so a morphism is always found
        // (the CSP without hints would reject mismatched kinds).
        let hints = std::collections::HashMap::new();
        let dc = panproto_mig::hom_search::DomainConstraints::default();
        if let Ok(res) =
            auto_generate_with_hints(&src, &tgt, &protocol, &balanced, &hints, &dc, None)
        {
            assert!(
                res.coerce_proposals.is_empty(),
                "Balanced must not populate coerce_proposals"
            );
        }

        // Exploratory: regardless of CSP outcome, the proposals vec
        // should contain the int_to_str bridge.
        let exploratory = AutoLensConfig {
            stringency: Stringency::Exploratory,
            ..Default::default()
        };
        if let Ok(res) =
            auto_generate_with_hints(&src, &tgt, &protocol, &exploratory, &hints, &dc, None)
        {
            assert!(
                res.coerce_proposals
                    .iter()
                    .any(|p| p.witness_name == "int_to_str"),
                "Exploratory should expose int_to_str in coerce_proposals; got {:?}",
                res.coerce_proposals
                    .iter()
                    .map(|p| p.witness_name.as_str())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn stringency_uses_coerce_only_at_exploratory() {
        assert!(!Stringency::Strict.uses_coerce());
        assert!(!Stringency::Balanced.uses_coerce());
        assert!(!Stringency::Lenient.uses_coerce());
        assert!(Stringency::Exploratory.uses_coerce());
    }

    #[test]
    fn stringency_display_matches_serde_tokens() {
        // Display/as_str must agree with the serde token used by
        // CLI parsers so user-facing rendering doesn't drift from
        // the on-wire format.
        assert_eq!(Stringency::Strict.to_string(), "strict");
        assert_eq!(Stringency::Balanced.to_string(), "balanced");
        assert_eq!(Stringency::Lenient.to_string(), "lenient");
        assert_eq!(Stringency::Exploratory.to_string(), "exploratory");
        for tier in [
            Stringency::Strict,
            Stringency::Balanced,
            Stringency::Lenient,
            Stringency::Exploratory,
        ] {
            let wire = serde_json::to_string(&tier).expect("serde");
            assert_eq!(
                wire.trim_matches('"'),
                tier.as_str(),
                "Display must match serde output"
            );
        }
    }

    #[test]
    fn auto_generate_is_deterministic_across_runs() {
        let protocol = test_protocol();
        let src = schema_post_with_created(&protocol);
        let tgt = schema_message_with_sent(&protocol);
        let config = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        let a = auto_generate(&src, &tgt, &protocol, &config).unwrap();
        let b = auto_generate(&src, &tgt, &protocol, &config).unwrap();
        let step_names = |r: &AutoLensResult| -> Vec<String> {
            r.chain.steps.iter().map(|s| s.name.to_string()).collect()
        };
        assert_eq!(step_names(&a), step_names(&b), "step order drift");
        assert!(
            (a.alignment_quality - b.alignment_quality).abs() < 1e-12,
            "quality drift"
        );
    }

    #[test]
    fn auto_generate_candidates_top_n_zero_returns_one() {
        // `top_n == 0` is documented to degenerate to `top_n == 1`;
        // pin the behaviour so a future refactor can't silently
        // treat 0 as "unlimited".
        let protocol = test_protocol();
        let src = schema_post_with_created(&protocol);
        let tgt = schema_message_with_sent(&protocol);
        let config = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        let c = auto_generate_candidates(&src, &tgt, &protocol, &config, 0)
            .unwrap_or_else(|e| panic!("candidates: {e}"));
        assert!(
            !c.is_empty() && c.len() == 1,
            "top_n=0 must yield exactly one candidate, got {}",
            c.len()
        );
    }

    #[test]
    fn alignment_to_theory_morphism_emit_spans_full_coverage_no_drops() {
        // When every source sort has a vertex in the vertex_map, the
        // emit_spans=true mode should still produce a sort_map that
        // covers every source sort (via the edge-pair scan), and
        // factorize should emit no DropSort steps. This pins the
        // "span degenerates to total morphism when nothing is lost"
        // guarantee.
        use panproto_mig::FoundMorphism;
        let protocol = test_protocol();
        let s = schema_v1(&protocol);
        let alignment = FoundMorphism {
            vertex_map: s.vertices.keys().map(|k| (k.clone(), k.clone())).collect(),
            edge_map: s.edges.keys().map(|e| (e.clone(), e.clone())).collect(),
            quality: 1.0,
        };
        let chain = protolens_from_alignment_mode(&alignment, &s, &s, true)
            .expect("span-mode chain on identity alignment");
        assert!(
            chain.steps.iter().all(|step| !matches!(
                &step.target.transform,
                TheoryTransform::DropSort(_) | TheoryTransform::DropOp(_)
            )),
            "emit_spans=true with complete vertex_map must not drop any sort/op; got {:?}",
            chain
                .steps
                .iter()
                .map(|s| s.name.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn stringency_serde_round_trip() {
        // Serialize every variant, deserialize, and assert equality —
        // pins the Display / serde rename_all contract in both
        // directions.
        for tier in [
            Stringency::Strict,
            Stringency::Balanced,
            Stringency::Lenient,
            Stringency::Exploratory,
        ] {
            let wire = serde_json::to_string(&tier).expect("serialize");
            let back: Stringency = serde_json::from_str(&wire).expect("deserialize");
            assert_eq!(back, tier, "round-trip drift for {tier:?}");
            // Display must produce the same token (minus quotes).
            assert_eq!(wire.trim_matches('"'), tier.to_string());
        }
    }

    #[test]
    fn auto_generate_surfaces_coerce_proposals_at_exploratory() {
        // Pin the parity: `auto_generate` (non-hinted) must populate
        // `coerce_proposals` when Exploratory is active, matching
        // `auto_generate_with_hints`. A divergence here would let
        // callers accidentally drop the proposal data by picking the
        // non-hinted entry point.
        let protocol = Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into(), "integer".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let src = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.n", "integer", None::<&str>)
            .unwrap()
            .edge("r", "r.n", "prop", Some("n"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None::<&str>)
            .unwrap()
            .vertex("r.n", "string", None::<&str>)
            .unwrap()
            .edge("r", "r.n", "prop", Some("n"))
            .unwrap()
            .build()
            .unwrap();
        let cfg = AutoLensConfig {
            stringency: Stringency::Exploratory,
            ..Default::default()
        };
        if let Ok(res) = auto_generate(&src, &tgt, &protocol, &cfg) {
            assert!(
                res.coerce_proposals
                    .iter()
                    .any(|p| p.witness_name == "int_to_str"),
                "auto_generate at Exploratory must expose int_to_str in coerce_proposals"
            );
        }
        // Balanced: proposals must stay empty on the same schemas.
        let balanced = AutoLensConfig {
            stringency: Stringency::Balanced,
            ..Default::default()
        };
        if let Ok(res) = auto_generate(&src, &tgt, &protocol, &balanced) {
            assert!(
                res.coerce_proposals.is_empty(),
                "Balanced must not populate coerce_proposals via auto_generate"
            );
        }
    }

    #[test]
    fn score_weights_are_pinned() {
        // Provisional weights (quality + 0.5*coverage + 0.2*avg_conf).
        // Any change to the weighting must update this pin explicitly.
        use crate::candidate::{CandidateStep, LensCandidate};
        let protocol = test_protocol();
        let s = schema_v1(&protocol);
        let chain = crate::protolens::ProtolensChain::new(vec![]);
        let lens = chain.instantiate(&s, &protocol).unwrap();
        let cand = LensCandidate {
            chain,
            lens,
            quality: 1.0,
            coverage: 1.0,
            seed_anchors: vec![],
            steps: vec![CandidateStep {
                kind: "k".into(),
                explanation: "e".into(),
                confidence: 1.0,
                strategy: None,
            }],
            strategies_used: vec![],
        };
        assert!(
            (cand.score() - 1.7).abs() < 1e-9,
            "weight drift: expected 1.7, got {}",
            cand.score()
        );
    }
}
