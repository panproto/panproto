# Your first diff

You will take two versions of a schema file, diff them, generate a migration between them, and convert a record from the old shape to the new one. Everything happens on the command line, with schema files you could have written before ever hearing of panproto. About ten minutes.

By the end you will have: a diff that identifies exactly what changed, a generated bidirectional converter (a *lens*), a record converted forward through it, and a verification pass confirming the conversion loses nothing.

You will not build a schema through the SDK, and you will not meet a vertex, an edge, or a theory. Those exist, and the later tutorials introduce them where they are needed; the core workflow does not require them.

## Prerequisites

The `schema` binary ([Install the CLI](../how-to/install/cli.md)). Nothing else.

## Step 1: write version 1

Create a working directory and a schema file. We use an ATProto lexicon here because `atproto` is the most fully-built-out protocol in the registry, but the file is ordinary JSON of a kind you may have seen on Bluesky's GitHub; the workflow is the same for the other [protocols](../reference/protocols.md).

`user-v1.json`:

```json
{
  "lexicon": 1,
  "id": "com.example.user",
  "defs": {
    "main": {
      "type": "record",
      "description": "A user profile.",
      "key": "tid",
      "record": {
        "type": "object",
        "required": ["name"],
        "properties": {
          "name": { "type": "string", "maxLength": 640 },
          "age": { "type": "integer", "minimum": 0 }
        }
      }
    }
  }
}
```

Check that panproto can read it:

```sh
schema validate --protocol atproto user-v1.json
```

A zero exit code means the file parses and satisfies the protocol's rules.

## Step 2: write version 2

Copy the file to `user-v2.json` and make one change: rename `age` to `years` (in both `properties` and, if you made it required, in `required`). A rename is the awkward case for diffing: plain-text tools see it as a removal plus an addition.

## Step 3: diff the two versions

```sh
schema diff user-v1.json user-v2.json
```

The diff is structural, not textual: it reports the schema elements that changed, rather than the lines. As far as this diff can tell, `age` was removed and `years` was added. Now ask panproto to look harder:

```sh
schema diff user-v1.json user-v2.json --detect-renames
```

With rename detection on, the pair is reported as a likely rename instead of a removal plus an addition. The distinction matters: a removal is a breaking change, while a rename is migratable with no data loss.

## Step 4: generate the migration

```sh
schema lens generate --protocol atproto user-v1.json user-v2.json --save chain.json
```

This produces a *lens*: a converter that runs in both directions. Forward takes v1 records to v2 shape; backward takes v2 records to v1 shape. The generated chain lands in `chain.json` as plain JSON. Open it if you are curious; it reads as a short list of steps, and for this schema pair the load-bearing step is the rename.

Add `--explain` to see why the generator aligned the fields the way it did, with a confidence per step.

## Step 5: convert a record

Create `alice.json`:

```json
{ "$type": "com.example.user", "name": "Alice", "age": 30 }
```

Convert it:

```sh
schema data convert --protocol atproto \
  --from user-v1.json --to user-v2.json \
  alice.json -o alice-v2.json
```

`alice-v2.json` contains the same record with `years` in place of `age`. Point the positional argument at a directory instead of a file and the same command converts a batch.

## Step 6: verify the round trip

panproto checks that the conversion loses nothing, rather than asking you to trust it:

```sh
schema lens verify --protocol atproto alice.json user-v2.json
```

Verification exercises the round-trip laws on your data: converting forward and then backward must return the record you started with. A pass means the migration is loss-free for the records you tested.

## What you built

A structural diff, a rename detected as a rename, a generated two-way migration, a converted record, and a mechanical check that nothing was lost. You wrote two JSON files and ran five commands; panproto generated the migration.

## Where next

- [Your first schema](./your-first-schema.md) introduces the SDK and the schema-construction API, for when you need to build or inspect schemas programmatically rather than from files.
- [Schema version control basics](./schema-vcs-basics.md) turns the v1/v2 pair into a history with commits and branches.
- [The vocabulary in plain terms](../explanation/decoder-ring.md) translates panproto's terms of art (lens, complement, morphism, and the rest), one line each.
- [What panproto solves](../explanation/what-panproto-solves.md) is the plain-terms account of the problem this workflow addresses.
