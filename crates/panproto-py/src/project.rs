//! Python bindings for multi-file project assembly.

use std::collections::HashMap;

use pyo3::prelude::*;

use panproto_project::ProjectBuilder;

use crate::schema::PySchema;

/// Builder for assembling a multi-file project into a unified schema.
///
/// Files are added one at a time (or by scanning a directory), then assembled
/// into a [`PyProjectSchema`] via coproduct construction.
#[pyclass(name = "ProjectBuilder", module = "panproto._native")]
pub struct PyProjectBuilder {
    inner: ProjectBuilder,
}

#[pymethods]
impl PyProjectBuilder {
    #[new]
    fn new() -> Self {
        Self {
            inner: ProjectBuilder::new(),
        }
    }

    /// Add a file to the project.
    fn add_file(&mut self, path: &str, content: &[u8]) -> PyResult<()> {
        self.inner
            .add_file(std::path::Path::new(path), content)
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
    }

    /// Add all files in a directory recursively.
    fn add_directory(&mut self, path: &str) -> PyResult<()> {
        self.inner
            .add_directory(std::path::Path::new(path))
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
    }

    /// Get the number of files added.
    fn file_count(&self) -> usize {
        self.inner.file_count()
    }

    fn __repr__(&self) -> String {
        format!("ProjectBuilder({} files)", self.inner.file_count())
    }
}

/// A parsed project containing a unified schema and per-file metadata.
#[pyclass(name = "ProjectSchema", module = "panproto._native")]
pub struct PyProjectSchema {
    inner: panproto_project::ProjectSchema,
}

#[pymethods]
impl PyProjectSchema {
    /// Get the unified schema.
    #[getter]
    fn schema(&self) -> PySchema {
        PySchema {
            inner: std::sync::Arc::new(self.inner.schema.clone()),
        }
    }

    /// Get the file -> protocol mapping as a dict.
    fn protocol_map(&self) -> HashMap<String, String> {
        self.inner
            .protocol_map
            .iter()
            .map(|(k, v)| (k.display().to_string(), v.clone()))
            .collect()
    }

    /// Get the list of file paths.
    fn file_paths(&self) -> Vec<String> {
        self.inner
            .file_map
            .keys()
            .map(|p| p.display().to_string())
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "ProjectSchema({} files, {} vertices)",
            self.inner.file_map.len(),
            self.inner.schema.vertices.len()
        )
    }
}

/// Build a `ProjectSchema` from a `ProjectBuilder`.
#[pyfunction]
fn build_project(builder: &mut PyProjectBuilder) -> PyResult<PyProjectSchema> {
    // We need to take ownership, so swap with a fresh builder.
    let owned = std::mem::replace(&mut builder.inner, ProjectBuilder::new());
    let project = owned
        .build()
        .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
    Ok(PyProjectSchema { inner: project })
}

/// Parse a directory into a `ProjectSchema` (convenience function).
#[pyfunction]
fn parse_project(directory: &str) -> PyResult<PyProjectSchema> {
    let mut builder = ProjectBuilder::new();
    builder
        .add_directory(std::path::Path::new(directory))
        .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
    let project = builder
        .build()
        .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
    Ok(PyProjectSchema { inner: project })
}

/// Register project types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyProjectBuilder>()?;
    parent.add_class::<PyProjectSchema>()?;
    parent.add_function(wrap_pyfunction!(build_project, parent)?)?;
    parent.add_function(wrap_pyfunction!(parse_project, parent)?)?;
    Ok(())
}
