# Tree-sitter and full-AST parsing

Writing a protocol by hand for every programming language one might reasonably want to parse is not a serious proposal. Two hundred and forty-eight languages ship with panproto, and we did not hand-write them. They come from [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammars, through a mechanical translation that turns a grammar's node-type metadata into a GAT and a parser. The present chapter explains the translation, what the resulting theories can and cannot express about the language they describe, and when a hand-written upgrade is worth the effort.

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

## What the derived theory can express

The derived theory captures the *structural* shape of source code. A function's name, parameter list, return type, and body are all present as operations. A type's fields, a module's imports, an expression's operator and operands are all there. Structural operations over source code, a rename across an entire codebase or an insertion of a new field into every struct of a given name, are expressible as migrations in the derived protocol.

The derived theory also captures the type constraints tree-sitter itself expresses. A tree-sitter grammar that says a `function_item`'s `parameters` field must contain a `parameters` node produces an equation in the derived theory that rejects any schema whose parameter field is populated with, say, an expression node.

## What the derived theory cannot express

A tree-sitter grammar does not declare a language's type system, its name-resolution rules, or its semantic constraints. A derived theory therefore accepts source code that is syntactically well-formed but semantically invalid: a variable used before declaration, a type mismatch in an operation, a call to a non-existent function. Those classes of error are the job of a language-specific analyser (a Rust compiler, a Python type checker) rather than of a tree-sitter-derived protocol.

For the programmer using panproto, this means that parsing a Rust source file through the Rust protocol produces a schema whose instance passes panproto's theory-level validation whenever the source is syntactically valid Rust. It does not follow that the code compiles. A developer who wants that guarantee combines panproto's parse with a call to `rustc` or a similar language-specific check; panproto is not a replacement for a compiler.

## Hand-written protocols as upgrades

For languages whose semantic structure panproto needs to reason about (Rust, when a migration is rewriting trait bounds in a way that requires understanding trait inheritance, for example), a hand-written protocol can be layered on top of the tree-sitter-derived one. The hand-written protocol's theory imports the derived theory through a theory morphism, then adds new sorts (trait resolution tables, type environments) and operations that encode the semantic structure. Migrations under the hand-written protocol have access to the richer information and can perform transformations the derived protocol alone could not verify.

Panproto's Rust protocol has this shape today only partially: the sort structure is derived, but a small number of operations (name resolution for `use` declarations, basic type-environment construction) are added by hand. The expectation is that language-specific semantic protocols will be contributed over time; the derived-from-tree-sitter substrate gives every such contribution a working starting point.

## Further reading

@brunsfeld2018treesitter is the original presentation of tree-sitter and the best overview of its design goals. The incremental-parsing algorithm is in the lineage of @wagnergraham1998efficient, and the parsing-expression-grammar tradition it extends is documented in @ford2002packrat. The tree-sitter project's documentation at https://tree-sitter.github.io/tree-sitter/ covers the grammar authoring process and the `node-types.json` schema that panproto's derivation reads.

For the broader category-theoretic framing of grammars as theories, the institutions framework of @goguenburstall1992institutions is the formal setting. Panproto's derivation does not depend on that framework explicitly — the translation is simple enough not to need it — but the underlying identification of a grammar with a specification of a theory is institutional.

## Closing

Part IV ends with this chapter. Part V turns to [version control](../vcs/git-background.md) and applies Part II's constructions to a new category: schemas under a fixed protocol rather than protocols themselves.
