# Alignment evidence

Automatic alignment begins with candidate pairs of source and target vertices. panproto calls each candidate an **anchor**. An anchor records the pair, a confidence, a strategy tag, the provenance of the comparison, and an explanation. The search has not accepted the pair merely because an anchor names it.

The implementation has two routes from anchors to a search. The [`EvidenceTable`](https://docs.rs/panproto-mig/latest/panproto_mig/align/evidence/struct.EvidenceTable.html) API can score every candidate pair and pass those scores to a span search. The automatic lens generator currently takes a different route: it selects a one-to-one seed map, places those pairs in `SearchOptions::hard_pins`, and then compares that pinned search with a second search in which the strategy pins have been released. This distinction determines which guarantees apply.

## Active proposal strategies

The auto-lens pipeline calls twelve strategy emitters. Their schedule is fixed by `Stringency`.

| Tiers | Strategy | Input used |
|---|---|---|
| Every tier | `Exact` | Equal vertex identifiers with compatible kinds |
| Every tier | `ExactSuffix` | Equal terminal dot-segments with compatible kinds and constraints |
| Every tier | `EdgeLabel` | Child vertices reached by edges with the same label and edge kind |
| Balanced and above | `Alias` | The alias dictionary, applied to leaf identifiers or outgoing edge labels |
| Balanced and above | `TokenSimilarity` | Tokens and character bigrams from vertex identifiers |
| Balanced and above | `DescriptionSimilarity` | Text stored in a vertex's `description` constraint |
| Lenient and above | `WrapUnwrap` | Corresponding field-label groups in flat and nested records |
| Lenient and above | `TypeSignature` | Multisets of outgoing edge kinds and target-vertex kinds |
| Lenient and above | `WlRefinement` | Singleton color classes after Weisfeiler-Leman refinement |
| Lenient and above | `Neighborhood` | Child pairs propagated from a selected parent-pair map |
| Exploratory | `Structural` | Degree and incident edge-kind profiles |
| Exploratory | `Coerce` | A registered coercion witness between different vertex kinds |

Three details qualify this table. First, `ExactSuffix` and `EdgeLabel` run even at `Strict`; strict mode thus runs more than exact identifier equality. Second, `Coerce` emits proposals and witness metadata, but the morphism-search domains still exclude kind-mismatched targets. A coerce proposal cannot steer the current search, though callers can inspect it in `AutoLensResult::coerce_proposals`. Third, neighborhood propagation is a second pass. Auto-lens aggregates and selects the other strategies to obtain parent seeds, emits neighborhood anchors from those seeds, and then aggregates the full pool again.

The [`StrategyTag`](https://docs.rs/panproto-mig/latest/panproto_mig/align/enum.StrategyTag.html) enum has fourteen variants because it also reserves `UserHint` and `Llm`. Neither variant names an auto-lens strategy emitter. The hint-taking auto-lens APIs put caller mappings directly into `hard_pins`; after the search, they construct `UserHint` anchors for the returned explanations and candidate metadata. `Llm` is an extension point. No production function emits that tag, and no auto-lens configuration field accepts language-model proposals. A caller can still construct either kind of anchor and use the public evidence API directly.

## From anchors to scores

The reducer in [`panproto_mig::align::evidence`](https://docs.rs/panproto-mig/latest/panproto_mig/align/evidence/) produces one score for each pair mentioned by at least one anchor. It discards anchors with a `NaN` confidence. Every other raw confidence is clamped to the unit interval and capped by its [`Provenance`](https://docs.rs/panproto-mig/latest/panproto_mig/align/evidence/enum.Provenance.html).

Under the default `StrictPriority` aggregation policy, the fourteen tags occupy priority bands of width $1/14$. Let $r$ be the tag's rank, with zero denoting `UserHint` and thirteen denoting `Llm`, and let $c$ be the clamped, provenance-capped confidence. The effective value is

$$
e = \frac{13-r+c}{14}.
$$

The implementation performs this as one division. Adjacent bands share an endpoint, so a tag's weakest value can equal the strongest value in the band immediately below it. The ordering is thus non-strict at those boundaries. The alternative `ConfidenceFirst` policy omits the bands and uses $c$ directly.

The reducer next groups anchors by the input from which they were computed. For each family it retains the largest effective value.

| Family | Tags in the family |
|---|---|
| User hint | `UserHint` |
| Identifier | `Exact`, `ExactSuffix`, `Alias`, `TokenSimilarity` |
| Edge label | `EdgeLabel`, `WrapUnwrap` |
| Documentation | `DescriptionSimilarity` |
| Structure | `TypeSignature`, `Neighborhood`, `WlRefinement`, `Structural`, `Llm` |
| Coercion | `Coerce` |

`Alias` is the one branch-sensitive case. A leaf alias compares identifiers and belongs to the identifier family. A composite alias compares outgoing edge labels and belongs to the edge-label family. The anchor's provenance distinguishes these branches.

If $f_j$ is the maximum for family $j$, with zero for a family that emitted nothing, the ordinary family mean is

$$
m = \frac{1}{6}\sum_{j=1}^{6} f_j.
$$

The fixed divisor makes this mean monotone under literal pool inclusion: adding an anchor can increase a family maximum or leave it unchanged. A divisor that counted only the families that fired could fall when a weak new family appeared.

User hints receive one additional rule. If $h$ is the largest capped confidence among `UserHint` anchors for the pair, the reported score is

$$
s = \max(m, h).
$$

Thus the often useful $k/6$ bound applies only to a pair without the hint override. Evidence from $k$ ordinary families cannot exceed $k/6$, while one full-confidence `UserHint` anchor yields a score of $1.0$. The unit tests assert both cases.

Before neighborhood propagation, auto-lens also adjusts the confidences already in the pool by required-set agreement. It adds $0.05$ when both vertices are required, subtracts $0.05$ when only one is required, and clamps the result. `UserHint` anchors are exempt. Neighborhood anchors are appended after this adjustment and thus do not receive it.

## Selection

Aggregation and selection are separate public operations. [`EvidenceTable::select`](https://docs.rs/panproto-mig/latest/panproto_mig/align/evidence/struct.EvidenceTable.html#method.select) accepts a configurable [`RowFilter`](https://docs.rs/panproto-mig/latest/panproto_mig/align/evidence/struct.RowFilter.html) and `Cardinality` rule. A row filter first applies an absolute threshold, then retains candidates within a relative delta of the best score for the same source. Cardinality may be strict, permissive, or hybrid. The final greedy pass is deterministic because it sorts by score and then by the two identifiers.

Auto-lens does not expose those choices through `AutoLensConfig`. Both of its seed-selection passes use `Cardinality::Strict` with `RowFilter::relative_only()`. The resulting seed map has at most one pair at either endpoint, the absolute threshold is zero, and the relative delta is the library default of $0.02$. The default absolute floor and the hybrid cardinality constants do not govern auto-lens seed selection.

Selection is also absent from the evidence-aware network builder. A caller that passes an evidence table to a span search gives the builder all pair scores; the solver chooses a complete assignment under the structural constraints and objective.

## The evidence-aware span API

[`SpanSearch::with_evidence`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html#method.with_evidence) attaches an evidence table to the cost-function network. For source vertex $v$, target vertex $a$, and source vertex set $V_S$, the builder adds the unary term

$$
C_{\mathrm{anchor}}(v,a)
  = w_{\mathrm{anchor}}\frac{1-s(v,a)}{|V_S|}.
$$

This term does not remove a target from a domain, add a hard constraint, or change the variable set. A score outside $[0,1]$ is rejected when the network is built. For a fixed non-negative weight, increasing a pair's score can only lower the cost of assignments that use that pair.

The shipped anchor weight is $0.0$. With the default [`CostWeights`](https://docs.rs/panproto-mig/latest/panproto_mig/struct.CostWeights.html), every evidence table thus contributes the same zero-weight term and cannot change the selected span. Callers can supply a non-zero weight through `SpanSearch::with_weights`; the tier and monotonicity tests do so to exercise the evidence term.

This path is public, but it is not the route used by current production code in the repository. The ordinary `find_span` helpers construct `SpanSearch` with `NoEvidence`, and the auto-lens source does not call `with_evidence`. Current production behavior should thus be described through provisional pins, not through the reward-only term.

## The auto-lens route

The single-result auto-lens pipeline resolves the strategy pool to a strict seed map and merges those seeds into `SearchOptions::hard_pins` without replacing caller pins. A hard pin collapses one source vertex's domain to the named target, plus the option to drop that vertex. The first search thus gives selected strategy proposals the force of domain restrictions.

Auto-lens then runs a released search whenever the strategies added at least one pin. This second search retains the caller's original pins and removes the strategy pins. The single-result APIs compare the two answers by the search objective: higher alignment quality wins, followed by more mapped source vertices. The pinned answer remains when both measures tie. Since releasing strategy pins adds domain values, the released search ranges over a superset of the pinned search when all other options are fixed.

The multi-candidate APIs use a different comparison. They choose between the pinned and released candidate lists by their best coverage, and they return a fully covering pinned list without running the released comparison. Claims about objective-based comparison should thus be limited to `auto_generate` and `auto_generate_with_hints`.

Caller hints remain hard on both attempts. The public hint parameter and `SearchOptions::hard_pins` express fixed correspondences in current auto-lens behavior. A soft user hint exists at the evidence-table level, but production auto-lens does not route hints there.

## Monotonicity and stringency

Evidence aggregation is monotone under pool inclusion. If pool $P'$ contains every anchor in $P$, then every score produced from $P'$ is at least the corresponding score from $P$. This property follows from the per-family maxima, the fixed divisor, and the maximum with the hint confidence.

An evidence-aware network has the same feasible assignments for every evidence table. Under a non-zero anchor weight, pointwise domination of one evidence table by another gives a non-increasing cost for every assignment and hence for the optimum. Under the shipped zero weight, all such costs are equal.

Stringency tiers do not guarantee pool inclusion. `WlRefinement` uses two iterations at Lenient and three at Exploratory; another refinement round can split a color class and withdraw an earlier anchor. `Neighborhood` depends on a selected seed map, so a larger first-pass pool can change the seeds and withdraw propagated anchors. The integration tests include a concrete Lenient-to-Exploratory case in which a neighborhood anchor disappears, the evidence score falls, and an anchor-weighted optimum becomes worse. General tier monotonicity is thus false. The tests assert monotonicity only when the higher tier's evidence table dominates pointwise, and they separately check that any shortfall comes from `WlRefinement` or `Neighborhood`.

Production auto-lens adds further tier-dependent behavior: Strict and Balanced ask for total morphisms, Lenient and Exploratory permit spans, and the latter tiers enable overlap retries by default. The pool-inclusion theorem for `aggregate` should not be presented as a theorem about the full auto-lens tier ladder.

## Defaults and evidence for the design

panproto has no labeled corpus of intended schema correspondences. The priority order, family partition, provenance ceilings, strategy thresholds, scoring coefficients, and objective weights have not been fitted to panproto data.

The `align::defaults` module centralizes the provenance ceilings, the general selection defaults, and the shipped anchor weight. It does not contain every numeric choice in the alignment pipeline. Tier thresholds live in `auto_lens.rs`, while the required-set adjustment lives in `align/mod.rs`. Strategy-specific mixture weights, confidence floors, and fixed confidences live with their emitters. An audit of all alignment numbers thus spans several files.

Prior work supplies design precedents rather than a validation of these settings. COMA evaluated composite similarity aggregation and matrix-selection rules [@dorahm2002coma]. The analysis of mapping extraction by @meilickestuckenschmidt2007analyzing supports treating selection as a separate stage. AgreementMakerLight supplies precedents for provenance weights and cardinality-aware selection, though panproto's constants are not corpus fits [@fariapesquitasantospalmonaricruzcouto2013agreementmakerlight]. The reducer also avoids Dempster-Shafer combination because several strategies are deterministic readings of the same input rather than independent sources, the condition required by that combination rule [@dempster1967upper]. None of these results establishes that panproto's current family partition or parameter values are optimal.

The evidence-aware span API provides the soft scoring route. Current auto-lens behavior still passes through selected provisional pins, which is why [Searching for a morphism](./morphism-search.md) treats evidence and domain restrictions separately.

## See also

- [Searching for a morphism](./morphism-search.md) for the objective and span-search constraints.
- [Find a span between two schemas](../how-to/spans.md) for the public span-search interface.
- [Migrations as morphisms](./migrations-as-morphisms.md) for the morphisms produced by the search.
- [What panproto verifies](./what-is-verified.md) for the mechanically checked properties.
- [CLI reference](../reference/cli.md) for the user-facing stringency and hint options.
