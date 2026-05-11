# panproto-grammars

[![crates.io](https://img.shields.io/crates/v/panproto-grammars.svg)](https://crates.io/crates/panproto-grammars)
[![docs.rs](https://docs.rs/panproto-grammars/badge.svg)](https://docs.rs/panproto-grammars)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Pre-compiled tree-sitter grammars for 259 programming languages, used by `panproto-parse`.

## What it does

This crate bundles tree-sitter grammar sources for up to 259 languages and compiles them from C at build time. Each `Grammar` value provides the tree-sitter `Language` object needed for parsing, the raw `node-types.json` bytes needed for theory extraction, the optional `grammar.json` production-rule table (used by `panproto-parse`'s `emit_pretty` to render by-construction schemas), and the file extensions the grammar handles.

The crate is published to crates.io with zero vendored grammars: the C sources weigh roughly 500MB, well above the 10MB package limit. The published version exposes the API surface so downstream crates compile, and consumers register individual grammar crates against `panproto_parse::ParserRegistry`. Inside the workspace, `build.rs` compiles all vendored grammars, so the in-tree build of `panproto-parse` (with the default `grammars` feature) gets the full set.

Each language is gated behind a `lang-{name}` feature flag. Group features enable sets of languages at once. The default group (`group-core`) includes Python, JavaScript, TypeScript, Java, C#, C++, PHP, Bash, C, Go, and Rust.

## Quick example

```rust,ignore
// Iterate all enabled grammars and print their names.
for grammar in panproto_grammars::grammars() {
    println!("{}: {:?}", grammar.name, grammar.extensions);
}

// Look up a grammar by file extension.
if let Some(lang) = panproto_grammars::extension_to_language("rs") {
    assert_eq!(lang, "rust");
}

// Check whether a grammar is compiled in.
assert!(panproto_grammars::has_grammar("python"));
```

## API overview

| Export | What it does |
|--------|-------------|
| `grammars()` | Returns all `Grammar` values enabled by feature flags, sorted by name |
| `has_grammar(name)` | Returns `true` if the named grammar is compiled in |
| `extension_to_language(ext)` | Maps a file extension to its grammar name, or `None` if not recognized |
| `grammar_count()` | Returns the number of enabled grammars |
| `Grammar` | Struct holding `name`, `extensions`, `language` (`tree_sitter::Language`), `node_types` bytes, and optional `grammar_json` production-rule bytes |

## Feature flags

| Feature | Languages |
|---------|-----------|
| `group-core` (default) | Python, JavaScript, TypeScript, Java, C#, C++, PHP, Bash, C, Go, Rust |
| `group-web` | HTML, CSS, JavaScript, TypeScript, TSX, JSON, Vue, Svelte, Astro, GraphQL |
| `group-systems` | C, C++, Rust, Go, Zig, D, Nim, Odin, V, Hare |
| `group-jvm` | Java, Kotlin, Scala, Groovy, Clojure |
| `group-scripting` | Python, Ruby, Lua, Bash, Perl, R, Julia, Nushell, Fish |
| `group-data` | JSON, TOML, XML, YAML, SQL, CSV, GraphQL, Protobuf |
| `group-functional` | Haskell, OCaml, Elm, Gleam, Erlang, Elixir, PureScript, F#, Clojure, Scheme, Racket |
| `group-devops` | Dockerfile, Terraform, HCL, Nix, Bash, YAML, TOML, Make, CMake |
| `group-mobile` | Swift, Kotlin, Dart, Java, Objective-C |
| `group-music` | SuperCollider, LilyPond, ABC, Csound, ChucK, Glicol, Tidal mini-notation, Strudel mini-notation |
| `group-all` | All 259 languages |
| `lang-{name}` | Any individual language by name |

## Companion grammar packs

When the panproto Python wheel is installed from PyPI, only the `group-core` 11 languages are baked into the core `_native` extension. The remaining grammars are distributed as separately-installable companion packs, one wheel per group:

| Wheel | Grammar group |
|-------|--------------|
| `panproto-grammars-web` | `group-web` |
| `panproto-grammars-systems` | `group-systems` |
| `panproto-grammars-jvm` | `group-jvm` |
| `panproto-grammars-scripting` | `group-scripting` |
| `panproto-grammars-data` | `group-data` |
| `panproto-grammars-functional` | `group-functional` |
| `panproto-grammars-devops` | `group-devops` |
| `panproto-grammars-mobile` | `group-mobile` |
| `panproto-grammars-music` | `group-music` |
| `panproto-grammars-all` | `group-all` |

Each is a separate pyo3 cdylib depending on `panproto-grammars` with the named feature flag. The Rust source for the companion crates lives at `crates/panproto-grammars-<group>/`; the Python wheel scaffolding at `bindings/python-grammars-<group>/`. Source builds against `panproto-grammars` directly do not need the companions; they pick a feature flag and link the grammar bytes in directly.

## License

[MIT](../../LICENSE)
