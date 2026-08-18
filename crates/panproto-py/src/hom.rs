//! Python bindings for homomorphism search and the theory→schema→data cascade.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::gat::{Name, TheoryMorphism};
use panproto_core::mig::{
    SchemaSpan, cascade,
    hom_search::{self, FoundMorphism, SearchOptions},
};
use panproto_core::schema::SchemaMorphism;

use crate::convert;
use crate::error::MigrationError;
use crate::mig::{PyCompiledMigration, PyMigration};
use crate::schema::{PyEdge, PyProtocol, PySchema};

/// The JSON projection of a found morphism.
///
/// Shared by [`PyFoundMorphism::to_dict`] and [`PySchemaSpan::to_dict`] so the
/// two agree on the shape. `edge_map` is a list of `(source, target)` pairs,
/// matching how [`Migration`](panproto_core::mig::Migration) serialises its own
/// edge map, because an edge is a structural four-tuple and not a JSON key.
fn found_morphism_json(found: &FoundMorphism) -> serde_json::Value {
    serde_json::json!({
        "vertex_map": found
            .vertex_map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>(),
        "edge_map": found
            .edge_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>(),
        "quality": found.quality,
    })
}

// ---------------------------------------------------------------------------
// PyTheoryMorphism
// ---------------------------------------------------------------------------

#[pyclass(
    from_py_object,
    name = "TheoryMorphism",
    frozen,
    module = "panproto._native"
)]
#[derive(Clone)]
pub struct PyTheoryMorphism {
    pub(crate) inner: TheoryMorphism,
}

#[pymethods]
impl PyTheoryMorphism {
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn domain(&self) -> &str {
        &self.inner.domain
    }

    #[getter]
    fn codomain(&self) -> &str {
        &self.inner.codomain
    }

    #[getter]
    fn sort_map(&self) -> HashMap<String, String> {
        self.inner
            .sort_map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[getter]
    fn op_map(&self) -> HashMap<String, String> {
        self.inner
            .op_map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner)
    }

    #[staticmethod]
    fn from_dict(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        let inner: TheoryMorphism = convert::from_python(obj)?;
        Ok(Self { inner })
    }

    fn __repr__(&self) -> String {
        format!(
            "TheoryMorphism({:?}: {:?} → {:?})",
            &*self.inner.name, &*self.inner.domain, &*self.inner.codomain
        )
    }
}

// ---------------------------------------------------------------------------
// PySchemaMorphism
// ---------------------------------------------------------------------------

#[pyclass(
    from_py_object,
    name = "SchemaMorphism",
    frozen,
    module = "panproto._native"
)]
#[derive(Clone)]
pub struct PySchemaMorphism {
    pub(crate) inner: SchemaMorphism,
}

#[pymethods]
impl PySchemaMorphism {
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn src_protocol(&self) -> &str {
        &self.inner.src_protocol
    }

    #[getter]
    fn tgt_protocol(&self) -> &str {
        &self.inner.tgt_protocol
    }

    #[getter]
    fn vertex_map(&self) -> HashMap<String, String> {
        self.inner
            .vertex_map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "SchemaMorphism({:?}: {:?} → {:?})",
            self.inner.name, self.inner.src_protocol, self.inner.tgt_protocol
        )
    }
}

// ---------------------------------------------------------------------------
// PyFoundMorphism
// ---------------------------------------------------------------------------

#[pyclass(
    from_py_object,
    name = "FoundMorphism",
    frozen,
    module = "panproto._native"
)]
#[derive(Clone)]
pub struct PyFoundMorphism {
    pub(crate) inner: FoundMorphism,
}

#[pymethods]
impl PyFoundMorphism {
    #[getter]
    fn vertex_map(&self) -> HashMap<String, String> {
        self.inner
            .vertex_map
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// Edge mapping, as `(source edge, target edge)` pairs.
    ///
    /// A list rather than a dict because an ``Edge`` is a structural
    /// four-tuple, not a name, and Python dicts keyed on it would need it
    /// hashable at the boundary. The order is unspecified.
    #[getter]
    fn edge_map(&self) -> Vec<(PyEdge, PyEdge)> {
        self.inner
            .edge_map
            .iter()
            .map(|(k, v)| (PyEdge { inner: k.clone() }, PyEdge { inner: v.clone() }))
            .collect()
    }

    #[getter]
    const fn quality(&self) -> f64 {
        self.inner.quality
    }

    fn to_migration(&self) -> PyMigration {
        PyMigration {
            inner: hom_search::morphism_to_migration(&self.inner),
        }
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &found_morphism_json(&self.inner))
    }

    fn __repr__(&self) -> String {
        format!(
            "FoundMorphism(quality={:.3}, mappings={})",
            self.inner.quality,
            self.inner.vertex_map.len()
        )
    }
}

// ---------------------------------------------------------------------------
// PySchemaSpan
// ---------------------------------------------------------------------------

/// A span of schema morphisms ``src <-l- apex -r-> tgt``.
///
/// The apex is the sub-schema of ``src`` induced on the vertices the search
/// gave a target, so the left leg is an inclusion and the right leg is a
/// general schema morphism. A span always exists: leaving every vertex out is
/// feasible, so "these two schemas share nothing" comes back as an empty apex
/// rather than as a failure.
///
/// A total morphism is the degenerate case, where the left leg is onto.
/// :attr:`is_total` tests it and :meth:`as_total_morphism` returns the older
/// ``FoundMorphism`` shape.
#[pyclass(
    from_py_object,
    name = "SchemaSpan",
    frozen,
    module = "panproto._native"
)]
#[derive(Clone)]
pub struct PySchemaSpan {
    pub(crate) inner: SchemaSpan,
}

#[pymethods]
impl PySchemaSpan {
    /// The apex: the sub-schema of ``src`` the search covered.
    #[getter]
    fn apex(&self) -> PySchema {
        PySchema {
            inner: Arc::new(self.inner.apex.clone()),
        }
    }

    /// The left leg ``apex -> src``, an inclusion.
    #[getter]
    fn left(&self) -> PyMigration {
        PyMigration {
            inner: self.inner.left.clone(),
        }
    }

    /// The right leg ``apex -> tgt``.
    #[getter]
    fn right(&self) -> PyMigration {
        PyMigration {
            inner: self.inner.right.clone(),
        }
    }

    /// How well the covered part matches, in ``[0, 1]``.
    ///
    /// A ranking signal among spans over one source schema and nothing else:
    /// every denominator of the objective is fixed by ``src``, so two spans
    /// over different sources are not comparable. Read it alongside
    /// :attr:`apex_coverage`, which answers how much was covered.
    #[getter]
    const fn quality(&self) -> f64 {
        self.inner.quality
    }

    /// ``(lower, upper)`` bracketing :attr:`quality`.
    ///
    /// The two are equal exactly when :attr:`proven_optimal` holds. When it
    /// does not, this interval is what separates "0.4, and nothing better
    /// exists" from "0.4, and the search ran out of budget".
    #[getter]
    const fn quality_bounds(&self) -> (f64, f64) {
        self.inner.quality_bounds
    }

    /// ``len(apex.vertices) / len(src.vertices)``, or one on an empty source.
    #[getter]
    const fn apex_coverage(&self) -> f64 {
        self.inner.apex_coverage
    }

    /// Whether the search proved its answer optimal.
    #[getter]
    const fn proven_optimal(&self) -> bool {
        self.inner.certificate.proven_optimal
    }

    /// Whether the span is a total morphism, i.e. whether the left leg is onto.
    #[getter]
    const fn is_total(&self) -> bool {
        self.inner.is_total()
    }

    /// Whether both legs passed the functoriality check.
    #[getter]
    const fn legs_are_functorial(&self) -> bool {
        self.inner.certificate.legs_are_functorial
    }

    /// The content digest of the apex, in lower-case hexadecimal.
    #[getter]
    fn apex_digest(&self) -> String {
        self.inner.apex_digest_hex()
    }

    /// The span as a total morphism, or ``None`` when it is not one.
    fn as_total_morphism(&self) -> Option<PyFoundMorphism> {
        self.inner
            .as_total_morphism()
            .map(|inner| PyFoundMorphism { inner })
    }

    /// The apex as the ``(source, target)`` pair lists a pushout expects.
    ///
    /// Both lists are sorted, so the overlap is a function of the span rather
    /// than of a hash seed.
    fn to_overlap(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let overlap = self.inner.to_overlap();
        let value = serde_json::json!({
            "vertex_pairs": overlap
                .vertex_pairs
                .iter()
                .map(|(left, right)| (left.to_string(), right.to_string()))
                .collect::<Vec<_>>(),
            "edge_pairs": overlap.edge_pairs,
        });
        convert::to_python(py, &value)
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let value = serde_json::json!({
            "apex": self.inner.apex,
            "left": self.inner.left,
            "right": self.inner.right,
            "quality": self.inner.quality,
            "quality_bounds": [self.inner.quality_bounds.0, self.inner.quality_bounds.1],
            "apex_coverage": self.inner.apex_coverage,
            "proven_optimal": self.inner.certificate.proven_optimal,
            "legs_are_functorial": self.inner.certificate.legs_are_functorial,
            "is_total": self.inner.is_total(),
            "apex_digest": self.inner.apex_digest_hex(),
        });
        convert::to_python(py, &value)
    }

    fn __repr__(&self) -> String {
        format!(
            "SchemaSpan(apex={} vertices, quality={:.3}, coverage={:.3}, total={})",
            self.inner.apex.vertices.len(),
            self.inner.quality,
            self.inner.apex_coverage,
            self.inner.is_total()
        )
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Optimal total schema morphisms from `src` to `tgt`.
///
/// Returns the morphisms **attaining the optimum**, capped by `max_results`,
/// rather than the whole hom-set ranked by quality. Every element carries the
/// same quality, which is the maximum over all total morphisms, so a caller
/// reading element zero gets what it always got; a caller walking the list for
/// a suboptimal alternative will not find one. An empty list means no total
/// morphism exists, and only that.
///
/// `anchors` are mappings the caller *knows*, and the search may not
/// reconsider them.
///
/// # Errors
///
/// Raises ``MigrationError`` when the search network could not be posed or the
/// iso path refused it. Neither means "no morphism exists", which is why they
/// are raised rather than folded into an empty list.
#[pyfunction]
#[pyo3(signature = (src, tgt, anchors=None, monic=false, epic=false, iso=false, max_results=0))]
#[allow(clippy::fn_params_excessive_bools)]
pub fn find_morphisms(
    src: &PySchema,
    tgt: &PySchema,
    anchors: Option<HashMap<String, String>>,
    monic: bool,
    epic: bool,
    iso: bool,
    max_results: usize,
) -> PyResult<Vec<PyFoundMorphism>> {
    let hard_pins = anchors
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
        .collect();
    let opts = SearchOptions {
        monic,
        epic,
        iso,
        max_results,
        hard_pins,
    };
    let found = hom_search::find_morphisms(&src.inner, &tgt.inner, &opts)
        .map_err(|e| MigrationError::new_err(format!("morphism search failed: {e}")))?;
    Ok(found
        .morphisms
        .into_iter()
        .map(|m| PyFoundMorphism { inner: m })
        .collect())
}

/// The single best total schema morphism from `src` to `tgt`, or `None` when
/// no total morphism exists.
///
/// # Errors
///
/// Raises ``MigrationError`` when the search network could not be posed or the
/// iso path refused it, so ``None`` means what it says.
#[pyfunction]
#[pyo3(signature = (src, tgt, anchors=None, monic=false, epic=false, iso=false))]
#[allow(clippy::fn_params_excessive_bools)]
pub fn find_best_morphism(
    src: &PySchema,
    tgt: &PySchema,
    anchors: Option<HashMap<String, String>>,
    monic: bool,
    epic: bool,
    iso: bool,
) -> PyResult<Option<PyFoundMorphism>> {
    let hard_pins = anchors
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
        .collect();
    let opts = SearchOptions {
        monic,
        epic,
        iso,
        max_results: 1,
        hard_pins,
    };
    hom_search::find_best_morphism(&src.inner, &tgt.inner, &opts)
        .map(|found| found.map(|inner| PyFoundMorphism { inner }))
        .map_err(|e| MigrationError::new_err(format!("morphism search failed: {e}")))
}

/// The optimal span between `src` and `tgt`.
///
/// Unlike :func:`find_best_morphism`, this never refuses for want of a match:
/// the assignment leaving every source vertex out is always feasible, so two
/// schemas with nothing in common come back as a span with an empty apex rather
/// than as ``None``. Read :attr:`SchemaSpan.apex_coverage` to tell the two
/// apart.
///
/// `protocol` is a parameter because the apex is a schema, and a schema is only
/// well formed against a protocol: inducing it re-validates the result rather
/// than assuming it.
///
/// `anchors` are mappings the caller *knows*, and the search may not reconsider
/// them.
///
/// # Errors
///
/// Raises ``MigrationError`` when the search network could not be posed, when
/// the iso path refused it, or when the induced apex is not a well-formed
/// schema. None of those means "no morphism exists".
///
/// It also raises when ``epic`` is set. Surjectivity is a property of a total
/// morphism: a span's right leg is deliberately partial and this entry point
/// never refuses for want of a match, so requiring the map to be onto would
/// contradict the paragraph above. Use :func:`find_morphisms` or
/// :func:`find_best_morphism` for a surjective total morphism.
#[pyfunction]
#[pyo3(signature = (src, tgt, protocol, anchors=None, monic=false, epic=false, iso=false))]
#[allow(clippy::fn_params_excessive_bools)]
pub fn find_span(
    src: &PySchema,
    tgt: &PySchema,
    protocol: &PyProtocol,
    anchors: Option<HashMap<String, String>>,
    monic: bool,
    epic: bool,
    iso: bool,
) -> PyResult<PySchemaSpan> {
    let hard_pins = anchors
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
        .collect();
    let opts = SearchOptions {
        monic,
        epic,
        iso,
        max_results: 1,
        hard_pins,
    };
    hom_search::find_span(&src.inner, &tgt.inner, &protocol.inner, &opts)
        .map(|inner| PySchemaSpan { inner })
        .map_err(|e| MigrationError::new_err(format!("span search failed: {e}")))
}

#[pyfunction]
pub fn induce_schema_morphism(
    theory_morph: &PyTheoryMorphism,
    src_schema: &PySchema,
) -> PySchemaMorphism {
    PySchemaMorphism {
        inner: cascade::induce_schema_morphism(&theory_morph.inner, &src_schema.inner),
    }
}

#[pyfunction]
pub fn induce_migration_from_theory(
    theory_morph: &PyTheoryMorphism,
    src_schema: &PySchema,
    tgt_schema: &PySchema,
) -> (PySchemaMorphism, PyCompiledMigration) {
    let (schema_morph, compiled) = cascade::induce_migration_from_theory(
        &theory_morph.inner,
        &src_schema.inner,
        &tgt_schema.inner,
    );
    (
        PySchemaMorphism {
            inner: schema_morph,
        },
        PyCompiledMigration {
            compiled,
            src_schema: Arc::clone(&src_schema.inner),
            tgt_schema: Arc::clone(&tgt_schema.inner),
        },
    )
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyTheoryMorphism>()?;
    parent.add_class::<PySchemaMorphism>()?;
    parent.add_class::<PyFoundMorphism>()?;
    parent.add_class::<PySchemaSpan>()?;
    parent.add_function(wrap_pyfunction!(find_morphisms, parent)?)?;
    parent.add_function(wrap_pyfunction!(find_best_morphism, parent)?)?;
    parent.add_function(wrap_pyfunction!(find_span, parent)?)?;
    parent.add_function(wrap_pyfunction!(induce_schema_morphism, parent)?)?;
    parent.add_function(wrap_pyfunction!(induce_migration_from_theory, parent)?)?;
    Ok(())
}
