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
///
/// Companion grammar packages (`panproto-grammars-*`) inject their
/// grammars at construction time via the `extra_grammars` argument.
/// The Python wrapper class in `panproto/__init__.py` discovers
/// companions through `importlib.metadata.entry_points` and threads
/// them through here.
#[pyclass(name = "AstParserRegistry", module = "panproto._native")]
pub struct PyAstParserRegistry {
    inner: Arc<ParserRegistry>,
}

#[pymethods]
impl PyAstParserRegistry {
    /// Construct a registry populated with all built-in grammars and
    /// any externally-supplied grammars from companion packages.
    ///
    /// `extra_grammars`, when supplied, is a list of dicts with the
    /// keys produced by a companion's `grammars_metadata()` function:
    /// `name`, `extensions`, `language_ptr`, `node_types_ptr`,
    /// `node_types_len`, and the optional `tags_query_ptr` /
    /// `tags_query_len` / `grammar_json_ptr` / `grammar_json_len`
    /// pairs. The `*_ptr` values are raw C pointers cast to integers,
    /// `language_ptr` being the `TSLanguage *` a `tree_sitter_<name>()`
    /// entry point returns rather than the address of that function;
    /// the companion is responsible for ensuring the underlying memory
    /// has process-lifetime extent (`&'static` in Rust terms). The
    /// `tags_query_*` bytes are checked for UTF-8 before use; the rest
    /// are read on the companion's word.
    #[new]
    #[pyo3(signature = (extra_grammars = None))]
    fn new(extra_grammars: Option<Vec<Bound<'_, pyo3::types::PyDict>>>) -> Self {
        let mut reg = ParserRegistry::new();
        if let Some(extras) = extra_grammars {
            for entry in extras {
                // A single broken grammar (e.g. an upstream
                // node-types.json with an invalid entry) shouldn't take
                // down the whole construction. The built-in
                // `ParserRegistry::new()` already swallows per-grammar
                // failures the same way. Emit a Python warning so the
                // dropped grammar is observable.
                if let Err(err) = register_external_from_metadata(&mut reg, &entry) {
                    let name = entry
                        .get_item("name")
                        .ok()
                        .flatten()
                        .and_then(|v| v.extract::<String>().ok())
                        .unwrap_or_else(|| "<unknown>".to_owned());
                    let msg =
                        format!("panproto: companion grammar {name:?} failed to register: {err}");
                    let py = entry.py();
                    // `CString::new` rejects strings containing NUL
                    // bytes; on rejection we fall back to an empty
                    // CString and the user sees an empty warning
                    // body. The condition is unreachable in practice
                    // (grammar names are alphanumeric and the failure
                    // message is a Display formatter output, neither
                    // of which produces NULs), so we leave this as
                    // a silent best-effort rather than failing
                    // construction over a broken warning.
                    let _ = pyo3::PyErr::warn(
                        py,
                        &py.get_type::<pyo3::exceptions::PyRuntimeWarning>(),
                        std::ffi::CString::new(msg).unwrap_or_default().as_c_str(),
                        1,
                    );
                }
            }
        }
        Self {
            inner: Arc::new(reg),
        }
    }

    /// Parse a source file into a full-AST schema.
    /// The language is auto-detected from the file extension.
    ///
    /// Runs with the GIL released, so parsing several files from a
    /// Python thread pool uses several cores.
    fn parse_file(&self, py: Python<'_>, path: &str, content: &[u8]) -> PyResult<PySchema> {
        let schema = py
            .detach(|| self.inner.parse_file(std::path::Path::new(path), content))
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
        Ok(PySchema {
            inner: std::sync::Arc::new(schema),
        })
    }

    /// Parse source code with a specific protocol name.
    ///
    /// Runs with the GIL released, so parsing several sources from a
    /// Python thread pool uses several cores.
    fn parse_with_protocol(
        &self,
        py: Python<'_>,
        protocol: &str,
        content: &[u8],
        file_path: &str,
    ) -> PyResult<PySchema> {
        let schema = py
            .detach(|| self.inner.parse_with_protocol(protocol, content, file_path))
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
    ///
    /// Runs with the GIL released.
    fn emit(&self, py: Python<'_>, protocol: &str, schema: &PySchema) -> PyResult<Vec<u8>> {
        py.detach(|| self.inner.emit_with_protocol(protocol, &schema.inner))
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
    }

    /// Render a by-construction schema to source bytes via the
    /// grammar.json production walker. Unlike :meth:`emit`, does not
    /// require the schema to carry parse-derived byte positions or
    /// interstitial constraints.
    ///
    /// Runs with the GIL released.
    fn emit_pretty(&self, py: Python<'_>, protocol: &str, schema: &PySchema) -> PyResult<Vec<u8>> {
        py.detach(|| {
            self.inner
                .emit_pretty_with_protocol(protocol, &schema.inner)
        })
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

    /// Override (or insert) a grammar registration at runtime.
    ///
    /// Intended for grammar-author workflows where a grammar's
    /// ``parser.c`` / ``grammar.json`` / ``node-types.json`` are evolving
    /// outside the panproto release cadence. The caller compiles the
    /// grammar themselves (typically via ``tree-sitter build``) and
    /// loads the resulting shared library; ``language_ptr`` is the
    /// ``TSLanguage *`` that library's ``tree_sitter_<name>()`` function
    /// **returns**, cast to an integer, not the address of the function
    /// itself. With ``ctypes`` that is
    /// ``ctypes.cast(lib.tree_sitter_<name>(), ctypes.c_void_p).value``
    /// after declaring ``lib.tree_sitter_<name>.restype = ctypes.c_void_p``;
    /// passing the function's own address instead makes tree-sitter read
    /// a language struct out of executable code. The byte payloads are
    /// owned by Python here and leaked into ``'static`` storage on the
    /// Rust side.
    ///
    /// If a parser is already registered under ``name``, it is dropped
    /// first (along with any extension mappings that targeted it); the
    /// new grammar's ``extensions`` are then bound.
    ///
    /// Cannot run while any :class:`ParseEmitLens` produced by
    /// :meth:`lens` is alive (those clone the registry's underlying
    /// reference-counted handle). Drop outstanding lens handles, or
    /// construct a fresh registry, before calling.
    #[pyo3(signature = (name, extensions, language_ptr, node_types, tags_query = None, grammar_json = None))]
    fn override_grammar(
        &mut self,
        name: String,
        extensions: Vec<String>,
        language_ptr: usize,
        node_types: Vec<u8>,
        tags_query: Option<String>,
        grammar_json: Option<Vec<u8>>,
    ) -> PyResult<()> {
        use pyo3::exceptions::PyValueError;
        if node_types.is_empty() {
            return Err(PyValueError::new_err(format!(
                "grammar {name:?}: node_types is empty"
            )));
        }
        let reg = Arc::get_mut(&mut self.inner).ok_or_else(|| {
            crate::error::PanprotoError::new_err(
                "cannot override grammar: registry handle is shared \
                 (drop any open ParseEmitLens first, or construct a fresh \
                 AstParserRegistry)",
            )
        })?;

        // Through the audited boundary, which is the only place this
        // crate reconstructs a language.
        let language = crate::grammar_boundary::language_from_raw_address(&name, language_ptr)?;

        reg.override_grammar(
            name,
            extensions,
            language,
            node_types,
            tags_query,
            grammar_json,
        )
        .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
        Ok(())
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
    ///
    /// Runs with the GIL released.
    fn parse(&self, py: Python<'_>, source: &[u8]) -> PyResult<PySchema> {
        let schema = py
            .detach(|| {
                let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
                lens.parse(source)
            })
            .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))?;
        Ok(PySchema {
            inner: Arc::new(schema),
        })
    }

    /// Backward direction: schema → canonical source bytes (no complement).
    ///
    /// Runs with the GIL released.
    fn emit(&self, py: Python<'_>, schema: &PySchema) -> PyResult<Vec<u8>> {
        py.detach(|| {
            let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
            lens.emit(&schema.inner)
        })
        .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
    }

    /// Verify the `EmitParse` retraction on ``schema``. Returns
    /// ``None`` on success, or a human-readable string describing the
    /// divergence on failure.
    ///
    /// Runs with the GIL released.
    fn check_emit_parse(&self, py: Python<'_>, schema: &PySchema) -> Option<String> {
        py.detach(|| {
            let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
            check_emit_parse(&lens, &schema.inner)
                .err()
                .map(|e| e.to_string())
        })
    }

    /// Verify the `ParseEmit` stability law on ``bytes``. Returns
    /// ``None`` on success, or a human-readable string describing the
    /// divergence on failure.
    ///
    /// Runs with the GIL released.
    fn check_parse_emit(&self, py: Python<'_>, bytes: &[u8]) -> Option<String> {
        py.detach(|| {
            let lens = ParseEmitLens::new(&self.registry, self.protocol.clone());
            check_parse_emit(&lens, bytes).err().map(|e| e.to_string())
        })
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
    fn kind_multiset(py: Python<'_>, schema: &PySchema) -> PyResult<Py<PyAny>> {
        let map = kind_multiset(&schema.inner);
        convert::to_python(py, &map)
    }

    /// Edge-shape multiset over ``(src_kind, edge_kind, tgt_kind)`` triples.
    #[staticmethod]
    fn edge_multiset(py: Python<'_>, schema: &PySchema) -> PyResult<Py<PyAny>> {
        let map = edge_multiset(&schema.inner);
        let entries: Vec<((String, String, String), usize)> = map.into_iter().collect();
        convert::to_python(py, &entries)
    }

    fn __repr__(&self) -> String {
        format!("ParseEmitLens(protocol={})", self.protocol)
    }
}

/// Parse a file using the default parser registry (convenience function).
///
/// Runs with the GIL released; building the registry dominates, so
/// prefer :class:`AstParserRegistry` when parsing more than one file.
#[pyfunction]
fn parse_source_file(py: Python<'_>, path: &str, content: &[u8]) -> PyResult<PySchema> {
    let schema = py
        .detach(|| {
            let registry = ParserRegistry::new();
            registry.parse_file(std::path::Path::new(path), content)
        })
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

/// Decode a single `extra_grammars` dict and register the corresponding
/// grammar with `reg`.
///
/// The companion package owns the underlying byte buffers (they live in
/// the companion cdylib's static memory for the process lifetime), so
/// the integer-pointer values we receive are safe to widen to
/// `&'static` at this boundary. Mistyped or short-lived pointers from a
/// non-companion caller would corrupt this registry; this function is
/// the trust boundary.
fn register_external_from_metadata(
    reg: &mut ParserRegistry,
    entry: &Bound<'_, pyo3::types::PyDict>,
) -> PyResult<()> {
    use pyo3::exceptions::PyValueError;
    use pyo3::types::PyDictMethods;

    let pop_str = |key: &str| -> PyResult<String> {
        entry
            .get_item(key)?
            .ok_or_else(|| {
                PyValueError::new_err(format!("missing key {key:?} in grammar metadata"))
            })?
            .extract::<String>()
    };
    let pop_usize = |key: &str| -> PyResult<usize> {
        entry
            .get_item(key)?
            .ok_or_else(|| {
                PyValueError::new_err(format!("missing key {key:?} in grammar metadata"))
            })?
            .extract::<usize>()
    };
    let pop_opt_usize = |key: &str| -> PyResult<Option<usize>> {
        match entry.get_item(key)? {
            Some(v) => {
                if v.is_none() {
                    Ok(None)
                } else {
                    Ok(Some(v.extract::<usize>()?))
                }
            }
            None => Ok(None),
        }
    };

    let name = pop_str("name")?;
    let extensions: Vec<String> = entry
        .get_item("extensions")?
        .ok_or_else(|| PyValueError::new_err("missing key \"extensions\" in grammar metadata"))?
        .extract()?;

    // Skip if the grammar is already registered. Two paths land us
    // here: (1) a companion's `all`-style pack contains a name the
    // built-in `ParserRegistry::new()` already added, and
    // (2) two different companions advertise the same grammar (the
    // umbrella `panproto-grammars-all` overlaps every per-group pack).
    // Either way the second registration would replace an identical
    // entry and leak fresh `&'static` allocations for nothing; an
    // early return preserves the first registration and avoids the
    // leak.
    if reg.has_parser(&name) {
        return Ok(());
    }

    // Payloads first, language last. Everything below can be checked
    // without touching a raw pointer as a pointer, and a grammar that
    // fails one of these checks is refused before anything reconstructs
    // a `Language` from an address the caller supplied. Copying also
    // means a payload that is about to be rejected is never aliased
    // into `'static` storage.
    let node_types_ptr = pop_usize("node_types_ptr")?;
    let node_types_len = pop_usize("node_types_len")?;
    if node_types_len == 0 {
        return Err(PyValueError::new_err(format!(
            "grammar {name:?}: node_types is empty"
        )));
    }
    let node_types =
        crate::grammar_boundary::payload(&name, "node_types", node_types_ptr, node_types_len)?;

    // The tags query is the one payload that becomes a `str`, so it is
    // checked for UTF-8 here. An unchecked conversion would hand
    // tree-sitter's query compiler a `str` violating its invariant,
    // which is a segmentation fault rather than an error.
    let tags_query = crate::grammar_boundary::text_payload(
        &name,
        "tags_query",
        pop_opt_usize("tags_query_ptr")?.unwrap_or(0),
        pop_opt_usize("tags_query_len")?.unwrap_or(0),
    )?;

    let grammar_json_bytes = crate::grammar_boundary::payload(
        &name,
        "grammar_json",
        pop_opt_usize("grammar_json_ptr")?.unwrap_or(0),
        pop_opt_usize("grammar_json_len")?.unwrap_or(0),
    )?;
    let grammar_json = (!grammar_json_bytes.is_empty()).then_some(grammar_json_bytes);

    // The language may arrive as a capsule, which carries a name this
    // module checks and which keeps its provider alive, or as a bare
    // address, which does neither and exists because ten published
    // companion packages send one. The capsule wins when both appear.
    let language = match entry.get_item("language_capsule")? {
        Some(capsule) if !capsule.is_none() => {
            crate::grammar_boundary::language_from_capsule(&name, &capsule)?
        }
        _ => crate::grammar_boundary::language_from_raw_address(&name, pop_usize("language_ptr")?)?,
    };

    // Registration is the last step, so a payload that failed any check
    // above leaves the registry exactly as it was rather than
    // half-populated.
    reg.register_external_grammar_owned(
        name,
        extensions,
        language,
        node_types,
        tags_query,
        grammar_json,
    )
    .map_err(|e| crate::error::PanprotoError::new_err(e.to_string()))
}

/// Register parse types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyAstParserRegistry>()?;
    parent.add_class::<PyParseEmitLens>()?;
    parent.add_function(wrap_pyfunction!(parse_source_file, parent)?)?;
    parent.add_function(wrap_pyfunction!(available_grammars, parent)?)?;
    Ok(())
}
