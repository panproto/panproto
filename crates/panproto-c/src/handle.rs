//! Thread-local slab allocator for opaque FFI handles.
//!
//! Mirrors the design of `panproto_wasm::slab` (see
//! `crates/panproto-wasm/src/slab.rs`). Resources live in a
//! `Vec<Option<Resource>>`; freed slots are reused. Handles are `u32`
//! indices.
//!
//! The C ABI never exposes the [`Resource`] enum; callers see only
//! `u32` and rely on the panproto-c API to dispatch to the right
//! variant. Today the slab tracks only [`Resource::Protocol`]; when
//! additional variants are added (for `Schema`, `Migration`, etc.),
//! the type-checked extraction helpers grow alongside, modelled on
//! the `as_*` helpers in `crates/panproto-wasm/src/slab.rs`.
//!
//! Slab state is thread-local: each OS thread sees its own resource
//! table. Tests rely on `cargo nextest`'s per-test process isolation
//! to avoid cross-test interference; running under `cargo test` (which
//! shares threads) can cause spurious failures in handle-equality
//! assertions, so the project's CI uses nextest.

use std::cell::RefCell;

use panproto_core::schema::Protocol;

use crate::error::FfiError;

/// A resource stored in the slab.
pub enum Resource {
    /// A protocol specification.
    Protocol(Protocol),
}

impl Resource {
    /// Project a [`Protocol`] reference out of a [`Resource`].
    ///
    /// Today every [`Resource`] is a [`Resource::Protocol`], so this
    /// is total. When new variants are added, change the return type
    /// to `Result<&Protocol, FfiError>` and return
    /// [`FfiError::TypeMismatch`] for non-`Protocol` variants; the
    /// `with_resource` callers in [`crate::api::protocol`] will then
    /// fail to compile, which is the intended forcing function.
    #[must_use]
    pub const fn as_protocol(&self) -> &Protocol {
        let Self::Protocol(p) = self;
        p
    }
}

thread_local! {
    static SLAB: RefCell<Vec<Option<Resource>>> = const { RefCell::new(Vec::new()) };
}

/// Allocate a resource and return its handle.
///
/// Reuses a freed slot when one is available; otherwise pushes onto
/// the end of the slab.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // u32 indices; >4B resources is unrealistic.
pub fn alloc(resource: Resource) -> u32 {
    SLAB.with_borrow_mut(|slab| {
        for (i, slot) in slab.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(resource);
                return i as u32;
            }
        }
        let handle = slab.len() as u32;
        slab.push(Some(resource));
        handle
    })
}

/// Read access to a resource by handle.
///
/// # Errors
///
/// Returns [`FfiError::InvalidHandle`] if the handle is out of range
/// or the slot has been freed. Propagates whatever error `f` returns.
pub fn with_resource<T>(
    handle: u32,
    f: impl FnOnce(&Resource) -> Result<T, FfiError>,
) -> Result<T, FfiError> {
    SLAB.with_borrow(|slab| {
        let resource = slab
            .get(handle as usize)
            .and_then(Option::as_ref)
            .ok_or(FfiError::InvalidHandle { handle })?;
        f(resource)
    })
}

/// Free a resource, marking the slot reusable.
///
/// Calling on an out-of-range or already-freed handle is a no-op
/// (double-free is safe).
pub fn free(handle: u32) {
    SLAB.with_borrow_mut(|slab| {
        let idx = handle as usize;
        if idx < slab.len() {
            slab[idx] = None;
        }
    });
}

/// Test-only: drop every resource in the slab and reset its length.
///
/// Used at the start of each unit test that asserts specific handle
/// values, so test order on shared threads cannot affect the slab's
/// observed state. Production callers do not need this; the slab
/// grows monotonically across calls.
#[cfg(test)]
pub fn reset() {
    SLAB.with_borrow_mut(Vec::clear);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn alloc_and_get_protocol() {
        reset();
        let h = alloc(Resource::Protocol(test_protocol()));
        let result = with_resource(h, |r| Ok(r.as_protocol().name.clone()));
        assert_eq!(result.unwrap(), "test");
        free(h);
    }

    #[test]
    fn free_reuses_slot() {
        reset();
        let h1 = alloc(Resource::Protocol(test_protocol()));
        free(h1);
        let h2 = alloc(Resource::Protocol(test_protocol()));
        assert_eq!(h1, h2);
        free(h2);
    }

    #[test]
    fn invalid_handle_errors() {
        reset();
        let result = with_resource(9999, |_| Ok(()));
        assert!(matches!(result, Err(FfiError::InvalidHandle { .. })));
    }

    #[test]
    fn double_free_is_safe() {
        reset();
        let h = alloc(Resource::Protocol(test_protocol()));
        free(h);
        free(h);
        assert!(with_resource(h, |_| Ok(())).is_err());
    }

    #[test]
    fn alloc_grows_then_reuses() {
        reset();
        let h0 = alloc(Resource::Protocol(test_protocol()));
        let h1 = alloc(Resource::Protocol(test_protocol()));
        let h2 = alloc(Resource::Protocol(test_protocol()));
        assert_eq!(h0, 0);
        assert_eq!(h1, 1);
        assert_eq!(h2, 2);
        // Freeing the middle slot should be reused next.
        free(h1);
        let h3 = alloc(Resource::Protocol(test_protocol()));
        assert_eq!(h3, h1);
        free(h0);
        free(h2);
        free(h3);
    }

    #[test]
    fn as_protocol_returns_inner() {
        reset();
        let r = Resource::Protocol(test_protocol());
        assert_eq!(r.as_protocol().name, "test");
    }
}
