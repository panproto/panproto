#![allow(clippy::future_not_send)]
//! # panproto-xrpc
//!
//! XRPC client for cospan node VCS operations.
//!
//! Implements the `dev.cospan.node.*` XRPC endpoints for push/pull/clone
//! of panproto-vcs objects between local stores and remote cospan nodes.
//!
//! ## Endpoints
//!
//! | Method | Endpoint | Description |
//! |--------|----------|-------------|
//! | GET | `getObject` | Fetch content-addressed object (msgpack) |
//! | POST | `putObject` | Store object (auth required) |
//! | GET | `getRef` | Resolve ref to object ID |
//! | POST | `setRef` | Update ref (auth required) |
//! | GET | `listRefs` | List all refs |
//! | GET | `getHead` | Get HEAD state |
//! | POST | `negotiate` | Have/want negotiation for efficient transfer |
//! | GET | `getRepoInfo` | Repository metadata |
//!
//! ## Push flow
//!
//! 1. List local refs
//! 2. Negotiate (send local object IDs, get needed IDs)
//! 3. `putObject` for each needed object
//! 4. `setRef` for each ref
//!
//! ## Pull flow
//!
//! 1. `listRefs` on remote
//! 2. Negotiate (send local object IDs, want remote refs)
//! 3. `getObject` for each needed object, store locally
//! 4. Update local refs

/// XRPC client for cospan node operations.
pub mod client;

/// Error types for XRPC operations.
pub mod error;

pub use client::{NegotiateResult, NodeClient, PullResult, PushResult, RepoInfo};
pub use error::XrpcError;
