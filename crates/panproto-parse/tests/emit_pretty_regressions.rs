//! Regression tests for `emit_pretty` across JavaScript, Python,
//! Julia, Stan, and Scheme grammars.

#![cfg(feature = "grammars")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;
use panproto_schema::Schema;

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn strip_byte_fragments(schema: &mut Schema) {
    for constraints in schema.constraints.values_mut() {
        constraints.retain(|c| {
            let s = c.sort.as_ref();
            !(s == "start-byte" || s == "end-byte" || s.starts_with("interstitial-"))
        });
    }
}

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

fn emit_stripped(reg: &ParserRegistry, protocol: &str, src: &[u8]) -> String {
    let mut schema = reg
        .parse_with_protocol(protocol, src, &format!("test.{protocol}"))
        .unwrap_or_else(|e| panic!("parse failed: {e}"));
    strip_byte_fragments(&mut schema);
    let emitted = reg
        .emit_pretty_with_protocol(protocol, &schema)
        .unwrap_or_else(|e| panic!("emit_pretty failed: {e}"));
    String::from_utf8(emitted).expect("non-utf8 emit")
}

#[test]
fn js_object_literal_contents_inside_braces() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "javascript", b"var x = {a: 1, b: 2};\n");
        assert!(
            !text.contains("}\na") && !text.contains("}\n  a"),
            "object pair children must be inside braces, got: {text}"
        );
    });
}

#[test]
#[ignore = "Python _simple_statements uses ';' as grammar-valid separator; eliminating requires per-rule format policy"]
fn python_function_body_no_semicolons() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "python", b"def f():\n    x = 1\n    return x\n");
        assert!(
            !text.contains(';'),
            "Python function body should not use ';', got: {text}"
        );
    });
}

#[test]
fn python_fstring_interpolation_inline() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "python", b"s = f\"x={x}\"\n");
        assert!(
            !text.contains("{\n"),
            "f-string interpolation must be inline, got: {text}"
        );
    });
}

#[test]
fn js_ternary_preserves_question_mark() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "javascript", b"var x = a ? b : c;\n");
        assert!(text.contains('?'), "ternary must preserve '?', got: {text}");
    });
}

#[test]
fn js_template_literal_interpolation_inline() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "javascript", b"var s = `hello ${name}`;\n");
        assert!(
            !text.contains("${\n") && !text.contains("${ "),
            "template interpolation must be inline, got: {text}"
        );
    });
}

#[test]
#[cfg(feature = "lang-julia")]
fn julia_function_body_not_inline() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(
            &reg,
            "julia",
            b"function f()\n    x = 1\n    y = 2\n    x + y\nend\n",
        );
        let reparsed = reg
            .parse_with_protocol("julia", text.as_bytes(), "rt.jl")
            .expect("reparse");
        let errors = reparsed
            .vertices
            .values()
            .filter(|v| v.kind.as_ref() == "ERROR")
            .count();
        assert_eq!(
            errors, 0,
            "re-parsed Julia should have 0 ERROR nodes, got {errors}\nemitted:\n{text}"
        );
    });
}

#[test]
#[cfg(feature = "lang-stan")]
fn stan_array_size_inside_declaration() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "stan", b"data { real x[10]; }\n");
        let bracket_pos = text.find('[');
        let semi_pos = text.find(';');
        if let (Some(bp), Some(sp)) = (bracket_pos, semi_pos) {
            assert!(bp < sp, "array size '[10]' must be before ';', got: {text}");
        }
    });
}

#[test]
#[cfg(feature = "lang-stan")]
fn stan_function_params_inside_parens() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(
            &reg,
            "stan",
            b"functions { real sq(real x) { return x * x; } } model {}\n",
        );
        assert!(
            !text.contains("sq() real x"),
            "function params must be inside parens, got: {text}"
        );
    });
}

#[test]
#[cfg(feature = "lang-julia")]
fn julia_macrocall_multi_arg_preserves_all() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "julia", b"@info \"msg\" foo bar\n");
        assert!(
            text.contains("foo") && text.contains("bar"),
            "multi-arg macrocall must preserve all args, got: {text}"
        );
    });
}

#[test]
fn python_assignment_no_stray_colon() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "python", b"x = 1\n");
        assert!(
            !text.contains(": =") && !text.contains(":="),
            "assignment must not have stray ':', got: {text}"
        );
        assert!(
            text.contains("x = 1") || text.contains("x= 1") || text.contains("x =1"),
            "assignment must contain 'x = 1', got: {text}"
        );
    });
}

#[test]
#[cfg(feature = "lang-stan")]
fn stan_vector_size_inside_brackets() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(
            &reg,
            "stan",
            b"data {\n  int N;\n  vector[N] y;\n}\nmodel { y ~ normal(0, 1); }\n",
        );
        assert!(
            !text.contains("vector N"),
            "vector size must be inside brackets, got: {text}"
        );
    });
}

#[test]
#[cfg(feature = "lang-scheme")]
fn scheme_emit_nonempty() {
    with_big_stack(|| {
        let reg = registry();
        let text = emit_stripped(&reg, "scheme", b"(define (f x) (+ x 1))\n");
        assert!(
            !text.is_empty(),
            "Scheme emit_pretty must produce non-empty output"
        );
        assert!(
            text.contains("define"),
            "Scheme output must contain 'define', got: {text}"
        );
    });
}
