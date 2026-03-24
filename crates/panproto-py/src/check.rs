//! Python bindings for panproto breaking change detection.

use pyo3::prelude::*;

use panproto_core::check::{self, CompatReport, SchemaDiff};

use crate::convert;
use crate::schema::{PyProtocol, PySchema};

/// Structural diff between two schemas.
#[pyclass(name = "SchemaDiff", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PySchemaDiff {
    pub(crate) inner: SchemaDiff,
}

#[pymethods]
impl PySchemaDiff {
    /// Classify the diff as breaking/non-breaking changes.
    fn classify(&self, protocol: &PyProtocol) -> PyCompatReport {
        let report = check::classify(&self.inner, &protocol.inner);
        PyCompatReport { inner: report }
    }

    /// The full diff as a Python dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "SchemaDiff(added_vertices={}, removed_vertices={}, kind_changes={})",
            self.inner.added_vertices.len(),
            self.inner.removed_vertices.len(),
            self.inner.kind_changes.len()
        )
    }
}

/// Compatibility report classifying changes as breaking or non-breaking.
#[pyclass(name = "CompatReport", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyCompatReport {
    pub(crate) inner: CompatReport,
}

#[pymethods]
impl PyCompatReport {
    /// Whether the change is backward-compatible.
    #[getter]
    const fn compatible(&self) -> bool {
        self.inner.compatible
    }

    /// Breaking changes as a list of dicts.
    #[getter]
    fn breaking_changes(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner.breaking)
    }

    /// Non-breaking changes as a list of dicts.
    #[getter]
    fn non_breaking_changes(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner.non_breaking)
    }

    /// Human-readable text report.
    fn report_text(&self) -> String {
        check::report_text(&self.inner)
    }

    /// JSON report.
    fn report_json(&self) -> String {
        check::report_json(&self.inner).to_string()
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "CompatReport(compatible={}, breaking={}, non_breaking={})",
            self.inner.compatible,
            self.inner.breaking.len(),
            self.inner.non_breaking.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Compute the structural diff between two schemas.
#[pyfunction]
pub fn diff_schemas(old_schema: &PySchema, new_schema: &PySchema) -> PySchemaDiff {
    let inner = check::diff(&old_schema.inner, &new_schema.inner);
    PySchemaDiff { inner }
}

/// Diff and classify in one step.
#[pyfunction]
pub fn diff_and_classify(
    old_schema: &PySchema,
    new_schema: &PySchema,
    protocol: &PyProtocol,
) -> PyCompatReport {
    let d = check::diff(&old_schema.inner, &new_schema.inner);
    let report = check::classify(&d, &protocol.inner);
    PyCompatReport { inner: report }
}

/// Register check types and functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PySchemaDiff>()?;
    parent.add_class::<PyCompatReport>()?;
    parent.add_function(wrap_pyfunction!(diff_schemas, parent)?)?;
    parent.add_function(wrap_pyfunction!(diff_and_classify, parent)?)?;
    Ok(())
}
