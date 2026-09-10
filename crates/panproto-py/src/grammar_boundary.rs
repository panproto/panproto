//! The one place a native grammar crosses into this process.
//!
//! A companion grammar pack is a separate cdylib. Everything it hands
//! over, the `TSLanguage *` and the `node-types.json`, `tags.scm` and
//! `grammar.json` payloads, arrives as an address, and an address is
//! not proof of anything: a null pointer, a pointer into a library that
//! has since been unloaded, or a length that outruns its allocation are
//! all undefined behaviour rather than errors, and none of them can be
//! checked from an integer.
//!
//! Two things narrow that.
//!
//! A **capsule** is preferred over a bare integer. `PyCapsule` carries
//! a name that this module checks, and it keeps the object that
//! provided it alive for as long as the capsule lives, so a pointer
//! obtained this way cannot outlive the library that owns it. The
//! integer form still works, because ten companion packages are already
//! published using it, but it is the explicitly unsafe path and says so.
//!
//! Every payload is **copied** on the way in rather than aliased. The
//! registry used to hold `&'static` slices pointing into a companion's
//! static memory, produced by `Box::leak` and by widening raw pointers,
//! which made the registry's soundness depend on a library staying
//! loaded for the process's life. Copying costs a few hundred kilobytes
//! once per grammar and makes the lifetime question disappear: the
//! registry owns what it holds.

use std::ffi::CStr;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyCapsuleMethods};

/// The name a companion's language capsule must carry.
///
/// Checked rather than assumed: a capsule from somewhere else, holding
/// a pointer to something else, would otherwise be accepted and
/// dereferenced as a `TSLanguage`.
pub const LANGUAGE_CAPSULE_NAME: &CStr = c"panproto.tree_sitter_language";

/// Reconstruct a `tree_sitter::Language` from an address.
///
/// # Safety
///
/// `address` must be a non-null `TSLanguage *` returned by a
/// `tree_sitter_<name>()` function in a library that stays loaded for
/// as long as any parser built from the result is used. Nothing here
/// can check that; it is the caller's obligation, and it is why the
/// capsule form exists.
unsafe fn language_from_address(address: usize) -> tree_sitter::Language {
    // SAFETY: `tree_sitter::Language` is a transparent wrapper around a
    // `*const TSLanguage`, so this reinterprets an address as the
    // pointer it already is. The caller guarantees the address came
    // from `tree_sitter_<name>()` and that its library outlives every
    // parser built from it. Null is rejected before this is reached.
    unsafe { std::mem::transmute::<usize, tree_sitter::Language>(address) }
}

/// Copy `len` bytes from `address`.
///
/// # Safety
///
/// `address` must point to at least `len` initialised bytes that stay
/// valid for the duration of this call. The copy is taken immediately,
/// so the source need not outlive it, which is the whole reason for
/// copying rather than borrowing.
unsafe fn copy_payload(address: usize, len: usize) -> Vec<u8> {
    if address == 0 || len == 0 {
        return Vec::new();
    }
    // SAFETY: the caller guarantees `len` initialised bytes live at
    // `address` for this call. The slice is consumed into an owned
    // `Vec` before returning, so no reference escapes.
    unsafe { std::slice::from_raw_parts(address as *const u8, len) }.to_vec()
}

/// Read a language from a capsule, checking its name.
///
/// This is the path a companion should use. The capsule keeps whatever
/// provided it alive, so the pointer cannot outlive its library.
///
/// # Errors
///
/// Returns `ValueError` if the object is not a capsule, carries the
/// wrong name, or holds a null pointer.
pub fn language_from_capsule(
    name: &str,
    object: &Bound<'_, PyAny>,
) -> PyResult<tree_sitter::Language> {
    let capsule = object.cast::<PyCapsule>().map_err(|_| {
        PyValueError::new_err(format!(
            "grammar {name:?}: language_capsule is not a PyCapsule"
        ))
    })?;
    let pointer = capsule
        .pointer_checked(Some(LANGUAGE_CAPSULE_NAME))
        .map_err(|e| {
            PyValueError::new_err(format!(
                "grammar {name:?}: language_capsule is not a valid {} capsule ({e})",
                LANGUAGE_CAPSULE_NAME.to_string_lossy(),
            ))
        })?;
    // SAFETY: the capsule carried this module's own name, which only a
    // panproto companion sets, and a capsule keeps its provider alive.
    // `pointer_checked` returns a `NonNull`, so the null case is gone.
    Ok(unsafe { language_from_address(pointer.as_ptr() as usize) })
}

/// Read a language from a bare address.
///
/// The explicitly unsafe path, kept because published companion
/// packages use it. Prefer [`language_from_capsule`].
///
/// # Errors
///
/// Returns `ValueError` if `address` is null. Nothing else about it can
/// be checked.
pub fn language_from_raw_address(name: &str, address: usize) -> PyResult<tree_sitter::Language> {
    if address == 0 {
        return Err(PyValueError::new_err(format!(
            "grammar {name:?}: language_ptr is null"
        )));
    }
    // SAFETY: not guaranteed here, and cannot be. The caller asserts by
    // using this entry point that `address` is a live `TSLanguage *`
    // whose library outlives every parser built from it. The capsule
    // form exists so this assertion need not be made.
    Ok(unsafe { language_from_address(address) })
}

/// Copy a grammar's byte payload out of the companion's memory.
///
/// # Errors
///
/// Returns `ValueError` when a non-empty length is paired with a null
/// address, which would otherwise be a read through null.
pub fn payload(name: &str, field: &str, address: usize, len: usize) -> PyResult<Vec<u8>> {
    if len != 0 && address == 0 {
        return Err(PyValueError::new_err(format!(
            "grammar {name:?}: {field} has a null pointer with length {len}"
        )));
    }
    // SAFETY: `len` is the companion's own length for its own static
    // payload, and the copy is taken now, so the source is only
    // required to be valid for this call.
    Ok(unsafe { copy_payload(address, len) })
}

/// Copy a grammar's text payload, checking it is UTF-8.
///
/// # Errors
///
/// Returns `ValueError` for a null pointer with a non-zero length, or
/// for bytes that are not UTF-8. The check happens before the text
/// reaches tree-sitter's query compiler, which would read a `str`
/// violating its invariant.
pub fn text_payload(
    name: &str,
    field: &str,
    address: usize,
    len: usize,
) -> PyResult<Option<String>> {
    if len == 0 {
        return Ok(None);
    }
    let bytes = payload(name, field, address, len)?;
    String::from_utf8(bytes).map(Some).map_err(|e| {
        PyValueError::new_err(format!(
            "grammar {name:?}: {field} is not valid UTF-8 ({e})"
        ))
    })
}
