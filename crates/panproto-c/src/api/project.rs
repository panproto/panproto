//! Multi-file project assembly via coproduct.
//!
//! Available only under the `project` feature. Ported from the working
//! reference in `crates/panproto-py/src/project.rs` (the `PyO3`
//! `ProjectBuilder` / `ProjectSchema` / `build_project` / `parse_project`
//! surface), with the `PyO3` classes replaced by slab handles and the
//! canonical CBOR codec. The assembly itself is driven by
//! `panproto_core::project::{ProjectBuilder, ProjectSchema}`.
//!
//! # Resources
//!
//! A builder is a [`Resource::ProjectBuilder`](crate::handle::Resource):
//! [`pp_project_builder_new`] allocates one, [`pp_project_add_file`] and
//! [`pp_project_add_directory`] mutate it in place (via
//! [`crate::handle::with_resource_mut`]), and [`pp_project_build`]
//! consumes it to produce a [`Resource::ProjectSchema`](crate::handle::Resource).
//! [`pp_project_schema_get`] extracts the assembled coproduct
//! [`Resource::Schema`](crate::handle::Resource) and
//! [`pp_project_protocol_map`] reads the path-to-protocol mapping.
//!
//! # Wire format
//!
//! [`pp_project_protocol_map`] emits a CBOR `HashMap<String, String>`
//! pairing each file path (rendered via `Path::display`) with the
//! protocol name it was parsed under, matching the Haskell
//! `Panproto.Project.decodeProtocolMap` decoder.

use std::collections::HashMap;

use panproto_core::project::ProjectBuilder;
use safer_ffi::prelude::*;

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Create an empty multi-file project builder.
///
/// On success, `out_handle` receives a fresh
/// [`Resource::ProjectBuilder`](crate::handle::Resource) handle wrapping
/// a `ProjectBuilder::new`. The out-handle slot is written only on
/// success.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_builder_new(out_handle: &mut u32) -> i32 {
    guard(|| {
        *out_handle = handle::alloc(Resource::ProjectBuilder(Box::new(ProjectBuilder::new())));
        Ok(PpStatus::Ok)
    })
}

/// Add a single file to a project builder.
///
/// `builder` is a [`Resource::ProjectBuilder`](crate::handle::Resource)
/// handle; `path` is the UTF-8 file path; `content` is the file bytes.
/// Mutates the builder in place via [`crate::handle::with_resource_mut`],
/// dispatching to `ProjectBuilder::add_file`.
///
/// The path is validated as UTF-8 at the boundary; a malformed path or a
/// parse failure surfaces as [`PpStatus::Operation`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_add_file(
    builder: u32,
    path: c_slice::Ref<'_, u8>,
    content: c_slice::Ref<'_, u8>,
) -> i32 {
    guard(|| {
        let path_str = std::str::from_utf8(path.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid file path: {e}")))?;
        handle::with_resource_mut(builder, |r| {
            r.as_project_builder_mut()?
                .add_file(std::path::Path::new(path_str), content.as_slice())
                .map_err(|e| FfiError::Operation(e.to_string()))
        })?;
        Ok(PpStatus::Ok)
    })
}

/// Recursively add all files in a directory to a project builder.
///
/// `builder` is a project-builder handle; `path` is the UTF-8 directory
/// path. Mutates the builder in place via
/// [`crate::handle::with_resource_mut`], dispatching to
/// `ProjectBuilder::add_directory`, which walks the directory on the
/// local filesystem (skipping hidden entries and the usual build-output
/// directories) and reads each file's bytes with `std::fs`.
///
/// The path is validated as UTF-8 at the boundary; a malformed path, an
/// unreadable directory, or a parse failure surfaces as
/// [`PpStatus::Operation`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_add_directory(builder: u32, path: c_slice::Ref<'_, u8>) -> i32 {
    guard(|| {
        let path_str = std::str::from_utf8(path.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid directory path: {e}")))?;
        handle::with_resource_mut(builder, |r| {
            r.as_project_builder_mut()?
                .add_directory(std::path::Path::new(path_str))
                .map_err(|e| FfiError::Operation(e.to_string()))
        })?;
        Ok(PpStatus::Ok)
    })
}

/// Assemble a project builder into a unified project schema.
///
/// `builder` is a project-builder handle. On success, `out_handle`
/// receives a fresh [`Resource::ProjectSchema`](crate::handle::Resource)
/// handle and the builder is logically consumed: its slab slot is left
/// holding a fresh empty builder (`ProjectBuilder::new`), so the handle
/// stays valid but carries no accumulated files. This mirrors the Python
/// reference, which swaps in a fresh builder before taking ownership for
/// `ProjectBuilder::build`.
///
/// A failed assembly (no files added, or a coproduct failure) surfaces as
/// [`PpStatus::Operation`] and leaves the out-handle slot untouched.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_build(builder: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        // `ProjectBuilder::build` consumes the builder, but the slab holds
        // it behind a mutable reference. Swap in a fresh builder to take
        // ownership of the accumulated one, exactly as the Python surface
        // does.
        let owned = handle::with_resource_mut(builder, |r| {
            let b = r.as_project_builder_mut()?;
            Ok(std::mem::take(b))
        })?;
        let project = owned
            .build()
            .map_err(|e| FfiError::Operation(e.to_string()))?;
        *out_handle = handle::alloc(Resource::ProjectSchema(Box::new(project)));
        Ok(PpStatus::Ok)
    })
}

/// Extract the unified schema from an assembled project.
///
/// `project` is a [`Resource::ProjectSchema`](crate::handle::Resource)
/// handle. On success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle for the
/// coproduct schema (cloned out of the project). The out-handle slot is
/// written only on success.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_schema_get(project: u32, out_handle: &mut u32) -> i32 {
    guard(|| {
        let schema = handle::with_resource(project, |r| Ok(r.as_project_schema()?.schema.clone()))?;
        *out_handle = handle::alloc(Resource::Schema(std::sync::Arc::new(schema)));
        Ok(PpStatus::Ok)
    })
}

/// Extract the file-to-protocol map from an assembled project.
///
/// `project` is a project-schema handle. On success, `out` receives a
/// CBOR-encoded `HashMap<String, String>` mapping file paths (rendered
/// via `Path::display`) to the protocol used to parse each, matching the
/// Haskell `Panproto.Project.decodeProtocolMap` decoder.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_protocol_map(project: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let map: HashMap<String, String> = handle::with_resource(project, |r| {
            let proj = r.as_project_schema()?;
            Ok(proj
                .protocol_map
                .iter()
                .map(|(path, protocol)| (path.display().to_string(), protocol.clone()))
                .collect())
        })?;
        *out = crate::canonical::encode(&map)?.into();
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

    use safer_ffi::prelude::c_slice;

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::decode;

    fn slice_box(bytes: &[u8]) -> c_slice::Box<u8> {
        let boxed: Box<[u8]> = bytes.to_vec().into_boxed_slice();
        boxed.into()
    }

    /// Allocate a builder, add `files` to it, and build it, returning the
    /// project-schema handle.
    fn build_from_files(files: &[(&str, &[u8])]) -> u32 {
        let mut builder_h: u32 = u32::MAX;
        assert_eq!(pp_project_builder_new(&mut builder_h), PpStatus::Ok as i32);

        for (path, content) in files {
            let path_slice = slice_box(path.as_bytes());
            let content_slice = slice_box(content);
            assert_eq!(
                pp_project_add_file(builder_h, path_slice.as_ref(), content_slice.as_ref()),
                PpStatus::Ok as i32,
                "add_file {path}",
            );
        }

        let mut project_h: u32 = u32::MAX;
        assert_eq!(
            pp_project_build(builder_h, &mut project_h),
            PpStatus::Ok as i32,
        );
        assert_eq!(pp_handle_free(builder_h), PpStatus::Ok as i32);
        project_h
    }

    #[test]
    fn builder_new_allocates_a_builder() {
        let mut h: u32 = u32::MAX;
        assert_eq!(pp_project_builder_new(&mut h), PpStatus::Ok as i32);
        assert_ne!(h, u32::MAX, "out-handle written");
        // The handle is a usable ProjectBuilder: adding a file succeeds.
        let path = slice_box(b"a.rs");
        let content = slice_box(b"pub fn a() {}");
        assert_eq!(
            pp_project_add_file(h, path.as_ref(), content.as_ref()),
            PpStatus::Ok as i32,
        );
        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }

    #[test]
    fn two_file_project_protocol_map_and_schema() {
        let project_h = build_from_files(&[
            ("a.rs", b"pub fn a() -> i32 { 1 }" as &[u8]),
            ("b.rs", b"pub fn b() -> i32 { 2 }" as &[u8]),
        ]);

        // protocol_map names both files, each mapped to a protocol.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_project_protocol_map(project_h, &mut out),
            PpStatus::Ok as i32,
        );
        let map: HashMap<String, String> = decode(&out).unwrap();
        pp_buf_free(out);
        assert_eq!(map.len(), 2, "two files expected, got {map:?}");
        assert!(map.contains_key("a.rs"), "a.rs missing from {map:?}");
        assert!(map.contains_key("b.rs"), "b.rs missing from {map:?}");
        for (path, protocol) in &map {
            assert!(!protocol.is_empty(), "empty protocol for {path}");
        }

        // schema_get yields a usable Schema handle over the coproduct.
        let mut schema_h: u32 = u32::MAX;
        assert_eq!(
            pp_project_schema_get(project_h, &mut schema_h),
            PpStatus::Ok as i32,
        );
        assert_ne!(schema_h, u32::MAX, "schema out-handle written");
        let vertex_count = handle::with_resource(schema_h, |r| Ok(r.as_schema()?.vertices.len()))
            .expect("schema handle resolves");
        assert!(vertex_count > 0, "coproduct schema should have vertices");

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(project_h), PpStatus::Ok as i32);
    }

    #[test]
    fn build_empty_project_is_an_operation_error() {
        let _ = crate::error::take_last_error();
        let mut builder_h: u32 = u32::MAX;
        assert_eq!(pp_project_builder_new(&mut builder_h), PpStatus::Ok as i32);

        let mut project_h: u32 = u32::MAX;
        let status = pp_project_build(builder_h, &mut project_h);
        assert_eq!(status, PpStatus::Operation as i32);
        // The out-handle slot is left untouched on failure.
        assert_eq!(project_h, u32::MAX);

        let env = crate::error::take_last_error().expect("error stashed");
        assert_eq!(env.tag, "operation");

        assert_eq!(pp_handle_free(builder_h), PpStatus::Ok as i32);
    }

    #[test]
    fn add_file_invalid_utf8_path_is_an_operation_error() {
        let _ = crate::error::take_last_error();
        let mut builder_h: u32 = u32::MAX;
        assert_eq!(pp_project_builder_new(&mut builder_h), PpStatus::Ok as i32);

        let path = slice_box(&[0xFF, 0xFE]); // not valid UTF-8
        let content = slice_box(b"x");
        let status = pp_project_add_file(builder_h, path.as_ref(), content.as_ref());
        assert_eq!(status, PpStatus::Operation as i32);
        let env = crate::error::take_last_error().expect("error stashed");
        assert!(
            env.message.contains("invalid file path"),
            "unexpected message: {}",
            env.message
        );

        assert_eq!(pp_handle_free(builder_h), PpStatus::Ok as i32);
    }

    #[test]
    fn protocol_map_on_non_project_handle_is_type_mismatch() {
        let _ = crate::error::take_last_error();
        // Allocate a Schema handle and feed it where a ProjectSchema is
        // expected.
        let project_h = build_from_files(&[("a.rs", b"pub fn a() {}" as &[u8])]);
        let mut schema_h: u32 = u32::MAX;
        assert_eq!(
            pp_project_schema_get(project_h, &mut schema_h),
            PpStatus::Ok as i32,
        );

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_project_protocol_map(schema_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(project_h), PpStatus::Ok as i32);
    }
}
