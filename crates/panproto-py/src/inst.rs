//! Python bindings for panproto instance types.
//!
//! Wraps `WInstance` (tree-shaped, W-type instances). The actual
//! functions `parse_json`, `to_json`, and `validate_wtype` take
//! specific argument orders matching their Rust signatures exactly.

use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::inst::{self, WInstance};
use panproto_core::schema::Schema;

use crate::convert;
use crate::schema::PySchema;

/// A W-type instance (tree-shaped data conforming to a schema).
#[pyclass(name = "Instance", module = "panproto._native")]
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
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
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
        let instance = inst::parse_json(&schema.inner, root_vertex, &json_val)
            .map_err(|e| crate::error::IoError::new_err(format!("parse failed: {e}")))?;
        Ok(Self {
            inner: instance,
            schema: Arc::clone(&schema.inner),
        })
    }

    /// Validate the instance against its schema.
    ///
    /// Returns
    /// -------
    /// list[str]
    ///     A list of validation error messages. Empty if valid.
    fn validate(&self) -> Vec<String> {
        let errors = inst::validate_wtype(&self.schema, &self.inner);
        errors.into_iter().map(|e| e.to_string()).collect()
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
    Ok(())
}
