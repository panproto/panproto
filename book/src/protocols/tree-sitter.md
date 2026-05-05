# Tree-sitter and full-AST parsing

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Writing a protocol by hand for every programming language one might reasonably want to parse is not a serious proposal. Two hundred and fifty languages ship with panproto, and we did not hand-write them. They come from [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammars, through a mechanical translation that turns a grammar's node-type metadata into a GAT and a parser. The present chapter explains the translation, what the resulting theories can and cannot express about the language they describe, and when a hand-written upgrade is worth the effort.

Tree-sitter itself is due to @brunsfeld2018treesitter. Its incremental-parsing algorithm descends from @wagnergraham1998efficient, and its parsing-expression-grammar support from the packrat-parsing tradition of @ford2002packrat.

The code is in [`panproto-parse`](https://docs.rs/panproto-parse/latest/panproto_parse/) for the derivation logic and [`panproto-grammars`](https://docs.rs/panproto-grammars/latest/panproto_grammars/) for the bundled tree-sitter grammars.

## What tree-sitter supplies

A tree-sitter grammar is a JavaScript-defined grammar that compiles to a C parser, a `node-types.json` metadata file, and an optional `tags.scm` query for identifier scoping. The parser, given source code in the language, produces a concrete-syntax tree whose node kinds are declared in `node-types.json`. Each node type has a name (like `function_definition` or `binary_expression`), a set of named fields (like `name`, `parameters`, `body` for a function), and a type constraint on each field that names which node types may appear there.

This metadata is a GAT in disguise. Each node type becomes a sort; each field becomes an operation from the parent sort to the field's declared child type; the type constraints on fields become the theory's well-typedness conditions. Tree-sitter's grammar format specifies all of this at the metadata level, so the translation is mechanical.

## The translation

Panproto's [`panproto_parse`](https://docs.rs/panproto-parse/latest/panproto_parse/) module reads `node-types.json` for a grammar and constructs a [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) whose sorts are the named node types, whose operations are the fields of each node type, and whose equations encode the grammar's type constraints. A register function binds the theory to a parser built on tree-sitter's runtime, together with an emitter that walks a schema's instance and renders the source text.

For Rust, this produces a protocol whose sorts include `source_file`, `function_item`, `struct_item`, `expression`, `identifier`, and roughly two hundred others. Operations include `function_item.name : function_item -> identifier`, `struct_item.fields : struct_item -> Array(field_declaration)`, and so on. A Rust source file parsed through this protocol yields a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) under the derived theory, with every AST node a typed value under the right sort. The same machinery runs for Python, TypeScript, Go, Java, C, C++, and the other 242 languages in [`panproto-grammars`](https://docs.rs/panproto-grammars/latest/panproto_grammars/).

## Interstitial text

A critical design choice is what to do with interstitial text: whitespace, comments, and the other material between the nodes a tree-sitter grammar recognises. A lossless round-trip (parse then emit produces the original bytes) requires preserving this material. Tree-sitter itself does not declare interstitial text as part of the grammar; it is recovered only through the parser's byte offsets.

Panproto's derivation captures interstitial text through a companion structure called the *CST complement*, developed in [Format-preserving parsing](https://docs.rs/panproto_io/latest/panproto_io/) and [related panproto-io modules](https://docs.rs/panproto_io/latest/panproto_io/). The complement is a sidecar record attached to the schema that records every comment and every whitespace run in its original position. When the emitter writes the source text back out, it composes the schema's instance data with the complement to produce exactly the input bytes, byte-for-byte. The round-trip law is verified in the [`panproto-io`](https://docs.rs/panproto-io/latest/panproto_io/) test suite.

## Emitting source code that was not parsed

The CST complement carries the bytes of an input that was parsed. A schema constructed by hand or by migration, with no parse history behind it, has no complement to consult; the emitter has to render source text from the schema alone. The 0.40 release adds a de-novo emitter for this case in [`AstParser::emit_pretty`](https://docs.rs/panproto-parse/latest/panproto_parse/), which walks each language's tree-sitter `grammar.json` production rules and renders by-construction source text without parse-recovered byte positions or interstitial material.

The walk handles the production combinators tree-sitter exposes (sequence, choice, repeat, optional, named field, alias, symbol, literal string, regex pattern, token, precedence). Hidden rules (those whose names begin with an underscore) inline-expand at the call site rather than producing their own surface form. Declared supertypes also expand: a `parameter` symbol matches an `identifier` schema vertex when the grammar lists `parameter` in its supertypes, which lets the emitter follow the same abstraction tree-sitter builds for query authors. Named alias productions take a child whose kind matches the alias's value and walk the alias's inner content as the rule, so the production at a child position depends on which alias the parent rule wrapped the child in, not on the child's kind alone. That last detail is the dependent-optic shape from [Protolenses](../core/protolenses.md): the optic at a site is not fixed by the site's own type but by the type-level choice the parent made.

A `FormatPolicy` supplies a default whitespace policy. Per-language polish currently covers JSON, TOML, Rust, Python, and Go; YAML is pending. All 250 vendored grammars now ship `grammar.json`, so the rule walker has a complete set of inputs to work from. The de-novo emitter closes the lens-to-source-code story for downstream tooling: a schema produced by migrating a Python source file can be rendered back to Python without ever having held the original bytes.

The parse/emit pair is exposed as a checkable lens through the `parse_emit_lens` module of [`panproto-parse`](https://docs.rs/panproto-parse/latest/panproto_parse/). `ParseEmitLens` packages a single language's parse and emit into a `Lens<bytes, schema>`, and two law-checkers verify the lens's behaviour on concrete inputs: `check_emit_parse` is the EmitParse retraction (`parse(emit(s))` is structurally equivalent to `s` modulo byte positions) and `check_parse_emit` is the ParseEmit stability law (`emit(parse(b)) == b` byte-for-byte when `b` is parseable). Structural equivalence here is witnessed jointly by `kind_multiset` (the vertex-kind multiset) and `edge_multiset` (a multiset over `(src_kind, edge_kind, tgt_kind)` triples); the vertex multiset alone does not distinguish a tree from its mirror, so the edge witness is load-bearing. The companion `strip_complement` function removes byte-position constraints while preserving the choice discriminators that the walker recorded at parse time; the discriminators are what makes the retraction tight rather than approximate, since they fix which CHOICE alternative the original parse committed to.

## What the derived theory can express

The derived theory captures the *structural* shape of source code. A function's name, parameter list, return type, and body are all present as operations. A type's fields, a module's imports, an expression's operator and operands are all there. Structural operations over source code, a rename across an entire codebase or an insertion of a new field into every struct of a given name, are expressible as migrations in the derived protocol.

The derived theory also captures the type constraints tree-sitter itself expresses. A tree-sitter grammar that says a `function_item`'s `parameters` field must contain a `parameters` node produces an equation in the derived theory that rejects any schema whose parameter field is populated with, say, an expression node.

## What the derived theory cannot express

A tree-sitter grammar does not declare a language's type system, its name-resolution rules, or its semantic constraints. A derived theory therefore accepts source code that is syntactically well-formed but semantically invalid: a variable used before declaration, a type mismatch in an operation, a call to a non-existent function. Those classes of error are the job of a language-specific analyser (a Rust compiler, a Python type checker) rather than of a tree-sitter-derived protocol.

For the programmer using panproto, this means that parsing a Rust source file through the Rust protocol produces a schema whose instance passes panproto's theory-level validation whenever the source is syntactically valid Rust. It does not follow that the code compiles. A developer who wants that guarantee combines panproto's parse with a call to `rustc` or a similar language-specific check; panproto is not a replacement for a compiler.

## Authoring grammars from spec

Most of the grammars panproto bundles are vendored from upstream `tree-sitter-<language>` repos: someone else wrote `grammar.js` and panproto's `tools/fetch-grammars.py` clones the repo and copies the result into `grammars/<language>/`. Two grammars in the bundle are authored in panproto itself against the language's documented specification: `tidal_mini` (TidalCycles mini-notation) and `strudel_mini` (Strudel mini-notation). The pattern they follow is the one any future panproto-authored grammar should follow:

1. Read the language's official syntactic reference. Cite the exact section each grammar rule is grounded in. For Tidal mini-notation that reference is the [TidalCycles mini-notation page](https://tidalcycles.org/docs/reference/mini_notation); for Strudel it is the [Strudel mini-notation page](https://strudel.cc/learn/mini-notation/).
2. Write `grammar.js` from those documented constructs only; do not invent syntax. Per-rule comments cite the documented example each rule was derived from.
3. Add a corpus test under `test/corpus/spec_examples.txt` for every documented construct. Every test case is named after the spec construct it exercises.
4. Run `tree-sitter generate` to produce `parser.c`, `grammar.json`, and `node-types.json`. Commit both the source and the generated outputs so consumers without the tree-sitter CLI can build the C parser directly.

The result for both Tidal and Strudel is an *island* grammar: Tidal and Strudel mini-notation are embedded inside the string argument of a host-language function call, not standalone files. The same procedure works for full-file grammars. The advantage of authoring a grammar against the spec rather than copying syntax from a reference implementation is that the grammar is independently grounded: it tracks the spec rather than a particular interpreter's accidental quirks. The disadvantage is that it forces every construct through the documentation, so anything the docs leave underspecified surfaces as a question rather than as silent inheritance.

## Hand-written protocols as upgrades

For languages whose semantic structure panproto needs to reason about (Rust, when a migration is rewriting trait bounds in a way that requires understanding trait inheritance, for example), a hand-written protocol can be layered on top of the tree-sitter-derived one. The hand-written protocol's theory imports the derived theory through a theory morphism, then adds new sorts (trait resolution tables, type environments) and operations that encode the semantic structure. Migrations under the hand-written protocol have access to the richer information and can perform transformations the derived protocol alone could not verify.

Panproto's Rust protocol has this shape today only partially: the sort structure is derived, but a small number of operations (name resolution for `use` declarations, basic type-environment construction) are added by hand. The expectation is that language-specific semantic protocols will be contributed over time; the derived-from-tree-sitter substrate gives every such contribution a working starting point.

## Further reading

@brunsfeld2018treesitter is the original presentation of tree-sitter and the best overview of its design goals. The incremental-parsing algorithm is in the lineage of @wagnergraham1998efficient, and the parsing-expression-grammar tradition it extends is documented in @ford2002packrat. The tree-sitter project's documentation at <https://tree-sitter.github.io/tree-sitter/> covers the grammar authoring process and the `node-types.json` schema that panproto's derivation reads.

For the broader category-theoretic framing of grammars as theories, the institutions framework of @goguenburstall1992institutions is the formal setting. Panproto's derivation does not depend on that framework explicitly — the translation is simple enough not to need it — but the underlying identification of a grammar with a specification of a theory is institutional.

## Closing

Part IV ends with this chapter. Part V turns to [version control](../vcs/git-background.md) and applies Part II's constructions to a new category: schemas under a fixed protocol rather than protocols themselves.
