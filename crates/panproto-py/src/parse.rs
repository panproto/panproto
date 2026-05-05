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
    /// pairs. The `*_ptr` values are raw C pointers cast to integers;
    /// the companion is responsible for ensuring the underlying
    /// memory has process-lifetime extent (`&'static` in Rust terms).
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

/// Process-wide cache of leaked metadata strings.
///
/// `panproto-parse::ParserRegistry::register_external_grammar` requires
/// `&'static` references for the grammar's name, extension list, and
/// byte payloads. The simplest way to obtain those at this FFI boundary
/// is `Box::leak`. Without a cache, every call to
/// `AstParserRegistry()` would leak fresh allocations for the same set
/// of grammars; long-running processes that construct registries
/// repeatedly would grow unboundedly.
///
/// The cache keys on grammar name and stores the allocated `&'static`
/// references (`name` and the per-name `extensions` slice). On repeat
/// registrations of the same grammar, the cached references are reused
/// and the new allocation is dropped.
fn leaked_metadata_cache()
-> &'static std::sync::Mutex<rustc_hash::FxHashMap<String, LeakedMetadata>> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<rustc_hash::FxHashMap<String, LeakedMetadata>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(rustc_hash::FxHashMap::default()))
}

#[derive(Clone, Copy)]
struct LeakedMetadata {
    name: &'static str,
    extensions: &'static [&'static str],
}

/// Resolve `name`/`extensions` into the leaked-`&'static` form
/// `register_external_grammar` requires, deduplicating against the
/// process-wide cache so repeat registrations of the same grammar
/// don't accumulate allocations.
fn leaked_metadata_for(name: &str, extensions: Vec<String>) -> PyResult<LeakedMetadata> {
    let mut cache = leaked_metadata_cache().lock().map_err(|e| {
        crate::error::PanprotoError::new_err(format!("metadata cache poisoned: {e}"))
    })?;
    if let Some(cached) = cache.get(name) {
        return Ok(*cached);
    }
    let leaked_name: &'static str = Box::leak(name.to_owned().into_boxed_str());
    let leaked_exts: Vec<&'static str> = extensions
        .into_iter()
        .map(|e| &*Box::leak(e.into_boxed_str()))
        .collect();
    let leaked_extensions: &'static [&'static str] = Box::leak(leaked_exts.into_boxed_slice());
    let entry = LeakedMetadata {
        name: leaked_name,
        extensions: leaked_extensions,
    };
    cache.insert(name.to_owned(), entry);
    drop(cache);
    Ok(entry)
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
#[allow(unsafe_code)]
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

    let language_ptr = pop_usize("language_ptr")?;
    let node_types_ptr = pop_usize("node_types_ptr")?;
    let node_types_len = pop_usize("node_types_len")?;
    let tags_query_ptr = pop_opt_usize("tags_query_ptr")?;
    let tags_query_len = pop_opt_usize("tags_query_len")?.unwrap_or(0);
    let grammar_json_ptr = pop_opt_usize("grammar_json_ptr")?;
    let grammar_json_len = pop_opt_usize("grammar_json_len")?.unwrap_or(0);

    // Reject obvious null-pointer payloads up front. A NULL
    // `language_ptr` would transmute into a `Language` wrapping a NULL
    // `*const TSLanguage`; tree-sitter would then null-deref inside C
    // when the parser is queried. Same logic for the `node_types_*`
    // pair: tree-sitter's theory extractor reads from this slice on
    // construction and a NULL with non-zero length would segfault.
    if language_ptr == 0 {
        return Err(PyValueError::new_err(format!(
            "grammar {name:?}: language_ptr is null"
        )));
    }
    if node_types_ptr == 0 || node_types_len == 0 {
        return Err(PyValueError::new_err(format!(
            "grammar {name:?}: node_types pointer/length is null/zero"
        )));
    }

    let LeakedMetadata {
        name: leaked_name,
        extensions: leaked_extensions,
    } = leaked_metadata_for(&name, extensions)?;
    let leaked_extensions: Vec<&'static str> = leaked_extensions.to_vec();
    let language: tree_sitter::Language = unsafe {
        // Cast through usize → *const c_void → tree_sitter::Language.
        // The conversion mirrors how wasm-bindgen and other FFI shims
        // cross cdylib boundaries: tree_sitter::Language is a
        // transparent wrapper around a raw pointer.
        std::mem::transmute::<usize, tree_sitter::Language>(language_ptr)
    };
    let node_types: &'static [u8] =
        unsafe { std::slice::from_raw_parts(node_types_ptr as *const u8, node_types_len) };
    let tags_query: Option<&'static str> = tags_query_ptr.map(|p| unsafe {
        let slice = std::slice::from_raw_parts(p as *const u8, tags_query_len);
        std::str::from_utf8_unchecked(slice)
    });
    let grammar_json: Option<&'static [u8]> = grammar_json_ptr
        .map(|p| unsafe { std::slice::from_raw_parts(p as *const u8, grammar_json_len) });

    reg.register_external_grammar(
        leaked_name,
        leaked_extensions,
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
