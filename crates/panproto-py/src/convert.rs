//! Conversion helpers between Rust serde types and Python objects.
//!
//! Uses `pythonize` to bridge serde-compatible Rust types to and from
//! Python dicts, lists, and scalars without manual field-by-field mapping.

use pyo3::prelude::*;

/// Convert a serde-serializable Rust value to a Python object.
///
/// Uses `pythonize` to walk the serde data model and produce the
/// corresponding Python dict/list/scalar.
pub fn to_python<T: serde::Serialize>(py: Python<'_>, value: &T) -> PyResult<PyObject> {
    pythonize::pythonize(py, value)
        .map(pyo3::Bound::unbind)
        .map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("serialization to Python failed: {e}"))
        })
}

/// Convert a Python object to a serde-deserializable Rust value.
///
/// The Python object must be a dict, list, or scalar that matches
/// the expected Rust type's serde structure.
pub fn from_python<T: serde::de::DeserializeOwned>(obj: &Bound<'_, PyAny>) -> PyResult<T> {
    pythonize::depythonize(obj).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("deserialization from Python failed: {e}"))
    })
}
