//! Compile a [`TheorySpec`] into a [`Theory`].
//!
//! Maps DSL spec types to GAT engine types: sorts, operations,
//! equations, directed equations, and conflict policies. Runs
//! typechecking on the result.

use std::sync::Arc;

use panproto_gat::{
    CoercionClass, ConflictPolicy, ConflictStrategy, DirectedEquation, Equation, Operation, Sort,
    SortExpr, SortKind, SortParam, Theory, ValueKind,
};

use crate::document::{
    DirectedEqSpec, EquationSpec, OpSpec, PolicySpec, SortKindSpec, SortSpec, StrategySpec,
    TheorySpec,
};
use crate::error::TheoryDslError;

/// Compile a [`TheorySpec`] into a [`Theory`].
///
/// Parses all sorts, operations, equations, directed equations, and
/// policies from spec types into GAT engine types, constructs the
/// theory via [`Theory::full`], and runs typechecking.
///
/// # Errors
///
/// Returns errors for parse failures, unknown value kinds, or
/// typechecking violations.
pub fn compile_theory(spec: &TheorySpec) -> Result<Theory, TheoryDslError> {
    let sorts: Vec<Sort> = spec
        .sorts
        .iter()
        .map(|s| compile_sort(s, &spec.theory))
        .collect::<Result<Vec<_>, _>>()?;
    let ops: Vec<Operation> = spec
        .ops
        .iter()
        .map(|o| compile_op(o, &spec.theory))
        .collect::<Result<Vec<_>, _>>()?;
    let eqs: Vec<Equation> = spec
        .equations
        .iter()
        .map(|eq| compile_equation(eq, &spec.theory))
        .collect::<Result<Vec<_>, _>>()?;
    let directed_eqs: Vec<DirectedEquation> = spec
        .directed_equations
        .iter()
        .map(|deq| compile_directed_eq(deq, &spec.theory))
        .collect::<Result<Vec<_>, _>>()?;
    let policies: Vec<ConflictPolicy> = spec
        .policies
        .iter()
        .map(|p| compile_policy(p, &spec.theory))
        .collect::<Result<Vec<_>, _>>()?;

    let extends: Vec<Arc<str>> = spec.extends.iter().map(|s| Arc::from(s.as_str())).collect();

    let theory = Theory::full(
        spec.theory.as_str(),
        extends,
        sorts,
        ops,
        eqs,
        directed_eqs,
        policies,
    );

    panproto_gat::typecheck_theory(&theory).map_err(|e| TheoryDslError::TypeCheck {
        theory: spec.theory.clone(),
        message: e.to_string(),
    })?;

    Ok(theory)
}

fn compile_sort(spec: &SortSpec, theory_name: &str) -> Result<Sort, TheoryDslError> {
    let params: Vec<SortParam> = spec
        .params
        .iter()
        .map(|p| {
            parse_sort_expr(&p.sort)
                .map(|sort| SortParam::new(p.name.as_str(), sort))
                .map_err(|msg| TheoryDslError::TermParse {
                    context: format!(
                        "parameter '{pname}' of sort '{sname}' in theory '{theory_name}'",
                        pname = p.name,
                        sname = spec.name,
                    ),
                    message: msg,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let kind = match &spec.kind {
        SortKindSpec::Structural => SortKind::Structural,
        SortKindSpec::Val { value_kind } => SortKind::Val(parse_value_kind(value_kind)),
        SortKindSpec::Coercion { from, to, class } => SortKind::Coercion {
            from: parse_value_kind(from),
            to: parse_value_kind(to),
            class: parse_coercion_class(class),
        },
        SortKindSpec::Merger { value_kind } => SortKind::Merger(parse_value_kind(value_kind)),
    };

    Ok(if params.is_empty() {
        Sort::with_kind(spec.name.as_str(), kind)
    } else {
        Sort {
            name: Arc::from(spec.name.as_str()),
            params,
            kind,
        }
    })
}

fn compile_op(spec: &OpSpec, theory_name: &str) -> Result<Operation, TheoryDslError> {
    let op_context = |suffix: &str| -> String {
        format!(
            "{suffix} of op '{opname}' in theory '{theory_name}'",
            opname = spec.name,
        )
    };
    let output = parse_sort_expr(&spec.output).map_err(|msg| TheoryDslError::TermParse {
        context: op_context("output sort"),
        message: msg,
    })?;
    Ok(match (&spec.input, &spec.inputs) {
        (Some(input_sort), _) => {
            let param_name = input_sort[..1].to_ascii_lowercase();
            let input = parse_sort_expr(input_sort).map_err(|msg| TheoryDslError::TermParse {
                context: op_context("input sort"),
                message: msg,
            })?;
            Operation::unary(spec.name.as_str(), param_name.as_str(), input, output)
        }
        (None, Some(inputs)) => {
            let input_pairs: Vec<(Arc<str>, SortExpr)> = inputs
                .iter()
                .map(|p| {
                    parse_sort_expr(&p.sort)
                        .map(|sort| (Arc::from(p.name.as_str()), sort))
                        .map_err(|msg| TheoryDslError::TermParse {
                            context: op_context(&format!("input '{pname}'", pname = p.name)),
                            message: msg,
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Operation::new(spec.name.as_str(), input_pairs, output)
        }
        (None, None) => Operation::nullary(spec.name.as_str(), output),
    })
}

/// Parse a sort string into a [`SortExpr`]. A bare identifier parses as
/// [`SortExpr::Name`]; `Ident(arg1, arg2, ...)` parses as a
/// [`SortExpr::App`] with the argument list parsed as terms via
/// [`parse_term`]. An `Ident()` input with no arguments normalizes to
/// [`SortExpr::Name`] via the smart constructor.
///
/// # Errors
///
/// Returns an error describing the problem for: empty input, malformed
/// identifiers, unclosed parentheses, unexpected trailing input, or any
/// error propagated from parsing an argument term.
pub(crate) fn parse_sort_expr(s: &str) -> Result<SortExpr, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("empty sort expression".to_owned());
    }
    match trimmed.find('(') {
        None => {
            validate_identifier(trimmed, "sort name")?;
            Ok(SortExpr::Name(Arc::from(trimmed)))
        }
        Some(paren_pos) => {
            let head = trimmed[..paren_pos].trim();
            validate_identifier(head, "sort head")?;
            let inner = &trimmed[paren_pos + 1..];
            let close = find_matching_paren(inner)
                .ok_or_else(|| format!("unclosed parenthesis in sort expression: {trimmed:?}"))?;
            let trailing = inner[close + 1..].trim();
            if !trailing.is_empty() {
                return Err(format!(
                    "unexpected trailing input after closing paren in sort expression \
                     {trimmed:?}: {trailing:?}"
                ));
            }
            let args_str = &inner[..close];
            let args = split_top_level_commas(args_str)
                .into_iter()
                .map(parse_term)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SortExpr::app(Arc::from(head), args))
        }
    }
}

fn compile_equation(spec: &EquationSpec, theory_name: &str) -> Result<Equation, TheoryDslError> {
    let lhs = parse_term(&spec.lhs).map_err(|msg| TheoryDslError::TermParse {
        context: format!(
            "equation '{name}' in theory '{theory_name}'",
            name = spec.name
        ),
        message: msg,
    })?;
    let rhs = parse_term(&spec.rhs).map_err(|msg| TheoryDslError::TermParse {
        context: format!(
            "equation '{name}' in theory '{theory_name}'",
            name = spec.name
        ),
        message: msg,
    })?;
    Ok(Equation::new(spec.name.as_str(), lhs, rhs))
}

fn compile_directed_eq(
    spec: &DirectedEqSpec,
    theory_name: &str,
) -> Result<DirectedEquation, TheoryDslError> {
    let ctx = format!(
        "directed equation '{name}' in theory '{theory_name}'",
        name = spec.name
    );
    let lhs = parse_term(&spec.lhs).map_err(|msg| TheoryDslError::TermParse {
        context: ctx.clone(),
        message: msg,
    })?;
    let rhs = parse_term(&spec.rhs).map_err(|msg| TheoryDslError::TermParse {
        context: ctx.clone(),
        message: msg,
    })?;
    let impl_term = parse_expr(&spec.impl_expr, &ctx)?;
    let inverse = spec
        .inverse
        .as_ref()
        .map(|inv| parse_expr(inv, &ctx))
        .transpose()?;

    let source_kind = spec.source_kind.as_deref().map(parse_value_kind);
    let target_kind = spec.target_kind.as_deref().map(parse_value_kind);
    let coercion_class = parse_coercion_class(&spec.coercion_class);

    Ok(DirectedEquation {
        name: Arc::from(spec.name.as_str()),
        lhs,
        rhs,
        impl_term,
        inverse,
        source_kind,
        target_kind,
        coercion_class,
    })
}

fn compile_policy(spec: &PolicySpec, theory_name: &str) -> Result<ConflictPolicy, TheoryDslError> {
    let strategy = match &spec.strategy {
        StrategySpec::KeepLeft => ConflictStrategy::KeepLeft,
        StrategySpec::KeepRight => ConflictStrategy::KeepRight,
        StrategySpec::Fail => ConflictStrategy::Fail,
        StrategySpec::Custom { expr } => {
            let ctx = format!(
                "policy '{name}' in theory '{theory_name}'",
                name = spec.name
            );
            let parsed = parse_expr(expr, &ctx)?;
            ConflictStrategy::Custom(parsed)
        }
    };

    Ok(ConflictPolicy {
        name: Arc::from(spec.name.as_str()),
        value_kind: parse_value_kind(&spec.value_kind),
        strategy,
    })
}

// ═══════════════════════════════════════════════════════════════════
// Parsing helpers
// ═══════════════════════════════════════════════════════════════════

fn parse_value_kind(s: &str) -> ValueKind {
    match s {
        "boolean" | "bool" => ValueKind::Bool,
        "integer" | "int" => ValueKind::Int,
        "float" | "number" => ValueKind::Float,
        "string" | "str" => ValueKind::Str,
        "bytes" => ValueKind::Bytes,
        "token" => ValueKind::Token,
        "null" => ValueKind::Null,
        _ => ValueKind::Any,
    }
}

fn parse_coercion_class(s: &str) -> CoercionClass {
    match s {
        "iso" => CoercionClass::Iso,
        "retraction" => CoercionClass::Retraction,
        "projection" => CoercionClass::Projection,
        _ => CoercionClass::Opaque,
    }
}

/// Parse an expression string via the panproto expression parser.
fn parse_expr(expr_str: &str, context: &str) -> Result<panproto_expr::Expr, TheoryDslError> {
    let tokens =
        panproto_expr_parser::tokenize(expr_str).map_err(|e| TheoryDslError::ExprParse {
            context: context.to_owned(),
            message: format!("tokenization failed: {e}"),
        })?;

    panproto_expr_parser::parse(&tokens).map_err(|errors| TheoryDslError::ExprParse {
        context: context.to_owned(),
        message: errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    })
}

/// Parse a term string into a GAT [`Term`](panproto_gat::Term).
///
/// Supports two forms:
/// - Variable: `x`, `my_var`
/// - Application: `op(arg1, arg2, ...)` with recursive arguments
///
/// Grammar:
/// ```text
/// term  ::= ident '(' term (',' term)* ')'   -- application
///          | ident                              -- variable
/// ident ::= [a-zA-Z_][a-zA-Z0-9_]*
/// ```
pub(crate) fn parse_term(s: &str) -> Result<panproto_gat::Term, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty term string".to_owned());
    }

    match s.find('(') {
        None => {
            validate_identifier(s, "term variable")?;
            Ok(panproto_gat::Term::Var(Arc::from(s)))
        }
        Some(paren_pos) => {
            let op_name = s[..paren_pos].trim();
            validate_identifier(op_name, "term operation")?;
            let inner = &s[paren_pos + 1..];
            let close = find_matching_paren(inner)
                .ok_or_else(|| format!("unclosed parenthesis in term: {s:?}"))?;
            let trailing = inner[close + 1..].trim();
            if !trailing.is_empty() {
                return Err(format!(
                    "unexpected trailing input after closing paren in term {s:?}: {trailing:?}"
                ));
            }
            let args_str = &inner[..close];
            let args = split_top_level_commas(args_str)
                .into_iter()
                .map(parse_term)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(panproto_gat::Term::App {
                op: Arc::from(op_name),
                args,
            })
        }
    }
}

/// Validate a parsed identifier: non-empty, starts with `_` or a letter,
/// continues with letters, digits, or underscores. Returns a descriptive
/// error naming `kind` (e.g. "sort head", "term variable") when the
/// input violates the grammar.
fn validate_identifier(s: &str, kind: &str) -> Result<(), String> {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return Err(format!("empty {kind}"));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(format!(
            "{kind} {s:?} must start with a letter or underscore"
        ));
    }
    for ch in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(format!("{kind} {s:?} contains invalid character {ch:?}"));
        }
    }
    Ok(())
}

/// Find the position of the closing `)` that matches the opening `(`.
/// The input starts immediately after the opening `(`.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1u32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a string by commas at the top level (not inside parentheses).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_parse_term_var() -> TestResult {
        let term = parse_term("x")?;
        assert!(matches!(term, panproto_gat::Term::Var(ref v) if &**v == "x"));
        Ok(())
    }

    #[test]
    fn test_parse_term_app() -> TestResult {
        let term = parse_term("f(x, y)")?;
        let panproto_gat::Term::App { ref op, ref args } = term else {
            return Err("expected App".into());
        };
        assert_eq!(&**op, "f");
        assert_eq!(args.len(), 2);
        Ok(())
    }

    #[test]
    fn test_parse_term_nested() -> TestResult {
        let term = parse_term("f(g(x), y)")?;
        let panproto_gat::Term::App { ref op, ref args } = term else {
            return Err("expected App".into());
        };
        assert_eq!(&**op, "f");
        assert_eq!(args.len(), 2);
        assert!(matches!(&args[0], panproto_gat::Term::App { .. }));
        Ok(())
    }

    #[test]
    fn test_compile_sort_simple() -> TestResult {
        let spec = SortSpec {
            name: "Vertex".to_owned(),
            params: vec![],
            kind: SortKindSpec::Structural,
        };
        let sort = compile_sort(&spec, "Th")?;
        assert_eq!(&*sort.name, "Vertex");
        assert!(sort.params.is_empty());
        assert!(matches!(sort.kind, SortKind::Structural));
        Ok(())
    }

    #[test]
    fn test_compile_op_unary() -> TestResult {
        let spec = OpSpec {
            name: "src".to_owned(),
            input: Some("Edge".to_owned()),
            inputs: None,
            output: "Vertex".to_owned(),
        };
        let op = compile_op(&spec, "Th")?;
        assert_eq!(&*op.name, "src");
        assert_eq!(op.arity(), 1);
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_bare_name() -> TestResult {
        let e = parse_sort_expr("Ob")?;
        if let SortExpr::Name(n) = e {
            assert_eq!(&*n, "Ob");
        } else {
            return Err("expected Name".into());
        }
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_applied() -> TestResult {
        let e = parse_sort_expr("Hom(x, y)")?;
        if let SortExpr::App { name, args } = e {
            assert_eq!(&*name, "Hom");
            assert_eq!(args.len(), 2);
        } else {
            return Err("expected App".into());
        }
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_nested_args() -> TestResult {
        let e = parse_sort_expr("Tm(extend(G, A), B)")?;
        if let SortExpr::App { name, args } = e {
            assert_eq!(&*name, "Tm");
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0], panproto_gat::Term::App { .. }));
        } else {
            return Err("expected App".into());
        }
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_empty_input_errors() {
        assert!(parse_sort_expr("").is_err());
        assert!(parse_sort_expr("   ").is_err());
    }

    #[test]
    fn test_parse_sort_expr_unclosed_paren_errors() {
        assert!(parse_sort_expr("Tm(Ctx, A").is_err());
        assert!(parse_sort_expr("Hom(").is_err());
    }

    #[test]
    fn test_parse_sort_expr_trailing_garbage_errors() {
        assert!(parse_sort_expr("Tm(A) junk").is_err());
    }

    #[test]
    fn test_parse_sort_expr_malformed_identifier_errors() {
        assert!(parse_sort_expr("1Hom(x, y)").is_err());
        assert!(parse_sort_expr("Hom(1bad, y)").is_err());
    }

    #[test]
    fn test_parse_sort_expr_empty_arglist_normalizes_to_name() -> TestResult {
        // `Tm()` normalizes to `Name("Tm")` via the smart constructor,
        // so Display and hashing match a bare-name spelling.
        let e = parse_sort_expr("Tm()")?;
        assert_eq!(e, SortExpr::Name(Arc::from("Tm")));
        Ok(())
    }

    #[test]
    fn test_compile_op_with_dependent_output() -> TestResult {
        let spec = OpSpec {
            name: "id".to_owned(),
            input: None,
            inputs: Some(vec![crate::document::ParamSpec {
                name: "x".to_owned(),
                sort: "Ob".to_owned(),
            }]),
            output: "Hom(x, x)".to_owned(),
        };
        let op = compile_op(&spec, "Th")?;
        assert_eq!(&*op.name, "id");
        if let SortExpr::App { name, args } = &op.output {
            assert_eq!(&**name, "Hom");
            assert_eq!(args.len(), 2);
        } else {
            return Err("expected dependent Hom output".into());
        }
        Ok(())
    }

    #[test]
    fn test_compile_simple_theory() -> TestResult {
        let spec = TheorySpec {
            theory: "ThTest".to_owned(),
            extends: vec![],
            sorts: vec![
                SortSpec {
                    name: "Vertex".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Structural,
                },
                SortSpec {
                    name: "Edge".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Structural,
                },
            ],
            ops: vec![OpSpec {
                name: "src".to_owned(),
                input: Some("Edge".to_owned()),
                inputs: None,
                output: "Vertex".to_owned(),
            }],
            equations: vec![],
            directed_equations: vec![],
            policies: vec![],
        };
        let theory = compile_theory(&spec)?;
        assert_eq!(&*theory.name, "ThTest");
        assert_eq!(theory.sorts.len(), 2);
        assert_eq!(theory.ops.len(), 1);
        Ok(())
    }
}
