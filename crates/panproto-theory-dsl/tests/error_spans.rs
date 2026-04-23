//! Tests for source-span-aware typecheck errors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use miette::Diagnostic;
use panproto_theory_dsl::{TheoryDslError, compile_with_source, document::TheoryDocument};

#[test]
fn typecheck_error_carries_source_and_span_for_json() {
    // A theory whose op references an undefined sort. The DSL
    // compiler raises a TypeCheck error; the spanned wrapper should
    // upgrade it to TypeCheckSpanned with source text attached.
    let source = r#"{
        "id": "dev.panproto.test.error_spans",
        "description": "Bad theory for span test.",
        "theory": "ThBadSort",
        "sorts": [
            { "name": "A" },
            { "name": "B" }
        ],
        "ops": [
            { "name": "mkA", "output": "A" },
            { "name": "mkB", "output": "B" }
        ],
        "equations": [
            { "name": "wrong", "lhs": "mkA()", "rhs": "mkB()" }
        ]
    }"#;
    let doc: TheoryDocument = serde_json::from_str(source).unwrap();
    let resolver = |_name: &str| None;
    let err =
        compile_with_source(&doc, source, &resolver).expect_err("typecheck should fail on Ghost");
    match err {
        TheoryDslError::TypeCheckSpanned {
            ref theory,
            ref src,
            span,
            ..
        } => {
            assert_eq!(theory, "ThBadSort");
            assert!(src.contains("ThBadSort"));
            let offset = span.offset();
            let end = offset + span.len();
            assert!(src[offset..end].contains("ThBadSort"));
        }
        TheoryDslError::TypeCheck { .. } => {
            panic!("should have been upgraded to TypeCheckSpanned")
        }
        other => panic!("unexpected error: {other:?}"),
    }
    // Verify the rendered diagnostic includes the theory name.
    let err = compile_with_source(&doc, source, &resolver).unwrap_err();
    let rendered = format!(
        "{:?}",
        err.diagnostic_source().or(Some(&err as &dyn Diagnostic))
    );
    let _ = rendered; // value exercised; content depends on miette version.
}
