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
}
