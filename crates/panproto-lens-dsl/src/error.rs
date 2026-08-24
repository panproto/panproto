//! Error types for the lens DSL.

use miette::Diagnostic;
use thiserror::Error;

/// Errors arising from loading, evaluating, or compiling lens documents.
#[derive(Debug, Error, Diagnostic)]
pub enum LensDslError {
    /// Nickel evaluation failed.
    #[error("nickel evaluation failed: {message}")]
    #[diagnostic(code(panproto_lens_dsl::nickel_eval))]
    NickelEval {
        /// Human-readable error message from the Nickel evaluator.
        message: String,
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

    /// The lens document has no body variant (steps, rules, compose, or auto).
    #[error("lens document '{id}' has no body: expected one of steps, rules, compose, or auto")]
    #[diagnostic(code(panproto_lens_dsl::no_body))]
    NoBody {
        /// The lens document ID.
        id: String,
    },

    /// The lens document has multiple body variants.
    #[error("lens document '{id}' has multiple bodies: {variants}")]
    #[diagnostic(code(panproto_lens_dsl::multiple_bodies))]
    MultipleBodies {
        /// The lens document ID.
        id: String,
        /// Comma-separated list of present variants.
        variants: String,
    },

    /// An unrecognized step variant was encountered.
    #[error("unrecognized step at index {index}: could not match any known step variant")]
    #[diagnostic(code(panproto_lens_dsl::unknown_step))]
    UnknownStep {
        /// Zero-based index of the step in the pipeline.
        index: usize,
    },

    /// Expression parsing failed.
    #[error("expression parse error in step {step_desc}: {message}")]
    #[diagnostic(code(panproto_lens_dsl::expr_parse))]
    ExprParse {
        /// Description of the step containing the expression.
        step_desc: String,
        /// Human-readable parse error.
        message: String,
    },

    /// A term in an equation was not in the term grammar.
    #[error("malformed term in {step_desc}: {term:?}: {message}")]
    #[diagnostic(code(panproto_lens_dsl::term_parse))]
    TermParse {
        /// Description of the step containing the term.
        step_desc: String,
        /// The term text as the document wrote it.
        term: String,
        /// What the term grammar refused.
        message: String,
    },

    /// A default value could not be carried into the instance algebra
    /// as written, or contradicted its declared kind.
    #[error("invalid default in {step_desc}: {message}")]
    #[diagnostic(code(panproto_lens_dsl::default_value))]
    DefaultValue {
        /// Description of the step declaring the default.
        step_desc: String,
        /// What was wrong with the default.
        message: String,
    },

    /// A referenced lens was not found during composition.
    #[error("referenced lens '{lens_ref}' not found")]
    #[diagnostic(code(panproto_lens_dsl::unresolved_ref))]
    UnresolvedRef {
        /// The lens reference ID that could not be resolved.
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

    /// Rule compilation failed.
    #[error("rule compilation error at rule index {index}: {message}")]
    #[diagnostic(code(panproto_lens_dsl::rule_compile))]
    RuleCompile {
        /// Zero-based index of the rule.
        index: usize,
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

    /// A `coerce_sort` step declares a coercion class whose forward and
    /// inverse expressions do not satisfy the class's round-trip laws on
    /// the sampled inputs, so the declaration cannot be honest. The check
    /// is evidence, not proof: the sampled inputs that fail to round-trip
    /// are enough to reject the declaration, though passing them would not
    /// prove honesty for every input.
    #[error("coerce_sort step {step_desc}: dishonest coercion declaration: {message}")]
    #[diagnostic(code(panproto_lens_dsl::coercion_not_honest))]
    CoercionNotHonest {
        /// Description of the step whose declared coercion failed
        /// verification (e.g. `coerce_sort[2]`).
        step_desc: String,
        /// The honesty check's rendering, carrying the per-sample
        /// violations and the evidence-not-proof caveat.
        message: String,
    },
}

impl From<panproto_dsl_eval::DslEvalError> for LensDslError {
    fn from(err: panproto_dsl_eval::DslEvalError) -> Self {
        match err {
            panproto_dsl_eval::DslEvalError::NickelEval { message }
            | panproto_dsl_eval::DslEvalError::NickelEvalSpanned { message, .. } => {
                Self::NickelEval { message }
            }
            panproto_dsl_eval::DslEvalError::Json(e) => Self::Json(e),
            panproto_dsl_eval::DslEvalError::Yaml { message } => Self::Yaml { message },
            panproto_dsl_eval::DslEvalError::Io(e) => Self::Io(e),
        }
    }
}
