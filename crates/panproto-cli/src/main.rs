//! # schema
//!
//! Command-line interface for panproto — schematic version control.
//!
//! Provides subcommands for schema validation, migration checking,
//! breaking change detection, record lifting, and git-like version
//! control for schema evolution.

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
    vcs::{self, Store as _},
};

/// The panproto command-line tool for schema migration and version control.
#[derive(Parser, Debug)]
#[command(
    name = "schema",
    version,
    about = "Schematic version control — schema migration toolkit based on generalized algebraic theories"
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
    // -- Schema tools (pre-VCS) --
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

    // -- VCS commands --
    /// Initialize a new panproto repository.
    Init {
        /// Directory to initialize (defaults to current dir).
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Stage a schema for the next commit.
    Add {
        /// Path to the schema JSON file.
        schema: PathBuf,
    },

    /// Create a new commit from staged changes.
    Commit {
        /// Commit message.
        #[arg(short, long)]
        message: String,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,
    },

    /// Show repository status.
    Status,

    /// Show commit history.
    Log {
        /// Maximum number of commits to show.
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },

    /// Diff two schemas or show staged changes.
    Diff {
        /// Path to the old schema (or first ref).
        old: PathBuf,

        /// Path to the new schema (or second ref).
        new: PathBuf,
    },

    /// Inspect a commit, schema, or migration object.
    Show {
        /// Ref name or object ID.
        target: String,
    },

    /// Create, list, or delete branches.
    Branch {
        /// Branch name to create. Lists branches if omitted.
        name: Option<String>,

        /// Delete the branch.
        #[arg(short, long)]
        delete: bool,
    },

    /// Create, list, or delete tags.
    Tag {
        /// Tag name to create. Lists tags if omitted.
        name: Option<String>,

        /// Delete the tag.
        #[arg(short, long)]
        delete: bool,
    },

    /// Switch to a branch or commit.
    Checkout {
        /// Branch name or commit ID.
        target: String,
    },

    /// Merge a branch into the current branch.
    Merge {
        /// Branch to merge.
        branch: String,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,
    },

    /// Replay current branch onto another.
    Rebase {
        /// Branch or commit to rebase onto.
        onto: String,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,
    },

    /// Apply a single commit's migration to the current branch.
    CherryPick {
        /// Commit ID to cherry-pick.
        commit: String,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,
    },

    /// Move HEAD / unstage / restore.
    Reset {
        /// Target ref or commit ID.
        target: String,

        /// Reset mode.
        #[arg(long, default_value = "mixed")]
        mode: String,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,
    },

    /// Save or restore working state.
    Stash {
        /// Stash operation: push, pop, list, drop.
        #[command(subcommand)]
        action: StashAction,
    },

    /// Show ref mutation history.
    Reflog {
        /// Ref name (defaults to HEAD).
        #[arg(default_value = "HEAD")]
        ref_name: String,

        /// Maximum entries to show.
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },

    /// Binary search for the commit that introduced a breaking change.
    Bisect {
        /// Known good commit.
        good: String,

        /// Known bad commit.
        bad: String,
    },

    /// Show which commit introduced a schema element.
    Blame {
        /// Element type: vertex, edge, or constraint.
        #[arg(long)]
        element_type: String,

        /// Element identifier (vertex ID, edge "src->tgt", or "vertex_id:sort").
        element_id: String,
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

    /// Garbage collect unreachable objects.
    Gc,
}

/// Stash sub-operations.
#[derive(Subcommand, Debug)]
enum StashAction {
    /// Save the current staged schema.
    Push {
        /// Optional stash message.
        #[arg(short, long)]
        message: Option<String>,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,
    },
    /// Restore the most recent stash.
    Pop,
    /// List all stash entries.
    List,
    /// Drop the most recent stash.
    Drop,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // Schema tools.
        Command::Validate { protocol, schema } => cmd_validate(&protocol, &schema, cli.verbose),
        Command::Check { src, tgt, mapping } => cmd_check(&src, &tgt, &mapping, cli.verbose),
        Command::Lift {
            migration,
            src_schema,
            tgt_schema,
            record,
        } => cmd_lift(&migration, &src_schema, &tgt_schema, &record, cli.verbose),

        // VCS commands.
        Command::Init { path } => cmd_init(&path),
        Command::Add { schema } => cmd_add(&schema),
        Command::Commit { message, author } => cmd_commit(&message, &author),
        Command::Status => cmd_status(),
        Command::Log { limit } => cmd_log(limit),
        Command::Diff { old, new } => cmd_diff(&old, &new, cli.verbose),
        Command::Show { target } => cmd_show(&target),
        Command::Branch { name, delete } => cmd_branch(name.as_deref(), delete),
        Command::Tag { name, delete } => cmd_tag(name.as_deref(), delete),
        Command::Checkout { target } => cmd_checkout(&target),
        Command::Merge { branch, author } => cmd_merge(&branch, &author),
        Command::Rebase { onto, author } => cmd_rebase(&onto, &author),
        Command::CherryPick { commit, author } => cmd_cherry_pick(&commit, &author),
        Command::Reset {
            target,
            mode,
            author,
        } => cmd_reset(&target, &mode, &author),
        Command::Stash { action } => cmd_stash(action),
        Command::Reflog { ref_name, limit } => cmd_reflog(&ref_name, limit),
        Command::Bisect { good, bad } => cmd_bisect(&good, &bad),
        Command::Blame {
            element_type,
            element_id,
        } => cmd_blame(&element_type, &element_id),
        Command::Gc => cmd_gc(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Open a VCS repository from the current directory (or parent search).
fn open_repo() -> Result<vcs::Repository> {
    // Try current directory first.
    let cwd = std::env::current_dir().into_diagnostic()?;
    vcs::Repository::open(&cwd)
        .into_diagnostic()
        .wrap_err("not a panproto repository (or any parent up to mount point)")
}

// ---------------------------------------------------------------------------
// Schema tool commands (pre-VCS)
// ---------------------------------------------------------------------------

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
        miette::bail!("schema validation failed with {} error(s)", errors.len());
    }
}

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

    let src_schema: Schema = load_json(src_schema_path)?;
    let tgt_schema: Schema = load_json(tgt_schema_path)?;

    let compiled = mig::compile(&src_schema, &tgt_schema, &migration)
        .into_diagnostic()
        .wrap_err("failed to compile migration")?;

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

    let lifted = mig::lift_wtype(&compiled, &src_schema, &tgt_schema, &instance)
        .into_diagnostic()
        .wrap_err("lift operation failed")?;

    let output = inst::to_json(&tgt_schema, &lifted);
    let pretty = serde_json::to_string_pretty(&output)
        .into_diagnostic()
        .wrap_err("failed to serialize output")?;

    println!("{pretty}");
    Ok(())
}

// ---------------------------------------------------------------------------
// VCS commands
// ---------------------------------------------------------------------------

fn cmd_init(path: &PathBuf) -> Result<()> {
    vcs::Repository::init(path)
        .into_diagnostic()
        .wrap_err("failed to initialize repository")?;
    println!("Initialized empty panproto repository in {}", path.join(".panproto").display());
    Ok(())
}

fn cmd_add(schema_path: &PathBuf) -> Result<()> {
    let schema: Schema = load_json(schema_path)?;
    let mut repo = open_repo()?;
    repo.add(&schema)
        .into_diagnostic()
        .wrap_err("failed to stage schema")?;
    println!("Staged schema from {}", schema_path.display());
    Ok(())
}

fn cmd_commit(message: &str, author: &str) -> Result<()> {
    let mut repo = open_repo()?;
    let commit_id = repo
        .commit(message, author)
        .into_diagnostic()
        .wrap_err("failed to commit")?;
    println!("[{}] {message}", commit_id.short());
    Ok(())
}

fn cmd_status() -> Result<()> {
    let repo = open_repo()?;
    let head = repo.store().get_head().into_diagnostic()?;

    match &head {
        vcs::HeadState::Branch(name) => {
            let head_id = vcs::store::resolve_head(repo.store()).into_diagnostic()?;
            match head_id {
                Some(id) => println!("On branch {name} ({id})"),
                None => println!("On branch {name} (no commits yet)"),
            }
        }
        vcs::HeadState::Detached(id) => println!("HEAD detached at {id}"),
    }

    Ok(())
}

fn cmd_log(limit: Option<usize>) -> Result<()> {
    let repo = open_repo()?;
    let commits = repo.log(limit).into_diagnostic()?;

    for commit in &commits {
        let schema_short = commit.schema_id.short();
        println!("commit {} (schema {})", vcs::hash::hash_commit(commit).into_diagnostic()?, schema_short);
        println!("Author: {}", commit.author);
        println!("Date:   {}", format_timestamp(commit.timestamp));
        if commit.parents.len() > 1 {
            let parents: Vec<String> = commit.parents.iter().map(|p| p.short()).collect();
            println!("Merge:  {}", parents.join(" "));
        }
        println!();
        println!("    {}", commit.message);
        println!();
    }

    Ok(())
}

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

    let schema_diff = panproto_core::check::diff::diff(&old_schema, &new_schema);

    if schema_diff.is_empty() {
        println!("Schemas are identical.");
        return Ok(());
    }

    let total = schema_diff.added_vertices.len()
        + schema_diff.removed_vertices.len()
        + schema_diff.added_edges.len()
        + schema_diff.removed_edges.len()
        + schema_diff.kind_changes.len()
        + schema_diff.modified_constraints.len();
    println!("{total} change(s) detected:\n");

    for v in &schema_diff.added_vertices {
        let kind = new_schema.vertices.get(v).map_or("?", |vtx| &vtx.kind);
        println!("  + vertex {v} ({kind})");
    }
    for v in &schema_diff.removed_vertices {
        let kind = old_schema.vertices.get(v).map_or("?", |vtx| &vtx.kind);
        println!("  - vertex {v} ({kind})");
    }
    for kc in &schema_diff.kind_changes {
        println!("  ~ vertex {}: {} -> {}", kc.vertex_id, kc.old_kind, kc.new_kind);
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
            println!("  ~ constraint {vid}: {} = {} -> {}", c.sort, c.old_value, c.new_value);
        }
    }

    Ok(())
}

fn cmd_show(target: &str) -> Result<()> {
    let repo = open_repo()?;
    let id = vcs::refs::resolve_ref(repo.store(), target)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{target}'"))?;

    let object = repo.store().get(&id).into_diagnostic()?;
    match object {
        vcs::Object::Commit(c) => {
            println!("commit {id}");
            println!("Schema:    {}", c.schema_id);
            println!("Parents:   {}", c.parents.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "));
            if let Some(mig_id) = c.migration_id {
                println!("Migration: {mig_id}");
            }
            println!("Protocol:  {}", c.protocol);
            println!("Author:    {}", c.author);
            println!("Date:      {}", format_timestamp(c.timestamp));
            println!("\n    {}", c.message);
        }
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
    }
    Ok(())
}

fn cmd_branch(name: Option<&str>, delete: bool) -> Result<()> {
    let mut repo = open_repo()?;

    match (name, delete) {
        (Some(name), true) => {
            vcs::refs::delete_branch(repo.store_mut(), name).into_diagnostic()?;
            println!("Deleted branch {name}");
        }
        (Some(name), false) => {
            let head_id = vcs::store::resolve_head(repo.store())
                .into_diagnostic()?
                .ok_or_else(|| miette::miette!("no commits yet"))?;
            vcs::refs::create_branch(repo.store_mut(), name, head_id).into_diagnostic()?;
            println!("Created branch {name} at {}", head_id.short());
        }
        (None, _) => {
            let branches = vcs::refs::list_branches(repo.store()).into_diagnostic()?;
            let current = match repo.store().get_head().into_diagnostic()? {
                vcs::HeadState::Branch(name) => Some(name),
                vcs::HeadState::Detached(_) => None,
            };
            for (branch_name, id) in &branches {
                let marker = if current.as_deref() == Some(branch_name) {
                    "* "
                } else {
                    "  "
                };
                println!("{marker}{branch_name} {}", id.short());
            }
        }
    }
    Ok(())
}

fn cmd_tag(name: Option<&str>, delete: bool) -> Result<()> {
    let mut repo = open_repo()?;

    match (name, delete) {
        (Some(name), true) => {
            vcs::refs::delete_tag(repo.store_mut(), name).into_diagnostic()?;
            println!("Deleted tag {name}");
        }
        (Some(name), false) => {
            let head_id = vcs::store::resolve_head(repo.store())
                .into_diagnostic()?
                .ok_or_else(|| miette::miette!("no commits yet"))?;
            vcs::refs::create_tag(repo.store_mut(), name, head_id).into_diagnostic()?;
            println!("Tagged {} as {name}", head_id.short());
        }
        (None, _) => {
            let tags = vcs::refs::list_tags(repo.store()).into_diagnostic()?;
            for (tag_name, id) in &tags {
                println!("{tag_name} {}", id.short());
            }
        }
    }
    Ok(())
}

fn cmd_checkout(target: &str) -> Result<()> {
    let mut repo = open_repo()?;

    // Try branch first.
    let branch_ref = format!("refs/heads/{target}");
    if repo.store().get_ref(&branch_ref).into_diagnostic()?.is_some() {
        vcs::refs::checkout_branch(repo.store_mut(), target).into_diagnostic()?;
        println!("Switched to branch '{target}'");
    } else {
        let id = vcs::refs::resolve_ref(repo.store(), target)
            .into_diagnostic()
            .wrap_err_with(|| format!("cannot resolve '{target}'"))?;
        vcs::refs::checkout_detached(repo.store_mut(), id).into_diagnostic()?;
        println!("HEAD is now at {}", id.short());
    }
    Ok(())
}

fn cmd_merge(branch: &str, author: &str) -> Result<()> {
    let mut repo = open_repo()?;
    let result = repo.merge(branch, author).into_diagnostic()?;

    if result.conflicts.is_empty() {
        println!("Merge successful.");
        println!(
            "Merged schema has {} vertices, {} edges.",
            result.merged_schema.vertex_count(),
            result.merged_schema.edge_count()
        );
    } else {
        println!("Merge produced {} conflict(s):", result.conflicts.len());
        for conflict in &result.conflicts {
            println!("  {conflict:?}");
        }
        miette::bail!("merge failed with {} conflict(s)", result.conflicts.len());
    }
    Ok(())
}

fn cmd_rebase(onto: &str, author: &str) -> Result<()> {
    let mut repo = open_repo()?;
    let onto_id = vcs::refs::resolve_ref(repo.store(), onto)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{onto}'"))?;
    let new_tip = repo.rebase(onto_id, author).into_diagnostic()?;
    println!("Rebased onto {onto}. New tip: {}", new_tip.short());
    Ok(())
}

fn cmd_cherry_pick(commit: &str, author: &str) -> Result<()> {
    let mut repo = open_repo()?;
    let commit_id = vcs::refs::resolve_ref(repo.store(), commit)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{commit}'"))?;
    let new_id = repo.cherry_pick(commit_id, author).into_diagnostic()?;
    println!("Cherry-picked {} -> {}", commit_id.short(), new_id.short());
    Ok(())
}

fn cmd_reset(target: &str, mode: &str, author: &str) -> Result<()> {
    let mut repo = open_repo()?;
    let target_id = vcs::refs::resolve_ref(repo.store(), target)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{target}'"))?;

    let reset_mode = match mode {
        "soft" => vcs::reset::ResetMode::Soft,
        "mixed" => vcs::reset::ResetMode::Mixed,
        "hard" => vcs::reset::ResetMode::Hard,
        _ => miette::bail!("invalid reset mode: {mode}. Use: soft, mixed, hard"),
    };

    let outcome = repo.reset(target_id, reset_mode, author).into_diagnostic()?;
    println!(
        "HEAD is now at {} (mode: {mode})",
        outcome.new_head.short()
    );
    Ok(())
}

fn cmd_stash(action: StashAction) -> Result<()> {
    let mut repo = open_repo()?;

    match action {
        StashAction::Push { message, author } => {
            // Read the current index to find the staged schema.
            let index_path = repo.store().root().join("index.json");
            if !index_path.exists() {
                miette::bail!("nothing staged to stash");
            }
            let index: vcs::Index = load_json(&index_path)?;
            let staged = index.staged.ok_or_else(|| miette::miette!("nothing staged to stash"))?;

            let stash_id = vcs::stash::stash_push(
                repo.store_mut(),
                staged.schema_id,
                &author,
                message.as_deref(),
            )
            .into_diagnostic()?;
            println!("Saved working state ({})", stash_id.short());
        }
        StashAction::Pop => {
            let schema_id = vcs::stash::stash_pop(repo.store_mut()).into_diagnostic()?;
            println!("Restored stash (schema {})", schema_id.short());
        }
        StashAction::List => {
            let entries = vcs::stash::stash_list(repo.store()).into_diagnostic()?;
            if entries.is_empty() {
                println!("No stash entries.");
            } else {
                for entry in &entries {
                    println!(
                        "stash@{{{}}} {}: {}",
                        entry.index,
                        entry.commit_id.short(),
                        entry.message
                    );
                }
            }
        }
        StashAction::Drop => {
            vcs::stash::stash_drop(repo.store_mut(), 0).into_diagnostic()?;
            println!("Dropped stash@{{0}}");
        }
    }
    Ok(())
}

fn cmd_reflog(ref_name: &str, limit: Option<usize>) -> Result<()> {
    let repo = open_repo()?;
    let entries = repo.store().read_reflog(ref_name, limit).into_diagnostic()?;

    if entries.is_empty() {
        println!("No reflog entries for {ref_name}.");
        return Ok(());
    }

    for (i, entry) in entries.iter().enumerate() {
        let old = entry
            .old_id
            .map_or_else(|| "0000000".to_owned(), |id| id.short());
        println!(
            "{ref_name}@{{{i}}} {} -> {} {}",
            old,
            entry.new_id.short(),
            entry.message
        );
    }
    Ok(())
}

fn cmd_bisect(good: &str, bad: &str) -> Result<()> {
    let repo = open_repo()?;
    let good_id = vcs::refs::resolve_ref(repo.store(), good)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{good}'"))?;
    let bad_id = vcs::refs::resolve_ref(repo.store(), bad)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{bad}'"))?;

    let (state, step) = vcs::bisect::bisect_start(repo.store(), good_id, bad_id)
        .into_diagnostic()?;

    match step {
        vcs::bisect::BisectStep::Found(id) => {
            println!("Breaking commit: {id}");
        }
        vcs::bisect::BisectStep::Test(id) => {
            println!("Test commit: {id}");
            println!(
                "Remaining steps: ~{}",
                vcs::bisect::bisect_remaining(&state)
            );
            println!("Use `prot show {id}` to inspect, then re-run bisect with narrowed range.");
        }
    }
    Ok(())
}

fn cmd_blame(element_type: &str, element_id: &str) -> Result<()> {
    let repo = open_repo()?;
    let head_id = vcs::store::resolve_head(repo.store())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no commits yet"))?;

    let entry = match element_type {
        "vertex" => vcs::blame::blame_vertex(repo.store(), head_id, element_id)
            .into_diagnostic()?,
        "edge" => {
            // Parse "src->tgt" or "src->tgt:kind:name".
            let parts: Vec<&str> = element_id.split("->").collect();
            if parts.len() != 2 {
                miette::bail!("edge format: src->tgt or src->tgt:kind:name");
            }
            let sub_parts: Vec<&str> = parts[1].split(':').collect();
            let edge = panproto_core::schema::Edge {
                src: parts[0].to_owned(),
                tgt: sub_parts[0].to_owned(),
                kind: sub_parts.get(1).unwrap_or(&"prop").to_string(),
                name: sub_parts.get(2).map(|s| s.to_string()),
            };
            vcs::blame::blame_edge(repo.store(), head_id, &edge)
                .into_diagnostic()?
        }
        "constraint" => {
            // Parse "vertex_id:sort".
            let parts: Vec<&str> = element_id.split(':').collect();
            if parts.len() != 2 {
                miette::bail!("constraint format: vertex_id:sort");
            }
            vcs::blame::blame_constraint(repo.store(), head_id, parts[0], parts[1])
                .into_diagnostic()?
        }
        _ => miette::bail!("unknown element type: {element_type}. Use: vertex, edge, constraint"),
    };

    println!("{} {} {}", entry.commit_id.short(), entry.author, entry.message);
    println!("Date: {}", format_timestamp(entry.timestamp));
    Ok(())
}

fn cmd_gc() -> Result<()> {
    let mut repo = open_repo()?;
    let report = repo.gc().into_diagnostic()?;
    println!(
        "Reachable objects: {}. Deleted: {}.",
        report.reachable,
        report.deleted.len()
    );
    Ok(())
}

fn format_timestamp(ts: u64) -> String {
    // Simple UTC formatting without external deps.
    let secs = ts;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate date from days since epoch (1970-01-01).
    let (year, month, day) = days_to_ymd(days);
    format!("{year}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02} UTC")
}

/// Convert days since epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
