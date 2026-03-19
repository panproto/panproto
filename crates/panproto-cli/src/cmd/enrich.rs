use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    gat::Name,
    lens,
    schema::{Constraint, Schema},
    vcs::{self, Store as _},
};

use super::helpers::{open_repo, resolve_protocol};

/// Add a default value expression to a schema vertex.
///
/// If the vertex already exists, adds a `default` constraint with the
/// provided expression value. If it does not exist, creates it via a
/// protolens `add_sort` step.
pub fn cmd_enrich_add_default(vertex: &str, expr_json: &str, verbose: bool) -> Result<()> {
    let repo = open_repo()?;
    let (schema, protocol_name) = load_head_schema(&repo)?;

    let default_value: panproto_core::inst::value::Value = serde_json::from_str(expr_json)
        .into_diagnostic()
        .wrap_err("failed to parse default expression JSON")?;

    if verbose {
        eprintln!("Adding default to vertex '{vertex}': {default_value:?}");
    }

    let new_schema = if schema.has_vertex(vertex) {
        let mut s = schema;
        let constraint = Constraint {
            sort: "default".into(),
            value: format!("{default_value:?}"),
        };
        s.constraints
            .entry(Name::from(vertex))
            .or_default()
            .push(constraint);
        s
    } else {
        let vertex_kind = Name::from("string");
        let protolens =
            lens::protolens::elementary::add_sort(Name::from(vertex), vertex_kind, default_value);

        let protocol = resolve_protocol(&protocol_name)?;
        let lens_obj = protolens
            .instantiate(&schema, &protocol)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to add default for '{vertex}'"))?;
        lens_obj.tgt_schema
    };

    stage_enriched_schema(&repo, &new_schema)?;
    println!("Added default for vertex '{vertex}'.");
    Ok(())
}

/// Add a coercion expression between two vertex kinds.
///
/// Creates a protolens rename step from `from_kind` to `to_kind`.
pub fn cmd_enrich_add_coercion(
    from_kind: &str,
    to_kind: &str,
    expr_json: &str,
    verbose: bool,
) -> Result<()> {
    let repo = open_repo()?;
    let (schema, protocol_name) = load_head_schema(&repo)?;

    let _coercion_term: panproto_core::gat::Term = serde_json::from_str(expr_json)
        .into_diagnostic()
        .wrap_err("failed to parse coercion expression JSON")?;

    if verbose {
        eprintln!("Adding coercion: {from_kind} -> {to_kind}");
    }

    let protolens =
        lens::protolens::elementary::rename_sort(Name::from(from_kind), Name::from(to_kind));

    let protocol = resolve_protocol(&protocol_name)?;
    let lens_obj = protolens
        .instantiate(&schema, &protocol)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to add coercion {from_kind} -> {to_kind}"))?;

    stage_enriched_schema(&repo, &lens_obj.tgt_schema)?;
    println!("Added coercion: {from_kind} -> {to_kind}.");
    Ok(())
}

/// Add a merger expression to a schema vertex.
///
/// Records a merge strategy as a constraint on the vertex.
pub fn cmd_enrich_add_merger(vertex: &str, expr_json: &str, verbose: bool) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct MergerSpec {
        strategy: String,
        #[serde(default)]
        args: Vec<String>,
    }

    let repo = open_repo()?;
    let (schema, _protocol_name) = load_head_schema(&repo)?;

    if !schema.has_vertex(vertex) {
        miette::bail!("vertex '{vertex}' not found in HEAD schema");
    }

    let merger: MergerSpec = serde_json::from_str(expr_json)
        .into_diagnostic()
        .wrap_err("failed to parse merger expression JSON")?;

    let MergerSpec { strategy, args } = merger;

    if verbose {
        eprintln!("Adding merger to vertex '{vertex}': strategy={strategy}");
    }
    let constraint_value = if args.is_empty() {
        strategy
    } else {
        format!("{strategy}({})", args.join(", "))
    };
    let mut new_schema = schema;
    let constraint = Constraint {
        sort: "merger".into(),
        value: constraint_value,
    };
    new_schema
        .constraints
        .entry(Name::from(vertex))
        .or_default()
        .push(constraint);

    stage_enriched_schema(&repo, &new_schema)?;
    println!("Added merger for vertex '{vertex}'.");
    Ok(())
}

/// Add a conflict policy to a schema vertex.
///
/// Records the policy as a `conflict_policy` constraint.
pub fn cmd_enrich_add_policy(vertex: &str, strategy: &str, verbose: bool) -> Result<()> {
    let repo = open_repo()?;
    let (schema, _protocol_name) = load_head_schema(&repo)?;

    if !schema.has_vertex(vertex) {
        miette::bail!("vertex '{vertex}' not found in HEAD schema");
    }

    if verbose {
        eprintln!("Adding conflict policy to vertex '{vertex}': {strategy}");
    }

    let mut new_schema = schema;
    let constraint = Constraint {
        sort: "conflict_policy".into(),
        value: strategy.to_owned(),
    };
    new_schema
        .constraints
        .entry(Name::from(vertex))
        .or_default()
        .push(constraint);

    stage_enriched_schema(&repo, &new_schema)?;
    println!("Added conflict policy for vertex '{vertex}': {strategy}");
    Ok(())
}

/// List all enrichments on the HEAD schema.
///
/// Scans the schema's constraints for enrichment annotations (defaults,
/// mergers, conflict policies, coercions).
pub fn cmd_enrich_list(verbose: bool) -> Result<()> {
    let repo = open_repo()?;
    let (schema, _protocol_name) = load_head_schema(&repo)?;

    if verbose {
        eprintln!(
            "HEAD schema: {} vertices, {} edges, {} constraint groups",
            schema.vertex_count(),
            schema.edge_count(),
            schema.constraints.len()
        );
    }

    let enrichment_sorts = ["default", "merger", "conflict_policy"];
    let mut found = 0u32;

    for (vertex_name, constraints) in &schema.constraints {
        for c in constraints {
            if enrichment_sorts.contains(&c.sort.as_ref()) {
                println!("  {vertex_name}: {} = {}", c.sort, c.value);
                found += 1;
            }
        }
    }

    if found == 0 {
        println!("No enrichments found on HEAD schema.");
    } else {
        println!("\n{found} enrichment(s) total.");
    }

    Ok(())
}

/// Remove an enrichment from the HEAD schema by name.
///
/// Searches constraint annotations for a matching enrichment and removes it.
pub fn cmd_enrich_remove(name: &str, verbose: bool) -> Result<()> {
    let repo = open_repo()?;
    let (schema, _protocol_name) = load_head_schema(&repo)?;

    let enrichment_sorts = ["default", "merger", "conflict_policy"];
    let mut new_schema = schema;
    let mut removed = false;

    for constraints in new_schema.constraints.values_mut() {
        let before_len = constraints.len();
        constraints.retain(|c| {
            let sort_str: &str = c.sort.as_ref();
            let value_str: &str = c.value.as_ref();
            !(enrichment_sorts.contains(&sort_str) && (value_str == name || sort_str == name))
        });
        if constraints.len() < before_len {
            removed = true;
        }
    }

    // Also try removing by vertex name + sort combo.
    if !removed {
        if let Some(constraints) = new_schema.constraints.get_mut(&Name::from(name)) {
            let before_len = constraints.len();
            constraints.retain(|c| !enrichment_sorts.contains(&c.sort.as_ref()));
            if constraints.len() < before_len {
                removed = true;
            }
        }
    }

    if !removed {
        miette::bail!("enrichment '{name}' not found");
    }

    if verbose {
        eprintln!("Removed enrichment: {name}");
    }

    stage_enriched_schema(&repo, &new_schema)?;
    println!("Removed enrichment '{name}'.");
    Ok(())
}

/// Load the HEAD schema and its protocol name from the repository.
fn load_head_schema(repo: &vcs::Repository) -> Result<(Schema, String)> {
    let head_id = vcs::store::resolve_head(repo.store())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("empty repository — no commits yet"))?;

    let head_obj = repo.store().get(&head_id).into_diagnostic()?;
    let vcs::Object::Commit(head_commit) = head_obj else {
        miette::bail!("HEAD does not point to a commit");
    };

    let schema_obj = repo.store().get(&head_commit.schema_id).into_diagnostic()?;
    let schema = match schema_obj {
        vcs::Object::Schema(s) => *s,
        _ => miette::bail!("HEAD commit does not reference a schema"),
    };

    Ok((schema, head_commit.protocol))
}

/// Stage an enriched schema in the repository using the standard add pipeline.
fn stage_enriched_schema(_repo: &vcs::Repository, schema: &Schema) -> Result<()> {
    // We need a mutable repo to stage. Re-open it.
    let cwd = std::env::current_dir().into_diagnostic()?;
    let mut mutable_repo = vcs::Repository::open(&cwd)
        .into_diagnostic()
        .wrap_err("failed to open repository for staging")?;

    mutable_repo
        .add(schema)
        .into_diagnostic()
        .wrap_err("failed to stage enriched schema")?;

    Ok(())
}
