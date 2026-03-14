//! Thread-local slab allocator with typed resource handles.
//!
//! Resources (protocols, schemas, compiled migrations) are stored in a
//! thread-local `Vec<Option<Resource>>`. Handles are `u32` indices into
//! this vector. Freed slots are reused on subsequent allocations.

use std::cell::RefCell;

use panproto_core::inst::CompiledMigration;
use panproto_core::schema::{Protocol, Schema};
use wasm_bindgen::JsError;

use crate::error::WasmError;

/// A resource stored in the slab.
pub enum Resource {
    /// A protocol specification.
    Protocol(Protocol),
    /// A built schema.
    Schema(Box<Schema>),
    /// A compiled migration ready for per-record application.
    Migration(CompiledMigration),
    /// A compiled migration bundled with its source and target schemas,
    /// needed for lens put operations and accurate schema reconstruction.
    MigrationWithSchemas {
        /// The compiled migration.
        compiled: CompiledMigration,
        /// The source schema (pre-migration).
        src_schema: Box<Schema>,
        /// The target schema (post-migration).
        tgt_schema: Box<Schema>,
    },
}

thread_local! {
    static SLAB: RefCell<Vec<Option<Resource>>> = const { RefCell::new(Vec::new()) };
}

/// Allocate a resource in the slab and return its handle.
#[allow(clippy::cast_possible_truncation)] // Handles are u32; exceeding 4B resources is not realistic.
pub fn alloc(resource: Resource) -> u32 {
    SLAB.with_borrow_mut(|slab| {
        // Try to reuse a freed slot.
        for (i, slot) in slab.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(resource);
                return i as u32;
            }
        }
        // No free slot; push a new one.
        let handle = slab.len() as u32;
        slab.push(Some(resource));
        handle
    })
}

/// Access a resource by handle, returning an error if the handle is
/// invalid or the slot is empty.
///
/// The callback `f` receives a reference to the resource. The borrow
/// is released when the callback returns, so the reference must not
/// escape.
pub fn with_resource<T>(
    handle: u32,
    f: impl FnOnce(&Resource) -> Result<T, WasmError>,
) -> Result<T, JsError> {
    SLAB.with_borrow(|slab| {
        let idx = handle as usize;
        let resource = slab
            .get(idx)
            .and_then(Option::as_ref)
            .ok_or(WasmError::InvalidHandle { handle })?;
        f(resource).map_err(Into::into)
    })
}

/// Access two resources by handle simultaneously.
///
/// # Errors
///
/// Returns `JsError` if either handle is invalid or freed.
pub fn with_two_resources<T>(
    h1: u32,
    h2: u32,
    f: impl FnOnce(&Resource, &Resource) -> Result<T, WasmError>,
) -> Result<T, JsError> {
    SLAB.with_borrow(|slab| {
        let r1 = slab
            .get(h1 as usize)
            .and_then(Option::as_ref)
            .ok_or(WasmError::InvalidHandle { handle: h1 })?;
        let r2 = slab
            .get(h2 as usize)
            .and_then(Option::as_ref)
            .ok_or(WasmError::InvalidHandle { handle: h2 })?;
        f(r1, r2).map_err(Into::into)
    })
}

/// Free a resource, making its slot available for reuse.
pub fn free(handle: u32) {
    SLAB.with_borrow_mut(|slab| {
        let idx = handle as usize;
        if idx < slab.len() {
            slab[idx] = None;
        }
    });
}

/// Try to access a resource by handle, returning `WasmError` on failure.
///
/// This is the non-WASM-aware version used in tests (avoids `JsError`
/// construction which panics on non-WASM targets).
#[cfg(test)]
pub fn try_get<T>(
    handle: u32,
    f: impl FnOnce(&Resource) -> Result<T, WasmError>,
) -> Result<T, WasmError> {
    SLAB.with_borrow(|slab| {
        let idx = handle as usize;
        let resource = slab
            .get(idx)
            .and_then(Option::as_ref)
            .ok_or(WasmError::InvalidHandle { handle })?;
        f(resource)
    })
}

/// Try to access two resources by handle, returning `WasmError` on failure.
#[cfg(test)]
pub fn try_get_two<T>(
    h1: u32,
    h2: u32,
    f: impl FnOnce(&Resource, &Resource) -> Result<T, WasmError>,
) -> Result<T, WasmError> {
    SLAB.with_borrow(|slab| {
        let r1 = slab
            .get(h1 as usize)
            .and_then(Option::as_ref)
            .ok_or(WasmError::InvalidHandle { handle: h1 })?;
        let r2 = slab
            .get(h2 as usize)
            .and_then(Option::as_ref)
            .ok_or(WasmError::InvalidHandle { handle: h2 })?;
        f(r1, r2)
    })
}

/// Extract a `Protocol` reference from a resource, or return a type
/// mismatch error.
pub const fn as_protocol(resource: &Resource) -> Result<&Protocol, WasmError> {
    match resource {
        Resource::Protocol(p) => Ok(p),
        _ => Err(WasmError::TypeMismatch {
            expected: "Protocol",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract a `Schema` reference from a resource, or return a type
/// mismatch error.
pub const fn as_schema(resource: &Resource) -> Result<&Schema, WasmError> {
    match resource {
        Resource::Schema(s) => Ok(s),
        _ => Err(WasmError::TypeMismatch {
            expected: "Schema",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract a `CompiledMigration` reference from a resource, or return
/// a type mismatch error. Accepts both `Migration` and
/// `MigrationWithSchemas` variants.
pub const fn as_migration(resource: &Resource) -> Result<&CompiledMigration, WasmError> {
    match resource {
        Resource::Migration(m) | Resource::MigrationWithSchemas { compiled: m, .. } => Ok(m),
        _ => Err(WasmError::TypeMismatch {
            expected: "Migration",
            actual: resource_type_name(resource),
        }),
    }
}

/// Return a human-readable name for a resource variant.
const fn resource_type_name(resource: &Resource) -> &'static str {
    match resource {
        Resource::Protocol(_) => "Protocol",
        Resource::Schema(_) => "Schema",
        Resource::Migration(_) => "Migration",
        Resource::MigrationWithSchemas { .. } => "MigrationWithSchemas",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use panproto_core::inst::CompiledMigration;
    use panproto_core::schema::Protocol;

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

    fn test_migration() -> CompiledMigration {
        CompiledMigration {
            surviving_verts: HashSet::new(),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        }
    }

    #[test]
    fn alloc_and_get_protocol() {
        let h = alloc(Resource::Protocol(test_protocol()));
        let result = try_get(h, |r| {
            let p = as_protocol(r)?;
            Ok(p.name.clone())
        });
        assert_eq!(result.ok(), Some("test".to_string()));
        free(h);
    }

    #[test]
    fn type_mismatch_error() {
        let h = alloc(Resource::Protocol(test_protocol()));
        let result = try_get(h, |r| {
            as_schema(r)?;
            Ok(())
        });
        assert!(result.is_err());
        free(h);
    }

    #[test]
    fn free_and_reuse_slot() {
        let h1 = alloc(Resource::Protocol(test_protocol()));
        free(h1);
        let h2 = alloc(Resource::Migration(test_migration()));
        // Should reuse the freed slot.
        assert_eq!(h1, h2);
        free(h2);
    }

    #[test]
    fn invalid_handle_error() {
        let result = try_get(999, |_| Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn double_free_is_safe() {
        let h = alloc(Resource::Protocol(test_protocol()));
        free(h);
        free(h); // Should not panic.
        let result = try_get(h, |_| Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn alloc_multiple_resources() {
        let h1 = alloc(Resource::Protocol(test_protocol()));
        let h2 = alloc(Resource::Migration(test_migration()));
        assert_ne!(h1, h2);

        let r1 = try_get(h1, |r| {
            as_protocol(r)?;
            Ok(())
        });
        assert!(r1.is_ok());

        let r2 = try_get(h2, |r| {
            as_migration(r)?;
            Ok(())
        });
        assert!(r2.is_ok());

        free(h1);
        free(h2);
    }

    #[test]
    fn with_two_resources_works() {
        let h1 = alloc(Resource::Protocol(test_protocol()));
        let h2 = alloc(Resource::Migration(test_migration()));
        let result = try_get_two(h1, h2, |r1, r2| {
            as_protocol(r1)?;
            as_migration(r2)?;
            Ok(())
        });
        assert!(result.is_ok());
        free(h1);
        free(h2);
    }

    #[test]
    fn with_two_resources_invalid_handle() {
        let h1 = alloc(Resource::Protocol(test_protocol()));
        let result = try_get_two(h1, 999, |_, _| Ok(()));
        assert!(result.is_err());
        free(h1);
    }
}
