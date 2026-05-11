//! Python bindings for panproto GAT (generalized algebraic theory) operations.
//!
//! Wraps `panproto-gat`: theories, morphisms, models, colimits.
//! Note that `Model` contains function pointers (`Arc<dyn Fn>`) and
//! thus cannot be serialized or cloned. We expose it as an opaque
//! handle with limited introspection.

use std::path::Path;
use std::sync::Arc;

use pyo3::prelude::*;

use panproto_core::gat::{
    self, Equation, FreeModelConfig, Model, Operation, Sort, SortExpr, Theory, TheoryMorphism,
};
use panproto_theory_dsl::{
    TheoryBody, TheoryDocument,
    compile_class::compile_class,
    compile_inductive::compile_inductive,
    compile_theory::{compile_theory_with_resolver, parse_term},
    eval::{eval_json, eval_nickel, eval_yaml},
};

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

    /// Serialize this theory to a JSON string.
    ///
    /// The output is the flat ``panproto_gat::Theory`` serde shape (the
    /// same shape ``Theory.to_dict()`` returns, just rendered as a JSON
    /// document). Round-trips with :func:`Theory.from_dict_json`.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self.inner.as_ref())
            .map_err(|e| crate::error::GatError::new_err(format!("theory to_json failed: {e}")))
    }

    /// Serialize this theory to a YAML string.
    ///
    /// The output is the flat ``panproto_gat::Theory`` serde shape
    /// (same as :meth:`to_json`, rendered as YAML). Round-trips with
    /// :meth:`from_dict_yaml`.
    fn to_yaml(&self) -> PyResult<String> {
        yaml_serde::to_string(self.inner.as_ref())
            .map_err(|e| crate::error::GatError::new_err(format!("theory to_yaml failed: {e}")))
    }

    /// Construct a theory from the YAML-rendered ``panproto_gat::Theory``
    /// shape. Inverse of :meth:`to_yaml`.
    ///
    /// To compile a DSL document (the YAML lens-DSL surface), use
    /// :meth:`from_yaml` instead.
    #[classmethod]
    fn from_dict_yaml(_cls: &Bound<'_, pyo3::types::PyType>, payload: &str) -> PyResult<Self> {
        let theory: Theory = yaml_serde::from_str(payload).map_err(|e| {
            crate::error::GatError::new_err(format!("theory from_dict_yaml failed: {e}"))
        })?;
        Ok(Self {
            inner: Arc::new(theory),
        })
    }

    /// Construct a theory from the serialized ``panproto_gat::Theory`` shape.
    ///
    /// Inverse of :meth:`to_json`. The expected payload is a flat
    /// theory record with ``name``, ``sorts``, ``ops``, and ``eqs``
    /// keys; this is the round-trip path for a theory that was
    /// previously emitted with :meth:`to_json`.
    ///
    /// To compile a *DSL document* (the JSON / YAML / Nickel surface
    /// from ``panproto-theory-dsl``), use :meth:`from_json`,
    /// :meth:`from_yaml`, or :meth:`from_nickel` instead.
    #[classmethod]
    fn from_dict_json(_cls: &Bound<'_, pyo3::types::PyType>, payload: &str) -> PyResult<Self> {
        let theory: Theory = serde_json::from_str(payload).map_err(|e| {
            crate::error::GatError::new_err(format!("theory from_dict_json failed: {e}"))
        })?;
        Ok(Self {
            inner: Arc::new(theory),
        })
    }

    /// Compile a JSON DSL document into a theory.
    ///
    /// Accepts the JSON surface of ``panproto-theory-dsl``: a top-level
    /// document with ``id`` and ``description`` plus exactly one body
    /// variant of ``theory``, ``class``, or ``inductive``. The
    /// dependent-sort surface (``Tm(arrow(a, b))`` etc.) is supported on
    /// the same footing as the Rust ``class!`` / ``inductive!`` macros.
    ///
    /// Other body variants (``morphism``, ``compose``, ``protocol``,
    /// ``bundle``, ``instance``) cannot collapse to a single
    /// ``Theory`` and raise ``GatError``; use the DSL crate directly
    /// (or :meth:`Protocol.from_theories`) for those.
    #[classmethod]
    fn from_json(_cls: &Bound<'_, pyo3::types::PyType>, source: &str) -> PyResult<Self> {
        compile_dsl_to_theory(eval_json(source).map_err(|e| dsl_err(&e))?)
    }

    /// Compile a YAML DSL document into a theory.
    ///
    /// Same body-variant rules as :meth:`from_json`.
    #[classmethod]
    fn from_yaml(_cls: &Bound<'_, pyo3::types::PyType>, source: &str) -> PyResult<Self> {
        compile_dsl_to_theory(eval_yaml(source).map_err(|e| dsl_err(&e))?)
    }

    /// Compile a Nickel DSL document into a theory.
    ///
    /// Same body-variant rules as :meth:`from_json`. ``import_paths``
    /// (default empty) extends Nickel's import-resolution lookup so
    /// user-defined modules can be referenced from ``source``. The
    /// bundled ``panproto/theory.ncl`` contracts are always available
    /// without configuring an import path.
    #[classmethod]
    #[pyo3(signature = (source, import_paths=None))]
    fn from_nickel(
        _cls: &Bound<'_, pyo3::types::PyType>,
        source: &str,
        import_paths: Option<Vec<std::path::PathBuf>>,
    ) -> PyResult<Self> {
        let paths = import_paths.unwrap_or_default();
        compile_dsl_to_theory(eval_nickel(source, &paths).map_err(|e| dsl_err(&e))?)
    }

    /// Compile a DSL document from a path, dispatching on file
    /// extension (``.ncl`` → Nickel, ``.json`` → JSON, ``.yaml`` /
    /// ``.yml`` → YAML).
    #[classmethod]
    #[allow(clippy::needless_pass_by_value)] // pyo3 #[classmethod] requires an owned argument here.
    fn from_path(
        _cls: &Bound<'_, pyo3::types::PyType>,
        path: std::path::PathBuf,
    ) -> PyResult<Self> {
        let source = std::fs::read_to_string(&path).map_err(|e| {
            crate::error::GatError::new_err(format!("could not read {}: {e}", path.display()))
        })?;
        let ext = path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("");
        // Note: this match is case-sensitive. A file named
        // `theory.JSON` (uppercase extension) would fall through to
        // the unsupported-extension branch. Real-world theory files
        // virtually always use lowercase extensions; lowercasing
        // here would mask user mistakes more than it would help, so
        // we leave the strict match in place.
        let doc = match ext {
            "json" => eval_json(&source).map_err(|e| dsl_err(&e))?,
            "yaml" | "yml" => eval_yaml(&source).map_err(|e| dsl_err(&e))?,
            "ncl" => {
                let parent = path
                    .parent()
                    .map(Path::to_path_buf)
                    .into_iter()
                    .collect::<Vec<_>>();
                eval_nickel(&source, &parent).map_err(|e| dsl_err(&e))?
            }
            other => {
                return Err(crate::error::GatError::new_err(format!(
                    "unsupported theory file extension: {other:?}; expected .json, .yaml, .yml, or .ncl"
                )));
            }
        };
        compile_dsl_to_theory(doc)
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

/// Map a `panproto-theory-dsl` error to a Python exception.
fn dsl_err(e: &panproto_theory_dsl::TheoryDslError) -> PyErr {
    crate::error::GatError::new_err(format!("theory DSL error: {e}"))
}

/// Compile a parsed DSL document into a single `Theory`, or raise.
///
/// Accepted body variants are `Theory`, `Class`, and `Inductive` —
/// the three that collapse to a single `panproto_gat::Theory`. Other
/// body variants describe morphisms, protocol bundles, or composition
/// graphs that cannot be returned as one `Theory`; they raise an error
/// pointing the caller at the DSL crate (or `Protocol.from_theories`)
/// for the multi-output cases.
fn compile_dsl_to_theory(doc: TheoryDocument) -> PyResult<PyTheory> {
    let theory = match doc.body {
        TheoryBody::Theory(spec) => {
            let resolver = panproto_theory_dsl::builtin_resolver();
            compile_theory_with_resolver(&spec, &resolver).map_err(|e| dsl_err(&e))?
        }
        TheoryBody::Class(spec) => compile_class(&spec).map_err(|e| dsl_err(&e))?,
        TheoryBody::Inductive(spec) => compile_inductive(&spec).map_err(|e| dsl_err(&e))?,
        TheoryBody::Morphism(_)
        | TheoryBody::Composition(_)
        | TheoryBody::Protocol(_)
        | TheoryBody::Bundle(_)
        | TheoryBody::Instance(_) => {
            return Err(crate::error::GatError::new_err(
                "DSL document does not produce a single Theory; \
                 only `theory`, `class`, and `inductive` bodies are \
                 supported by Theory.from_*. Use the panproto-theory-dsl \
                 crate directly for morphism / composition / protocol / \
                 bundle / instance documents.",
            ));
        }
    };
    Ok(PyTheory {
        inner: Arc::new(theory),
    })
}

// ---------------------------------------------------------------------------
// PyTheoryBuilder — fluent builder for incremental theory construction
// ---------------------------------------------------------------------------

/// Fluent builder for ``Theory`` values.
///
/// Mirrors :class:`SchemaBuilder` and :class:`MigrationBuilder`. Each
/// chainable method appends one element (sort, operation, equation) to
/// the in-progress theory; :meth:`build` produces the immutable
/// ``Theory`` ready to feed into :func:`colimit_theories`,
/// :func:`free_model`, the migration engine, etc.
///
/// Existing ``create_theory(dict)`` callers keep working unchanged;
/// this is purely an additional surface for theories that are easier
/// to read as a sequence of declarations than as one nested literal.
#[pyclass(name = "TheoryBuilder", module = "panproto._native")]
pub struct PyTheoryBuilder {
    name: String,
    sorts: Vec<Sort>,
    ops: Vec<Operation>,
    eqs: Vec<Equation>,
}

#[pymethods]
impl PyTheoryBuilder {
    /// Start a new builder for a theory of the given ``name``.
    #[new]
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            sorts: Vec::new(),
            ops: Vec::new(),
            eqs: Vec::new(),
        }
    }

    /// Append a simple sort.
    fn sort<'py>(slf: Bound<'py, Self>, name: &str) -> Bound<'py, Self> {
        slf.borrow_mut().sorts.push(Sort::simple(name));
        slf
    }

    /// Append an operation.
    ///
    /// Parameters
    /// ----------
    /// `name` : str
    ///     Operation name.
    /// `inputs` : Sequence[str]
    ///     Input sort names. Each ``"S"`` becomes a ``SortExpr::Name(S)``;
    ///     pass ``"Tm(arrow(a, b))"`` to denote a dependent input sort,
    ///     parsed by the panproto-theory-dsl term parser.
    /// `output` : str
    ///     Output sort, in the same surface form as ``inputs``.
    /// `input_names` : Sequence[str], optional
    ///     Parameter names paired with ``inputs``. Defaults to
    ///     ``["x0", "x1", ...]`` when omitted; useful when the caller
    ///     wants stable variable names for axioms that reference them.
    #[pyo3(signature = (name, inputs, output, input_names=None))]
    #[allow(clippy::needless_pass_by_value)] // pyo3 #[pymethods] take owned arguments here.
    fn op<'py>(
        slf: Bound<'py, Self>,
        name: &str,
        inputs: Vec<String>,
        output: &str,
        input_names: Option<Vec<String>>,
    ) -> PyResult<Bound<'py, Self>> {
        let names =
            input_names.unwrap_or_else(|| (0..inputs.len()).map(|i| format!("x{i}")).collect());
        if names.len() != inputs.len() {
            return Err(crate::error::GatError::new_err(format!(
                "TheoryBuilder.op({name:?}): input_names has {} entries but inputs has {}",
                names.len(),
                inputs.len()
            )));
        }
        let inputs_se = inputs
            .iter()
            .zip(names.iter())
            .map(|(sort_src, var)| Ok((Arc::from(var.as_str()), sort_str_to_sort_expr(sort_src)?)))
            .collect::<PyResult<Vec<(Arc<str>, SortExpr)>>>()?;
        let out = sort_str_to_sort_expr(output)?;
        slf.borrow_mut()
            .ops
            .push(Operation::new(name, inputs_se, out));
        Ok(slf)
    }

    /// Append an equational axiom.
    ///
    /// ``lhs`` and ``rhs`` are parsed as terms by the panproto-theory-dsl
    /// term parser, so ``"transpose(p, 0)"`` and ``"p"`` work directly.
    fn eq<'py>(
        slf: Bound<'py, Self>,
        name: &str,
        lhs: &str,
        rhs: &str,
    ) -> PyResult<Bound<'py, Self>> {
        let lhs_t = parse_term(lhs).map_err(|e| {
            crate::error::GatError::new_err(format!(
                "TheoryBuilder.eq({name:?}): lhs parse error: {e}"
            ))
        })?;
        let rhs_t = parse_term(rhs).map_err(|e| {
            crate::error::GatError::new_err(format!(
                "TheoryBuilder.eq({name:?}): rhs parse error: {e}"
            ))
        })?;
        slf.borrow_mut().eqs.push(Equation::new(name, lhs_t, rhs_t));
        Ok(slf)
    }

    /// Finalize and return the constructed ``Theory``.
    fn build(&self) -> PyTheory {
        PyTheory {
            inner: Arc::new(Theory::new(
                self.name.as_str(),
                self.sorts.clone(),
                self.ops.clone(),
                self.eqs.clone(),
            )),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "TheoryBuilder({:?}, sorts={}, ops={}, eqs={})",
            self.name,
            self.sorts.len(),
            self.ops.len(),
            self.eqs.len()
        )
    }
}

/// Parse a sort surface form (``"S"`` or ``"S(t1, t2)"``) into a
/// [`SortExpr`].
///
/// Bare identifiers produce [`SortExpr::Name`]; applied identifiers
/// produce [`SortExpr::App`] with [`Term`]-valued arguments. Reuses
/// the same term parser as the JSON / YAML / Nickel surface so the
/// fluent builder accepts the dependent-sort syntax (``"Tm(arrow(a,
/// b))"``) on the same footing as the macro and DSL paths.
fn sort_str_to_sort_expr(s: &str) -> PyResult<SortExpr> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(crate::error::GatError::new_err("empty sort expression"));
    }
    let term = parse_term(trimmed).map_err(|e| {
        crate::error::GatError::new_err(format!("sort expression {s:?} parse error: {e}"))
    })?;
    match term {
        panproto_core::gat::Term::Var(name) => Ok(SortExpr::Name(name)),
        panproto_core::gat::Term::App { op, args } => Ok(SortExpr::app(op, args)),
        other => Err(crate::error::GatError::new_err(format!(
            "sort expression {s:?} produced unsupported term shape: {other:?}"
        ))),
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
    let result = gat::free_model(&theory.inner, &config)
        .map_err(|e| crate::error::GatError::new_err(format!("free model failed: {e}")))?;
    Ok(PyModel {
        inner: result.model,
    })
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
    parent.add_class::<PyTheoryBuilder>()?;
    parent.add_class::<PyModel>()?;
    parent.add_function(wrap_pyfunction!(create_theory, parent)?)?;
    parent.add_function(wrap_pyfunction!(colimit_theories, parent)?)?;
    parent.add_function(wrap_pyfunction!(check_morphism, parent)?)?;
    parent.add_function(wrap_pyfunction!(migrate_model, parent)?)?;
    parent.add_function(wrap_pyfunction!(free_model, parent)?)?;
    parent.add_function(wrap_pyfunction!(check_model, parent)?)?;
    Ok(())
}
