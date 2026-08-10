//! Python bindings for homomorphism search and the theory→schema→data cascade.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::gat::{Name, TheoryMorphism};
use panproto_core::mig::{
    cascade,
    hom_search::{self, FoundMorphism, SearchOptions},
};
use panproto_core::schema::SchemaMorphism;

use crate::convert;
use crate::mig::{PyCompiledMigration, PyMigration};
use crate::schema::PySchema;

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
        let map: HashMap<&str, serde_json::Value> = HashMap::from([
            (
                "vertex_map",
                serde_json::to_value(
                    self.inner
                        .vertex_map
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect::<HashMap<_, _>>(),
                )
                .unwrap_or_default(),
            ),
            ("quality", serde_json::json!(self.inner.quality)),
        ]);
        convert::to_python(py, &map)
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
// Module-level functions
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (src, tgt, anchors=None, monic=false, epic=false, iso=false, max_results=0, relax_edge_name_pruning=false))]
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn find_morphisms(
    src: &PySchema,
    tgt: &PySchema,
    anchors: Option<HashMap<String, String>>,
    monic: bool,
    epic: bool,
    iso: bool,
    max_results: usize,
    relax_edge_name_pruning: bool,
) -> Vec<PyFoundMorphism> {
    let initial = anchors
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
        .collect();
    let opts = SearchOptions {
        monic,
        epic,
        iso,
        max_results,
        initial,
        // The Python surface passes hard anchors only. `preferred` and
        // `max_nodes` exist for `lens::auto_generate`, which derives its
        // anchors from alignment strategies and needs a channel that
        // orders a domain rather than collapsing it; a caller reaching
        // the search directly is supplying anchors it knows, which is
        // what `initial` is for.
        preferred: HashMap::new(),
        max_nodes: 0,
        relax_edge_name_pruning,
    };
    hom_search::find_morphisms(&src.inner, &tgt.inner, &opts)
        .into_iter()
        .map(|m| PyFoundMorphism { inner: m })
        .collect()
}

#[pyfunction]
#[pyo3(signature = (src, tgt, anchors=None, monic=false, epic=false, iso=false, relax_edge_name_pruning=false))]
#[allow(clippy::fn_params_excessive_bools)]
pub fn find_best_morphism(
    src: &PySchema,
    tgt: &PySchema,
    anchors: Option<HashMap<String, String>>,
    monic: bool,
    epic: bool,
    iso: bool,
    relax_edge_name_pruning: bool,
) -> Option<PyFoundMorphism> {
    let initial = anchors
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
        .collect();
    let opts = SearchOptions {
        monic,
        epic,
        iso,
        max_results: 1,
        initial,
        // The Python surface passes hard anchors only. `preferred` and
        // `max_nodes` exist for `lens::auto_generate`, which derives its
        // anchors from alignment strategies and needs a channel that
        // orders a domain rather than collapsing it; a caller reaching
        // the search directly is supplying anchors it knows, which is
        // what `initial` is for.
        preferred: HashMap::new(),
        max_nodes: 0,
        relax_edge_name_pruning,
    };
    hom_search::find_best_morphism(&src.inner, &tgt.inner, &opts)
        .map(|m| PyFoundMorphism { inner: m })
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
    parent.add_function(wrap_pyfunction!(find_morphisms, parent)?)?;
    parent.add_function(wrap_pyfunction!(find_best_morphism, parent)?)?;
    parent.add_function(wrap_pyfunction!(induce_schema_morphism, parent)?)?;
    parent.add_function(wrap_pyfunction!(induce_migration_from_theory, parent)?)?;
    Ok(())
}
