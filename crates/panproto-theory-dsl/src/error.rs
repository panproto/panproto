//! Error types for the theory DSL.

use miette::Diagnostic;
use thiserror::Error;

/// Errors arising from loading, evaluating, or compiling theory documents.
#[derive(Debug, Error, Diagnostic)]
pub enum TheoryDslError {
    /// Nickel evaluation failed, with no locatable source span.
    ///
    /// The fallback for [`Self::NickelEvalSpanned`]: raised when the
    /// Nickel diagnostic carries no label that resolves to a byte range
    /// within the evaluated source (for instance, an error originating
    /// wholly inside an imported module).
    #[error("nickel evaluation failed: {message}")]
    #[diagnostic(code(panproto_theory_dsl::nickel_eval))]
    NickelEval {
        /// Human-readable error message from the Nickel evaluator.
        message: String,
    },

    /// Nickel evaluation failed, with a source-pointed diagnostic.
    ///
    /// Populated when the Nickel diagnostic carries a label whose byte
    /// range falls within the evaluated source, so a compile error from
    /// a malformed canonical (`.ncl`) theory points at the offending
    /// location instead of losing it through Nickel evaluation.
    #[error("nickel evaluation failed: {message}")]
    #[diagnostic(code(panproto_theory_dsl::nickel_eval_spanned))]
    NickelEvalSpanned {
        /// Human-readable error message from the Nickel evaluator.
        message: String,
        /// The evaluated source text, used by miette for rendering.
        #[source_code]
        src: String,
        /// Span into `src` pointing at the failing element.
        #[label("here")]
        span: miette::SourceSpan,
    },

    /// JSON deserialization failed.
    #[error("JSON parse error: {0}")]
    #[diagnostic(code(panproto_theory_dsl::json))]
    Json(#[from] serde_json::Error),

    /// YAML deserialization failed.
    #[error("YAML parse error: {message}")]
    #[diagnostic(code(panproto_theory_dsl::yaml))]
    Yaml {
        /// Human-readable error message.
        message: String,
    },

    /// File extension not recognized.
    #[error("unsupported file extension '{ext}': expected .ncl, .json, .yaml, or .yml")]
    #[diagnostic(code(panproto_theory_dsl::unsupported_ext))]
    UnsupportedExtension {
        /// The extension that was not recognized.
        ext: String,
    },

    /// Term parsing failed.
    #[error("term parse error in {context}: {message}")]
    #[diagnostic(code(panproto_theory_dsl::term_parse))]
    TermParse {
        /// Where the parse error occurred.
        context: String,
        /// Human-readable parse error.
        message: String,
    },

    /// Expression parsing failed.
    #[error("expression parse error in {context}: {message}")]
    #[diagnostic(code(panproto_theory_dsl::expr_parse))]
    ExprParse {
        /// Where the parse error occurred.
        context: String,
        /// Human-readable parse error.
        message: String,
    },

    /// A referenced theory was not found.
    #[error("theory '{name}' not found (referenced in {context})")]
    #[diagnostic(code(panproto_theory_dsl::theory_not_found))]
    TheoryNotFound {
        /// The missing theory name.
        name: String,
        /// Where it was referenced.
        context: String,
    },

    /// Type-checking a theory failed: its signature is ill formed, an
    /// implicit input cannot be inferred, a closed sort's constructors are
    /// wrong, or an equation is ill typed.
    #[error("type error in theory '{theory}': {error}")]
    #[diagnostic(code(panproto_theory_dsl::typecheck))]
    TypeCheck {
        /// The theory that failed typechecking.
        theory: String,
        /// The checker's refusal.
        error: panproto_gat::GatError,
    },

    /// A theory's directed rewrite system is not provably sound.
    ///
    /// Normalization decides the theory's equality judgment by running
    /// the rewrite system, so a system that is not locally confluent or
    /// not LPO-terminating leaves that judgment unsound. Each entry
    /// names a non-joining critical pair or an LPO violation.
    #[error("theory '{theory}' has an unsound rewrite system: {}", warnings.join("; "))]
    #[diagnostic(code(panproto_theory_dsl::unsound_rewrite_system))]
    UnsoundRewriteSystem {
        /// The theory whose rewrite system was refused.
        theory: String,
        /// One entry per non-joining critical pair or LPO violation.
        warnings: Vec<String>,
    },

    /// The soundness analysis of a theory's rewrite system could not be
    /// completed, so the system is neither proven sound nor refuted.
    #[error("theory '{theory}': rewrite-system check could not be completed: {message}")]
    #[diagnostic(code(panproto_theory_dsl::rewrite_system_check))]
    RewriteSystemCheck {
        /// The theory whose rewrite system could not be analysed.
        theory: String,
        /// What stopped the analysis.
        message: String,
    },

    /// A document value breaks the document grammar.
    #[error("malformed document at '{path}': {problem}")]
    #[diagnostic(code(panproto_theory_dsl::shape))]
    Shape {
        /// The path of the refused value (for a missing field, of the
        /// object that lacks it).
        path: crate::path::DocumentPath,
        /// What is wrong with it.
        problem: crate::shape::ShapeProblem,
    },

    /// A refusal of the document element at `path`.
    #[error("{error} (at '{path}')")]
    #[diagnostic(code(panproto_theory_dsl::at))]
    At {
        /// The path of the element refused.
        path: crate::path::DocumentPath,
        /// The refusal.
        error: Box<Self>,
    },

    /// A refusal located in the source text the document was read from.
    #[error("{error}")]
    #[diagnostic(code(panproto_theory_dsl::spanned))]
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

    /// An operation declares both the unary `input` shorthand and an
    /// `inputs` list.
    #[error("operation '{op}' declares both `input` and `inputs`")]
    #[diagnostic(code(panproto_theory_dsl::ambiguous_inputs))]
    AmbiguousOpInputs {
        /// The operation.
        op: String,
    },

    /// An import exposes a name the imported theory does not declare.
    #[error("import of '{from}' exposes '{name}', which it does not declare")]
    #[diagnostic(code(panproto_theory_dsl::unknown_exposed))]
    UnknownExposed {
        /// The imported theory.
        from: String,
        /// The exposed name.
        name: String,
    },

    /// A morphism's sort map or operation map has an entry for a symbol its
    /// domain does not declare.
    #[error("morphism '{morphism}' maps {kind} '{name}', which its domain does not declare")]
    #[diagnostic(code(panproto_theory_dsl::unknown_map_key))]
    UnknownMapKey {
        /// The morphism.
        morphism: String,
        /// `sort` or `op`.
        kind: String,
        /// The entry's key.
        name: String,
    },

    /// Morphism validation failed.
    #[error("morphism check failed for '{morphism}': {error}")]
    #[diagnostic(code(panproto_theory_dsl::morphism_check))]
    MorphismCheck {
        /// The morphism that failed validation.
        morphism: String,
        /// The checker's refusal.
        error: panproto_gat::GatError,
    },

    /// Colimit composition failed.
    #[error("colimit failed at step {step}: {message}")]
    #[diagnostic(code(panproto_theory_dsl::colimit_failed))]
    ColimitFailed {
        /// Zero-based step index.
        step: usize,
        /// Human-readable error.
        message: String,
    },

    /// Duplicate definition within a document.
    #[error("duplicate definition: {kind} '{name}' defined twice")]
    #[diagnostic(code(panproto_theory_dsl::duplicate))]
    Duplicate {
        /// The kind of definition (theory, morphism, etc.).
        kind: String,
        /// The duplicated name.
        name: String,
    },

    /// IO error reading a theory file.
    #[error("IO error: {0}")]
    #[diagnostic(code(panproto_theory_dsl::io))]
    Io(#[from] std::io::Error),

    /// GAT engine error.
    #[error("GAT engine error: {0}")]
    #[diagnostic(code(panproto_theory_dsl::gat))]
    Gat(#[from] panproto_gat::GatError),

    /// A value-kind string in a theory spec is not one of the recognised
    /// kinds.
    #[error(
        "unknown value kind '{kind}': expected one of boolean/bool, \
         integer/int, float/number, string/str, bytes, token, null, or any"
    )]
    #[diagnostic(code(panproto_theory_dsl::unknown_value_kind))]
    UnknownValueKind {
        /// The offending kind string.
        kind: String,
    },

    /// A coercion-class string in a theory spec is not one of the
    /// recognised classes.
    #[error(
        "unknown coercion class '{class}': expected one of iso, \
         retraction, projection, or opaque"
    )]
    #[diagnostic(code(panproto_theory_dsl::unknown_coercion_class))]
    UnknownCoercionClass {
        /// The offending class string.
        class: String,
    },

    /// An instance binding names a key that is neither a sort param nor
    /// an operation of the class theory.
    #[error(
        "instance '{instance}' has binding '{name}' that is not a sort param \
         or operation of class '{class}'"
    )]
    #[diagnostic(code(panproto_theory_dsl::instance_binding))]
    InstanceBinding {
        /// The instance name.
        instance: String,
        /// The class theory name.
        class: String,
        /// The offending binding key.
        name: String,
    },

    /// Sample-based coercion law check detected a declared coercion
    /// class whose forward/inverse pair falsifies the declared law on
    /// the sample registry's samples. Raised by the default
    /// [`compile`](fn@crate::compile) path (which runs the check against the
    /// built-in sample registry) and by
    /// [`crate::compile_theory_with_law_check`];
    /// [`crate::compile_unchecked`] skips the check.
    #[error(
        "coercion law violation in theory '{theory}': {} violation(s) across {distinct_equations} equation(s)",
        violations.len(),
    )]
    #[diagnostic(code(panproto_theory_dsl::coercion_law_violation))]
    CoercionLawViolation {
        /// The theory whose directed equations failed the sample
        /// check.
        theory: String,
        /// One entry per sample-level violation, carrying the
        /// equation name and the structured
        /// [`panproto_lens::coercion_laws::CoercionLawViolation`]
        /// payload (preserved so downstream consumers can tree-shake
        /// by variant: `Backward` / `Forward` / `NonDeterministic` /
        /// `MissingInverse` / `ForwardEvalError` / `InverseEvalError`
        /// / `UnknownClass`).
        violations: Vec<CoercionLawViolationDetail>,
        /// Count of distinct equations that produced at least one
        /// violation. Cached at construction time so the
        /// [`std::fmt::Display`] impl does not rebuild a `HashSet`
        /// on every render.
        distinct_equations: usize,
    },
}

/// One entry in
/// [`TheoryDslError::CoercionLawViolation::violations`]. Pairs the
/// originating equation name with the structured violation the
/// sample-law checker produced.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoercionLawViolationDetail {
    /// The directed equation on which the violation fired.
    pub equation: String,
    /// The structured violation produced by
    /// [`panproto_lens::coercion_laws::check_directed_equation_with_registry`].
    pub violation: panproto_lens::coercion_laws::CoercionLawViolation,
}

impl std::fmt::Display for CoercionLawViolationDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.equation, self.violation)
    }
}

impl TheoryDslError {
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

    /// The refusal without its location.
    #[must_use]
    pub fn unlocated(&self) -> &Self {
        match self {
            Self::At { error, .. } | Self::Spanned { error, .. } => error.unlocated(),
            other => other,
        }
    }
}

impl From<panproto_dsl_eval::DslEvalError> for TheoryDslError {
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
