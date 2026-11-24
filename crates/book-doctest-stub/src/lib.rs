//! Stub crate re-exporting the crates the book examples reference.
//!
//! `xtask/src/bin/test-book.rs` builds this crate with
//! `cargo build -p book-doctest-stub --message-format=json` so that
//! each dependency the book examples reference produces exactly one
//! compiler artifact. Those artifacts are then handed to
//! `rustdoc --test` via `--extern` flags. The re-exports make each of
//! them a used dependency of this crate.

pub use anyhow;
pub use miette;
pub use panproto_core;
pub use panproto_gat;
pub use panproto_inst;
pub use panproto_io;
pub use panproto_lens;
pub use panproto_mig;
pub use panproto_parse;
pub use panproto_protocols;
pub use panproto_schema;
pub use serde_json;
pub use smallvec;
