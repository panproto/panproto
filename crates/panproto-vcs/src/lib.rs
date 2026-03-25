//! # panproto-vcs
//!
//! Schematic version control for panproto.
//!
//! This crate implements a git-like version control system for schema
//! evolution. Schemas are content-addressed objects stored in a commit
//! DAG, with branches, merge (via colimit/pushout), and data lifting
//! through history.
//!
//! ## Architecture
//!
//! - **Object store**: [`hash`], [`object`], [`store`], [`fs_store`], [`mem_store`]
//! - **Refs + DAG**: [`refs`], [`dag`], [`blame`], [`bisect`]
//! - **Staging + commit**: [`index`], [`auto_mig`], [`status`]
//! - **Merge + rewrite**: [`merge`], [`rebase`], [`cherry_pick`], [`reset`], [`stash`]
//! - **Orchestration**: [`repo`] (composes all of the above), [`gc`]
//!
//! ## Quick Start
//!
//! ```rust
//! use panproto_vcs::{MemStore, ObjectId, Object, Store, HeadState};
//!
//! let mut store = MemStore::new();
//! assert_eq!(store.get_head().unwrap(), HeadState::Branch("main".into()));
//! ```

pub mod auto_mig;
pub mod bisect;
pub mod blame;
pub mod cherry_pick;
pub mod dag;
pub mod data_mig;
pub mod edit_mig;
pub mod error;
pub mod expr;
pub mod fs_store;
pub mod gat_validate;
pub mod gc;
pub mod hash;
pub mod index;
pub mod mem_store;
pub mod merge;
pub mod object;
pub mod rebase;
pub mod refs;
pub mod rename_detect;
pub mod repo;
pub mod reset;
pub mod stash;
pub mod status;
pub mod store;

// Re-exports for convenience.
pub use data_mig::{StaleData, detect_staleness, migrate_backward, migrate_forward};
pub use edit_mig::{decode_edit_log, encode_edit_log, incremental_migrate};
pub use error::VcsError;
pub use expr::{load_expr, store_expr};
pub use fs_store::FsStore;
pub use hash::ObjectId;
pub use index::Index;
pub use mem_store::MemStore;
pub use object::{CommitObject, ComplementObject, DataSetObject, EditLogObject, Object, TagObject};
pub use repo::{CommitOptions, Repository};
pub use store::{HeadState, ReflogEntry, Store};
