//! C ABI for panproto.
//!
//! This crate exposes panproto's core operations behind a panic-safe,
//! `safer-ffi`-generated C ABI. It is the basis for non-Rust bindings
//! whose host runtime cannot tolerate Rust panics propagating across
//! the FFI boundary (Haskell GHC, in particular, has no agreed
//! mechanism for catching foreign unwinds).
//!
//! # Boundary protocol
//!
//! Two wire formats coexist:
//!
//! - **Hot path**: opaque `u32` handles into a thread-local slab
//!   ([`handle`]), small fixed records as `#[repr(C)]` structs.
//!   No serialization on every call.
//! - **Cold path**: CBOR via `ciborium` on the Rust side, decoded by
//!   the host language with whatever CBOR library is idiomatic
//!   (`cborg` for Haskell). Used for `Protocol` ingest, schema
//!   introspection, and structured errors.
//!
//! # Panic policy
//!
//! Every `#[ffi_export]` entry point is wrapped via [`panic::guard`]
//! in `std::panic::catch_unwind`.
//! Panics are converted to a CBOR-encoded error envelope retrievable
//! via [`api::pp_last_error_take`]. The release profile is
//! `panic = "unwind"` (NOT abort), so panics never tear down the host
//! process.

pub mod api;
pub mod canonical;
pub mod error;
pub mod handle;
pub mod panic;

#[cfg(feature = "headers")]
#[doc(hidden)]
pub fn generate_headers_to(path: &::std::path::Path) -> ::std::io::Result<()> {
    ::safer_ffi::headers::builder().to_file(path)?.generate()
}
