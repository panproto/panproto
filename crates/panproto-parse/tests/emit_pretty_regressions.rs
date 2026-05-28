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

/// Issue #160: `emit_pretty(parse(emit_pretty(s))) == emit_pretty(s)`
/// must hold for Python function bodies. Quivers's NumPyro / Pyro /
/// PyMC / Edward2 backends all emit `def model(...): ...` shapes and
/// the fixed-point law is the cleanest correctness witness for the
/// schema → bytes pipeline.
#[test]
fn python_function_body_emit_is_fixed_point() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"def f():\n    x = 1\n    return x\n";
        let sch1 = reg
            .parse_with_protocol("python", src, "x.py")
            .expect("parse");
        let emit1 = reg
            .emit_pretty_with_protocol("python", &sch1)
            .expect("emit1");
        let sch2 = reg
            .parse_with_protocol("python", &emit1, "x.py")
            .expect("reparse");
        let emit2 = reg
            .emit_pretty_with_protocol("python", &sch2)
            .expect("emit2");
        let emit1_s = String::from_utf8_lossy(&emit1);
        let emit2_s = String::from_utf8_lossy(&emit2);
        assert_eq!(
            emit1, emit2,
            "Python emit must be a fixed point.\nemit1: {emit1_s}\nemit2: {emit2_s}",
        );
        // The return statement must remain inside the function body —
        // structurally, the re-parsed schema must still have a single
        // function_definition vertex containing two statements.
        let return_count = sch2
            .vertices
            .values()
            .filter(|v| v.kind.as_ref() == "return_statement")
            .count();
        assert_eq!(
            return_count, 1,
            "re-parsed schema must contain exactly one return_statement; got {return_count}.\nemitted:\n{emit1_s}",
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

/// Issue #160 sibling: quivers transpiles QVR → Stan, Julia (Gen, Turing),
/// JavaScript (WebPPL), BUGS, JAGS, and Scheme (Church). The fixed-point
/// law `emit(parse(emit(s))) == emit(s)` must hold for every backend so
/// downstream re-parsing pipelines remain stable.
#[test]
#[cfg(feature = "lang-stan")]
fn stan_emit_is_fixed_point() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"data {\n  int N;\n  vector[N] y;\n}\nmodel { y ~ normal(0, 1); }\n";
        let sch1 = reg.parse_with_protocol("stan", src, "m.stan").unwrap();
        let emit1 = reg.emit_pretty_with_protocol("stan", &sch1).unwrap();
        let sch2 = reg
            .parse_with_protocol("stan", &emit1, "m.stan")
            .unwrap();
        let emit2 = reg.emit_pretty_with_protocol("stan", &sch2).unwrap();
        assert_eq!(
            emit1,
            emit2,
            "Stan emit must be a fixed point.\nemit1: {}\nemit2: {}",
            String::from_utf8_lossy(&emit1),
            String::from_utf8_lossy(&emit2)
        );
    });
}

#[test]
#[cfg(feature = "lang-bugs")]
fn bugs_emit_is_fixed_point() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"model {\n  for (i in 1:N) {\n    y[i] ~ dnorm(mu, tau)\n  }\n}\n";
        let sch1 = reg.parse_with_protocol("bugs", src, "m.bug").unwrap();
        let emit1 = reg.emit_pretty_with_protocol("bugs", &sch1).unwrap();
        let sch2 = reg
            .parse_with_protocol("bugs", &emit1, "m.bug")
            .unwrap();
        let emit2 = reg.emit_pretty_with_protocol("bugs", &sch2).unwrap();
        assert_eq!(
            emit1,
            emit2,
            "BUGS emit must be a fixed point.\nemit1: {}\nemit2: {}",
            String::from_utf8_lossy(&emit1),
            String::from_utf8_lossy(&emit2)
        );
    });
}

#[test]
#[cfg(feature = "lang-jags")]
fn jags_emit_is_fixed_point() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"model {\n  for (i in 1:N) {\n    y[i] ~ dnorm(mu, tau)\n  }\n  mu ~ dnorm(0, 0.001)\n  tau ~ dgamma(0.001, 0.001)\n}\n";
        let sch1 = reg.parse_with_protocol("jags", src, "m.jag").unwrap();
        let emit1 = reg.emit_pretty_with_protocol("jags", &sch1).unwrap();
        let sch2 = reg
            .parse_with_protocol("jags", &emit1, "m.jag")
            .unwrap();
        let emit2 = reg.emit_pretty_with_protocol("jags", &sch2).unwrap();
        assert_eq!(
            emit1,
            emit2,
            "JAGS emit must be a fixed point.\nemit1: {}\nemit2: {}",
            String::from_utf8_lossy(&emit1),
            String::from_utf8_lossy(&emit2)
        );
    });
}

#[test]
#[cfg(feature = "lang-julia")]
fn julia_emit_is_fixed_point() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"function f(x)\n    y = x + 1\n    return y\nend\n";
        let sch1 = reg.parse_with_protocol("julia", src, "m.jl").unwrap();
        let emit1 = reg.emit_pretty_with_protocol("julia", &sch1).unwrap();
        let sch2 = reg
            .parse_with_protocol("julia", &emit1, "m.jl")
            .unwrap();
        let emit2 = reg.emit_pretty_with_protocol("julia", &sch2).unwrap();
        assert_eq!(
            emit1,
            emit2,
            "Julia emit must be a fixed point.\nemit1: {}\nemit2: {}",
            String::from_utf8_lossy(&emit1),
            String::from_utf8_lossy(&emit2)
        );
    });
}

#[test]
#[cfg(feature = "lang-scheme")]
fn scheme_emit_is_fixed_point() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"(define (f x) (+ x 1))\n";
        let sch1 = reg.parse_with_protocol("scheme", src, "m.scm").unwrap();
        let emit1 = reg.emit_pretty_with_protocol("scheme", &sch1).unwrap();
        let sch2 = reg
            .parse_with_protocol("scheme", &emit1, "m.scm")
            .unwrap();
        let emit2 = reg.emit_pretty_with_protocol("scheme", &sch2).unwrap();
        assert_eq!(
            emit1,
            emit2,
            "Scheme emit must be a fixed point.\nemit1: {}\nemit2: {}",
            String::from_utf8_lossy(&emit1),
            String::from_utf8_lossy(&emit2)
        );
    });
}

#[test]
fn javascript_emit_is_fixed_point() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"function f(x) { return x + 1; }\n";
        let sch1 = reg
            .parse_with_protocol("javascript", src, "m.js")
            .unwrap();
        let emit1 = reg
            .emit_pretty_with_protocol("javascript", &sch1)
            .unwrap();
        let sch2 = reg
            .parse_with_protocol("javascript", &emit1, "m.js")
            .unwrap();
        let emit2 = reg
            .emit_pretty_with_protocol("javascript", &sch2)
            .unwrap();
        assert_eq!(
            emit1,
            emit2,
            "JavaScript emit must be a fixed point.\nemit1: {}\nemit2: {}",
            String::from_utf8_lossy(&emit1),
            String::from_utf8_lossy(&emit2)
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
