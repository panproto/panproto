//! Python bindings for panproto bidirectional lenses.
//!
//! Wraps `panproto-lens`: asymmetric lenses with get/put, lens law
//! verification, auto-generation, and composition. The lens `Complement`
//! type (from `panproto-lens`) is Serialize-able, unlike the `inst`
//! version.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::gat::Name;
use panproto_core::inst::FieldTransform;
use panproto_core::lens::{self, AutoLensConfig, Complement, Lens, Stringency};

/// Parse a Python-side stringency string into the engine [`Stringency`].
///
/// Accepts `"strict" | "balanced" | "lenient" | "exploratory"`
/// (case-insensitive). `None` keeps the engine's default.
fn parse_stringency(s: Option<&str>) -> PyResult<Option<Stringency>> {
    // Trim surrounding whitespace and treat an empty string as unset so
    // Python behaves the same as the WASM/TS side. Without this, values
    // like `" strict "` or `""` would misparse as "unknown stringency",
    // breaking cross-SDK parity for the same JSON payload.
    let trimmed = s.map(str::trim).filter(|s| !s.is_empty());
    match trimmed.map(str::to_ascii_lowercase).as_deref() {
        None => Ok(None),
        Some("strict") => Ok(Some(Stringency::Strict)),
        Some("balanced") => Ok(Some(Stringency::Balanced)),
        Some("lenient") => Ok(Some(Stringency::Lenient)),
        Some("exploratory") => Ok(Some(Stringency::Exploratory)),
        Some(other) => Err(crate::error::LensError::new_err(format!(
            "unknown stringency '{other}'; expected one of strict, balanced, lenient, exploratory"
        ))),
    }
}

use crate::convert;
use crate::inst::PyInstance;
use crate::schema::{PyProtocol, PySchema};

/// An asymmetric lens with compiled migration and schema references.
///
/// Provides bidirectional transformations: ``get`` projects an instance
/// through the lens (producing a view and complement), and ``put``
/// reconstructs the original from a modified view and the complement.
///
/// ``Lens`` is not ``Clone`` in Rust; it is wrapped in ``Arc`` here.
#[pyclass(name = "Lens", frozen, module = "panproto._native")]
pub struct PyLens {
    pub(crate) inner: Arc<Lens>,
}

/// The complement from a ``get`` operation.
///
/// Stores dropped nodes, arcs, and contraction choices needed by ``put``
/// to reconstruct the original source instance.
#[pyclass(name = "Complement", frozen, module = "panproto._native")]
pub struct PyComplement {
    pub(crate) inner: Complement,
}

#[pymethods]
impl PyComplement {
    /// Number of dropped nodes.
    #[getter]
    fn dropped_node_count(&self) -> usize {
        self.inner.dropped_nodes.len()
    }

    /// Number of dropped arcs.
    #[getter]
    fn dropped_arc_count(&self) -> usize {
        self.inner.dropped_arcs.len()
    }

    /// Serialize the complement to a Python dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "Complement(dropped_nodes={}, dropped_arcs={})",
            self.inner.dropped_nodes.len(),
            self.inner.dropped_arcs.len()
        )
    }
}

#[pymethods]
impl PyLens {
    /// Project an instance through the lens.
    ///
    /// Returns
    /// -------
    /// tuple[Instance, Complement]
    ///     The view instance and the complement (data needed by ``put``
    ///     to reconstruct the original).
    fn get(&self, instance: &PyInstance) -> PyResult<(PyInstance, PyComplement)> {
        let (view, complement) = lens::get(&self.inner, &instance.inner)
            .map_err(|e| crate::error::LensError::new_err(format!("get failed: {e}")))?;

        let view_inst = PyInstance {
            inner: view,
            schema: Arc::new(self.inner.tgt_schema.clone()),
        };
        Ok((view_inst, PyComplement { inner: complement }))
    }

    /// Reconstruct an instance from a view and complement.
    ///
    /// Parameters
    /// ----------
    /// view : Instance
    ///     The (possibly modified) view.
    /// complement : Complement
    ///     The complement from a prior ``get`` call.
    fn put(&self, view: &PyInstance, complement: &PyComplement) -> PyResult<PyInstance> {
        let restored = lens::put(&self.inner, &view.inner, &complement.inner)
            .map_err(|e| crate::error::LensError::new_err(format!("put failed: {e}")))?;
        Ok(PyInstance {
            inner: restored,
            schema: Arc::new(self.inner.src_schema.clone()),
        })
    }

    /// Check both `GetPut` and `PutGet` lens laws on a test instance.
    ///
    /// Raises
    /// ------
    /// `LensError`
    ///     If either law is violated, with details in the message.
    fn check_laws(&self, instance: &PyInstance) -> PyResult<()> {
        lens::check_laws(&self.inner, &instance.inner)
            .map_err(|e| crate::error::LensError::new_err(format!("law violation: {e}")))
    }

    /// Check the `GetPut` law: ``put(get(s), complement(s)) = s``.
    fn check_get_put(&self, instance: &PyInstance) -> PyResult<()> {
        lens::check_get_put(&self.inner, &instance.inner)
            .map_err(|e| crate::error::LensError::new_err(format!("GetPut violation: {e}")))
    }

    /// Check the `PutGet` law: ``get(put(v, c)) = v``.
    fn check_put_get(&self, instance: &PyInstance) -> PyResult<()> {
        lens::check_put_get(&self.inner, &instance.inner)
            .map_err(|e| crate::error::LensError::new_err(format!("PutGet violation: {e}")))
    }

    /// Compose this lens with another: ``self ; other``.
    fn compose(&self, other: &Self) -> PyResult<Self> {
        let composed = lens::compose(&self.inner, &other.inner)
            .map_err(|e| crate::error::LensError::new_err(format!("compose failed: {e}")))?;
        Ok(Self {
            inner: Arc::new(composed),
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Lens(src_vertices={}, tgt_vertices={})",
            self.inner.src_schema.vertex_count(),
            self.inner.tgt_schema.vertex_count()
        )
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Auto-generate a lens between two schemas.
///
/// Runs the alignment strategies enabled by ``stringency`` (alias
/// dictionary, token similarity, etc.), seeds the CSP solver, and
/// returns the best validated morphism along with its alignment
/// quality score in ``[0.0, 1.0]``.
///
/// Parameters
/// ----------
/// `src_schema` : Schema
///     Source schema.
/// `tgt_schema` : Schema
///     Target schema.
/// protocol : Protocol
///     Protocol for the schemas.
/// stringency : str, optional
///     One of ``"strict"``, ``"balanced"``, ``"lenient"``,
///     ``"exploratory"`` (case-insensitive). Defaults to ``"balanced"``
///     when unspecified.
///
/// Returns
/// -------
/// tuple[Lens, float, list[dict]]
///     The generated lens, the alignment quality score (0.0 to 1.0),
///     and the list of coerce proposals emitted at `"exploratory"`
///     stringency. Each proposal dict has keys ``src``, ``tgt``,
///     ``witness_name``, ``witness_class``, ``confidence``, and
///     ``explanation``. The list is empty at every tier below
///     `"exploratory"`.
#[pyfunction]
#[pyo3(signature = (src_schema, tgt_schema, protocol, stringency=None))]
pub fn auto_generate_lens(
    py: Python<'_>,
    src_schema: &PySchema,
    tgt_schema: &PySchema,
    protocol: &PyProtocol,
    stringency: Option<&str>,
) -> PyResult<(PyLens, f64, Py<PyAny>)> {
    let mut config = AutoLensConfig::default();
    if let Some(s) = parse_stringency(stringency)? {
        config.stringency = s;
    }
    // Alignment search plus lens construction over both schemas; no
    // Python state is touched, so the GIL is released.
    let result = py
        .detach(|| {
            lens::auto_generate(
                &src_schema.inner,
                &tgt_schema.inner,
                &protocol.inner,
                &config,
            )
        })
        .map_err(|e| crate::error::LensError::new_err(format!("auto-generate failed: {e}")))?;
    let lens = PyLens {
        inner: Arc::new(result.lens),
    };
    let proposals_json = coerce_proposals_to_json(&result.coerce_proposals);
    let proposals_py = convert::to_python(py, &proposals_json)?;
    Ok((lens, result.alignment_quality, proposals_py))
}

/// Serialize coerce proposals as a JSON array for `to_python`.
fn coerce_proposals_to_json(
    proposals: &[panproto_core::mig::align::CoerceAnchor],
) -> serde_json::Value {
    let entries: Vec<serde_json::Value> = proposals
        .iter()
        .map(|p| {
            serde_json::json!({
                "src": p.anchor.src.as_str(),
                "tgt": p.anchor.tgt.as_str(),
                "witness_name": p.witness_name,
                "witness_class": p.witness_class,
                "confidence": p.anchor.confidence,
                "explanation": p.anchor.explanation,
            })
        })
        .collect();
    serde_json::Value::Array(entries)
}

// ---------------------------------------------------------------------------
// ProtolensChain: schema-independent lens family
// ---------------------------------------------------------------------------

/// A schema-independent lens family (protolens chain).
///
/// A chain of protolens steps that can be instantiated against any
/// matching schema to produce a concrete ``Lens``. Supports composition,
/// fusion, and JSON serialization.
#[pyclass(name = "ProtolensChain", frozen, module = "panproto._native")]
pub struct PyProtolensChain {
    pub(crate) inner: Arc<lens::ProtolensChain>,
    field_transforms: Arc<HashMap<Name, Vec<FieldTransform>>>,
    stages: Arc<Vec<panproto_core::lens_dsl::steps::CompiledStage>>,
}

/// Serialized Python representation of a compiled protolens chain.
///
/// `steps` and `field_transforms` retain the previous flat summaries. `stages`
/// records the execution order and is authoritative when present. Each added
/// member defaults to empty so legacy transform-free and flat-transform JSON
/// remain readable.
#[derive(serde::Deserialize, serde::Serialize)]
struct SerializedProtolensChain {
    steps: Vec<lens::Protolens>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    field_transforms: HashMap<Name, Vec<FieldTransform>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    stages: Vec<panproto_core::lens_dsl::steps::CompiledStage>,
}

impl PyProtolensChain {
    fn from_chain(chain: lens::ProtolensChain) -> Self {
        Self::from_parts(chain, HashMap::new())
    }

    fn from_parts(
        chain: lens::ProtolensChain,
        field_transforms: HashMap<Name, Vec<FieldTransform>>,
    ) -> Self {
        let stages = if chain.steps.is_empty() && field_transforms.is_empty() {
            Vec::new()
        } else {
            vec![panproto_core::lens_dsl::steps::CompiledStage {
                chain,
                field_transforms,
            }]
        };
        Self::from_stages(stages)
    }

    fn from_stages(stages: Vec<panproto_core::lens_dsl::steps::CompiledStage>) -> Self {
        let mut steps = Vec::new();
        let mut field_transforms = HashMap::new();
        for stage in &stages {
            steps.extend(stage.chain.steps.iter().cloned());
            extend_field_transforms(&mut field_transforms, &stage.field_transforms);
        }
        Self {
            inner: Arc::new(lens::ProtolensChain::new(steps)),
            field_transforms: Arc::new(field_transforms),
            stages: Arc::new(stages),
        }
    }

    fn from_compiled(compiled: panproto_core::lens_dsl::CompiledLens) -> Self {
        Self::from_stages(compiled.stages)
    }

    fn from_auto_result(result: lens::AutoLensResult) -> Self {
        Self::from_parts(result.chain, result.lens.compiled.field_transforms)
    }
}

#[pymethods]
impl PyProtolensChain {
    /// Auto-generate a protolens chain between two schemas.
    #[staticmethod]
    #[pyo3(signature = (src_schema, tgt_schema, protocol, stringency=None))]
    fn auto_generate(
        py: Python<'_>,
        src_schema: &PySchema,
        tgt_schema: &PySchema,
        protocol: &PyProtocol,
        stringency: Option<&str>,
    ) -> PyResult<Self> {
        let mut config = AutoLensConfig::default();
        if let Some(s) = parse_stringency(stringency)? {
            config.stringency = s;
        }
        let result = py
            .detach(|| {
                lens::auto_generate(
                    &src_schema.inner,
                    &tgt_schema.inner,
                    &protocol.inner,
                    &config,
                )
            })
            .map_err(|e| crate::error::LensError::new_err(format!("auto-generate failed: {e}")))?;
        Ok(Self::from_auto_result(result))
    }

    /// Auto-generate with morphism hints (vertex correspondences).
    #[staticmethod]
    #[pyo3(signature = (src_schema, tgt_schema, protocol, hints, stringency=None))]
    #[allow(clippy::needless_pass_by_value)]
    fn auto_generate_with_hints(
        py: Python<'_>,
        src_schema: &PySchema,
        tgt_schema: &PySchema,
        protocol: &PyProtocol,
        hints: std::collections::HashMap<String, String>,
        stringency: Option<&str>,
    ) -> PyResult<Self> {
        let mut hard_pins = std::collections::HashMap::new();
        for (src, tgt) in &hints {
            hard_pins.insert(Name::from(src.as_str()), Name::from(tgt.as_str()));
        }
        let mut config = AutoLensConfig {
            try_overlap: true,
            search_opts: panproto_core::mig::hom_search::SearchOptions {
                hard_pins,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Some(s) = parse_stringency(stringency)? {
            config.stringency = s;
        }
        let result = py
            .detach(|| {
                lens::auto_generate(
                    &src_schema.inner,
                    &tgt_schema.inner,
                    &protocol.inner,
                    &config,
                )
            })
            .map_err(|e| crate::error::LensError::new_err(format!("auto-generate failed: {e}")))?;
        Ok(Self::from_auto_result(result))
    }

    /// Auto-generate with a full hint specification.
    ///
    /// The ``hints`` dict should have:
    ///   - ``anchors``: ``dict[str, str]`` mapping source to target vertex names
    ///   - ``constraints``: ``list[dict]`` with constraint objects
    ///
    /// Constraints are dicts with a ``type`` key:
    ///   - ``{"type": "scope", "under": "...", "targets": "..."}``
    ///   - ``{"type": "exclude_targets", "vertices": ["..."]}``
    ///   - ``{"type": "exclude_sources", "vertices": ["..."]}``
    #[staticmethod]
    #[allow(clippy::needless_pass_by_value)]
    fn auto_generate_with_hint_spec(
        py: Python<'_>,
        src_schema: &PySchema,
        tgt_schema: &PySchema,
        protocol: &PyProtocol,
        hints: String,
    ) -> PyResult<Self> {
        let hint_spec: panproto_core::lens_dsl::HintSpec = serde_json::from_str(&hints)
            .map_err(|e| crate::error::LensError::new_err(format!("invalid hints JSON: {e}")))?;

        let parts = lens::hint::HintParts {
            anchors: hint_spec.anchors.clone(),
            scope_pairs: hint_spec.scope_pairs(),
            excluded_targets: hint_spec.excluded_target_names(),
            excluded_sources: hint_spec.excluded_source_names(),
            scoring_weights: hint_spec.scoring_weights(),
        };
        let (derived, domain_constraints) =
            lens::hint::resolve_hints(&parts, &src_schema.inner, &tgt_schema.inner);

        let mut config = AutoLensConfig {
            try_overlap: true,
            ..Default::default()
        };
        if let Some(s) = hint_spec.stringency {
            config.stringency = match s {
                panproto_core::lens_dsl::HintStringency::Strict => Stringency::Strict,
                panproto_core::lens_dsl::HintStringency::Balanced => Stringency::Balanced,
                panproto_core::lens_dsl::HintStringency::Lenient => Stringency::Lenient,
                panproto_core::lens_dsl::HintStringency::Exploratory => Stringency::Exploratory,
            };
        }
        for cluster in &hint_spec.alias_clusters {
            config.alias_dict.add_cluster(cluster);
        }

        let result = py
            .detach(|| {
                lens::auto_generate_with_hints(
                    &src_schema.inner,
                    &tgt_schema.inner,
                    &protocol.inner,
                    &config,
                    &derived,
                    &domain_constraints,
                    None,
                )
            })
            .map_err(|e| {
                crate::error::LensError::new_err(format!("auto-generate with hints failed: {e}"))
            })?;

        Ok(Self::from_auto_result(result))
    }

    /// Instantiate against a concrete schema to produce a ``Lens``.
    fn instantiate(&self, schema: &PySchema, protocol: &PyProtocol) -> PyResult<PyLens> {
        let mut lens_obj: Option<Lens> = None;
        for stage in self.stages.iter() {
            let running_schema = lens_obj
                .as_ref()
                .map_or_else(|| schema.inner.as_ref(), |lens| &lens.tgt_schema);
            let mut stage_lens = stage
                .chain
                .instantiate(running_schema, &protocol.inner)
                .map_err(|e| {
                    crate::error::LensError::new_err(format!("instantiate failed: {e}"))
                })?;
            extend_field_transforms(
                &mut stage_lens.compiled.field_transforms,
                &stage.field_transforms,
            );
            lens_obj = Some(match lens_obj {
                Some(accumulated) => lens::compose(&accumulated, &stage_lens).map_err(|e| {
                    crate::error::LensError::new_err(format!("instantiate failed: {e}"))
                })?,
                None => stage_lens,
            });
        }
        let lens_obj = match lens_obj {
            Some(lens) => lens,
            None => lens::ProtolensChain::new(Vec::new())
                .instantiate(&schema.inner, &protocol.inner)
                .map_err(|e| {
                    crate::error::LensError::new_err(format!("instantiate failed: {e}"))
                })?,
        };
        Ok(PyLens {
            inner: Arc::new(lens_obj),
        })
    }

    /// Compose with another chain (vertical composition).
    fn compose(&self, other: &Self) -> Self {
        let mut stages = self.stages.as_ref().clone();
        stages.extend(other.stages.iter().cloned());
        Self::from_stages(stages)
    }

    /// Fuse the structural steps within each ordered execution stage.
    ///
    /// A value-transform boundary remains a separate stage because moving a
    /// transform across that boundary would change the field-name frame in
    /// which its expression is evaluated.
    fn fuse(&self) -> PyResult<Self> {
        let mut stages = Vec::with_capacity(self.stages.len());
        for stage in self.stages.iter() {
            let chain = match stage.chain.steps.as_slice() {
                [] => lens::ProtolensChain::new(Vec::new()),
                [only] => lens::ProtolensChain::new(vec![only.clone()]),
                _ => {
                    let fused = stage.chain.fuse().map_err(|e| {
                        crate::error::LensError::new_err(format!("fuse failed: {e}"))
                    })?;
                    lens::ProtolensChain::new(vec![fused])
                }
            };
            stages.push(panproto_core::lens_dsl::steps::CompiledStage {
                chain,
                field_transforms: stage.field_transforms.clone(),
            });
        }
        Ok(Self::from_stages(stages))
    }

    /// Serialize to JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&SerializedProtolensChain {
            steps: self.inner.steps.clone(),
            field_transforms: self.field_transforms.as_ref().clone(),
            stages: self.stages.as_ref().clone(),
        })
        .map_err(|e| crate::error::LensError::new_err(format!("to_json failed: {e}")))
    }

    /// Deserialize from JSON.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let serialized: SerializedProtolensChain = serde_json::from_str(json)
            .map_err(|e| crate::error::LensError::new_err(format!("from_json failed: {e}")))?;
        if serialized.stages.is_empty() {
            Ok(Self::from_parts(
                lens::ProtolensChain::new(serialized.steps),
                serialized.field_transforms,
            ))
        } else {
            Ok(Self::from_stages(serialized.stages))
        }
    }

    /// Compile a JSON lens-DSL document into a protolens chain.
    ///
    /// Accepts the JSON surface of ``panproto-lens-dsl``: a top-level
    /// document with ``id`` / ``description`` / ``steps`` (and optional
    /// ``constraints`` / ``hints`` / ``preferences``). The
    /// ``body_vertex`` argument is the entry vertex of the source
    /// schema the chain is being authored against; the DSL compiler
    /// uses it to anchor the per-step protolens construction.
    #[staticmethod]
    fn from_dsl_json(source: &str, body_vertex: &str) -> PyResult<Self> {
        let doc = panproto_core::lens_dsl::eval::eval_json(source).map_err(|e| lens_dsl_err(&e))?;
        let compiled = panproto_core::lens_dsl::compile(&doc, body_vertex, &|_| None)
            .map_err(|e| lens_dsl_err(&e))?;
        Ok(Self::from_compiled(compiled))
    }

    /// Compile a YAML lens-DSL document into a protolens chain.
    ///
    /// Same body shape as :meth:`from_dsl_json`, in YAML.
    #[staticmethod]
    fn from_dsl_yaml(source: &str, body_vertex: &str) -> PyResult<Self> {
        let doc = panproto_core::lens_dsl::eval::eval_yaml(source).map_err(|e| lens_dsl_err(&e))?;
        let compiled = panproto_core::lens_dsl::compile(&doc, body_vertex, &|_| None)
            .map_err(|e| lens_dsl_err(&e))?;
        Ok(Self::from_compiled(compiled))
    }

    /// Compile a Nickel lens-DSL document into a protolens chain.
    ///
    /// Same body shape as :meth:`from_dsl_json`, in Nickel.
    /// ``import_paths`` (default empty) extends Nickel's
    /// import-resolution lookup so user-defined modules can be
    /// referenced from ``source``.
    #[staticmethod]
    #[pyo3(signature = (source, body_vertex, import_paths=None))]
    fn from_dsl_nickel(
        source: &str,
        body_vertex: &str,
        import_paths: Option<Vec<std::path::PathBuf>>,
    ) -> PyResult<Self> {
        let paths = import_paths.unwrap_or_default();
        let doc = panproto_core::lens_dsl::eval::eval_nickel(source, &paths)
            .map_err(|e| lens_dsl_err(&e))?;
        let compiled = panproto_core::lens_dsl::compile(&doc, body_vertex, &|_| None)
            .map_err(|e| lens_dsl_err(&e))?;
        Ok(Self::from_compiled(compiled))
    }

    /// Compile a lens-DSL document from a file, dispatching on
    /// extension (``.ncl`` → Nickel, ``.json`` → JSON, ``.yaml`` /
    /// ``.yml`` → YAML).
    ///
    /// Named references in a ``compose`` body resolve against the other
    /// lens documents in the same directory as ``path``.
    #[staticmethod]
    #[allow(clippy::needless_pass_by_value)] // pyo3 #[staticmethod] requires an owned argument here.
    fn from_dsl_path(path: std::path::PathBuf, body_vertex: &str) -> PyResult<Self> {
        let compiled = panproto_core::lens_dsl::load_and_compile(&path, body_vertex)
            .map_err(|e| lens_dsl_err(&e))?;
        Ok(Self::from_compiled(compiled))
    }

    /// Compile a JSON or YAML lens-DSL document, resolving ``compose``
    /// named references against a map of sibling documents.
    ///
    /// ``refs`` maps a lens ``id`` to the source of the lens document
    /// (in the same ``format``). When the compiled document's
    /// ``compose`` body references a lens by ``id``, the matching entry
    /// in ``refs`` is compiled and its chain spliced in. This is the
    /// binding surface for named-reference composition without touching
    /// the filesystem.
    ///
    /// Example
    /// -------
    /// ```python
    /// drop_a = '{"id":"dev.ex.drop-a","source":"s","target":"t",'\
    ///          '"steps":[{"remove_field":"a"}]}'
    /// main = '{"id":"dev.ex.main","source":"s","target":"t",'\
    ///        '"compose":{"mode":"vertical",'\
    ///        '"lenses":[{"ref":"dev.ex.drop-a"},'\
    ///        '{"inline":{"steps":[{"remove_field":"b"}]}}]}}'
    /// chain = ProtolensChain.from_dsl_with_refs(
    ///     main, "json", "record:body", {"dev.ex.drop-a": drop_a})
    /// ```
    #[staticmethod]
    #[allow(clippy::needless_pass_by_value)]
    fn from_dsl_with_refs(
        source: &str,
        format: &str,
        body_vertex: &str,
        refs: std::collections::HashMap<String, String>,
    ) -> PyResult<Self> {
        let parse = |src: &str| match format {
            "json" => panproto_core::lens_dsl::eval::eval_json(src),
            "yaml" | "yml" => panproto_core::lens_dsl::eval::eval_yaml(src),
            other => Err(
                panproto_core::lens_dsl::LensDslError::UnsupportedExtension {
                    ext: other.to_owned(),
                },
            ),
        };

        let doc = parse(source).map_err(|e| lens_dsl_err(&e))?;

        let mut docs_by_id = std::collections::HashMap::new();
        for (id, ref_source) in &refs {
            let ref_doc = parse(ref_source).map_err(|e| lens_dsl_err(&e))?;
            docs_by_id.insert(id.clone(), ref_doc);
        }

        let compiled = panproto_core::lens_dsl::compile_with_refs(&doc, body_vertex, &docs_by_id)
            .map_err(|e| lens_dsl_err(&e))?;
        Ok(Self::from_compiled(compiled))
    }

    /// Number of steps in the chain.
    fn __len__(&self) -> usize {
        self.inner.steps.len()
    }

    fn __repr__(&self) -> String {
        format!("ProtolensChain(steps={})", self.inner.steps.len())
    }
}

// ---------------------------------------------------------------------------
// Combinator functions
// ---------------------------------------------------------------------------

/// Rename a field's JSON property key.
///
/// Parameters
/// ----------
/// parent : str
///     The parent vertex ID.
/// field : str
///     The field's vertex ID (target of the edge from parent).
/// `old_name` : str
///     The current edge label (JSON property key).
/// `new_name` : str
///     The new edge label.
#[pyfunction]
pub fn rename_field(parent: &str, field: &str, old_name: &str, new_name: &str) -> PyProtolensChain {
    PyProtolensChain::from_chain(lens::combinators::rename_field(
        Name::from(parent),
        Name::from(field),
        Name::from(old_name),
        Name::from(new_name),
    ))
}

/// Remove a field (drop sort with edge cascade).
#[pyfunction]
pub fn remove_field(field: &str) -> PyProtolensChain {
    PyProtolensChain::from_chain(lens::combinators::remove_field(Name::from(field)))
}

/// Add a field with a default value.
#[pyfunction]
pub fn add_field(parent: &str, name: &str, kind: &str) -> PyProtolensChain {
    use panproto_core::inst::value::Value;
    PyProtolensChain::from_chain(lens::combinators::add_field(
        Name::from(parent),
        Name::from(name),
        Name::from(kind),
        Value::Null,
    ))
}

/// Hoist a nested field up one level.
#[pyfunction]
pub fn hoist_field(parent: &str, intermediate: &str, child: &str) -> PyProtolensChain {
    PyProtolensChain::from_chain(lens::combinators::hoist_field(
        Name::from(parent),
        Name::from(intermediate),
        Name::from(child),
    ))
}

/// Build a pipeline from multiple protolens chains (vertical composition).
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub fn pipeline(chains: Vec<PyRef<'_, PyProtolensChain>>) -> PyProtolensChain {
    let mut stages = Vec::new();
    for chain in &chains {
        stages.extend(chain.stages.iter().cloned());
    }
    PyProtolensChain::from_stages(stages)
}

fn extend_field_transforms(
    target: &mut HashMap<Name, Vec<FieldTransform>>,
    source: &HashMap<Name, Vec<FieldTransform>>,
) {
    for (anchor, transforms) in source {
        target
            .entry(anchor.clone())
            .or_default()
            .extend(transforms.iter().cloned());
    }
}

/// Auto-generate up to ``top_n`` ranked candidate lenses with per-step
/// explanations.
///
/// Each returned entry is a dict with fields ``quality``, ``coverage``,
/// ``score``, ``strategies_used`` (list of strategy tag strings), and
/// ``steps`` (list of ``{kind, explanation, confidence, strategy}``
/// dicts). The returned list is sorted by descending composite score.
///
/// Parameters
/// ----------
/// `src_schema` : Schema
/// `tgt_schema` : Schema
/// protocol : Protocol
/// `top_n` : int
///     Maximum number of ranked candidates to return. Values < 1 are
///     treated as 1.
/// stringency : str, optional
///     One of ``"strict" | "balanced" | "lenient" | "exploratory"``.
#[pyfunction]
#[pyo3(signature = (src_schema, tgt_schema, protocol, top_n=1, stringency=None))]
pub fn auto_generate_lens_candidates(
    py: Python<'_>,
    src_schema: &PySchema,
    tgt_schema: &PySchema,
    protocol: &PyProtocol,
    top_n: usize,
    stringency: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let mut config = AutoLensConfig::default();
    if let Some(s) = parse_stringency(stringency)? {
        config.stringency = s;
    }
    // Runs the alignment search once per candidate strategy, with the
    // GIL released for the duration.
    let candidates = py
        .detach(|| {
            lens::auto_generate_candidates(
                &src_schema.inner,
                &tgt_schema.inner,
                &protocol.inner,
                &config,
                top_n,
            )
        })
        .map_err(|e| {
            crate::error::LensError::new_err(format!("auto-generate-candidates failed: {e}"))
        })?;

    convert::to_python(py, &candidates_to_json(&candidates))
}

/// Render the candidate list as JSON suitable for `to_python`.
fn candidates_to_json(candidates: &[panproto_core::lens::LensCandidate]) -> Vec<serde_json::Value> {
    candidates
        .iter()
        .map(|c| {
            serde_json::json!({
                "quality": c.quality,
                "coverage": c.coverage,
                "score": c.score(),
                "strategies_used": c.strategies_used,
                "steps": c.steps.iter().map(|s| serde_json::json!({
                    "kind": s.kind,
                    "explanation": s.explanation,
                    "confidence": s.confidence,
                    "strategy": s.strategy,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// Register lens types and functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyLens>()?;
    parent.add_class::<PyComplement>()?;
    parent.add_class::<PyProtolensChain>()?;
    parent.add_function(wrap_pyfunction!(auto_generate_lens, parent)?)?;
    parent.add_function(wrap_pyfunction!(auto_generate_lens_candidates, parent)?)?;
    parent.add_function(wrap_pyfunction!(rename_field, parent)?)?;
    parent.add_function(wrap_pyfunction!(remove_field, parent)?)?;
    parent.add_function(wrap_pyfunction!(add_field, parent)?)?;
    parent.add_function(wrap_pyfunction!(hoist_field, parent)?)?;
    parent.add_function(wrap_pyfunction!(pipeline, parent)?)?;
    Ok(())
}

/// Map a `panproto-lens-dsl` error to a Python exception.
fn lens_dsl_err(e: &panproto_core::lens_dsl::LensDslError) -> PyErr {
    crate::error::LensError::new_err(format!("lens DSL error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPUTE_FIELD_JSON: &str = r#"{
        "id": "test-compute",
        "source": "s",
        "target": "t",
        "steps": [{
            "compute_field": {
                "target": "derived",
                "expr": "add count 1"
            }
        }]
    }"#;

    #[test]
    fn dsl_field_transform_survives_json_round_trip() -> PyResult<()> {
        let chain = PyProtolensChain::from_dsl_json(COMPUTE_FIELD_JSON, "r:body")?;
        assert!(has_derived_compute(&chain));

        let json = chain.to_json()?;
        let serialized: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| crate::error::LensError::new_err(e.to_string()))?;
        assert!(serialized.get("field_transforms").is_some());

        let structural = lens::ProtolensChain::from_json(&json)
            .map_err(|e| crate::error::LensError::new_err(e.to_string()))?;
        assert_eq!(structural.steps.len(), chain.inner.steps.len());

        let restored = PyProtolensChain::from_json(&json)?;
        assert!(has_derived_compute(&restored));
        Ok(())
    }

    #[test]
    fn legacy_chain_json_remains_compatible() -> PyResult<()> {
        let json = lens::ProtolensChain::new(Vec::new())
            .to_json()
            .map_err(|e| crate::error::LensError::new_err(e.to_string()))?;
        let restored = PyProtolensChain::from_json(&json)?;
        assert!(restored.field_transforms.is_empty());

        let reserialized: serde_json::Value = serde_json::from_str(&restored.to_json()?)
            .map_err(|e| crate::error::LensError::new_err(e.to_string()))?;
        assert!(reserialized.get("field_transforms").is_none());
        Ok(())
    }

    fn has_derived_compute(chain: &PyProtolensChain) -> bool {
        chain
            .field_transforms
            .get(&Name::from("r:body"))
            .is_some_and(|transforms| {
                transforms.iter().any(|transform| {
                    matches!(
                        transform,
                        FieldTransform::ComputeField { target_key, .. }
                            if target_key == "derived"
                    )
                })
            })
    }
}
