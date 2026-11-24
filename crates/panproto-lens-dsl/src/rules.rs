//! Compilation of pattern-match rewrite rules to lens steps.
//!
//! Rules are a declarative shorthand for common schema transformations:
//! name remapping, attribute operations, and feature filtering. Each
//! rule is expanded into [`Step`]s, in a fixed order, and each step is
//! compiled with [`compile_step`](crate::steps::compile_step).
//!
//! A rule `{ pattern: { name: n }, replace: r }` expands, in this order,
//! to: a `rename_sort` from `n` to `r.name` when that is a literal other
//! than `n`, or a `compute_field` of `name` when it is a template; a
//! `rename_field` per `rename_attrs` entry, a `remove_field` per
//! `drop_attrs` entry, an `add_field` per `add_attrs` entry and an
//! `apply_expr` per `map_attr_value` entry, the map entries in the order
//! of their keys. A rule without `replace` (or with `replace: null`)
//! expands to a `drop_sort` of `n`. After every rule's steps come one
//! `KeepFields` stage holding every rule's `keep_attrs`, in rule order,
//! and, under `passthrough: drop`, one `KeepFields` stage holding the name
//! each rule with a replacement keeps.

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};

use crate::document::{AddFieldSpec, Passthrough, RenameSpec, ReplacementName, Rule, Step};
use crate::error::LensDslError;
use crate::path::DocumentPath;
use crate::steps::{self, CompiledSteps};

/// Compile a set of rules into a [`CompiledSteps`].
///
/// The `body_vertex` is the parent vertex for field operations.
///
/// # Errors
///
/// Refusals located relative to the rule list: a refusal of a step that
/// rule `i` expands to names the rule member the step came from (for
/// instance `/i/replace/add_attrs/<key>`), and a malformed template is
/// [`LensDslError::RuleCompile`] at `/i/replace/name`.
pub fn compile_rules(
    rules: &[Rule],
    passthrough: Option<Passthrough>,
    body_vertex: &str,
) -> Result<CompiledSteps, LensDslError> {
    let mut parts = Vec::new();
    for (i, rule) in rules.iter().enumerate() {
        let rule_path = DocumentPath::root().index(i);
        for (origin, step) in expand_rule(rule).map_err(|e| e.within(&rule_path))? {
            let located = rule_path.join(&origin);
            parts.push(
                steps::compile_step(&step, body_vertex)
                    .map_err(|e| e.into_unlocated().within(&located))?,
            );
        }
    }

    let keep_attrs: Vec<String> = rules
        .iter()
        .filter_map(|rule| rule.replace.as_ref()?.keep_attrs.clone())
        .flatten()
        .collect();
    if !keep_attrs.is_empty() {
        parts.push(keep_stage(body_vertex, keep_attrs));
    }

    // Under `passthrough: drop`, keep only the features the rules name:
    // the literal new name of a renaming rule, else the matched name.
    if passthrough == Some(Passthrough::Drop) {
        let kept: Vec<String> = rules
            .iter()
            .filter_map(|rule| {
                let replacement = rule.replace.as_ref()?;
                Some(match &replacement.name {
                    Some(ReplacementName::Literal(name)) => name.clone(),
                    Some(ReplacementName::Template { .. }) | None => rule.match_.name.clone(),
                })
            })
            .collect();
        if !kept.is_empty() {
            parts.push(keep_stage(body_vertex, kept));
        }
    }

    Ok(steps::concat(parts))
}

/// One stage holding a `KeepFields` transform at the body vertex.
fn keep_stage(body_vertex: &str, keys: Vec<String>) -> CompiledSteps {
    let body_key = panproto_gat::Name::from(body_vertex);
    let transform = panproto_inst::FieldTransform::KeepFields { keys };
    let stage = steps::CompiledStage {
        chain: panproto_lens::ProtolensChain::new(Vec::new()),
        field_transforms: std::collections::HashMap::from([(
            body_key.clone(),
            vec![transform.clone()],
        )]),
    };
    CompiledSteps {
        chain: panproto_lens::ProtolensChain::new(Vec::new()),
        field_transforms: std::collections::HashMap::from([(body_key, vec![transform])]),
        stages: vec![stage],
    }
}

/// Expand a single rule into its steps, each with the path (relative to
/// the rule) of the member it came from.
fn expand_rule(rule: &Rule) -> Result<Vec<(DocumentPath, Step)>, LensDslError> {
    let root = DocumentPath::root();
    let name = &rule.match_.name;
    let Some(replacement) = &rule.replace else {
        return Ok(vec![(
            root.key("pattern").key("name"),
            Step::DropSort {
                drop_sort: name.clone(),
            },
        )]);
    };
    let replace = root.key("replace");
    let mut steps = Vec::new();

    match &replacement.name {
        Some(ReplacementName::Literal(new)) if new != name => steps.push((
            replace.key("name"),
            Step::RenameSort {
                rename_sort: RenameSpec {
                    old: name.clone(),
                    new: new.clone(),
                },
            },
        )),
        Some(ReplacementName::Template { template }) => {
            let expr = template_to_expr(template).map_err(|e| e.within(&replace.key("name")))?;
            steps.push((
                replace.key("name"),
                Step::ComputeField {
                    compute_field: crate::document::ComputeFieldSpec {
                        target: "name".to_owned(),
                        expr: expr.into(),
                        inverse: None,
                        coercion: None,
                    },
                },
            ));
        }
        Some(ReplacementName::Literal(_)) | None => {}
    }

    for (old, new) in replacement.rename_attrs.iter().flatten() {
        steps.push((
            replace.key("rename_attrs").key(old),
            Step::RenameField {
                rename_field: RenameSpec {
                    old: old.clone(),
                    new: new.clone(),
                },
            },
        ));
    }
    for (index, attr) in replacement.drop_attrs.iter().flatten().enumerate() {
        steps.push((
            replace.key("drop_attrs").index(index),
            Step::RemoveField {
                remove_field: attr.clone(),
            },
        ));
    }
    for (attr, value) in replacement.add_attrs.iter().flatten() {
        steps.push((
            replace.key("add_attrs").key(attr),
            Step::AddField {
                add_field: AddFieldSpec {
                    name: attr.clone(),
                    kind: json_value_kind(value).to_owned(),
                    default: value.clone(),
                    expr: None,
                },
            },
        ));
    }
    for (field, op_spec) in replacement.map_attr_value.iter().flatten() {
        let here = replace.key("map_attr_value").key(field);
        let expr = attr_value_op_to_expr(field, op_spec).ok_or_else(|| {
            LensDslError::RuleCompile {
                message: format!("unsupported or malformed map_attr_value op: {op_spec}"),
            }
            .within(&here)
        })?;
        steps.push((
            here,
            Step::ApplyExpr {
                apply_expr: crate::document::ApplyExprSpec {
                    field: field.clone(),
                    expr: expr.into(),
                    inverse: None,
                    coercion: None,
                },
            },
        ));
    }
    Ok(steps)
}

/// Convert a template string like `"h{level}"` into an expression.
///
/// Interpolated variables are coerced to strings via `int_to_str`.
/// This is appropriate for the primary use case (numeric attributes
/// like heading level). For non-integer variables, the expression
/// produces a type error at evaluation time, which is correct: a
/// non-trivial coercion belongs in an explicit `compute_field`.
///
/// The expression is built as an AST rather than as source text. A
/// template's literal segments are arbitrary user text, so rendering
/// them into source would need escaping, and a template carrying a
/// quote or a backslash would otherwise produce source that means
/// something else or does not parse at all.
///
/// # Errors
///
/// Returns [`LensDslError::RuleCompile`] for an interpolation that is
/// never closed or that names no variable.
fn template_to_expr(template: &str) -> Result<Expr, LensDslError> {
    let mut parts: Vec<Expr> = Vec::new();
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        if open > 0 {
            parts.push(Expr::Lit(Literal::Str(rest[..open].to_owned())));
        }
        let Some(offset) = rest[open..].find('}') else {
            return Err(LensDslError::RuleCompile {
                message: format!(
                    "template {template:?}: an interpolation is opened and never closed"
                ),
            });
        };
        let close = open + offset;
        let var = rest[open + 1..close].trim();
        if var.is_empty() {
            return Err(LensDslError::RuleCompile {
                message: format!("template {template:?}: an interpolation names no variable"),
            });
        }
        parts.push(Expr::int_to_str(Expr::var(var)));
        rest = &rest[close + 1..];
    }

    if !rest.is_empty() {
        parts.push(Expr::Lit(Literal::Str(rest.to_owned())));
    }

    let mut parts = parts.into_iter();
    let Some(first) = parts.next() else {
        // An empty template denotes the empty string.
        return Ok(Expr::Lit(Literal::Str(String::new())));
    };
    Ok(parts.fold(first, |acc, part| {
        Expr::Builtin(BuiltinOp::Concat, vec![acc, part])
    }))
}

/// Build the numeric literal a value-operation operand denotes.
///
/// A whole number stays an integer; anything else becomes a float. The
/// literal carries the value itself, so nothing is lost to formatting it
/// and reading it back.
fn numeric_operand(operand: Option<&serde_json::Value>) -> Option<Literal> {
    let number = operand?.as_number()?;
    number.as_i64().map_or_else(
        || number.as_f64().map(Literal::Float),
        |i| Some(Literal::Int(i)),
    )
}

/// Convert an `attrValueOp` descriptor into an expression.
///
/// Supports the relationaltext operator vocabulary:
/// add, subtract, multiply, prefix, suffix, negate, to-string,
/// to-number, to-boolean.
///
/// The field reference and every operand go into the AST directly, so a
/// field name or a string operand containing a quote, a backslash, or a
/// space names exactly what it says rather than reshaping the
/// expression around it.
fn attr_value_op_to_expr(field: &str, op_spec: &serde_json::Value) -> Option<Expr> {
    let op = op_spec.get("op")?.as_str()?;
    let operand = op_spec.get("value");
    let subject = || Expr::var(field);

    let expr = match op {
        "add" => Expr::Builtin(
            BuiltinOp::Add,
            vec![subject(), Expr::Lit(numeric_operand(operand)?)],
        ),
        "subtract" => Expr::Builtin(
            BuiltinOp::Sub,
            vec![subject(), Expr::Lit(numeric_operand(operand)?)],
        ),
        "multiply" => Expr::Builtin(
            BuiltinOp::Mul,
            vec![subject(), Expr::Lit(numeric_operand(operand)?)],
        ),
        "prefix" => Expr::Builtin(
            BuiltinOp::Concat,
            vec![
                Expr::Lit(Literal::Str(operand?.as_str()?.to_owned())),
                subject(),
            ],
        ),
        "suffix" => Expr::Builtin(
            BuiltinOp::Concat,
            vec![
                subject(),
                Expr::Lit(Literal::Str(operand?.as_str()?.to_owned())),
            ],
        ),
        "negate" => Expr::Builtin(BuiltinOp::Not, vec![subject()]),
        "to-string" => Expr::int_to_str(subject()),
        "to-number" => Expr::str_to_int(subject()),
        "to-boolean" => Expr::Match {
            // Truthy coercion: a non-empty string or a non-zero number.
            scrutinee: Box::new(Expr::Builtin(BuiltinOp::TypeOf, vec![subject()])),
            arms: vec![
                (
                    Pattern::Lit(Literal::Str("string".to_owned())),
                    Expr::Builtin(
                        BuiltinOp::Neq,
                        vec![subject(), Expr::Lit(Literal::Str(String::new()))],
                    ),
                ),
                (
                    Pattern::Lit(Literal::Str("number".to_owned())),
                    Expr::Builtin(BuiltinOp::Neq, vec![subject(), Expr::Lit(Literal::Int(0))]),
                ),
                (Pattern::Wildcard, subject()),
            ],
        },
        _ => return None,
    };
    Some(expr)
}

/// The kind of the `add_field` an `add_attrs` entry expands to, read off
/// its value: `integer` for a number an `i64` holds, `number` for any other
/// number, `boolean`, `string` (also for `null`), `array` or `object`.
fn json_value_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Number(n) if n.as_i64().is_some() => "integer",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Null | serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::document::{FeaturePattern, Replacement};

    const BODY: &str = "record:body";

    fn named_pattern(name: &str) -> FeaturePattern {
        FeaturePattern {
            name: name.to_owned(),
        }
    }

    /// A template's literal text and an operand's string are user data.
    /// Building the expression as an AST means a quote, a backslash, or
    /// a brace in either one names itself instead of reshaping the
    /// expression around it.
    #[test]
    fn quotes_and_backslashes_survive_a_template_and_an_operand() {
        let expr = template_to_expr(r#"say "hi\" {level}"#).unwrap();
        let mut literals = Vec::new();
        collect_literals(&expr, &mut literals);
        assert!(
            literals.contains(&r#"say "hi\" "#.to_owned()),
            "the template's literal text must survive verbatim: {literals:?}",
        );

        let op = serde_json::json!({ "op": "prefix", "value": r#"a"b\c"# });
        let Some(expr) = attr_value_op_to_expr("field", &op) else {
            panic!("prefix is supported");
        };
        let mut literals = Vec::new();
        collect_literals(&expr, &mut literals);
        assert!(
            literals.contains(&r#"a"b\c"#.to_owned()),
            "the operand must survive verbatim: {literals:?}",
        );

        // A field name that is not an identifier still refers to that
        // field rather than to a reshaped expression.
        let op = serde_json::json!({ "op": "to-string" });
        let Some(expr) = attr_value_op_to_expr("data-my attr", &op) else {
            panic!("to-string is supported");
        };
        let mut vars = Vec::new();
        collect_vars(&expr, &mut vars);
        assert_eq!(vars, vec!["data-my attr".to_owned()], "{expr:?}");
    }

    #[test]
    fn a_multiply_operand_keeps_its_exact_value() {
        let op = serde_json::json!({ "op": "multiply", "value": 1.5 });
        let Some(expr) = attr_value_op_to_expr("n", &op) else {
            panic!("multiply is supported");
        };
        let mut floats = Vec::new();
        collect_float_literals(&expr, &mut floats);
        assert_eq!(floats, vec![1.5], "{expr:?}");

        // 2^53 + 1 is the smallest integer an f64 cannot hold, so an
        // operand routed through a float arrives one short.
        let op = serde_json::json!({ "op": "add", "value": 9_007_199_254_740_993_i64 });
        let Some(expr) = attr_value_op_to_expr("n", &op) else {
            panic!("add is supported");
        };
        let mut ints = Vec::new();
        collect_int_literals(&expr, &mut ints);
        assert_eq!(ints, vec![9_007_199_254_740_993_i64], "{expr:?}");
    }

    #[test]
    fn an_unclosed_interpolation_is_refused() {
        match template_to_expr("h{level") {
            Err(LensDslError::RuleCompile { message, .. }) => {
                assert!(message.contains("never closed"), "{message}");
            }
            other => panic!("an unclosed interpolation must be refused, got {other:?}"),
        }
        match template_to_expr("h{}") {
            Err(LensDslError::RuleCompile { message, .. }) => {
                assert!(message.contains("names no variable"), "{message}");
            }
            other => panic!("an empty interpolation must be refused, got {other:?}"),
        }
    }

    fn collect_literals(expr: &Expr, out: &mut Vec<String>) {
        if let Expr::Lit(Literal::Str(s)) = expr {
            out.push(s.clone());
        }
        for child in children(expr) {
            collect_literals(child, out);
        }
    }

    fn collect_float_literals(expr: &Expr, out: &mut Vec<f64>) {
        if let Expr::Lit(Literal::Float(f)) = expr {
            out.push(*f);
        }
        for child in children(expr) {
            collect_float_literals(child, out);
        }
    }

    fn collect_int_literals(expr: &Expr, out: &mut Vec<i64>) {
        if let Expr::Lit(Literal::Int(i)) = expr {
            out.push(*i);
        }
        for child in children(expr) {
            collect_int_literals(child, out);
        }
    }

    fn collect_vars(expr: &Expr, out: &mut Vec<String>) {
        if let Expr::Var(name) = expr {
            let name = name.to_string();
            if !out.contains(&name) {
                out.push(name);
            }
        }
        for child in children(expr) {
            collect_vars(child, out);
        }
    }

    fn children(expr: &Expr) -> Vec<&Expr> {
        match expr {
            Expr::Builtin(_, args) | Expr::List(args) => args.iter().collect(),
            Expr::Lam(_, body) | Expr::Field(body, _) => vec![body],
            Expr::App(f, a) | Expr::Index(f, a) => vec![f, a],
            Expr::Record(fields) => fields.iter().map(|(_, v)| v).collect(),
            Expr::Match { scrutinee, arms } => std::iter::once(&**scrutinee)
                .chain(arms.iter().map(|(_, body)| body))
                .collect(),
            Expr::Let { value, body, .. } => vec![value, body],
            Expr::Var(_) | Expr::Lit(_) => Vec::new(),
        }
    }

    #[test]
    fn rename_rule_expands_to_rename_sort() {
        let rules = vec![Rule {
            match_: named_pattern("heading"),
            replace: Some(Replacement {
                name: Some(ReplacementName::Literal("header".to_owned())),
                rename_attrs: None,
                add_attrs: None,
                drop_attrs: None,
                keep_attrs: None,
                map_attr_value: None,
            }),
        }];
        let compiled = compile_rules(&rules, None, BODY).unwrap();
        assert!(
            !compiled.chain.steps.is_empty(),
            "a rename rule should emit at least one chain step"
        );
    }

    #[test]
    fn drop_rule_with_name_compiles() {
        let rules = vec![Rule {
            match_: named_pattern("obsolete"),
            replace: None,
        }];
        let compiled = compile_rules(&rules, None, BODY).unwrap();
        assert!(!compiled.chain.steps.is_empty());
    }

    #[test]
    fn passthrough_drop_emits_keep_fields() {
        let rules = vec![Rule {
            match_: named_pattern("kept"),
            replace: Some(Replacement {
                name: Some(ReplacementName::Literal("kept".to_owned())),
                rename_attrs: None,
                add_attrs: None,
                drop_attrs: None,
                keep_attrs: None,
                map_attr_value: None,
            }),
        }];
        let compiled = compile_rules(&rules, Some(Passthrough::Drop), BODY).unwrap();
        let key = panproto_gat::Name::from(BODY);
        let transforms = compiled.field_transforms.get(&key).unwrap();
        assert!(
            transforms
                .iter()
                .any(|t| matches!(t, panproto_inst::FieldTransform::KeepFields { keys } if keys.iter().any(|k| k == "kept"))),
            "passthrough=drop should emit a KeepFields transform retaining `kept`"
        );
    }

    #[test]
    fn add_attrs_rule_emits_add_field() {
        let mut add_attrs = BTreeMap::new();
        add_attrs.insert("published".to_owned(), serde_json::Value::Bool(true));
        let rules = vec![Rule {
            match_: named_pattern("post"),
            replace: Some(Replacement {
                name: Some(ReplacementName::Literal("post".to_owned())),
                rename_attrs: None,
                add_attrs: Some(add_attrs),
                drop_attrs: None,
                keep_attrs: None,
                map_attr_value: None,
            }),
        }];
        let compiled = compile_rules(&rules, None, BODY).unwrap();
        // add_attrs expands to an add_field step (schema-level).
        assert!(!compiled.chain.steps.is_empty());
    }
}
