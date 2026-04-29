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
//!
//! Slab state is thread-local: each OS thread sees its own resource
//! table. Tests rely on `cargo nextest`'s per-test process isolation
//! to avoid cross-test interference; running under `cargo test` (which
//! shares threads) can cause spurious failures in handle-equality
//! assertions, so the project's CI uses nextest.

use std::cell::RefCell;
use std::sync::Arc;

use panproto_core::schema::{Protocol, Schema};

use crate::error::FfiError;

/// A resource stored in the slab.
///
/// Schemas are stored behind `Arc` so that downstream operations
/// that need both the source and target schema (lens `put`, schema
/// diff) can share ownership without deep-cloning. Protocols are
/// boxed for the same reason that motivates `Box<T>` for any large
/// enum payload: the slab vector should not pay the worst-case
/// variant size on every slot.
pub enum Resource {
    /// A protocol specification.
    Protocol(Box<Protocol>),
    /// A built schema, behind an `Arc` for cheap clones.
    Schema(Arc<Schema>),
}

impl Resource {
    /// Project a [`Protocol`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::Protocol`].
    pub fn as_protocol(&self) -> Result<&Protocol, FfiError> {
        match self {
            Self::Protocol(p) => Ok(p),
            Self::Schema(_) => Err(FfiError::TypeMismatch {
                expected: "Protocol",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a [`Schema`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::Schema`].
    pub fn as_schema(&self) -> Result<&Schema, FfiError> {
        match self {
            Self::Schema(s) => Ok(s),
            Self::Protocol(_) => Err(FfiError::TypeMismatch {
                expected: "Schema",
                actual: self.type_name(),
            }),
        }
    }

    /// Human-readable variant name. Populates `TypeMismatch` envelopes
    /// and is exposed for diagnostic logging from the C ABI.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Protocol(_) => "Protocol",
            Self::Schema(_) => "Schema",
        }
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

/// Read access to two resources by handle, in a single slab borrow.
///
/// # Errors
///
/// Returns [`FfiError::InvalidHandle`] if either handle is out of
/// range or freed. Propagates whatever error `f` returns.
pub fn with_two_resources<T>(
    h1: u32,
    h2: u32,
    f: impl FnOnce(&Resource, &Resource) -> Result<T, FfiError>,
) -> Result<T, FfiError> {
    SLAB.with_borrow(|slab| {
        let r1 = slab
            .get(h1 as usize)
            .and_then(Option::as_ref)
            .ok_or(FfiError::InvalidHandle { handle: h1 })?;
        let r2 = slab
            .get(h2 as usize)
            .and_then(Option::as_ref)
            .ok_or(FfiError::InvalidHandle { handle: h2 })?;
        f(r1, r2)
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
#[cfg(test)]
pub fn reset() {
    SLAB.with_borrow_mut(Vec::clear);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

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

    fn test_schema() -> Schema {
        Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: vec![],
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn alloc_and_get_protocol() {
        reset();
        let h = alloc(Resource::Protocol(Box::new(test_protocol())));
        let result = with_resource(h, |r| Ok(r.as_protocol()?.name.clone()));
        assert_eq!(result.unwrap(), "test");
        free(h);
    }

    #[test]
    fn alloc_and_get_schema() {
        reset();
        let h = alloc(Resource::Schema(Arc::new(test_schema())));
        let result = with_resource(h, |r| Ok(r.as_schema()?.protocol.clone()));
        assert_eq!(result.unwrap(), "test");
        free(h);
    }

    #[test]
    fn protocol_handle_rejected_as_schema() {
        reset();
        let h = alloc(Resource::Protocol(Box::new(test_protocol())));
        let result = with_resource(h, |r| {
            let _ = r.as_schema()?;
            Ok(())
        });
        match result {
            Err(FfiError::TypeMismatch { expected, actual }) => {
                assert_eq!(expected, "Schema");
                assert_eq!(actual, "Protocol");
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
        free(h);
    }

    #[test]
    fn schema_handle_rejected_as_protocol() {
        reset();
        let h = alloc(Resource::Schema(Arc::new(test_schema())));
        let result = with_resource(h, |r| {
            let _ = r.as_protocol()?;
            Ok(())
        });
        match result {
            Err(FfiError::TypeMismatch { expected, actual }) => {
                assert_eq!(expected, "Protocol");
                assert_eq!(actual, "Schema");
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
        free(h);
    }

    #[test]
    fn free_reuses_slot() {
        reset();
        let h1 = alloc(Resource::Protocol(Box::new(test_protocol())));
        free(h1);
        let h2 = alloc(Resource::Protocol(Box::new(test_protocol())));
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
        let h = alloc(Resource::Protocol(Box::new(test_protocol())));
        free(h);
        free(h);
        assert!(with_resource(h, |_| Ok(())).is_err());
    }

    #[test]
    fn alloc_grows_then_reuses() {
        reset();
        let h0 = alloc(Resource::Protocol(Box::new(test_protocol())));
        let h1 = alloc(Resource::Protocol(Box::new(test_protocol())));
        let h2 = alloc(Resource::Protocol(Box::new(test_protocol())));
        assert_eq!(h0, 0);
        assert_eq!(h1, 1);
        assert_eq!(h2, 2);
        free(h1);
        let h3 = alloc(Resource::Protocol(Box::new(test_protocol())));
        assert_eq!(h3, h1);
        free(h0);
        free(h2);
        free(h3);
    }

    #[test]
    fn type_name_is_correct() {
        let p = Resource::Protocol(Box::new(test_protocol()));
        assert_eq!(p.type_name(), "Protocol");
        let s = Resource::Schema(Arc::new(test_schema()));
        assert_eq!(s.type_name(), "Schema");
    }

    #[test]
    fn with_two_resources_works() {
        reset();
        let h1 = alloc(Resource::Protocol(Box::new(test_protocol())));
        let h2 = alloc(Resource::Schema(Arc::new(test_schema())));
        let result = with_two_resources(h1, h2, |r1, r2| {
            let _ = r1.as_protocol()?;
            let _ = r2.as_schema()?;
            Ok(())
        });
        assert!(result.is_ok());
        free(h1);
        free(h2);
    }

    #[test]
    fn with_two_resources_invalid_handle() {
        reset();
        let h = alloc(Resource::Protocol(Box::new(test_protocol())));
        let result = with_two_resources(h, 9999, |_, _| Ok(()));
        assert!(matches!(result, Err(FfiError::InvalidHandle { .. })));
        free(h);
    }
}
