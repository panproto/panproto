//! Multi-file project assembly via coproduct.
//!
//! Available only under the `project` feature. Frozen-signature
//! scaffold; every entry point currently returns
//! [`PpStatus::Operation`](crate::error::PpStatus::Operation). The
//! engine-wiring pass fills in the bodies
//! against `panproto_core::project` (`ProjectBuilder`, `ProjectSchema`),
//! stored as
//! [`Resource::ProjectBuilder`](crate::handle::Resource) and
//! [`Resource::ProjectSchema`](crate::handle::Resource).

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Create an empty multi-file project builder.
///
/// On success, `out_handle` receives a fresh
/// [`Resource::ProjectBuilder`](crate::handle::Resource) handle. Will
/// call `ProjectBuilder::new`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_builder_new(out_handle: &mut u32) -> i32 {
    let _ = out_handle;
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_project_builder_new".into(),
        ))
    })
}

/// Add a single file to a project builder.
///
/// `builder` is a [`Resource::ProjectBuilder`](crate::handle::Resource)
/// handle; `path` is the UTF-8 file path; `content` is the file bytes.
/// Mutates the builder in place via
/// [`crate::handle::with_resource_mut`]. Will call
/// `ProjectBuilder::add_file`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_add_file(
    builder: u32,
    path: c_slice::Ref<'_, u8>,
    content: c_slice::Ref<'_, u8>,
) -> i32 {
    let _ = (builder, path, content);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_project_add_file".into(),
        ))
    })
}

/// Recursively add all files in a directory to a project builder.
///
/// `builder` is a project-builder handle; `path` is the UTF-8 directory
/// path. Mutates the builder in place. Will call
/// `ProjectBuilder::add_directory`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_add_directory(builder: u32, path: c_slice::Ref<'_, u8>) -> i32 {
    let _ = (builder, path);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_project_add_directory".into(),
        ))
    })
}

/// Assemble a project builder into a unified project schema.
///
/// `builder` is a project-builder handle (consumed). On success,
/// `out_handle` receives a fresh
/// [`Resource::ProjectSchema`](crate::handle::Resource) handle. Will
/// call `ProjectBuilder::build`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_build(builder: u32, out_handle: &mut u32) -> i32 {
    let _ = (builder, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_project_build".into(),
        ))
    })
}

/// Extract the unified schema from an assembled project.
///
/// `project` is a [`Resource::ProjectSchema`](crate::handle::Resource)
/// handle. On success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle for the
/// coproduct schema.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_schema_get(project: u32, out_handle: &mut u32) -> i32 {
    let _ = (project, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_project_schema_get".into(),
        ))
    })
}

/// Extract the file-to-protocol map from an assembled project.
///
/// `project` is a project-schema handle. On success, `out` receives a
/// CBOR-encoded `HashMap<String, String>` mapping file paths to the
/// protocol used to parse each.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_project_protocol_map(project: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (project, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_project_protocol_map".into(),
        ))
    })
}
