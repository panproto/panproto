//! A refused document names the place it is refused at: a JSON document by
//! the byte range of the refused value, any document by its path.

use panproto_theory_dsl::shape::ShapeProblem;
use panproto_theory_dsl::{SourceFormat, TheoryDslError, compile_source, read_source};

fn no_theories(_name: &str) -> Option<panproto_gat::Theory> {
    None
}

/// The text of `source` that a spanned refusal points at.
fn pointed_text(error: &TheoryDslError) -> Option<&str> {
    let TheoryDslError::Spanned { src, span, .. } = error else {
        return None;
    };
    src.get(span.offset()..span.offset() + span.len())
}

#[test]
fn an_undeclared_output_sort_points_at_the_output_sort() {
    let source = r#"{
        "id": "dev.panproto.test.error_spans",
        "description": "Bad theory for span test.",
        "theory": "ThBadSort",
        "sorts": [{ "name": "A" }],
        "ops": [
            { "name": "mkA", "output": "A" },
            { "name": "mkB", "output": "Ghost" }
        ]
    }"#;
    let Err(error) = compile_source(source, SourceFormat::Json, &no_theories) else {
        panic!("an undeclared sort must be refused");
    };
    assert_eq!(pointed_text(&error), Some("\"Ghost\""), "{error:?}");
    assert_eq!(
        error.path().map(ToString::to_string).as_deref(),
        Some("/ops/1/output")
    );
    assert!(matches!(
        error.unlocated(),
        TheoryDslError::TypeCheck {
            error: panproto_gat::GatError::IllTypedSignature { .. },
            ..
        }
    ));
}

#[test]
fn an_ill_typed_equation_points_at_the_equation() {
    let source = r#"{"id": "x", "description": "", "theory": "T",
        "sorts": [{"name": "A"}, {"name": "B"}],
        "ops": [{"name": "a", "output": "A"}, {"name": "b", "output": "B"}],
        "equations": [{"name": "wrong", "lhs": "a()", "rhs": "b()"}]}"#;
    let Err(error) = compile_source(source, SourceFormat::Json, &no_theories) else {
        panic!("an ill-typed equation must be refused");
    };
    assert_eq!(
        pointed_text(&error),
        Some(r#"{"name": "wrong", "lhs": "a()", "rhs": "b()"}"#)
    );
}

#[test]
fn an_unknown_member_points_at_its_value() {
    let source = r#"{"id": "x", "description": "", "theory": "T", "sortz": [1]}"#;
    let Err(error) = read_source(source, SourceFormat::Json) else {
        panic!("an unknown member must be refused");
    };
    assert_eq!(pointed_text(&error), Some("[1]"));
    assert!(matches!(
        error.unlocated(),
        TheoryDslError::Shape {
            problem: ShapeProblem::UnknownField,
            ..
        }
    ));
}

#[test]
fn a_json_syntax_error_points_at_the_byte_the_reader_stopped_at() {
    let source = "{\"id\": \"x\",, }";
    let Err(error) = read_source(source, SourceFormat::Json) else {
        panic!("malformed JSON must be refused");
    };
    assert_eq!(pointed_text(&error), Some(","));
    assert!(matches!(error.unlocated(), TheoryDslError::Json(_)));
}

#[test]
fn a_yaml_refusal_names_its_path_without_a_span() {
    let source = "id: x\ndescription: ''\ntheory: T\nops:\n  - name: f\n    output: Ghost\n";
    let Err(error) = compile_source(source, SourceFormat::Yaml, &no_theories) else {
        panic!("an undeclared sort must be refused");
    };
    assert!(!matches!(error, TheoryDslError::Spanned { .. }));
    assert_eq!(
        error.path().map(ToString::to_string).as_deref(),
        Some("/ops/0/output")
    );
}

#[test]
fn a_nickel_contract_refusal_points_into_the_source_and_a_compile_refusal_names_its_path() {
    let contract_breach = "let { Theory, .. } = import \"panproto/theory.ncl\" in\n\
        { id = \"x\", description = \"\", theory = 3 } | Theory";
    let Err(error) = read_source(contract_breach, SourceFormat::Nickel) else {
        panic!("a contract breach must be refused");
    };
    let TheoryDslError::NickelEvalSpanned { src, span, .. } = &error else {
        panic!("expected a spanned Nickel refusal, got {error:?}");
    };
    assert!(span.offset() + span.len() <= src.len());

    let ill_typed = "{ id = \"x\", description = \"\", theory = \"T\",\n\
        ops = [{ name = \"f\", output = \"Ghost\" }] }";
    let Err(error) = compile_source(ill_typed, SourceFormat::Nickel, &no_theories) else {
        panic!("an undeclared sort must be refused");
    };
    assert!(!matches!(error, TheoryDslError::Spanned { .. }));
    assert_eq!(
        error.path().map(ToString::to_string).as_deref(),
        Some("/ops/0/output")
    );
}
