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

use panproto_core::gat::Theory;
use panproto_core::inst::CompiledMigration;
use panproto_core::io::ProtocolRegistry;
use panproto_core::lens::{ProtolensChain, SymmetricLens};
use panproto_core::schema::{Protocol, Schema};
use panproto_core::vcs::{DataSetObject, MemStore};

#[cfg(feature = "full-parse")]
use panproto_core::parse::ParserRegistry;
#[cfg(feature = "project")]
use panproto_core::project::{ProjectBuilder, ProjectSchema};

use crate::error::FfiError;

/// A resource stored in the slab.
///
/// Schemas are stored behind `Arc` so that downstream operations
/// that need both the source and target schema (lens `put`, schema
/// diff) can share ownership without deep-cloning. Every other large
/// payload is boxed: the slab vector should not pay the worst-case
/// variant size on every slot.
///
/// This mirrors the resource taxonomy of `panproto_wasm::slab`; see
/// `crates/panproto-wasm/src/slab.rs` for the authoritative surface
/// that the bindings target.
pub enum Resource {
    /// A protocol specification.
    Protocol(Box<Protocol>),
    /// A built schema, behind an `Arc` for cheap clones.
    Schema(Arc<Schema>),
    /// A compiled migration ready for per-record application.
    Migration(Box<CompiledMigration>),
    /// A compiled migration bundled with its source and target schemas,
    /// needed for lens `put` operations and accurate schema
    /// reconstruction.
    MigrationWithSchemas {
        /// The compiled migration.
        compiled: Box<CompiledMigration>,
        /// The source schema (pre-migration).
        src_schema: Arc<Schema>,
        /// The target schema (post-migration).
        tgt_schema: Arc<Schema>,
    },
    /// An I/O protocol registry with all built-in protocol codecs.
    IoRegistry(Box<ProtocolRegistry>),
    /// A GAT theory.
    Theory(Box<Theory>),
    /// A VCS in-memory repository.
    VcsRepo(Box<MemStore>),
    /// A protolens chain (reusable, schema-independent).
    ProtolensChain(Box<ProtolensChain>),
    /// A symmetric lens.
    SymmetricLensHandle(Box<SymmetricLens>),
    /// A data set (instances bound to a schema).
    DataSet(Box<DataSetObject>),
    /// A full-AST parser registry over all enabled tree-sitter grammars.
    #[cfg(feature = "full-parse")]
    AstRegistry(Box<ParserRegistry>),
    /// A multi-file project builder accumulating files for assembly.
    #[cfg(feature = "project")]
    ProjectBuilder(Box<ProjectBuilder>),
    /// A parsed project: a unified schema plus per-file metadata.
    #[cfg(feature = "project")]
    ProjectSchema(Box<ProjectSchema>),
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
            _ => Err(FfiError::TypeMismatch {
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
            _ => Err(FfiError::TypeMismatch {
                expected: "Schema",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a [`CompiledMigration`] reference out of a [`Resource`].
    ///
    /// Accepts both [`Resource::Migration`] and
    /// [`Resource::MigrationWithSchemas`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is neither.
    pub fn as_migration(&self) -> Result<&CompiledMigration, FfiError> {
        match self {
            Self::Migration(m) | Self::MigrationWithSchemas { compiled: m, .. } => Ok(m),
            _ => Err(FfiError::TypeMismatch {
                expected: "Migration",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a [`ProtocolRegistry`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::IoRegistry`].
    pub fn as_io_registry(&self) -> Result<&ProtocolRegistry, FfiError> {
        match self {
            Self::IoRegistry(r) => Ok(r),
            _ => Err(FfiError::TypeMismatch {
                expected: "IoRegistry",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a [`Theory`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::Theory`].
    pub fn as_theory(&self) -> Result<&Theory, FfiError> {
        match self {
            Self::Theory(t) => Ok(t),
            _ => Err(FfiError::TypeMismatch {
                expected: "Theory",
                actual: self.type_name(),
            }),
        }
    }

    /// Project an immutable [`MemStore`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::VcsRepo`].
    pub fn as_vcs_repo(&self) -> Result<&MemStore, FfiError> {
        match self {
            Self::VcsRepo(s) => Ok(s),
            _ => Err(FfiError::TypeMismatch {
                expected: "VcsRepo",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a mutable [`MemStore`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::VcsRepo`].
    pub fn as_vcs_repo_mut(&mut self) -> Result<&mut MemStore, FfiError> {
        match self {
            Self::VcsRepo(s) => Ok(s),
            other => Err(FfiError::TypeMismatch {
                expected: "VcsRepo",
                actual: other.type_name(),
            }),
        }
    }

    /// Project a [`ProtolensChain`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::ProtolensChain`].
    pub fn as_protolens_chain(&self) -> Result<&ProtolensChain, FfiError> {
        match self {
            Self::ProtolensChain(c) => Ok(c),
            _ => Err(FfiError::TypeMismatch {
                expected: "ProtolensChain",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a [`SymmetricLens`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::SymmetricLensHandle`].
    pub fn as_symmetric_lens(&self) -> Result<&SymmetricLens, FfiError> {
        match self {
            Self::SymmetricLensHandle(s) => Ok(s),
            _ => Err(FfiError::TypeMismatch {
                expected: "SymmetricLens",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a [`DataSetObject`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::DataSet`].
    pub fn as_dataset(&self) -> Result<&DataSetObject, FfiError> {
        match self {
            Self::DataSet(d) => Ok(d),
            _ => Err(FfiError::TypeMismatch {
                expected: "DataSet",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a [`ParserRegistry`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::AstRegistry`].
    #[cfg(feature = "full-parse")]
    pub fn as_ast_registry(&self) -> Result<&ParserRegistry, FfiError> {
        match self {
            Self::AstRegistry(r) => Ok(r),
            _ => Err(FfiError::TypeMismatch {
                expected: "AstRegistry",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a mutable [`ProjectBuilder`] reference out of a
    /// [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::ProjectBuilder`].
    #[cfg(feature = "project")]
    pub fn as_project_builder_mut(&mut self) -> Result<&mut ProjectBuilder, FfiError> {
        match self {
            Self::ProjectBuilder(b) => Ok(b),
            other => Err(FfiError::TypeMismatch {
                expected: "ProjectBuilder",
                actual: other.type_name(),
            }),
        }
    }

    /// Project a [`ProjectSchema`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::ProjectSchema`].
    #[cfg(feature = "project")]
    pub fn as_project_schema(&self) -> Result<&ProjectSchema, FfiError> {
        match self {
            Self::ProjectSchema(p) => Ok(p),
            _ => Err(FfiError::TypeMismatch {
                expected: "ProjectSchema",
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
            Self::Migration(_) => "Migration",
            Self::MigrationWithSchemas { .. } => "MigrationWithSchemas",
            Self::IoRegistry(_) => "IoRegistry",
            Self::Theory(_) => "Theory",
            Self::VcsRepo(_) => "VcsRepo",
            Self::ProtolensChain(_) => "ProtolensChain",
            Self::SymmetricLensHandle(_) => "SymmetricLens",
            Self::DataSet(_) => "DataSet",
            #[cfg(feature = "full-parse")]
            Self::AstRegistry(_) => "AstRegistry",
            #[cfg(feature = "project")]
            Self::ProjectBuilder(_) => "ProjectBuilder",
            #[cfg(feature = "project")]
            Self::ProjectSchema(_) => "ProjectSchema",
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

/// Mutable access to a resource by handle.
///
/// Used by domains that mutate a resource in place (the VCS repo
/// staging area, project-builder file accumulation).
///
/// # Errors
///
/// Returns [`FfiError::InvalidHandle`] if the handle is out of range
/// or the slot has been freed. Propagates whatever error `f` returns.
pub fn with_resource_mut<T>(
    handle: u32,
    f: impl FnOnce(&mut Resource) -> Result<T, FfiError>,
) -> Result<T, FfiError> {
    SLAB.with_borrow_mut(|slab| {
        let resource = slab
            .get_mut(handle as usize)
            .and_then(Option::as_mut)
            .ok_or(FfiError::InvalidHandle { handle })?;
        f(resource)
    })
}

/// Read access to three resources by handle, in a single slab borrow.
///
/// Used by domains that take three handles at once (theory colimit
/// over a shared base).
///
/// # Errors
///
/// Returns [`FfiError::InvalidHandle`] if any handle is out of range
/// or freed. Propagates whatever error `f` returns.
pub fn with_three_resources<T>(
    h1: u32,
    h2: u32,
    h3: u32,
    f: impl FnOnce(&Resource, &Resource, &Resource) -> Result<T, FfiError>,
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
        let r3 = slab
            .get(h3 as usize)
            .and_then(Option::as_ref)
            .ok_or(FfiError::InvalidHandle { handle: h3 })?;
        f(r1, r2, r3)
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
    use std::collections::{HashMap, HashSet};

    use super::*;

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
            expansion_path: HashMap::new(),
        }
    }

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

    #[test]
    fn migration_projection_accepts_both_variants() {
        reset();
        let bare = alloc(Resource::Migration(Box::new(test_migration())));
        assert!(with_resource(bare, |r| r.as_migration().map(|_| ())).is_ok());

        let bundled = alloc(Resource::MigrationWithSchemas {
            compiled: Box::new(test_migration()),
            src_schema: Arc::new(test_schema()),
            tgt_schema: Arc::new(test_schema()),
        });
        assert!(with_resource(bundled, |r| r.as_migration().map(|_| ())).is_ok());

        // A non-migration handle is rejected.
        let proto = alloc(Resource::Protocol(Box::new(test_protocol())));
        let mismatch = with_resource(proto, |r| r.as_migration().map(|_| ()));
        assert!(matches!(mismatch, Err(FfiError::TypeMismatch { .. })));

        free(bare);
        free(bundled);
        free(proto);
    }

    #[test]
    fn with_resource_mut_allows_mutation() {
        reset();
        // Exercise the mut accessor path on a VcsRepo resource.
        let h = alloc(Resource::VcsRepo(Box::new(
            panproto_core::vcs::MemStore::new(),
        )));
        let result = with_resource_mut(h, |r| {
            let _store = r.as_vcs_repo_mut()?;
            Ok(())
        });
        assert!(result.is_ok());
        free(h);
    }

    #[test]
    fn with_three_resources_works() {
        reset();
        let h1 = alloc(Resource::Protocol(Box::new(test_protocol())));
        let h2 = alloc(Resource::Schema(Arc::new(test_schema())));
        let h3 = alloc(Resource::Migration(Box::new(test_migration())));
        let result = with_three_resources(h1, h2, h3, |r1, r2, r3| {
            let _ = r1.as_protocol()?;
            let _ = r2.as_schema()?;
            let _ = r3.as_migration()?;
            Ok(())
        });
        assert!(result.is_ok());
        free(h1);
        free(h2);
        free(h3);
    }

    #[test]
    fn with_three_resources_invalid_handle() {
        reset();
        let h = alloc(Resource::Protocol(Box::new(test_protocol())));
        let result = with_three_resources(h, 9999, h, |_, _, _| Ok(()));
        assert!(matches!(result, Err(FfiError::InvalidHandle { .. })));
        free(h);
    }
}
