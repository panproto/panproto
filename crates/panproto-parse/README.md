# panproto-parse

Tree-sitter full-AST parsing for panproto language protocols.

## Overview

This crate provides the schema-level presentation functors that map between source code text and panproto `Schema` models. It operates at the schema level of panproto's two-parameter architecture (complementing `panproto-io` which operates at the instance level).

## Theory extraction

Tree-sitter grammars are theory presentations. Each grammar's `node-types.json` is structurally isomorphic to a GAT: named node types become sorts, fields become operations, supertypes become abstract sorts with subtype inclusions. The `theory_extract` module auto-derives panproto theories from grammar metadata, ensuring the theory is always in sync with the parser.

| `node-types.json` | panproto GAT |
|---|---|
| Named node type | Sort (vertex kind) |
| Field (`required: true`) | Operation (mandatory edge kind) |
| Field (`required: false`) | Partial operation |
| Field (`multiple: true`) | Ordered operation |
| Supertype | Abstract sort with subtype inclusions |

## Generic walker

Because theories are auto-derived from the grammar, the AST walker is fully generic: one `AstWalker` implementation works for all languages. The node's `kind()` IS the panproto vertex kind; the field name IS the edge kind. Per-language customization is limited to scope detection hints and formatting constraint extraction.

## Interstitial text emission

The walker captures interstitial text (keywords, punctuation, whitespace between named children) as constraints with byte positions. The emitter collects all text fragments (interstitials + leaf literals), sorts by byte position, and concatenates to produce exact round-trip fidelity: `emit(parse(source)) == source`.

## Supported languages

TypeScript, TSX, Python, Rust, Java, Go, Swift, Kotlin, C#, C, C++

## License

MIT
