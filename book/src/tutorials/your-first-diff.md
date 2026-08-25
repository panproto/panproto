# Your first diff

This tutorial compares two [ATProto](https://atproto.com/) Lexicon documents and reports one removed field and one added field.

You will create two versions of a `User` record and run one structural diff. The inputs remain ordinary Lexicon JSON rather than panproto's internal schema representation.

## Prerequisite

Install the `schema` binary by following [Install the CLI](../how-to/install/cli.md), then confirm that `schema --version` succeeds.

## Create two versions

The following block creates a fresh directory, a small manifest that identifies the document protocol, and both input files:

```sh
mkdir -p panproto-first-diff
cd panproto-first-diff

cat > panproto.toml <<'EOF'
[workspace]
name = "first-diff"

[[package]]
name = "lexicons"
path = "."
protocol = "atproto"
EOF

cat > user-v1.json <<'EOF'
{
  "lexicon": 1,
  "id": "com.example.user",
  "defs": {
    "main": {
      "type": "record",
      "key": "tid",
      "record": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "age": { "type": "integer" }
        }
      }
    }
  }
}
EOF

cat > user-v2.json <<'EOF'
{
  "lexicon": 1,
  "id": "com.example.user",
  "defs": {
    "main": {
      "type": "record",
      "key": "tid",
      "record": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "years": { "type": "integer" }
        }
      }
    }
  }
}
EOF
```

*Listing 2.1: A manifest and two complete Lexicon inputs for the first structural diff.*

The two files differ by one field name: `age` became `years`.

## Run the diff

From `panproto-first-diff/`, run:

```sh
schema diff user-v1.json user-v2.json
```

The command uses `panproto.toml` to select the ATProto document parser, then compares the resulting schema graphs. Its report includes a removed `com.example.user:body.age` vertex and an added `com.example.user:body.years` vertex, together with the corresponding property edges. That report establishes the structural removal/addition pair. The rename interpretation requires the second pass below.

Rename detection is a second pass over that structural result:

```sh
schema diff user-v1.json user-v2.json --detect-renames
```

If a removed and added element clear the detector's similarity threshold, the command adds them to a `Detected renames` section with confidence scores. This score is evidence for a possible correspondence. [Your first migration](./your-first-migration.md) later records the correspondence explicitly.

For a compact count rather than the element-by-element report, run:

```sh
schema diff user-v1.json user-v2.json --stat
```

The diff is structural in a precise sense: panproto compares parsed vertices, edges, and constraints rather than changed lines. The shared diff loader also accepts panproto schema JSON, source files supported by the [tree-sitter](https://tree-sitter.github.io/tree-sitter/) registry, and manifest-backed directories.

## Next

[Your first schema](./your-first-schema.md) builds the same `User` model through the SDK and validates records against it. If the command line is your main interface, [Schema version control basics](./schema-vcs-basics.md) turns these source files into commits and branches. [The vocabulary in plain terms](../explanation/decoder-ring.md) defines *vertex*, *edge*, *migration*, and *lens* when you are ready for those names.
