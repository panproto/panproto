//! # panproto-cli
//!
//! Command-line interface for panproto.
//!
//! Provides subcommands for schema validation, migration checking,
//! breaking change detection, and record lifting.

use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    gat::Theory,
    inst,
    mig::{self, Migration},
    protocols,
    schema::{Protocol, Schema},
};

/// The panproto command-line tool for schema migration and validation.
#[derive(Parser, Debug)]
#[command(
    name = "panproto",
    version,
    about = "Schema migration toolkit based on generalized algebraic theories"
)]
struct Cli {
    /// Enable verbose output.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// The subcommand to execute.
    #[command(subcommand)]
    command: Command,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
enum Command {
    /// Validate a schema against a protocol.
    Validate {
        /// The protocol name (e.g., "atproto").
        #[arg(long)]
        protocol: String,

        /// Path to the schema JSON file.
        schema: PathBuf,
    },

    /// Check existence conditions for a migration between two schemas.
    Check {
        /// Path to the source schema JSON file.
        #[arg(long)]
        src: PathBuf,

        /// Path to the target schema JSON file.
        #[arg(long)]
        tgt: PathBuf,

        /// Path to the migration mapping JSON file.
        #[arg(long)]
        mapping: PathBuf,
    },

    /// Diff two schemas and report structural changes.
    Diff {
        /// Path to the old schema JSON file.
        old: PathBuf,

        /// Path to the new schema JSON file.
        new: PathBuf,
    },

    /// Apply a migration to a record, transforming it from source to
    /// target schema.
    Lift {
        /// Path to the migration mapping JSON file.
        #[arg(long)]
        migration: PathBuf,

        /// Path to the source schema JSON file.
        #[arg(long)]
        src_schema: PathBuf,

        /// Path to the target schema JSON file.
        #[arg(long)]
        tgt_schema: PathBuf,

        /// Path to the record JSON file.
        record: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Validate { protocol, schema } => cmd_validate(&protocol, &schema, cli.verbose),
        Command::Check { src, tgt, mapping } => cmd_check(&src, &tgt, &mapping, cli.verbose),
        Command::Diff { old, new } => cmd_diff(&old, &new, cli.verbose),
        Command::Lift {
            migration,
            src_schema,
            tgt_schema,
            record,
        } => cmd_lift(&migration, &src_schema, &tgt_schema, &record, cli.verbose),
    }
}

/// Load and parse a JSON file into a typed value.
fn load_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let contents = std::fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", path.display()))?;

    serde_json::from_str(&contents)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to parse JSON from {}", path.display()))
}

/// Resolve a protocol by name from built-in definitions.
///
/// Supported protocol names: `atproto`, `sql`, `protobuf`, `graphql`,
/// `json-schema` / `jsonschema`.
///
/// # Errors
///
/// Returns an error if the protocol name is not recognized.
fn resolve_protocol(name: &str) -> Result<Protocol> {
    match name {
        "atproto" => Ok(protocols::atproto::protocol()),
        "sql" => Ok(protocols::sql::protocol()),
        "protobuf" => Ok(protocols::protobuf::protocol()),
        "graphql" => Ok(protocols::graphql::protocol()),
        "json-schema" | "jsonschema" => Ok(protocols::json_schema::protocol()),
        _ => miette::bail!(
            "unknown protocol: {name:?}. Supported: atproto, sql, protobuf, graphql, json-schema"
        ),
    }
}

/// Build a theory registry for a protocol by name.
///
/// # Errors
///
/// Returns an error if the protocol name is not recognized.
fn build_theory_registry(protocol_name: &str) -> Result<HashMap<String, Theory>> {
    let mut registry = HashMap::new();
    match protocol_name {
        "atproto" => protocols::atproto::register_theories(&mut registry),
        "sql" => protocols::sql::register_theories(&mut registry),
        "protobuf" => protocols::protobuf::register_theories(&mut registry),
        "graphql" => protocols::graphql::register_theories(&mut registry),
        "json-schema" | "jsonschema" => protocols::json_schema::register_theories(&mut registry),
        _ => miette::bail!(
            "unknown protocol for theory registry: {protocol_name:?}. Supported: atproto, sql, protobuf, graphql, json-schema"
        ),
    }
    Ok(registry)
}

// ---------------------------------------------------------------------------
// Subcommand implementations
// ---------------------------------------------------------------------------

/// Validate a schema file against a protocol.
fn cmd_validate(protocol_name: &str, schema_path: &PathBuf, verbose: bool) -> Result<()> {
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

    if errors.is_empty() {
        println!("Schema is valid.");
        Ok(())
    } else {
        println!("Validation found {} error(s):", errors.len());
        for (i, err) in errors.iter().enumerate() {
            println!("  {}. {err}", i + 1);
        }
        // Return an error to set exit code.
        miette::bail!("schema validation failed with {} error(s)", errors.len());
    }
}

/// Check existence conditions for a migration.
fn cmd_check(
    src_path: &PathBuf,
    tgt_path: &PathBuf,
    mapping_path: &PathBuf,
    verbose: bool,
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

    // Output as JSON for machine consumption.
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

/// Diff two schemas and print structural changes.
fn cmd_diff(old_path: &PathBuf, new_path: &PathBuf, verbose: bool) -> Result<()> {
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

    // Compute vertex additions and removals.
    let mut added_vertices = Vec::new();
    let mut removed_vertices = Vec::new();
    let mut kind_changes = Vec::new();

    for (id, new_v) in &new_schema.vertices {
        if let Some(old_v) = old_schema.vertices.get(id) {
            if old_v.kind != new_v.kind {
                kind_changes.push((id.clone(), old_v.kind.clone(), new_v.kind.clone()));
            }
        } else {
            added_vertices.push(id.clone());
        }
    }
    for id in old_schema.vertices.keys() {
        if !new_schema.vertices.contains_key(id) {
            removed_vertices.push(id.clone());
        }
    }

    // Compute edge additions and removals.
    let mut added_edges = Vec::new();
    let mut removed_edges = Vec::new();

    for edge in new_schema.edges.keys() {
        if !old_schema.edges.contains_key(edge) {
            added_edges.push(edge.clone());
        }
    }
    for edge in old_schema.edges.keys() {
        if !new_schema.edges.contains_key(edge) {
            removed_edges.push(edge.clone());
        }
    }

    // Sort for deterministic output.
    added_vertices.sort();
    removed_vertices.sort();

    // Print the diff.
    let total_changes = added_vertices.len()
        + removed_vertices.len()
        + added_edges.len()
        + removed_edges.len()
        + kind_changes.len();

    if total_changes == 0 {
        println!("Schemas are identical.");
        return Ok(());
    }

    println!("{total_changes} change(s) detected:\n");

    if !added_vertices.is_empty() {
        println!("Added vertices ({}):", added_vertices.len());
        for v in &added_vertices {
            let kind = new_schema
                .vertices
                .get(v)
                .map_or("?", |vertex| &vertex.kind);
            println!("  + {v} ({kind})");
        }
        println!();
    }

    if !removed_vertices.is_empty() {
        println!("Removed vertices ({}):", removed_vertices.len());
        for v in &removed_vertices {
            let kind = old_schema
                .vertices
                .get(v)
                .map_or("?", |vertex| &vertex.kind);
            println!("  - {v} ({kind})");
        }
        println!();
    }

    if !kind_changes.is_empty() {
        println!("Kind changes ({}):", kind_changes.len());
        for (id, old_kind, new_kind) in &kind_changes {
            println!("  ~ {id}: {old_kind} -> {new_kind}");
        }
        println!();
    }

    if !added_edges.is_empty() {
        println!("Added edges ({}):", added_edges.len());
        for e in &added_edges {
            let label = e.name.as_deref().unwrap_or("");
            println!("  + {} -> {} ({}) {label}", e.src, e.tgt, e.kind);
        }
        println!();
    }

    if !removed_edges.is_empty() {
        println!("Removed edges ({}):", removed_edges.len());
        for e in &removed_edges {
            let label = e.name.as_deref().unwrap_or("");
            println!("  - {} -> {} ({}) {label}", e.src, e.tgt, e.kind);
        }
    }

    Ok(())
}

/// Apply a migration to a record.
fn cmd_lift(
    migration_path: &PathBuf,
    src_schema_path: &PathBuf,
    tgt_schema_path: &PathBuf,
    record_path: &PathBuf,
    verbose: bool,
) -> Result<()> {
    let migration: Migration = load_json(migration_path)?;
    let record_json: serde_json::Value = load_json(record_path)?;

    if verbose {
        eprintln!(
            "Lifting record through migration ({} vertex mappings)",
            migration.vertex_map.len()
        );
    }

    // Load source and target schemas from provided files so that vertex
    // kinds (object/array/string/etc.) are preserved. This is required
    // for correct parsing and serialization.
    let src_schema: Schema = load_json(src_schema_path)?;
    let tgt_schema: Schema = load_json(tgt_schema_path)?;

    // Compile the migration.
    let compiled = mig::compile(&src_schema, &tgt_schema, &migration)
        .into_diagnostic()
        .wrap_err("failed to compile migration")?;

    // Parse the record as a W-type instance.
    // Pick the root vertex: prefer a vertex with no incoming edges in the
    // source schema; fall back to the lexicographically first vertex.
    let root_vertex = {
        let domain_vertices: std::collections::BTreeSet<&String> =
            migration.vertex_map.keys().collect();
        let targets: std::collections::HashSet<&String> = migration
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

    // Apply the migration.
    let lifted = mig::lift_wtype(&compiled, &src_schema, &tgt_schema, &instance)
        .into_diagnostic()
        .wrap_err("lift operation failed")?;

    // Serialize back to JSON.
    let output = inst::to_json(&tgt_schema, &lifted);
    let pretty = serde_json::to_string_pretty(&output)
        .into_diagnostic()
        .wrap_err("failed to serialize output")?;

    println!("{pretty}");
    Ok(())
}
