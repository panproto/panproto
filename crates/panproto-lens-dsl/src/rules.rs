//! Compilation of pattern-match rewrite rules to lens steps.
//!
//! Rules are a declarative shorthand for common schema transformations:
//! name remapping, attribute operations, and feature filtering. Each
//! rule is translated into one or more [`Step`]s, which are then
//! compiled via [`compile_steps`](crate::steps::compile_steps).

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};

use crate::document::{AddFieldSpec, Passthrough, RenameSpec, ReplacementName, Rule, Step};
use crate::error::LensDslError;
use crate::steps::{self, CompiledSteps};

/// Compile a set of rules into a [`CompiledSteps`].
///
/// Rules are expanded into steps, then compiled via the step pipeline.
/// The `body_vertex` is the parent vertex for field operations.
///
/// # Errors
///
/// Returns [`LensDslError::RuleCompile`] for invalid rules, or
/// propagates errors from step compilation.
pub fn compile_rules(
    rules: &[Rule],
    passthrough: Option<Passthrough>,
    body_vertex: &str,
) -> Result<CompiledSteps, LensDslError> {
    let mut expanded_steps: Vec<Step> = Vec::new();

    for (i, rule) in rules.iter().enumerate() {
        expand_rule(rule, i, &mut expanded_steps)?;
    }

    // Collect keep_attrs from all rules (value-level, not schema-level).
    let mut all_keep_attrs: Vec<String> = Vec::new();
    for rule in rules {
        if let Some(rep) = &rule.replace {
            if let Some(keep) = &rep.keep_attrs {
                all_keep_attrs.extend(keep.clone());
            }
        }
    }

    let mut compiled = steps::compile_steps(&expanded_steps, body_vertex)?;

    // Emit KeepFields for per-rule keep_attrs.
    if !all_keep_attrs.is_empty() {
        let body_key = panproto_gat::Name::from(body_vertex);
        compiled.field_transforms.entry(body_key).or_default().push(
            panproto_inst::FieldTransform::KeepFields {
                keys: all_keep_attrs,
            },
        );
    }

    // If passthrough is "drop", emit a KeepFields transform that retains
    // only the features explicitly mentioned in the rules. Unmatched features
    // are filtered out at the value level via FieldTransform::KeepFields.
    if passthrough == Some(Passthrough::Drop) {
        let kept: Vec<String> = rules
            .iter()
            .filter_map(|r| {
                if r.replace.is_some() {
                    r.replace
                        .as_ref()
                        .and_then(|rep| {
                            rep.name.as_ref().map(|n| match n {
                                ReplacementName::Literal(s) => s.clone(),
                                ReplacementName::Template { .. } => {
                                    r.match_.name.clone().unwrap_or_default()
                                }
                            })
                        })
                        .or_else(|| r.match_.name.clone())
                } else {
                    None
                }
            })
            .collect();

        if !kept.is_empty() {
            let body_key = panproto_gat::Name::from(body_vertex);
            compiled
                .field_transforms
                .entry(body_key)
                .or_default()
                .push(panproto_inst::FieldTransform::KeepFields { keys: kept });
        }
    }

    Ok(compiled)
}

/// Expand a single rule into one or more steps.
fn expand_rule(rule: &Rule, index: usize, steps: &mut Vec<Step>) -> Result<(), LensDslError> {
    let match_name = rule.match_.name.as_deref();

    let Some(replacement) = &rule.replace else {
        // replace: null → drop the matched feature
        let Some(name) = match_name else {
            return Err(LensDslError::RuleCompile {
                index,
                message: "drop rule must have a match name".to_owned(),
            });
        };
        steps.push(Step::DropSort {
            drop_sort: name.to_owned(),
        });
        return Ok(());
    };

    // Name remapping
    if let Some(new_name) = &replacement.name {
        if let Some(old_name) = match_name {
            let new = match new_name {
                ReplacementName::Literal(s) => s.clone(),
                ReplacementName::Template { template } => {
                    // Template names are handled as compute_field expressions.
                    // Generate a compute step with string interpolation.
                    steps.push(Step::ComputeField {
                        compute_field: crate::document::ComputeFieldSpec {
                            target: "name".to_owned(),
                            expr: template_to_expr(template)?.into(),
                            inverse: None,
                            coercion: None,
                        },
                    });
                    // Don't add a rename step for templates.
                    expand_attr_ops(replacement, steps)?;
                    return Ok(());
                }
            };

            if old_name != new {
                steps.push(Step::RenameSort {
                    rename_sort: RenameSpec {
                        old: old_name.to_owned(),
                        new,
                    },
                });
            }
        }
    }

    // Attribute operations
    expand_attr_ops(replacement, steps)
}

/// Expand attribute operations from a replacement into steps.
fn expand_attr_ops(
    replacement: &crate::document::Replacement,
    steps: &mut Vec<Step>,
) -> Result<(), LensDslError> {
    // rename_attrs → rename_field per entry
    if let Some(renames) = &replacement.rename_attrs {
        for (old, new) in renames {
            steps.push(Step::RenameField {
                rename_field: RenameSpec {
                    old: old.clone(),
                    new: new.clone(),
                },
            });
        }
    }

    // drop_attrs → remove_field per entry
    if let Some(drops) = &replacement.drop_attrs {
        for attr in drops {
            steps.push(Step::RemoveField {
                remove_field: attr.clone(),
            });
        }
    }

    // add_attrs → add_field per entry
    if let Some(adds) = &replacement.add_attrs {
        for (name, value) in adds {
            let kind = json_value_kind(value);
            steps.push(Step::AddField {
                add_field: AddFieldSpec {
                    name: name.clone(),
                    kind,
                    default: value.clone(),
                    expr: None,
                },
            });
        }
    }

    // map_attr_value → apply_expr per entry
    // Each value is an attrValueOp descriptor with "op" and optional "value".
    if let Some(transforms) = &replacement.map_attr_value {
        for (field, op_spec) in transforms {
            let expr =
                attr_value_op_to_expr(field, op_spec).ok_or_else(|| LensDslError::RuleCompile {
                    index: 0,
                    message: format!(
                        "unsupported or malformed map_attr_value op for field '{field}': {op_spec}"
                    ),
                })?;
            steps.push(Step::ApplyExpr {
                apply_expr: crate::document::ApplyExprSpec {
                    field: field.clone(),
                    expr: expr.into(),
                    inverse: None,
                    coercion: None,
                },
            });
        }
    }

    // keep_attrs is a value-level operation (FieldTransform::KeepFields)
    // and cannot be expressed as a schema-level Step. It is collected
    // by the rules compiler and added to field_transforms directly.
    Ok(())
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
                index: 0,
                message: format!(
                    "template {template:?}: an interpolation is opened and never closed"
                ),
            });
        };
        let close = open + offset;
        let var = rest[open + 1..close].trim();
        if var.is_empty() {
            return Err(LensDslError::RuleCompile {
                index: 0,
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

/// Infer a kind string from a JSON value for `add_field`.
fn json_value_kind(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Number(_) => "integer".to_owned(),
        serde_json::Value::Bool(_) => "boolean".to_owned(),
        _ => "string".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::document::{FeaturePattern, Replacement};

    const BODY: &str = "record:body";

    fn named_pattern(name: &str) -> FeaturePattern {
        FeaturePattern {
            name: Some(name.to_owned()),
            type_id: None,
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
    fn drop_rule_requires_a_match_name() {
        // `replace: None` on a pattern with no name cannot resolve to a
        // sort to drop.
        let rules = vec![Rule {
            match_: FeaturePattern {
                name: None,
                type_id: Some("app.some.type".to_owned()),
            },
            replace: None,
        }];
        let err = compile_rules(&rules, None, BODY).unwrap_err();
        assert!(matches!(err, LensDslError::RuleCompile { .. }));
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
        let mut add_attrs = HashMap::new();
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
