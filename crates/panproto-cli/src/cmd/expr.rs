use std::io::{self, BufRead, Write as _};
use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::gat::{self, Term};

use super::helpers::load_json;

/// Evaluate a GAT expression from a JSON file.
///
/// The file contains a JSON-encoded `Term`. An optional environment can be
/// provided via `--env` (a JSON file mapping variable names to values) or
/// piped through stdin.
pub fn cmd_expr_eval(file: &Path, env_file: Option<&Path>, verbose: bool) -> Result<()> {
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
pub fn cmd_expr_check(file: &Path, verbose: bool) -> Result<()> {
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
            std::sync::Arc::from(sort_name.as_str()),
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

/// Interactive expression REPL.
///
/// Reads GAT terms from stdin (one per line, JSON-encoded), evaluates them,
/// and prints results. Type `:q` or `Ctrl-D` to exit.
pub fn cmd_expr_repl() -> Result<()> {
    println!("panproto expression REPL");
    println!("Enter JSON-encoded GAT terms. Type :q to exit.\n");

    let stdin = io::stdin();
    let mut env: Vec<(String, gat::ModelValue)> = Vec::new();

    loop {
        print!("expr> ");
        io::stdout().flush().into_diagnostic()?;

        let mut line = String::new();
        let bytes_read = stdin.lock().read_line(&mut line).into_diagnostic()?;
        if bytes_read == 0 {
            // EOF
            println!();
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == ":q" || trimmed == ":quit" {
            break;
        }

        // Handle `:let x = <value>` for binding variables.
        if let Some(rest) = trimmed.strip_prefix(":let ") {
            if let Some((name, value_str)) = rest.split_once('=') {
                let name = name.trim().to_string();
                let value_str = value_str.trim();
                match serde_json::from_str::<gat::ModelValue>(value_str) {
                    Ok(value) => {
                        // Remove any existing binding with same name.
                        env.retain(|(k, _)| k != &name);
                        println!("  {name} = {value:?}");
                        env.push((name, value));
                    }
                    Err(e) => {
                        println!("  parse error: {e}");
                    }
                }
                continue;
            }
            println!("  usage: :let <name> = <json-value>");
            continue;
        }

        // Handle `:env` to show current environment.
        if trimmed == ":env" {
            if env.is_empty() {
                println!("  (empty)");
            } else {
                for (k, v) in &env {
                    println!("  {k} = {v:?}");
                }
            }
            continue;
        }

        // Try parsing as a Term and evaluate.
        match serde_json::from_str::<Term>(trimmed) {
            Ok(term) => match eval_term(&term, &env) {
                Ok(result) => {
                    let json = serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_| format!("{result:?}"));
                    println!("  {json}");
                }
                Err(e) => {
                    println!("  error: {e}");
                }
            },
            Err(e) => {
                println!("  parse error: {e}");
            }
        }
    }

    Ok(())
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
    }
}
