# panproto-grammars

[![crates.io](https://img.shields.io/crates/v/panproto-grammars.svg)](https://crates.io/crates/panproto-grammars)
[![docs.rs](https://docs.rs/panproto-grammars/badge.svg)](https://docs.rs/panproto-grammars)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Build-time tree-sitter grammar registry used by `panproto-parse`.

## What it does

`grammars.toml` defines 261 grammar entries. Each `lang-{name}` feature selects one entry, and the `group-*` features select fixed sets. The default feature is `group-core`, which selects 11 languages: Python, JavaScript, TypeScript, Java, C#, C++, PHP, Bash, C, Go, and Rust.

In a workspace build, `build.rs` compiles the enabled vendored C or C++ sources. A missing source directory, missing `parser.c`, or compilation failure causes that grammar to be omitted. Thus `grammars()` reports the grammars that compiled, not every feature that Cargo enabled.

The crates.io package cannot include the workspace-level `grammars.toml` and `grammars/` tree. Its build script consequently generates an empty registry. A Rust application using the published crate must register parsers from individual tree-sitter grammar crates with `panproto_parse::ParserRegistry`. The companion crates described below are internal, unpublished crates used to build Python wheels from a repository checkout.

## Example

This example assumes a repository build in which the default group compiled successfully.

```rust,ignore
// Iterate all successfully compiled grammars and print their names.
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
| `grammars()` | Returns the successfully compiled `Grammar` values, sorted by name |
| `has_grammar(name)` | Returns `true` if the named grammar is compiled in |
| `extension_to_language(ext)` | Maps a file extension to its grammar name, or `None` if not recognized |
| `grammar_count()` | Returns the number of successfully compiled grammars |
| `Grammar` | Holds `name`, `extensions`, `language`, `node_types`, and optional `tags_query` and `grammar_json` data |

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
| `group-all` | All 261 manifest entries |
| `lang-{name}` | Any individual language by name |

## Companion grammar packs

The core Python extension is built with the default `group-core` feature. Companion wheels add one feature group each. Installing a companion registers a `panproto.grammars` entry point. The public `panproto.AstParserRegistry()` factory reads those entry points whenever it constructs a registry.

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

Each wheel contains a separate pyo3 cdylib built from `crates/panproto-grammars-<group>/`. Its package metadata and entry point live under `bindings/python-grammars-<group>/`. Duplicate grammar names are ignored when a companion overlaps the core group or another installed pack. A companion that fails to load or supplies invalid metadata produces a runtime warning, and its affected grammars remain unavailable.

## License

[MIT](../../LICENSE)
