//! Error types for the lens DSL.

use miette::Diagnostic;
use thiserror::Error;

/// Errors arising from loading, evaluating, or compiling lens documents.
#[derive(Debug, Error, Diagnostic)]
pub enum LensDslError {
    /// Nickel evaluation failed, with no source span in the evaluated
    /// text (for instance, an error inside an imported module).
    #[error("nickel evaluation failed: {message}")]
    #[diagnostic(code(panproto_lens_dsl::nickel_eval))]
    NickelEval {
        /// Human-readable error message from the Nickel evaluator.
        message: String,
    },

    /// Nickel evaluation failed at a place in the evaluated text, which
    /// Nickel's own diagnostic locates.
    #[error("nickel evaluation failed: {message}")]
    #[diagnostic(code(panproto_lens_dsl::nickel_eval_spanned))]
    NickelEvalSpanned {
        /// Human-readable error message from the Nickel evaluator.
        message: String,
        /// The evaluated source text.
        #[source_code]
        src: String,
        /// The byte range Nickel's diagnostic names.
        #[label("here")]
        span: miette::SourceSpan,
    },

    /// JSON deserialization failed.
    #[error("JSON parse error: {0}")]
    #[diagnostic(code(panproto_lens_dsl::json))]
    Json(#[from] serde_json::Error),

    /// YAML deserialization failed.
    #[error("YAML parse error: {message}")]
    #[diagnostic(code(panproto_lens_dsl::yaml))]
    Yaml {
        /// Human-readable error message.
        message: String,
    },

    /// A document value breaks the document grammar.
    #[error("malformed document at '{path}': {problem}")]
    #[diagnostic(code(panproto_lens_dsl::shape))]
    Shape {
        /// The path of the refused value (for a missing field, of the
        /// object that lacks it).
        path: crate::path::DocumentPath,
        /// What is wrong with it.
        problem: crate::shape::ShapeProblem,
    },

    /// A refusal of the document element at `path`.
    #[error("{error} (at '{path}')")]
    #[diagnostic(code(panproto_lens_dsl::at))]
    At {
        /// The path of the element refused.
        path: crate::path::DocumentPath,
        /// The refusal.
        error: Box<Self>,
    },

    /// A refusal located in the source text the document was read from.
    #[error("{error}")]
    #[diagnostic(code(panproto_lens_dsl::spanned))]
    Spanned {
        /// The path of the element refused, when the refusal names one.
        path: Option<crate::path::DocumentPath>,
        /// The source text.
        #[source_code]
        src: String,
        /// The byte range of the refused text in `src`.
        #[label("here")]
        span: miette::SourceSpan,
        /// The refusal.
        error: Box<Self>,
    },

    /// Expression parsing failed.
    #[error("expression parse error: {message}")]
    #[diagnostic(code(panproto_lens_dsl::expr_parse))]
    ExprParse {
        /// Human-readable parse error.
        message: String,
    },

    /// A term in an equation was not in the term grammar.
    #[error("malformed term {term:?}: {message}")]
    #[diagnostic(code(panproto_lens_dsl::term_parse))]
    TermParse {
        /// The term text as the document wrote it.
        term: String,
        /// What the term grammar refused.
        message: String,
    },

    /// A default value could not be carried into the instance algebra
    /// as written, or contradicted its declared kind.
    #[error("invalid default: {message}")]
    #[diagnostic(code(panproto_lens_dsl::default_value))]
    DefaultValue {
        /// What was wrong with the default.
        message: String,
    },

    /// A protolens chain could not be fused into one protolens: a
    /// `scoped` step's inner stage, or a part of a horizontal composition,
    /// whose adjacent steps are not composable.
    #[error("cannot fuse the chain: {error}")]
    #[diagnostic(code(panproto_lens_dsl::fuse))]
    Fuse {
        /// The lens engine's refusal.
        error: Box<panproto_lens::LensError>,
    },

    /// A part of a horizontal composition compiles to the empty chain,
    /// which has no protolens to compose.
    #[error("a part of a horizontal composition is empty")]
    #[diagnostic(code(panproto_lens_dsl::empty_horizontal_part))]
    EmptyHorizontalPart,

    /// A step of a `symmetric` leg acts on values (`apply_expr`,
    /// `compute_field`, or `add_field` with an `expr`). A symmetric lens is
    /// assembled from the legs' protolens chains alone, so the value-level
    /// transform would be dropped.
    #[error("a symmetric leg cannot carry a value-level step")]
    #[diagnostic(code(panproto_lens_dsl::value_step_in_symmetric_leg))]
    ValueStepInSymmetricLeg,

    /// The named references of `compose` bodies form a cycle.
    #[error("lens references form a cycle: {}", cycle.join(" -> "))]
    #[diagnostic(code(panproto_lens_dsl::ref_cycle))]
    RefCycle {
        /// The references along the cycle, the repeated one last.
        cycle: Vec<String>,
    },

    /// A referenced lens document was refused.
    #[error("referenced lens '{lens_ref}' is refused: {error}")]
    #[diagnostic(code(panproto_lens_dsl::in_ref))]
    InRef {
        /// The reference.
        lens_ref: String,
        /// The refusal of the referenced document, located in it.
        error: Box<Self>,
    },

    /// A referenced lens was not found during composition.
    #[error("referenced lens '{lens_ref}' not found")]
    #[diagnostic(code(panproto_lens_dsl::unresolved_ref))]
    UnresolvedRef {
        /// The lens reference ID that could not be resolved.
        lens_ref: String,
    },

    /// A directed `compose` body referenced a symmetric lens. A symmetric
    /// lens is a span with two legs and therefore has no single directed
    /// chain that composition can consume without discarding information.
    #[error(
        "referenced lens '{lens_ref}' has a symmetric body and cannot be used in directed compose"
    )]
    #[diagnostic(code(panproto_lens_dsl::symmetric_ref_in_directional_compose))]
    SymmetricRefInDirectionalCompose {
        /// The referenced symmetric lens document ID.
        lens_ref: String,
    },

    /// IO error reading a lens file.
    #[error("IO error: {0}")]
    #[diagnostic(code(panproto_lens_dsl::io))]
    Io(#[from] std::io::Error),

    /// File extension not recognized.
    #[error("unsupported file extension '{ext}': expected .ncl, .json, .yaml, or .yml")]
    #[diagnostic(code(panproto_lens_dsl::unsupported_ext))]
    UnsupportedExtension {
        /// The extension that was not recognized.
        ext: String,
    },

    /// A rule's template could not be read.
    #[error("rule compilation error: {message}")]
    #[diagnostic(code(panproto_lens_dsl::rule_compile))]
    RuleCompile {
        /// Human-readable error.
        message: String,
    },

    /// An `auto` (or `from_diff`) body was compiled without schema
    /// context. Auto-generation and diff-based generation both require
    /// the source and target schemas plus a protocol; use
    /// [`compile_with_schemas`](crate::compile::compile_with_schemas)
    /// instead of the schema-less [`compile`](crate::compile::compile).
    #[error(
        "lens document '{id}' has an auto/from_diff body but was compiled without schemas: use compile_with_schemas"
    )]
    #[diagnostic(code(panproto_lens_dsl::auto_requires_schemas))]
    AutoRequiresSchemas {
        /// The lens document ID.
        id: String,
    },

    /// The compiled chain, applied to the source schema, produced a
    /// schema whose NSID does not match the document's declared
    /// `target`. Raised only by the schema-aware entry point
    /// [`compile_with_schemas`](crate::compile::compile_with_schemas);
    /// the schema-less [`compile`](crate::compile::compile) cannot
    /// perform this check.
    #[error(
        "lens document '{id}': declared target '{declared}' but the compiled chain produces '{actual}'"
    )]
    #[diagnostic(code(panproto_lens_dsl::target_mismatch))]
    TargetMismatch {
        /// The lens document ID.
        id: String,
        /// The `target` NSID declared in the document.
        declared: String,
        /// The NSID of the schema the compiled chain actually produces.
        actual: String,
    },

    /// The document declared `invertible: true` but the compiled chain
    /// contains a lossy element (a non-`Iso` step or field transform),
    /// so the lens cannot round-trip.
    #[error("lens document '{id}' declares invertible but contains a lossy element: {element}")]
    #[diagnostic(code(panproto_lens_dsl::not_invertible))]
    NotInvertible {
        /// The lens document ID.
        id: String,
        /// A description of the first lossy element found.
        element: String,
    },

    /// Auto-generation or diff-based generation of the chain failed
    /// inside the schema-aware entry point.
    #[error("lens document '{id}': schema-aware generation failed: {message}")]
    #[diagnostic(code(panproto_lens_dsl::generation))]
    Generation {
        /// The lens document ID.
        id: String,
        /// Human-readable error from the lens engine.
        message: String,
    },

    /// A `coerce_sort` step or a directed equation declares a coercion
    /// class whose forward and inverse expressions do not satisfy the
    /// class's round-trip laws on the sampled inputs, so the declaration
    /// cannot be honest. The check is evidence, not proof: the sampled
    /// inputs that fail to round-trip are enough to reject the declaration,
    /// though passing them would not prove honesty for every input.
    #[error("dishonest coercion declaration: {message}")]
    #[diagnostic(code(panproto_lens_dsl::coercion_not_honest))]
    CoercionNotHonest {
        /// The honesty check's rendering, carrying the per-sample
        /// violations and the evidence-not-proof caveat.
        message: String,
    },

    /// A concrete stage's migration has no executable restrict plan.
    #[error("lens document '{id}': migration plan compilation failed: {source}")]
    #[diagnostic(code(panproto_lens_dsl::plan_compilation))]
    PlanCompilation {
        /// The lens document ID.
        id: String,
        /// The typed plan compilation refusal.
        source: panproto_inst::RestrictError,
    },
}

impl LensDslError {
    /// This refusal, raised by an element at `prefix`, located in the
    /// enclosing document: a path the refusal already names is taken
    /// relative to `prefix`, and a refusal that names none is located at
    /// `prefix` itself.
    #[must_use]
    pub fn within(self, prefix: &crate::path::DocumentPath) -> Self {
        match self {
            Self::At { path, error } => Self::At {
                path: prefix.join(&path),
                error,
            },
            Self::Shape { path, problem } => Self::Shape {
                path: prefix.join(&path),
                problem,
            },
            Self::Spanned { .. } => self,
            error => Self::At {
                path: prefix.clone(),
                error: Box::new(error),
            },
        }
    }

    /// The document path this refusal names, if any.
    #[must_use]
    pub const fn path(&self) -> Option<&crate::path::DocumentPath> {
        match self {
            Self::At { path, .. } | Self::Shape { path, .. } => Some(path),
            Self::Spanned { path, .. } => path.as_ref(),
            _ => None,
        }
    }

    /// The refusal without its location, by value.
    #[must_use]
    pub fn into_unlocated(self) -> Self {
        match self {
            Self::At { error, .. } | Self::Spanned { error, .. } => error.into_unlocated(),
            other => other,
        }
    }

    /// The refusal without its location.
    #[must_use]
    pub fn unlocated(&self) -> &Self {
        match self {
            Self::At { error, .. } | Self::Spanned { error, .. } => error.unlocated(),
            other => other,
        }
    }
}

impl From<panproto_dsl_eval::DslEvalError> for LensDslError {
    fn from(err: panproto_dsl_eval::DslEvalError) -> Self {
        match err {
            panproto_dsl_eval::DslEvalError::NickelEval { message } => Self::NickelEval { message },
            panproto_dsl_eval::DslEvalError::NickelEvalSpanned { message, src, span } => {
                Self::NickelEvalSpanned {
                    message,
                    src,
                    span: miette::SourceSpan::new(span.0.into(), span.1),
                }
            }
            panproto_dsl_eval::DslEvalError::Json(e) => Self::Json(e),
            panproto_dsl_eval::DslEvalError::Yaml { message } => Self::Yaml { message },
            panproto_dsl_eval::DslEvalError::Io(e) => Self::Io(e),
        }
    }
}
