use std::path::Path;

use miette::{Context, IntoDiagnostic as _, Result};
use panproto_core::gat::{self, Term};

use crate::repl::{self, LineResult, ReplConfig};

use super::helpers::load_json;

/// Parse source text through the lexer and parser, returning the AST on success.
fn parse_source(source: &str) -> Result<panproto_expr::Expr> {
    let tokens =
        panproto_expr_parser::tokenize(source).map_err(|e| miette::miette!("lex error: {e}"))?;
    panproto_expr_parser::parse(&tokens).map_err(|errs| {
        let msgs: Vec<String> = errs.iter().map(ToString::to_string).collect();
        miette::miette!("parse error(s):\n{}", msgs.join("\n"))
    })
}

/// Parse a Haskell-style expression and print its AST.
pub fn cmd_expr_parse(source: &str, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Parsing: {source}");
    }
    let expr = parse_source(source)?;
    println!("{expr:#?}");
    Ok(())
}

/// Parse and evaluate a Haskell-style expression, printing the result as JSON.
pub fn cmd_expr_eval_source(source: &str, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Parsing: {source}");
    }
    let expr = parse_source(source)?;
    if verbose {
        eprintln!("AST: {expr:?}");
    }
    let config = panproto_expr::EvalConfig::default();
    let env = panproto_expr::Env::new();
    let result = panproto_expr::eval(&expr, &env, &config)
        .map_err(|e| miette::miette!("eval error: {e}"))?;
    let json = serde_json::to_string_pretty(&result)
        .into_diagnostic()
        .wrap_err("failed to serialize result")?;
    println!("{json}");
    Ok(())
}

/// Parse an expression and pretty-print it back in canonical form.
pub fn cmd_expr_fmt(source: &str, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Parsing: {source}");
    }
    let expr = parse_source(source)?;
    let formatted = panproto_expr_parser::pretty_print(&expr);
    println!("{formatted}");
    Ok(())
}

/// Parse an expression and report any syntax errors.
///
/// Exits with success and a confirmation message when the source is valid.
/// Reports each error on failure.
pub fn cmd_expr_check_source(source: &str, verbose: bool) -> Result<()> {
    if verbose {
        eprintln!("Checking: {source}");
    }
    // Lex phase.
    let tokens = match panproto_expr_parser::tokenize(source) {
        Ok(t) => t,
        Err(e) => {
            println!("lex error: {e}");
            miette::bail!("expression contains syntax errors");
        }
    };
    // Parse phase.
    match panproto_expr_parser::parse(&tokens) {
        Ok(_) => {
            println!("OK");
            Ok(())
        }
        Err(errs) => {
            for e in &errs {
                println!("parse error: {e}");
            }
            miette::bail!("expression contains {} parse error(s)", errs.len());
        }
    }
}

/// Evaluate a GAT expression from a JSON file.
///
/// The file contains a JSON-encoded `Term`. An optional environment can be
/// provided via `--env` (a JSON file mapping variable names to values) or
/// piped through stdin.
pub fn cmd_expr_gat_eval(file: &Path, env_file: Option<&Path>, verbose: bool) -> Result<()> {
    let term: Term = load_json(file)?;

    let env: Vec<(String, gat::ModelValue)> = if let Some(env_path) = env_file {
        load_json(env_path)?
    } else {
        Vec::new()
    };

    if verbose {
        eprintln!("Evaluating expression: {term:?}");
        if !env.is_empty() {
            eprintln!("Environment: {} bindings", env.len());
        }
    }

    let result = eval_term(&term, &env)?;
    let json = serde_json::to_string_pretty(&result)
        .into_diagnostic()
        .wrap_err("failed to serialize result")?;
    println!("{json}");

    Ok(())
}

/// Type-check a GAT expression from a JSON file.
///
/// The file contains a JSON object with `term` and `theory` fields.
/// Verifies the expression is well-formed against the theory.
pub fn cmd_expr_gat_check(file: &Path, verbose: bool) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct CheckInput {
        term: Term,
        theory: gat::Theory,
        #[serde(default)]
        context: Vec<(String, String)>,
    }

    let input: CheckInput = load_json(file)?;

    if verbose {
        eprintln!("Type-checking expression: {:?}", input.term);
        eprintln!("Theory sorts: {}", input.theory.sorts.len());
        eprintln!("Theory operations: {}", input.theory.ops.len());
    }

    let mut ctx = rustc_hash::FxHashMap::default();
    for (var_name, sort_name) in &input.context {
        ctx.insert(
            std::sync::Arc::from(var_name.as_str()),
            gat::SortExpr::Name(std::sync::Arc::from(sort_name.as_str())),
        );
    }

    match gat::typecheck_term(&input.term, &ctx, &input.theory) {
        Ok(sort) => {
            println!("Well-formed. Output sort: {sort}");
            Ok(())
        }
        Err(e) => {
            println!("Type error: {e}");
            miette::bail!("expression type-check failed: {e}");
        }
    }
}

/// Expression keyword set used by the REPL highlighter. Tracks the
/// surface vocabulary the expression-language parser recognises.
const EXPR_KEYWORDS: &[&str] = &[
    "lambda", "fun", "let", "in", "if", "then", "else", "match", "case", "of", "true", "false",
    "null",
];

/// REPL meta-commands recognised by the expression REPL.
const EXPR_COMMANDS: &[&str] = &["let", "env", "quit", "q"];

/// Interactive expression REPL.
///
/// Reads GAT terms (one per line, JSON-encoded), evaluates them, and
/// prints results. Type `:q` or `Ctrl-D` to exit. Uses the shared
/// rustyline driver from `crate::repl` for syntax highlighting,
/// persistent history, and tab completion of meta-command names.
pub fn cmd_expr_repl() -> Result<()> {
    let banner = [
        "panproto expression REPL",
        "Enter JSON-encoded GAT terms. :let <name>=<json> to bind, :env to inspect, :q to quit.",
    ];

    let env: std::cell::RefCell<Vec<(String, gat::ModelValue)>> =
        std::cell::RefCell::new(Vec::new());

    let config = ReplConfig {
        banner: &banner,
        keywords: EXPR_KEYWORDS,
        commands: EXPR_COMMANDS,
        history_file: repl::history_path("expr_history"),
        prompt: Box::new(|| "expr> ".to_owned()),
    };

    repl::run_repl(config, |line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return LineResult::Output(String::new());
        }
        if trimmed == ":q" || trimmed == ":quit" {
            return LineResult::Quit;
        }

        // `:let name = <json>` binds a variable.
        if let Some(rest) = trimmed.strip_prefix(":let ") {
            let Some((name, value_str)) = rest.split_once('=') else {
                return LineResult::Error("usage: :let <name> = <json-value>".to_owned());
            };
            let name = name.trim().to_owned();
            let value_str = value_str.trim();
            match serde_json::from_str::<gat::ModelValue>(value_str) {
                Ok(value) => {
                    let mut env_mut = env.borrow_mut();
                    env_mut.retain(|(k, _)| k != &name);
                    let line = format!("  {name} = {value:?}");
                    env_mut.push((name, value));
                    return LineResult::Output(line);
                }
                Err(e) => return LineResult::Error(format!("parse error: {e}")),
            }
        }

        // `:env` lists all bindings.
        if trimmed == ":env" {
            let env_ref = env.borrow();
            if env_ref.is_empty() {
                return LineResult::Output("  (empty)".to_owned());
            }
            let mut buf = String::new();
            for (k, v) in env_ref.iter() {
                use std::fmt::Write as _;
                let _ = writeln!(buf, "  {k} = {v:?}");
            }
            return LineResult::Output(buf.trim_end().to_owned());
        }

        // Otherwise, parse as a Term and evaluate.
        match serde_json::from_str::<Term>(trimmed) {
            Ok(term) => {
                let env_ref = env.borrow();
                match eval_term(&term, &env_ref) {
                    Ok(result) => {
                        let json = serde_json::to_string_pretty(&result)
                            .unwrap_or_else(|_| format!("{result:?}"));
                        LineResult::Output(format!("  {json}"))
                    }
                    Err(e) => LineResult::Error(format!("error: {e}")),
                }
            }
            Err(e) => LineResult::Error(format!("parse error: {e}")),
        }
    })
}

/// Evaluate a term with a variable environment.
///
/// Variables are resolved from the environment. Operations with no
/// arguments are treated as constants (returning their name as a string).
/// Operations with arguments produce a structured result showing the
/// operation, evaluated arguments, and output sort.
fn eval_term(term: &Term, env: &[(String, gat::ModelValue)]) -> Result<gat::ModelValue> {
    match term {
        Term::Var(name) => env
            .iter()
            .find(|(k, _)| k.as_str() == name.as_ref())
            .map(|(_, v)| v.clone())
            .ok_or_else(|| miette::miette!("unbound variable: {name}")),
        Term::App { op, args } => {
            let mut evaluated = Vec::with_capacity(args.len());
            for arg in args {
                evaluated.push(eval_term(arg, env)?);
            }

            if args.is_empty() {
                // Nullary operation: return the constant name.
                Ok(gat::ModelValue::Str(op.to_string()))
            } else {
                // N-ary operation: return structured result.
                Ok(gat::ModelValue::Map({
                    let mut map = rustc_hash::FxHashMap::default();
                    map.insert("op".to_string(), gat::ModelValue::Str(op.to_string()));
                    map.insert("args".to_string(), gat::ModelValue::List(evaluated));
                    map
                }))
            }
        }
        Term::Case { .. } => Err(miette::miette!(
            "case terms are not yet supported in CLI expression evaluation"
        )),
        Term::Hole { .. } => Err(miette::miette!(
            "typed holes cannot be evaluated; they only carry type information"
        )),
        Term::Let { name, bound, body } => {
            let v = eval_term(bound, env)?;
            let mut extended = env.to_vec();
            extended.push((name.to_string(), v));
            eval_term(body, &extended)
        }
    }
}
