# Install panproto

panproto has six user-facing distributions. Install only the command-line or language surface that the project uses.

| Surface | Page | Package |
|---|---|---|
| Command line (`schema`) | [Install the CLI](./cli.md) | `panproto-cli` (Homebrew, shell installer, cargo install) |
| Rust application | [Install the Rust SDK](./rust.md) | `panproto-core` (crates.io) |
| TypeScript / JavaScript application | [Install the TypeScript SDK](./typescript.md) | `@panproto/core` (npm) |
| Python application | [Install the Python SDK](./python.md) | `panproto` (PyPI) |
| Haskell application | [Install the Haskell SDK](./haskell.md) | `panproto` (this repository) |
| Swift application | [Install the Swift SDK](./swift.md) | `panproto` (this repository, SwiftPM) |

The CLI and SDK packages are independent; installing one does not install the others.

Every release attaches checksums, a software bill of materials, and build attestations for its binary archives. [Verify a release artifact](./verify-artifacts.md) covers checking them.

## See also

- [Reference: configuration](../../reference/configuration.md) for the `panproto.toml` manifest.
- [Tutorial: your first schema](../../tutorials/your-first-schema.md) once installed.
