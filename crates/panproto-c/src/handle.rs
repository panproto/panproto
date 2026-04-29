//! Thread-local slab allocator for opaque FFI handles.
//!
//! Mirrors the design of `panproto_wasm::slab` (see
//! `crates/panproto-wasm/src/slab.rs`). Resources live in a
//! `Vec<Option<Resource>>`; freed slots are reused. Handles are `u32`
//! indices.
//!
//! The C ABI never exposes the [`Resource`] enum; callers see only
//! `u32` and rely on the panproto-c API to dispatch to the right
//! variant. Each entry point validates the resource type at the slab
//! boundary and returns [`FfiError::TypeMismatch`] on mismatch.

use std::cell::RefCell;

use panproto_core::schema::Protocol;

use crate::error::FfiError;

/// A resource stored in the slab.
///
/// The vertical slice tracks only [`Protocol`]; further variants
/// ([`panproto_core::schema::Schema`], `Migration`, …) are added as
/// each capability class lands on the Haskell side.
pub enum Resource {
    /// A protocol specification.
    Protocol(Protocol),
}

impl Resource {
    /// Human-readable name of the resource variant. Used to populate
    /// [`FfiError::TypeMismatch`] envelopes once additional variants
    /// land. The vertical slice has only `Protocol`, so this always
    /// returns `"Protocol"` today.
    #[must_use]
    #[allow(dead_code)] // Used by future as_schema/as_migration variants.
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Protocol(_) => "Protocol",
        }
    }
}

thread_local! {
    static SLAB: RefCell<Vec<Option<Resource>>> = const { RefCell::new(Vec::new()) };
}

/// Allocate a resource and return its handle.
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
pub fn free(handle: u32) {
    SLAB.with_borrow_mut(|slab| {
        let idx = handle as usize;
        if idx < slab.len() {
            slab[idx] = None;
        }
    });
}

/// Extract a [`Protocol`] reference, returning [`FfiError::TypeMismatch`]
/// if the resource is something else.
///
/// # Errors
///
/// Returns [`FfiError::TypeMismatch`] when the variant is not
/// [`Resource::Protocol`]. With only one variant today this never
/// fires, but the signature is stable for future variants.
pub const fn as_protocol(resource: &Resource) -> Result<&Protocol, FfiError> {
    match resource {
        Resource::Protocol(p) => Ok(p),
    }
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
        let h = alloc(Resource::Protocol(test_protocol()));
        let result = with_resource(h, |r| Ok(as_protocol(r)?.name.clone()));
        assert_eq!(result.unwrap(), "test");
        free(h);
    }

    #[test]
    fn free_reuses_slot() {
        let h1 = alloc(Resource::Protocol(test_protocol()));
        free(h1);
        let h2 = alloc(Resource::Protocol(test_protocol()));
        assert_eq!(h1, h2);
        free(h2);
    }

    #[test]
    fn invalid_handle_errors() {
        let result = with_resource(9999, |_| Ok(()));
        assert!(matches!(result, Err(FfiError::InvalidHandle { .. })));
    }

    #[test]
    fn double_free_is_safe() {
        let h = alloc(Resource::Protocol(test_protocol()));
        free(h);
        free(h);
        assert!(with_resource(h, |_| Ok(())).is_err());
    }

    #[test]
    fn type_name_is_protocol() {
        let r = Resource::Protocol(test_protocol());
        assert_eq!(r.type_name(), "Protocol");
    }
}
