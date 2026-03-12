//! # panproto-check
//!
//! Breaking change detection for panproto.
//!
//! This crate analyzes schema migrations and lens definitions to
//! determine whether a proposed change is backward-compatible,
//! producing detailed diagnostics when breaking changes are found.
//!
//! The pipeline is:
//! 1. [`diff()`] two schemas to produce a [`SchemaDiff`].
//! 2. [`classify()`] the diff against a [`Protocol`](panproto_schema::Protocol) to get a [`CompatReport`].
//! 3. Render via [`report_text`] (human-readable) or [`report_json`] (machine-readable).

// Allow concrete HashMap in public API per ENGINEERING.md.
#![allow(clippy::implicit_hasher)]

pub mod classify;
pub mod diff;
pub mod error;
pub mod report;

pub use classify::{BreakingChange, CompatReport, NonBreakingChange, classify};
pub use diff::{ConstraintChange, ConstraintDiff, KindChange, SchemaDiff, diff};
pub use error::CheckError;
pub use report::{report_json, report_text};
