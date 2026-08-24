//! Python bindings for the git bridge.

use pyo3::prelude::*;

use panproto_core::vcs::MemStore;
use panproto_git::import_git_repo;

/// Result of importing a git repository into panproto.
#[pyclass(name = "GitImportResult", module = "panproto._native")]
pub struct PyGitImportResult {
    /// Number of commits imported.
    commit_count: usize,
    /// Head commit ID as hex string.
    head_id: String,
}

#[pymethods]
impl PyGitImportResult {
    /// Number of commits imported.
    #[getter]
    const fn commit_count(&self) -> usize {
        self.commit_count
    }

    /// Head commit ID as hex string.
    #[getter]
    fn head_id(&self) -> &str {
        &self.head_id
    }

    fn __repr__(&self) -> String {
        format!(
            "GitImportResult(commits={}, head={})",
            self.commit_count,
            &self.head_id[..8.min(self.head_id.len())]
        )
    }
}

/// Import a git repository into a panproto-vcs in-memory store.
/// Returns the import result summarizing what was imported.
///
/// Walks the whole revision range, so it runs with the GIL released.
#[pyfunction]
fn git_import(py: Python<'_>, repo_path: &str, revspec: &str) -> PyResult<PyGitImportResult> {
    enum ImportFailure {
        Open(git2::Error),
        Import(panproto_git::GitBridgeError),
    }

    let result = py
        .detach(|| {
            let git_repo = git2::Repository::open(repo_path).map_err(ImportFailure::Open)?;
            let mut store = MemStore::new();
            import_git_repo(&git_repo, &mut store, revspec).map_err(ImportFailure::Import)
        })
        .map_err(|e| match e {
            ImportFailure::Open(e) => {
                crate::error::VcsError::new_err(format!("failed to open git repo: {e}"))
            }
            ImportFailure::Import(e) => crate::error::VcsError::new_err(e.to_string()),
        })?;
    Ok(PyGitImportResult {
        commit_count: result.commit_count,
        head_id: result.head_id.to_string(),
    })
}

/// Register git bridge types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyGitImportResult>()?;
    parent.add_function(wrap_pyfunction!(git_import, parent)?)?;
    Ok(())
}
