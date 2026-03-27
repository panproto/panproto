# panproto-grammars

Pre-compiled tree-sitter grammars for panproto. Bundles up to 248 languages,
compiled from vendored C sources via `build.rs`.

## Feature flags

Each grammar is individually gated behind a `lang-{name}` feature.
Group features enable sets of languages at once:

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
| `group-all` | All 248 languages |

## Usage

```rust
// Iterate all enabled grammars
for grammar in panproto_grammars::grammars() {
    println!("{}: {} extensions", grammar.name, grammar.extensions.len());
}

// Look up by extension
if let Some(lang) = panproto_grammars::extension_to_language("rs") {
    assert_eq!(lang, "rust");
}
```

## Updating grammars

Grammar sources live in `grammars/` at the workspace root, fetched by
`tools/fetch-grammars.py` from the repos listed in `grammars.toml`.
See `tools/README.md` for prerequisites and usage.

## License

MIT
