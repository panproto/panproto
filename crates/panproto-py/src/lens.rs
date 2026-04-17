//! Python bindings for panproto bidirectional lenses.
//!
//! Wraps `panproto-lens`: asymmetric lenses with get/put, lens law
//! verification, auto-generation, and composition. The lens `Complement`
//! type (from `panproto-lens`) is Serialize-able, unlike the `inst`
//! version.

use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::lens::{self, AutoLensConfig, Complement, Lens, Stringency};

/// Parse a Python-side stringency string into the engine [`Stringency`].
///
/// Accepts `"strict" | "balanced" | "lenient" | "exploratory"`
/// (case-insensitive). `None` keeps the engine's default.
fn parse_stringency(s: Option<&str>) -> PyResult<Option<Stringency>> {
    match s.map(str::to_ascii_lowercase).as_deref() {
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
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
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
/// tuple[Lens, float]
///     The generated lens and the alignment quality score (0.0 to 1.0).
#[pyfunction]
#[pyo3(signature = (src_schema, tgt_schema, protocol, stringency=None))]
pub fn auto_generate_lens(
    src_schema: &PySchema,
    tgt_schema: &PySchema,
    protocol: &PyProtocol,
    stringency: Option<&str>,
) -> PyResult<(PyLens, f64)> {
    let mut config = AutoLensConfig::default();
    if let Some(s) = parse_stringency(stringency)? {
        config.stringency = s;
    }
    let result = lens::auto_generate(
        &src_schema.inner,
        &tgt_schema.inner,
        &protocol.inner,
        &config,
    )
    .map_err(|e| crate::error::LensError::new_err(format!("auto-generate failed: {e}")))?;
    let lens = PyLens {
        inner: Arc::new(result.lens),
    };
    Ok((lens, result.alignment_quality))
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
}

#[pymethods]
impl PyProtolensChain {
    /// Auto-generate a protolens chain between two schemas.
    #[staticmethod]
    #[pyo3(signature = (src_schema, tgt_schema, protocol, stringency=None))]
    fn auto_generate(
        src_schema: &PySchema,
        tgt_schema: &PySchema,
        protocol: &PyProtocol,
        stringency: Option<&str>,
    ) -> PyResult<Self> {
        let mut config = AutoLensConfig::default();
        if let Some(s) = parse_stringency(stringency)? {
            config.stringency = s;
        }
        let result = lens::auto_generate(
            &src_schema.inner,
            &tgt_schema.inner,
            &protocol.inner,
            &config,
        )
        .map_err(|e| crate::error::LensError::new_err(format!("auto-generate failed: {e}")))?;
        Ok(Self {
            inner: Arc::new(result.chain),
        })
    }

    /// Auto-generate with morphism hints (vertex correspondences).
    #[staticmethod]
    #[pyo3(signature = (src_schema, tgt_schema, protocol, hints, stringency=None))]
    #[allow(clippy::needless_pass_by_value)]
    fn auto_generate_with_hints(
        src_schema: &PySchema,
        tgt_schema: &PySchema,
        protocol: &PyProtocol,
        hints: std::collections::HashMap<String, String>,
        stringency: Option<&str>,
    ) -> PyResult<Self> {
        use panproto_core::gat::Name;

        let mut initial = std::collections::HashMap::new();
        for (src, tgt) in &hints {
            initial.insert(Name::from(src.as_str()), Name::from(tgt.as_str()));
        }
        let mut config = AutoLensConfig {
            try_overlap: true,
            search_opts: panproto_core::mig::hom_search::SearchOptions {
                initial,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Some(s) = parse_stringency(stringency)? {
            config.stringency = s;
        }
        let result = lens::auto_generate(
            &src_schema.inner,
            &tgt_schema.inner,
            &protocol.inner,
            &config,
        )
        .map_err(|e| crate::error::LensError::new_err(format!("auto-generate failed: {e}")))?;
        Ok(Self {
            inner: Arc::new(result.chain),
        })
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
        src_schema: &PySchema,
        tgt_schema: &PySchema,
        protocol: &PyProtocol,
        hints: String,
    ) -> PyResult<Self> {
        let hint_spec: panproto_lens_dsl::HintSpec = serde_json::from_str(&hints)
            .map_err(|e| crate::error::LensError::new_err(format!("invalid hints JSON: {e}")))?;

        let parts = lens::hint::HintParts {
            anchors: hint_spec.anchors.clone(),
            scope_pairs: hint_spec.scope_pairs(),
            excluded_targets: hint_spec.excluded_target_names(),
            excluded_sources: hint_spec.excluded_source_names(),
            scoring_weights: hint_spec.scoring_weights(),
            name_similarity_threshold: hint_spec.name_similarity_threshold(),
        };
        let (derived, domain_constraints) =
            lens::hint::resolve_hints(&parts, &src_schema.inner, &tgt_schema.inner);

        let mut config = AutoLensConfig {
            try_overlap: true,
            ..Default::default()
        };
        if let Some(s) = hint_spec.stringency {
            config.stringency = match s {
                panproto_lens_dsl::HintStringency::Strict => Stringency::Strict,
                panproto_lens_dsl::HintStringency::Balanced => Stringency::Balanced,
                panproto_lens_dsl::HintStringency::Lenient => Stringency::Lenient,
                panproto_lens_dsl::HintStringency::Exploratory => Stringency::Exploratory,
            };
        }
        for cluster in &hint_spec.alias_clusters {
            config.alias_dict.add_cluster(cluster);
        }

        let result = lens::auto_generate_with_hints(
            &src_schema.inner,
            &tgt_schema.inner,
            &protocol.inner,
            &config,
            &derived,
            &domain_constraints,
            None,
        )
        .map_err(|e| {
            crate::error::LensError::new_err(format!("auto-generate with hints failed: {e}"))
        })?;

        Ok(Self {
            inner: Arc::new(result.chain),
        })
    }

    /// Instantiate against a concrete schema to produce a ``Lens``.
    fn instantiate(&self, schema: &PySchema, protocol: &PyProtocol) -> PyResult<PyLens> {
        let lens_obj = self
            .inner
            .instantiate(&schema.inner, &protocol.inner)
            .map_err(|e| crate::error::LensError::new_err(format!("instantiate failed: {e}")))?;
        Ok(PyLens {
            inner: Arc::new(lens_obj),
        })
    }

    /// Compose with another chain (vertical composition).
    fn compose(&self, other: &Self) -> Self {
        let mut steps = self.inner.steps.clone();
        steps.extend(other.inner.steps.clone());
        Self {
            inner: Arc::new(lens::ProtolensChain::new(steps)),
        }
    }

    /// Fuse all steps into a single protolens.
    fn fuse(&self) -> PyResult<Self> {
        let fused = self
            .inner
            .fuse()
            .map_err(|e| crate::error::LensError::new_err(format!("fuse failed: {e}")))?;
        Ok(Self {
            inner: Arc::new(lens::ProtolensChain::new(vec![fused])),
        })
    }

    /// Serialize to JSON.
    fn to_json(&self) -> PyResult<String> {
        self.inner
            .to_json()
            .map_err(|e| crate::error::LensError::new_err(format!("to_json failed: {e}")))
    }

    /// Deserialize from JSON.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let chain = lens::ProtolensChain::from_json(json)
            .map_err(|e| crate::error::LensError::new_err(format!("from_json failed: {e}")))?;
        Ok(Self {
            inner: Arc::new(chain),
        })
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
    use panproto_core::gat::Name;
    PyProtolensChain {
        inner: Arc::new(lens::combinators::rename_field(
            Name::from(parent),
            Name::from(field),
            Name::from(old_name),
            Name::from(new_name),
        )),
    }
}

/// Remove a field (drop sort with edge cascade).
#[pyfunction]
pub fn remove_field(field: &str) -> PyProtolensChain {
    use panproto_core::gat::Name;
    PyProtolensChain {
        inner: Arc::new(lens::combinators::remove_field(Name::from(field))),
    }
}

/// Add a field with a default value.
#[pyfunction]
pub fn add_field(parent: &str, name: &str, kind: &str) -> PyProtolensChain {
    use panproto_core::gat::Name;
    use panproto_core::inst::value::Value;
    PyProtolensChain {
        inner: Arc::new(lens::combinators::add_field(
            Name::from(parent),
            Name::from(name),
            Name::from(kind),
            Value::Null,
        )),
    }
}

/// Hoist a nested field up one level.
#[pyfunction]
pub fn hoist_field(parent: &str, intermediate: &str, child: &str) -> PyProtolensChain {
    use panproto_core::gat::Name;
    PyProtolensChain {
        inner: Arc::new(lens::combinators::hoist_field(
            Name::from(parent),
            Name::from(intermediate),
            Name::from(child),
        )),
    }
}

/// Build a pipeline from multiple protolens chains (vertical composition).
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub fn pipeline(chains: Vec<PyRef<'_, PyProtolensChain>>) -> PyProtolensChain {
    let all_chains: Vec<lens::ProtolensChain> = chains.iter().map(|c| (*c.inner).clone()).collect();
    PyProtolensChain {
        inner: Arc::new(lens::combinators::pipeline(all_chains)),
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
) -> PyResult<PyObject> {
    let mut config = AutoLensConfig::default();
    if let Some(s) = parse_stringency(stringency)? {
        config.stringency = s;
    }
    let candidates = lens::auto_generate_candidates(
        &src_schema.inner,
        &tgt_schema.inner,
        &protocol.inner,
        &config,
        top_n,
    )
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::redundant_pattern_matching,
    clippy::items_after_test_module
)]
mod tests {
    use super::*;

    #[test]
    fn parse_stringency_accepts_the_four_tiers_case_insensitive() {
        pyo3::prepare_freethreaded_python();
        assert!(parse_stringency(None).unwrap().is_none());
        assert!(matches!(
            parse_stringency(Some("strict")).unwrap(),
            Some(Stringency::Strict)
        ));
        assert!(matches!(
            parse_stringency(Some("BALANCED")).unwrap(),
            Some(Stringency::Balanced)
        ));
        assert!(matches!(
            parse_stringency(Some("Lenient")).unwrap(),
            Some(Stringency::Lenient)
        ));
        assert!(matches!(
            parse_stringency(Some("exploratory")).unwrap(),
            Some(Stringency::Exploratory)
        ));
    }

    #[test]
    fn parse_stringency_rejects_garbage_with_clear_error() {
        pyo3::prepare_freethreaded_python();
        let err = parse_stringency(Some("loose")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("loose") && msg.contains("strict"),
            "error message should name the bad value and list valid tiers; got: {msg}",
        );
    }
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
