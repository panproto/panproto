//! Comprehensive `emit_pretty` regression tests across all grammars.

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

// =================================================================
// Julia
// =================================================================

#[cfg(feature = "lang-julia")]
mod julia {
    use super::*;

    #[test]
    fn comment_does_not_swallow_next_line() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "julia", b"# comment\nx = 1\n");
            assert!(
                text.contains("x = 1") || text.contains("x =1"),
                "comment must not swallow next line, got: {text}"
            );
        });
    }

    #[test]
    fn string_literal_preserves_closing_quote() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "julia", b"s = \"hello\"\n");
            let quote_count = text.matches('"').count();
            assert!(
                quote_count >= 2,
                "string must have opening and closing quotes, got: {text}"
            );
        });
    }

    #[test]
    fn anonymous_function_preserves_body() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "julia", b"f = (x) -> x + 1\n");
            assert!(
                text.contains("->") || text.contains("→"),
                "anonymous function must preserve arrow, got: {text}"
            );
            assert!(
                text.contains("x + 1"),
                "anonymous function must preserve body, got: {text}"
            );
        });
    }
}

// =================================================================
// JavaScript
// =================================================================

mod javascript {
    use super::*;

    #[test]
    fn object_literal_preserves_pairs() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "javascript", b"const o = {a: 1, b: 2};\n");
            assert!(
                text.contains("a:") || text.contains("a :"),
                "object must preserve pair 'a', got: {text}"
            );
            assert!(
                text.contains("b:") || text.contains("b :"),
                "object must preserve pair 'b', got: {text}"
            );
        });
    }

    #[test]
    fn arrow_function_preserves_param() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "javascript", b"const f = x => x + 1;\n");
            assert!(
                text.contains("x =>") || text.contains("x=>"),
                "arrow function must preserve parameter, got: {text}"
            );
        });
    }

    #[test]
    fn new_expression_preserves_args() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "javascript", b"const o = new Foo(1);\n");
            assert!(
                text.contains("Foo") && text.contains('1'),
                "new expression must preserve constructor and arguments, got: {text}"
            );
        });
    }

    #[test]
    fn spread_element_preserved() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "javascript", b"f(...args);\n");
            assert!(
                text.contains("...args") || text.contains("... args"),
                "spread element must be preserved, got: {text}"
            );
        });
    }

    #[test]
    fn comment_does_not_swallow_next_line() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "javascript", b"// comment\nvar x = 1;\n");
            assert!(
                text.contains("var"),
                "comment must not swallow next line, got: {text}"
            );
        });
    }
}

// =================================================================
// Python
// =================================================================

mod python {
    use super::*;

    #[test]
    fn single_kwarg_call_preserved() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "python", b"f(x=1)\n");
            assert!(
                text.contains("x=1")
                    || text.contains("x =1")
                    || text.contains("x= 1")
                    || text.contains("x = 1"),
                "single kwarg must be preserved, got: {text}"
            );
        });
    }

    #[test]
    fn comment_does_not_swallow_next_line() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "python", b"# comment\nx = 1\n");
            assert!(
                text.contains("x = 1") || text.contains("x =1") || text.contains("x= 1"),
                "comment must not swallow next line, got: {text}"
            );
        });
    }
}

// =================================================================
// Stan
// =================================================================

#[cfg(feature = "lang-stan")]
mod stan {
    use super::*;

    #[test]
    fn comment_does_not_swallow_next_line() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(
                &reg,
                "stan",
                b"// comment\nmodel{\n  y ~ normal(0, 1);\n}\n",
            );
            assert!(
                text.contains("model"),
                "comment must not swallow next line, got: {text}"
            );
            assert!(
                !text.contains("// comment model"),
                "comment and model must be on separate lines, got: {text}"
            );
        });
    }

    #[test]
    fn function_params_preserved() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(
                &reg,
                "stan",
                b"functions{\n  real f(real x){\n    return x;\n  }\n}\nmodel{}\n",
            );
            assert!(
                text.contains("real x") || text.contains("real  x"),
                "function params must be preserved, got: {text}"
            );
        });
    }
}

// =================================================================
// BUGS / JAGS
// =================================================================

#[cfg(feature = "lang-bugs")]
mod bugs {
    use super::*;

    #[test]
    fn comment_does_not_swallow_next_line() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "bugs", b"# comment\nmodel{\n  y ~ dnorm(0, 1)\n}\n");
            assert!(
                text.contains("model") && !text.contains("# comment model"),
                "comment must not swallow next line, got: {text}"
            );
        });
    }
}

#[cfg(feature = "lang-jags")]
mod jags {
    use super::*;

    #[test]
    fn comment_does_not_swallow_next_line() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "jags", b"# comment\nmodel{\n  y ~ dnorm(0, 1)\n}\n");
            assert!(
                text.contains("model") && !text.contains("# comment model"),
                "comment must not swallow next line, got: {text}"
            );
        });
    }

    #[test]
    fn jags_censoring_preserved() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "jags", b"model{\n  y ~ dnorm(mu, tau) I(0, 1)\n}\n");
            assert!(
                text.contains("I(") || text.contains("I ("),
                "JAGS I(...) censoring must be preserved, got: {text}"
            );
        });
    }
}

// =================================================================
// Additional BUGS tests
// =================================================================

#[cfg(feature = "lang-bugs")]
mod bugs_extra {
    use super::*;

    #[test]
    fn data_keyword_preserved() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(
                &reg,
                "bugs",
                b"data{\n  x[1] <- 1\n}\nmodel{\n  y ~ dnorm(0, 1)\n}\n",
            );
            assert!(
                text.contains("data"),
                "BUGS data keyword must be preserved, got: {text}"
            );
        });
    }
}

// =================================================================
// Additional Stan tests
// =================================================================

#[cfg(feature = "lang-stan")]
mod stan_extra {
    use super::*;

    #[test]
    fn constraint_brackets_tight() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(
                &reg,
                "stan",
                b"parameters{\n  real<lower=0> x;\n}\nmodel{}\n",
            );
            assert!(
                !text.contains("< lower") && !text.contains("0 >"),
                "constraint brackets must be tight, got: {text}"
            );
        });
    }
}

// =================================================================
// Additional JS tests
// =================================================================

mod javascript_extra {
    use super::*;

    #[test]
    fn spread_no_space() {
        with_big_stack(|| {
            let reg = registry();
            let text = emit_stripped(&reg, "javascript", b"f(...args);\n");
            assert!(
                text.contains("...args"),
                "spread must be tight with identifier, got: {text}"
            );
        });
    }
}
