//! Expression language: parsing, functional evaluation, GAT-term
//! evaluation, type checking, and declarative queries.
//!
//! Ported from `panproto_wasm::api::helpers` (`parse_expr`,
//! `eval_func_expr`, `execute_query`) and `panproto_wasm::api::enriched`
//! (`eval_expr`, `check_expr`): the engine logic is identical, with the
//! WASM `JsError`/`MessagePack` pairing replaced by [`FfiError`] and CBOR
//! via [`crate::canonical`].
//!
//! Surface parsing and functional evaluation go through
//! [`panproto_expr_parser`] and [`panproto_expr`] directly. GAT-term
//! evaluation and type checking go through `panproto_core::gat`; the
//! recursive term evaluator mirrors `eval_term_recursive` in the WASM
//! helpers. Queries go through `panproto_core::inst::execute_query`.

use std::sync::Arc;

use panproto_core::gat::{self, Term, Theory, VarContext};
use panproto_core::inst::{self, WInstance};
use rustc_hash::FxHashMap;
use safer_ffi::prelude::*;
use serde::Serialize;

use crate::error::{FfiError, PpStatus};
use crate::handle;
use crate::panic::guard;

/// Parse expression source text into a `panproto-expr` AST.
///
/// `source` is the UTF-8 source bytes. On success, `out` receives the
/// CBOR-encoded [`panproto_expr::Expr`]. Tokenizes via
/// [`panproto_expr_parser::tokenize`] then parses via
/// [`panproto_expr_parser::parse`]; either failure maps to
/// [`FfiError::Operation`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_parse(source: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let text = std::str::from_utf8(source.as_slice())
            .map_err(|e| FfiError::Serialization(format!("source: invalid UTF-8: {e}")))?;

        let tokens = panproto_expr_parser::tokenize(text)
            .map_err(|e| FfiError::Operation(format!("tokenize failed: {e}")))?;

        let expr = panproto_expr_parser::parse(&tokens).map_err(|errs| {
            FfiError::Operation(format!(
                "parse failed: {}",
                errs.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ))
        })?;

        *out = crate::canonical::encode(&expr)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Evaluate a functional expression against an environment.
///
/// `expr` is a CBOR-encoded [`panproto_expr::Expr`]; `env` is a
/// CBOR-encoded `Vec<(String, panproto_expr::Literal)>`. On success,
/// `out` receives the CBOR-encoded [`panproto_expr::Literal`] result.
/// Calls [`panproto_expr::eval`] with the default
/// [`panproto_expr::EvalConfig`] (step and depth limits).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_eval_func(
    expr: c_slice::Ref<'_, u8>,
    env: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let expr: panproto_expr::Expr = crate::canonical::decode(expr.as_slice())?;
        let bindings: Vec<(String, panproto_expr::Literal)> =
            crate::canonical::decode(env.as_slice())?;

        let env: panproto_expr::Env = bindings
            .into_iter()
            .map(|(k, v)| (Arc::<str>::from(k.as_str()), v))
            .collect();

        let config = panproto_expr::EvalConfig::default();
        let result = panproto_expr::eval(&expr, &env, &config)
            .map_err(|e| FfiError::Operation(format!("expression evaluation failed: {e}")))?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Evaluate a GAT term against a theory and a variable environment.
///
/// `expr` is a CBOR-encoded [`gat::Term`]; `env` is a CBOR-encoded
/// `Vec<(String, gat::ModelValue)>`; `theory` is a
/// [`Resource::Theory`](crate::handle::Resource) handle. On success,
/// `out` receives the CBOR-encoded [`gat::ModelValue`] result.
///
/// The recursive evaluator mirrors `eval_term_recursive` in the WASM
/// helpers: variables resolve against the environment, applications
/// evaluate their arguments and consult the theory's operation table,
/// nullary constants reduce to their name as a string, and other
/// applications produce a structured `{ op, args, output_sort }` map.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_eval_gat(
    expr: c_slice::Ref<'_, u8>,
    env: c_slice::Ref<'_, u8>,
    theory: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let term: Term = crate::canonical::decode(expr.as_slice())?;
        let bindings: Vec<(String, gat::ModelValue)> = crate::canonical::decode(env.as_slice())?;

        let theory = handle::with_resource(theory, |r| Ok(r.as_theory()?.clone()))?;

        let result = eval_term_recursive(&term, &bindings, &theory)
            .map_err(|e| FfiError::Operation(format!("term evaluation failed: {e}")))?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Type-check a GAT term against a theory and a typing context.
///
/// `expr` is a CBOR-encoded [`gat::Term`]; `theory` is a
/// [`Resource::Theory`](crate::handle::Resource) handle; `context` is a
/// CBOR-encoded `Vec<(String, String)>` mapping variable names to sort
/// names. On success, `out` receives a CBOR-encoded `CheckOutput`
/// record (`well_formed`, `output_sort`, `error`).
///
/// The result itself encodes well-formedness, so the entry point returns
/// [`PpStatus::Ok`] for both a well-formed and an ill-formed term; only a
/// malformed payload or a bad handle yields a non-`Ok` status. Calls
/// [`gat::typecheck_term`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_expr_check(
    expr: c_slice::Ref<'_, u8>,
    theory: u32,
    context: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let term: Term = crate::canonical::decode(expr.as_slice())?;
        let context: Vec<(String, String)> = crate::canonical::decode(context.as_slice())?;

        let theory = handle::with_resource(theory, |r| Ok(r.as_theory()?.clone()))?;

        let mut ctx: VarContext = FxHashMap::default();
        for (var_name, sort_name) in &context {
            ctx.insert(
                Arc::from(var_name.as_str()),
                gat::SortExpr::Name(Arc::from(sort_name.as_str())),
            );
        }

        let result = match gat::typecheck_term(&term, &ctx, &theory) {
            Ok(sort) => CheckOutput {
                well_formed: true,
                output_sort: Some(sort.to_string()),
                error: None,
            },
            Err(e) => CheckOutput {
                well_formed: false,
                output_sort: None,
                error: Some(e.to_string()),
            },
        };

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Execute a declarative query against a W-type instance.
///
/// `query` is a CBOR-encoded [`inst::InstanceQuery`]; `instance` is a
/// CBOR-encoded [`WInstance`]; `schema_handle` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded list of match records, each a map with
/// `node_id`, `anchor`, `value`, and `fields`. Calls
/// [`inst::execute_query`].
///
/// The schema handle must resolve to a
/// [`Resource::Schema`](crate::handle::Resource); a bad handle yields
/// [`PpStatus::InvalidHandle`] and a wrong resource type yields
/// [`PpStatus::TypeMismatch`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_query_execute(
    query: c_slice::Ref<'_, u8>,
    instance: c_slice::Ref<'_, u8>,
    schema_handle: u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let query: inst::InstanceQuery = crate::canonical::decode(query.as_slice())?;
        let instance: WInstance = crate::canonical::decode(instance.as_slice())?;

        // The anchoring schema must be a live schema handle.
        let schema = handle::with_resource(schema_handle, |r| Ok(r.as_schema()?.clone()))?;

        let matches = inst::execute_query(&query, &instance, &schema);

        let results: Vec<serde_json::Value> = matches
            .into_iter()
            .map(|m| {
                let fields: serde_json::Map<String, serde_json::Value> = m
                    .fields
                    .into_iter()
                    .map(|(k, v)| {
                        let json_v = serde_json::to_value(&v).unwrap_or(serde_json::Value::Null);
                        (k, json_v)
                    })
                    .collect();
                serde_json::json!({
                    "node_id": m.node_id,
                    "anchor": m.anchor.as_ref(),
                    "value": serde_json::to_value(&m.value).unwrap_or(serde_json::Value::Null),
                    "fields": fields,
                })
            })
            .collect();

        *out = crate::canonical::encode(&results)?.into();
        Ok(PpStatus::Ok)
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// CBOR payload for [`pp_expr_check`]: the type-checking verdict
/// (mirrors the WASM `CheckOutput` inline struct).
#[derive(Serialize)]
struct CheckOutput {
    well_formed: bool,
    output_sort: Option<String>,
    error: Option<String>,
}

/// Evaluate a GAT term recursively using a variable environment and
/// theory.
///
/// Ports `eval_term_recursive` from the WASM helpers
/// (`crates/panproto-wasm/src/api/helpers.rs`). Variables resolve against
/// the environment; applications evaluate arguments and consult the
/// theory's operation table; nullary constants reduce to their name as a
/// string; `let` extends the environment; `case` reduces the scrutinee
/// to a constructor and evaluates the matching branch; `hole` terms are
/// not evaluable (they carry type information only).
///
/// The symbolic term representation is the convention this evaluator
/// already uses for applications: a nullary constructor `c` reduces to
/// `Str("c")`, and an applied constructor `f(a, b)` reduces to a `Map`
/// with keys `op` (the constructor name) and `args` (the evaluated
/// arguments). `Case` matching reads that representation back: it
/// extracts the constructor name and its arguments, finds the branch
/// whose constructor matches, binds the branch's binders to the
/// constructor arguments, and evaluates the chosen branch's body. This
/// mirrors the `Term::Case` semantics in
/// `panproto_gat::check_model::eval_term`.
fn eval_term_recursive(
    term: &Term,
    env: &[(String, gat::ModelValue)],
    theory: &Theory,
) -> Result<gat::ModelValue, String> {
    match term {
        Term::Var(name) => env
            .iter()
            .find(|(k, _)| k.as_str() == name.as_ref())
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("unbound variable: {name}")),
        Term::App { op, args } => {
            let mut evaluated_args = Vec::with_capacity(args.len());
            for arg in args {
                evaluated_args.push(eval_term_recursive(arg, env, theory)?);
            }

            let operation = theory
                .find_op(op)
                .ok_or_else(|| format!("unknown operation: {op}"))?;

            // Nullary constants reduce to their name as a string value.
            if operation.inputs.is_empty() && args.is_empty() {
                return Ok(gat::ModelValue::Str(op.to_string()));
            }

            // Operations with arguments produce a structured result: in
            // the absence of a concrete model, the value records the
            // operation name, its evaluated arguments, and the output
            // sort.
            Ok(gat::ModelValue::Map({
                let mut map = FxHashMap::default();
                map.insert("op".to_string(), gat::ModelValue::Str(op.to_string()));
                map.insert("args".to_string(), gat::ModelValue::List(evaluated_args));
                map.insert(
                    "output_sort".to_string(),
                    gat::ModelValue::Str(operation.output.to_string()),
                );
                map
            }))
        }
        Term::Case {
            scrutinee,
            branches,
        } => {
            let value = eval_term_recursive(scrutinee, env, theory)?;
            let (constructor, ctor_args) = constructor_of(&value)?;

            let branch = branches
                .iter()
                .find(|b| b.constructor.as_ref() == constructor.as_str())
                .ok_or_else(|| format!("no case branch matches constructor '{constructor}'"))?;

            if branch.binders.len() != ctor_args.len() {
                return Err(format!(
                    "case branch '{constructor}' binds {} names but the constructor \
                     carries {} arguments",
                    branch.binders.len(),
                    ctor_args.len()
                ));
            }

            // Bind each branch binder to the corresponding constructor
            // argument, then evaluate the branch body in the extended
            // environment.
            let mut extended: Vec<(String, gat::ModelValue)> = env.to_vec();
            for (binder, arg) in branch.binders.iter().zip(ctor_args) {
                extended.push((binder.to_string(), arg));
            }
            eval_term_recursive(&branch.body, &extended, theory)
        }
        Term::Hole { .. } => {
            Err("typed holes cannot be evaluated; they only carry type information".to_string())
        }
        Term::Let { name, bound, body } => {
            let v = eval_term_recursive(bound, env, theory)?;
            let mut extended: Vec<(String, gat::ModelValue)> = env.to_vec();
            extended.push((name.to_string(), v));
            eval_term_recursive(body, &extended, theory)
        }
    }
}

/// Extract the constructor name and arguments a [`gat::ModelValue`]
/// represents, in the symbolic-term convention this evaluator produces.
///
/// A nullary constructor reduces to `Str(name)` (no arguments); an
/// applied constructor reduces to a `Map { op: Str(name), args: List }`.
/// Any other value does not name a constructor and therefore cannot be
/// case-analysed.
fn constructor_of(value: &gat::ModelValue) -> Result<(String, Vec<gat::ModelValue>), String> {
    match value {
        gat::ModelValue::Str(name) => Ok((name.clone(), Vec::new())),
        gat::ModelValue::Map(map) => {
            let Some(gat::ModelValue::Str(name)) = map.get("op") else {
                return Err("case scrutinee did not reduce to a matchable constructor".to_string());
            };
            let args = match map.get("args") {
                Some(gat::ModelValue::List(items)) => items.clone(),
                _ => Vec::new(),
            };
            Ok((name.clone(), args))
        }
        other => Err(format!(
            "case scrutinee reduced to {other:?}, which does not name a constructor"
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use panproto_core::gat::{ModelValue, Operation, Sort, Theory};
    use panproto_core::inst::InstanceQuery;
    use panproto_core::schema::{Schema, SchemaBuilder};

    use super::*;
    use crate::api::instance::pp_inst_json_to_instance;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::{decode, encode};
    use crate::handle::Resource;

    fn slice_of(bytes: Vec<u8>) -> c_slice::Box<u8> {
        bytes.into_boxed_slice().into()
    }

    /// A `post` record carrying a string `text` property.
    fn post_schema() -> Schema {
        let proto = crate::api::helpers::default_protocol("expr-test");
        SchemaBuilder::new(&proto)
            .vertex("post", "record", None)
            .unwrap()
            .vertex("text", "string", None)
            .unwrap()
            .edge("post", "text", "prop", Some("text"))
            .unwrap()
            .build()
            .unwrap()
    }

    /// A two-sort theory `{ A, B }` with a nullary constant `c : A` and a
    /// unary op `f : A -> B`.
    fn theory_ab() -> Theory {
        Theory::new(
            "ThAB",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![
                Operation::nullary("c", "A"),
                Operation::unary("f", "x", "A", "B"),
            ],
            vec![],
        )
    }

    // ── pp_expr_parse + pp_expr_eval_func ──────────────────────────────

    #[test]
    fn parse_then_eval_func_adds() {
        // Parse `1 + 2`, then evaluate it against an empty environment.
        let mut parsed: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_expr_parse(slice_of(b"1 + 2".to_vec()).as_ref(), &mut parsed);
        assert_eq!(status, PpStatus::Ok as i32);

        // The bytes round-trip back to an `Expr`.
        let expr: panproto_expr::Expr = decode(&parsed).unwrap();

        let expr_bytes = encode(&expr).unwrap();
        let env_bytes = encode(&Vec::<(String, panproto_expr::Literal)>::new()).unwrap();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_expr_eval_func(
            slice_of(expr_bytes).as_ref(),
            slice_of(env_bytes).as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let result: panproto_expr::Literal = decode(&out).unwrap();
        assert_eq!(result, panproto_expr::Literal::Int(3));

        pp_buf_free(parsed);
        pp_buf_free(out);
    }

    #[test]
    fn eval_func_resolves_variable_from_env() {
        // `x * 2` with `x = 21` evaluates to 42.
        let mut parsed: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_expr_parse(slice_of(b"x * 2".to_vec()).as_ref(), &mut parsed),
            PpStatus::Ok as i32
        );
        let expr: panproto_expr::Expr = decode(&parsed).unwrap();
        pp_buf_free(parsed);

        let env: Vec<(String, panproto_expr::Literal)> =
            vec![("x".to_string(), panproto_expr::Literal::Int(21))];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_expr_eval_func(
                slice_of(encode(&expr).unwrap()).as_ref(),
                slice_of(encode(&env).unwrap()).as_ref(),
                &mut out,
            ),
            PpStatus::Ok as i32
        );
        let result: panproto_expr::Literal = decode(&out).unwrap();
        assert_eq!(result, panproto_expr::Literal::Int(42));
        pp_buf_free(out);
    }

    #[test]
    fn parse_rejects_garbage_source_with_operation_status() {
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        // An unterminated string is a lexer error.
        let status = pp_expr_parse(slice_of(b"\"unterminated".to_vec()).as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
    }

    #[test]
    fn eval_func_rejects_garbage_expr_with_serialization_status() {
        let bad = vec![0xFFu8, 0xFE, 0xFD];
        let env_bytes = encode(&Vec::<(String, panproto_expr::Literal)>::new()).unwrap();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_expr_eval_func(
            slice_of(bad).as_ref(),
            slice_of(env_bytes).as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Serialization as i32);
    }

    // ── pp_expr_eval_gat ───────────────────────────────────────────────

    #[test]
    fn eval_gat_nullary_constant_reduces_to_name() {
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_ab())));

        // The term `c` (a nullary constant) reduces to the string "c".
        let term = Term::App {
            op: Arc::from("c"),
            args: vec![],
        };
        let env: Vec<(String, ModelValue)> = vec![];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_expr_eval_gat(
            slice_of(encode(&term).unwrap()).as_ref(),
            slice_of(encode(&env).unwrap()).as_ref(),
            theory_h,
            &mut out,
        );
        assert_eq!(status, PpStatus::Ok as i32);

        let result: ModelValue = decode(&out).unwrap();
        assert_eq!(result, ModelValue::Str("c".to_string()));

        pp_buf_free(out);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn eval_gat_unary_op_builds_structured_value() {
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_ab())));

        // `f(a)` with `a` bound to the string "hi" yields a structured
        // map { op: "f", args: ["hi"], output_sort: "B" }.
        let term = Term::App {
            op: Arc::from("f"),
            args: vec![Term::Var(Arc::from("a"))],
        };
        let env: Vec<(String, ModelValue)> =
            vec![("a".to_string(), ModelValue::Str("hi".to_string()))];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_expr_eval_gat(
                slice_of(encode(&term).unwrap()).as_ref(),
                slice_of(encode(&env).unwrap()).as_ref(),
                theory_h,
                &mut out,
            ),
            PpStatus::Ok as i32
        );

        let result: ModelValue = decode(&out).unwrap();
        match result {
            ModelValue::Map(m) => {
                assert_eq!(m.get("op"), Some(&ModelValue::Str("f".to_string())));
                assert_eq!(
                    m.get("output_sort"),
                    Some(&ModelValue::Str("B".to_string()))
                );
                assert_eq!(
                    m.get("args"),
                    Some(&ModelValue::List(vec![ModelValue::Str("hi".to_string())]))
                );
            }
            other => panic!("expected a Map, got {other:?}"),
        }

        pp_buf_free(out);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn eval_gat_unbound_variable_is_operation_error() {
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_ab())));
        let term = Term::Var(Arc::from("missing"));
        let env: Vec<(String, ModelValue)> = vec![];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_expr_eval_gat(
            slice_of(encode(&term).unwrap()).as_ref(),
            slice_of(encode(&env).unwrap()).as_ref(),
            theory_h,
            &mut out,
        );
        assert_eq!(status, PpStatus::Operation as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn eval_gat_rejects_non_theory_handle() {
        // A schema handle passed where a theory is expected is a mismatch.
        let schema_h = handle::alloc(Resource::Schema(Arc::new(post_schema())));
        let term = Term::Var(Arc::from("x"));
        let env: Vec<(String, ModelValue)> = vec![("x".to_string(), ModelValue::Int(1))];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_expr_eval_gat(
            slice_of(encode(&term).unwrap()).as_ref(),
            slice_of(encode(&env).unwrap()).as_ref(),
            schema_h,
            &mut out,
        );
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    // ── pp_expr_check ──────────────────────────────────────────────────

    #[test]
    fn check_well_formed_op_reports_output_sort() {
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_ab())));

        // `f(x)` with `x : A` typechecks, producing output sort `B`.
        let term = Term::App {
            op: Arc::from("f"),
            args: vec![Term::Var(Arc::from("x"))],
        };
        let context: Vec<(String, String)> = vec![("x".to_string(), "A".to_string())];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_expr_check(
                slice_of(encode(&term).unwrap()).as_ref(),
                theory_h,
                slice_of(encode(&context).unwrap()).as_ref(),
                &mut out,
            ),
            PpStatus::Ok as i32
        );

        let value: serde_json::Value = decode(&out).unwrap();
        assert_eq!(value["well_formed"], serde_json::json!(true));
        assert_eq!(value["output_sort"], serde_json::json!("B"));
        assert_eq!(value["error"], serde_json::Value::Null);

        pp_buf_free(out);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn check_ill_formed_term_reports_error() {
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_ab())));

        // `f(x)` with `x : B` is ill-typed (`f` wants `A`).
        let term = Term::App {
            op: Arc::from("f"),
            args: vec![Term::Var(Arc::from("x"))],
        };
        let context: Vec<(String, String)> = vec![("x".to_string(), "B".to_string())];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        // An ill-formed term is still an OK call; the verdict lives in the
        // payload.
        assert_eq!(
            pp_expr_check(
                slice_of(encode(&term).unwrap()).as_ref(),
                theory_h,
                slice_of(encode(&context).unwrap()).as_ref(),
                &mut out,
            ),
            PpStatus::Ok as i32
        );

        let value: serde_json::Value = decode(&out).unwrap();
        assert_eq!(value["well_formed"], serde_json::json!(false));
        assert_eq!(value["output_sort"], serde_json::Value::Null);
        assert!(value["error"].is_string(), "expected an error string");

        pp_buf_free(out);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    // ── pp_query_execute ───────────────────────────────────────────────

    #[test]
    fn query_matches_anchor_nodes() {
        let schema_h = handle::alloc(Resource::Schema(Arc::new(post_schema())));

        // Build a real instance via the json_to_instance entry point.
        let mut inst_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_inst_json_to_instance(
                schema_h,
                slice_of(br#"{"text": "hello"}"#.to_vec()).as_ref(),
                slice_of(b"post".to_vec()).as_ref(),
                &mut inst_out,
            ),
            PpStatus::Ok as i32
        );
        let instance_bytes = inst_out.to_vec();
        pp_buf_free(inst_out);

        // Query for `post`-anchored nodes.
        let query = InstanceQuery {
            anchor: panproto_core::gat::Name::from("post"),
            ..InstanceQuery::default()
        };

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_query_execute(
                slice_of(encode(&query).unwrap()).as_ref(),
                slice_of(instance_bytes).as_ref(),
                schema_h,
                &mut out,
            ),
            PpStatus::Ok as i32
        );

        let results: Vec<serde_json::Value> = decode(&out).unwrap();
        assert_eq!(results.len(), 1, "expected one post node, got {results:?}");
        assert_eq!(results[0]["anchor"], serde_json::json!("post"));

        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);
    }

    #[test]
    fn query_rejects_bad_schema_handle() {
        // The query engine requires a live schema handle. u32::MAX is
        // never a live handle, so the call reports InvalidHandle rather
        // than fabricating a schema.
        let schema_h = handle::alloc(Resource::Schema(Arc::new(post_schema())));
        let mut inst_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_inst_json_to_instance(
                schema_h,
                slice_of(br#"{"text": "hi"}"#.to_vec()).as_ref(),
                slice_of(b"post".to_vec()).as_ref(),
                &mut inst_out,
            ),
            PpStatus::Ok as i32
        );
        let instance_bytes = inst_out.to_vec();
        pp_buf_free(inst_out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);

        let query = InstanceQuery {
            anchor: panproto_core::gat::Name::from("post"),
            ..InstanceQuery::default()
        };

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_query_execute(
            slice_of(encode(&query).unwrap()).as_ref(),
            slice_of(instance_bytes).as_ref(),
            u32::MAX,
            &mut out,
        );
        assert_eq!(status, PpStatus::InvalidHandle as i32);
    }

    // ── pp_expr_eval_gat: Case ─────────────────────────────────────────

    /// A `Bool` theory with a closed sort `Bool` whose constructors are
    /// `t` and `f`, plus a unary `not : Bool -> Bool`. The closed sort
    /// makes `t`/`f` the complete constructor set for case analysis.
    fn theory_bool() -> Theory {
        use panproto_core::gat::Sort;
        Theory::new(
            "Bool",
            vec![Sort::closed("Bool", vec![], ["t", "f"])],
            vec![
                Operation::nullary("t", "Bool"),
                Operation::nullary("f", "Bool"),
                Operation::unary("not", "x", "Bool", "Bool"),
            ],
            vec![],
        )
    }

    #[test]
    fn eval_gat_case_selects_matching_branch() {
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_bool())));

        // case t of t => f | f => t end. The scrutinee `t` reduces to
        // Str("t"), matching the first branch, whose body `f` reduces to
        // Str("f").
        let term = Term::Case {
            scrutinee: Box::new(Term::App {
                op: Arc::from("t"),
                args: vec![],
            }),
            branches: vec![
                panproto_core::gat::CaseBranch {
                    constructor: Arc::from("t"),
                    binders: vec![],
                    body: Term::App {
                        op: Arc::from("f"),
                        args: vec![],
                    },
                },
                panproto_core::gat::CaseBranch {
                    constructor: Arc::from("f"),
                    binders: vec![],
                    body: Term::App {
                        op: Arc::from("t"),
                        args: vec![],
                    },
                },
            ],
        };
        let env: Vec<(String, ModelValue)> = vec![];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_expr_eval_gat(
                slice_of(encode(&term).unwrap()).as_ref(),
                slice_of(encode(&env).unwrap()).as_ref(),
                theory_h,
                &mut out,
            ),
            PpStatus::Ok as i32
        );
        let result: ModelValue = decode(&out).unwrap();
        assert_eq!(result, ModelValue::Str("f".to_string()));
        pp_buf_free(out);

        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn eval_gat_case_binds_constructor_arguments() {
        // A scrutinee that is an applied constructor `not(t)` reduces to a
        // structured Map; the matching branch binds the single argument
        // and returns it, so the result is the evaluated argument t.
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_bool())));

        let term = Term::Case {
            scrutinee: Box::new(Term::App {
                op: Arc::from("not"),
                args: vec![Term::App {
                    op: Arc::from("t"),
                    args: vec![],
                }],
            }),
            branches: vec![panproto_core::gat::CaseBranch {
                constructor: Arc::from("not"),
                binders: vec![Arc::from("inner")],
                body: Term::Var(Arc::from("inner")),
            }],
        };
        let env: Vec<(String, ModelValue)> = vec![];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_expr_eval_gat(
                slice_of(encode(&term).unwrap()).as_ref(),
                slice_of(encode(&env).unwrap()).as_ref(),
                theory_h,
                &mut out,
            ),
            PpStatus::Ok as i32
        );
        let result: ModelValue = decode(&out).unwrap();
        // not(t) binds `inner` to the evaluated argument t = Str("t").
        assert_eq!(result, ModelValue::Str("t".to_string()));
        pp_buf_free(out);

        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }

    #[test]
    fn eval_gat_case_no_matching_branch_is_error() {
        let theory_h = handle::alloc(Resource::Theory(Box::new(theory_bool())));

        // Scrutinee reduces to Str("t") but the only branch matches `f`.
        let term = Term::Case {
            scrutinee: Box::new(Term::App {
                op: Arc::from("t"),
                args: vec![],
            }),
            branches: vec![panproto_core::gat::CaseBranch {
                constructor: Arc::from("f"),
                binders: vec![],
                body: Term::App {
                    op: Arc::from("t"),
                    args: vec![],
                },
            }],
        };
        let env: Vec<(String, ModelValue)> = vec![];

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_expr_eval_gat(
            slice_of(encode(&term).unwrap()).as_ref(),
            slice_of(encode(&env).unwrap()).as_ref(),
            theory_h,
            &mut out,
        );
        assert_eq!(status, PpStatus::Operation as i32);
        assert_eq!(pp_handle_free(theory_h), PpStatus::Ok as i32);
    }
}
