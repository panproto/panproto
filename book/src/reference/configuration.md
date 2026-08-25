# Configuration reference

The project manifest is `panproto.toml` in the project root. [`load_config`](https://docs.rs/panproto-project/latest/panproto_project/config/fn.load_config.html) returns `Ok(None)` when the file is absent and rejects malformed TOML as `ProjectError::InvalidManifest`.

## Manifest shape

```toml
[workspace]
name = "my-project"
exclude = ["target", "build", "**/*.log"]

[[package]]
name = "user-api"
path = "schemas/user"
protocol = "json-schema"

[[package]]
name = "user-events"
path = "schemas/events"
```

### `[workspace]`

| Field | Rust type | Required | Default | Meaning |
|---|---|---|---|---|
| `name` | `String` | yes | none | Workspace name. |
| `exclude` | `Vec<String>` | no | empty | Glob patterns compiled relative to the manifest directory. Invalid patterns return `ProjectError::InvalidPattern`. |

### `[[package]]`

The top-level `package` array may be omitted and then defaults to empty.

| Field | Rust type | Required | Default | Meaning |
|---|---|---|---|---|
| `name` | `String` | yes | none | Package label. The loader does not enforce uniqueness. |
| `path` | `PathBuf` | yes | none | Package root relative to the manifest directory. |
| `protocol` | `Option<String>` | no | `None` | Parser override for files below `path`. Without an override, detection uses the file path and parser registry. |

If an overridden parser rejects a file, project assembly falls back to `raw_file`. It does not retry ordinary language detection. Package paths supply protocol-prefix overrides, but do not restrict the directory walk to the declared packages.

## Generated defaults

`schema init [PATH]` always initializes `.panproto/`. When package scanning finds at least one recognized package, it also writes `panproto.toml` with detected package entries and these exclusions:

```toml
exclude = ["target", "node_modules", "__pycache__", "build", "dist", ".git"]
```

When scanning finds no package markers, `schema init` does not create a manifest. The package scanner recognizes Cargo, npm, Go, Python, Gradle, Elixir, and CMake project markers. Programmatic callers can use [`generate_config`](https://docs.rs/panproto-project/latest/panproto_project/config/fn.generate_config.html) and [`serialize_config`](https://docs.rs/panproto-project/latest/panproto_project/config/fn.serialize_config.html).

## Source

The manifest structs and defaults live in [`crates/panproto-project/src/config.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-project/src/config.rs). Project assembly applies them in [`crates/panproto-project/src/lib.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-project/src/lib.rs).

## See also

- [Define a schema from the CLI](../how-to/define-schema/cli.md)
- [Schema version control: init and commit](../how-to/schema-vcs/init-and-commit.md)
