//! Thread-local slab allocator with typed resource handles.
//!
//! Resources (protocols, schemas, compiled migrations) are stored in a
//! thread-local `Vec<Option<Resource>>`. Handles are `u32` indices into
//! this vector. Freed slots are reused on subsequent allocations.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use panproto_core::gat::{Name, Theory};
use panproto_core::inst::{CompiledMigration, FieldTransform};
use panproto_core::io::ProtocolRegistry;
use panproto_core::lens::{ProtolensChain, SymmetricLens};
use panproto_core::lens_dsl::{CompiledLens, steps::CompiledStage};
use panproto_core::schema::{Protocol, Schema};
use panproto_core::vcs::{DataSetObject, MemStore};
use wasm_bindgen::JsError;

use crate::error::WasmError;

/// A resource stored in the slab.
///
/// Schemas are stored behind `Arc` so that `MigrationWithSchemas`
/// can share ownership without deep-cloning on every `lift_record`
/// or `get_record` call.
pub enum Resource {
    /// A protocol specification.
    Protocol(Protocol),
    /// A built schema.
    Schema(Arc<Schema>),
    /// A compiled migration ready for per-record application.
    Migration(CompiledMigration),
    /// A compiled migration bundled with its source and target schemas,
    /// needed for lens put operations and accurate schema reconstruction.
    MigrationWithSchemas {
        /// The compiled migration.
        compiled: CompiledMigration,
        /// The source schema (pre-migration).
        src_schema: Arc<Schema>,
        /// The target schema (post-migration).
        tgt_schema: Arc<Schema>,
    },
    /// An I/O protocol registry with the 50 base protocol codecs.
    IoRegistry(Box<ProtocolRegistry>),
    /// A GAT theory.
    Theory(Box<Theory>),
    /// A VCS in-memory repository.
    VcsRepo(Box<MemStore>),
    /// A protolens chain (reusable, schema-independent).
    ProtolensChain(Box<ProtolensChain>),
    /// A compiled lens document with ordered structural and value stages.
    ///
    /// A lens DSL document's `apply_expr` and `compute_field` steps compile
    /// to field transforms. Keeping the complete document on one handle lets
    /// [`crate::api::lens::instantiate_protolens`] run each transform in its
    /// original position. Wherever a plain `ProtolensChain` is expected, this
    /// variant supplies its compatibility `chain` summary.
    CompiledLensDoc(Box<CompiledLens>),
    /// A symmetric lens.
    SymmetricLensHandle(Box<SymmetricLens>),
    /// A data set (instances bound to a schema).
    DataSet(Box<DataSetObject>),
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
    try_get(handle, f).map_err(Into::into)
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
    try_get_two(h1, h2, f).map_err(Into::into)
}

/// Access a resource by handle mutably, returning an error if the handle
/// is invalid or the slot is empty.
pub fn with_resource_mut<T>(
    handle: u32,
    f: impl FnOnce(&mut Resource) -> Result<T, WasmError>,
) -> Result<T, JsError> {
    SLAB.with_borrow_mut(|slab| {
        let idx = handle as usize;
        let resource = slab
            .get_mut(idx)
            .and_then(Option::as_mut)
            .ok_or(WasmError::InvalidHandle { handle })?;
        f(resource).map_err(Into::into)
    })
}

/// Access three resources by handle simultaneously.
///
/// # Errors
///
/// Returns `JsError` if any handle is invalid or freed.
pub fn with_three_resources<T>(
    h1: u32,
    h2: u32,
    h3: u32,
    f: impl FnOnce(&Resource, &Resource, &Resource) -> Result<T, WasmError>,
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
        let r3 = slab
            .get(h3 as usize)
            .and_then(Option::as_ref)
            .ok_or(WasmError::InvalidHandle { handle: h3 })?;
        f(r1, r2, r3).map_err(Into::into)
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

/// Access a resource by handle, reporting failure as [`WasmError`].
///
/// This is the form callers reach for when they are not at the
/// `#[wasm_bindgen]` boundary yet: constructing a `JsError` needs a JS
/// runtime and aborts off wasm32, so any code path that must stay
/// drivable from a host `cargo test` keeps its errors in `WasmError`
/// terms and converts once, at the entry point.
/// [`with_resource`] is that conversion.
///
/// # Errors
///
/// Returns [`WasmError::InvalidHandle`] if the handle is out of range or
/// its slot has been freed. Propagates whatever error `f` returns.
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

/// Access two resources by handle at once, reporting failure as
/// [`WasmError`]. The [`try_get`] counterpart of [`with_two_resources`].
///
/// # Errors
///
/// Returns [`WasmError::InvalidHandle`] if either handle is out of range
/// or its slot has been freed. Propagates whatever error `f` returns.
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
pub fn as_schema(resource: &Resource) -> Result<&Schema, WasmError> {
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

/// Extract a `ProtocolRegistry` reference from a resource, or return
/// a type mismatch error.
pub fn as_io_registry(resource: &Resource) -> Result<&ProtocolRegistry, WasmError> {
    match resource {
        Resource::IoRegistry(r) => Ok(r),
        _ => Err(WasmError::TypeMismatch {
            expected: "IoRegistry",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract a `Theory` reference from a resource, or return a type
/// mismatch error.
pub fn as_theory(resource: &Resource) -> Result<&Theory, WasmError> {
    match resource {
        Resource::Theory(t) => Ok(t),
        _ => Err(WasmError::TypeMismatch {
            expected: "Theory",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract a mutable `MemStore` reference from a `VcsRepo` resource.
pub fn as_vcs_repo_mut(resource: &mut Resource) -> Result<&mut MemStore, WasmError> {
    match resource {
        Resource::VcsRepo(s) => Ok(s),
        _ => Err(WasmError::TypeMismatch {
            expected: "VcsRepo",
            actual: resource_type_name_ref(resource),
        }),
    }
}

/// Extract an immutable `MemStore` reference from a `VcsRepo` resource.
pub fn as_vcs_repo(resource: &Resource) -> Result<&MemStore, WasmError> {
    match resource {
        Resource::VcsRepo(s) => Ok(s.as_ref()),
        _ => Err(WasmError::TypeMismatch {
            expected: "VcsRepo",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract a `ProtolensChain` reference from a resource, or return a
/// type mismatch error.
/// A `CompiledLensDoc` also answers here, with its structural half, so
/// every export that consumes a chain handle accepts either variant.
pub fn as_protolens_chain(resource: &Resource) -> Result<&ProtolensChain, WasmError> {
    match resource {
        Resource::ProtolensChain(c) => Ok(c),
        Resource::CompiledLensDoc(compiled) => Ok(&compiled.chain),
        _ => Err(WasmError::TypeMismatch {
            expected: "ProtolensChain",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract the value-level field transforms carried by a chain-shaped
/// resource, or return a type mismatch error.
///
/// A plain `ProtolensChain` carries none, which is distinct from a
/// non-chain resource: it yields an empty map rather than an error.
pub fn as_field_transforms(
    resource: &Resource,
) -> Result<HashMap<Name, Vec<FieldTransform>>, WasmError> {
    match resource {
        Resource::ProtolensChain(_) => Ok(HashMap::new()),
        Resource::CompiledLensDoc(compiled) => Ok(compiled.field_transforms.clone()),
        _ => Err(WasmError::TypeMismatch {
            expected: "ProtolensChain",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract the ordered stages carried by a chain-shaped resource.
///
/// A legacy plain chain is represented as one structural-only stage. An
/// empty identity chain has no stages.
pub fn as_compiled_stages(resource: &Resource) -> Result<Vec<CompiledStage>, WasmError> {
    match resource {
        Resource::ProtolensChain(chain) => {
            if chain.steps.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![CompiledStage {
                    chain: (**chain).clone(),
                    field_transforms: HashMap::new(),
                }])
            }
        }
        Resource::CompiledLensDoc(compiled) => Ok(compiled.stages.clone()),
        _ => Err(WasmError::TypeMismatch {
            expected: "ProtolensChain",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract a `SymmetricLens` reference from a resource, or return a
/// type mismatch error.
pub fn as_symmetric_lens(resource: &Resource) -> Result<&SymmetricLens, WasmError> {
    match resource {
        Resource::SymmetricLensHandle(s) => Ok(s),
        _ => Err(WasmError::TypeMismatch {
            expected: "SymmetricLens",
            actual: resource_type_name(resource),
        }),
    }
}

/// Extract a `DataSetObject` reference from a resource, or return a
/// type mismatch error.
pub fn as_dataset(resource: &Resource) -> Result<&DataSetObject, WasmError> {
    match resource {
        Resource::DataSet(d) => Ok(d),
        _ => Err(WasmError::TypeMismatch {
            expected: "DataSet",
            actual: resource_type_name(resource),
        }),
    }
}

/// Return a human-readable name for a resource variant (const version).
const fn resource_type_name(resource: &Resource) -> &'static str {
    match resource {
        Resource::Protocol(_) => "Protocol",
        Resource::Schema(_) => "Schema",
        Resource::Migration(_) => "Migration",
        Resource::MigrationWithSchemas { .. } => "MigrationWithSchemas",
        Resource::IoRegistry(_) => "IoRegistry",
        Resource::Theory(_) => "Theory",
        Resource::VcsRepo(_) => "VcsRepo",
        Resource::ProtolensChain(_) => "ProtolensChain",
        Resource::CompiledLensDoc(_) => "CompiledLensDoc",
        Resource::SymmetricLensHandle(_) => "SymmetricLens",
        Resource::DataSet(_) => "DataSet",
    }
}

/// Return a human-readable name for a mutable resource variant.
const fn resource_type_name_ref(resource: &Resource) -> &'static str {
    resource_type_name(resource)
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
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
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
