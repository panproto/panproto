# Tutorials

Tutorials walk through a complete example end-to-end. They assume no prior knowledge of panproto and no category-theoretic background. Each one ends with a working artifact you can run and modify.

If a tutorial mentions a concept by name without explaining it (a *lens*, a *protocol*, a *migration*), it links forward to the explanation page that defines it. You do not need to follow those links to finish the tutorial.

| Tutorial | What you build | Time |
|---|---|---|
| [Your first diff](./your-first-diff.md) | Two schema files diffed, a migration generated between them, and a record converted (CLI only, no SDK) | ~10 min |
| [Your first schema](./your-first-schema.md) | A typed schema for a small data model in TypeScript, validated against a protocol | ~10 min |
| [Your first migration](./your-first-migration.md) | A migration that adds a field, renames another, and lifts existing data forward | ~15 min |
| [Schema version control basics](./schema-vcs-basics.md) | A schema repository with branches and a merge | ~20 min |
| [Cross-protocol translation](./cross-protocol-translation.md) | A JSON Schema converted into a Protobuf definition with data round-tripped through both | ~20 min |
