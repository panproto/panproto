# Find a span between two schemas

A *span* is a pair of schema morphisms out of a shared middle: `src ← A → tgt`. The middle, called the **apex**, is the part of the source that the search found a home for in the target. Use a span when the two schemas overlap without one embedding into the other, which is the ordinary case rather than the exception on the corpus panproto is measured against: 5117 of its 5852 ordered lexicon pairs admit no total morphism at all. [Migrations as morphisms](../explanation/migrations-as-morphisms.md#spans-and-why-partiality-is-the-ordinary-case) breaks that figure down and names the test that produces it.

## Prerequisites

The Rust SDK ([`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/)) or the `schema` CLI. A protocol, because the apex is itself a schema and a schema is well formed only against a protocol.

## The task

### From the CLI

```sh
schema auto-migrate schemas/v1.json schemas/v2.json
```

The output reports the apex size, what fraction of the source it covers, the quality, and the interval the search proved the quality lies in. Running it on the pair the Rust listing below builds, a `post` record that renamed `text` to `body` and dropped a `likes` counter:

```text
Found span (quality: 0.230, bounds: [0.230, 0.230]):

Apex: 2 of 3 vertices (66.7% coverage), 1 edge

Vertex map:
  post -> post
  post.text -> post.body

Edge map:
  post->post.text (prop) text -> post->post.body (prop) body
```

The bounds collapse to a point exactly when the search proved optimality, and the edge map appears only when the apex has an arc to carry.

Two flags and their absence select how much of an answer counts as one. They form a **strictness ladder** over a single search rather than selecting between searches:

| Flag | Accepts |
|---|---|
| `--total` | Only a span whose left leg is onto, which is to say a total morphism. Totality needs the edges as well as the vertices, so this is a stricter test than full vertex coverage. Exits non-zero otherwise. |
| (none) | Any span covering at least one vertex. |
| `--span` | Any span, including an empty apex, which reports that the two schemas share nothing. |

The two flags name opposite ends of the ladder, so passing both is rejected before either search runs. `--monic` sits outside the ladder and constrains the search rather than the acceptance: it requires the vertex map to be injective, and composes with either end. `--json` writes the span's right leg, a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) out of the apex.

### When to ask for a total morphism

Use `--total` when a partial answer is not worth having: a release gate that will migrate every record or none, a compiled lens whose source schema must be covered, a CI check that should fail loudly when a field has nowhere to go. Leave it off when the question is descriptive, and in particular when you are ranking candidate targets or reporting how far apart two versions have drifted.

The flag does not filter the default answer. The span objective is lexicographic in `(quality, drops)` and the reported quality excludes the drop count, so a span that drops a vertex can score strictly better than a total morphism that keeps it. An optimal span that turns out to be partial is thus no evidence about whether a total morphism exists. This is why `--total` runs a second search, on **the bail path**, that is, the branch taken only when the optimal span comes back partial: a pair whose optimal span is already total pays nothing extra, and a pair whose optimal span is partial gets the question it actually asked. In Rust the same distinction is the choice between [`find_span`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_span.html) and [`find_best_morphism`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_best_morphism.html).

An answer recovered on the bail path prints without an apex, a coverage figure or a certificate. A total morphism carries none of those, so what you get is a report with fewer headings rather than one with empty ones.

### From Rust

[`find_span`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_span.html) is total: it never refuses for want of a match, because leaving every source vertex out of the apex is always a feasible answer. A pair with nothing in common gets an empty apex rather than an error.

```rust
use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_schema::{Protocol, SchemaBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = Protocol {
        name: "demo".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
        ..Protocol::default()
    };

    let old = SchemaBuilder::new(&protocol)
        .vertex("post", "object", None::<&str>)?
        .vertex("post.text", "string", None::<&str>)?
        .vertex("post.likes", "integer", None::<&str>)?
        .edge("post", "post.text", "prop", Some("text"))?
        .edge("post", "post.likes", "prop", Some("likes"))?
        .entry("post")
        .build()?;

    // The new schema renamed one field and dropped the counter, so no
    // total morphism out of `old` exists.
    let new = SchemaBuilder::new(&protocol)
        .vertex("post", "object", None::<&str>)?
        .vertex("post.body", "string", None::<&str>)?
        .edge("post", "post.body", "prop", Some("body"))?
        .entry("post")
        .build()?;

    let span = find_span(&old, &new, &protocol, &SearchOptions::default())?;

    assert!(!span.is_total(), "the counter has nowhere to go");
    assert_eq!(span.apex.vertices.len(), 2);
    assert!((span.apex_coverage - 2.0 / 3.0).abs() < 1e-12);
    assert!(span.certificate.proven_optimal);

    // The left leg is an inclusion, so the apex reuses source identifiers
    // and the right leg carries the renaming.
    assert_eq!(
        span.right.vertex_map.get("post.text").map(|n| n.as_str()),
        Some("post.body"),
    );
    Ok(())
}
```

*Listing 1: Searching for a span between two schemas, one of which dropped a field.*

The left leg is an inclusion: apex vertex identifiers *are* source vertex identifiers. That is what makes it a mono by construction and what makes two spans comparable without a graph-isomorphism test.

Every SDK carries the search. Python spells it `panproto.find_span(src, tgt, protocol)`, TypeScript `p.span(from, to, hints?)`, Haskell `findSpan`, and Swift `findSpan(to:in:options:constraints:)`. What comes back over the wire is narrower than the Rust type, and narrower in two different ways. Swift and Haskell decode the whole `SchemaSpan`: the apex, both legs, the quality and its bounds, the coverage, `proven_optimal` and `is_total`. TypeScript's `SpanResponse` decodes the apex as a vertex list and an edge list, the right leg as `vertex_map`, and the same scalars, with no apex schema and no left leg, on the reasoning that the caller already holds the source and can induce the rest.

None of them carries the leg shape. A host that needs to know whether the right leg embeds should ask for injectivity up front rather than infer it after the fact. Where that is not possible, comparing the number of distinct values in the right leg's vertex map against its number of keys settles the vertex half of the question, and nothing on the wire settles the edge half.

### Pinning what you already know

A correspondence you *know* goes in [`SearchOptions::hard_pins`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/struct.SearchOptions.html), which restricts a source vertex to exactly that target. A correspondence something *inferred* belongs in the evidence table [`SpanSearch::with_evidence`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html) reads, where it changes which assignment is optimal without removing any other from the search. The distinction is load-bearing: two individually plausible pins can be jointly infeasible, and a pin that turns out to be wrong costs a solution rather than a rank.

Evidence never removes a value from a domain, which has a consequence you can rely on when tuning: adding a strategy, or raising the confidence of one already firing, cannot make a pair that previously answered stop answering. The feasible set is the same whichever strategies ran; only which member of it is optimal changes.

### Merging along the apex

A span is the input a pushout wants. `SchemaSpan::to_overlap` produces the pair list `schema_pushout` expects, and `SchemaSpan::pushout` performs the merge, returning `src ⊔_A tgt` together with the two injections. The pushout is the integrated schema; the CLI reaches it through `schema integrate --auto-overlap`.

The merge has a precondition: the right leg must not identify two apex vertices. A merge along the apex has to commute, and a right leg that sends two apex vertices to one target vertex makes that impossible, since the merge identifies elements by a map keyed on the target element and a repeated key names only one preimage. A contracting right leg is an ordinary answer from the default search, so `SchemaSpan::pushout` reports `SpanError::ContractingRightLeg` rather than returning a square that does not commute. Search with `SearchOptions::iso` for a span that embeds; `discover_overlap` does exactly that, which is why the CLI path is safe.

### When the right leg contracts

A source with four string fields and a target with one produces a right leg that sends all four to the same place. Nothing is wrong with that as a span. It costs you every operation downstream that assumes a preimage is unique.

It shows up in three places. First, the CLI prints a warning on stderr, which keeps stdout pipeable under `--json`:

```text
warning: the right leg is not injective on vertices, so this migration
identifies two source vertices. Lifting it has no well-defined answer
without a rule for combining them; pass --monic to search for a leg
that embeds.
```

Second, in Rust, `span.certificate.shape.right_is_mono` is false, and `span.certificate.shape.right_edge_images` reports `EdgeImages::Shared` when two apex arcs also share an image. The two come apart: an injective vertex map may still send two parallel apex arcs to one target arc, so a caller merging along the span reads both. Third, and loudest, `SchemaSpan::pushout` refuses outright, which is likely the symptom you meet first.

There are three responses, in increasing order of effort. First, ask for injectivity up front: `--monic` on the CLI, [`SearchOptions::monic`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/struct.SearchOptions.html) in Rust. The search then optimises over the injective assignments, so what you get back is the best embedding rather than the best map filtered after the fact, and a source vertex with no free target drops out of the apex instead of colliding. Second, ask for `SearchOptions::iso` when the span has to feed a pushout or a symmetric lens, since only that path promises an injective edge map as well. Third, keep the contraction and decide the collision yourself, by excluding the losing source vertices through [`DomainConstraints::excluded_sources`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/struct.DomainConstraints.html) so that they leave the apex, and then recovering their data at the value level with a [field transform](./field-transforms.md) that reads the whole record. That third route is the one to take when the collision is genuine, that is, when two source fields do belong in one target field and you know how they combine.

## Verification

### Coverage and quality

`apex_coverage` is `|apex.vertices| / |src.vertices|`, or one when the source has no vertices. It answers *how much* of the source was covered. `quality` answers *how well the covered part matches*, and it excludes the drop count for exactly that reason. Read them together. A coverage of 0.2 with a quality of 0.95 says the search found a small, confident overlap; a coverage of 0.95 with a quality of 0.2 says it found a large, doubtful one. Those two ask for different responses from you, and neither number alone distinguishes them.

Coverage counts vertices only. [`SchemaSpan::is_total`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) additionally requires that every mappable source edge survive, so a span can report a coverage of 1.0 and still not be a total morphism: every vertex found a home and some arc between two of them did not. When you need the total-morphism answer, test `is_total` rather than comparing the coverage against one.

Quality ranks spans over one source schema and nothing else. Every denominator in the objective is fixed by the source, so two spans out of the same schema are comparable and two spans out of different schemas are not.

### The certificate

Every span carries a [`SpanCertificate`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanCertificate.html) recording what the construction proved rather than what it assumed. Read it alongside `span.quality_bounds`, which sits on the span itself rather than on the certificate:

- `proven_optimal` says whether the search ruled out a better span, or merely ran out of budget. When it is false, `span.quality_bounds` is the interval that separates "0.4, and nothing better exists" from "0.4, and the search stopped before it could tell". The two ends are equal exactly when `proven_optimal` holds.
- `limit_hit` names what stopped it: [`LimitKind::Nodes`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/enum.LimitKind.html) for the node budget, `LimitKind::Time` for a wall-clock budget the caller set. There is no default wall-clock budget, so a `Time` reading means the caller asked for one and accepted a non-deterministic result.
- `path` records which algorithm answered. [`SolverPath::Eliminate { width }`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/enum.SolverPath.html) is exact bucket elimination, which never prunes and always proves optimality; `BranchAndBound { width }` is the fallback taken when the message tables would not fit the budget; `Monic` and `Iso` are the injective and maximum-common-sub-schema paths, neither of which eliminates. The `width` on the first two is the induced width of the order actually used, and it has been 1 or 2 throughout both committed corpus snapshots: across the 861 record-typed lexicon pairs whose answers are recorded, and across all 5852 ordered pairs whose networks are, 5168 at width 1 and 684 at width 2.
- `shape` reports what the two legs are, as a [`LegShape`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.LegShape.html). `left_is_mono` is true by construction, since an inclusion is a mono. `right_is_mono` and `right_edge_images` are the two halves of "does the right leg embed", reported separately because an injective vertex map can still collapse parallel arcs. `left_is_iso` is what `is_total` reads.
- `legs_are_functorial` records that both legs passed the schema-morphism check: every mapped edge lands between the images of its own endpoints.
- `left_existence` and `right_existence` report the existence check on each leg separately, because the two legs have different codomains and can fail different obligations. The left leg is an inclusion and so cannot fail functoriality, but it can fail reachability: an apex whose every vertex sits on a cycle has no root, and the check reports that. It is a finding about the apex rather than a defect in the leg.
- `apex_pointed` says whether the apex has an entry point. [`induce`](https://docs.rs/panproto-schema/latest/panproto_schema/induce/fn.induce.html) never synthesises one, so an apex with no entries is recorded rather than repaired.
- `tie_break_order` is the sequence the tie among equally good assignments was settled in, when exact inference produced the answer. It is the *reverse* of the elimination order, so reading the rule against the elimination order names the wrong sequence. `SpanSearch::optima` enumerates the rest of the tie for a caller wanting a different canonical choice.
- `apex_digest` plus the two leg maps is the span's identity, which is how a caller checks that a span was not assembled from parts.

## Common mistakes

- **Comparing `quality` across source schemas.** There is no absolute reading of the number, and no threshold on it is meaningful across pairs. Read `apex_coverage` alongside it.
- **Reading an empty apex as a verdict.** An empty apex charges the full penalty on each component the source gives mass to, and a component has mass only when the source has something for it to measure. Name and degree are per source vertex and always charge; the edge component is per source edge and the Jaccard component per source vertex with a named outgoing edge, so an edgeless source charges neither. Under the default weights an empty apex therefore reads `0.0` over a source with at least one named edge, `0.30` over a source whose edges are all unnamed, `0.55` over an edgeless source, and `1.0` over an empty source. All four say "these two share nothing" on four different scales, which is what makes each a floor rather than a verdict.
- **Reading a partial optimal span as proof that no total morphism exists.** It is not. The two searches answer different questions, and `--total` runs the second one for you; in Rust, call `find_best_morphism` rather than inferring from `is_total`.
- **Treating a coverage of 1.0 as totality.** Totality needs the edges too. Test `is_total`.
- **Confusing the apex with a symmetric lens's apex.** The two are different objects related by pushout and then `Mod`. `SchemaSpan::pushout` gates on `right_is_mono` alone, which `--monic` already delivers; what the iso path adds on top is an injective *edge* map, and that is why `auto_symmetric` runs iso rather than monic.
- **Routing inferred correspondences through `hard_pins`.** See [Pinning what you already know](#pinning-what-you-already-know): a pin collapses a domain, and a collapsed domain cannot be recovered by the objective.
- **Expecting `find_morphisms` to hand back the whole hom-set.** It returns the morphisms attaining the optimum, capped by `max_results`. Reading `results[0]` gets what it always got; walking the list for a second-best alternative finds nothing, because there is no k-best over distinct quality levels.

## See also

- [Build a migration](./build-migration.md) for turning a span into a checked mapping, or for the case where a mapping file already exists.
- [Apply field transforms](./field-transforms.md) for the correspondences a vertex map cannot state.
- [Use lenses](./use-lenses.md) for the bidirectional layer built on top of an alignment.
- [Migrations as morphisms](../explanation/migrations-as-morphisms.md) for the model the span sits in.
- [CLI reference](../reference/cli.md) for the full `schema auto-migrate` help text.
