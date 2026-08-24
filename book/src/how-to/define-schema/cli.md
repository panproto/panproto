# Define a schema from the CLI

## Prerequisites

The `schema` binary installed ([Install the CLI](../install/cli.md)). A schema file in a supported protocol, or a protocol name to scaffold an empty one.

## The task

### Validate an existing schema

```sh
schema validate --protocol atproto path/to/schema.json
```

The command loads panproto schema JSON, checks vertex kinds, edge rules, constraint sorts, required-edge references, and recursion references, then type-checks the registered protocol theories. It exits nonzero if either pass reports an error. It does not parse an ATProto Lexicon or another external schema document at this entry point.

### Scaffold from an existing schema

```sh
schema scaffold --protocol atproto schemas/post.json
```

`scaffold` runs bounded free-model construction over panproto schema JSON and prints sample term assignments. Use `--json` for machine-readable output, and use `--depth` and `--max-terms` to set the bounds. A truncated run is a partial enumeration rather than proof that no other terms exist.

### Inspect

```sh
schema diff schemas/post-v1.json schemas/post-v2.json
```

`diff` reports vertex and edge changes between two schemas. Inside a panproto repository, `schema show <ref>` resolves a commit, schema, or migration object and prints its contents.

## Verification

After validation, run:

```sh
schema verify --protocol atproto path/to/schema.json
```

`verify` tests assignments for the equations in the registered protocol theories, up to `--max-assignments` per equation. The current command prints `Verification passed` even when a theory has type errors or an equation check is incomplete, so use `schema validate` as the CI gate and inspect the full `verify` output. A bounded pass is evidence from the checked assignments, not a proof over every possible assignment.

## Common mistakes

- Passing an external schema-language document to `validate`, `verify`, or `scaffold`. These commands deserialize panproto's internal schema JSON. Parse or convert an external document first.
- Running `schema validate` when you mean `schema check` (the latter checks a *migration*, not a schema).

## See also

- [Reference: CLI](../../reference/cli.md) for the full subcommand list.
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
