# Configuration reference

The project manifest is `panproto.toml` at the project root. It is read by `panproto-project::load_config` and consumed by every CLI subcommand that operates on a project rather than a single file.

## Top-level shape

```toml
[workspace]
name = "my-project"
exclude = ["build/**", "**/.cache"]

[[package]]
name = "user-api"
path = "schemas/user"
protocol = "json-schema"

[[package]]
name = "user-events"
path = "schemas/events"
protocol = "avro"
```

## Sections

### `[workspace]`

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Workspace identifier; used for VCS commits and emitted artifacts. |
| `exclude` | list of glob patterns | no | Globs to exclude from project assembly. Evaluated relative to the manifest directory. |

### `[[package]]`

One entry per schema package. Packages are assembled into a single project by schema coproduct.

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Package identifier, unique within the workspace. |
| `path` | path | yes | Directory containing the package's schemas, relative to the manifest. |
| `protocol` | string | no | Protocol name (`json-schema`, `atproto`, `protobuf`, ...). When omitted, the protocol is inferred from file extensions and contents. |

## Generating a manifest

`schema init` creates a default manifest. To regenerate one in place, use:

```sh
schema init --name my-project
```

The CLI also exposes `panproto_project::generate_config` and `serialize_config` for programmatic creation.

## Authoritative source

Manifest schema: [`crates/panproto-project/src/config.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-project/src/config.rs).

## See also

- [Define a schema from the CLI](../how-to/define-schema/cli.md) for the project-level workflow.
- [Schema version control: init and commit](../how-to/schema-vcs/init-and-commit.md).
