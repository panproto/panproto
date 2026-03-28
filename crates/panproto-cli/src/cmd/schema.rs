use std::collections::HashMap;
use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    gat::Name,
    inst,
    mig::{self, Migration},
    schema::Schema,
    vcs::{self, Store as _},
};

use super::helpers::{
    build_schema_model, build_theory_registry, load_json, open_repo, print_stored_theory_diff,
    print_theory_diff, resolve_protocol,
};
use crate::format;

pub fn cmd_validate(protocol_name: &str, schema_path: &Path, verbose: bool) -> Result<()> {
    let schema: Schema = load_json(schema_path)?;
    let protocol = resolve_protocol(protocol_name)?;

    if verbose {
        eprintln!(
            "Validating schema ({} vertices, {} edges) against protocol '{}'",
            schema.vertex_count(),
            schema.edge_count(),
            protocol_name
        );
    }

    let errors = panproto_core::schema::validate(&schema, &protocol);

    if !errors.is_empty() {
        println!("Validation found {} error(s):", errors.len());
        for (i, err) in errors.iter().enumerate() {
            println!("  {}. {err}", i + 1);
        }
        miette::bail!("schema validation failed with {} error(s)", errors.len());
    }

    // Also type-check the protocol's theory equations.
    let theory_registry = build_theory_registry(protocol_name)?;
    let mut type_errors = Vec::new();
    for (name, theory) in &theory_registry {
        let diag = vcs::gat_validate::validate_theory_equations(theory);
        if diag.has_errors() {
            for e in diag.all_errors() {
                type_errors.push(format!("theory '{name}': {e}"));
            }
        }
    }

    if type_errors.is_empty() {
        println!("Schema is valid. Theory type-check: OK.");
    } else {
        println!("Schema is valid but theory type-check found issues:");
        for e in &type_errors {
            println!("  {e}");
        }
    }

    Ok(())
}

pub fn cmd_check(
    src_path: &Path,
    tgt_path: &Path,
    mapping_path: &Path,
    verbose: bool,
    typecheck: bool,
) -> Result<()> {
    let src_schema: Schema = load_json(src_path)?;
    let tgt_schema: Schema = load_json(tgt_path)?;
    let migration: Migration = load_json(mapping_path)?;

    if verbose {
        eprintln!(
            "Checking migration: {} vertices -> {} vertices",
            src_schema.vertex_count(),
            tgt_schema.vertex_count()
        );
    }

    let protocol = resolve_protocol(&src_schema.protocol)?;
    let theory_registry = build_theory_registry(&src_schema.protocol)?;

    let report = mig::check_existence(
        &protocol,
        &src_schema,
        &tgt_schema,
        &migration,
        &theory_registry,
    );

    if report.valid {
        println!("Migration is valid. All existence conditions satisfied.");
    } else {
        println!("Migration check found {} error(s):", report.errors.len());
        for (i, err) in report.errors.iter().enumerate() {
            println!("  {}. {err}", i + 1);
        }
    }

    // GAT-level type-checking of the migration morphism.
    if typecheck {
        let diag = vcs::gat_validate::validate_migration(&src_schema, &tgt_schema, &migration);
        if diag.is_clean() && diag.migration_warnings.is_empty() {
            println!("Migration type-check: OK");
        } else {
            for w in &diag.migration_warnings {
                println!("  warning: {w}");
            }
            for e in &diag.all_errors() {
                println!("  error: {e}");
            }
            if diag.has_errors() {
                miette::bail!("migration type-check failed");
            }
        }
    }

    let json = serde_json::to_string_pretty(&report)
        .into_diagnostic()
        .wrap_err("failed to serialize report")?;
    if verbose {
        eprintln!("---\n{json}");
    }

    if report.valid {
        Ok(())
    } else {
        miette::bail!(
            "migration check failed with {} error(s)",
            report.errors.len()
        );
    }
}

pub fn cmd_lift(
    migration_path: &Path,
    src_schema_path: &Path,
    tgt_schema_path: &Path,
    record_path: &Path,
    direction: &str,
    instance_type: &str,
    verbose: bool,
) -> Result<()> {
    let migration: Migration = load_json(migration_path)?;
    let record_json: serde_json::Value = load_json(record_path)?;

    if verbose {
        eprintln!(
            "Lifting record through migration ({} vertex mappings, direction: {direction}, instance_type: {instance_type})",
            migration.vertex_map.len()
        );
    }

    let src_schema: Schema = load_json(src_schema_path)?;
    let tgt_schema: Schema = load_json(tgt_schema_path)?;

    let compiled = mig::compile(&src_schema, &tgt_schema, &migration)
        .into_diagnostic()
        .wrap_err("failed to compile migration")?;

    match instance_type {
        "functor" => {
            return cmd_lift_functor(&compiled, &record_json, direction);
        }
        "wtype" => {}
        other => miette::bail!("unknown instance type: {other:?}. Use: wtype or functor"),
    }

    let root_vertex = {
        let domain_vertices: std::collections::BTreeSet<&Name> =
            migration.vertex_map.keys().collect();
        let targets: std::collections::HashSet<&Name> = migration
            .edge_map
            .keys()
            .map(|e| &e.tgt)
            .filter(|t| domain_vertices.contains(t))
            .collect();
        (*domain_vertices
            .iter()
            .find(|v| !targets.contains(*v))
            .or_else(|| domain_vertices.iter().next())
            .ok_or_else(|| miette::miette!("migration has no vertex mappings"))?)
        .clone()
    };

    let instance = inst::parse_json(&src_schema, &root_vertex, &record_json)
        .into_diagnostic()
        .wrap_err("failed to parse record as W-type instance")?;

    if verbose {
        eprintln!(
            "Parsed instance: {} nodes, {} arcs",
            instance.node_count(),
            instance.arc_count()
        );
    }

    let lifted = match direction {
        "restrict" => mig::lift_wtype(&compiled, &src_schema, &tgt_schema, &instance)
            .into_diagnostic()
            .wrap_err("lift (restrict / `Delta_F`) operation failed")?,
        "sigma" => mig::lift_wtype_sigma(&compiled, &tgt_schema, &instance)
            .into_diagnostic()
            .wrap_err("lift (`Sigma_F`) operation failed")?,
        "pi" => mig::lift_wtype_pi(&compiled, &tgt_schema, &instance, 10_000)
            .into_diagnostic()
            .wrap_err("lift (`Pi_F`) operation failed")?,
        other => miette::bail!("unknown lift direction: {other:?}. Use: restrict, sigma, or pi"),
    };

    let output = inst::to_json(&tgt_schema, &lifted);
    let pretty = serde_json::to_string_pretty(&output)
        .into_diagnostic()
        .wrap_err("failed to serialize output")?;

    println!("{pretty}");
    Ok(())
}

pub fn cmd_lift_functor(
    compiled: &inst::CompiledMigration,
    record_json: &serde_json::Value,
    direction: &str,
) -> Result<()> {
    let instance: inst::FInstance = serde_json::from_value(record_json.clone())
        .into_diagnostic()
        .wrap_err("failed to parse record as functor instance")?;

    let lifted = match direction {
        "restrict" => mig::lift_functor(compiled, &instance)
            .into_diagnostic()
            .wrap_err("lift functor (restrict / `Delta_F`) operation failed")?,
        "sigma" => inst::functor_extend(&instance, compiled)
            .into_diagnostic()
            .wrap_err("lift functor (`Sigma_F`) operation failed")?,
        "pi" => mig::lift_functor_pi(compiled, &instance, 10_000)
            .into_diagnostic()
            .wrap_err("lift functor (`Pi_F`) operation failed")?,
        other => miette::bail!("unknown lift direction: {other:?}. Use: restrict, sigma, or pi"),
    };

    let output = serde_json::to_string_pretty(&lifted)
        .into_diagnostic()
        .wrap_err("failed to serialize output")?;
    println!("{output}");
    Ok(())
}

pub fn cmd_auto_migrate(
    old_path: &Path,
    new_path: &Path,
    monic: bool,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let old_schema: Schema = load_json(old_path)?;
    let new_schema: Schema = load_json(new_path)?;

    if verbose {
        eprintln!(
            "Searching for morphism: {} vertices -> {} vertices{}",
            old_schema.vertex_count(),
            new_schema.vertex_count(),
            if monic { " (monic)" } else { "" }
        );
    }

    let opts = mig::SearchOptions {
        monic,
        ..mig::SearchOptions::default()
    };

    let best = mig::find_best_morphism(&old_schema, &new_schema, &opts);
    let Some(found) = best else {
        miette::bail!("no valid morphism found between the two schemas");
    };

    if json {
        let migration = mig::hom_search::morphism_to_migration(&found);
        let output = serde_json::to_string_pretty(&migration)
            .into_diagnostic()
            .wrap_err("failed to serialize migration")?;
        println!("{output}");
    } else {
        println!("Found morphism (quality: {:.3}):\n", found.quality);
        println!("Vertex map:");
        let mut vertex_entries: Vec<_> = found.vertex_map.iter().collect();
        vertex_entries.sort_by_key(|(k, _)| k.as_str());
        for (src, tgt) in &vertex_entries {
            println!("  {src} -> {tgt}");
        }
        if !found.edge_map.is_empty() {
            println!("\nEdge map:");
            for (src_e, tgt_e) in &found.edge_map {
                let src_label = src_e.name.as_deref().unwrap_or("");
                let tgt_label = tgt_e.name.as_deref().unwrap_or("");
                println!(
                    "  {}->{} ({}) {src_label} -> {}->{} ({}) {tgt_label}",
                    src_e.src, src_e.tgt, src_e.kind, tgt_e.src, tgt_e.tgt, tgt_e.kind
                );
            }
        }
    }

    Ok(())
}

pub fn cmd_integrate(
    left_path: &Path,
    right_path: &Path,
    auto_overlap: bool,
    json: bool,
    verbose: bool,
) -> Result<()> {
    use panproto_core::schema::{SchemaOverlap, schema_pushout};

    let left: Schema = load_json(left_path)?;
    let right: Schema = load_json(right_path)?;

    if verbose {
        eprintln!(
            "Integrating schemas: {} vertices / {} edges vs {} vertices / {} edges",
            left.vertex_count(),
            left.edge_count(),
            right.vertex_count(),
            right.edge_count()
        );
    }

    let overlap = if auto_overlap {
        let o = mig::discover_overlap(&left, &right);
        if verbose {
            eprintln!(
                "Discovered overlap: {} vertex pairs, {} edge pairs",
                o.vertex_pairs.len(),
                o.edge_pairs.len()
            );
        }
        o
    } else {
        SchemaOverlap::default()
    };

    let (pushout, left_morphism, right_morphism) = schema_pushout(&left, &right, &overlap)
        .into_diagnostic()
        .wrap_err("schema pushout failed")?;

    if json {
        let output = serde_json::to_string_pretty(&pushout)
            .into_diagnostic()
            .wrap_err("failed to serialize pushout schema")?;
        println!("{output}");
    } else {
        println!(
            "Integrated schema: {} vertices, {} edges",
            pushout.vertex_count(),
            pushout.edge_count()
        );
        println!(
            "  Left input:  {} vertices, {} edges",
            left.vertex_count(),
            left.edge_count()
        );
        println!(
            "  Right input: {} vertices, {} edges",
            right.vertex_count(),
            right.edge_count()
        );

        println!("\nLeft morphism (left -> pushout):");
        let mut left_entries: Vec<_> = left_morphism.vertex_map.iter().collect();
        left_entries.sort_by_key(|(k, _)| k.as_str());
        for (src, tgt) in &left_entries {
            println!("  {src} -> {tgt}");
        }

        println!("\nRight morphism (right -> pushout):");
        let mut right_entries: Vec<_> = right_morphism.vertex_map.iter().collect();
        right_entries.sort_by_key(|(k, _)| k.as_str());
        for (src, tgt) in &right_entries {
            println!("  {src} -> {tgt}");
        }
    }

    Ok(())
}

pub fn cmd_scaffold(
    protocol_name: &str,
    schema_path: &Path,
    depth: usize,
    max_terms: usize,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let schema: Schema = load_json(schema_path)?;
    let theory_registry = build_theory_registry(protocol_name)?;

    if verbose {
        eprintln!(
            "Scaffolding test data for schema ({} vertices, {} edges), depth={depth}, max_terms={max_terms}",
            schema.vertex_count(),
            schema.edge_count(),
        );
    }

    let config = panproto_core::gat::FreeModelConfig {
        max_depth: depth,
        max_terms_per_sort: max_terms,
    };

    // Build a model seeded from the schema's actual structure.
    // Map schema vertex IDs as carrier elements for "Vertex"-like sorts,
    // and schema edges as carrier elements for "Edge"-like sorts.
    let vertex_ids: Vec<String> = schema.vertices.keys().map(ToString::to_string).collect();
    let edge_strs: Vec<String> = schema
        .edges
        .keys()
        .map(|e| {
            let label = e.name.as_deref().unwrap_or("");
            format!("{}→{} {label}", e.src, e.tgt)
        })
        .collect();

    for (name, theory) in &theory_registry {
        // Build a free model from the theory to get the abstract structure.
        let model = panproto_core::gat::free_model(theory, &config)
            .into_diagnostic()
            .wrap_err_with(|| format!("free model construction failed for theory '{name}'"))?
            .model;

        if json {
            // Merge free model carriers with schema elements for richer output.
            let mut carriers: HashMap<String, Vec<String>> = HashMap::new();

            for (sort, values) in &model.sort_interp {
                let mut sort_values: Vec<String> =
                    values.iter().map(|v| format!("{v:?}")).collect();

                // Augment with schema data when the sort name suggests vertices/edges.
                let sort_lower = sort.to_lowercase();
                if sort_lower.contains("vertex")
                    || sort_lower.contains("node")
                    || sort_lower.contains("object")
                {
                    for vid in &vertex_ids {
                        sort_values.push(format!("Str(\"{vid}\")"));
                    }
                } else if sort_lower.contains("edge")
                    || sort_lower.contains("arrow")
                    || sort_lower.contains("morphism")
                {
                    for estr in &edge_strs {
                        sort_values.push(format!("Str(\"{estr}\")"));
                    }
                }

                carriers.insert(sort.clone(), sort_values);
            }

            let output = serde_json::to_string_pretty(&carriers)
                .into_diagnostic()
                .wrap_err("failed to serialize scaffold")?;
            println!("{output}");
        } else {
            println!("Theory '{name}':");
            println!(
                "  schema: {} vertices, {} edges",
                vertex_ids.len(),
                edge_strs.len()
            );
            for (sort, values) in &model.sort_interp {
                println!("  sort '{sort}': {} element(s)", values.len());
                if verbose {
                    for (i, v) in values.iter().enumerate() {
                        println!("    [{i}] {v:?}");
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn cmd_normalize(
    protocol_name: &str,
    schema_path: &Path,
    identifications: &[String],
    json: bool,
    verbose: bool,
) -> Result<()> {
    let schema: Schema = load_json(schema_path)?;
    let theory_registry = build_theory_registry(protocol_name)?;

    if verbose {
        eprintln!(
            "Normalizing schema ({} vertices, {} edges)",
            schema.vertex_count(),
            schema.edge_count(),
        );
    }

    // Validate that identified elements exist in the schema.
    // Parse identifications: each is "A=B".
    let mut ident_pairs: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)> = Vec::new();
    for ident in identifications {
        let parts: Vec<&str> = ident.split('=').collect();
        if parts.len() != 2 {
            miette::bail!("invalid identification '{ident}': expected 'A=B' format");
        }
        ident_pairs.push((parts[0].into(), parts[1].into()));
    }

    if ident_pairs.is_empty() {
        miette::bail!("at least one --identify pair is required (e.g., --identify A=B)");
    }

    // Warn if identified names don't appear in the schema as vertices.
    for (a, b) in &ident_pairs {
        if !schema.vertices.contains_key(a.as_ref())
            && !schema
                .edges
                .keys()
                .any(|e| e.name.as_deref() == Some(a.as_ref()))
        {
            eprintln!("warning: '{a}' not found as a vertex or edge name in schema");
        }
        if !schema.vertices.contains_key(b.as_ref())
            && !schema
                .edges
                .keys()
                .any(|e| e.name.as_deref() == Some(b.as_ref()))
        {
            eprintln!("warning: '{b}' not found as a vertex or edge name in schema");
        }
    }

    for (name, theory) in &theory_registry {
        match panproto_core::gat::quotient(theory, &ident_pairs) {
            Ok(simplified) => {
                if json {
                    let info = serde_json::json!({
                        "theory": name,
                        "original_sorts": theory.sorts.len(),
                        "original_ops": theory.ops.len(),
                        "simplified_sorts": simplified.sorts.len(),
                        "simplified_ops": simplified.ops.len(),
                        "sorts": simplified.sorts.iter().map(|s| s.name.to_string()).collect::<Vec<_>>(),
                        "operations": simplified.ops.iter().map(|o| o.name.to_string()).collect::<Vec<_>>(),
                    });
                    let output = serde_json::to_string_pretty(&info)
                        .into_diagnostic()
                        .wrap_err("failed to serialize")?;
                    println!("{output}");
                } else {
                    println!("Theory '{name}':");
                    println!(
                        "  {} sorts -> {} sorts",
                        theory.sorts.len(),
                        simplified.sorts.len()
                    );
                    println!(
                        "  {} operations -> {} operations",
                        theory.ops.len(),
                        simplified.ops.len()
                    );
                    if verbose {
                        println!("  Remaining sorts:");
                        for sort in simplified.sorts {
                            println!("    {}", sort.name);
                        }
                        println!("  Remaining operations:");
                        for op in simplified.ops {
                            println!("    {}", op.name);
                        }
                    }
                }
            }
            Err(e) => {
                println!("error: cannot normalize theory '{name}': {e}");
                println!("  hint: check that the identified elements have compatible arities");
            }
        }
    }

    Ok(())
}

pub fn cmd_typecheck(
    src_path: &Path,
    tgt_path: &Path,
    migration_path: &Path,
    verbose: bool,
) -> Result<()> {
    let src_schema: Schema = load_json(src_path)?;
    let tgt_schema: Schema = load_json(tgt_path)?;
    let migration: Migration = load_json(migration_path)?;

    if verbose {
        eprintln!(
            "Type-checking migration: {} vertex mappings, {} edge mappings",
            migration.vertex_map.len(),
            migration.edge_map.len()
        );
    }

    // Run GAT-level validation.
    let diag = vcs::gat_validate::validate_migration(&src_schema, &tgt_schema, &migration);

    // Also type-check any protocol theories.
    let protocol_name = &src_schema.protocol;
    let theory_diag = build_theory_registry(protocol_name).map_or_else(
        |_| Vec::new(),
        |registry| {
            let mut errors = Vec::new();
            for (name, theory) in &registry {
                let td = vcs::gat_validate::validate_theory_equations(theory);
                for e in td.all_errors() {
                    errors.push(format!("theory '{name}': {e}"));
                }
            }
            errors
        },
    );

    let mut has_errors = false;

    if !diag.migration_warnings.is_empty() {
        println!("Migration warnings:");
        for w in &diag.migration_warnings {
            println!("  warning: {w}");
        }
    }

    if diag.has_errors() {
        has_errors = true;
        println!("Migration errors:");
        for e in &diag.all_errors() {
            println!("  error: {e}");
        }
    }

    if !theory_diag.is_empty() {
        has_errors = true;
        println!("Theory type-check errors:");
        for e in &theory_diag {
            println!("  error: {e}");
        }
    }

    if has_errors {
        miette::bail!("type-check failed");
    }

    println!("Type-check passed.");
    Ok(())
}

pub fn cmd_verify(
    protocol_name: &str,
    schema_path: &Path,
    max_assignments: usize,
    verbose: bool,
) -> Result<()> {
    let schema: Schema = load_json(schema_path)?;
    let theory_registry = build_theory_registry(protocol_name)?;

    if verbose {
        eprintln!(
            "Verifying schema ({} vertices, {} edges) against {} theories (max_assignments={max_assignments})",
            schema.vertex_count(),
            schema.edge_count(),
            theory_registry.len()
        );
    }

    let options = panproto_core::gat::CheckModelOptions { max_assignments };
    let mut total_violations = 0;

    for (name, theory) in &theory_registry {
        if let Err(e) = panproto_core::gat::typecheck_theory(theory) {
            println!("error: theory '{name}' has type errors, skipping equation check\n  --> {e}");
            continue;
        }

        let model = build_schema_model(&schema, name, theory);

        match panproto_core::gat::check_model_with_options(&model, theory, &options) {
            Ok(violations) => {
                if violations.is_empty() {
                    println!("Theory '{name}': all equations satisfied.");
                } else {
                    total_violations += violations.len();
                    println!(
                        "Theory '{name}': {} equation violation(s):",
                        violations.len()
                    );
                    for v in &violations {
                        let assignment_str: String = v
                            .assignment
                            .iter()
                            .map(|(var, val)| format!("{var}={val:?}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        println!(
                            "  equation '{}' violated when {}: LHS={:?}, RHS={:?}",
                            v.equation, assignment_str, v.lhs_value, v.rhs_value
                        );
                    }
                }
            }
            Err(e) => {
                println!("Theory '{name}': equation check incomplete: {e}");
            }
        }
    }

    if total_violations > 0 {
        miette::bail!("verification failed with {total_violations} equation violation(s)");
    }

    println!("Verification passed.");
    Ok(())
}

/// Options controlling diff output format.
#[allow(clippy::struct_excessive_bools)]
pub struct DiffOptions {
    pub stat: bool,
    pub name_only: bool,
    pub name_status: bool,
    pub staged: bool,
    pub verbose: bool,
    pub detect_renames: bool,
    pub theory: bool,
    pub optic_kind: bool,
}

pub fn cmd_diff(
    old_path: Option<&Path>,
    new_path: Option<&Path>,
    opts: &DiffOptions,
) -> Result<()> {
    let DiffOptions {
        stat,
        name_only,
        name_status,
        staged,
        verbose,
        detect_renames,
        theory,
        optic_kind,
    } = *opts;
    if staged {
        return cmd_diff_staged(opts);
    }

    let old_path =
        old_path.ok_or_else(|| miette::miette!("old schema path is required (or use --staged)"))?;
    let new_path =
        new_path.ok_or_else(|| miette::miette!("new schema path is required (or use --staged)"))?;

    let old_schema: Schema = load_json(old_path)?;
    let new_schema: Schema = load_json(new_path)?;

    if verbose {
        eprintln!(
            "Diffing schemas: {} vertices / {} edges vs {} vertices / {} edges",
            old_schema.vertex_count(),
            old_schema.edge_count(),
            new_schema.vertex_count(),
            new_schema.edge_count()
        );
    }

    let schema_diff = panproto_core::check::diff::diff(&old_schema, &new_schema);
    print_diff(
        &schema_diff,
        &old_schema,
        &new_schema,
        stat,
        name_only,
        name_status,
    );
    if detect_renames {
        print_detected_renames(&old_schema, &new_schema);
    }
    if theory {
        print_theory_diff(&old_schema, &new_schema);
    }
    if optic_kind {
        print_optic_kind(&old_schema, &new_schema);
    }
    Ok(())
}

/// Diff the staged schema against HEAD.
fn cmd_diff_staged(opts: &DiffOptions) -> Result<()> {
    let DiffOptions {
        stat,
        name_only,
        name_status,
        detect_renames,
        theory,
        optic_kind,
        ..
    } = *opts;

    let repo = open_repo()?;
    let index_path = repo.store().root().join("index.json");
    if !index_path.exists() {
        miette::bail!("nothing staged");
    }
    let index: vcs::Index = load_json(&index_path)?;
    let staged_entry = index
        .staged
        .ok_or_else(|| miette::miette!("nothing staged"))?;

    let head_id = vcs::store::resolve_head(repo.store())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no commits yet — use diff with file paths instead"))?;
    let head_obj = repo.store().get(&head_id).into_diagnostic()?;
    let vcs::Object::Commit(head_commit) = head_obj else {
        miette::bail!("HEAD does not point to a commit")
    };
    let old_obj = repo.store().get(&head_commit.schema_id).into_diagnostic()?;
    let old_schema = match old_obj {
        vcs::Object::Schema(s) => *s,
        _ => miette::bail!("HEAD commit does not reference a schema"),
    };
    let new_obj = repo
        .store()
        .get(&staged_entry.schema_id)
        .into_diagnostic()?;
    let new_schema = match new_obj {
        vcs::Object::Schema(s) => *s,
        _ => miette::bail!("staged entry does not reference a schema"),
    };

    let schema_diff = panproto_core::check::diff::diff(&old_schema, &new_schema);
    print_diff(
        &schema_diff,
        &old_schema,
        &new_schema,
        stat,
        name_only,
        name_status,
    );
    if detect_renames {
        print_detected_renames(&old_schema, &new_schema);
    }
    if theory {
        let staged_commit = vcs::CommitObject::builder(staged_entry.schema_id, "", "", "").build();
        print_stored_theory_diff(
            repo.store(),
            &head_commit,
            &staged_commit,
            &old_schema,
            &new_schema,
        );
    }
    if optic_kind {
        print_optic_kind(&old_schema, &new_schema);
    }
    Ok(())
}

/// Print the optic classification of the diff between two schemas.
///
/// Auto-generates a protolens chain and classifies it as iso, lens,
/// prism, affine, or traversal based on complement structure.
pub fn print_optic_kind(old_schema: &Schema, new_schema: &Schema) {
    let protocol = match resolve_protocol(&old_schema.protocol) {
        Ok(p) => p,
        Err(_) => {
            if let Ok(p) = resolve_protocol("atproto") {
                p
            } else {
                println!("\nCould not resolve protocol for optic classification.");
                return;
            }
        }
    };

    let config = panproto_core::lens::AutoLensConfig::default();
    match panproto_core::lens::auto_generate(old_schema, new_schema, &protocol, &config) {
        Ok(result) => {
            let kind = classify_chain_optic_kind(&result.chain);
            println!("\nOptic classification: {kind}");
        }
        Err(e) => {
            println!("\nCould not classify optic kind: {e}");
        }
    }
}

/// Classify a protolens chain's optic kind based on complement constructors.
fn classify_chain_optic_kind(chain: &panproto_core::lens::ProtolensChain) -> &'static str {
    if chain.steps.is_empty() {
        return "iso";
    }

    let mut has_added = false;
    let mut has_dropped = false;

    for step in &chain.steps {
        classify_complement_kind(
            &step.complement_constructor,
            &mut has_added,
            &mut has_dropped,
        );
    }

    match (has_added, has_dropped) {
        (false, false) => "iso",
        (true, false) => "lens",
        (false, true) => "prism",
        (true, true) => "affine",
    }
}

/// Recursively classify a complement constructor.
fn classify_complement_kind(
    cc: &panproto_core::lens::protolens::ComplementConstructor,
    has_added: &mut bool,
    has_dropped: &mut bool,
) {
    use panproto_core::lens::protolens::ComplementConstructor;
    match cc {
        ComplementConstructor::Empty => {}
        ComplementConstructor::AddedElement { .. } => {
            *has_added = true;
        }
        ComplementConstructor::Composite(subs) => {
            for sub in subs {
                classify_complement_kind(sub, has_added, has_dropped);
            }
        }
        _ => {
            *has_dropped = true;
        }
    }
}

/// Print detected vertex and edge renames between two schemas.
pub fn print_detected_renames(old_schema: &Schema, new_schema: &Schema) {
    let vertex_renames = vcs::rename_detect::detect_vertex_renames(old_schema, new_schema, 0.3);
    let edge_renames = vcs::rename_detect::detect_edge_renames(old_schema, new_schema, 0.3);

    if vertex_renames.is_empty() && edge_renames.is_empty() {
        println!("\nNo renames detected.");
        return;
    }

    println!("\nDetected renames:");
    for r in &vertex_renames {
        println!(
            "  vertex {} -> {} (confidence: {:.2})",
            r.rename.old, r.rename.new, r.confidence
        );
    }
    for r in &edge_renames {
        println!(
            "  edge {} -> {} (confidence: {:.2})",
            r.rename.old, r.rename.new, r.confidence
        );
    }
}

pub fn print_diff(
    schema_diff: &panproto_core::check::diff::SchemaDiff,
    old_schema: &Schema,
    new_schema: &Schema,
    stat: bool,
    name_only: bool,
    name_status: bool,
) {
    if schema_diff.is_empty() {
        println!("Schemas are identical.");
        return;
    }

    if stat {
        println!("{}", format::format_diff_stat(schema_diff));
        return;
    }

    if name_only {
        println!(
            "{}",
            format::format_diff_name_only(schema_diff, old_schema, new_schema)
        );
        return;
    }

    if name_status {
        println!(
            "{}",
            format::format_diff_name_status(schema_diff, old_schema, new_schema)
        );
        return;
    }

    // Default detailed output.
    let total = schema_diff.added_vertices.len()
        + schema_diff.removed_vertices.len()
        + schema_diff.added_edges.len()
        + schema_diff.removed_edges.len()
        + schema_diff.kind_changes.len()
        + schema_diff.modified_constraints.len();
    println!("{total} change(s) detected:\n");

    for v in &schema_diff.added_vertices {
        let kind = new_schema
            .vertices
            .get(v.as_str())
            .map_or("?", |vtx| &vtx.kind);
        println!("  + vertex {v} ({kind})");
    }
    for v in &schema_diff.removed_vertices {
        let kind = old_schema
            .vertices
            .get(v.as_str())
            .map_or("?", |vtx| &vtx.kind);
        println!("  - vertex {v} ({kind})");
    }
    for kc in &schema_diff.kind_changes {
        println!(
            "  ~ vertex {}: {} -> {}",
            kc.vertex_id, kc.old_kind, kc.new_kind
        );
    }
    for e in &schema_diff.added_edges {
        let label = e.name.as_deref().unwrap_or("");
        println!("  + edge {} -> {} ({}) {label}", e.src, e.tgt, e.kind);
    }
    for e in &schema_diff.removed_edges {
        let label = e.name.as_deref().unwrap_or("");
        println!("  - edge {} -> {} ({}) {label}", e.src, e.tgt, e.kind);
    }
    for (vid, cdiff) in &schema_diff.modified_constraints {
        for c in &cdiff.added {
            println!("  + constraint {vid}: {} = {}", c.sort, c.value);
        }
        for c in &cdiff.removed {
            println!("  - constraint {vid}: {} = {}", c.sort, c.value);
        }
        for c in &cdiff.changed {
            println!(
                "  ~ constraint {vid}: {} = {} -> {}",
                c.sort, c.old_value, c.new_value
            );
        }
    }
}

fn show_commit(
    repo: &vcs::Repository,
    id: &vcs::ObjectId,
    c: &vcs::CommitObject,
    fmt: Option<&str>,
    stat: bool,
) -> Result<()> {
    if let Some(fmt_str) = fmt {
        println!("{}", format::format_commit(c, fmt_str)?);
        return Ok(());
    }

    println!("commit {id}");
    println!("Schema:    {}", c.schema_id);
    println!(
        "Parents:   {}",
        c.parents
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    if let Some(mig_id) = c.migration_id {
        println!("Migration: {mig_id}");
    }
    println!("Protocol:  {}", c.protocol);
    println!("Author:    {}", c.author);
    println!(
        "Date:      {}",
        super::helpers::format_timestamp(c.timestamp)
    );
    println!("\n    {}", c.message);

    if stat {
        if let Some(parent_id) = c.parents.first() {
            let parent_obj = repo.store().get(parent_id).into_diagnostic()?;
            if let vcs::Object::Commit(parent_commit) = parent_obj {
                let old_obj = repo
                    .store()
                    .get(&parent_commit.schema_id)
                    .into_diagnostic()?;
                let new_obj = repo.store().get(&c.schema_id).into_diagnostic()?;
                if let (vcs::Object::Schema(old_s), vcs::Object::Schema(new_s)) = (old_obj, new_obj)
                {
                    let d = panproto_core::check::diff::diff(&old_s, &new_s);
                    println!("\n {}", format::format_diff_stat(&d));
                }
            }
        }
    }
    Ok(())
}

pub fn cmd_show(target: &str, fmt: Option<&str>, stat: bool) -> Result<()> {
    let repo = open_repo()?;
    let id = vcs::refs::resolve_ref(repo.store(), target)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{target}'"))?;

    let object = repo.store().get(&id).into_diagnostic()?;
    match object {
        vcs::Object::Commit(c) => show_commit(&repo, &id, &c, fmt, stat)?,
        vcs::Object::Schema(s) => {
            println!("schema {id}");
            println!("Protocol:  {}", s.protocol);
            println!("Vertices:  {}", s.vertex_count());
            println!("Edges:     {}", s.edge_count());
        }
        vcs::Object::Migration { src, tgt, mapping } => {
            println!("migration {id}");
            println!("Source:    {src}");
            println!("Target:    {tgt}");
            println!("Vertex mappings: {}", mapping.vertex_map.len());
            println!("Edge mappings:   {}", mapping.edge_map.len());
        }
        vcs::Object::Tag(tag) => {
            println!("tag {id}");
            println!("Target:    {}", tag.target);
            println!("Tagger:    {}", tag.tagger);
            println!(
                "Date:      {}",
                super::helpers::format_timestamp(tag.timestamp)
            );
            println!("\n    {}", tag.message);
        }
        vcs::Object::DataSet(ds) => {
            println!("dataset {id}");
            println!("Schema:    {}", ds.schema_id);
            println!("Records:   {}", ds.record_count);
            println!("Size:      {} bytes", ds.data.len());
        }
        vcs::Object::Complement(comp) => {
            println!("complement {id}");
            println!("Migration: {}", comp.migration_id);
            println!("Data:      {}", comp.data_id);
            println!("Size:      {} bytes", comp.complement.len());
        }
        vcs::Object::Protocol(proto) => {
            println!("protocol {id}");
            println!("Name:      {}", proto.name);
            println!("Schema theory: {}", proto.schema_theory);
            println!("Instance theory: {}", proto.instance_theory);
            println!("Object kinds: {}", proto.obj_kinds.len());
        }
        vcs::Object::Expr(expr) => {
            println!("expr {id}");
            println!("{expr:?}");
        }
        vcs::Object::EditLog(el) => {
            println!("editlog {id}");
            println!("Schema:     {}", el.schema_id);
            println!("Data:       {}", el.data_id);
            println!("Edits:      {}", el.edit_count);
            println!("Complement: {}", el.final_complement);
            println!("Size:       {} bytes", el.edits.len());
        }
        vcs::Object::Theory(theory) => {
            println!("theory {id}");
            println!("Name:       {}", theory.name);
            println!("Sorts:      {}", theory.sorts.len());
            println!("Operations: {}", theory.ops.len());
            println!("Equations:  {}", theory.eqs.len());
        }
        vcs::Object::TheoryMorphism(morph) => {
            println!("theory_morphism {id}");
            println!("Name:      {}", morph.name);
            println!("Domain:    {}", morph.domain);
            println!("Codomain:  {}", morph.codomain);
            println!("Sort map:  {}", morph.sort_map.len());
            println!("Op map:    {}", morph.op_map.len());
        }
    }
    Ok(())
}
