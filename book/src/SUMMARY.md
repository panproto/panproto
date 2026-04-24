# Summary

[Preface](index.md)

# Orientation

- [A note on notation](preface/notation.md)
- [What panproto is](preface/what-panproto-is.md)

# Mathematical foundations

- [Categories](foundations/categories.md)
- [Functors and natural transformations](foundations/functors.md)
- [Universal properties](foundations/universal-properties.md)
- [Colimits and pushouts](foundations/colimits.md)
- [Algebraic and generalized algebraic theories](foundations/gats.md)
- [Rewriting, confluence, and termination](foundations/rewriting.md)

# Core constructions

- [Protocols as theories, schemas as instances](core/schemas-as-instances.md)
- [Dependent sorts in practice](core/dependent-sorts.md)
- [Typeclasses as theory morphisms](core/typeclasses.md)
- [Theory morphisms and instance migration](core/morphisms-and-migration.md)
- [The restrict/lift pipeline](core/restrict-lift.md)
- [Bidirectional lenses](core/lenses.md)
- [Protolenses](core/protolenses.md)
- [Protocol colimits](core/protocol-colimits.md)

# The expression language

- [Syntax and semantics](expr/syntax-semantics.md)
- [Totality and termination](expr/totality.md)
- [Why bounded pure evaluation](expr/design-choices.md)

# Protocols

- [Defining a protocol](protocols/defining.md)
- [ATProto lexicons](protocols/atproto.md)
- [Avro: schema evolution as migration](protocols/avro.md)
- [A relational case study](protocols/relational.md)
- [A document case study](protocols/document.md)
- [Tree-sitter and full-AST parsing](protocols/tree-sitter.md)

# Schematic version control

- [What git already versions and what it does not](vcs/git-background.md)
- [Objects, refs, and the DAG](vcs/objects-and-dag.md)
- [Merge as pushout](vcs/merge-as-pushout.md)
- [Data versioning](vcs/data-versioning.md)
- [The git bridge](vcs/git-bridge.md)

# SDKs and operational use

- [The WASM boundary](sdks/wasm-boundary.md)
- [The Rust SDK](sdks/rust.md)
- [The TypeScript SDK](sdks/typescript.md)
- [The Python SDK](sdks/python.md)
- [The CLI](sdks/cli.md)

# For contributors

- [Workspace layout](contributing/workspace.md)
- [CI, semver-checks, and release](contributing/ci.md)
- [Extending panproto](contributing/extending.md)
- [Experimental and feature-gated subsystems](contributing/experimental.md)

# Appendices

- [Notation reference](appendices/notation-table.md)
- [Glossary](appendices/glossary.md)
- [Open problems](appendices/open-problems.md)
