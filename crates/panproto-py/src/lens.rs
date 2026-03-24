//! Python bindings for panproto bidirectional lenses.
//!
//! Wraps `panproto-lens`: asymmetric lenses with get/put, lens law
//! verification, auto-generation, and composition. The lens `Complement`
//! type (from `panproto-lens`) is Serialize-able, unlike the `inst`
//! version.

use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::lens::{self, AutoLensConfig, Complement, Lens};

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
/// Uses hom-search to find the best morphism, factorizes it into
/// elementary protolens steps, and instantiates the chain.
///
/// Parameters
/// ----------
/// `src_schema` : Schema
///     Source schema.
/// `tgt_schema` : Schema
///     Target schema.
/// protocol : Protocol
///     Protocol for the schemas.
///
/// Returns
/// -------
/// tuple[Lens, float]
///     The generated lens and the alignment quality score (0.0 to 1.0).
#[pyfunction]
pub fn auto_generate_lens(
    src_schema: &PySchema,
    tgt_schema: &PySchema,
    protocol: &PyProtocol,
) -> PyResult<(PyLens, f64)> {
    let config = AutoLensConfig::default();
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

/// Register lens types and functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyLens>()?;
    parent.add_class::<PyComplement>()?;
    parent.add_function(wrap_pyfunction!(auto_generate_lens, parent)?)?;
    Ok(())
}
