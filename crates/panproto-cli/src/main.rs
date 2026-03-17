//! # schema
//!
//! Command-line interface for panproto — schematic version control.
//!
//! Provides subcommands for schema validation, migration checking,
//! breaking change detection, record lifting, and git-like version
//! control for schema evolution.

mod format;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    gat::{Name, Theory},
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

        /// Also type-check the migration morphism at the GAT level.
        #[arg(long)]
        typecheck: bool,
    },

    /// Generate minimal test data from a protocol theory using free model construction.
    Scaffold {
        /// The protocol name (e.g., "atproto").
        #[arg(long)]
        protocol: String,

        /// Path to the schema JSON file.
        schema: PathBuf,

        /// Maximum term generation depth (default: 3).
        #[arg(long, default_value = "3")]
        depth: usize,

        /// Maximum terms per sort (default: 1000).
        #[arg(long, default_value = "1000")]
        max_terms: usize,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Simplify a schema by merging equivalent elements.
    Normalize {
        /// The protocol name (e.g., "atproto").
        #[arg(long)]
        protocol: String,

        /// Path to the schema JSON file.
        schema: PathBuf,

        /// Pairs of elements to identify, as "A=B".
        #[arg(long = "identify", value_delimiter = ',')]
        identifications: Vec<String>,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Type-check a migration between two schemas at the GAT level.
    Typecheck {
        /// Path to the source schema JSON file.
        #[arg(long)]
        src: PathBuf,

        /// Path to the target schema JSON file.
        #[arg(long)]
        tgt: PathBuf,

        /// Path to the migration mapping JSON file.
        #[arg(long)]
        migration: PathBuf,
    },

    /// Verify that a schema satisfies its protocol theory's equations.
    Verify {
        /// The protocol name (e.g., "atproto").
        #[arg(long)]
        protocol: String,

        /// Path to the schema JSON file.
        schema: PathBuf,

        /// Maximum assignments to check per equation (default: 10000).
        #[arg(long, default_value = "10000")]
        max_assignments: usize,
    },

    // -- VCS commands --
    /// Initialize a new panproto repository.
    Init {
        /// Directory to initialize (defaults to current dir).
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Use the given name for the initial branch.
        #[arg(short = 'b', long = "initial-branch")]
        initial_branch: Option<String>,
    },

    /// Stage a schema for the next commit.
    Add {
        /// Path to the schema JSON file.
        schema: PathBuf,

        /// Show what would be staged without actually staging.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Force staging even if validation fails.
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// Create a new commit from staged changes.
    Commit {
        /// Commit message.
        #[arg(short, long)]
        message: String,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,

        /// Amend the previous commit instead of creating a new one.
        #[arg(long)]
        amend: bool,

        /// Allow creating a commit with no changes.
        #[arg(long)]
        allow_empty: bool,

        /// Skip GAT equation verification.
        #[arg(long)]
        skip_verify: bool,
    },

    /// Show repository status.
    Status {
        /// Show output in short format.
        #[arg(short = 's', long)]
        short: bool,

        /// Show output in machine-readable format.
        #[arg(long)]
        porcelain: bool,

        /// Show branch information.
        #[arg(short = 'b', long)]
        branch: bool,
    },

    /// Show commit history.
    Log {
        /// Maximum number of commits to show.
        #[arg(short = 'n', long)]
        limit: Option<usize>,

        /// Show each commit on a single line.
        #[arg(long)]
        oneline: bool,

        /// Show an ASCII graph of the branch structure.
        #[arg(long)]
        graph: bool,

        /// Show all branches, not just the current one.
        #[arg(long)]
        all: bool,

        /// Pretty-print commits using a format string.
        #[arg(long)]
        format: Option<String>,

        /// Filter commits by author.
        #[arg(long)]
        author: Option<String>,

        /// Filter commits whose message matches a pattern.
        #[arg(long)]
        grep: Option<String>,
    },

    /// Diff two schemas or show staged changes.
    Diff {
        /// Path to the old schema (or first ref).
        old: Option<PathBuf>,

        /// Path to the new schema (or second ref).
        new: Option<PathBuf>,

        /// Show a diffstat summary.
        #[arg(long)]
        stat: bool,

        /// Show only names of changed elements.
        #[arg(long)]
        name_only: bool,

        /// Show names and status (A/D/M) of changed elements.
        #[arg(long)]
        name_status: bool,

        /// Diff the staged schema against HEAD.
        #[arg(long, alias = "cached")]
        staged: bool,

        /// Detect likely renames between schemas.
        #[arg(long)]
        detect_renames: bool,

        /// Show theory-level diff (sorts, operations, equations).
        #[arg(long)]
        theory: bool,
    },

    /// Inspect a commit, schema, or migration object.
    Show {
        /// Ref name or object ID.
        target: String,

        /// Pretty-print using a format string.
        #[arg(long)]
        format: Option<String>,

        /// Show a diffstat summary for commits.
        #[arg(long)]
        stat: bool,
    },

    /// Create, list, or delete branches.
    Branch {
        /// Branch name to create. Lists branches if omitted.
        name: Option<String>,

        /// Delete the branch.
        #[arg(short, long)]
        delete: bool,

        /// Force-delete the branch even if not fully merged.
        #[arg(short = 'D')]
        force_delete: bool,

        /// Force overwrite if branch already exists.
        #[arg(short = 'f', long)]
        force: bool,

        /// Rename a branch (value is the new name).
        #[arg(short = 'm', long = "move")]
        rename: Option<String>,

        /// Show commit info for each branch.
        #[arg(short = 'v', long)]
        verbose: bool,

        /// List both local and remote-tracking branches.
        #[arg(short = 'a', long)]
        all: bool,
    },

    /// Create, list, or delete tags.
    Tag {
        /// Tag name to create. Lists tags if omitted.
        name: Option<String>,

        /// Delete the tag.
        #[arg(short, long)]
        delete: bool,

        /// Create an annotated tag.
        #[arg(short = 'a', long)]
        annotate: bool,

        /// Tag message (implies --annotate).
        #[arg(short = 'm', long)]
        message: Option<String>,

        /// List tags matching a pattern.
        #[arg(short = 'l', long)]
        list: bool,

        /// Force-replace an existing tag.
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// Switch to a branch or commit.
    Checkout {
        /// Branch name or commit ID.
        target: String,

        /// Create a new branch with the given name at HEAD and switch to it.
        #[arg(short = 'b')]
        create: bool,

        /// Detach HEAD at the target commit.
        #[arg(long)]
        detach: bool,
    },

    /// Merge a branch into the current branch.
    Merge {
        /// Branch to merge.
        branch: Option<String>,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,

        /// Perform the merge but do not commit.
        #[arg(long)]
        no_commit: bool,

        /// Refuse to merge unless fast-forward is possible.
        #[arg(long)]
        ff_only: bool,

        /// Create a merge commit even for fast-forward merges.
        #[arg(long)]
        no_ff: bool,

        /// Squash the branch into a single change set.
        #[arg(long)]
        squash: bool,

        /// Abort an in-progress merge.
        #[arg(long)]
        abort: bool,

        /// Custom merge commit message.
        #[arg(short = 'm', long)]
        message: Option<String>,

        /// Show pullback-based overlap detection details.
        #[arg(short = 'v', long)]
        verbose: bool,
    },

    /// Replay current branch onto another.
    Rebase {
        /// Branch or commit to rebase onto.
        onto: Option<String>,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,

        /// Abort the current rebase operation.
        #[arg(long)]
        abort: bool,

        /// Continue a paused rebase after resolving conflicts.
        #[arg(long, alias = "continue")]
        cont: bool,
    },

    /// Apply a single commit's migration to the current branch.
    CherryPick {
        /// Commit ID to cherry-pick.
        commit: Option<String>,

        /// Author name.
        #[arg(long, default_value = "anonymous")]
        author: String,

        /// Apply the change without committing.
        #[arg(short = 'n', long)]
        no_commit: bool,

        /// Append "(cherry picked from commit ...)" to the message.
        #[arg(short = 'x')]
        record_origin: bool,

        /// Abort the current cherry-pick operation.
        #[arg(long)]
        abort: bool,
    },

    /// Move HEAD / unstage / restore.
    Reset {
        /// Target ref or commit ID.
        target: String,

        /// Soft reset: move HEAD only, keep staged and working changes.
        #[arg(long)]
        soft: bool,

        /// Hard reset: move HEAD, discard all changes.
        #[arg(long)]
        hard: bool,

        /// Legacy mode flag (hidden, for backward compatibility).
        #[arg(long, hide = true)]
        mode: Option<String>,

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

        /// Show reflogs for all refs.
        #[arg(long)]
        all: bool,
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

        /// Element identifier (vertex ID, edge `"src->tgt"`, or `"vertex_id:sort"`).
        element_id: String,

        /// Walk history from the first commit forward.
        #[arg(long)]
        reverse: bool,
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

        /// Migration direction: restrict (default, `Delta_F`), sigma (`Sigma_F`), or pi (`Pi_F`).
        #[arg(long, default_value = "restrict")]
        direction: String,

        /// Instance type: wtype (default) or functor.
        #[arg(long, default_value = "wtype")]
        instance_type: String,
    },

    /// Integrate two schemas by computing their pushout.
    Integrate {
        /// Path to the left schema JSON file.
        left: PathBuf,
        /// Path to the right schema JSON file.
        right: PathBuf,
        /// Automatically discover the overlap between schemas.
        #[arg(long)]
        auto_overlap: bool,
        /// Output the integrated schema as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Automatically discover a migration between two schemas.
    AutoMigrate {
        /// Path to the old/source schema JSON file.
        old: PathBuf,

        /// Path to the new/target schema JSON file.
        new: PathBuf,

        /// Require injective (one-to-one) vertex mapping.
        #[arg(long)]
        monic: bool,

        /// Output the migration as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Garbage collect unreachable objects.
    Gc {
        /// Show what would be deleted without actually deleting.
        #[arg(long)]
        dry_run: bool,

        /// Prune loose objects older than the default expiry.
        #[arg(long)]
        prune: bool,
    },

    // -- Remote command stubs --
    /// Add, list, or remove remote repositories.
    Remote {
        /// Remote operation.
        #[command(subcommand)]
        action: RemoteAction,
    },

    /// Push schemas to a remote repository.
    Push {
        /// Remote name.
        remote: Option<String>,

        /// Branch to push.
        branch: Option<String>,
    },

    /// Pull schemas from a remote repository.
    Pull {
        /// Remote name.
        remote: Option<String>,

        /// Branch to pull.
        branch: Option<String>,
    },

    /// Fetch schemas from a remote repository.
    Fetch {
        /// Remote name.
        remote: Option<String>,
    },

    /// Clone a remote repository.
    Clone {
        /// Repository URL.
        url: String,

        /// Local path.
        path: Option<PathBuf>,
    },
}

/// Remote sub-operations.
#[derive(Subcommand, Debug)]
enum RemoteAction {
    /// Register a new remote.
    Add {
        /// Remote name.
        name: String,
        /// Remote URL.
        url: String,
    },
    /// Remove a remote.
    Remove {
        /// Remote name to remove.
        name: String,
    },
    /// List configured remotes.
    List,
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
    /// Apply a stash entry without removing it.
    Apply {
        /// Stash index to apply.
        #[arg(default_value = "0")]
        index: usize,
    },
    /// Show the contents of a stash entry.
    Show {
        /// Stash index to inspect.
        #[arg(default_value = "0")]
        index: usize,
    },
    /// Remove all stash entries.
    Clear,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli.command, cli.verbose)
}

/// Dispatch a parsed CLI command to the appropriate handler.
fn dispatch(command: Command, verbose: bool) -> Result<()> {
    match command {
        // Schema tools.
        command @ (Command::Validate { .. }
        | Command::Check { .. }
        | Command::Lift { .. }
        | Command::Integrate { .. }
        | Command::AutoMigrate { .. }
        | Command::Scaffold { .. }
        | Command::Normalize { .. }
        | Command::Typecheck { .. }
        | Command::Verify { .. }) => dispatch_schema_commands(command, verbose),

        // Core VCS commands.
        Command::Init {
            path,
            initial_branch,
        } => cmd_init(&path, initial_branch.as_deref()),
        Command::Add {
            schema,
            dry_run,
            force,
        } => cmd_add(&schema, dry_run, force),
        Command::Commit {
            message,
            author,
            amend,
            allow_empty,
            skip_verify,
        } => cmd_commit(&message, &author, amend, allow_empty, skip_verify),
        Command::Status {
            short,
            porcelain,
            branch,
        } => cmd_status(short, porcelain, branch),
        Command::Log {
            limit,
            oneline,
            graph,
            all,
            format,
            author,
            grep,
        } => cmd_log(
            limit,
            oneline,
            graph,
            all,
            format.as_deref(),
            author.as_deref(),
            grep.as_deref(),
        ),
        Command::Diff {
            old,
            new,
            stat,
            name_only,
            name_status,
            staged,
            detect_renames,
            theory,
        } => cmd_diff(
            old.as_deref(),
            new.as_deref(),
            &DiffOptions {
                stat,
                name_only,
                name_status,
                staged,
                verbose,
                detect_renames,
                theory,
            },
        ),
        Command::Show {
            target,
            format,
            stat,
        } => cmd_show(&target, format.as_deref(), stat),

        // Branching, tagging, and merge commands.
        command @ (Command::Branch { .. }
        | Command::Tag { .. }
        | Command::Checkout { .. }
        | Command::Merge { .. }) => dispatch_branch_commands(command),

        // History rewriting and misc commands.
        command @ (Command::Rebase { .. }
        | Command::CherryPick { .. }
        | Command::Reset { .. }
        | Command::Stash { .. }
        | Command::Reflog { .. }
        | Command::Bisect { .. }
        | Command::Blame { .. }
        | Command::Gc { .. }
        | Command::Remote { .. }
        | Command::Push { .. }
        | Command::Pull { .. }
        | Command::Fetch { .. }
        | Command::Clone { .. }) => dispatch_history_commands(command),
    }
}

/// Dispatch schema tool commands (validate, check, lift, auto-migrate, scaffold, etc.).
fn dispatch_schema_commands(command: Command, verbose: bool) -> Result<()> {
    match command {
        Command::Validate { protocol, schema } => cmd_validate(&protocol, &schema, verbose),
        Command::Check {
            src,
            tgt,
            mapping,
            typecheck,
        } => cmd_check(&src, &tgt, &mapping, verbose, typecheck),
        Command::Scaffold {
            protocol,
            schema,
            depth,
            max_terms,
            json,
        } => cmd_scaffold(&protocol, &schema, depth, max_terms, json, verbose),
        Command::Normalize {
            protocol,
            schema,
            identifications,
            json,
        } => cmd_normalize(&protocol, &schema, &identifications, json, verbose),
        Command::Typecheck {
            src,
            tgt,
            migration,
        } => cmd_typecheck(&src, &tgt, &migration, verbose),
        Command::Verify {
            protocol,
            schema,
            max_assignments,
        } => cmd_verify(&protocol, &schema, max_assignments, verbose),
        Command::Lift {
            migration,
            src_schema,
            tgt_schema,
            record,
            direction,
            instance_type,
        } => cmd_lift(
            &migration,
            &src_schema,
            &tgt_schema,
            &record,
            &direction,
            &instance_type,
            verbose,
        ),
        Command::Integrate {
            left,
            right,
            auto_overlap,
            json,
        } => cmd_integrate(&left, &right, auto_overlap, json, verbose),
        Command::AutoMigrate {
            old,
            new,
            monic,
            json,
        } => cmd_auto_migrate(&old, &new, monic, json, verbose),
        _ => unreachable!(),
    }
}

/// Dispatch branching, tagging, checkout, and merge commands.
fn dispatch_branch_commands(command: Command) -> Result<()> {
    match command {
        Command::Branch {
            name,
            delete,
            force_delete,
            force,
            rename,
            verbose,
            all,
        } => cmd_branch(&BranchCmdOptions {
            name: name.as_deref(),
            delete,
            force_delete,
            force,
            rename: rename.as_deref(),
            verbose,
            all,
        }),
        Command::Tag {
            name,
            delete,
            annotate,
            message,
            list,
            force,
        } => cmd_tag(&TagCmdOptions {
            name: name.as_deref(),
            delete,
            annotate,
            message: message.as_deref(),
            list,
            force,
        }),
        Command::Checkout {
            target,
            create,
            detach,
        } => cmd_checkout(&target, create, detach),
        Command::Merge {
            branch,
            author,
            no_commit,
            ff_only,
            no_ff,
            squash,
            abort,
            message,
            verbose,
        } => cmd_merge(&MergeCmdOptions {
            branch: branch.as_deref(),
            author: &author,
            no_commit,
            ff_only,
            no_ff,
            squash,
            abort,
            message: message.as_deref(),
            verbose,
        }),
        _ => unreachable!(),
    }
}

/// Dispatch history rewriting and miscellaneous commands.
fn dispatch_history_commands(command: Command) -> Result<()> {
    match command {
        Command::Rebase {
            onto,
            author,
            abort,
            cont,
        } => cmd_rebase(onto.as_deref(), &author, abort, cont),
        Command::CherryPick {
            commit,
            author,
            no_commit,
            record_origin,
            abort,
        } => cmd_cherry_pick(commit.as_deref(), &author, no_commit, record_origin, abort),
        Command::Reset {
            target,
            soft,
            hard,
            mode,
            author,
        } => cmd_reset(&target, soft, hard, mode.as_deref(), &author),
        Command::Stash { action } => cmd_stash(action),
        Command::Reflog {
            ref_name,
            limit,
            all,
        } => cmd_reflog(&ref_name, limit, all),
        Command::Bisect { good, bad } => cmd_bisect(&good, &bad),
        Command::Blame {
            element_type,
            element_id,
            reverse,
        } => cmd_blame(&element_type, &element_id, reverse),
        Command::Gc { dry_run, prune } => cmd_gc(dry_run, prune),
        Command::Remote { action } => cmd_remote(action),
        Command::Push { remote, branch } => cmd_push(remote.as_deref(), branch.as_deref()),
        Command::Pull { remote, branch } => cmd_pull(remote.as_deref(), branch.as_deref()),
        Command::Fetch { remote } => cmd_fetch(remote.as_deref()),
        Command::Clone { url, path } => cmd_clone(&url, path.as_deref()),
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load and parse a JSON file into a typed value.
fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
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

fn cmd_validate(protocol_name: &str, schema_path: &Path, verbose: bool) -> Result<()> {
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

fn cmd_check(
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

fn cmd_lift(
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

fn cmd_lift_functor(
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

fn cmd_auto_migrate(
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

fn cmd_integrate(
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

fn cmd_scaffold(
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
            .wrap_err_with(|| format!("free model construction failed for theory '{name}'"))?;

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

fn cmd_normalize(
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

fn cmd_typecheck(
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

/// Build a model from a schema for a given theory.
///
/// Maps vertex-like sorts to schema vertex IDs and edge-like sorts to
/// schema edge representations. Other sorts get a small free model carrier.
fn build_schema_model(
    schema: &Schema,
    name: &str,
    theory: &panproto_core::gat::Theory,
) -> panproto_core::gat::Model {
    use panproto_core::gat::{GatError, ModelValue};

    let mut model = panproto_core::gat::Model::new(name);
    for sort in &theory.sorts {
        let sort_lower = sort.name.to_lowercase();
        let carrier: Vec<ModelValue> = if sort_lower.contains("vertex")
            || sort_lower.contains("node")
            || sort_lower.contains("object")
        {
            schema
                .vertices
                .keys()
                .map(|k| ModelValue::Str(k.to_string()))
                .collect()
        } else if sort_lower.contains("edge")
            || sort_lower.contains("arrow")
            || sort_lower.contains("morphism")
        {
            schema
                .edges
                .keys()
                .map(|e| {
                    let label = e.name.as_deref().unwrap_or("");
                    ModelValue::Str(format!("{}→{} {label}", e.src, e.tgt))
                })
                .collect()
        } else {
            let config = panproto_core::gat::FreeModelConfig {
                max_depth: 2,
                max_terms_per_sort: 100,
            };
            panproto_core::gat::free_model(theory, &config).map_or_else(
                |_| Vec::new(),
                |fm| {
                    fm.sort_interp
                        .get(&sort.name.to_string())
                        .cloned()
                        .unwrap_or_default()
                },
            )
        };
        model.add_sort(sort.name.to_string(), carrier);
    }
    for op in &theory.ops {
        let op_name = op.name.to_string();
        let arity = op.arity();
        model.add_op(op_name.clone(), move |args: &[ModelValue]| {
            if args.len() != arity {
                return Err(GatError::ModelError(format!(
                    "operation '{op_name}' expects {arity} args, got {}",
                    args.len()
                )));
            }
            let arg_strs: Vec<&str> = args
                .iter()
                .map(|a| match a {
                    ModelValue::Str(s) => s.as_str(),
                    _ => "?",
                })
                .collect();
            Ok(ModelValue::Str(format!(
                "{op_name}({})",
                arg_strs.join(", ")
            )))
        });
    }
    model
}

fn cmd_verify(
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

/// Print a theory-level diff between two schemas (sorts/operations at the GAT level).
fn print_theory_diff(old_schema: &Schema, new_schema: &Schema) {
    type EdgeKey = (String, String, Option<String>);

    // Treat vertex IDs as sorts and edges as operations.
    let old_sorts: std::collections::BTreeSet<&str> =
        old_schema.vertices.keys().map(Name::as_str).collect();
    let new_sorts: std::collections::BTreeSet<&str> =
        new_schema.vertices.keys().map(Name::as_str).collect();

    let added_sorts: Vec<&&str> = new_sorts.difference(&old_sorts).collect();
    let removed_sorts: Vec<&&str> = old_sorts.difference(&new_sorts).collect();

    let edge_key = |e: &panproto_core::schema::Edge| -> EdgeKey {
        (
            e.src.to_string(),
            e.tgt.to_string(),
            e.name.as_ref().map(ToString::to_string),
        )
    };
    let old_edges: std::collections::BTreeSet<EdgeKey> =
        old_schema.edges.keys().map(edge_key).collect();
    let new_edges: std::collections::BTreeSet<EdgeKey> =
        new_schema.edges.keys().map(edge_key).collect();

    let added_ops: Vec<&EdgeKey> = new_edges.difference(&old_edges).collect();
    let removed_ops: Vec<&EdgeKey> = old_edges.difference(&new_edges).collect();

    if added_sorts.is_empty()
        && removed_sorts.is_empty()
        && added_ops.is_empty()
        && removed_ops.is_empty()
    {
        println!("\nTheory diff: no changes.");
        return;
    }

    println!("\nTheory-level diff:");
    for s in &added_sorts {
        println!("  + sort {s}");
    }
    for s in &removed_sorts {
        println!("  - sort {s}");
    }
    for (src, tgt, name) in &added_ops {
        let label = name.as_deref().unwrap_or("");
        println!("  + op {src} -> {tgt} {label}");
    }
    for (src, tgt, name) in &removed_ops {
        let label = name.as_deref().unwrap_or("");
        println!("  - op {src} -> {tgt} {label}");
    }
}

// ---------------------------------------------------------------------------
// VCS commands
// ---------------------------------------------------------------------------

fn cmd_init(path: &Path, initial_branch: Option<&str>) -> Result<()> {
    let mut repo = vcs::Repository::init(path)
        .into_diagnostic()
        .wrap_err("failed to initialize repository")?;
    if let Some(branch_name) = initial_branch {
        vcs::refs::rename_branch(repo.store_mut(), "main", branch_name).into_diagnostic()?;
    }
    let branch = initial_branch.unwrap_or("main");
    println!(
        "Initialized empty panproto repository in {} (branch: {branch})",
        path.join(".panproto").display()
    );
    Ok(())
}

fn cmd_add(schema_path: &Path, dry_run: bool, force: bool) -> Result<()> {
    let schema: Schema = load_json(schema_path)?;

    if dry_run {
        println!(
            "Would stage schema from {} ({} vertices, {} edges)",
            schema_path.display(),
            schema.vertex_count(),
            schema.edge_count()
        );
        return Ok(());
    }

    let mut repo = open_repo()?;
    let _ = force; // reserved for skipping validation in the future
    repo.add(&schema)
        .into_diagnostic()
        .wrap_err("failed to stage schema")?;
    println!("Staged schema from {}", schema_path.display());
    Ok(())
}

fn cmd_commit(
    message: &str,
    author: &str,
    amend: bool,
    allow_empty: bool,
    skip_verify: bool,
) -> Result<()> {
    let mut repo = open_repo()?;
    let _ = allow_empty; // placeholder for future use

    if amend {
        let commit_id = repo
            .amend(message, author)
            .into_diagnostic()
            .wrap_err("failed to amend commit")?;
        println!("[{}] (amended) {message}", commit_id.short());
    } else {
        let opts = vcs::CommitOptions { skip_verify };
        let commit_id = repo
            .commit_with_options(message, author, &opts)
            .into_diagnostic()
            .wrap_err("failed to commit")?;
        println!("[{}] {message}", commit_id.short());
    }
    Ok(())
}

fn cmd_status(short: bool, porcelain: bool, show_branch: bool) -> Result<()> {
    let repo = open_repo()?;
    let head = repo.store().get_head().into_diagnostic()?;

    if porcelain {
        // Machine-readable output.
        match &head {
            vcs::HeadState::Branch(name) => println!("## {name}"),
            vcs::HeadState::Detached(id) => println!("## HEAD (detached) {}", id.short()),
        }
        return Ok(());
    }

    if short {
        match &head {
            vcs::HeadState::Branch(name) => {
                if show_branch {
                    println!("## {name}");
                }
            }
            vcs::HeadState::Detached(id) => {
                if show_branch {
                    println!("## HEAD (detached) {}", id.short());
                }
            }
        }
        return Ok(());
    }

    // Default (long) format.
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

fn cmd_log(
    limit: Option<usize>,
    oneline: bool,
    _graph: bool,
    _all: bool,
    fmt: Option<&str>,
    filter_author: Option<&str>,
    filter_grep: Option<&str>,
) -> Result<()> {
    let repo = open_repo()?;
    let commits = repo.log(limit).into_diagnostic()?;

    for commit in &commits {
        // Apply filters.
        if let Some(author_pat) = filter_author {
            if !commit.author.contains(author_pat) {
                continue;
            }
        }
        if let Some(grep_pat) = filter_grep {
            if !commit.message.contains(grep_pat) {
                continue;
            }
        }

        if let Some(fmt_str) = fmt {
            println!("{}", format::format_commit(commit, fmt_str)?);
            continue;
        }

        if oneline {
            println!("{}", format::format_commit_oneline(commit)?);
            continue;
        }

        // Default format.
        let schema_short = commit.schema_id.short();
        println!(
            "commit {} (schema {})",
            vcs::hash::hash_commit(commit).into_diagnostic()?,
            schema_short
        );
        println!("Author: {}", commit.author);
        println!("Date:   {}", format_timestamp(commit.timestamp));
        if commit.parents.len() > 1 {
            let parents: Vec<String> = commit.parents.iter().map(vcs::ObjectId::short).collect();
            println!("Merge:  {}", parents.join(" "));
        }
        println!();
        println!("    {}", commit.message);
        println!();
    }

    Ok(())
}

/// Options controlling diff output format.
#[allow(clippy::struct_excessive_bools)]
struct DiffOptions {
    stat: bool,
    name_only: bool,
    name_status: bool,
    staged: bool,
    verbose: bool,
    detect_renames: bool,
    theory: bool,
}

fn cmd_diff(old_path: Option<&Path>, new_path: Option<&Path>, opts: &DiffOptions) -> Result<()> {
    let DiffOptions {
        stat,
        name_only,
        name_status,
        staged,
        verbose,
        detect_renames,
        theory,
    } = *opts;
    if staged {
        // Diff staged schema vs HEAD.
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
            print_theory_diff(&old_schema, &new_schema);
        }
        return Ok(());
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
    Ok(())
}

/// Print detected vertex and edge renames between two schemas.
fn print_detected_renames(old_schema: &Schema, new_schema: &Schema) {
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

fn print_diff(
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

fn cmd_show(target: &str, fmt: Option<&str>, stat: bool) -> Result<()> {
    let repo = open_repo()?;
    let id = vcs::refs::resolve_ref(repo.store(), target)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{target}'"))?;

    let object = repo.store().get(&id).into_diagnostic()?;
    match object {
        vcs::Object::Commit(c) => {
            if let Some(fmt_str) = fmt {
                println!("{}", format::format_commit(&c, fmt_str)?);
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
            println!("Date:      {}", format_timestamp(c.timestamp));
            println!("\n    {}", c.message);

            if stat {
                // Show diff stat between parent and this commit.
                if let Some(parent_id) = c.parents.first() {
                    let parent_obj = repo.store().get(parent_id).into_diagnostic()?;
                    if let vcs::Object::Commit(parent_commit) = parent_obj {
                        let old_obj = repo
                            .store()
                            .get(&parent_commit.schema_id)
                            .into_diagnostic()?;
                        let new_obj = repo.store().get(&c.schema_id).into_diagnostic()?;
                        if let (vcs::Object::Schema(old_s), vcs::Object::Schema(new_s)) =
                            (old_obj, new_obj)
                        {
                            let d = panproto_core::check::diff::diff(&old_s, &new_s);
                            println!("\n {}", format::format_diff_stat(&d));
                        }
                    }
                }
            }
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
        vcs::Object::Tag(tag) => {
            println!("tag {id}");
            println!("Target:    {}", tag.target);
            println!("Tagger:    {}", tag.tagger);
            println!("Date:      {}", format_timestamp(tag.timestamp));
            println!("\n    {}", tag.message);
        }
    }
    Ok(())
}

/// Options for the `branch` subcommand.
#[allow(clippy::struct_excessive_bools)]
struct BranchCmdOptions<'a> {
    name: Option<&'a str>,
    delete: bool,
    force_delete: bool,
    force: bool,
    rename: Option<&'a str>,
    verbose: bool,
    #[allow(dead_code)]
    all: bool,
}

fn cmd_branch(opts: &BranchCmdOptions<'_>) -> Result<()> {
    let BranchCmdOptions {
        name,
        delete,
        force_delete,
        force,
        rename,
        verbose,
        all: _,
    } = *opts;

    let mut repo = open_repo()?;

    // Handle rename.
    if let Some(new_name) = rename {
        let old_name = name.ok_or_else(|| miette::miette!("branch name required for rename"))?;
        vcs::refs::rename_branch(repo.store_mut(), old_name, new_name).into_diagnostic()?;
        println!("Renamed branch {old_name} -> {new_name}");
        return Ok(());
    }

    // Handle force-delete.
    if force_delete {
        let branch_name = name.ok_or_else(|| miette::miette!("branch name required for -D"))?;
        vcs::refs::force_delete_branch(repo.store_mut(), branch_name).into_diagnostic()?;
        println!("Deleted branch {branch_name} (force)");
        return Ok(());
    }

    // Handle normal delete (also force-delete if -f is set).
    if delete {
        let branch_name = name.ok_or_else(|| miette::miette!("branch name required for delete"))?;
        if force {
            vcs::refs::force_delete_branch(repo.store_mut(), branch_name).into_diagnostic()?;
            println!("Deleted branch {branch_name} (force)");
        } else {
            vcs::refs::delete_branch(repo.store_mut(), branch_name).into_diagnostic()?;
            println!("Deleted branch {branch_name}");
        }
        return Ok(());
    }

    // Create or list.
    if let Some(name) = name {
        let head_id = vcs::store::resolve_head(repo.store())
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("no commits yet"))?;
        vcs::refs::create_branch(repo.store_mut(), name, head_id).into_diagnostic()?;
        println!("Created branch {name} at {}", head_id.short());
    } else {
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
            if verbose {
                let obj = repo.store().get(id).into_diagnostic()?;
                if let vcs::Object::Commit(c) = obj {
                    println!("{marker}{branch_name} {} {}", id.short(), c.message);
                } else {
                    println!("{marker}{branch_name} {}", id.short());
                }
            } else {
                println!("{marker}{branch_name} {}", id.short());
            }
        }
    }
    Ok(())
}

/// Options for the `tag` subcommand.
#[allow(clippy::struct_excessive_bools)]
struct TagCmdOptions<'a> {
    name: Option<&'a str>,
    delete: bool,
    annotate: bool,
    message: Option<&'a str>,
    list: bool,
    force: bool,
}

fn cmd_tag(opts: &TagCmdOptions<'_>) -> Result<()> {
    let TagCmdOptions {
        name,
        delete,
        annotate,
        message,
        list,
        force,
    } = *opts;
    let mut repo = open_repo()?;

    // Explicit list mode.
    if list || (name.is_none() && !delete) {
        let tags = vcs::refs::list_tags(repo.store()).into_diagnostic()?;
        for (tag_name, id) in &tags {
            println!("{tag_name} {}", id.short());
        }
        return Ok(());
    }

    let tag_name = name.ok_or_else(|| miette::miette!("tag name required"))?;

    if delete {
        vcs::refs::delete_tag(repo.store_mut(), tag_name).into_diagnostic()?;
        println!("Deleted tag {tag_name}");
        return Ok(());
    }

    let head_id = vcs::store::resolve_head(repo.store())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no commits yet"))?;

    // Annotated tag if -a or -m is provided.
    if annotate || message.is_some() {
        let msg = message.unwrap_or("");
        vcs::refs::create_annotated_tag(repo.store_mut(), tag_name, head_id, "anonymous", msg)
            .into_diagnostic()?;
        println!("Tagged {} as {tag_name} (annotated)", head_id.short());
    } else if force {
        vcs::refs::create_tag_force(repo.store_mut(), tag_name, head_id).into_diagnostic()?;
        println!("Tagged {} as {tag_name} (force)", head_id.short());
    } else {
        vcs::refs::create_tag(repo.store_mut(), tag_name, head_id).into_diagnostic()?;
        println!("Tagged {} as {tag_name}", head_id.short());
    }

    Ok(())
}

fn cmd_checkout(target: &str, create: bool, detach: bool) -> Result<()> {
    let mut repo = open_repo()?;

    if create {
        // Create a new branch at HEAD and switch to it.
        let head_id = vcs::store::resolve_head(repo.store())
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("no commits yet"))?;
        vcs::refs::create_and_checkout_branch(repo.store_mut(), target, head_id)
            .into_diagnostic()?;
        println!("Switched to a new branch '{target}'");
        return Ok(());
    }

    if detach {
        let id = vcs::refs::resolve_ref(repo.store(), target)
            .into_diagnostic()
            .wrap_err_with(|| format!("cannot resolve '{target}'"))?;
        vcs::refs::checkout_detached(repo.store_mut(), id).into_diagnostic()?;
        println!("HEAD is now at {}", id.short());
        return Ok(());
    }

    // Try branch first.
    let branch_ref = format!("refs/heads/{target}");
    if repo
        .store()
        .get_ref(&branch_ref)
        .into_diagnostic()?
        .is_some()
    {
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

/// Options for the `merge` subcommand.
#[allow(clippy::struct_excessive_bools)]
struct MergeCmdOptions<'a> {
    branch: Option<&'a str>,
    author: &'a str,
    no_commit: bool,
    ff_only: bool,
    no_ff: bool,
    squash: bool,
    abort: bool,
    message: Option<&'a str>,
    verbose: bool,
}

fn cmd_merge(cmd_opts: &MergeCmdOptions<'_>) -> Result<()> {
    let MergeCmdOptions {
        branch,
        author,
        no_commit,
        ff_only,
        no_ff,
        squash,
        abort,
        message,
        verbose: _verbose,
    } = *cmd_opts;

    if abort {
        // Abort an in-progress merge. Clear any merge state files.
        let repo = open_repo()?;
        let merge_head = repo.store().root().join("MERGE_HEAD");
        if merge_head.exists() {
            std::fs::remove_file(&merge_head).into_diagnostic()?;
        }
        println!("Merge aborted.");
        return Ok(());
    }

    let branch_name = branch.ok_or_else(|| miette::miette!("branch name required for merge"))?;
    let mut repo = open_repo()?;

    let opts = vcs::merge::MergeOptions {
        no_commit,
        ff_only,
        no_ff,
        squash,
        message: message.map(ToOwned::to_owned),
    };

    let result = repo
        .merge_with_options(branch_name, author, &opts)
        .into_diagnostic()?;

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

    if cmd_opts.verbose {
        if let Some(ref overlap) = result.pullback_overlap {
            println!("\nPullback overlap detection:");
            if overlap.shared_vertices.is_empty() {
                println!("  No shared vertices detected.");
            } else {
                println!("  {} shared vertex(es):", overlap.shared_vertices.len());
                let mut sorted: Vec<_> = overlap.shared_vertices.iter().collect();
                sorted.sort();
                for v in sorted {
                    println!("    {v}");
                }
            }
            if !overlap.shared_edges.is_empty() {
                println!("  {} shared edge(s):", overlap.shared_edges.len());
                let mut sorted: Vec<_> = overlap.shared_edges.iter().collect();
                sorted.sort();
                for (src, tgt) in sorted {
                    println!("    {src} -> {tgt}");
                }
            }
        }
    }

    Ok(())
}

fn cmd_rebase(onto: Option<&str>, author: &str, abort: bool, cont: bool) -> Result<()> {
    if abort {
        miette::bail!("rebase --abort is not yet implemented");
    }
    if cont {
        miette::bail!("rebase --continue is not yet implemented");
    }

    let onto_name = onto.ok_or_else(|| miette::miette!("target branch required for rebase"))?;
    let mut repo = open_repo()?;
    let onto_id = vcs::refs::resolve_ref(repo.store(), onto_name)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{onto_name}'"))?;
    let new_tip = repo.rebase(onto_id, author).into_diagnostic()?;
    println!("Rebased onto {onto_name}. New tip: {}", new_tip.short());
    Ok(())
}

fn cmd_cherry_pick(
    commit: Option<&str>,
    author: &str,
    no_commit: bool,
    record_origin: bool,
    abort: bool,
) -> Result<()> {
    if abort {
        miette::bail!("cherry-pick --abort is not yet implemented");
    }

    let commit_ref = commit.ok_or_else(|| miette::miette!("commit ID required for cherry-pick"))?;
    let mut repo = open_repo()?;
    let commit_id = vcs::refs::resolve_ref(repo.store(), commit_ref)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{commit_ref}'"))?;

    let opts = vcs::cherry_pick::CherryPickOptions {
        no_commit,
        record_origin,
    };

    let new_id =
        vcs::cherry_pick::cherry_pick_with_options(repo.store_mut(), commit_id, author, &opts)
            .into_diagnostic()?;
    println!("Cherry-picked {} -> {}", commit_id.short(), new_id.short());
    Ok(())
}

fn cmd_reset(
    target: &str,
    soft: bool,
    hard: bool,
    legacy_mode: Option<&str>,
    author: &str,
) -> Result<()> {
    let mut repo = open_repo()?;
    let target_id = vcs::refs::resolve_ref(repo.store(), target)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{target}'"))?;

    let (reset_mode, mode_label) = if let Some(m) = legacy_mode {
        // Backward-compatible --mode flag.
        let rm = match m {
            "soft" => vcs::reset::ResetMode::Soft,
            "mixed" => vcs::reset::ResetMode::Mixed,
            "hard" => vcs::reset::ResetMode::Hard,
            _ => miette::bail!("invalid reset mode: {m}. Use: soft, mixed, hard"),
        };
        (rm, m.to_owned())
    } else if soft {
        (vcs::reset::ResetMode::Soft, "soft".to_owned())
    } else if hard {
        (vcs::reset::ResetMode::Hard, "hard".to_owned())
    } else {
        (vcs::reset::ResetMode::Mixed, "mixed".to_owned())
    };

    let outcome = repo
        .reset(target_id, reset_mode, author)
        .into_diagnostic()?;
    println!(
        "HEAD is now at {} (mode: {mode_label})",
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
            let staged = index
                .staged
                .ok_or_else(|| miette::miette!("nothing staged to stash"))?;

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
        StashAction::Apply { index } => {
            let schema_id = vcs::stash::stash_apply(repo.store(), index).into_diagnostic()?;
            println!("Applied stash@{{{index}}} (schema {})", schema_id.short());
        }
        StashAction::Show { index } => {
            let info = vcs::stash::stash_show(repo.store(), index).into_diagnostic()?;
            println!("stash@{{{index}}}: {info}");
        }
        StashAction::Clear => {
            vcs::stash::stash_clear(repo.store_mut()).into_diagnostic()?;
            println!("Cleared all stash entries.");
        }
    }
    Ok(())
}

fn cmd_reflog(ref_name: &str, limit: Option<usize>, all: bool) -> Result<()> {
    let repo = open_repo()?;

    if all {
        // Show reflogs for all branches.
        let branches = vcs::refs::list_branches(repo.store()).into_diagnostic()?;
        for (branch_name, _) in &branches {
            let r = format!("refs/heads/{branch_name}");
            let entries = repo.store().read_reflog(&r, limit).into_diagnostic()?;
            for (i, entry) in entries.iter().enumerate() {
                let old = entry
                    .old_id
                    .map_or_else(|| "0000000".to_owned(), |id| id.short());
                println!(
                    "{r}@{{{i}}} {} -> {} {}",
                    old,
                    entry.new_id.short(),
                    entry.message
                );
            }
        }
        return Ok(());
    }

    let entries = repo
        .store()
        .read_reflog(ref_name, limit)
        .into_diagnostic()?;

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

    let (state, step) =
        vcs::bisect::bisect_start(repo.store(), good_id, bad_id).into_diagnostic()?;

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

fn cmd_blame(element_type: &str, element_id: &str, reverse: bool) -> Result<()> {
    if reverse {
        eprintln!("note: --reverse blame is not yet implemented; falling back to standard blame");
    }

    let repo = open_repo()?;
    let head_id = vcs::store::resolve_head(repo.store())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no commits yet"))?;

    let entry = match element_type {
        "vertex" => {
            vcs::blame::blame_vertex(repo.store(), head_id, element_id).into_diagnostic()?
        }
        "edge" => {
            // Parse "src->tgt" or "src->tgt:kind:name".
            let parts: Vec<&str> = element_id.split("->").collect();
            if parts.len() != 2 {
                miette::bail!("edge format: src->tgt or src->tgt:kind:name");
            }
            let sub_parts: Vec<&str> = parts[1].split(':').collect();
            let edge = panproto_core::schema::Edge {
                src: Name::from(parts[0]),
                tgt: Name::from(sub_parts[0]),
                kind: Name::from(*sub_parts.get(1).unwrap_or(&"prop")),
                name: sub_parts.get(2).map(|s| Name::from(*s)),
            };
            vcs::blame::blame_edge(repo.store(), head_id, &edge).into_diagnostic()?
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

    println!(
        "{} {} {}",
        entry.commit_id.short(),
        entry.author,
        entry.message
    );
    println!("Date: {}", format_timestamp(entry.timestamp));
    Ok(())
}

fn cmd_gc(dry_run: bool, _prune: bool) -> Result<()> {
    let mut repo = open_repo()?;

    if dry_run {
        let opts = vcs::gc::GcOptions { dry_run: true };
        let report = vcs::gc::gc_with_options(repo.store_mut(), &opts).into_diagnostic()?;
        println!(
            "Reachable objects: {}. Would delete: {}.",
            report.reachable,
            report.deleted.len()
        );
    } else {
        let report = repo.gc().into_diagnostic()?;
        println!(
            "Reachable objects: {}. Deleted: {}.",
            report.reachable,
            report.deleted.len()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Remote stubs (not yet implemented)
// ---------------------------------------------------------------------------

fn cmd_remote(_action: RemoteAction) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

fn cmd_push(_remote: Option<&str>, _branch: Option<&str>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

fn cmd_pull(_remote: Option<&str>, _branch: Option<&str>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

fn cmd_fetch(_remote: Option<&str>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

fn cmd_clone(_url: &str, _path: Option<&Path>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
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
const fn days_to_ymd(days: u64) -> (u64, u64, u64) {
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
