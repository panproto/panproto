//! Process-global slab allocator for opaque FFI handles.
//!
//! Mirrors the design of `panproto_wasm::slab` (see
//! `crates/panproto-wasm/src/slab.rs`). Handles are `u32` indices into a
//! table of slots; a freed slot goes on a free list and is handed back
//! out by the next allocation.
//!
//! The C ABI never exposes the [`Resource`] enum; callers see only
//! `u32` and rely on the panproto-c API to dispatch to the right
//! variant. Each entry point validates the resource type at the slab
//! boundary and returns [`FfiError::TypeMismatch`] on mismatch.
//!
//! # Two levels of locking
//!
//! Slab state is process-global: a handle allocated by one OS thread is
//! valid from any other. This is required for correctness, not just
//! throughput. Host runtimes call across the C ABI on a pool of OS
//! threads (GHC's threaded RTS migrates a Haskell thread between OS
//! threads across `safe` foreign calls), so a thread-local table would
//! make a handle created on one call invisible on the next.
//!
//! The *table* lock and the *resource* locks are separate. The table
//! lock covers allocation, freeing, and the index lookup that turns a
//! handle into a slot; it is never held while an entry point runs engine
//! work. Each slot then carries its own [`Mutex`], taken for as long as
//! the caller borrows the resource. A VCS commit that writes to disk
//! therefore blocks only other calls on that same repository, not every
//! schema lookup in the process.
//!
//! Calls that borrow several resources at once take the slot locks in
//! ascending handle order and lock a repeated handle only once, so two
//! such calls can never deadlock against each other and a caller that
//! passes the same handle twice gets one shared borrow rather than a
//! self-deadlock.
//!
//! Every lock recovers from poisoning: a panic caught by
//! [`crate::panic::guard`] leaves both the table and the resource it was
//! touching structurally intact, so one panicking operation cannot brick
//! the slab.
//!
//! Tests rely on `cargo nextest`'s per-test process isolation for a
//! fresh table per test; running under `cargo test` (one process,
//! shared global table) can cause spurious handle-equality failures, so
//! the project's CI uses nextest.

use std::sync::{Arc, Mutex, MutexGuard};

use panproto_core::gat::{Model, Theory};
use panproto_core::inst::CompiledMigration;
use panproto_core::io::ProtocolRegistry;
use panproto_core::lens::{ProtolensChain, SymmetricLens};
use panproto_core::lens_dsl::CompiledLens;
use panproto_core::schema::{Protocol, Schema};
use panproto_core::vcs::{DataSetObject, Repository};

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
    /// A free model of a GAT theory. Held by handle rather than
    /// serialized: a model's operation interpretations are closures.
    Model(Box<Model>),
    /// A version-control repository backed by an on-disk store.
    VcsRepo(Box<Repository>),
    /// A protolens chain (reusable, schema-independent).
    ProtolensChain(Box<ProtolensChain>),
    /// A compiled lens document with authoritative ordered stages.
    ///
    /// Chain-shaped C functions continue to expose its structural summary,
    /// while instantiation uses the complete document so value transforms
    /// retain their position relative to structural steps.
    CompiledLensDoc(Box<CompiledLens>),
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

    /// Share the [`Schema`] a [`Resource::Schema`] slot holds.
    ///
    /// Returns a clone of the slot's `Arc`, so an entry point that needs
    /// the schema after the slab borrow ends pays a refcount bump rather
    /// than a deep copy of every vertex, edge, span, and coercion map.
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::Schema`].
    pub fn as_schema_arc(&self) -> Result<Arc<Schema>, FfiError> {
        match self {
            Self::Schema(s) => Ok(Arc::clone(s)),
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

    /// Project an immutable [`Model`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::Model`].
    pub fn as_model(&self) -> Result<&Model, FfiError> {
        match self {
            Self::Model(m) => Ok(m),
            _ => Err(FfiError::TypeMismatch {
                expected: "Model",
                actual: self.type_name(),
            }),
        }
    }

    /// Project an immutable [`Repository`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::VcsRepo`].
    pub fn as_vcs_repo(&self) -> Result<&Repository, FfiError> {
        match self {
            Self::VcsRepo(s) => Ok(s),
            _ => Err(FfiError::TypeMismatch {
                expected: "VcsRepo",
                actual: self.type_name(),
            }),
        }
    }

    /// Project a mutable [`Repository`] reference out of a [`Resource`].
    ///
    /// # Errors
    ///
    /// Returns [`FfiError::TypeMismatch`] when the variant is not
    /// [`Resource::VcsRepo`].
    pub fn as_vcs_repo_mut(&mut self) -> Result<&mut Repository, FfiError> {
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
    /// A [`Resource::CompiledLensDoc`] supplies its compatibility chain, so
    /// existing chain-oriented ABI functions accept handles returned by the
    /// lens-document compiler.
    pub fn as_protolens_chain(&self) -> Result<&ProtolensChain, FfiError> {
        match self {
            Self::ProtolensChain(c) => Ok(c),
            Self::CompiledLensDoc(compiled) => Ok(&compiled.chain),
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
            Self::Model(_) => "Model",
            Self::VcsRepo(_) => "VcsRepo",
            Self::ProtolensChain(_) => "ProtolensChain",
            Self::CompiledLensDoc(_) => "CompiledLensDoc",
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

/// One occupied slab slot: a resource behind its own lock.
///
/// The `Arc` lets a lookup hand the slot out and release the table lock
/// before the caller starts using the resource.
type Slot = Arc<Mutex<Resource>>;

/// The handle table.
///
/// `slots` is indexed by handle. `free` holds the handles whose slots
/// are empty, so allocation is O(1) rather than a scan for the first
/// hole.
struct Table {
    slots: Vec<Option<Slot>>,
    free: Vec<u32>,
}

impl Table {
    const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}

static SLAB: Mutex<Table> = Mutex::new(Table::new());

/// Lock the handle table, recovering the guard if a previous holder
/// panicked. A panic inside an access closure (caught by
/// [`crate::panic::guard`]) poisons the mutex, but the table is
/// structurally sound, so taking the inner guard is safe and keeps it
/// usable for subsequent calls.
fn lock_table() -> MutexGuard<'static, Table> {
    SLAB.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Lock one resource, with the same poison recovery as the table.
fn lock_resource(slot: &Slot) -> MutexGuard<'_, Resource> {
    slot.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Look a handle up in the table, cloning out its slot.
///
/// The table lock is released when this returns, so the caller holds
/// only the resource's own lock while it works.
fn find(handle: u32) -> Result<Slot, FfiError> {
    lock_table()
        .slots
        .get(handle as usize)
        .and_then(Option::as_ref)
        .map(Arc::clone)
        .ok_or(FfiError::InvalidHandle { handle })
}

/// Lock the given slots, taking each distinct handle exactly once and in
/// ascending handle order.
///
/// The order makes acquisition deterministic across threads, so two
/// calls sharing slots cannot deadlock against each other; taking a
/// repeated handle once keeps a caller that passes the same handle twice
/// from deadlocking against itself.
fn lock_in_order(entries: &[(u32, Slot)]) -> Vec<(u32, MutexGuard<'_, Resource>)> {
    let mut order: Vec<&(u32, Slot)> = entries.iter().collect();
    order.sort_by_key(|(handle, _)| *handle);
    order.dedup_by_key(|(handle, _)| *handle);
    order
        .into_iter()
        .map(|(handle, slot)| (*handle, lock_resource(slot)))
        .collect()
}

/// Project the guard taken for `handle` out of a [`lock_in_order`]
/// result.
///
/// # Errors
///
/// Returns [`FfiError::InvalidHandle`] if `handle` was not among the
/// locked entries, which cannot happen for a handle this call locked.
fn borrow<'g>(
    guards: &'g [(u32, MutexGuard<'_, Resource>)],
    handle: u32,
) -> Result<&'g Resource, FfiError> {
    guards
        .iter()
        .find(|(locked, _)| *locked == handle)
        .map(|(_, guard)| &**guard)
        .ok_or(FfiError::InvalidHandle { handle })
}

/// Allocate a resource and return its handle.
///
/// Reuses a freed slot when the free list has one; otherwise appends.
/// If the table has grown to the largest index a `u32` handle can name,
/// the returned handle is one no lookup resolves, so the caller sees an
/// invalid-handle error rather than a silently truncated index.
#[must_use]
pub fn alloc(resource: Resource) -> u32 {
    let slot: Slot = Arc::new(Mutex::new(resource));
    let mut table = lock_table();
    if let Some(handle) = table.free.pop() {
        if let Some(entry) = table.slots.get_mut(handle as usize) {
            *entry = Some(slot);
            return handle;
        }
    }
    match u32::try_from(table.slots.len()) {
        Ok(handle) if handle < u32::MAX => {
            table.slots.push(Some(slot));
            handle
        }
        _ => u32::MAX,
    }
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
    let slot = find(handle)?;
    let guard = lock_resource(&slot);
    f(&guard)
}

/// Read access to two resources by handle.
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
    let entries = [(h1, find(h1)?), (h2, find(h2)?)];
    let guards = lock_in_order(&entries);
    f(borrow(&guards, h1)?, borrow(&guards, h2)?)
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
    let slot = find(handle)?;
    let mut guard = lock_resource(&slot);
    f(&mut guard)
}

/// Read access to three resources by handle.
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
    let entries = [(h1, find(h1)?), (h2, find(h2)?), (h3, find(h3)?)];
    let guards = lock_in_order(&entries);
    f(
        borrow(&guards, h1)?,
        borrow(&guards, h2)?,
        borrow(&guards, h3)?,
    )
}

/// Free a resource, marking the slot reusable.
///
/// Calling on an out-of-range or already-freed handle is a no-op
/// (double-free is safe). The resource itself is dropped after the table
/// lock is released, so a destructor that takes time or touches disk
/// does not hold up other handle operations.
pub fn free(handle: u32) {
    let dropped = {
        let mut table = lock_table();
        match table.slots.get_mut(handle as usize) {
            Some(entry @ Some(_)) => {
                let taken = entry.take();
                table.free.push(handle);
                taken
            }
            _ => None,
        }
    };
    drop(dropped);
}

/// Test-only: drop every resource in the slab and reset its length.
#[cfg(test)]
pub fn reset() {
    let mut table = lock_table();
    table.slots.clear();
    table.free.clear();
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
            op_term_assignments: HashMap::new(),
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
    fn schema_lookup_shares_the_slot_allocation() {
        reset();
        let h = alloc(Resource::Schema(Arc::new(test_schema())));

        let first = with_resource(h, Resource::as_schema_arc).unwrap();
        let second = with_resource(h, Resource::as_schema_arc).unwrap();

        // Both lookups must hand back the slot's own allocation. A deep
        // copy here would cost a full traversal of every vertex, edge,
        // span, and coercion map on each call.
        assert!(
            Arc::ptr_eq(&first, &second),
            "two lookups of one handle returned separate schema allocations"
        );
        let slot_shared = with_resource(h, |r| {
            let held = r.as_schema_arc()?;
            Ok(Arc::ptr_eq(&held, &first))
        })
        .unwrap();
        assert!(slot_shared, "the lookup did not share the slot's schema");

        free(h);
    }

    #[test]
    fn non_schema_handle_rejected_by_schema_arc() {
        reset();
        let h = alloc(Resource::Protocol(Box::new(test_protocol())));
        match with_resource(h, Resource::as_schema_arc) {
            Err(FfiError::TypeMismatch { expected, actual }) => {
                assert_eq!(expected, "Schema");
                assert_eq!(actual, "Protocol");
            }
            other => panic!("expected TypeMismatch, got {:?}", other.map(|_| ())),
        }
        free(h);
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
        // Exercise the mut accessor path on a VcsRepo resource backed by
        // an on-disk Repository rooted at a temp dir.
        let dir = tempfile::tempdir().unwrap();
        let repo = panproto_core::vcs::Repository::init(dir.path()).unwrap();
        let h = alloc(Resource::VcsRepo(Box::new(repo)));
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

    #[test]
    fn handle_allocated_on_another_thread_is_valid() {
        // The slab is process-global, not thread-local: a handle made on
        // one OS thread must be usable from another. A regression to a
        // `thread_local!` slab would make this fail, and would silently
        // corrupt any host runtime that migrates calls across OS threads
        // (GHC's threaded RTS does, across `safe` foreign calls).
        let h = std::thread::spawn(|| alloc(Resource::Protocol(Box::new(test_protocol()))))
            .join()
            .unwrap();
        let name =
            with_resource(h, |r| Ok(r.as_protocol()?.name.clone())).expect("handle valid here");
        assert_eq!(name, "test");
        free(h);
    }

    #[test]
    fn concurrent_alloc_use_free_is_consistent() {
        // Many threads allocating, projecting, and freeing in parallel
        // must never see another thread's slot or a freed slot: the
        // global lock keeps every distinct live handle isolated.
        let workers: Vec<_> = (0..16)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..500 {
                        let h = alloc(Resource::Schema(Arc::new(test_schema())));
                        let ok = with_resource(h, |r| Ok(r.as_schema()?.protocol.clone()))
                            .expect("own handle valid");
                        assert_eq!(ok, "test");
                        free(h);
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("worker thread panicked");
        }
    }

    #[test]
    fn a_borrow_may_span_another_handles_borrow() {
        reset();
        let h1 = alloc(Resource::Protocol(Box::new(test_protocol())));
        let h2 = alloc(Resource::Schema(Arc::new(test_schema())));
        // Borrowing one resource must not lock every other handle in the
        // process. An entry point that reaches back into the slab for a
        // second, unrelated resource has to make progress; under a
        // single table-wide lock held for the whole borrow it cannot.
        let both = with_resource(h1, |p| {
            let proto = p.as_protocol()?.name.clone();
            let schema = with_resource(h2, |s| Ok(s.as_schema()?.protocol.clone()))?;
            Ok(format!("{proto}/{schema}"))
        })
        .unwrap();
        assert_eq!(both, "test/test");
        free(h1);
        free(h2);
    }

    #[test]
    fn slow_work_on_one_handle_does_not_stall_another() {
        reset();
        let h1 = alloc(Resource::Protocol(Box::new(test_protocol())));
        let (borrowed_tx, borrowed_rx) = std::sync::mpsc::channel::<()>();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel::<String>();

        // The worker allocates and uses a handle of its own while the
        // main thread is still inside a borrow. Engine work and disk I/O
        // happen inside such a borrow, so a table lock held across it
        // would serialize every unrelated call in the process; here that
        // shows up as the worker never finishing.
        let worker = std::thread::spawn(move || {
            borrowed_rx.recv().expect("main thread borrowed h1");
            let h2 = alloc(Resource::Schema(Arc::new(test_schema())));
            let name = with_resource(h2, |s| Ok(s.as_schema()?.protocol.clone()))
                .expect("worker handle valid");
            free(h2);
            finished_tx.send(name).expect("main thread still listening");
        });

        let reported = with_resource(h1, |p| {
            let _ = p.as_protocol()?;
            borrowed_tx.send(()).expect("worker still listening");
            Ok(finished_rx.recv().expect("worker made progress"))
        })
        .unwrap();

        worker.join().expect("worker thread panicked");
        assert_eq!(reported, "test");
        free(h1);
    }
}
