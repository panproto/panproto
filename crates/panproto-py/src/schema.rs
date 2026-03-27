//! Python bindings for panproto schema types.
//!
//! Exposes [`Protocol`], [`Schema`], [`SchemaBuilder`], [`Vertex`],
//! [`Edge`], [`HyperEdge`], and [`Constraint`] as Python classes.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::schema::{
    self, Constraint, Edge, HyperEdge, Protocol, Schema, SchemaBuilder, Vertex,
};

use crate::convert;
use crate::error::MapPyErr;

// ---------------------------------------------------------------------------
// PyVertex
// ---------------------------------------------------------------------------

/// A schema vertex.
#[pyclass(name = "Vertex", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyVertex {
    pub(crate) inner: Vertex,
}

#[pymethods]
impl PyVertex {
    /// Unique vertex identifier within the schema.
    #[getter]
    fn id(&self) -> &str {
        self.inner.id.as_ref()
    }

    /// Vertex kind (e.g., ``"record"``, ``"object"``, ``"string"``).
    #[getter]
    fn kind(&self) -> &str {
        self.inner.kind.as_ref()
    }

    /// Optional namespace identifier (e.g., ``"app.bsky.feed.post"``).
    #[getter]
    fn nsid(&self) -> Option<&str> {
        self.inner.nsid.as_deref()
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "Vertex(id={:?}, kind={:?}{})",
            self.inner.id.as_ref(),
            self.inner.kind.as_ref(),
            self.inner
                .nsid
                .as_deref()
                .map_or(String::new(), |n| format!(", nsid={n:?}"))
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

// ---------------------------------------------------------------------------
// PyEdge
// ---------------------------------------------------------------------------

/// A directed binary edge between two vertices.
#[pyclass(name = "Edge", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyEdge {
    pub(crate) inner: Edge,
}

#[pymethods]
impl PyEdge {
    /// Source vertex ID.
    #[getter]
    fn src(&self) -> &str {
        self.inner.src.as_ref()
    }

    /// Target vertex ID.
    #[getter]
    fn tgt(&self) -> &str {
        self.inner.tgt.as_ref()
    }

    /// Edge kind (e.g., ``"prop"``, ``"record-schema"``).
    #[getter]
    fn kind(&self) -> &str {
        self.inner.kind.as_ref()
    }

    /// Optional edge label (e.g., a property name).
    #[getter]
    fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!(
            "Edge({:?} -> {:?}, kind={:?}{})",
            self.inner.src.as_ref(),
            self.inner.tgt.as_ref(),
            self.inner.kind.as_ref(),
            self.inner
                .name
                .as_deref()
                .map_or(String::new(), |n| format!(", name={n:?}"))
        )
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

// ---------------------------------------------------------------------------
// PyConstraint
// ---------------------------------------------------------------------------

/// A constraint on a schema vertex.
#[pyclass(name = "Constraint", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyConstraint {
    pub(crate) inner: Constraint,
}

#[pymethods]
impl PyConstraint {
    /// Constraint sort (e.g., ``"maxLength"``, ``"format"``).
    #[getter]
    fn sort(&self) -> &str {
        self.inner.sort.as_ref()
    }

    /// Constraint value (e.g., ``"3000"``, ``"at-uri"``).
    #[getter]
    fn value(&self) -> &str {
        &self.inner.value
    }

    fn __repr__(&self) -> String {
        format!(
            "Constraint(sort={:?}, value={:?})",
            self.inner.sort.as_ref(),
            self.inner.value
        )
    }
}

// ---------------------------------------------------------------------------
// PyHyperEdge
// ---------------------------------------------------------------------------

/// A hyper-edge connecting multiple vertices via a labeled signature.
#[pyclass(name = "HyperEdge", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyHyperEdge {
    pub(crate) inner: HyperEdge,
}

#[pymethods]
impl PyHyperEdge {
    #[getter]
    fn id(&self) -> &str {
        self.inner.id.as_ref()
    }

    #[getter]
    fn kind(&self) -> &str {
        self.inner.kind.as_ref()
    }

    /// Label-to-vertex mapping.
    #[getter]
    fn signature(&self) -> HashMap<String, String> {
        self.inner
            .signature
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[getter]
    fn parent_label(&self) -> &str {
        self.inner.parent_label.as_ref()
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }
}

// ---------------------------------------------------------------------------
// PyProtocol
// ---------------------------------------------------------------------------

/// A protocol specification defining schema and instance theories.
///
/// Protocols are the Level-1 configuration objects that drive schema
/// construction and validation. Each protocol names a schema theory GAT,
/// an instance theory GAT, and supplies edge rules and recognized vertex kinds.
#[pyclass(name = "Protocol", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PyProtocol {
    pub(crate) inner: Protocol,
}

#[pymethods]
impl PyProtocol {
    /// Human-readable protocol name (e.g., ``"atproto"``, ``"brat"``).
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Name of the schema theory GAT.
    #[getter]
    fn schema_theory(&self) -> &str {
        &self.inner.schema_theory
    }

    /// Name of the instance theory GAT.
    #[getter]
    fn instance_theory(&self) -> &str {
        &self.inner.instance_theory
    }

    /// Recognized vertex kinds (``"object"``, ``"record"``, etc.).
    #[getter]
    fn obj_kinds(&self) -> Vec<String> {
        self.inner.obj_kinds.clone()
    }

    /// Recognized constraint sorts (``"maxLength"``, ``"format"``, etc.).
    #[getter]
    fn constraint_sorts(&self) -> Vec<String> {
        self.inner.constraint_sorts.clone()
    }

    /// Well-formedness rules for edges.
    #[getter]
    fn edge_rules(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner.edge_rules)
    }

    /// Create a new ``SchemaBuilder`` for this protocol.
    fn schema(&self) -> PySchemaBuilder {
        PySchemaBuilder {
            builder: Some(SchemaBuilder::new(&self.inner)),
            protocol: self.inner.clone(),
        }
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        format!("Protocol({:?})", self.inner.name)
    }
}

// ---------------------------------------------------------------------------
// PySchemaBuilder
// ---------------------------------------------------------------------------

/// Fluent builder for constructing validated schemas.
///
/// Each ``vertex()`` and ``edge()`` call validates against the protocol's
/// edge rules. The builder is consumed on ``build()``.
///
/// Methods mutate in place and return ``self`` for `chaining::`
///
///     schema = (protocol.schema()
///         .vertex("users", "TABLE")
///         .edge("users", "users.id", "COLUMN")
///         .constraint("users.id", "PRIMARY_KEY", "")
///         .build())
#[pyclass(name = "SchemaBuilder", module = "panproto._native")]
pub struct PySchemaBuilder {
    /// `Option` so we can `take()` on `build()`.
    builder: Option<SchemaBuilder>,
    #[allow(dead_code)]
    protocol: Protocol,
}

#[pymethods]
impl PySchemaBuilder {
    /// Add a vertex to the schema.
    ///
    /// Parameters
    /// ----------
    /// id : str
    ///     Unique vertex identifier.
    /// kind : str
    ///     Vertex kind, must be recognized by the protocol.
    /// nsid : str or None
    ///     Optional namespace identifier.
    ///
    /// Raises
    /// ------
    /// `SchemaValidationError`
    ///     If the vertex ID is a duplicate or the kind is unrecognized.
    #[pyo3(signature = (id, kind, nsid=None))]
    fn vertex(&mut self, id: &str, kind: &str, nsid: Option<&str>) -> PyResult<()> {
        let builder = self.take_builder()?;
        self.builder = Some(builder.vertex(id, kind, nsid).map_py_err()?);
        Ok(())
    }

    /// Add a directed edge between two vertices.
    ///
    /// Parameters
    /// ----------
    /// src : str
    ///     Source vertex ID.
    /// tgt : str
    ///     Target vertex ID.
    /// kind : str
    ///     Edge kind (e.g., ``"prop"``, ``"record-schema"``).
    /// name : str or None
    ///     Optional edge label.
    #[pyo3(signature = (src, tgt, kind, name=None))]
    fn edge(&mut self, src: &str, tgt: &str, kind: &str, name: Option<&str>) -> PyResult<()> {
        let builder = self.take_builder()?;
        self.builder = Some(builder.edge(src, tgt, kind, name).map_py_err()?);
        Ok(())
    }

    /// Add a hyper-edge connecting multiple vertices.
    ///
    /// Parameters
    /// ----------
    /// id : str
    ///     Unique hyper-edge identifier.
    /// kind : str
    ///     Hyper-edge kind.
    /// signature : dict[str, str]
    ///     Label-to-vertex mapping.
    /// parent : str
    ///     The label identifying the parent vertex.
    fn hyper_edge(
        &mut self,
        id: &str,
        kind: &str,
        signature: HashMap<String, String>,
        parent: &str,
    ) -> PyResult<()> {
        let builder = self.take_builder()?;
        self.builder = Some(
            builder
                .hyper_edge(id, kind, signature, parent)
                .map_py_err()?,
        );
        Ok(())
    }

    /// Add a constraint to a vertex.
    ///
    /// Parameters
    /// ----------
    /// `vertex_id` : str
    ///     The vertex to constrain.
    /// sort : str
    ///     Constraint sort (e.g., ``"maxLength"``).
    /// value : str
    ///     Constraint value (e.g., ``"3000"``).
    fn constraint(&mut self, vertex_id: &str, sort: &str, value: &str) -> PyResult<()> {
        let builder = self.take_builder()?;
        self.builder = Some(builder.constraint(vertex_id, sort, value));
        Ok(())
    }

    /// Consume the builder and produce a validated ``Schema``.
    ///
    /// Returns
    /// -------
    /// Schema
    ///     The built schema with precomputed adjacency indices.
    ///
    /// Raises
    /// ------
    /// `SchemaValidationError`
    ///     If the schema is empty or otherwise invalid.
    fn build(&mut self) -> PyResult<PySchema> {
        let builder = self.take_builder()?;
        let schema = builder.build().map_py_err()?;
        Ok(PySchema {
            inner: Arc::new(schema),
        })
    }
}

impl PySchemaBuilder {
    fn take_builder(&mut self) -> PyResult<SchemaBuilder> {
        self.builder.take().ok_or_else(|| {
            crate::error::SchemaValidationError::new_err("builder already consumed by build()")
        })
    }
}

// ---------------------------------------------------------------------------
// PySchema
// ---------------------------------------------------------------------------

/// A validated schema with precomputed adjacency indices.
///
/// Schemas are immutable once built. Use ``SchemaBuilder`` to construct one.
#[pyclass(name = "Schema", frozen, module = "panproto._native")]
#[derive(Clone)]
pub struct PySchema {
    pub(crate) inner: Arc<Schema>,
}

#[pymethods]
impl PySchema {
    /// The protocol name this schema belongs to.
    #[getter]
    fn protocol(&self) -> &str {
        &self.inner.protocol
    }

    /// Number of vertices.
    #[getter]
    fn vertex_count(&self) -> usize {
        self.inner.vertex_count()
    }

    /// Number of edges.
    #[getter]
    fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// All vertices as a list.
    #[getter]
    fn vertices(&self) -> Vec<PyVertex> {
        self.inner
            .vertices
            .values()
            .map(|v| PyVertex { inner: v.clone() })
            .collect()
    }

    /// All edges as a list.
    #[getter]
    fn edges(&self) -> Vec<PyEdge> {
        self.inner
            .edges
            .keys()
            .map(|e| PyEdge { inner: e.clone() })
            .collect()
    }

    /// All hyper-edges as a list.
    #[getter]
    fn hyper_edges(&self) -> Vec<PyHyperEdge> {
        self.inner
            .hyper_edges
            .values()
            .map(|h| PyHyperEdge { inner: h.clone() })
            .collect()
    }

    /// Look up a vertex by ID. Returns ``None`` if not found.
    fn vertex(&self, id: &str) -> Option<PyVertex> {
        self.inner.vertex(id).map(|v| PyVertex { inner: v.clone() })
    }

    /// Outgoing edges from a vertex.
    fn outgoing_edges(&self, vertex_id: &str) -> Vec<PyEdge> {
        self.inner
            .outgoing_edges(vertex_id)
            .iter()
            .map(|e| PyEdge { inner: e.clone() })
            .collect()
    }

    /// Incoming edges to a vertex.
    fn incoming_edges(&self, vertex_id: &str) -> Vec<PyEdge> {
        self.inner
            .incoming_edges(vertex_id)
            .iter()
            .map(|e| PyEdge { inner: e.clone() })
            .collect()
    }

    /// Constraints on a vertex.
    fn constraints_for(&self, vertex_id: &str) -> Vec<PyConstraint> {
        self.inner
            .constraints
            .get(vertex_id)
            .map_or_else(Vec::new, |cs| {
                cs.iter()
                    .map(|c| PyConstraint { inner: c.clone() })
                    .collect()
            })
    }

    /// Whether the given vertex ID exists.
    fn has_vertex(&self, id: &str) -> bool {
        self.inner.has_vertex(id)
    }

    /// Normalize the schema (collapse ref-chains).
    fn normalize(&self) -> Self {
        let normalized = schema::normalize(&self.inner);
        Self {
            inner: Arc::new(normalized),
        }
    }

    /// Validate the schema against a protocol, returning a list of issues.
    fn validate(&self, protocol: &PyProtocol) -> Vec<String> {
        let errors = schema::validate(&self.inner, &protocol.inner);
        errors.into_iter().map(|e| e.to_string()).collect()
    }

    /// Serialize the schema to a Python dict via serde.
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        convert::to_python(py, self.inner.as_ref())
    }

    /// Serialize the schema to a JSON string.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self.inner.as_ref()).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("JSON serialization failed: {e}"))
        })
    }

    /// Deserialize a schema from a JSON string.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let schema: Schema = serde_json::from_str(json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("JSON deserialization failed: {e}"))
        })?;
        Ok(Self {
            inner: Arc::new(schema),
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Schema(protocol={:?}, vertices={}, edges={})",
            self.inner.protocol,
            self.inner.vertex_count(),
            self.inner.edge_count()
        )
    }

    fn __len__(&self) -> usize {
        self.inner.vertex_count()
    }
}

/// Register schema types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<PyVertex>()?;
    parent.add_class::<PyEdge>()?;
    parent.add_class::<PyConstraint>()?;
    parent.add_class::<PyHyperEdge>()?;
    parent.add_class::<PyProtocol>()?;
    parent.add_class::<PySchemaBuilder>()?;
    parent.add_class::<PySchema>()?;
    Ok(())
}
