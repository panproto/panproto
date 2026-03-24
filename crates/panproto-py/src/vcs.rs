//! Python bindings for panproto schematic version control.

use pyo3::prelude::*;

use panproto_core::vcs::{MemStore, Object, Store};

use crate::convert;
use crate::schema::PySchema;

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
    /// Create a new empty repository.
    #[new]
    fn new() -> Self {
        Self {
            store: MemStore::new(),
        }
    }

    /// Add a schema to the object store.
    ///
    /// Returns
    /// -------
    /// str
    ///     The content-addressed object ID (blake3 hash).
    fn add(&mut self, schema: &PySchema) -> PyResult<String> {
        let object = Object::Schema(Box::new(schema.inner.as_ref().clone()));
        let id = self
            .store
            .put(&object)
            .map_err(|e| crate::error::VcsError::new_err(format!("add failed: {e}")))?;
        Ok(id.to_string())
    }

    /// List all refs in the store.
    fn list_refs(&self, py: Python<'_>) -> PyResult<PyObject> {
        let refs = self
            .store
            .list_refs("")
            .map_err(|e| crate::error::VcsError::new_err(format!("list_refs failed: {e}")))?;
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

/// Register VCS types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyVcsRepository>()?;
    Ok(())
}
