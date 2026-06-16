//! Full-AST tree-sitter parsing across all enabled grammars.
//!
//! Available only under the `full-parse` feature. Ported from the working
//! reference in `crates/panproto-py/src/parse.rs` (the `PyO3`
//! `AstParserRegistry` / `ParseEmitLens` surface), with the `PyO3` result
//! classes and exception type replaced by raw handles, the canonical CBOR
//! codec, and [`FfiError`]. The engine work is driven entirely by
//! `panproto_core::parse` (`ParserRegistry`, `ParseEmitLens`,
//! `check_emit_parse`, `check_parse_emit`).
//!
//! The registry lives in the slab as a
//! [`Resource::AstRegistry`](crate::handle::Resource); parsed schemas are
//! allocated as [`Resource::Schema`](crate::handle::Resource) handles (the
//! same resource the schema and instance surfaces share), so a parsed
//! schema can be driven by any `pp_schema_*` entry point or released with
//! [`pp_handle_free`](crate::api::pp_handle_free).
//!
//! # Law-check wire format
//!
//! [`pp_parse_check_emit_parse`] and [`pp_parse_check_parse_emit`] write
//! the *empty* buffer when the law holds and the divergence text (a
//! `LawViolation` `Display` rendering, as UTF-8 bytes) when it does not.
//! This mirrors the `Option<String>` the Python surface returns (`None`
//! ⇒ holds) and the `parse` section of `crates/panproto-c/CONTRACT.md`.

use std::sync::Arc;

use panproto_core::parse::{
    ParseEmitLens, ParserRegistry, check_emit_parse, check_parse_emit,
};
use safer_ffi::prelude::*;

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Read a borrowed byte slice as UTF-8, mapping a decode failure to an
/// [`FfiError::Operation`] tagged with `what` so the divergence is
/// attributable at the boundary.
fn utf8<'a>(bytes: &'a c_slice::Ref<'_, u8>, what: &str) -> Result<&'a str, FfiError> {
    std::str::from_utf8(bytes.as_slice())
        .map_err(|e| FfiError::Operation(format!("invalid {what}: {e}")))
}

/// Construct a parser registry populated with all enabled grammars.
///
/// On success, `out_handle` receives a fresh
/// [`Resource::AstRegistry`](crate::handle::Resource) handle wrapping a
/// `ParserRegistry::new()`. The set of registered grammars is fixed by the
/// crate's compiled-in grammar group features (the default `group-core`,
/// or whatever the dependent build enables).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_registry_new(out_handle: &mut u32) -> i32 {
    guard(|| {
        let registry = ParserRegistry::new();
        *out_handle = handle::alloc(Resource::AstRegistry(Box::new(registry)));
        Ok(PpStatus::Ok)
    })
}

/// Parse a source file into a full-AST schema, language auto-detected
/// from the path.
///
/// `registry` is an AST-registry handle; `path` is the UTF-8 file path
/// (used for extension detection); `content` is the source bytes. On
/// success, `out_handle` receives a fresh
/// [`Resource::Schema`](crate::handle::Resource) handle.
///
/// An unrecognised extension, an unparseable source, or a non-UTF-8 path
/// surfaces as [`PpStatus::Operation`]; a type-mismatched `registry`
/// handle as [`PpStatus::TypeMismatch`]. The out-handle slot is written
/// only on success.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_file(
    registry: u32,
    path: c_slice::Ref<'_, u8>,
    content: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let path_str = utf8(&path, "path")?;
        let schema = handle::with_resource(registry, |r| {
            r.as_ast_registry()?
                .parse_file(std::path::Path::new(path_str), content.as_slice())
                .map_err(|e| FfiError::Operation(e.to_string()))
        })?;
        *out_handle = handle::alloc(Resource::Schema(Arc::new(schema)));
        Ok(PpStatus::Ok)
    })
}

/// Parse source code with an explicit protocol name.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `content` is the source bytes; `file_path` is the
/// UTF-8 path recorded on the parsed schema. On success, `out_handle`
/// receives a fresh [`Resource::Schema`](crate::handle::Resource)
/// handle.
///
/// An unregistered protocol, an unparseable source, or non-UTF-8
/// `protocol` / `file_path` surfaces as [`PpStatus::Operation`]. The
/// out-handle slot is written only on success.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_with_protocol(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    content: c_slice::Ref<'_, u8>,
    file_path: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
) -> i32 {
    guard(|| {
        let protocol_str = utf8(&protocol, "protocol")?;
        let file_path_str = utf8(&file_path, "file path")?;
        let schema = handle::with_resource(registry, |r| {
            r.as_ast_registry()?
                .parse_with_protocol(protocol_str, content.as_slice(), file_path_str)
                .map_err(|e| FfiError::Operation(e.to_string()))
        })?;
        *out_handle = handle::alloc(Resource::Schema(Arc::new(schema)));
        Ok(PpStatus::Ok)
    })
}

/// Detect the language protocol for a file path.
///
/// `registry` is an AST-registry handle; `path` is the UTF-8 file path.
/// On success, `out` receives the detected protocol name as UTF-8 bytes,
/// or the empty buffer when no grammar claims the extension (mirroring
/// the `Option<&str>` the core `detect_language` returns).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_detect_language(
    registry: u32,
    path: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let path_str = utf8(&path, "path")?;
        let detected = handle::with_resource(registry, |r| {
            Ok(r.as_ast_registry()?
                .detect_language(std::path::Path::new(path_str))
                .map(str::to_owned))
        })?;
        *out = detected.unwrap_or_default().into_bytes().into();
        Ok(PpStatus::Ok)
    })
}

/// Emit a schema back to source bytes via the parse-derived layout.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `schema` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives the source bytes. Calls `emit_with_protocol`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_emit(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    schema: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let protocol_str = utf8(&protocol, "protocol")?;
        let bytes = handle::with_two_resources(registry, schema, |reg, sch| {
            reg.as_ast_registry()?
                .emit_with_protocol(protocol_str, sch.as_schema()?)
                .map_err(|e| FfiError::Operation(e.to_string()))
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// Render a by-construction schema to source bytes via the grammar's
/// production walker.
///
/// Arguments match [`pp_parse_emit`]; unlike that entry point, the
/// schema need not carry parse-derived byte positions. Calls
/// `emit_pretty_with_protocol`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_emit_pretty(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    schema: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let protocol_str = utf8(&protocol, "protocol")?;
        let bytes = handle::with_two_resources(registry, schema, |reg, sch| {
            reg.as_ast_registry()?
                .emit_pretty_with_protocol(protocol_str, sch.as_schema()?)
                .map_err(|e| FfiError::Operation(e.to_string()))
        })?;
        *out = bytes.into();
        Ok(PpStatus::Ok)
    })
}

/// List all protocol names registered in an AST registry.
///
/// `registry` is an AST-registry handle. On success, `out` receives a
/// CBOR-encoded `Vec<String>` (sorted for a deterministic wire image, as
/// the underlying `protocol_names` iterates an unordered map).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_protocol_names(registry: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let mut names = handle::with_resource(registry, |r| {
            Ok(r.as_ast_registry()?
                .protocol_names()
                .map(str::to_owned)
                .collect::<Vec<String>>())
        })?;
        names.sort_unstable();
        *out = crate::canonical::encode(&names)?.into();
        Ok(PpStatus::Ok)
    })
}

/// List all available tree-sitter grammar languages enabled by feature
/// flags.
///
/// On success, `out` receives a CBOR-encoded `Vec<String>` (sorted). The
/// catalogue is registry-independent: it is read off a throwaway
/// `ParserRegistry::new()`, which `panproto-parse` populates from
/// `panproto_grammars::grammars()` (the set fixed by the compiled-in
/// grammar group features). Reading it through a fresh registry keeps the
/// list exactly in step with what `pp_parse_registry_new` would register,
/// without `panproto-c` needing a direct `panproto-grammars` dependency.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_available_grammars(out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let registry = ParserRegistry::new();
        let mut names: Vec<String> = registry.protocol_names().map(str::to_owned).collect();
        names.sort_unstable();
        *out = crate::canonical::encode(&names)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Verify the `EmitParse` retraction on a schema.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `schema` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives the empty buffer when the law holds, or the
/// divergence message bytes otherwise. Calls `check_emit_parse` against a
/// `ParseEmitLens` bound to `protocol`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_check_emit_parse(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    schema: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let protocol_str = utf8(&protocol, "protocol")?;
        let divergence = handle::with_two_resources(registry, schema, |reg, sch| {
            let registry = reg.as_ast_registry()?;
            let lens = ParseEmitLens::new(registry, protocol_str.to_owned());
            Ok(check_emit_parse(&lens, sch.as_schema()?)
                .err()
                .map(|e| e.to_string()))
        })?;
        *out = divergence.unwrap_or_default().into_bytes().into();
        Ok(PpStatus::Ok)
    })
}

/// Verify the `ParseEmit` stability law on source bytes.
///
/// `registry` is an AST-registry handle; `protocol` is the UTF-8
/// protocol name; `bytes` is the source to round-trip. On success,
/// `out` receives the empty buffer when the law holds, or the
/// divergence message bytes otherwise. Calls `check_parse_emit` against a
/// `ParseEmitLens` bound to `protocol`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_parse_check_parse_emit(
    registry: u32,
    protocol: c_slice::Ref<'_, u8>,
    bytes: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let protocol_str = utf8(&protocol, "protocol")?;
        let divergence = handle::with_resource(registry, |reg| {
            let registry = reg.as_ast_registry()?;
            let lens = ParseEmitLens::new(registry, protocol_str.to_owned());
            Ok(check_parse_emit(&lens, bytes.as_slice())
                .err()
                .map(|e| e.to_string()))
        })?;
        *out = divergence.unwrap_or_default().into_bytes().into();
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use safer_ffi::prelude::c_slice;

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::decode;

    /// A small Go program. Go is in the default `group-core` grammar
    /// group, so it is registered under `cargo build --features
    /// full-parse` without any extra grammar-group feature.
    const GO_SOURCE: &[u8] = b"package main\n\nfunc main() {\n\tprintln(\"hi\")\n}\n";

    fn slice_box(bytes: &[u8]) -> c_slice::Box<u8> {
        let boxed: Box<[u8]> = bytes.to_vec().into_boxed_slice();
        boxed.into()
    }

    /// Copy a populated out-buffer's bytes into an owned `Vec<u8>`. The
    /// `repr_c::Vec<u8>` deref does not surface a stable slice accessor,
    /// so tests round-trip through `to_vec` (the same convention the
    /// other domains' tests use).
    fn bytes(out: &repr_c::Vec<u8>) -> Vec<u8> {
        out.to_vec()
    }

    /// Allocate a fresh registry handle, asserting success.
    fn new_registry() -> u32 {
        let mut h: u32 = u32::MAX;
        assert_eq!(pp_parse_registry_new(&mut h), PpStatus::Ok as i32);
        assert_ne!(h, u32::MAX);
        h
    }

    #[test]
    fn registry_new_allocates_ast_registry() {
        let reg = new_registry();
        // The handle is a usable AstRegistry: a projection succeeds.
        let ok = handle::with_resource(reg, |r| r.as_ast_registry().map(|_| ()));
        assert!(ok.is_ok());
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn detect_language_matches_extension() {
        let reg = new_registry();
        let path = slice_box(b"main.go");
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_detect_language(reg, path.as_ref(), &mut out),
            PpStatus::Ok as i32
        );
        assert_eq!(bytes(&out), b"go", "expected go for .go extension");
        pp_buf_free(out);

        // An unrecognised extension yields the empty buffer, not an error.
        let unknown = slice_box(b"x.no_such_ext_xyz");
        let mut out2: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_detect_language(reg, unknown.as_ref(), &mut out2),
            PpStatus::Ok as i32
        );
        assert!(bytes(&out2).is_empty(), "expected empty for unknown ext");
        pp_buf_free(out2);

        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn parse_file_then_emit_round_trips() {
        let reg = new_registry();

        // Parse a Go file (extension-detected) into a Schema handle.
        let path = slice_box(b"main.go");
        let content = slice_box(GO_SOURCE);
        let mut schema_h: u32 = u32::MAX;
        assert_eq!(
            pp_parse_file(reg, path.as_ref(), content.as_ref(), &mut schema_h),
            PpStatus::Ok as i32
        );
        assert_ne!(schema_h, u32::MAX);

        // Emit it back via the parse-derived layout: byte-identical for a
        // parsed (layout-carrying) schema.
        let protocol = slice_box(b"go");
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_emit(reg, protocol.as_ref(), schema_h, &mut out),
            PpStatus::Ok as i32
        );
        assert_eq!(
            bytes(&out),
            GO_SOURCE,
            "emit(parse(go)) should reproduce the source bytes"
        );
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn parse_with_protocol_matches_parse_file() {
        let reg = new_registry();
        let protocol = slice_box(b"go");
        let content = slice_box(GO_SOURCE);
        let file_path = slice_box(b"main.go");
        let mut schema_h: u32 = u32::MAX;
        assert_eq!(
            pp_parse_with_protocol(
                reg,
                protocol.as_ref(),
                content.as_ref(),
                file_path.as_ref(),
                &mut schema_h,
            ),
            PpStatus::Ok as i32
        );
        assert_ne!(schema_h, u32::MAX);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn emit_pretty_round_trips_through_parse() {
        let reg = new_registry();
        let path = slice_box(b"main.go");
        let content = slice_box(GO_SOURCE);
        let mut first_schema: u32 = u32::MAX;
        assert_eq!(
            pp_parse_file(reg, path.as_ref(), content.as_ref(), &mut first_schema),
            PpStatus::Ok as i32
        );

        let protocol = slice_box(b"go");
        let mut pretty: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_emit_pretty(reg, protocol.as_ref(), first_schema, &mut pretty),
            PpStatus::Ok as i32
        );
        // The by-construction render must itself re-parse and re-emit
        // identically (the fixed-point law the registry verifies).
        let pretty_bytes = bytes(&pretty);
        assert!(!pretty_bytes.is_empty(), "emit_pretty produced bytes");

        let mut reparsed_schema: u32 = u32::MAX;
        let reparse_input = slice_box(&pretty_bytes);
        assert_eq!(
            pp_parse_with_protocol(
                reg,
                protocol.as_ref(),
                reparse_input.as_ref(),
                path.as_ref(),
                &mut reparsed_schema,
            ),
            PpStatus::Ok as i32
        );
        let mut pretty2: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_emit_pretty(reg, protocol.as_ref(), reparsed_schema, &mut pretty2),
            PpStatus::Ok as i32
        );
        assert_eq!(
            pretty_bytes,
            bytes(&pretty2),
            "emit_pretty fixed-point should hold"
        );

        pp_buf_free(pretty);
        pp_buf_free(pretty2);
        assert_eq!(pp_handle_free(first_schema), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reparsed_schema), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn protocol_names_lists_core_grammars() {
        let reg = new_registry();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_protocol_names(reg, &mut out),
            PpStatus::Ok as i32
        );
        let names: Vec<String> = decode(&out).unwrap();
        assert!(
            names.iter().any(|n| n == "go"),
            "expected go among protocol names: {names:?}"
        );
        // Sorted, deterministic.
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "protocol names should be sorted");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn available_grammars_matches_registry_names() {
        // available_grammars (registry-independent) and a fresh
        // registry's protocol_names report the same compiled-in set.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_available_grammars(&mut out),
            PpStatus::Ok as i32
        );
        let grammars: Vec<String> = decode(&out).unwrap();
        pp_buf_free(out);

        let reg = new_registry();
        let mut names_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_protocol_names(reg, &mut names_out),
            PpStatus::Ok as i32
        );
        let names: Vec<String> = decode(&names_out).unwrap();
        pp_buf_free(names_out);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);

        assert_eq!(
            grammars, names,
            "available_grammars should match a fresh registry's protocol names"
        );
        assert!(grammars.iter().any(|n| n == "go"));
    }

    #[test]
    fn check_parse_emit_holds_for_parseable_source() {
        let reg = new_registry();
        let protocol = slice_box(b"go");
        let content = slice_box(GO_SOURCE);
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_check_parse_emit(reg, protocol.as_ref(), content.as_ref(), &mut out),
            PpStatus::Ok as i32
        );
        let divergence = bytes(&out);
        assert!(
            divergence.is_empty(),
            "ParseEmit law should hold; got divergence: {:?}",
            String::from_utf8_lossy(&divergence)
        );
        pp_buf_free(out);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn check_emit_parse_holds_for_parsed_schema() {
        let reg = new_registry();
        let path = slice_box(b"main.go");
        let content = slice_box(GO_SOURCE);
        let mut schema_h: u32 = u32::MAX;
        assert_eq!(
            pp_parse_file(reg, path.as_ref(), content.as_ref(), &mut schema_h),
            PpStatus::Ok as i32
        );

        let protocol = slice_box(b"go");
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_parse_check_emit_parse(reg, protocol.as_ref(), schema_h, &mut out),
            PpStatus::Ok as i32
        );
        let divergence = bytes(&out);
        assert!(
            divergence.is_empty(),
            "EmitParse law should hold; got divergence: {:?}",
            String::from_utf8_lossy(&divergence)
        );
        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn unknown_protocol_is_an_operation_error() {
        let _ = crate::error::take_last_error();
        let reg = new_registry();
        let protocol = slice_box(b"no_such_protocol_xyz");
        let content = slice_box(GO_SOURCE);
        let file_path = slice_box(b"x.txt");
        let mut schema_h: u32 = u32::MAX;
        let status = pp_parse_with_protocol(
            reg,
            protocol.as_ref(),
            content.as_ref(),
            file_path.as_ref(),
            &mut schema_h,
        );
        assert_eq!(status, PpStatus::Operation as i32);
        // The out-handle slot is left untouched on failure.
        assert_eq!(schema_h, u32::MAX);
        let _ = crate::error::take_last_error();
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn invalid_utf8_path_is_an_operation_error() {
        let _ = crate::error::take_last_error();
        let reg = new_registry();
        let bad_path = slice_box(&[0xFF, 0xFE]);
        let content = slice_box(GO_SOURCE);
        let mut schema_h: u32 = u32::MAX;
        let status = pp_parse_file(reg, bad_path.as_ref(), content.as_ref(), &mut schema_h);
        assert_eq!(status, PpStatus::Operation as i32);
        let env = crate::error::take_last_error().expect("error stashed");
        assert!(
            env.message.contains("invalid path"),
            "unexpected message: {}",
            env.message
        );
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }

    #[test]
    fn non_registry_handle_is_a_type_mismatch() {
        let _ = crate::error::take_last_error();
        // A Schema handle is not an AstRegistry.
        let reg = new_registry();
        let path = slice_box(b"main.go");
        let content = slice_box(GO_SOURCE);
        let mut schema_h: u32 = u32::MAX;
        assert_eq!(
            pp_parse_file(reg, path.as_ref(), content.as_ref(), &mut schema_h),
            PpStatus::Ok as i32
        );

        // Feed the schema handle where a registry is expected.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_parse_protocol_names(schema_h, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);
        let _ = crate::error::take_last_error();

        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(reg), PpStatus::Ok as i32);
    }
}
