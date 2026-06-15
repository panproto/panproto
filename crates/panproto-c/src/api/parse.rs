//! Full-AST tree-sitter parsing across all enabled grammars.
//!
//! Available only under the `full-parse` feature. Frozen-signature
//! scaffold; every entry point currently returns
//! [`PpStatus::Operation`](crate::error::PpStatus::Operation). The
//! engine-wiring pass fills in the bodies
//! against `panproto_core::parse` (`ParserRegistry`, `ParseEmitLens`),
//! storing the registry as a
//! [`Resource::AstRegistry`](crate::handle::Resource).

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Construct a parser registry populated with all enabled grammars.
///
/// On success, `out_handle` receives a fresh
/// [`Resource::AstRegistry`](crate::handle::Resource) handle. Will call
/// `ParserRegistry::new`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_registry_new(out_handle: &mut u32) -> i32 {
    let _ = out_handle;
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_registry_new".into(),
        ))
    })
}

/// Parse a source file into a full-AST schema, language auto-detected
/// from the path.
///
/// `registry` is an AST-registry handle; `path` is the UTF-8 file path
/// (used for extension detection); `content` is the source bytes. On
/// success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle. Will call
/// `ParserRegistry::parse_file`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_file(
    registry: u32,
    path: c_slice::Ref<'_, u8>,
    content: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (registry, path, content, out_handle);
    guard(|| Err(FfiError::Operation("unimplemented: pp_parse_file".into())))
}

/// Parse source code with an explicit protocol name.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `content` is the source bytes; `file_path` is the
/// UTF-8 path used for diagnostics. On success, `out_handle` receives a
/// fresh [`Resource::Schema`](crate::handle::Resource) handle. Will call
/// `ParserRegistry::parse_with_protocol`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_with_protocol(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    content: c_slice::Ref<'_, u8>,
    file_path: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    let _ = (registry, protocol, content, file_path, out_handle);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_with_protocol".into(),
        ))
    })
}

/// Detect the language protocol for a file path.
///
/// `registry` is an AST-registry handle; `path` is the UTF-8 file path.
/// On success, `out` receives the detected protocol name as UTF-8 bytes
/// (empty when no grammar matches the extension). Will call
/// `ParserRegistry::detect_language`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_detect_language(
    registry: u32,
    path: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (registry, path, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_detect_language".into(),
        ))
    })
}

/// Emit a schema back to source bytes via the parse-derived layout.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `schema` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives the source bytes. Will call
/// `ParserRegistry::emit_with_protocol`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_emit(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    schema: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (registry, protocol, schema, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_parse_emit".into())))
}

/// Render a by-construction schema to source bytes via the grammar's
/// production walker.
///
/// Arguments match [`pp_parse_emit`]; unlike that entry point, the
/// schema need not carry parse-derived byte positions. Will call
/// `ParserRegistry::emit_pretty_with_protocol`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_emit_pretty(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    schema: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (registry, protocol, schema, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_emit_pretty".into(),
        ))
    })
}

/// List all protocol names registered in an AST registry.
///
/// `registry` is an AST-registry handle. On success, `out` receives a
/// CBOR-encoded `Vec<String>`. Will call
/// `ParserRegistry::protocol_names`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_protocol_names(registry: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (registry, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_protocol_names".into(),
        ))
    })
}

/// List all available tree-sitter grammar languages enabled by feature
/// flags.
///
/// On success, `out` receives a CBOR-encoded `Vec<String>`. Will call
/// `panproto_grammars::grammars`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_available_grammars(out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = out;
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_available_grammars".into(),
        ))
    })
}

/// Verify the `EmitParse` retraction on a schema.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `schema` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives the empty buffer when the law holds, or the
/// divergence message bytes otherwise. Will call
/// `panproto_parse::check_emit_parse`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_check_emit_parse(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    schema: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (registry, protocol, schema, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_check_emit_parse".into(),
        ))
    })
}

/// Verify the `ParseEmit` stability law on source bytes.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `bytes` is the source to round-trip. On success,
/// `out` receives the empty buffer when the law holds, or the
/// divergence message bytes otherwise. Will call
/// `panproto_parse::check_parse_emit`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_check_parse_emit(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    bytes: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (registry, protocol, bytes, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_parse_check_parse_emit".into(),
        ))
    })
}
