# Contributing to the panproto book

## Rust code blocks in the book are compile-tested

Every non-ignored `` ```rust `` code block under `book/src/**/*.md` is
compiled via `rustdoc --test` on every CI run. The driver lives at
`xtask/src/bin/test-book.rs` and the dep set it can reference is the
list in `crates/book-doctest-stub/Cargo.toml`.

Run locally:

```sh
cargo run -p xtask --bin test-book
```

The job is also wired into `.github/workflows/ci.yml` as the
`book-doctest` job and into `publish-book.yml` ahead of the `mdbook
build` step.

### Fence conventions

| Fence | Effect |
|---|---|
| `` ```rust `` | Compiled and executed by rustdoc. Use for simple snippets that have no runtime dependencies. |
| `` ```rust,no_run `` | Compiled but not executed. Use for snippets that touch the filesystem, depend on external state, or are illustrative `main()` programs. Most book examples want this. |
| `` ```rust,ignore `` | Skipped entirely. Reserve for snippets that show internal type definitions copied from the source (`pub enum Foo { ... }`) or contributor-only patterns (`use crate::theories;` inside a panproto-protocols submodule). |
| `` ```text `` | Plain text. Not seen by rustdoc; use for pseudocode or output samples. |
| `` ```sh `` / `` ```ts `` / `` ```python `` / etc. | Other languages. Not tested by the book-doctest job. |

### Hidden setup lines

A block can carry hidden `# ` lines that are stripped from rendered
output but included in compilation. Use them to import dependencies,
set up shared state, or wrap a snippet in `fn main()` so it compiles
as a standalone program:

````markdown
```rust,no_run
use panproto_core::schema::SchemaBuilder;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let proto = panproto_core::protocols::atproto::protocol();
let schema = SchemaBuilder::new(&proto)
    .vertex("user", "record", Some("app.example.user"))?
    .entry("user")
    .build()?;
# Ok(()) }
```
````

The reader sees only the un-prefixed lines; rustdoc compiles all of
them. Each Rust block in the book is its own translation unit, so
state from an earlier block does not carry into the next — repeat
the setup with hidden lines whenever a block references something
defined elsewhere.

### Adding a new dependency

If a new book example needs a crate not yet in scope:

1. Add it as a dependency of `crates/book-doctest-stub/Cargo.toml`.
2. Add its crate name (with underscores) to `ALLOWED_EXTERN_CRATES`
   in `xtask/src/bin/test-book.rs`.
3. Run `cargo run -p xtask --bin test-book` to confirm rustdoc finds
   it.

### Common failure modes

- `cannot find module or crate panproto_core` — the stub crate was
  not built or `--extern` was not passed. Re-run the xtask; it builds
  the stub fresh on every invocation.
- `multiple different versions of crate X` — the workspace has two
  feature-unification contexts for `X` and the xtask picked the
  wrong one. Open `book-doctest-stub/Cargo.toml` and pin the feature
  set the consumer crate uses (e.g. `serde_json` with `preserve_order`).
- `the ? operator can only be used in a function that returns Result`
  — wrap the body in a hidden `# fn main() -> Result<(), Box<dyn std::error::Error>> { ... # Ok(()) }`.
