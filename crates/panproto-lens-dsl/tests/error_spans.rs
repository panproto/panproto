//! A refused lens document names the place it is refused at: a JSON
//! document by the byte range of the refused value, any document by its
//! path, and every refusal by a typed class.

use std::collections::HashMap;

use panproto_lens_dsl::shape::{Expected, ShapeProblem};
use panproto_lens_dsl::{
    CompiledLens, LensDocument, LensDslError, SourceFormat, compile, compile_source,
    compile_with_refs, eval, read_source,
};

const fn no_lenses(_id: &str) -> Option<CompiledLens> {
    None
}

/// A document with the standard header and `body` spliced in.
fn doc(body: &str) -> String {
    format!(r#"{{"id": "x", "source": "s", "target": "t", {body}}}"#)
}

/// Compile JSON text and return its refusal.
fn refusal(source: &str) -> LensDslError {
    match compile_source(source, SourceFormat::Json, "b", &no_lenses) {
        Ok(compiled) => panic!("{source} must be refused, compiled {:?}", compiled.id),
        Err(error) => error,
    }
}

/// The path a refusal names, as a JSON Pointer.
fn path(error: &LensDslError) -> Option<String> {
    error.path().map(ToString::to_string)
}

/// The text of the source that a spanned refusal points at.
fn pointed_text(error: &LensDslError) -> Option<&str> {
    let LensDslError::Spanned { src, span, .. } = error else {
        return None;
    };
    src.get(span.offset()..span.offset() + span.len())
}

fn assert_shape(source: &str, at: &str, text: &str, problem: &ShapeProblem) {
    let error = refusal(source);
    assert_eq!(path(&error).as_deref(), Some(at), "{error:?}");
    assert_eq!(pointed_text(&error), Some(text), "{error:?}");
    assert!(
        matches!(error.unlocated(), LensDslError::Shape { problem: found, .. } if found == problem),
        "{error:?}"
    );
}

#[test]
fn an_unknown_document_member_is_refused_at_its_value() {
    assert_shape(
        &doc(r#""steps": [], "bogus": 1"#),
        "/bogus",
        "1",
        &ShapeProblem::UnknownField,
    );
}

#[test]
fn a_step_member_that_names_no_step_is_refused() {
    assert_shape(
        &doc(r#""steps": [{"remove_field": "a", "extra": 1}]"#),
        "/steps/0/extra",
        "1",
        &ShapeProblem::UnknownVariant {
            found: "extra".to_owned(),
        },
    );
    assert_shape(
        &doc(r#""steps": [{"remove_field": "a"}, {"frobnicate": 1}]"#),
        "/steps/1/frobnicate",
        "1",
        &ShapeProblem::UnknownVariant {
            found: "frobnicate".to_owned(),
        },
    );
}

#[test]
fn a_step_naming_two_variants_is_refused_whatever_their_order() {
    let several = ShapeProblem::SeveralVariants {
        keys: vec!["remove_field".to_owned(), "rename_field".to_owned()],
    };
    let first = r#"{"remove_field": "a", "rename_field": {"old": "x", "new": "y"}}"#;
    let second = r#"{"rename_field": {"old": "x", "new": "y"}, "remove_field": "a"}"#;
    for step in [first, second] {
        assert_shape(
            &doc(&format!(r#""steps": [{step}]"#)),
            "/steps/0",
            step,
            &several,
        );
    }
}

#[test]
fn a_second_spelling_of_a_member_is_unknown() {
    assert_shape(
        &doc(r#""steps": [{"add_field": {"name": "n", "kind": "integer", "fallback": 2}}]"#),
        "/steps/0/add_field/fallback",
        "2",
        &ShapeProblem::UnknownField,
    );
    assert_shape(
        &doc(r#""rules": [{"match": {"name": "a"}}]"#),
        "/rules/0/match",
        r#"{"name": "a"}"#,
        &ShapeProblem::UnknownField,
    );
    assert_shape(
        &doc(
            r#""steps": [], "directed_equations": [{"name": "e", "lhs": "a", "rhs": "b", "impl_term": "a"}]"#,
        ),
        "/directed_equations/0/impl_term",
        "\"a\"",
        &ShapeProblem::UnknownField,
    );
    assert_shape(
        &doc(r#""rules": [{"pattern": {"type_id": "t"}, "replace": {"name": "y"}}]"#),
        "/rules/0/pattern/type_id",
        "\"t\"",
        &ShapeProblem::UnknownField,
    );
}

#[test]
fn a_value_kind_outside_the_vocabulary_is_refused() {
    assert_shape(
        &doc(
            r#""steps": [{"coerce_sort": {"sort": "s", "target_kind": "boolen", "expr": "v", "inverse": "v", "coercion": "iso"}}]"#,
        ),
        "/steps/0/coerce_sort/target_kind",
        "\"boolen\"",
        &ShapeProblem::UnknownVariant {
            found: "boolen".to_owned(),
        },
    );
}

#[test]
fn a_member_belongs_to_the_bodies_that_use_it() {
    assert_shape(
        &doc(r#""steps": [], "passthrough": "drop""#),
        "/passthrough",
        "\"drop\"",
        &ShapeProblem::UnknownField,
    );
    assert_shape(
        &doc(r#""symmetric": {"left": [], "right": []}, "directed_equations": []"#),
        "/directed_equations",
        "[]",
        &ShapeProblem::UnknownField,
    );
}

#[test]
fn an_expression_must_be_written_as_text() {
    assert_shape(
        &doc(r#""steps": [{"apply_expr": {"field": "x", "expr": {"Var": "y"}}}]"#),
        "/steps/0/apply_expr/expr",
        r#"{"Var": "y"}"#,
        &ShapeProblem::WrongType {
            expected: Expected::String,
        },
    );
    assert_shape(
        &doc(r#""steps": [{"remove_field": 5}]"#),
        "/steps/0/remove_field",
        "5",
        &ShapeProblem::WrongType {
            expected: Expected::String,
        },
    );
}

#[test]
fn a_lens_reference_names_exactly_one_form() {
    assert_shape(
        &doc(
            r#""compose": {"mode": "vertical", "lenses": [{"ref": "q", "inline": {"steps": []}}]}"#,
        ),
        "/compose/lenses/0",
        r#"{"ref": "q", "inline": {"steps": []}}"#,
        &ShapeProblem::SeveralVariants {
            keys: vec!["ref".to_owned(), "inline".to_owned()],
        },
    );
    assert_shape(
        &doc(
            r#""compose": {"mode": "vertical", "lenses": [{"inline": {"steps": [], "rules": []}}]}"#,
        ),
        "/compose/lenses/0/inline/rules",
        "[]",
        &ShapeProblem::UnknownField,
    );
}

#[test]
fn a_document_without_a_body_is_refused_at_the_root() {
    let source = r#"{"id": "x", "source": "s", "target": "t"}"#;
    assert_shape(source, "", source, &ShapeProblem::NoBody);
}

#[test]
fn a_dishonest_directed_equation_points_at_the_equation() {
    let equation = r#"{"name": "e", "lhs": "x", "rhs": "y", "impl": "upper x", "source_kind": "string", "coercion": "iso"}"#;
    let error = refusal(&doc(&format!(
        r#""steps": [], "directed_equations": [{equation}]"#
    )));
    assert_eq!(path(&error).as_deref(), Some("/directed_equations/0"));
    assert_eq!(pointed_text(&error), Some(equation));
    assert!(matches!(
        error.unlocated(),
        LensDslError::CoercionNotHonest { .. }
    ));
}

#[test]
fn a_malformed_template_points_at_its_own_rule() {
    let error = refusal(&doc(
        r#""rules": [{"pattern": {"name": "a"}, "replace": {"name": "a"}}, {"pattern": {"name": "b"}}, {"pattern": {"name": "c"}, "replace": {"name": {"template": "h{lvl"}}}]"#,
    ));
    assert_eq!(path(&error).as_deref(), Some("/rules/2/replace/name"));
    assert_eq!(pointed_text(&error), Some(r#"{"template": "h{lvl"}"#));
    assert!(matches!(
        error.unlocated(),
        LensDslError::RuleCompile { .. }
    ));
}

#[test]
fn an_inner_scoped_refusal_points_at_the_inner_step() {
    let error = refusal(&doc(
        r#""steps": [{"remove_field": "a"}, {"scoped": {"focus": "f", "inner": [{"apply_expr": {"field": "x", "expr": "(((("}}]}}]"#,
    ));
    assert_eq!(
        path(&error).as_deref(),
        Some("/steps/1/scoped/inner/0/apply_expr/expr")
    );
    assert_eq!(pointed_text(&error), Some("\"((((\""));
    assert!(matches!(error.unlocated(), LensDslError::ExprParse { .. }));
}

#[test]
fn a_symmetric_leg_refuses_a_value_level_step() {
    let error = refusal(&doc(
        r#""symmetric": {"left": [{"remove_field": "l"}], "right": [{"apply_expr": {"field": "r", "expr": "r"}}]}"#,
    ));
    assert_eq!(path(&error).as_deref(), Some("/symmetric/right/0"));
    assert!(matches!(
        error.unlocated(),
        LensDslError::ValueStepInSymmetricLeg
    ));
}

#[test]
fn an_empty_horizontal_part_is_refused_at_the_part() {
    let error = refusal(&doc(
        r#""compose": {"mode": "horizontal", "lenses": [{"inline": {"steps": [{"remove_field": "a"}]}}, {"inline": {"steps": []}}]}"#,
    ));
    assert_eq!(path(&error).as_deref(), Some("/compose/lenses/1"));
    assert_eq!(pointed_text(&error), Some(r#"{"inline": {"steps": []}}"#));
    assert!(matches!(
        error.unlocated(),
        LensDslError::EmptyHorizontalPart
    ));
}

#[test]
fn a_json_syntax_error_points_at_the_byte_the_reader_stopped_at() {
    let source = r#"{"id": "x", "steps": [1 2]}"#;
    let Err(error) = read_source(source, SourceFormat::Json) else {
        panic!("malformed JSON must be refused");
    };
    assert_eq!(pointed_text(&error), Some("2"), "{error:?}");
    assert!(matches!(error.unlocated(), LensDslError::Json(_)));
}

#[test]
fn a_yaml_refusal_points_at_the_first_character_of_the_refused_value() {
    let source = "id: x\nsource: s\ntarget: t\nsteps:\n  - remove_field: a\n    bogus: 1\n";
    let Err(error) = read_source(source, SourceFormat::Yaml) else {
        panic!("an unknown step member must be refused");
    };
    assert!(
        matches!(error.unlocated(), LensDslError::Shape { path, problem: ShapeProblem::UnknownVariant { .. } }
            if path.to_string() == "/steps/0/bogus"),
        "{error:?}"
    );
    assert_eq!(pointed_text(&error), Some("1"), "{error:?}");
}

fn on_big_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    match std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(work)
        .map(std::thread::JoinHandle::join)
    {
        Ok(Ok(value)) => value,
        _ => panic!("the Nickel worker thread failed"),
    }
}

#[test]
fn a_nickel_document_is_checked_by_the_grammar_and_keeps_nickel_spans() {
    let refused = on_big_stack(|| {
        read_source(
            r#"{ id = "x", source = "s", target = "t", steps = [{ remove_field = "a", bogus = 1 }] }"#,
            SourceFormat::Nickel,
        )
        .map(|doc| doc.id)
    });
    assert!(
        matches!(&refused, Err(LensDslError::Shape { path, .. }) if path.to_string() == "/steps/0/bogus"),
        "{refused:?}"
    );

    let source = "let L = import \"panproto/lens.ncl\" in { id = \"x\", source = \"s\", target = \"t\", steps = [{ remove_field = \"a\", bogus = 1 }] } | L.Lens";
    let contract =
        on_big_stack(move || read_source(source, SourceFormat::Nickel).map(|doc| doc.id));
    let Err(LensDslError::NickelEvalSpanned { src, span, .. }) = &contract else {
        panic!("a contract violation must carry Nickel's span: {contract:?}");
    };
    assert!(
        src.get(span.offset()..span.offset() + span.len())
            .is_some_and(|text| text.contains("bogus")),
        "{contract:?}"
    );
}

/// Compile one JSON document with the null resolver.
fn compiled(source: &str) -> CompiledLens {
    match eval::eval_json(source).and_then(|document| compile(&document, "b", &no_lenses)) {
        Ok(compiled) => compiled,
        Err(error) => panic!("{source} must compile: {error:?}"),
    }
}

fn step_names(compiled: &CompiledLens) -> Vec<String> {
    compiled
        .chain
        .steps
        .iter()
        .map(|step| step.name.to_string())
        .collect()
}

#[test]
fn rule_attribute_maps_expand_in_key_order_on_every_compile() {
    let source = doc(
        r#""rules": [{"pattern": {"name": "r"}, "replace": {"rename_attrs": {"e": "f", "c": "d", "a": "b"}}}]"#,
    );
    let first = step_names(&compiled(&source));
    for _ in 0..32 {
        assert_eq!(step_names(&compiled(&source)), first);
    }
    assert_eq!(
        first,
        ["rename_edge_a_b", "rename_edge_c_d", "rename_edge_e_f"]
    );
}

#[test]
fn an_attribute_added_with_a_fractional_value_compiles() {
    let lens = compiled(&doc(
        r#""rules": [{"pattern": {"name": "a"}, "replace": {"add_attrs": {"x": 1.5, "y": [1], "z": {"k": true}}}}]"#,
    ));
    assert!(!lens.chain.steps.is_empty());
}

#[test]
fn vertical_composition_equals_compiling_the_concatenated_steps() {
    let first = r#"[{"remove_field": "a"}, {"apply_expr": {"field": "x", "expr": "x"}}]"#;
    let second = r#"[{"rename_field": {"old": "p", "new": "q"}}, {"add_field": {"name": "z", "kind": "integer", "default": 0}}]"#;
    let together = format!(
        "{}, {}",
        first.trim_end_matches(']'),
        second.trim_start_matches('[')
    );
    let concatenated = compiled(&doc(&format!(r#""steps": {together}"#)));
    let composed = compiled(&doc(&format!(
        r#""compose": {{"mode": "vertical", "lenses": [{{"inline": {{"steps": {first}}}}}, {{"inline": {{"steps": {second}}}}}]}}"#
    )));
    assert_eq!(
        serde_json::to_value(&concatenated.stages).ok(),
        serde_json::to_value(&composed.stages).ok()
    );
    assert_eq!(
        concatenated.chain.to_json().ok(),
        composed.chain.to_json().ok()
    );
}

fn library() -> HashMap<String, LensDocument> {
    let mut docs = HashMap::new();
    for source in [
        r#"{"id": "A", "source": "s", "target": "t", "steps": [{"remove_field": "a"}]}"#,
        r#"{"id": "AB", "source": "s", "target": "t", "compose": {"mode": "vertical", "lenses": [{"ref": "A"}, {"inline": {"steps": [{"remove_field": "b"}]}}]}}"#,
        r#"{"id": "BAD", "source": "s", "target": "t", "steps": [{"apply_expr": {"field": "x", "expr": "(((("}}]}"#,
        r#"{"id": "LOOP", "source": "s", "target": "t", "compose": {"mode": "vertical", "lenses": [{"ref": "LOOP2"}]}}"#,
        r#"{"id": "LOOP2", "source": "s", "target": "t", "compose": {"mode": "vertical", "lenses": [{"ref": "LOOP"}]}}"#,
    ] {
        match eval::eval_json(source) {
            Ok(document) => {
                docs.insert(document.id.clone(), document);
            }
            Err(error) => panic!("{source}: {error:?}"),
        }
    }
    docs
}

fn top(reference: &str) -> LensDocument {
    match eval::eval_json(&format!(
        r#"{{"id": "top", "source": "s", "target": "t", "compose": {{"mode": "vertical", "lenses": [{{"ref": "{reference}"}}]}}}}"#
    )) {
        Ok(document) => document,
        Err(error) => panic!("{error:?}"),
    }
}

#[test]
fn references_resolve_transitively() {
    let docs = library();
    let Ok(nested) = compile_with_refs(&top("AB"), "b", &docs) else {
        panic!("a reference to a composition must resolve");
    };
    assert_eq!(
        nested.chain.steps.len(),
        compiled(&doc(
            r#""steps": [{"remove_field": "a"}, {"remove_field": "b"}]"#
        ))
        .chain
        .steps
        .len()
    );
}

#[test]
fn a_referenced_refusal_is_reported_located_in_the_referenced_document() {
    let Err(error) = compile_with_refs(&top("BAD"), "b", &library()) else {
        panic!("a refused reference must refuse the composition");
    };
    assert_eq!(path(&error).as_deref(), Some("/compose/lenses/0/ref"));
    let LensDslError::InRef { lens_ref, error } = error.unlocated() else {
        panic!("expected InRef, got {error:?}");
    };
    assert_eq!(lens_ref, "BAD");
    assert_eq!(path(error).as_deref(), Some("/steps/0/apply_expr/expr"));
    assert!(matches!(error.unlocated(), LensDslError::ExprParse { .. }));
}

#[test]
fn a_reference_cycle_is_refused() {
    let Err(error) = compile_with_refs(&top("LOOP"), "b", &library()) else {
        panic!("a cycle must be refused");
    };
    let mut current = &error;
    let cycle = loop {
        match current.unlocated() {
            LensDslError::InRef { error, .. } => current = error,
            LensDslError::RefCycle { cycle } => break cycle.clone(),
            other => panic!("expected a cycle, got {other:?}"),
        }
    };
    assert_eq!(cycle, ["top", "LOOP", "LOOP2", "LOOP"]);
}

#[test]
fn a_symmetric_lens_whose_right_leg_drops_a_field_is_not_invertible() {
    let error = refusal(&doc(
        r#""symmetric": {"left": [], "right": [{"remove_field": "a"}]}, "invertible": true"#,
    ));
    assert_eq!(path(&error).as_deref(), Some("/invertible"));
    assert!(matches!(
        error.unlocated(),
        LensDslError::NotInvertible { .. }
    ));
}

#[test]
fn a_scoped_rename_and_a_horizontal_composite_of_renames_are_invertible() {
    compiled(&doc(
        r#""steps": [{"scoped": {"focus": "item", "inner": [{"rename_op": {"old": "f", "new": "g"}}]}}], "invertible": true"#,
    ));
    compiled(&doc(
        r#""compose": {"mode": "horizontal", "lenses": [{"inline": {"steps": [{"rename_sort": {"old": "s", "new": "t"}}]}}, {"inline": {"steps": [{"rename_op": {"old": "f", "new": "g"}}]}}]}, "invertible": true"#,
    ));
}

#[test]
fn a_horizontal_composite_composes_after_a_step_inside_a_horizontal_part() {
    let mut docs = library();
    for source in [
        r#"{"id": "H", "source": "s", "target": "t", "compose": {"mode": "horizontal", "lenses": [{"inline": {"steps": [{"rename_sort": {"old": "s", "new": "t"}}]}}, {"inline": {"steps": [{"drop_op": "f"}]}}]}}"#,
        r#"{"id": "AH", "source": "s", "target": "t", "compose": {"mode": "vertical", "lenses": [{"ref": "A"}, {"ref": "H"}]}}"#,
    ] {
        match eval::eval_json(source) {
            Ok(document) => {
                docs.insert(document.id.clone(), document);
            }
            Err(error) => panic!("{source}: {error:?}"),
        }
    }
    let Ok(top) = eval::eval_json(
        r#"{"id": "top", "source": "s", "target": "t", "compose": {"mode": "horizontal", "lenses": [{"ref": "AH"}]}}"#,
    ) else {
        panic!("the composing document reads");
    };
    let Ok(lens) = compile_with_refs(&top, "b", &docs) else {
        panic!("Id composed with Id is the identity, so the part fuses");
    };
    assert_eq!(lens.chain.steps.len(), 1);
}
