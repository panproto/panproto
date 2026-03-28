//! Python bindings for panproto GAT (generalized algebraic theory) operations.
//!
//! Wraps `panproto-gat`: theories, morphisms, models, colimits.
//! Note that `Model` contains function pointers (`Arc<dyn Fn>`) and
//! thus cannot be serialized or cloned. We expose it as an opaque
//! handle with limited introspection.

use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::gat::{self, FreeModelConfig, Model, Theory, TheoryMorphism};

use crate::convert;

/// A generalized algebraic theory.
#[pyclass(name = "Theory", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyTheory {
    pub(crate) inner: Arc<Theory>,
}

#[pymethods]
impl PyTheory {
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Number of sorts in the theory.
    #[getter]
    fn sort_count(&self) -> usize {
        self.inner.sorts.len()
    }

    /// Number of operations in the theory.
    #[getter]
    fn op_count(&self) -> usize {
        self.inner.ops.len()
    }

    /// Number of equations in the theory.
    #[getter]
    fn eq_count(&self) -> usize {
        self.inner.eqs.len()
    }

    /// Sorts as a list of dicts.
    #[getter]
    fn sorts(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner.sorts)
    }

    /// Operations as a list of dicts.
    #[getter]
    fn ops(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner.ops)
    }

    /// Equations as a list of dicts.
    #[getter]
    fn eqs(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner.eqs)
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, self.inner.as_ref())
    }

    fn __repr__(&self) -> String {
        format!(
            "Theory({:?}, sorts={}, ops={}, eqs={})",
            self.inner.name,
            self.inner.sorts.len(),
            self.inner.ops.len(),
            self.inner.eqs.len()
        )
    }
}

/// An opaque handle to a GAT model.
///
/// Models contain function pointers and cannot be serialized or cloned.
/// Use ``sort_interp_keys`` and ``theory_name`` for introspection, or
/// ``check_model`` to verify equation satisfaction.
#[pyclass(name = "Model", module = "panproto._native")]
pub struct PyModel {
    pub(crate) inner: Model,
}

#[pymethods]
impl PyModel {
    /// The name of the theory this model interprets.
    #[getter]
    fn theory_name(&self) -> &str {
        &self.inner.theory
    }

    /// The sort names that have carrier sets in this model.
    #[getter]
    fn sort_interp_keys(&self) -> Vec<String> {
        self.inner.sort_interp.keys().cloned().collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "Model(theory={:?}, sorts={})",
            self.inner.theory,
            self.inner.sort_interp.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Create a theory from a dict specification.
#[pyfunction]
pub fn create_theory(spec: &Bound<'_, PyAny>) -> PyResult<PyTheory> {
    let theory: Theory = convert::from_python(spec)?;
    Ok(PyTheory {
        inner: Arc::new(theory),
    })
}

/// Compute the colimit (pushout) of two theories over a shared sub-theory.
///
/// Parameters
/// ----------
/// t1 : Theory
///     First theory.
/// t2 : Theory
///     Second theory.
/// shared : Theory
///     Shared sub-theory (the pushout apex).
#[pyfunction]
pub fn colimit_theories(t1: &PyTheory, t2: &PyTheory, shared: &PyTheory) -> PyResult<PyTheory> {
    let result = gat::colimit_by_name(&t1.inner, &t2.inner, &shared.inner)
        .map_err(|e| crate::error::GatError::new_err(format!("colimit failed: {e}")))?;
    Ok(PyTheory {
        inner: Arc::new(result),
    })
}

/// Check that a theory morphism is well-defined.
///
/// Parameters
/// ----------
/// morphism : dict
///     Theory morphism specification with ``src_theory``, ``tgt_theory``,
///     ``sort_map``, ``op_map``.
/// domain : Theory
///     The domain (source) theory.
/// codomain : Theory
///     The codomain (target) theory.
#[pyfunction]
pub fn check_morphism(
    morphism: &Bound<'_, PyAny>,
    domain: &PyTheory,
    codomain: &PyTheory,
) -> PyResult<()> {
    let morph: TheoryMorphism = convert::from_python(morphism)?;
    gat::check_morphism(&morph, &domain.inner, &codomain.inner)
        .map_err(|e| crate::error::GatError::new_err(format!("morphism check failed: {e}")))?;
    Ok(())
}

/// Migrate a model along a theory morphism.
#[pyfunction]
pub fn migrate_model(morphism: &Bound<'_, PyAny>, model: &PyModel) -> PyResult<PyModel> {
    let morph: TheoryMorphism = convert::from_python(morphism)?;
    let migrated = gat::migrate_model(&morph, &model.inner)
        .map_err(|e| crate::error::GatError::new_err(format!("model migration failed: {e}")))?;
    Ok(PyModel { inner: migrated })
}

/// Construct the free (initial) model of a theory.
///
/// Parameters
/// ----------
/// theory : Theory
///     The theory to construct the free model for.
/// `max_depth` : int
///     Maximum depth of term generation. Default 3.
/// `max_terms_per_sort` : int
///     Safety bound on terms per sort. Default 1000.
#[pyfunction]
#[pyo3(signature = (theory, max_depth=3, max_terms_per_sort=1000))]
pub fn free_model(
    theory: &PyTheory,
    max_depth: usize,
    max_terms_per_sort: usize,
) -> PyResult<PyModel> {
    let config = FreeModelConfig {
        max_depth,
        max_terms_per_sort,
    };
    let model = gat::free_model(&theory.inner, &config)
        .map_err(|e| crate::error::GatError::new_err(format!("free model failed: {e}")))?;
    Ok(PyModel { inner: model })
}

/// Check a model against its theory, returning equation violations.
///
/// Returns
/// -------
/// list[str]
///     Equation violation descriptions. Empty if the model satisfies
///     all equations.
#[pyfunction]
pub fn check_model(model: &PyModel, theory: &PyTheory) -> PyResult<Vec<String>> {
    let violations = gat::check_model(&model.inner, &theory.inner)
        .map_err(|e| crate::error::GatError::new_err(format!("model check failed: {e}")))?;
    Ok(violations.into_iter().map(|v| format!("{v:?}")).collect())
}

/// Register GAT types and functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyTheory>()?;
    parent.add_class::<PyModel>()?;
    parent.add_function(wrap_pyfunction!(create_theory, parent)?)?;
    parent.add_function(wrap_pyfunction!(colimit_theories, parent)?)?;
    parent.add_function(wrap_pyfunction!(check_morphism, parent)?)?;
    parent.add_function(wrap_pyfunction!(migrate_model, parent)?)?;
    parent.add_function(wrap_pyfunction!(free_model, parent)?)?;
    parent.add_function(wrap_pyfunction!(check_model, parent)?)?;
    Ok(())
}
