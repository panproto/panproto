//! Python bindings for panproto migration operations.
//!
//! Wraps `panproto-mig` functions: compile, lift, get/put, compose, invert,
//! `check_existence`, and `check_coverage`. The `Migration` struct uses `Name`
//! and `Edge` internally; the builder converts from Python strings.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::gat::Name;
use panproto_core::inst::CompiledMigration;
use panproto_core::mig::{self, Migration};
use panproto_core::schema::{Edge, Schema};

use crate::convert;
use crate::inst::PyInstance;
use crate::schema::{PyProtocol, PySchema};

/// A migration specification mapping source to target schema elements.
#[pyclass(name = "Migration", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyMigration {
    pub(crate) inner: Migration,
}

#[pymethods]
impl PyMigration {
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }

    #[staticmethod]
    fn from_dict(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        let inner: Migration = convert::from_python(obj)?;
        Ok(Self { inner })
    }
}

/// Builder for constructing migration specifications.
///
/// Accumulates vertex mappings, edge mappings, and resolvers, then
/// produces a ``Migration`` object with the correct internal types.
#[pyclass(name = "MigrationBuilder", module = "panproto._native")]
pub struct PyMigrationBuilder {
    vertex_map: HashMap<String, String>,
    #[allow(clippy::type_complexity)]
    resolver: HashMap<(String, String), (String, String, String, Option<String>)>,
}

#[pymethods]
impl PyMigrationBuilder {
    #[new]
    fn new() -> Self {
        Self {
            vertex_map: HashMap::new(),
            resolver: HashMap::new(),
        }
    }

    /// Map a source vertex to a target vertex.
    fn map_vertex(&mut self, src: &str, tgt: &str) {
        self.vertex_map.insert(src.to_owned(), tgt.to_owned());
    }

    /// Add a contraction resolver: when vertices ``src_vertex`` and
    /// ``tgt_vertex`` are contracted, resolve with the given edge.
    #[pyo3(signature = (src_vertex, tgt_vertex, edge_src, edge_tgt, edge_kind, edge_name=None))]
    fn resolve(
        &mut self,
        src_vertex: &str,
        tgt_vertex: &str,
        edge_src: &str,
        edge_tgt: &str,
        edge_kind: &str,
        edge_name: Option<&str>,
    ) {
        self.resolver.insert(
            (src_vertex.to_owned(), tgt_vertex.to_owned()),
            (
                edge_src.to_owned(),
                edge_tgt.to_owned(),
                edge_kind.to_owned(),
                edge_name.map(str::to_owned),
            ),
        );
    }

    /// Build the migration specification.
    fn build(&self) -> PyMigration {
        let vertex_map: HashMap<Name, Name> = self
            .vertex_map
            .iter()
            .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
            .collect();

        let resolver: HashMap<(Name, Name), Edge> = self
            .resolver
            .iter()
            .map(|((sv, tv), (es, et, ek, en))| {
                (
                    (Name::from(sv.as_str()), Name::from(tv.as_str())),
                    Edge {
                        src: Name::from(es.as_str()),
                        tgt: Name::from(et.as_str()),
                        kind: Name::from(ek.as_str()),
                        name: en.as_deref().map(Name::from),
                    },
                )
            })
            .collect();

        PyMigration {
            inner: Migration {
                vertex_map,
                edge_map: HashMap::new(),
                hyper_edge_map: HashMap::new(),
                label_map: HashMap::new(),
                resolver,
                hyper_resolver: HashMap::new(),
                expr_resolvers: HashMap::new(),
            },
        }
    }
}

/// A compiled migration ready for per-record application.
#[pyclass(name = "CompiledMigration", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyCompiledMigration {
    pub(crate) compiled: CompiledMigration,
    pub(crate) src_schema: Arc<Schema>,
    pub(crate) tgt_schema: Arc<Schema>,
}

#[pymethods]
impl PyCompiledMigration {
    /// Lift a W-type instance through this migration (left Kan extension).
    fn lift(&self, instance: &PyInstance) -> PyResult<PyInstance> {
        let lifted = mig::lift_wtype(
            &self.compiled,
            &self.src_schema,
            &self.tgt_schema,
            &instance.inner,
        )
        .map_err(|e| crate::error::MigrationError::new_err(format!("lift failed: {e}")))?;
        Ok(PyInstance {
            inner: lifted,
            schema: Arc::clone(&self.tgt_schema),
        })
    }

    /// Get: project through the migration, producing a view and complement.
    ///
    /// Returns
    /// -------
    /// tuple[Instance, dict]
    ///     The view instance and complement. The complement is returned as
    ///     a dict with keys ``dropped_nodes`` and ``dropped_arcs`` because
    ///     the Rust ``Complement`` type does not implement ``Serialize``.
    fn get(&self, instance: &PyInstance, py: Python<'_>) -> PyResult<(PyInstance, PyObject)> {
        let (view, complement) = panproto_core::inst::restrict_with_complement(
            &instance.inner,
            &self.src_schema,
            &self.tgt_schema,
            &self.compiled,
        )
        .map_err(|e| crate::error::MigrationError::new_err(format!("get failed: {e}")))?;

        // Complement doesn't derive Serialize, so build a dict manually.
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("dropped_node_count", complement.dropped_nodes.len())?;
        dict.set_item("dropped_arc_count", complement.dropped_arcs.len())?;

        let view_inst = PyInstance {
            inner: view,
            schema: Arc::clone(&self.tgt_schema),
        };
        Ok((view_inst, dict.into_any().unbind()))
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.compiled)
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Compile a migration into an executable form.
///
/// Parameters
/// ----------
/// migration : Migration
///     The migration specification.
/// `src_schema` : Schema
///     Source schema (pre-migration).
/// `tgt_schema` : Schema
///     Target schema (post-migration).
///
/// Returns
/// -------
/// `CompiledMigration`
///     The compiled migration with precomputed surviving sets and remaps.
#[pyfunction]
pub fn compile_migration(
    migration: &PyMigration,
    src_schema: &PySchema,
    tgt_schema: &PySchema,
) -> PyResult<PyCompiledMigration> {
    let compiled = mig::compile(&src_schema.inner, &tgt_schema.inner, &migration.inner)
        .map_err(|e| crate::error::MigrationError::new_err(format!("compilation failed: {e}")))?;
    Ok(PyCompiledMigration {
        compiled,
        src_schema: Arc::clone(&src_schema.inner),
        tgt_schema: Arc::clone(&tgt_schema.inner),
    })
}

/// Check that a migration is well-defined (all referenced sorts exist).
///
/// Returns the existence report as a dict. The report is not a ``Result``;
/// it always succeeds but may contain errors in its ``errors`` field.
#[pyfunction]
pub fn check_existence(
    migration: &PyMigration,
    protocol: &PyProtocol,
    src_schema: &PySchema,
    tgt_schema: &PySchema,
    py: Python<'_>,
) -> PyResult<PyObject> {
    // check_existence requires a theory_registry; pass an empty one
    // since the built-in protocols don't require external theory lookup
    // for basic existence checks.
    let theory_registry = HashMap::new();
    let report = mig::check_existence(
        &protocol.inner,
        &src_schema.inner,
        &tgt_schema.inner,
        &migration.inner,
        &theory_registry,
    );
    convert::to_python(py, &report)
}

/// Compose two migrations sequentially.
#[pyfunction]
pub fn compose_migrations(mig1: &PyMigration, mig2: &PyMigration) -> PyResult<PyMigration> {
    let composed = mig::compose(&mig1.inner, &mig2.inner)
        .map_err(|e| crate::error::MigrationError::new_err(format!("compose failed: {e}")))?;
    Ok(PyMigration { inner: composed })
}

/// Invert a migration (if bijective).
#[pyfunction]
pub fn invert_migration(
    migration: &PyMigration,
    src_schema: &PySchema,
    tgt_schema: &PySchema,
) -> PyResult<PyMigration> {
    let inverted = mig::invert(&migration.inner, &src_schema.inner, &tgt_schema.inner)
        .map_err(|e| crate::error::MigrationError::new_err(format!("invert failed: {e}")))?;
    Ok(PyMigration { inner: inverted })
}

/// Check migration coverage on a set of instances.
///
/// Parameters
/// ----------
/// compiled : `CompiledMigration`
///     The compiled migration.
/// instances : list[Instance]
///     Instances to test lift on.
/// `src_schema` : Schema
///     Source schema.
/// `tgt_schema` : Schema
///     Target schema.
///
/// Returns
/// -------
/// dict
///     Coverage report with ``total_records``, ``successful``, and ``failed``.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub fn check_coverage(
    compiled: &PyCompiledMigration,
    instances: Vec<PyInstance>,
    src_schema: &PySchema,
    tgt_schema: &PySchema,
    py: Python<'_>,
) -> PyResult<PyObject> {
    let winstances: Vec<_> = instances.iter().map(|i| i.inner.clone()).collect();
    let report = mig::check_coverage(
        &compiled.compiled,
        &winstances,
        &src_schema.inner,
        &tgt_schema.inner,
    );
    convert::to_python(py, &report)
}

/// Register migration types and functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyMigration>()?;
    parent.add_class::<PyMigrationBuilder>()?;
    parent.add_class::<PyCompiledMigration>()?;
    parent.add_function(wrap_pyfunction!(compile_migration, parent)?)?;
    parent.add_function(wrap_pyfunction!(check_existence, parent)?)?;
    parent.add_function(wrap_pyfunction!(compose_migrations, parent)?)?;
    parent.add_function(wrap_pyfunction!(invert_migration, parent)?)?;
    parent.add_function(wrap_pyfunction!(check_coverage, parent)?)?;
    Ok(())
}
