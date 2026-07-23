//! Python bindings for panproto schematic version control.
//!
//! Two repository types are exposed:
//!
//! * [`PyVcsRepository`] — an in-memory `MemStore`-backed repository,
//!   useful for tests and ephemeral schema-tracking. Kept for backward
//!   compatibility with earlier panproto-py releases.
//! * [`PyRepository`] — the filesystem-backed `Repository` from
//!   `panproto-vcs`. Wraps the full porcelain (`init`, `open`, `add`,
//!   `commit`, `log`, `merge`, `cherry_pick`, `rebase`, `reset`, `amend`,
//!   `gc`) plus the plumbing needed by external tooling (branches, tags,
//!   blame, bisect, stash, status, data migration). Closes issue #56.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use panproto_core::gat::Name;
use panproto_core::schema::Edge;
use panproto_core::vcs::{
    CommitObject, HeadState, Index, MemStore, ObjectId, ReflogEntry, Repository, StaleData, Store,
    TagObject,
    bisect::{self, BisectStep},
    blame::{self, BlameEntry},
    data_mig, edit_mig,
    gc::GcReport,
    hash::hash_commit,
    index::ValidationStatus,
    merge::MergeResult,
    object::Object,
    refs,
    reset::ResetMode,
    stash::{self, StashEntry},
    store as vcs_store,
    tree::{resolve_commit_schema_dyn, store_schema_as_tree},
};

use crate::convert;
use crate::error::VcsError;
use crate::schema::PySchema;

// ---------------------------------------------------------------------------
// MemStore-backed repository (preserved for back-compat)
// ---------------------------------------------------------------------------

/// An in-memory schematic version control repository.
///
/// Tracks schema evolution via a content-addressed DAG of commits.
/// Merge is computed via schema colimit (pushout).
#[pyclass(name = "VcsRepository", module = "panproto._native")]
pub struct PyVcsRepository {
    store: MemStore,
}

#[pymethods]
impl PyVcsRepository {
    /// Create a new empty in-memory repository.
    #[new]
    fn new() -> Self {
        Self {
            store: MemStore::new(),
        }
    }

    /// Add a schema to the object store.
    fn add(&mut self, schema: &PySchema) -> PyResult<String> {
        let id = store_schema_as_tree(&mut self.store, schema.inner.as_ref().clone())
            .map_err(|e| VcsError::new_err(format!("add failed: {e}")))?;
        Ok(id.to_string())
    }

    /// List all refs in the store.
    fn list_refs(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let refs = self
            .store
            .list_refs("")
            .map_err(|e| VcsError::new_err(format!("list_refs failed: {e}")))?;
        let items: Vec<(String, String)> = refs
            .into_iter()
            .map(|(name, id)| (name, id.to_string()))
            .collect();
        convert::to_python(py, &items)
    }

    #[allow(clippy::unused_self)]
    fn __repr__(&self) -> String {
        "VcsRepository(in-memory)".to_string()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_oid(s: &str) -> PyResult<ObjectId> {
    ObjectId::from_str(s)
        .map_err(|e| PyValueError::new_err(format!("invalid object id {s:?}: {e}")))
}

fn vcs_err<E: std::fmt::Display>(e: E) -> PyErr {
    VcsError::new_err(e.to_string())
}

fn commit_to_value(id: ObjectId, c: &CommitObject) -> serde_json::Value {
    let parents: Vec<String> = c.parents.iter().map(ToString::to_string).collect();
    serde_json::json!({
        "id": id.to_string(),
        "schema_id": c.schema_id.to_string(),
        "parents": parents,
        "migration_id": c.migration_id.map(|i| i.to_string()),
        "protocol": c.protocol,
        "author": c.author,
        "timestamp": c.timestamp,
        "message": c.message,
        "data_ids": c.data_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
    })
}

fn blame_to_value(b: &BlameEntry) -> serde_json::Value {
    serde_json::json!({
        "commit_id": b.commit_id.to_string(),
        "author": b.author,
        "timestamp": b.timestamp,
        "message": b.message,
    })
}

fn stash_to_value(s: &StashEntry) -> serde_json::Value {
    serde_json::json!({
        "index": s.index,
        "commit_id": s.commit_id.to_string(),
        "message": s.message,
        "timestamp": s.timestamp,
    })
}

fn stale_to_value(s: &StaleData) -> serde_json::Value {
    serde_json::json!({
        "data_id": s.data_id.to_string(),
        "data_schema_id": s.data_schema_id.to_string(),
        "head_schema_id": s.head_schema_id.to_string(),
    })
}

fn gc_to_value(r: &GcReport) -> serde_json::Value {
    serde_json::json!({
        "reachable": r.reachable,
        "deleted": r.deleted.iter().map(ToString::to_string).collect::<Vec<_>>(),
    })
}

fn head_state_to_string(state: &HeadState) -> String {
    match state {
        HeadState::Branch(name) => format!("ref: refs/heads/{name}"),
        HeadState::Detached(id) => id.to_string(),
    }
}

fn validation_to_string(v: &ValidationStatus) -> String {
    match v {
        ValidationStatus::Pending => "pending".to_string(),
        ValidationStatus::Valid => "valid".to_string(),
        ValidationStatus::Invalid(reasons) => format!("invalid: {}", reasons.join("; ")),
    }
}

fn reflog_entry_to_value(e: &ReflogEntry) -> serde_json::Value {
    serde_json::json!({
        "old_id": e.old_id.map(|i| i.to_string()),
        "new_id": e.new_id.to_string(),
        "author": e.author,
        "timestamp": e.timestamp,
        "message": e.message,
    })
}

fn index_to_value(idx: &Index) -> serde_json::Value {
    let staged = idx.staged.as_ref().map(|s| {
        serde_json::json!({
            "schema_id": s.schema_id.to_string(),
            "migration_id": s.migration_id.map(|i| i.to_string()),
            "auto_derived": s.auto_derived,
            "validation": validation_to_string(&s.validation),
        })
    });
    let staged_data: Vec<_> = idx
        .staged_data
        .iter()
        .map(|d| {
            serde_json::json!({
                "source_path": d.source_path.to_string_lossy(),
                "data_id": d.data_id.to_string(),
                "schema_id": d.schema_id.to_string(),
            })
        })
        .collect();
    serde_json::json!({
        "staged": staged,
        "staged_data": staged_data,
        "staged_protocol": idx.staged_protocol.map(|i| i.to_string()),
        "has_staged": idx.has_staged(),
    })
}

fn merge_result_to_value(result: &MergeResult) -> serde_json::Value {
    let conflicts: Vec<_> = result
        .conflicts
        .iter()
        .map(|c| serde_json::to_value(c).unwrap_or(serde_json::Value::Null))
        .collect();
    serde_json::json!({
        "conflict_count": result.conflicts.len(),
        "conflicts": conflicts,
        "vertex_count": result.merged_schema.vertices.len(),
        "edge_count": result.merged_schema.edges.len(),
    })
}

fn bisect_step_to_value(step: &BisectStep) -> serde_json::Value {
    match step {
        BisectStep::Test(id) => serde_json::json!({
            "kind": "test",
            "commit_id": id.to_string(),
        }),
        BisectStep::Found(id) => serde_json::json!({
            "kind": "found",
            "commit_id": id.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Filesystem-backed Repository
// ---------------------------------------------------------------------------

/// A filesystem-backed panproto repository.
///
/// Wraps `panproto_vcs::Repository`. The underlying `.panproto/`
/// directory holds the content-addressed object store, refs, the
/// staging index, and reflog entries.
#[pyclass(name = "Repository", module = "panproto._native")]
pub struct PyRepository {
    inner: Repository,
    working_dir: PathBuf,
}

#[pymethods]
impl PyRepository {
    /// Initialise a new repository at ``path``.
    #[staticmethod]
    fn init(path: &str) -> PyResult<Self> {
        let p = PathBuf::from(path);
        let inner = Repository::init(&p).map_err(vcs_err)?;
        Ok(Self {
            inner,
            working_dir: p,
        })
    }

    /// Open an existing repository at ``path``.
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let p = PathBuf::from(path);
        let inner = Repository::open(&p).map_err(vcs_err)?;
        Ok(Self {
            inner,
            working_dir: p,
        })
    }

    /// Path to the repository working directory.
    #[getter]
    fn working_dir(&self) -> String {
        self.working_dir.to_string_lossy().into_owned()
    }

    // -- Staging + commit --

    /// Stage a schema for the next commit.
    ///
    /// With ``skip_verify=True`` the derived migration is still recorded
    /// but GAT migration validation is skipped and the stage is left
    /// ``pending`` (an escape hatch for bulk historical VCS builds where
    /// each version was already validated at its own release).
    #[pyo3(signature = (schema, *, skip_verify=false))]
    fn add(
        &mut self,
        py: Python<'_>,
        schema: &PySchema,
        skip_verify: bool,
    ) -> PyResult<Py<PyAny>> {
        let index = if skip_verify {
            self.inner
                .add_with_options(
                    schema.inner.as_ref(),
                    &panproto_core::vcs::AddOptions { skip_verify: true },
                )
                .map_err(vcs_err)?
        } else {
            self.inner.add(schema.inner.as_ref()).map_err(vcs_err)?
        };
        convert::to_python(py, &index_to_value(&index))
    }

    /// Stage a data file for the next commit.
    ///
    /// Reads ``path``, associates it with the staged or HEAD schema,
    /// counts records, stores the data set, and records it in the index.
    /// The set is keyed by ``key``, or by ``path`` when ``key`` is
    /// ``None``, so the committed data read back with :meth:`data_at` can
    /// be mapped to its origin. Returns the updated index.
    #[pyo3(signature = (path, key=None))]
    fn add_data(&mut self, py: Python<'_>, path: &str, key: Option<&str>) -> PyResult<Py<PyAny>> {
        let index = self
            .inner
            .add_data(std::path::Path::new(path), key)
            .map_err(vcs_err)?;
        convert::to_python(py, &index_to_value(&index))
    }

    /// Create a commit with the given message and author.
    #[pyo3(signature = (message, author, *, skip_verify=false))]
    fn commit(&mut self, message: &str, author: &str, skip_verify: bool) -> PyResult<String> {
        let id = if skip_verify {
            self.inner
                .commit_with_options(
                    message,
                    author,
                    &panproto_core::vcs::CommitOptions { skip_verify: true },
                )
                .map_err(vcs_err)?
        } else {
            self.inner.commit(message, author).map_err(vcs_err)?
        };
        Ok(id.to_string())
    }

    /// Amend the most recent commit with a new message.
    fn amend(&mut self, message: &str, author: &str) -> PyResult<String> {
        let id = self.inner.amend(message, author).map_err(vcs_err)?;
        Ok(id.to_string())
    }

    /// List commits reachable from HEAD, newest first.
    #[pyo3(signature = (limit=None))]
    fn log(&self, py: Python<'_>, limit: Option<usize>) -> PyResult<Py<PyAny>> {
        let commits = self.inner.log(limit).map_err(vcs_err)?;
        let dicts: Result<Vec<_>, _> = commits
            .iter()
            .map(|c| hash_commit(c).map(|id| commit_to_value(id, c)))
            .collect();
        let dicts = dicts.map_err(vcs_err)?;
        convert::to_python(py, &dicts)
    }

    /// Resolve HEAD to a commit object id, or `None` if the repo is empty.
    fn head(&self) -> PyResult<Option<String>> {
        let id = vcs_store::resolve_head(self.inner.store()).map_err(vcs_err)?;
        Ok(id.map(|i| i.to_string()))
    }

    /// Return a string describing the current HEAD state
    /// (``"ref: refs/heads/main"`` or a detached commit id).
    fn head_state(&self) -> PyResult<String> {
        let s = self.inner.store().get_head().map_err(vcs_err)?;
        Ok(head_state_to_string(&s))
    }

    /// Resolve any ref expression (branch, tag, commit-id prefix) to a commit id.
    fn resolve_ref(&self, target: &str) -> PyResult<String> {
        let id = refs::resolve_ref(self.inner.store(), target).map_err(vcs_err)?;
        Ok(id.to_string())
    }

    /// Load the schema stored at a given commit id.
    fn schema_at(&self, commit_id: &str) -> PyResult<PySchema> {
        let id = parse_oid(commit_id)?;
        let commit = match self.inner.store().get(&id).map_err(vcs_err)? {
            Object::Commit(c) => c,
            other => {
                return Err(VcsError::new_err(format!(
                    "object {commit_id} is a {}, not a Commit",
                    other.type_name()
                )));
            }
        };
        let schema = resolve_commit_schema_dyn(self.inner.store(), &commit).map_err(vcs_err)?;
        Ok(PySchema {
            inner: Arc::new(schema),
        })
    }

    /// Read the data sets committed at ``ref`` without moving HEAD.
    ///
    /// Resolves ``ref`` (branch, tag, or commit-id prefix) and returns
    /// one dict per recorded data set, each with ``schema_id`` (hex object
    /// id), ``data`` (the committed data bytes), ``record_count``, and
    /// ``key`` (the caller key from :meth:`add_data`, or ``None``). This
    /// is the data counterpart to :meth:`schema_at`: it never moves HEAD,
    /// the index, or the working tree, unlike :meth:`checkout_with_data`.
    fn data_at(&self, py: Python<'_>, r#ref: &str) -> PyResult<Py<PyAny>> {
        let datasets = self.inner.data_at(r#ref).map_err(vcs_err)?;
        let list = PyList::empty(py);
        for ds in datasets {
            let dict = PyDict::new(py);
            dict.set_item("schema_id", ds.schema_id.to_string())?;
            dict.set_item("data", PyBytes::new(py, &ds.data))?;
            dict.set_item("record_count", ds.record_count)?;
            dict.set_item("key", ds.key)?;
            list.append(dict)?;
        }
        Ok(list.into_any().unbind())
    }

    // -- Refs: branches --

    /// Create a new branch pointing at ``commit_id``.
    fn create_branch(&mut self, name: &str, commit_id: &str) -> PyResult<()> {
        let id = parse_oid(commit_id)?;
        refs::create_branch(self.inner.store_mut(), name, id).map_err(vcs_err)
    }

    /// Delete a branch (refuses if not merged into HEAD).
    fn delete_branch(&mut self, name: &str) -> PyResult<()> {
        refs::delete_branch(self.inner.store_mut(), name).map_err(vcs_err)
    }

    /// Force-delete a branch even if not merged.
    fn force_delete_branch(&mut self, name: &str) -> PyResult<()> {
        refs::force_delete_branch(self.inner.store_mut(), name).map_err(vcs_err)
    }

    /// Rename ``old_name`` to ``new_name``.
    fn rename_branch(&mut self, old_name: &str, new_name: &str) -> PyResult<()> {
        refs::rename_branch(self.inner.store_mut(), old_name, new_name).map_err(vcs_err)
    }

    /// List ``(name, commit_id)`` pairs for every branch.
    fn list_branches(&self) -> PyResult<Vec<(String, String)>> {
        let branches = refs::list_branches(self.inner.store()).map_err(vcs_err)?;
        Ok(branches
            .into_iter()
            .map(|(n, i)| (n, i.to_string()))
            .collect())
    }

    /// Switch HEAD to ``name``.
    fn checkout_branch(&mut self, name: &str) -> PyResult<()> {
        refs::checkout_branch(self.inner.store_mut(), name).map_err(vcs_err)
    }

    /// Detach HEAD onto ``commit_id``.
    fn checkout_detached(&mut self, commit_id: &str) -> PyResult<()> {
        let id = parse_oid(commit_id)?;
        refs::checkout_detached(self.inner.store_mut(), id).map_err(vcs_err)
    }

    /// Create ``name`` pointing at HEAD and check it out.
    fn create_and_checkout_branch(&mut self, name: &str) -> PyResult<()> {
        let head = vcs_store::resolve_head(self.inner.store())
            .map_err(vcs_err)?
            .ok_or_else(|| VcsError::new_err("HEAD does not resolve to a commit"))?;
        refs::create_and_checkout_branch(self.inner.store_mut(), name, head).map_err(vcs_err)
    }

    // -- Refs: tags --

    /// Create a lightweight tag.
    fn create_tag(&mut self, name: &str, commit_id: &str) -> PyResult<()> {
        let id = parse_oid(commit_id)?;
        refs::create_tag(self.inner.store_mut(), name, id).map_err(vcs_err)
    }

    /// Force-create a tag, overwriting if it exists.
    fn create_tag_force(&mut self, name: &str, commit_id: &str) -> PyResult<()> {
        let id = parse_oid(commit_id)?;
        refs::create_tag_force(self.inner.store_mut(), name, id).map_err(vcs_err)
    }

    /// Create an annotated tag with author and message.
    fn create_annotated_tag(
        &mut self,
        name: &str,
        commit_id: &str,
        author: &str,
        message: &str,
    ) -> PyResult<String> {
        let id = parse_oid(commit_id)?;
        let tag_id = refs::create_annotated_tag(self.inner.store_mut(), name, id, author, message)
            .map_err(vcs_err)?;
        Ok(tag_id.to_string())
    }

    /// Delete a tag.
    fn delete_tag(&mut self, name: &str) -> PyResult<()> {
        refs::delete_tag(self.inner.store_mut(), name).map_err(vcs_err)
    }

    /// List ``(name, commit_id)`` pairs for every tag.
    fn list_tags(&self) -> PyResult<Vec<(String, String)>> {
        let tags = refs::list_tags(self.inner.store()).map_err(vcs_err)?;
        Ok(tags.into_iter().map(|(n, i)| (n, i.to_string())).collect())
    }

    /// Read an annotated tag object by its id.
    fn read_annotated_tag(&self, py: Python<'_>, tag_oid: &str) -> PyResult<Py<PyAny>> {
        let id = parse_oid(tag_oid)?;
        let obj = self.inner.store().get(&id).map_err(vcs_err)?;
        match obj {
            Object::Tag(TagObject {
                target,
                tagger,
                timestamp,
                message,
            }) => {
                let v = serde_json::json!({
                    "target": target.to_string(),
                    "tagger": tagger,
                    "timestamp": timestamp,
                    "message": message,
                });
                convert::to_python(py, &v)
            }
            other => Err(VcsError::new_err(format!(
                "object {tag_oid} is a {}, not a Tag",
                other.type_name()
            ))),
        }
    }

    // -- Merge / rewrite --

    /// Three-way merge ``branch`` into HEAD.
    fn merge(&mut self, py: Python<'_>, branch: &str, author: &str) -> PyResult<Py<PyAny>> {
        let result = self.inner.merge(branch, author).map_err(vcs_err)?;
        convert::to_python(py, &merge_result_to_value(&result))
    }

    /// Apply the change introduced by ``commit_id`` on top of HEAD.
    fn cherry_pick(&mut self, commit_id: &str, author: &str) -> PyResult<String> {
        let id = parse_oid(commit_id)?;
        let new_id = self.inner.cherry_pick(id, author).map_err(vcs_err)?;
        Ok(new_id.to_string())
    }

    /// Rebase HEAD onto ``onto``.
    fn rebase(&mut self, onto: &str, author: &str) -> PyResult<String> {
        let id = parse_oid(onto)?;
        let new_id = self.inner.rebase(id, author).map_err(vcs_err)?;
        Ok(new_id.to_string())
    }

    /// Reset HEAD to ``target`` in the given mode (``"soft"``, ``"mixed"``, ``"hard"``).
    fn reset(&mut self, target: &str, mode: &str, author: &str) -> PyResult<()> {
        let id = parse_oid(target)?;
        let m = match mode {
            "soft" => ResetMode::Soft,
            "mixed" => ResetMode::Mixed,
            "hard" => ResetMode::Hard,
            other => {
                return Err(PyValueError::new_err(format!(
                    "reset mode must be one of soft/mixed/hard, got {other:?}"
                )));
            }
        };
        self.inner.reset(id, m, author).map_err(vcs_err)?;
        Ok(())
    }

    /// Delete unreachable objects.
    fn gc(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let report = self.inner.gc().map_err(vcs_err)?;
        convert::to_python(py, &gc_to_value(&report))
    }

    // -- Status / index --

    /// Inspect the staging index.
    fn index(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let idx = self.inner.read_index().map_err(vcs_err)?;
        convert::to_python(py, &index_to_value(&idx))
    }

    /// Returns ``True`` if anything is staged.
    fn has_staged(&self) -> PyResult<bool> {
        let idx = self.inner.read_index().map_err(vcs_err)?;
        Ok(idx.has_staged())
    }

    /// Clear the staging index.
    fn clear_index(&mut self) -> PyResult<()> {
        self.inner.clear_index().map_err(vcs_err)
    }

    // -- Blame --

    /// Find the commit that introduced ``vertex_id`` reachable from ``head``.
    fn blame_vertex(&self, py: Python<'_>, head: &str, vertex_id: &str) -> PyResult<Py<PyAny>> {
        let head_id = parse_oid(head)?;
        let entry = blame::blame_vertex(self.inner.store(), head_id, vertex_id).map_err(vcs_err)?;
        convert::to_python(py, &blame_to_value(&entry))
    }

    /// Find the commit that introduced an edge ``(src, tgt, kind, name)``.
    /// ``name`` is optional (the edge label).
    #[pyo3(signature = (head, src, tgt, kind, name=None))]
    fn blame_edge(
        &self,
        py: Python<'_>,
        head: &str,
        src: &str,
        tgt: &str,
        kind: &str,
        name: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let head_id = parse_oid(head)?;
        let edge = Edge {
            src: Name::from(src),
            tgt: Name::from(tgt),
            kind: Name::from(kind),
            name: name.map(Name::from),
        };
        let entry = blame::blame_edge(self.inner.store(), head_id, &edge).map_err(vcs_err)?;
        convert::to_python(py, &blame_to_value(&entry))
    }

    /// Find the commit that introduced a constraint of ``sort`` on ``vertex_id``.
    fn blame_constraint(
        &self,
        py: Python<'_>,
        head: &str,
        vertex_id: &str,
        sort: &str,
    ) -> PyResult<Py<PyAny>> {
        let head_id = parse_oid(head)?;
        let entry = blame::blame_constraint(self.inner.store(), head_id, vertex_id, sort)
            .map_err(vcs_err)?;
        convert::to_python(py, &blame_to_value(&entry))
    }

    // -- Bisect --

    /// Start a bisect session. Returns ``(state_handle, step)`` where
    /// ``step`` is a dict with ``kind`` (``"test"`` or ``"found"``) and
    /// ``commit_id``.
    fn bisect_start(
        &self,
        py: Python<'_>,
        good: &str,
        bad: &str,
    ) -> PyResult<(PyBisectState, Py<PyAny>)> {
        let good_id = parse_oid(good)?;
        let bad_id = parse_oid(bad)?;
        let (state, step) =
            bisect::bisect_start(self.inner.store(), good_id, bad_id).map_err(vcs_err)?;
        let step_obj = convert::to_python(py, &bisect_step_to_value(&step))?;
        Ok((PyBisectState { inner: state }, step_obj))
    }

    // -- Stash --

    /// Stash the staged schema and return its commit id.
    #[pyo3(signature = (schema_id, author, message=None))]
    fn stash_push(
        &mut self,
        schema_id: &str,
        author: &str,
        message: Option<&str>,
    ) -> PyResult<String> {
        let id = parse_oid(schema_id)?;
        let stash_id =
            stash::stash_push(self.inner.store_mut(), id, author, message).map_err(vcs_err)?;
        Ok(stash_id.to_string())
    }

    /// Pop the most recent stash; returns the schema id it referenced.
    fn stash_pop(&mut self) -> PyResult<String> {
        let id = stash::stash_pop(self.inner.store_mut()).map_err(vcs_err)?;
        Ok(id.to_string())
    }

    /// List stash entries (most recent first).
    fn stash_list(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let entries = stash::stash_list(self.inner.store()).map_err(vcs_err)?;
        let dicts: Vec<_> = entries.iter().map(stash_to_value).collect();
        convert::to_python(py, &dicts)
    }

    /// Apply stash at ``index`` without removing it; returns the schema id.
    fn stash_apply(&self, index: usize) -> PyResult<String> {
        let id = stash::stash_apply(self.inner.store(), index).map_err(vcs_err)?;
        Ok(id.to_string())
    }

    /// Look up the schema id referenced by stash at ``index``.
    fn stash_show(&self, index: usize) -> PyResult<String> {
        let id = stash::stash_show(self.inner.store(), index).map_err(vcs_err)?;
        Ok(id.to_string())
    }

    /// Drop a single stash entry.
    fn stash_drop(&mut self, index: usize) -> PyResult<()> {
        stash::stash_drop(self.inner.store_mut(), index).map_err(vcs_err)
    }

    /// Drop every stash entry.
    fn stash_clear(&mut self) -> PyResult<()> {
        stash::stash_clear(self.inner.store_mut()).map_err(vcs_err)
    }

    // -- Data migration --

    /// Detect data sets whose schema lags behind HEAD's schema.
    fn detect_staleness(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let head = vcs_store::resolve_head(self.inner.store())
            .map_err(vcs_err)?
            .ok_or_else(|| VcsError::new_err("HEAD does not resolve to a commit"))?;
        let commit = match self.inner.store().get(&head).map_err(vcs_err)? {
            Object::Commit(c) => c,
            other => {
                return Err(VcsError::new_err(format!(
                    "HEAD points at a {}, not a Commit",
                    other.type_name()
                )));
            }
        };
        let stale = data_mig::detect_staleness(self.inner.store(), &commit).map_err(vcs_err)?;
        let dicts: Vec<_> = stale.iter().map(stale_to_value).collect();
        convert::to_python(py, &dicts)
    }

    /// Encode an empty edit log to bytes (passthrough wrapper around
    /// ``edit_mig::encode_edit_log`` for testing).
    #[staticmethod]
    fn encode_edit_log_empty() -> PyResult<Vec<u8>> {
        edit_mig::encode_edit_log(&[]).map_err(vcs_err)
    }

    // -- Reflog --

    /// Read the reflog for a ref (e.g. ``"refs/heads/main"``).
    #[pyo3(signature = (ref_name, limit=None))]
    fn read_reflog(
        &self,
        py: Python<'_>,
        ref_name: &str,
        limit: Option<usize>,
    ) -> PyResult<Py<PyAny>> {
        let entries = self
            .inner
            .store()
            .read_reflog(ref_name, limit)
            .map_err(vcs_err)?;
        let dicts: Vec<_> = entries.iter().map(reflog_entry_to_value).collect();
        convert::to_python(py, &dicts)
    }

    fn __repr__(&self) -> String {
        format!("Repository(at={})", self.working_dir.display())
    }
}

// ---------------------------------------------------------------------------
// Bisect state handle
// ---------------------------------------------------------------------------

/// In-progress bisect session.
#[pyclass(name = "BisectState", module = "panproto._native")]
pub struct PyBisectState {
    inner: panproto_core::vcs::bisect::BisectState,
}

#[pymethods]
impl PyBisectState {
    /// Advance the bisect by reporting whether the most recent test commit
    /// was good. Returns the next step (``"test"`` or ``"found"``).
    fn step(&mut self, py: Python<'_>, is_good: bool) -> PyResult<Py<PyAny>> {
        let step = bisect::bisect_step(&mut self.inner, is_good);
        convert::to_python(py, &bisect_step_to_value(&step))
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register VCS types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyVcsRepository>()?;
    parent.add_class::<PyRepository>()?;
    parent.add_class::<PyBisectState>()?;
    Ok(())
}
