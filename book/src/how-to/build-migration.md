# Build a migration

A migration is a structured map between two schemas plus, optionally, value-level transforms applied during lift. This page covers building one from the CLI and from the SDKs.

## Prerequisites

Two schemas in the same protocol (or compatible protocols). The `schema` CLI installed, or one of the language SDKs.

## The task

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

For schema-level diff classification, generate a lens between the two schemas:

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

`schema check` exits zero if the migration is well-defined (existence conditions hold). For diff classification, use `panproto.diff_and_classify(old, new, protocol)` in Python, or `panproto_check::diff(old, new)` followed by `panproto_check::classify(&diff, &protocol)` in Rust. In TypeScript the equivalent is `Panproto.diffFull(old, new).classify(protocol)`, which returns a `CompatReport` with one of:

| Classification | Meaning |
|---|---|
| `fully-compatible` | Old data lifts unchanged; the migration is a refinement. |
| `backward-compatible` | Old data lifts via a value-level transform. |
| `breaking` | Some old records cannot be lifted; CI should reject. |

For wiring this into CI, see [Breaking-change gate](./ci/breaking-change-gate.md).

## Common mistakes

- Skipping `--typecheck` for non-trivial migrations. Existence checking does not catch GAT-level type errors; the `--typecheck` flag does.
- Treating a `breaking` classification as a warning. CI should reject by default; merging a breaking migration without an explicit acknowledgement is the most common cause of data corruption in production.
- Lifting data before the check passes. Lift can produce invalid output if the migration is not well-defined.

## See also

- [Reference: CLI](../reference/cli.md) for the full subcommand list.
- [Apply field transforms](./field-transforms.md) for value-level transforms.
- [Migrations as morphisms](../explanation/migrations-as-morphisms.md) for the model.
