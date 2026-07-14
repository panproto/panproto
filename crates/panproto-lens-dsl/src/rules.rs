//! Compilation of pattern-match rewrite rules to lens steps.
//!
//! Rules are a declarative shorthand for common schema transformations:
//! name remapping, attribute operations, and feature filtering. Each
//! rule is translated into one or more [`Step`]s, which are then
//! compiled via [`compile_steps`](crate::steps::compile_steps).

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
                            expr: template_to_expr(template),
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
                    expr,
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

/// Convert a template string like `"h{level}"` to a panproto expression.
///
/// Interpolated variables are coerced to strings via `int_to_str`.
/// This is appropriate for the primary use case (numeric attributes
/// like heading level). For non-integer variables, the expression
/// will produce a type error at evaluation time, which is correct
/// (the user should use `compute_field` with an explicit expression
/// for non-trivial coercions).
fn template_to_expr(template: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        if open > 0 {
            parts.push(format!("\"{}\"", &rest[..open]));
        }
        let close = rest[open..].find('}').map_or(rest.len(), |c| open + c);
        let var = &rest[open + 1..close];
        parts.push(format!("(int_to_str {var})"));
        rest = if close + 1 < rest.len() {
            &rest[close + 1..]
        } else {
            ""
        };
    }

    if !rest.is_empty() {
        parts.push(format!("\"{rest}\""));
    }

    if parts.len() == 1 {
        parts.into_iter().next().unwrap_or_default()
    } else {
        // Build nested concat calls
        let mut expr = parts.remove(0);
        for part in parts {
            expr = format!("concat {expr} {part}");
        }
        expr
    }
}

/// Convert an `attrValueOp` descriptor to a panproto expression string.
///
/// Supports the relationaltext operator vocabulary:
/// add, subtract, multiply, prefix, suffix, negate, to-string, to-number, to-boolean.
fn attr_value_op_to_expr(field: &str, op_spec: &serde_json::Value) -> Option<String> {
    let op = op_spec.get("op")?.as_str()?;
    let operand = op_spec.get("value");

    let expr = match op {
        "add" => {
            let v = operand?.as_f64()?;
            format!("add {field} {v}")
        }
        "subtract" => {
            let v = operand?.as_f64()?;
            format!("sub {field} {v}")
        }
        "multiply" => {
            let v = operand?.as_f64()?;
            format!("mul {field} {v}")
        }
        "prefix" => {
            let v = operand?.as_str()?;
            format!("concat \"{v}\" {field}")
        }
        "suffix" => {
            let v = operand?.as_str()?;
            format!("concat {field} \"{v}\"")
        }
        "negate" => format!("not {field}"),
        "to-string" => format!("int_to_str {field}"),
        "to-number" => format!("str_to_int {field}"),
        "to-boolean" => {
            // Truthy coercion: non-empty string / non-zero number → true
            format!(
                "case type_of {field} of \"string\" -> neq {field} \"\" | \"number\" -> neq {field} 0 | _ -> {field}"
            )
        }
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
