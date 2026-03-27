//! Python bindings for panproto I/O (instance parse/emit across 77 protocols).

use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::io::{self, ProtocolRegistry};

use crate::inst::PyInstance;
use crate::schema::PySchema;

/// Protocol-aware instance parser and emitter.
///
/// Wraps the full protocol registry with 77 codecs for parsing and emitting
/// instances across annotation, API, config, data schema, data science,
/// database, domain, serialization, type system, and web document protocols.
#[pyclass(name = "IoRegistry", module = "panproto._native")]
pub struct PyIoRegistry {
    inner: Box<ProtocolRegistry>,
}

#[pymethods]
impl PyIoRegistry {
    /// Create a new registry with all built-in protocol codecs.
    #[new]
    fn new() -> Self {
        Self {
            inner: Box::new(io::default_registry()),
        }
    }

    /// List all registered protocol names.
    fn list_protocols(&self) -> Vec<String> {
        self.inner.protocol_names().map(String::from).collect()
    }

    /// Number of registered protocols.
    fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// Parse raw bytes into a W-type instance.
    ///
    /// Parameters
    /// ----------
    /// protocol : str
    ///     Protocol name (e.g., ``"atproto"``, ``"brat"``, ``"avro"``).
    /// schema : Schema
    ///     The schema the instance should conform to.
    /// data : bytes
    ///     Raw input bytes.
    ///
    /// Returns
    /// -------
    /// Instance
    ///     The parsed instance.
    fn parse(&self, protocol: &str, schema: &PySchema, data: &[u8]) -> PyResult<PyInstance> {
        let instance = self
            .inner
            .parse_wtype(protocol, &schema.inner, data)
            .map_err(|e| crate::error::IoError::new_err(format!("parse failed: {e}")))?;
        Ok(PyInstance {
            inner: instance,
            schema: Arc::clone(&schema.inner),
        })
    }

    /// Emit a W-type instance to raw bytes.
    ///
    /// Parameters
    /// ----------
    /// protocol : str
    ///     Protocol name.
    /// schema : Schema
    ///     The schema the instance conforms to.
    /// instance : Instance
    ///     The instance to emit.
    ///
    /// Returns
    /// -------
    /// bytes
    ///     The serialized output.
    fn emit(&self, protocol: &str, schema: &PySchema, instance: &PyInstance) -> PyResult<Vec<u8>> {
        self.inner
            .emit_wtype(protocol, &schema.inner, &instance.inner)
            .map_err(|e| crate::error::IoError::new_err(format!("emit failed: {e}")))
    }

    fn __repr__(&self) -> String {
        format!("IoRegistry(protocols={})", self.inner.len())
    }
}

/// Register I/O types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyIoRegistry>()?;
    Ok(())
}
