//! Error types for the theory DSL.

use miette::Diagnostic;
use thiserror::Error;

/// Errors arising from loading, evaluating, or compiling theory documents.
#[derive(Debug, Error, Diagnostic)]
pub enum TheoryDslError {
    /// Nickel evaluation failed.
    #[error("nickel evaluation failed: {message}")]
    #[diagnostic(code(panproto_theory_dsl::nickel_eval))]
    NickelEval {
        /// Human-readable error message from the Nickel evaluator.
        message: String,
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

    /// The theory document has no body variant.
    #[error(
        "theory document '{id}' has no body: expected one of theory, morphism, compose, protocol, or bundle"
    )]
    #[diagnostic(code(panproto_theory_dsl::no_body))]
    NoBody {
        /// The document ID.
        id: String,
    },

    /// The theory document has multiple body variants.
    #[error("theory document '{id}' has multiple bodies: {variants}")]
    #[diagnostic(code(panproto_theory_dsl::multiple_bodies))]
    MultipleBodies {
        /// The document ID.
        id: String,
        /// Comma-separated list of present variants.
        variants: String,
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

    /// Type-checking a theory failed.
    #[error("type error in theory '{theory}': {message}")]
    #[diagnostic(code(panproto_theory_dsl::typecheck))]
    TypeCheck {
        /// The theory that failed typechecking.
        theory: String,
        /// Human-readable type error.
        message: String,
    },

    /// Type-checking a theory failed, with a source-pointed diagnostic.
    ///
    /// Populated when the DSL compiler has the original source text on
    /// hand and can locate a span corresponding to the failing element
    /// (the theory name, an op name, etc.). Nickel-loaded theories
    /// currently fall back to the un-spanned [`Self::TypeCheck`] variant
    /// because spans are lost through Nickel evaluation.
    #[error("type error in theory '{theory}': {message}")]
    #[diagnostic(code(panproto_theory_dsl::typecheck_spanned))]
    TypeCheckSpanned {
        /// The theory that failed typechecking.
        theory: String,
        /// Human-readable type error.
        message: String,
        /// Source text, used by miette for rendering.
        #[source_code]
        src: String,
        /// Span into `src` pointing at the failing element.
        #[label("in this theory")]
        span: miette::SourceSpan,
    },

    /// Morphism validation failed.
    #[error("morphism check failed for '{morphism}': {message}")]
    #[diagnostic(code(panproto_theory_dsl::morphism_check))]
    MorphismCheck {
        /// The morphism that failed validation.
        morphism: String,
        /// Human-readable error.
        message: String,
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

    /// Dependency cycle detected.
    #[error("dependency cycle: {cycle}")]
    #[diagnostic(code(panproto_theory_dsl::cycle))]
    DependencyCycle {
        /// Human-readable cycle description.
        cycle: String,
    },

    /// IO error reading a theory file.
    #[error("IO error: {0}")]
    #[diagnostic(code(panproto_theory_dsl::io))]
    Io(#[from] std::io::Error),

    /// GAT engine error.
    #[error("GAT engine error: {0}")]
    #[diagnostic(code(panproto_theory_dsl::gat))]
    Gat(#[from] panproto_gat::GatError),

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
}
