//! Properties of composition and of refusal location over generated
//! documents: vertical composition equals compiling the concatenated steps,
//! it is associative through named references, and an unknown member
//! anywhere in a step list is refused at that member's value.

use std::collections::HashMap;

use panproto_lens_dsl::{
    CompiledLens, LensDocument, LensDslError, SourceFormat, compile_source, compile_with_refs, eval,
};
use proptest::prelude::*;

/// One step, as JSON text, from a small alphabet over the fields `f0..f3`.
fn step() -> impl Strategy<Value = String> {
    let field = || (0..4_u8).prop_map(|i| format!("f{i}"));
    prop_oneof![
        field().prop_map(|f| format!(r#"{{"remove_field": "{f}"}}"#)),
        (field(), field())
            .prop_map(|(a, b)| format!(r#"{{"rename_field": {{"old": "{a}", "new": "{b}x"}}}}"#)),
        (field(), 0..3_i64).prop_map(|(f, d)| format!(
            r#"{{"add_field": {{"name": "{f}n", "kind": "integer", "default": {d}}}}}"#
        )),
        field().prop_map(|f| format!(r#"{{"apply_expr": {{"field": "{f}", "expr": "{f}"}}}}"#)),
        field()
            .prop_map(|f| format!(r#"{{"compute_field": {{"target": "{f}c", "expr": "{f}"}}}}"#)),
        (field(), field())
            .prop_map(|(a, b)| format!(r#"{{"rename_sort": {{"old": "{a}", "new": "{b}s"}}}}"#)),
        field().prop_map(|f| format!(r#"{{"drop_sort": "{f}"}}"#)),
    ]
}

fn steps(max: usize) -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(step(), 0..max)
}

fn document(id: &str, body: &str) -> String {
    format!(r#"{{"id": "{id}", "source": "s", "target": "t", {body}}}"#)
}

fn steps_body(steps: &[String]) -> String {
    format!(r#""steps": [{}]"#, steps.join(", "))
}

fn compose_body(parts: &[String]) -> String {
    format!(
        r#""compose": {{"mode": "vertical", "lenses": [{}]}}"#,
        parts.join(", ")
    )
}

fn inline(steps: &[String]) -> String {
    format!(r#"{{"inline": {{"steps": [{}]}}}}"#, steps.join(", "))
}

fn read(source: &str) -> Result<LensDocument, TestCaseError> {
    eval::eval_json(source).map_err(|e| TestCaseError::fail(format!("{source}: {e:?}")))
}

fn compiled_with(
    source: &str,
    library: &HashMap<String, LensDocument>,
) -> Result<CompiledLens, TestCaseError> {
    compile_with_refs(&read(source)?, "b", library)
        .map_err(|e| TestCaseError::fail(format!("{source}: {e:?}")))
}

/// The observable structure of a compiled lens: its stages and flat chain.
fn observed(lens: &CompiledLens) -> (Option<serde_json::Value>, Option<String>) {
    (
        serde_json::to_value(&lens.stages).ok(),
        lens.chain.to_json().ok(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn vertical_composition_compiles_the_concatenation(
        a in steps(6),
        b in steps(6),
    ) {
        let library = HashMap::new();
        let together: Vec<String> = a.iter().chain(&b).cloned().collect();
        let concatenated = compiled_with(&document("x", &steps_body(&together)), &library)?;
        let composed = compiled_with(
            &document("x", &compose_body(&[inline(&a), inline(&b)])),
            &library,
        )?;
        prop_assert_eq!(observed(&concatenated), observed(&composed));
    }

    #[test]
    fn vertical_composition_is_associative_through_references(
        a in steps(5),
        b in steps(5),
        c in steps(5),
    ) {
        let mut library = HashMap::new();
        for (id, body) in [
            ("A", steps_body(&a)),
            ("B", steps_body(&b)),
            ("C", steps_body(&c)),
            ("AB", compose_body(&[r#"{"ref": "A"}"#.to_owned(), r#"{"ref": "B"}"#.to_owned()])),
            ("BC", compose_body(&[r#"{"ref": "B"}"#.to_owned(), r#"{"ref": "C"}"#.to_owned()])),
        ] {
            library.insert(id.to_owned(), read(&document(id, &body))?);
        }
        let left = compiled_with(
            &document("L", &compose_body(&[r#"{"ref": "AB"}"#.to_owned(), r#"{"ref": "C"}"#.to_owned()])),
            &library,
        )?;
        let right = compiled_with(
            &document("R", &compose_body(&[r#"{"ref": "A"}"#.to_owned(), r#"{"ref": "BC"}"#.to_owned()])),
            &library,
        )?;
        let empty = compiled_with(&document("E", &compose_body(&[])), &library)?;
        let a_alone = compiled_with(&document("A1", &steps_body(&a)), &library)?;
        let a_with_unit = compiled_with(
            &document("AU", &compose_body(&[inline(&a), inline(&[])])),
            &library,
        )?;
        prop_assert_eq!(observed(&left), observed(&right));
        prop_assert!(empty.stages.is_empty() && empty.chain.steps.is_empty());
        prop_assert_eq!(observed(&a_alone), observed(&a_with_unit));
    }

    #[test]
    fn an_unknown_member_is_refused_at_its_value(
        all in prop::collection::vec(step(), 1..8),
        pick in any::<prop::sample::Index>(),
        payload in 0..1000_u32,
    ) {
        let k = pick.index(all.len());
        let mut with_unknown = all.clone();
        // Every step is `{"<variant>": <spec>}`; insert a member into the
        // step object itself, which then names a second, unknown variant.
        with_unknown[k] = format!(r#"{{"zz": {payload}, {}"#, &all[k][1..]);
        let source = document("x", &steps_body(&with_unknown));
        let refused = compile_source(&source, SourceFormat::Json, "b", &|_| None);
        let Err(error) = refused else {
            return Err(TestCaseError::fail(format!("{source} must be refused")));
        };
        prop_assert_eq!(
            error.path().map(ToString::to_string),
            Some(format!("/steps/{k}/zz"))
        );
        let LensDslError::Spanned { src, span, .. } = &error else {
            return Err(TestCaseError::fail(format!("unspanned: {error:?}")));
        };
        let payload_text = payload.to_string();
        prop_assert_eq!(
            src.get(span.offset()..span.offset() + span.len()),
            Some(payload_text.as_str())
        );
    }
}
