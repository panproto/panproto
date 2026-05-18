# Define a schema from the CLI

## Prerequisites

The `schema` binary installed ([Install the CLI](../install/cli.md)). A schema file in a supported protocol, or a protocol name to scaffold an empty one.

## The task

### Validate an existing schema

```sh
schema validate --protocol atproto path/to/schema.json
```

Exits zero on success. On failure, prints the failing equation or constraint with the offending vertex and edge.

### Scaffold from an existing schema

```sh
schema scaffold --protocol atproto schemas/post.json
```

`scaffold` runs free-model construction over the given schema and prints sample term assignments to stdout. Use `--json` for machine-readable output, `--depth` and `--max-terms` to bound the search. (To start from nothing, write a one-vertex schema by hand and scaffold against that.)

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

`verify` checks that the schema satisfies *every* equation in the protocol's theory, not just the constraints you wrote. A pass guarantees the schema is well-formed in the categorical sense.

## Common mistakes

- Forgetting `--protocol`. Many subcommands require an explicit protocol; auto-detection happens only at the project level when a `panproto.toml` is present.
- Running `schema validate` when you mean `schema check` (the latter checks a *migration*, not a schema).

## See also

- [Reference: CLI](../../reference/cli.md) for the full subcommand list.
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
