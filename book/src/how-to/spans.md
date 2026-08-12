# Find a span between two schemas

A *span* is a pair of schema morphisms out of a shared middle: `src ← A → tgt`. The middle, called the **apex**, is the part of the source that the search found a home for in the target. Use a span when the two schemas overlap without one embedding into the other, which on real schema pairs is the ordinary case rather than the exception.

## Prerequisites

The Rust SDK ([`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/)) or the `schema` CLI. A protocol, because the apex is itself a schema and a schema is well formed only against a protocol.

## The task

### From the CLI

```sh
schema auto-migrate schemas/v1.json schemas/v2.json
```

The output reports the apex size, what fraction of the source it covers, the quality, and the interval the search proved the quality lies in:

```text
Found span (quality: 0.451, bounds: [0.451, 0.451]):

Apex: 3 of 4 vertices (75.0% coverage), 2 edges

Vertex map:
  post -> post
  post.lang -> post.lang
  post.text -> post.body
```

Three flags select how much of an answer counts as one. They form a strictness ladder over a single search rather than selecting between searches:

| Flag | Accepts |
|---|---|
| `--total` | Only a span covering every source vertex, i.e. a total morphism. Exits non-zero otherwise. |
| (none) | Any span covering at least one vertex. |
| `--span` | Any span, including an empty apex, which reports that the two schemas share nothing. |

`--monic` is orthogonal to all three and requires the vertex map to be injective. `--json` writes the span's right leg, a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) out of the apex.

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

The left leg is literally an inclusion: apex vertex identifiers *are* source vertex identifiers. That is what makes it a mono by construction and what makes two spans comparable without a graph-isomorphism test.

### Pinning what you already know

A correspondence you *know* goes in [`SearchOptions::hard_pins`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/struct.SearchOptions.html), which restricts a source vertex to exactly that target. A correspondence something *inferred* belongs in the evidence table [`SpanSearch::with_evidence`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html) reads, where it changes which assignment is optimal without removing any other from the search. The distinction is load-bearing: two individually plausible pins can be jointly infeasible, and a pin that turns out to be wrong costs a solution rather than a rank.

### Merging along the apex

A span is the input a pushout wants. `SchemaSpan::to_overlap` produces the pair list `schema_pushout` expects, and `SchemaSpan::pushout` performs the merge, returning `src ⊔_A tgt` together with the two injections. The pushout is the integrated schema; the CLI reaches it through `schema integrate --auto-overlap`.

## Verification

Every span carries a `SpanCertificate` recording what the construction proved rather than what it assumed:

- `proven_optimal` says whether the search ruled out a better span, or merely ran out of budget. When it is false, `quality_bounds` is the interval that separates "0.4, and nothing better exists" from "0.4, and the search stopped before it could tell".
- `legs_are_functorial` records that both legs passed the schema-morphism check: every mapped edge lands between the images of its own endpoints.
- `left_existence` and `right_existence` report the existence check on each leg separately, because the two legs have different codomains and can fail different obligations.
- `apex_pointed` says whether the apex has an entry point. Inducing never synthesises one, so an apex with no entries is recorded rather than repaired.
- `apex_digest` plus the two leg maps is the span's identity, which is how a caller checks that a span was not assembled from parts.

## Common mistakes

- **Comparing `quality` across source schemas.** Every denominator in the objective is fixed by the source, so two spans over one source are comparable and two spans over different sources are not. There is no absolute reading of the number and no threshold on it is meaningful across pairs. Read `apex_coverage` alongside it.
- **Reading an empty apex as a verdict.** An empty apex over a non-empty source reads `quality = 0.0`, because every vertex paid the drop cost; an empty apex over an *empty* source reads `1.0`, because there was nothing to pay either. Both say "these two share nothing", and they are numerically opposite.
- **Confusing the apex with a symmetric lens's apex.** The two are different objects related by pushout and then `Mod`. Only the iso path produces a span whose right leg is a mono, which is the case `SymmetricLens::from_span` needs, so `auto_symmetric` runs that path.
- **Routing inferred correspondences through `hard_pins`.** See above: a pin collapses a domain, and a collapsed domain cannot be recovered by the objective.

## See also

- [Build a migration](./build-migration.md) when a mapping file already exists.
- [Use lenses](./use-lenses.md) for the bidirectional layer built on top of an alignment.
- [Migrations as morphisms](../explanation/migrations-as-morphisms.md) for the model the span sits in.
- [CLI reference](../reference/cli.md) for the full `schema auto-migrate` help text.
