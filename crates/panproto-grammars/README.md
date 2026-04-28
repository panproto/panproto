# panproto-grammars

[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Pre-compiled tree-sitter grammars for 250 programming languages, used by `panproto-parse`.

## What it does

This crate bundles tree-sitter grammar sources for up to 250 languages and compiles them from C at build time. Each `Grammar` value provides the tree-sitter `Language` object needed for parsing, the raw `node-types.json` bytes needed for theory extraction, the optional `grammar.json` production-rule table (used by `panproto-parse`'s `emit_pretty` to render by-construction schemas), and the file extensions the grammar handles.

This crate is `publish = false` and is not on crates.io: the vendored C sources weigh roughly 500MB, well above crates.io's 10MB limit. You get these grammars through `panproto-parse`, which depends on this crate when its `grammars` feature is enabled (the default). There is no reason to depend on this crate directly.

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
| `group-all` | All 250 languages |
| `lang-{name}` | Any individual language by name |

## License

[MIT](../../LICENSE)
