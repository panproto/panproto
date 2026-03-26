//! # panproto-git
//!
//! Bidirectional git ↔ panproto-vcs translation bridge.
//!
//! Enables `git push cospan main` by translating between git repositories
//! and panproto-vcs stores. On import, git trees are parsed through
//! `panproto-project` to produce structural schemas. On export, schemas
//! are emitted back to source text via `panproto-parse` emitters.
//!
//! ## Import flow (git → panproto)
//!
//! 1. Walk git commit DAG topologically (parents before children)
//! 2. For each commit: read all files from the git tree
//! 3. Parse each file through its language parser (via `panproto-project`)
//! 4. Assemble project-level schema (coproduct)
//! 5. Store schema and create panproto-vcs commit (preserving author, timestamp, message)
//!
//! ## Export flow (panproto → git)
//!
//! 1. Load project schema from panproto-vcs commit
//! 2. Emit source files via panproto-parse emitters
//! 3. Build git tree and commit objects
//!
//! ## Functoriality
//!
//! Import preserves DAG structure: parent pointers in panproto-vcs match the
//! git DAG. Composition of imports matches import of composition:
//! `import(a ; b) = import(a) ; import(b)`.

/// Error types for git bridge operations.
pub mod error;

/// Import git repositories into panproto-vcs.
pub mod import;

/// Export panproto-vcs repositories to git.
pub mod export;

#[cfg(test)]
mod tests;

pub use error::GitBridgeError;
pub use export::ExportResult;
pub use export::export_to_git;
pub use import::{ImportResult, import_git_repo};
