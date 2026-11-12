//! A refused document names the place it is refused at: a JSON document by
//! the byte range of the refused value, any document by its path.

use panproto_theory_dsl::shape::ShapeProblem;
use panproto_theory_dsl::{SourceFormat, TheoryDslError, compile_source, read_source};

const fn no_theories(_name: &str) -> Option<panproto_gat::Theory> {
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
fn a_yaml_refusal_points_at_the_first_character_of_the_refused_value() {
    let source = "id: x\ndescription: ''\ntheory: T\nops:\n  - name: f\n    output: Ghost\n";
    let Err(error) = compile_source(source, SourceFormat::Yaml, &no_theories) else {
        panic!("an undeclared sort must be refused");
    };
    assert_eq!(
        error.path().map(ToString::to_string).as_deref(),
        Some("/ops/0/output")
    );
    assert_eq!(pointed_text(&error), Some("G"), "{error:?}");
}

#[test]
fn a_yaml_grammar_refusal_points_at_the_refused_node_and_the_last_repeated_member() {
    // A block mapping starts at its first key; a repeated member is read,
    // and located, at its last occurrence.
    let source = "id: x\ndescription: ''\ntheory: T\nsorts:\n  - name: S\n    kind: {type: nope}\n    kind:\n      type: alsonope\n";
    let Err(error) = read_source(source, SourceFormat::Yaml) else {
        panic!("an unknown sort kind must be refused");
    };
    assert_eq!(
        error.path().map(ToString::to_string).as_deref(),
        Some("/sorts/0/kind/type")
    );
    let TheoryDslError::Spanned { span, .. } = &error else {
        panic!("a YAML refusal carries a span: {error:?}");
    };
    assert_eq!(span.offset(), source.find("alsonope").unwrap_or_default());
    assert_eq!(span.len(), 1);
}

#[test]
fn a_yaml_syntax_error_points_at_the_character_the_reader_stopped_at() {
    let source = "id: x\ndescription: [unclosed\n";
    let Err(error) = read_source(source, SourceFormat::Yaml) else {
        panic!("malformed YAML must be refused");
    };
    assert!(matches!(error.unlocated(), TheoryDslError::Yaml { .. }));
    let TheoryDslError::Spanned { span, .. } = &error else {
        panic!("a YAML syntax error carries a span: {error:?}");
    };
    assert!(span.offset() <= source.len());
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

/// The path and the pointed text of the refusal of a JSON document compiled
/// against the built-in theories.
fn refusal_place(source: &str) -> (String, String) {
    let resolver = panproto_theory_dsl::builtin_resolver();
    let Err(error) = compile_source(source, SourceFormat::Json, &resolver) else {
        panic!("{source} must be refused");
    };
    (
        error.path().map(ToString::to_string).unwrap_or_default(),
        pointed_text(&error).unwrap_or_default().to_owned(),
    )
}

#[test]
fn composition_refusals_point_at_the_base_side_or_step_they_concern() {
    assert_eq!(
        refusal_place(
            r#"{"id":"c","description":"","compose":{"result":"R","bases":["ThGraph","Nope"],"steps":[{"left":"ThGraph","right":"ThConstraint","shared_sorts":["Vertex"]}]}}"#
        ),
        ("/compose/bases/1".to_owned(), "\"Nope\"".to_owned())
    );
    assert_eq!(
        refusal_place(
            r#"{"id":"c","description":"","compose":{"result":"R","steps":[{"left":"step_1","right":"ThConstraint","shared_sorts":["Vertex"]}]}}"#
        ),
        ("/compose/steps/0/left".to_owned(), "\"step_1\"".to_owned())
    );
    assert_eq!(
        refusal_place(r#"{"id":"c","description":"","compose":{"result":"R","steps":[]}}"#),
        ("/compose/steps".to_owned(), "[]".to_owned())
    );
    let (path, text) = refusal_place(
        r#"{"id":"c","description":"","compose":{"result":"R","steps":[{"left":"ThGraph","right":"ThConstraint","shared_sorts":["Zed"]}]}}"#,
    );
    assert_eq!(path, "/compose/steps/0");
    assert!(text.starts_with("{\"left\""), "{text}");
}

#[test]
fn protocol_refusals_point_into_the_theory_reference_they_concern() {
    assert_eq!(
        refusal_place(
            r#"{"id":"p","description":"","protocol":"P","schema_theory":"ThGraph","instance_theory":"Nope"}"#
        ),
        ("/instance_theory".to_owned(), "\"Nope\"".to_owned())
    );
    assert_eq!(
        refusal_place(
            r#"{"id":"p","description":"","protocol":"P","schema_theory":{"theory":"S","ops":[{"name":"f","output":"Q"}]},"instance_theory":"ThGraph"}"#
        ),
        ("/schema_theory/ops/0/output".to_owned(), "\"Q\"".to_owned())
    );
    assert_eq!(
        refusal_place(
            r#"{"id":"p","description":"","protocol":"P","schema_theory":{"result":"R","steps":[{"left":"ThGraph","right":"Nope","shared_sorts":["Vertex"]}]},"instance_theory":"ThGraph"}"#
        )
        .0,
        "/schema_theory/steps/0/right"
    );
}

#[test]
fn a_protocol_records_its_inline_theories_and_composition_recipes() {
    let resolver = panproto_theory_dsl::builtin_resolver();
    let Ok(compiled) = compile_source(
        r#"{"id":"p","description":"","protocol":"P","schema_theory":{"theory":"S","extends":["ThGraph"]},"instance_theory":{"result":"R","steps":[{"left":"ThGraph","right":"ThConstraint","shared_sorts":["Vertex"]}]}}"#,
        SourceFormat::Json,
        &resolver,
    ) else {
        panic!("an inline theory may extend a resolved theory");
    };
    let protocol = &compiled.protocols["P"];
    assert_eq!(protocol.schema_theory, "S");
    assert_eq!(protocol.instance_theory, "R");
    assert!(protocol.schema_composition.is_none());
    assert_eq!(
        protocol.instance_composition.as_ref(),
        compiled.composition_specs.get("R")
    );
    assert_eq!(compiled.theories["S"].sorts.len(), 2);
    assert!(compiled.theories.contains_key("R"));
}
