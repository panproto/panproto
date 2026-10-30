//! Python bindings for panproto instance types.
//!
//! Wraps `WInstance` (tree-shaped, W-type instances) and the
//! `ValidationReport` that `validate_wtype` returns. The actual functions
//! `parse_json`, `to_json`, and `validate_wtype` take specific argument
//! orders matching their Rust signatures exactly.

use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::gat::CheckStatus;
use panproto_core::inst::{self, Requirement, ValidationReport, WInstance};
use panproto_core::schema::Schema;

use crate::convert;
use crate::schema::{PyProtocol, PySchema};

/// The result of validating an instance against a schema under a protocol.
///
/// Wraps `panproto_inst::ValidationReport`. ``status`` is ``"passed"``,
/// ``"failed"`` or ``"incomplete"``: a required diagnostic makes it
/// ``"failed"``, otherwise a required check left undecided makes it
/// ``"incomplete"``; advisory entries never change it. ``diagnostics``,
/// ``incomplete`` and ``coverage`` carry the structured entries, and
/// ``messages`` renders the entries that decide the status.
#[pyclass(name = "ValidationReport", frozen, module = "panproto._native")]
pub struct PyValidationReport {
    pub(crate) inner: ValidationReport,
}

/// The messages of the entries that decide `report`'s status: none when it
/// passed, otherwise every required diagnostic in report order, then every
/// required check left undecided.
pub fn required_messages(report: &ValidationReport) -> Vec<String> {
    if report.status == CheckStatus::Passed {
        return Vec::new();
    }
    report
        .diagnostics
        .iter()
        .filter(|d| d.requirement == Requirement::Required)
        .map(ToString::to_string)
        .chain(
            report
                .incomplete
                .iter()
                .filter(|i| i.requirement == Requirement::Required)
                .map(ToString::to_string),
        )
        .collect()
}

impl PyValidationReport {
    /// The report as a Python dict, through its serde form.
    fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        Ok(convert::to_python(py, &self.inner)?
            .into_bound(py)
            .cast_into::<pyo3::types::PyDict>()?)
    }

    /// The value of one field of the report's serde form.
    fn field(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        self.as_dict(py)?
            .get_item(key)?
            .map(Bound::unbind)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_owned()))
    }
}

#[pymethods]
impl PyValidationReport {
    /// The aggregate status: ``"passed"``, ``"failed"`` or ``"incomplete"``.
    #[getter]
    const fn status(&self) -> &'static str {
        match self.inner.status {
            CheckStatus::Passed => "passed",
            CheckStatus::Failed => "failed",
            CheckStatus::Incomplete => "incomplete",
        }
    }

    /// Whether the status is ``"passed"``.
    #[getter]
    fn passed(&self) -> bool {
        self.inner.status == CheckStatus::Passed
    }

    /// The shape of the instance validated: ``"tree"``, ``"table"`` or
    /// ``"graph"``.
    #[getter]
    fn shape(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.field(py, "shape")
    }

    /// What the report covers, e.g. ``"instance"``.
    #[getter]
    fn scope(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.field(py, "scope")
    }

    /// The digest of the validation plan that produced the report, as 64
    /// lowercase hex characters.
    #[getter]
    fn plan(&self) -> String {
        self.inner.plan.to_hex()
    }

    /// Every failed check instance, as dicts in report order.
    #[getter]
    fn diagnostics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner.diagnostics)
    }

    /// Every check instance not decided, as dicts in report order.
    #[getter]
    fn incomplete(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner.incomplete)
    }

    /// How the instances of each compiled check fared, as
    /// ``(check, coverage)`` pairs in check order.
    #[getter]
    fn coverage(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.field(py, "coverage")
    }

    /// The messages of the entries that decide the status: empty when the
    /// report passed, otherwise every required diagnostic in report order,
    /// then every required check left undecided.
    #[getter]
    fn messages(&self) -> Vec<String> {
        required_messages(&self.inner)
    }

    /// The identity of the report, as 64 lowercase hex characters.
    fn digest(&self) -> String {
        self.inner.digest().to_hex()
    }

    /// The whole report as a dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "ValidationReport(status='{}', diagnostics={}, incomplete={})",
            self.status(),
            self.inner.diagnostics.len(),
            self.inner.incomplete.len()
        )
    }
}

/// A W-type instance (tree-shaped data conforming to a schema).
#[pyclass(from_py_object, name = "Instance", module = "panproto._native")]
#[derive(Clone)]
pub struct PyInstance {
    pub(crate) inner: WInstance,
    pub(crate) schema: Arc<Schema>,
}

#[pymethods]
impl PyInstance {
    /// Number of nodes in the instance.
    #[getter]
    fn node_count(&self) -> usize {
        self.inner.nodes.len()
    }

    /// Number of arcs in the instance.
    #[getter]
    fn arc_count(&self) -> usize {
        self.inner.arcs.len()
    }

    /// The root node ID.
    #[getter]
    const fn root(&self) -> u32 {
        self.inner.root
    }

    /// Serialize the instance to a JSON string using the schema's structure.
    fn to_json(&self) -> String {
        let val = inst::to_json(&self.schema, &self.inner);
        serde_json::to_string_pretty(&val).unwrap_or_else(|_| "null".to_string())
    }

    /// Serialize the raw W-type structure to a Python dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner)
    }

    /// Parse an instance from a JSON value string.
    ///
    /// Parameters
    /// ----------
    /// schema : Schema
    ///     The schema the instance should conform to.
    /// `root_vertex` : str
    ///     The root vertex ID in the schema to parse from.
    /// `json_str` : str
    ///     JSON string representing the instance data.
    #[staticmethod]
    fn from_json(schema: &PySchema, root_vertex: &str, json_str: &str) -> PyResult<Self> {
        let json_val: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| crate::error::IoError::new_err(format!("invalid JSON: {e}")))?;
        let instance = inst::parse_json(
            &schema.inner,
            &crate::protocols::protocol_for_schema(&schema.inner),
            root_vertex,
            &json_val,
        )
            .map_err(|e| crate::error::IoError::new_err(format!("parse failed: {e}")))?;
        Ok(Self {
            inner: instance,
            schema: Arc::clone(&schema.inner),
        })
    }

    /// Validate the instance against its schema under ``protocol``.
    ///
    /// The schema says which vertices and edges exist; the protocol says
    /// what its vertex kinds and constraint sorts mean, so validation
    /// needs both. Runs with an unlimited budget and no external
    /// verdicts.
    ///
    /// Parameters
    /// ----------
    /// protocol : Protocol
    ///     The protocol the instance's schema is read under.
    ///
    /// Returns
    /// -------
    /// `ValidationReport`
    ///     The report; ``report.passed`` is ``True`` when the instance is
    ///     valid, and ``report.messages`` lists what decides the status
    ///     otherwise.
    fn validate(&self, py: Python<'_>, protocol: &PyProtocol) -> PyValidationReport {
        let report = py.detach(|| inst::validate_wtype(&self.inner, &self.schema, &protocol.inner));
        PyValidationReport { inner: report }
    }

    fn __repr__(&self) -> String {
        format!(
            "Instance(nodes={}, arcs={}, root={})",
            self.inner.nodes.len(),
            self.inner.arcs.len(),
            self.inner.root
        )
    }

    fn __len__(&self) -> usize {
        self.inner.nodes.len()
    }
}

/// Register instance types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyInstance>()?;
    parent.add_class::<PyValidationReport>()?;
    Ok(())
}
