//! Theory DSL CLI commands.

use std::path::Path;

use miette::Result;

/// Validate a theory document (load + typecheck).
pub fn cmd_theory_validate(file: &Path, verbose: bool) -> Result<()> {
    let resolver = panproto_theory_dsl::builtin_resolver();
    let doc = panproto_theory_dsl::load(file).map_err(|e| miette::miette!("{e}"))?;

    if verbose {
        eprintln!("loaded document: {}", doc.id);
    }

    let compiled =
        panproto_theory_dsl::compile(&doc, &resolver).map_err(|e| miette::miette!("{e}"))?;

    eprintln!(
        "valid: {} theories, {} morphisms, {} protocols",
        compiled.theories.len(),
        compiled.morphisms.len(),
        compiled.protocols.len(),
    );
    Ok(())
}

/// Compile a theory document and print resulting theory names as JSON.
pub fn cmd_theory_compile(file: &Path, json: bool, verbose: bool) -> Result<()> {
    let resolver = panproto_theory_dsl::builtin_resolver();
    let doc = panproto_theory_dsl::load(file).map_err(|e| miette::miette!("{e}"))?;
    let compiled =
        panproto_theory_dsl::compile(&doc, &resolver).map_err(|e| miette::miette!("{e}"))?;

    if json {
        let summary = serde_json::json!({
            "id": compiled.id,
            "theories": compiled.theories.keys().collect::<Vec<_>>(),
            "morphisms": compiled.morphisms.keys().collect::<Vec<_>>(),
            "protocols": compiled.protocols.keys().collect::<Vec<_>>(),
            "compositions": compiled.composition_specs.keys().collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|e| miette::miette!("{e}"))?
        );
    } else {
        println!("Document: {}", compiled.id);
        for (name, theory) in &compiled.theories {
            println!(
                "  theory {name}: {} sorts, {} ops, {} eqs",
                theory.sorts.len(),
                theory.ops.len(),
                theory.eqs.len(),
            );
            if verbose {
                for sort in &theory.sorts {
                    println!("    sort {}", sort.name);
                }
                for op in &theory.ops {
                    println!("    op {} : arity {}", op.name, op.arity());
                }
            }
        }
        for name in compiled.morphisms.keys() {
            println!("  morphism {name}");
        }
        for name in compiled.protocols.keys() {
            println!("  protocol {name}");
        }
    }
    Ok(())
}

/// Compile all theory documents in a directory.
pub fn cmd_theory_compile_dir(dir: &Path, verbose: bool) -> Result<()> {
    let result = panproto_theory_dsl::load_dir(dir).map_err(|e| miette::miette!("{e}"))?;
    let resolver = panproto_theory_dsl::builtin_resolver();

    let mut total_theories = 0usize;
    let mut total_morphisms = 0usize;
    let mut total_protocols = 0usize;

    for doc in &result.documents {
        match panproto_theory_dsl::compile(doc, &resolver) {
            Ok(compiled) => {
                total_theories += compiled.theories.len();
                total_morphisms += compiled.morphisms.len();
                total_protocols += compiled.protocols.len();
                if verbose {
                    eprintln!("compiled {}: {:?}", doc.id, compiled);
                }
            }
            Err(e) => {
                eprintln!("error compiling {}: {e}", doc.id);
            }
        }
    }

    for (path, err) in &result.errors {
        eprintln!("error loading {}: {err}", path.display());
    }

    println!(
        "compiled {} documents: {total_theories} theories, {total_morphisms} morphisms, {total_protocols} protocols",
        result.documents.len(),
    );
    Ok(())
}

/// Validate a morphism document.
pub fn cmd_theory_check_morphism(file: &Path, verbose: bool) -> Result<()> {
    let resolver = panproto_theory_dsl::builtin_resolver();
    let doc = panproto_theory_dsl::load(file).map_err(|e| miette::miette!("{e}"))?;

    if verbose {
        eprintln!("loaded document: {}", doc.id);
    }

    let compiled =
        panproto_theory_dsl::compile(&doc, &resolver).map_err(|e| miette::miette!("{e}"))?;

    if compiled.morphisms.is_empty() {
        eprintln!("warning: no morphisms in document");
    } else {
        for name in compiled.morphisms.keys() {
            eprintln!("morphism '{name}' is valid");
        }
    }
    Ok(())
}

/// Replay a composition and print the result.
pub fn cmd_theory_recompose(file: &Path, verbose: bool) -> Result<()> {
    let resolver = panproto_theory_dsl::builtin_resolver();
    let doc = panproto_theory_dsl::load(file).map_err(|e| miette::miette!("{e}"))?;
    let compiled =
        panproto_theory_dsl::compile(&doc, &resolver).map_err(|e| miette::miette!("{e}"))?;

    for (name, theory) in &compiled.theories {
        println!(
            "theory {name}: {} sorts, {} ops, {} eqs",
            theory.sorts.len(),
            theory.ops.len(),
            theory.eqs.len(),
        );
        if verbose {
            for sort in &theory.sorts {
                println!("  sort {}", sort.name);
            }
            for op in &theory.ops {
                println!("  op {} : arity {}", op.name, op.arity());
            }
        }
    }

    if let Some((name, spec)) = compiled.composition_specs.iter().next() {
        println!("composition '{name}': {} steps", spec.steps.len());
    }
    Ok(())
}

/// Run sample-based coercion law checks over every directed
/// equation in every theory compiled from `file`.
///
/// Non-zero exit status on any violation; clean (exit 0) otherwise.
pub fn cmd_theory_check_coercion_laws(file: &Path, json: bool, verbose: bool) -> Result<()> {
    use panproto_core::lens::coercion_laws::{
        CoercionSampleRegistry, TheoryCoercionReport, check_theory,
    };

    let resolver = panproto_theory_dsl::builtin_resolver();
    let doc = panproto_theory_dsl::load(file).map_err(|e| miette::miette!("{e}"))?;
    let compiled =
        panproto_theory_dsl::compile(&doc, &resolver).map_err(|e| miette::miette!("{e}"))?;

    let registry = CoercionSampleRegistry::with_defaults();
    let mut reports: Vec<(String, TheoryCoercionReport)> = Vec::new();
    for (name, theory) in &compiled.theories {
        let report = check_theory(theory, &registry);
        reports.push((name.clone(), report));
    }

    let total_violations: usize = reports.iter().map(|(_, r)| r.violation_count()).sum();
    let clean = total_violations == 0;

    if json {
        let payload = serde_json::json!({
            "document": compiled.id,
            "clean": clean,
            "total_violations": total_violations,
            "theories": reports.iter().map(|(name, report)| {
                serde_json::json!({
                    "name": name,
                    "clean": report.is_clean(),
                    "violation_count": report.violation_count(),
                    "equations": report.per_equation.iter().map(|(eq_name, violations)| {
                        serde_json::json!({
                            "name": eq_name.as_ref(),
                            "violations": violations.iter()
                                .map(|v| format!("{v:?}"))
                                .collect::<Vec<_>>(),
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| miette::miette!("{e}"))?
        );
    } else {
        println!("Document: {}", compiled.id);
        for (name, report) in &reports {
            if report.is_clean() {
                println!(
                    "  theory {name}: clean ({} equations checked)",
                    report.per_equation.len()
                );
                if verbose {
                    for (eq, _) in &report.per_equation {
                        println!("    equation {eq}: ok");
                    }
                }
            } else {
                println!(
                    "  theory {name}: {} violation(s) across {} equation(s)",
                    report.violation_count(),
                    report.per_equation.len(),
                );
                for (eq, violations) in &report.per_equation {
                    if !violations.is_empty() {
                        println!("    equation {eq}: {} violation(s)", violations.len());
                        for v in violations {
                            println!("      {v:?}");
                        }
                    }
                }
            }
        }
        if clean {
            println!("All {} theor(y|ies) clean.", reports.len());
        } else {
            println!("Total violations: {total_violations}");
        }
    }

    if clean {
        Ok(())
    } else {
        miette::bail!(
            "coercion law violation(s) detected: {total_violations} across {} theor(y|ies)",
            reports.iter().filter(|(_, r)| !r.is_clean()).count(),
        );
    }
}
