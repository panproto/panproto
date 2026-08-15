# Build a migration

A migration is a structured map between two schemas plus, optionally, value-level transforms applied during lift. This page covers deriving one from a span search, building one from the CLI and from the SDKs, and checking either before any data moves.

## Prerequisites

Two schemas in the same protocol (or compatible protocols). The `schema` CLI installed, or one of the language SDKs.

## The task

### Derive the mapping from a span

Most pairs do not arrive with a mapping file. `schema auto-migrate` writes one:

```sh
schema auto-migrate schemas/v1.json schemas/v2.json --json > migrations/v1-to-v2.json
```

The file holds the span's right leg, a migration out of the *apex* rather than out of `v1`. That distinction costs nothing here. The left leg is an inclusion, so apex vertex identifiers are source vertex identifiers, the file's `vertex_map` keys are `v1` vertex names, and every `v1` vertex the search found no home for is simply absent from the map. `schema check` quantifies over the map's own entries rather than over the source schema, so it checks a partial mapping instead of rejecting it.

`check` says nothing about how much of `v1` that mapping covers. Run `auto-migrate` once without `--json`, read the coverage line, and decide whether a partial mapping is the answer you wanted; [Find a span between two schemas](./spans.md) covers reading that report, along with the `--total` and `--span` ladder, and the `--monic` flag that sits outside it.

Then treat the file as a draft. The search returns the optimum of an objective whose component weights have never been calibrated against a labelled corpus of correct alignments, so what comes back encodes a judgement about what a good match looks like rather than a measurement of one. Read the vertex map, fix what is wrong, and add value-level transforms for the correspondences a vertex map cannot state: one source field becoming two target fields has no morphism to be, and [Apply field transforms](./field-transforms.md) is where that case goes instead.

Every SDK carries the same route. In Rust it is [`find_span`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_span.html) followed by `span.right`, which is already a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) and needs no conversion; in Python `panproto.find_span(src, tgt, protocol).right`; in TypeScript `p.span(from, to)`; in Haskell `findSpan`; in Swift `findSpan(to:in:options:constraints:)`. Beside them sit `find_morphisms` and `find_best_morphism`, the total-morphism entry points. Both return the morphisms attaining the optimum rather than the whole hom-set ranked, and that distinction bites a binding consumer written against the older behaviour: reading `results[0]` gets what it always got, while walking the list for a second-best alternative now turns up only the ties.

### From the CLI

Given `schemas/v1.json` and `schemas/v2.json`, plus a mapping file `migrations/v1-to-v2.json`:

```sh
schema check --src schemas/v1.json --tgt schemas/v2.json --mapping migrations/v1-to-v2.json
```

`check` runs the existence check: which fields in `v2` require which fields in `v1`, and is every required input present. Exits zero if the migration is well-defined.

To also type-check at the GAT level (equivalent to a separate `schema typecheck`):

```sh
schema check --src schemas/v1.json --tgt schemas/v2.json --mapping migrations/v1-to-v2.json --typecheck
```

For schema-level diff classification, run `schema compat`:

```sh
schema compat schemas/v1.json schemas/v2.json --protocol atproto
```

It prints the changes grouped by tier and exits 0 when the change is non-breaking, 1 when it is breaking. To inspect the generated lens chain as well, pair `schema lens generate` with `schema diff --lens`:

```sh
schema lens generate --protocol atproto schemas/v1.json schemas/v2.json --save lens.json
schema diff schemas/v1.json schemas/v2.json --lens --save lens.json
```

To migrate data, use the VCS-driven path: commit `v1` and `v2` to a panproto repository, then run `schema data migrate <data-dir>` against the working tree (see [Schema VCS data versioning](./schema-vcs/data-versioning.md)).

### From the SDKs

```ts
const mig = p
  .migration(srcSchema, tgtSchema)
  .map('user', 'user')
  .compile();

const { data: forward } = mig.lift(oldRecord);   // forward
const { view, complement } = mig.get(oldRecord); // forward, retaining complement
mig.put(view, complement);                       // backward
```

`p.checkExistence(src, tgt, builder)` runs the same existence check as the CLI. Python and Rust SDKs use the same shape with language-idiomatic naming.

## Verification

`schema check` exits zero if the migration is well-defined (existence conditions hold). For diff classification, use `panproto.diff_and_classify(old, new, protocol)` in Python, or `panproto_check::diff(old, new)` followed by `panproto_check::classify(&diff, &protocol)` in Rust. In TypeScript the equivalent is `Panproto.diffFull(old, new).classify(protocol)`. Each returns a `CompatReport` whose `classification` field records one of three tiers, alongside a list of breaking changes, a list of non-breaking changes, and a `compatible` boolean:

| `classification` | Meaning |
|---|---|
| `fully-compatible` | No breaking and no non-breaking changes; the two schemas agree in shape. Old data lifts unchanged. |
| `backward-compatible` | Non-breaking changes only; old data lifts, either unchanged or via a value-level transform. |
| `breaking` | At least one change cannot be lifted safely; CI should reject. |

For wiring this into CI, see [Breaking-change gate](./ci/breaking-change-gate.md).

## Common mistakes

- Skipping `--typecheck` for non-trivial migrations. Existence checking does not catch GAT-level type errors; the `--typecheck` flag does.
- Treating a `breaking` classification as a warning. CI should reject by default; merging a breaking migration without an explicit acknowledgement is the most common cause of data corruption in production.
- Lifting data before the check passes. Lift can produce invalid output if the migration is not well-defined.
- Shipping an auto-derived mapping unreviewed. `proven_optimal` on the span says the search ruled out a better answer under the shipped weights. It says nothing about whether those weights rank alignments the way you would.
- Reading a passing `schema check` as a claim about coverage. The existence check quantifies over the entries the mapping holds, so a mapping covering one vertex out of twenty passes exactly as cleanly as a total one. Coverage comes from the span report.
- Ignoring the contracting-right-leg warning and lifting anyway. Two source vertices mapped to one target vertex give lift no rule for combining them; `schema auto-migrate` says so on stderr, and [Find a span between two schemas](./spans.md) covers what to do about it.

## See also

- [Reference: CLI](../reference/cli.md) for the full subcommand list.
- [Find a span between two schemas](./spans.md) for deriving the mapping and reading its certificate.
- [Apply field transforms](./field-transforms.md) for value-level transforms.
- [Migrations as morphisms](../explanation/migrations-as-morphisms.md) for the model.
