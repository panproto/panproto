//! Python bindings for full-AST tree-sitter parsing.

use std::sync::Arc;

use pyo3::prelude::*;

use panproto_parse::{
    ParseEmitLens, ParserRegistry, check_emit_parse, check_parse_emit, edge_multiset,
    kind_multiset, strip_complement,
};

use crate::convert;
use crate::schema::PySchema;

/// Registry of full-AST parsers for all supported languages.
///
/// Wraps [`ParserRegistry`] from `panproto-parse`, providing parse
/// (source -> Schema) and emit (Schema -> source) operations.
#[pyclass(name = "AstParserRegistry", module = "panproto._native")]
pub struct PyAstParserRegistry {
    inner: Arc<ParserRegistry>,
}

#[pymethods]
impl PyAstParserRegistry {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(ParserRegistry::new()),
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

    /// Render a by-construction schema to source bytes via the
    /// grammar.json production walker. Unlike :meth:`emit`, does not
    /// require the schema to carry parse-derived byte positions or
    /// interstitial constraints.
    fn emit_pretty(&self, protocol: &str, schema: &PySchema) -> PyResult<Vec<u8>> {
        self.inner
            .emit_pretty_with_protocol(protocol, &schema.inner)
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
    }

    /// Build a parse/emit lens for ``protocol`` against this registry.
    fn lens(&self, protocol: &str) -> PyParseEmitLens {
        PyParseEmitLens {
            registry: Arc::clone(&self.inner),
            protocol: protocol.to_owned(),
        }
    }

    /// List all registered protocol names.
    fn protocol_names(&self) -> Vec<String> {
        self.inner.protocol_names().map(String::from).collect()
    }

    fn __repr__(&self) -> String {
        format!("AstParserRegistry({} parsers)", self.inner.len())
    }
}

/// Asymmetric parse/emit lens for a single protocol.
///
/// Wraps :class:`panproto_parse::ParseEmitLens`. Two laws are
/// machine-checkable on concrete inputs:
///
/// * ``check_emit_parse(schema)`` verifies the `EmitParse` retraction
///   (``parse(emit(s)) ≅ s`` modulo byte positions).
/// * ``check_parse_emit(bytes)`` verifies the `ParseEmit` stability law
///   (``emit(parse(b)) == b`` byte-for-byte when ``b`` is parseable).
#[pyclass(name = "ParseEmitLens", module = "panproto._native")]
pub struct PyParseEmitLens {
    registry: Arc<ParserRegistry>,
    protocol: String,
}

#[pymethods]
impl PyParseEmitLens {
    /// Forward direction: source bytes → schema.
    fn parse(&self, source: &[u8]) -> PyResult<PySchema> {
        let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
        let schema = lens
            .parse(source)
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
        Ok(PySchema {
            inner: Arc::new(schema),
        })
    }

    /// Backward direction: schema → canonical source bytes (no complement).
    fn emit(&self, schema: &PySchema) -> PyResult<Vec<u8>> {
        let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
        lens.emit(&schema.inner)
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
    }

    /// Verify the `EmitParse` retraction on ``schema``. Returns
    /// ``None`` on success, or a human-readable string describing the
    /// divergence on failure.
    fn check_emit_parse(&self, schema: &PySchema) -> Option<String> {
        let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
        check_emit_parse(&lens, &schema.inner)
            .err()
            .map(|e| e.to_string())
    }

    /// Verify the `ParseEmit` stability law on ``bytes``. Returns
    /// ``None`` on success, or a human-readable string describing the
    /// divergence on failure.
    fn check_parse_emit(&self, bytes: &[u8]) -> Option<String> {
        let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
        check_parse_emit(&lens, bytes).err().map(|e| e.to_string())
    }

    /// Strip byte-position constraints from a schema, returning a copy.
    /// Useful for comparing by-construction schemas to parse-derived ones.
    #[staticmethod]
    fn strip_complement(schema: &PySchema) -> PySchema {
        let mut copy = schema.inner.as_ref().clone();
        strip_complement(&mut copy);
        PySchema {
            inner: Arc::new(copy),
        }
    }

    /// Vertex-kind multiset of a schema (one half of the retraction witness).
    #[staticmethod]
    fn kind_multiset(py: Python<'_>, schema: &PySchema) -> PyResult<PyObject> {
        let map = kind_multiset(&schema.inner);
        convert::to_python(py, &map)
    }

    /// Edge-shape multiset over ``(src_kind, edge_kind, tgt_kind)`` triples.
    #[staticmethod]
    fn edge_multiset(py: Python<'_>, schema: &PySchema) -> PyResult<PyObject> {
        let map = edge_multiset(&schema.inner);
        let entries: Vec<((String, String, String), usize)> = map.into_iter().collect();
        convert::to_python(py, &entries)
    }

    fn __repr__(&self) -> String {
        format!("ParseEmitLens(protocol={})", self.protocol)
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

/// List all available tree-sitter grammar languages.
///
/// Returns the names of all grammars enabled by feature flags.
/// With ``group-all``, this is 240+ languages.
#[pyfunction]
fn available_grammars() -> Vec<String> {
    panproto_grammars::grammars()
        .into_iter()
        .map(|g| g.name.to_owned())
        .collect()
}

/// Register parse types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyAstParserRegistry>()?;
    parent.add_class::<PyParseEmitLens>()?;
    parent.add_function(wrap_pyfunction!(parse_source_file, parent)?)?;
    parent.add_function(wrap_pyfunction!(available_grammars, parent)?)?;
    Ok(())
}
