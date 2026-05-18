//! Empty stub crate.
//!
//! `xtask/src/bin/test-book.rs` builds this crate with
//! `cargo build -p book-doctest-stub --message-format=json` so that
//! each dependency the book examples reference produces exactly one
//! compiler artifact in `target/debug/deps`. Those artifacts are
//! then handed to `rustdoc --test` via `--extern` flags.
//!
//! This crate has no code; its job is to be a single anchor for the
//! dep set the book needs.
