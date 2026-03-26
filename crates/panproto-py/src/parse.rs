//! Python bindings for full-AST tree-sitter parsing.

use pyo3::prelude::*;

use panproto_parse::ParserRegistry;

use crate::schema::PySchema;

/// Registry of full-AST parsers for all supported languages.
///
/// Wraps [`ParserRegistry`] from `panproto-parse`, providing parse
/// (source -> Schema) and emit (Schema -> source) operations.
#[pyclass(name = "AstParserRegistry", module = "panproto._native")]
pub struct PyAstParserRegistry {
    inner: ParserRegistry,
}

#[pymethods]
impl PyAstParserRegistry {
    #[new]
    fn new() -> Self {
        Self {
            inner: ParserRegistry::new(),
        }
    }

    /// Parse a source file into a full-AST schema.
    /// The language is auto-detected from the file extension.
    fn parse_file(&self, path: &str, content: &[u8]) -> PyResult<PySchema> {
        let schema = self
            .inner
            .parse_file(std::path::Path::new(path), content)
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
        Ok(PySchema {
            inner: std::sync::Arc::new(schema),
        })
    }

    /// Parse source code with a specific protocol name.
    fn parse_with_protocol(
        &self,
        protocol: &str,
        content: &[u8],
        file_path: &str,
    ) -> PyResult<PySchema> {
        let schema = self
            .inner
            .parse_with_protocol(protocol, content, file_path)
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
        Ok(PySchema {
            inner: std::sync::Arc::new(schema),
        })
    }

    /// Detect the language protocol for a file path.
    fn detect_language(&self, path: &str) -> Option<String> {
        self.inner
            .detect_language(std::path::Path::new(path))
            .map(String::from)
    }

    /// Emit a schema back to source code bytes.
    fn emit(&self, protocol: &str, schema: &PySchema) -> PyResult<Vec<u8>> {
        self.inner
            .emit_with_protocol(protocol, &schema.inner)
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
    }

    /// List all registered protocol names.
    fn protocol_names(&self) -> Vec<String> {
        self.inner.protocol_names().map(String::from).collect()
    }

    fn __repr__(&self) -> String {
        format!("AstParserRegistry({} parsers)", self.inner.len())
    }
}

/// Parse a file using the default parser registry (convenience function).
#[pyfunction]
fn parse_source_file(path: &str, content: &[u8]) -> PyResult<PySchema> {
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_file(std::path::Path::new(path), content)
        .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
    Ok(PySchema {
        inner: std::sync::Arc::new(schema),
    })
}

/// Register parse types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyAstParserRegistry>()?;
    parent.add_function(wrap_pyfunction!(parse_source_file, parent)?)?;
    Ok(())
}
