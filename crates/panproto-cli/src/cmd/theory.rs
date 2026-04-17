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
