# Roadmap

This file lists the themes of the next planned panproto releases. Each release has a GitHub milestone with its
issues; the milestone is the authoritative list. Work not listed here has no release commitment.

## 0.77.0: Graph Core

One engine for tree, table, and graph instances, with its laws stated in the book's Correctness part and checked
against independent reference implementations: exact values, value I/O, theory composition, keys, instance
representation, Kan extensions, restriction and contraction, edit primitives, validation, segments, state-based
lenses, and reference resolution.

## 0.78.0: Semantic Formats

[Milestone](https://github.com/panproto/panproto/milestone/1)

- Every advertised format reads into a documented tree, table, or graph instance and writes back out, with exact
  values, resolved references, validation, and canonical output.
- Format families: text (YAML, TOML, CSV, TSV, node-link JSON), SQL schemas and data, other databases, RDF, XML
  and other annotation formats, API descriptions, configuration, data schemas, data-science formats, domain
  standards, serialization formats, and web documents.
- Stated and checked laws for the lens and theory DSLs, migration search, parsing and emission, and the existing
  version-control merge.
- A published table of supported formats and their limits, and a table of unsupported formats with reasons.

## 0.79.0: Operational Graph Parity

[Milestone](https://github.com/panproto/panproto/milestone/2)

- Edit lenses for graphs and tables.
- Schema version control for tree, table, and graph data.
- Segmented execution and segmented edits for instances too large to process whole.
- The full feature set in C, Swift, Haskell, WASM and TypeScript, Python, and the CLI, checked by one shared
  conformance corpus.
- Final documentation, fuzzing, and published benchmarks.
