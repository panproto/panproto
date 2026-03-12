//! # panproto-wasm
//!
//! WASM bindings for panproto.
//!
//! This crate exposes the panproto API to JavaScript and TypeScript
//! consumers via `wasm-bindgen`, using a handle-based API with
//! `MessagePack` serialization for crossing the WASM boundary.
//!
//! ## Architecture
//!
//! Opaque `u32` handles reference resources stored in a thread-local
//! slab allocator. Data crosses the boundary as `MessagePack` byte
//! slices (`&[u8]` / `Vec<u8>`), never as JS objects. This avoids
//! the serialization overhead of `serde-wasm-bindgen` for structured
//! data while keeping the JS API clean.
//!
//! ## Entry Points
//!
//! Nine `#[wasm_bindgen]` functions cover the full migration lifecycle:
//!
//! 1. [`define_protocol`] -- register a protocol specification
//! 2. [`build_schema`] -- build a schema from a protocol handle + ops
//! 3. [`check_existence`] -- validate a migration mapping
//! 4. [`compile_migration`] -- compile a migration for fast application
//! 5. [`lift_record`] -- apply a compiled migration to a record
//! 6. [`put_record`] -- restore a record from view + complement
//! 7. [`compose_migrations`] -- compose two compiled migrations
//! 8. [`diff_schemas`] -- diff two schemas
//! 9. [`free_handle`] -- release a resource handle

mod api;
mod error;
mod slab;

pub use api::{
    build_schema, check_existence, compile_migration, compose_migrations, define_protocol,
    diff_schemas, free_handle, lift_record, put_record,
};
