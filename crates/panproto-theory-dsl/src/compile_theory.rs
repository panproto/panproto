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
/// theory via [`Theory::full`], and runs typechecking. When the spec
/// declares imports, the importing crate must call
/// [`compile_theory_with_resolver`] so the imports can be resolved.
///
/// # Errors
///
/// Returns errors for parse failures, unknown value kinds, or
/// typechecking violations.
pub fn compile_theory(spec: &TheorySpec) -> Result<Theory, TheoryDslError> {
    compile_theory_with_resolver(spec, &|_name| None)
}

/// Compile a [`TheorySpec`] and then run sample-based coercion law
/// checks on every directed equation using `registry`.
///
/// Binds each sample under the default variable name `"x"`. Theories
/// whose directed equations use a different free variable name
/// should call [`compile_theory_with_law_check_and_var`] directly.
///
/// Produces the same [`Theory`] as [`compile_theory`] when every
/// declared coercion class is consistent with the sample evidence.
/// Otherwise returns
/// [`TheoryDslError::CoercionLawViolation`] with one entry per
/// sample-level violation. The plain [`compile_theory`] path does
/// not run this check, preserving the pre-0.38 behavior.
///
/// # Errors
///
/// Same as [`compile_theory`], plus
/// [`TheoryDslError::CoercionLawViolation`] when a declared coercion
/// class is falsified on `registry`'s samples.
pub fn compile_theory_with_law_check(
    spec: &TheorySpec,
    registry: &panproto_lens::coercion_laws::CoercionSampleRegistry,
) -> Result<Theory, TheoryDslError> {
    compile_theory_with_law_check_and_var(spec, registry, "x")
}

/// Variable-name-parameterized [`compile_theory_with_law_check`].
///
/// Binds each sample under `var_name` instead of the default `"x"`.
/// Use this when the theory's directed equations share a different
/// free variable name (for example, `"v"` or a field key).
///
/// # Errors
///
/// Same as [`compile_theory_with_law_check`].
pub fn compile_theory_with_law_check_and_var(
    spec: &TheorySpec,
    registry: &panproto_lens::coercion_laws::CoercionSampleRegistry,
    var_name: &str,
) -> Result<Theory, TheoryDslError> {
    let theory = compile_theory(spec)?;
    enforce_coercion_laws(&theory, registry, var_name)?;
    Ok(theory)
}

/// Run the sample-based coercion-law check on `theory`, converting any
/// violations into a [`TheoryDslError::CoercionLawViolation`].
///
/// Each directed equation is checked against samples drawn from
/// `registry` and bound under `var_name`. Returns `Ok(())` when every
/// declared coercion class holds on those samples, and vacuously when
/// the theory declares no directed equations.
///
/// # Errors
///
/// Returns [`TheoryDslError::CoercionLawViolation`] carrying one entry
/// per sample-level violation when a declared class is falsified.
pub(crate) fn enforce_coercion_laws(
    theory: &Theory,
    registry: &panproto_lens::coercion_laws::CoercionSampleRegistry,
    var_name: &str,
) -> Result<(), TheoryDslError> {
    let report = panproto_lens::coercion_laws::check_theory_with_var(theory, registry, var_name);
    if report.is_clean() {
        return Ok(());
    }
    let mut violations: Vec<crate::error::CoercionLawViolationDetail> = Vec::new();
    let mut distinct_equations: usize = 0;
    for (name, vs) in report.per_equation {
        if vs.is_empty() {
            continue;
        }
        distinct_equations += 1;
        for v in vs {
            violations.push(crate::error::CoercionLawViolationDetail {
                equation: name.as_ref().to_owned(),
                violation: v,
            });
        }
    }
    Err(TheoryDslError::CoercionLawViolation {
        theory: theory.name.as_ref().to_owned(),
        violations,
        distinct_equations,
    })
}

/// Compile a [`TheorySpec`] with support for imports.
///
/// # Errors
///
/// Same as [`compile_theory`], plus [`TheoryDslError::TheoryNotFound`]
/// when an import names a theory the resolver cannot find.
pub fn compile_theory_with_resolver(
    spec: &TheorySpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<Theory, TheoryDslError> {
    let spec = if spec.imports.is_empty() {
        spec.clone()
    } else {
        resolve_imports(spec, resolver)?
    };
    compile_theory_inner(&spec)
}

fn resolve_imports(
    spec: &TheorySpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<TheorySpec, TheoryDslError> {
    let mut out = spec.clone();
    out.imports = Vec::new();
    for imp in &spec.imports {
        let imported = resolver(&imp.from).ok_or_else(|| TheoryDslError::TheoryNotFound {
            name: imp.from.clone(),
            context: format!("import in theory '{}'", spec.theory),
        })?;
        let expose_set: std::collections::HashSet<String> = imp.expose.iter().cloned().collect();
        let mut name_rewrite: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let rename =
            |name: &str| canonical_name(name, &imp.from, imp.alias.as_deref(), &expose_set);
        for s in &imported.sorts {
            let canonical = rename(&s.name);
            record_rewrite(
                &mut name_rewrite,
                &s.name,
                &canonical,
                imp.alias.as_deref(),
                &expose_set,
                &imp.from,
            );
            out.sorts.insert(0, imported_sort_to_spec(s, canonical));
        }
        for op in &imported.ops {
            let canonical = rename(&op.name);
            record_rewrite(
                &mut name_rewrite,
                &op.name,
                &canonical,
                imp.alias.as_deref(),
                &expose_set,
                &imp.from,
            );
            out.ops
                .insert(0, imported_op_to_spec(op, canonical, &name_rewrite));
        }
        rewrite_inplace(&mut out, &name_rewrite);
    }
    Ok(out)
}

fn canonical_name(
    name: &str,
    from: &str,
    alias: Option<&str>,
    expose: &std::collections::HashSet<String>,
) -> String {
    if expose.contains(name) {
        name.to_string()
    } else if let Some(a) = alias {
        format!("{a}_{name}")
    } else {
        format!("{from}_{name}")
    }
}

fn record_rewrite(
    rewrite: &mut std::collections::HashMap<String, String>,
    original: &str,
    canonical: &str,
    alias: Option<&str>,
    expose: &std::collections::HashSet<String>,
    from: &str,
) {
    // Always populate the fully-qualified form so that references like
    // `<importee_id>.Foo` resolve regardless of alias or expose
    // settings.
    rewrite.insert(format!("{from}.{original}"), canonical.to_string());
    if let Some(a) = alias {
        rewrite.insert(format!("{a}.{original}"), canonical.to_string());
    }
    if expose.contains(original) {
        rewrite.insert(original.to_string(), canonical.to_string());
    }
    // For no-alias, no-expose imports, let the bare name still resolve
    // to the canonical name: callers frequently write `Foo` expecting
    // the importee's sort to be reachable, and the canonical rename
    // already disambiguates when two imports collide.
    if alias.is_none() && !expose.contains(original) {
        rewrite
            .entry(original.to_string())
            .or_insert_with(|| canonical.to_string());
    }
}

fn imported_sort_to_spec(s: &Sort, canonical: String) -> SortSpec {
    SortSpec {
        name: canonical,
        params: s
            .params
            .iter()
            .map(|p| crate::document::ParamSpec {
                name: p.name.to_string(),
                sort: p.sort.to_string(),
                implicit: false,
            })
            .collect(),
        kind: match &s.kind {
            panproto_gat::SortKind::Structural => crate::document::SortKindSpec::Structural,
            panproto_gat::SortKind::Val(vk) => crate::document::SortKindSpec::Val {
                value_kind: vk.as_str().to_string(),
            },
            panproto_gat::SortKind::Coercion { from, to, class } => {
                crate::document::SortKindSpec::Coercion {
                    from: from.as_str().to_string(),
                    to: to.as_str().to_string(),
                    class: format!("{class:?}"),
                }
            }
            panproto_gat::SortKind::Merger(vk) => crate::document::SortKindSpec::Merger {
                value_kind: vk.as_str().to_string(),
            },
        },
        closed: match &s.closure {
            panproto_gat::SortClosure::Open => None,
            panproto_gat::SortClosure::Closed(cs) => {
                Some(cs.iter().map(ToString::to_string).collect())
            }
        },
    }
}

fn imported_op_to_spec(
    op: &Operation,
    canonical: String,
    name_rewrite: &std::collections::HashMap<String, String>,
) -> OpSpec {
    OpSpec {
        name: canonical,
        input: None,
        inputs: Some(
            op.inputs
                .iter()
                .map(|(n, s, _)| crate::document::ParamSpec {
                    name: n.to_string(),
                    sort: rewrite_sort_string(&s.to_string(), name_rewrite),
                    implicit: false,
                })
                .collect(),
        ),
        output: rewrite_sort_string(&op.output.to_string(), name_rewrite),
    }
}

fn rewrite_inplace(out: &mut TheorySpec, name_rewrite: &std::collections::HashMap<String, String>) {
    for sort in &mut out.sorts {
        for p in &mut sort.params {
            p.sort = rewrite_sort_string(&p.sort, name_rewrite);
        }
    }
    for op in &mut out.ops {
        op.output = rewrite_sort_string(&op.output, name_rewrite);
        if let Some(ins) = &mut op.inputs {
            for p in ins {
                p.sort = rewrite_sort_string(&p.sort, name_rewrite);
            }
        }
        if let Some(i) = &mut op.input {
            *i = rewrite_sort_string(i, name_rewrite);
        }
    }
}

fn rewrite_sort_string(s: &str, rewrite: &std::collections::HashMap<String, String>) -> String {
    // Replace `Alias.Name` (or bare exposed names) with their canonical
    // form. We match tokens separated by non-identifier characters so
    // that `Foo.Bar(x)` rewrites to the canonical `Foo_Bar(x)` and a
    // bare `Bar` in the `expose` list rewrites the same way.
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() {
                let cc = bytes[i] as char;
                if cc.is_ascii_alphanumeric() || cc == '_' || cc == '.' {
                    i += 1;
                } else {
                    break;
                }
            }
            let tok = &s[start..i];
            match rewrite.get(tok) {
                Some(canonical) => result.push_str(canonical),
                None => result.push_str(tok),
            }
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

fn compile_theory_inner(spec: &TheorySpec) -> Result<Theory, TheoryDslError> {
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

    // Gate the theory's directed rewrite system on local confluence and LPO
    // termination. Compilation is not blocked on the result: a rewrite system
    // that is not provably sound is reported for investigation, so the gate
    // cannot reject an otherwise well-typed theory.
    if let Ok(report) = panproto_gat::validate_rewrite_system(&theory) {
        for warning in report.warnings() {
            eprintln!(
                "theory `{}`: rewrite-system warning: {warning}",
                spec.theory.as_str()
            );
        }
    }

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
        SortKindSpec::Val { value_kind } => SortKind::Val(parse_value_kind(value_kind)?),
        SortKindSpec::Coercion { from, to, class } => SortKind::Coercion {
            from: parse_value_kind(from)?,
            to: parse_value_kind(to)?,
            class: parse_coercion_class(class)?,
        },
        SortKindSpec::Merger { value_kind } => SortKind::Merger(parse_value_kind(value_kind)?),
    };

    let closure = spec
        .closed
        .as_ref()
        .map_or(panproto_gat::SortClosure::Open, |ctors| {
            panproto_gat::SortClosure::Closed(ctors.iter().map(|c| Arc::from(c.as_str())).collect())
        });

    Ok(Sort {
        name: Arc::from(spec.name.as_str()),
        params,
        kind,
        closure,
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
            // Take the first character safely: byte slicing with [..1]
            // would panic on a multi-byte UTF-8 boundary. Fall back to a
            // conventional placeholder when the sort name is empty or
            // starts with a non-ASCII character (giving it a readable
            // default rather than an empty string).
            let first_char = input_sort.chars().next().filter(char::is_ascii_alphabetic);
            let param_name: String =
                first_char.map_or_else(|| "x".to_string(), |c| c.to_ascii_lowercase().to_string());
            let input = parse_sort_expr(input_sort).map_err(|msg| TheoryDslError::TermParse {
                context: op_context("input sort"),
                message: msg,
            })?;
            Operation::unary(spec.name.as_str(), param_name.as_str(), input, output)
        }
        (None, Some(inputs)) => {
            let input_triples: Vec<(Arc<str>, SortExpr, panproto_gat::Implicit)> = inputs
                .iter()
                .map(|p| {
                    parse_sort_expr(&p.sort)
                        .map(|sort| {
                            let imp = if p.implicit {
                                panproto_gat::Implicit::Yes
                            } else {
                                panproto_gat::Implicit::No
                            };
                            (Arc::from(p.name.as_str()), sort, imp)
                        })
                        .map_err(|msg| TheoryDslError::TermParse {
                            context: op_context(&format!("input '{pname}'", pname = p.name)),
                            message: msg,
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Operation::with_implicit(spec.name.as_str(), input_triples, output)
        }
        (None, None) => Operation::nullary(spec.name.as_str(), output),
    })
}

/// Parse a sort string into a [`SortExpr`].
///
/// A bare identifier parses as [`SortExpr::Name`]; `Ident(arg1, arg2, ...)`
/// parses as a [`SortExpr::App`] with the argument list parsed as terms
/// via [`parse_term`]. An `Ident()` input with no arguments normalizes
/// to [`SortExpr::Name`] via the smart constructor.
///
/// # Errors
///
/// Returns an error describing the problem for: empty input, malformed
/// identifiers, unclosed parentheses, unexpected trailing input, or any
/// error propagated from parsing an argument term.
pub fn parse_sort_expr(s: &str) -> Result<SortExpr, String> {
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

    let source_kind = spec
        .source_kind
        .as_deref()
        .map(parse_value_kind)
        .transpose()?;
    let target_kind = spec
        .target_kind
        .as_deref()
        .map(parse_value_kind)
        .transpose()?;
    let coercion_class = parse_coercion_class(&spec.coercion_class)?;

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
        value_kind: parse_value_kind(&spec.value_kind)?,
        strategy,
    })
}

// ═══════════════════════════════════════════════════════════════════
// Parsing helpers
// ═══════════════════════════════════════════════════════════════════

fn parse_value_kind(s: &str) -> Result<ValueKind, TheoryDslError> {
    match s {
        "boolean" | "bool" => Ok(ValueKind::Bool),
        "integer" | "int" => Ok(ValueKind::Int),
        "float" | "number" => Ok(ValueKind::Float),
        "string" | "str" => Ok(ValueKind::Str),
        "bytes" => Ok(ValueKind::Bytes),
        "token" => Ok(ValueKind::Token),
        "null" => Ok(ValueKind::Null),
        "date-time" | "datetime" => Ok(ValueKind::DateTime),
        "date" => Ok(ValueKind::Date),
        "time" => Ok(ValueKind::Time),
        "decimal" => Ok(ValueKind::Decimal),
        "uuid" => Ok(ValueKind::Uuid),
        "any" => Ok(ValueKind::Any),
        other => Err(TheoryDslError::UnknownValueKind {
            kind: other.to_owned(),
        }),
    }
}

fn parse_coercion_class(s: &str) -> Result<CoercionClass, TheoryDslError> {
    match s {
        "iso" => Ok(CoercionClass::Iso),
        "retraction" => Ok(CoercionClass::Retraction),
        "projection" => Ok(CoercionClass::Projection),
        "opaque" => Ok(CoercionClass::Opaque),
        other => Err(TheoryDslError::UnknownCoercionClass {
            class: other.to_owned(),
        }),
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
///
/// # Errors
///
/// Returns a descriptive message for parse failures: empty input,
/// malformed identifiers, unclosed parentheses, missing keywords, or
/// errors propagated from nested term parses.
pub fn parse_term(s: &str) -> Result<panproto_gat::Term, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty term string".to_owned());
    }

    if let Some(rest) = s.strip_prefix("case ") {
        return parse_case_term(rest);
    }

    if let Some(rest) = s.strip_prefix("let ") {
        return parse_let_term(rest);
    }

    if let Some(rest) = s.strip_prefix('?') {
        let rest = rest.trim();
        if rest.is_empty() {
            return Ok(panproto_gat::Term::Hole { name: None });
        }
        validate_identifier(rest, "hole name")?;
        return Ok(panproto_gat::Term::Hole {
            name: Some(Arc::from(rest)),
        });
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

/// Parse the body of a `let` term, given the text following the
/// leading `let ` keyword.
///
/// Grammar:
///
/// ```text
/// let_body ::= ident '=' term 'in' term
/// ```
fn parse_let_term(rest: &str) -> Result<panproto_gat::Term, String> {
    let rest = rest.trim();
    let eq_pos = rest
        .find('=')
        .ok_or_else(|| format!("let term missing `=`: {rest:?}"))?;
    let name_part = rest[..eq_pos].trim();
    validate_identifier(name_part, "let binder")?;
    let after_eq = &rest[eq_pos + 1..];
    let in_pos = find_top_level_keyword(after_eq, "in")
        .ok_or_else(|| format!("let term missing `in`: {rest:?}"))?;
    let bound_str = after_eq[..in_pos].trim();
    let body_str = after_eq[in_pos + 2..].trim();
    let bound = parse_term(bound_str)?;
    let body = parse_term(body_str)?;
    Ok(panproto_gat::Term::Let {
        name: Arc::from(name_part),
        bound: Box::new(bound),
        body: Box::new(body),
    })
}

/// Parse the body of a `case` term, given the text following the
/// leading `case ` keyword.
///
/// Grammar:
///
/// ```text
/// case_body ::= scrutinee 'of' branch ('|' branch)* 'end'
/// branch    ::= ctor '(' binder (',' binder)* ')' '=>' body
/// ```
fn parse_case_term(rest: &str) -> Result<panproto_gat::Term, String> {
    let rest = rest.trim();
    let stripped = rest
        .strip_suffix("end")
        .ok_or_else(|| format!("case term missing trailing `end`: {rest:?}"))?
        .trim_end();
    let of_pos = find_top_level_keyword(stripped, "of")
        .ok_or_else(|| format!("case term missing `of` keyword: {rest:?}"))?;
    let scrutinee_str = stripped[..of_pos].trim();
    let branches_str = stripped[of_pos + 2..].trim();
    let scrutinee = parse_term(scrutinee_str)?;

    let branch_parts = split_top_level_pipes(branches_str);
    if branch_parts.is_empty() {
        return Err(format!("case term has no branches: {rest:?}"));
    }
    let branches = branch_parts
        .into_iter()
        .map(parse_case_branch)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(panproto_gat::Term::Case {
        scrutinee: Box::new(scrutinee),
        branches,
    })
}

fn parse_case_branch(s: &str) -> Result<panproto_gat::CaseBranch, String> {
    let s = s.trim();
    let arrow = s
        .find("=>")
        .ok_or_else(|| format!("case branch missing `=>`: {s:?}"))?;
    let head = s[..arrow].trim();
    let body_str = s[arrow + 2..].trim();
    let body = parse_term(body_str)?;

    let paren_pos = head
        .find('(')
        .ok_or_else(|| format!("case branch constructor missing `(`: {head:?}"))?;
    let ctor_name = head[..paren_pos].trim();
    validate_identifier(ctor_name, "case branch constructor")?;
    let inner = &head[paren_pos + 1..];
    let close = find_matching_paren(inner)
        .ok_or_else(|| format!("unclosed paren in case branch: {head:?}"))?;
    let trailing = inner[close + 1..].trim();
    if !trailing.is_empty() {
        return Err(format!(
            "unexpected trailing input in case branch {head:?}: {trailing:?}"
        ));
    }
    let binders_str = &inner[..close];
    let binders = if binders_str.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level_commas(binders_str)
            .into_iter()
            .map(|b| {
                let b = b.trim();
                validate_identifier(b, "case branch binder")?;
                Ok::<Arc<str>, String>(Arc::from(b))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(panproto_gat::CaseBranch {
        constructor: Arc::from(ctor_name),
        binders,
        body,
    })
}

/// Find a whitespace-delimited occurrence of `keyword` at the top level
/// (not inside parens). Returns the byte offset of the keyword start.
fn find_top_level_keyword(s: &str, keyword: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let klen = keyword.len();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0
            && i + klen <= bytes.len()
            && &s[i..i + klen] == keyword
            && (i == 0 || (bytes[i - 1] as char).is_whitespace())
            && (i + klen == bytes.len() || (bytes[i + klen] as char).is_whitespace())
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Split by `|` at the top level (not inside parens).
fn split_top_level_pipes(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            '|' if depth == 0 => {
                let p = s[start..i].trim();
                if !p.is_empty() {
                    parts.push(p);
                }
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
            closed: None,
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
                implicit: false,
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
            imports: vec![],
            sorts: vec![
                SortSpec {
                    name: "Vertex".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Structural,
                    closed: None,
                },
                SortSpec {
                    name: "Edge".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Structural,
                    closed: None,
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

    fn lying_iso_theory_spec() -> TheorySpec {
        use crate::document::DirectedEqSpec;
        TheorySpec {
            theory: "ThLying".to_owned(),
            extends: vec![],
            imports: vec![],
            sorts: vec![SortSpec {
                name: "Str".to_owned(),
                params: vec![],
                kind: SortKindSpec::Val {
                    value_kind: "string".to_owned(),
                },
                closed: None,
            }],
            ops: vec![OpSpec {
                name: "upper".to_owned(),
                input: Some("Str".to_owned()),
                inputs: None,
                output: "Str".to_owned(),
            }],
            equations: vec![],
            directed_equations: vec![DirectedEqSpec {
                name: "lying_upper_iso".to_owned(),
                lhs: "upper(x)".to_owned(),
                rhs: "x".to_owned(),
                impl_expr: "upper(x)".to_owned(),
                inverse: Some("x".to_owned()),
                source_kind: Some("string".to_owned()),
                target_kind: Some("string".to_owned()),
                coercion_class: "iso".to_owned(),
            }],
            policies: vec![],
        }
    }

    #[test]
    fn compile_theory_accepts_lying_iso_without_law_check() -> TestResult {
        // Baseline: plain compile does not consult coercion classes,
        // so a lying Iso declaration round-trips through the
        // compiler.
        let spec = lying_iso_theory_spec();
        let theory = compile_theory(&spec)?;
        assert_eq!(theory.directed_eqs.len(), 1);
        Ok(())
    }

    #[test]
    fn compile_theory_with_law_check_rejects_lying_iso() -> TestResult {
        let spec = lying_iso_theory_spec();
        let registry = panproto_lens::coercion_laws::CoercionSampleRegistry::with_defaults();
        let result = compile_theory_with_law_check(&spec, &registry);
        let Err(err) = result else {
            return Err("lying iso must be rejected".into());
        };
        match err {
            TheoryDslError::CoercionLawViolation {
                theory,
                violations,
                distinct_equations,
            } => {
                assert_eq!(theory, "ThLying");
                assert!(!violations.is_empty());
                assert!(violations.iter().all(|d| d.equation == "lying_upper_iso"));
                assert_eq!(distinct_equations, 1);
                // The structured payload must be preserved so
                // downstream consumers can match on the variant.
                assert!(
                    violations.iter().any(|d| matches!(
                        d.violation,
                        panproto_lens::coercion_laws::CoercionLawViolation::Backward { .. }
                            | panproto_lens::coercion_laws::CoercionLawViolation::Forward { .. }
                    )),
                    "expected at least one Backward or Forward structured violation, \
                     got {violations:?}",
                );
                Ok(())
            }
            other => Err(format!("expected CoercionLawViolation, got {other:?}").into()),
        }
    }

    #[test]
    fn compile_theory_with_law_check_accepts_honest_iso() -> TestResult {
        let spec = TheorySpec {
            theory: "ThHonest".to_owned(),
            extends: vec![],
            imports: vec![],
            sorts: vec![SortSpec {
                name: "Str".to_owned(),
                params: vec![],
                kind: SortKindSpec::Val {
                    value_kind: "string".to_owned(),
                },
                closed: None,
            }],
            ops: vec![],
            equations: vec![],
            directed_equations: vec![crate::document::DirectedEqSpec {
                name: "identity_iso".to_owned(),
                lhs: "x".to_owned(),
                rhs: "x".to_owned(),
                impl_expr: "x".to_owned(),
                inverse: Some("x".to_owned()),
                source_kind: Some("string".to_owned()),
                target_kind: Some("string".to_owned()),
                coercion_class: "iso".to_owned(),
            }],
            policies: vec![],
        };
        let registry = panproto_lens::coercion_laws::CoercionSampleRegistry::with_defaults();
        let theory = compile_theory_with_law_check(&spec, &registry)?;
        assert_eq!(theory.directed_eqs.len(), 1);
        Ok(())
    }
}
