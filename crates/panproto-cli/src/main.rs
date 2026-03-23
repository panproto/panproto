//! # schema
//!
//! Command-line interface for panproto — schematic version control.
//!
//! Provides subcommands for schema validation, migration checking,
//! breaking change detection, record lifting, and git-like version
//! control for schema evolution.

mod cmd;
mod format;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use miette::Result;

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

        /// Stage data files alongside the schema.
        #[arg(long)]
        data: Option<PathBuf>,
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

        /// Show data staleness for files in this directory.
        #[arg(long)]
        data: Option<PathBuf>,
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

        /// Show data and complement IDs in commit history.
        #[arg(long)]
        data: bool,
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

        /// Also generate a protolens chain between the schemas.
        #[arg(long)]
        lens: bool,

        /// Save the protolens chain to a file (requires --lens).
        #[arg(long)]
        save: Option<PathBuf>,

        /// Show the optic classification of the diff.
        #[arg(long)]
        optic_kind: bool,
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

        /// Migrate data in this directory to match the target branch's schema.
        #[arg(long)]
        migrate: Option<PathBuf>,
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

        /// Migrate data in this directory through the merge.
        #[arg(long)]
        migrate: Option<PathBuf>,
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
    },

    // -- Expression operations --
    /// Evaluate, type-check, or interactively explore GAT expressions.
    Expr {
        /// Expression operation.
        #[command(subcommand)]
        action: ExprAction,
    },

    // -- Schema enrichment --
    /// Add, list, or remove schema enrichments (defaults, coercions, mergers, policies).
    Enrich {
        /// Enrichment operation.
        #[command(subcommand)]
        action: EnrichAction,
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

    // -- Data migration --
    /// Migrate data to match the current schema version.
    ///
    /// Examples:
    ///   schema migrate records/
    ///   schema migrate records/ --range HEAD~3..HEAD
    ///   schema migrate records/ --dry-run
    ///   schema migrate records/ --backward
    ///   schema migrate records/ -o migrated/
    Migrate {
        /// Data directory containing JSON files.
        data: PathBuf,
        /// Protocol name (inferred from HEAD commit if omitted).
        #[arg(long)]
        protocol: Option<String>,
        /// Migrate between specific commits (default: parent..HEAD).
        #[arg(long)]
        range: Option<String>,
        /// Preview without modifying files.
        #[arg(long)]
        dry_run: bool,
        /// Output directory (default: overwrite in place).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Migrate backward (requires stored complement).
        #[arg(long)]
        backward: bool,
        /// Apply migration and print coverage statistics.
        #[arg(long)]
        coverage: bool,
    },

    /// Convert data between schemas. Works on single files or directories.
    ///
    /// Examples:
    ///   schema convert record.json --from old.json --to new.json --protocol atproto
    ///   schema convert records/ --from old.json --to new.json -o migrated/ --protocol atproto
    ///   schema convert records/ --chain policy.json -o migrated/ --protocol atproto
    Convert {
        /// Data file or directory of JSON files.
        data: PathBuf,
        /// Source schema (required unless --chain is used).
        #[arg(long)]
        from: Option<PathBuf>,
        /// Target schema (required unless --chain is used).
        #[arg(long)]
        to: Option<PathBuf>,
        /// Protocol name.
        #[arg(long)]
        protocol: String,
        /// Pre-built protolens chain JSON (alternative to --from/--to).
        #[arg(long)]
        chain: Option<PathBuf>,
        /// Output file or directory (default: stdout for single file).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Direction: "forward" or "backward".
        #[arg(long, default_value = "forward")]
        direction: String,
        /// Default values as key=value pairs.
        #[arg(long, value_delimiter = ',')]
        defaults: Vec<String>,
    },

    // -- Lens operations --
    /// Generate, inspect, and manage bidirectional lenses.
    ///
    /// By default, generates a lens between two schemas and prints a
    /// human-readable summary. Use flags for other operations.
    ///
    /// Examples:
    ///   schema lens old.json new.json --protocol atproto
    ///   schema lens old.json new.json --protocol atproto --apply data.json
    ///   schema lens old.json new.json --protocol atproto --chain > chain.json
    ///   schema lens --apply chain.json data.json --protocol atproto
    ///   schema lens --compose chain1.json chain2.json --protocol atproto
    ///   schema lens --verify chain.json --data test.json --protocol atproto
    ///   schema lens --check chain.json schemas/ --protocol atproto
    ///   schema lens --lift chain.json morphism.json --protocol atproto
    Lens {
        /// Positional arguments (schemas, chains, or morphisms depending on mode).
        args: Vec<PathBuf>,
        /// Protocol name.
        #[arg(long)]
        protocol: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Output a reusable protolens chain (JSON to stdout).
        #[arg(long)]
        chain: bool,
        /// Show complement requirements (defaults/data needed).
        #[arg(long)]
        requirements: bool,
        /// Fuse multi-step chain into single protolens.
        #[arg(long)]
        fuse: bool,
        /// Try overlap-based alignment when direct morphism fails.
        #[arg(long)]
        try_overlap: bool,
        /// Default values as key=value pairs.
        #[arg(long, value_delimiter = ',')]
        defaults: Vec<String>,
        /// Apply lens to data: --apply data.json (with two schemas)
        /// or positional chain + data (with --apply alone).
        #[arg(long)]
        apply: Option<PathBuf>,
        /// Verify lens laws on test data.
        #[arg(long)]
        verify: Option<PathBuf>,
        /// Compose two chains (positional args are the two chain files).
        #[arg(long)]
        compose: bool,
        /// Check applicability against schemas in a directory.
        #[arg(long)]
        check: bool,
        /// Lift a chain along a theory morphism.
        #[arg(long)]
        lift: bool,
        /// Save the generated protolens chain to a file.
        #[arg(long)]
        save: Option<PathBuf>,
        /// Schema for chain instantiation (with --apply on a saved chain).
        #[arg(long)]
        schema: Option<PathBuf>,
        /// Direction for --apply: "forward" or "backward".
        #[arg(long, default_value = "forward")]
        direction: String,
        /// Complement data for backward --apply.
        #[arg(long)]
        complement: Option<PathBuf>,
        /// Dry-run for --check (report only, don't instantiate).
        #[arg(long)]
        dry_run: bool,
        /// Test data for --verify.
        #[arg(long)]
        data: Option<PathBuf>,
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

/// Expression sub-operations.
#[derive(Subcommand, Debug)]
enum ExprAction {
    /// Evaluate a JSON-encoded GAT term from a file.
    GatEval {
        /// Path to the JSON file containing a GAT term.
        file: PathBuf,

        /// Path to a JSON file with variable bindings.
        #[arg(long)]
        env: Option<PathBuf>,
    },
    /// Type-check a JSON-encoded GAT term from a file.
    GatCheck {
        /// Path to the JSON file containing term, theory, and context.
        file: PathBuf,
    },
    /// Interactive expression REPL.
    Repl,
    /// Parse a Haskell-style expression and print its AST.
    Parse {
        /// Expression source text.
        source: String,
    },
    /// Parse and evaluate a Haskell-style expression, printing the result.
    Eval {
        /// Expression source text.
        source: String,
    },
    /// Parse an expression and pretty-print it back in canonical form.
    Fmt {
        /// Expression source text.
        source: String,
    },
    /// Parse an expression and report any syntax errors.
    Check {
        /// Expression source text.
        source: String,
    },
}

/// Enrichment sub-operations.
#[derive(Subcommand, Debug)]
enum EnrichAction {
    /// Add a default value expression to a vertex.
    AddDefault {
        /// Vertex name.
        vertex: String,
        /// Default value as JSON.
        #[arg(long)]
        expr: String,
    },
    /// Add a coercion expression between two vertex kinds.
    AddCoercion {
        /// Source vertex kind.
        from: String,
        /// Target vertex kind.
        to: String,
        /// Coercion expression as JSON.
        #[arg(long)]
        expr: String,
    },
    /// Add a merger expression to a vertex.
    AddMerger {
        /// Vertex name.
        vertex: String,
        /// Merger specification as JSON.
        #[arg(long)]
        expr: String,
    },
    /// Add a conflict policy to a vertex.
    AddPolicy {
        /// Vertex name.
        vertex: String,
        /// Conflict resolution strategy name.
        #[arg(long)]
        strategy: String,
    },
    /// List all enrichments on the HEAD schema.
    List,
    /// Remove an enrichment by name.
    Remove {
        /// Enrichment name or vertex name to remove enrichments from.
        name: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli.command, cli.verbose)
}

/// Dispatch a parsed CLI command to the appropriate handler.
#[allow(clippy::too_many_lines)]
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
        | Command::Verify { .. }
        | Command::Convert { .. }
        | Command::Migrate { .. }
        | Command::Lens { .. }) => dispatch_schema_commands(command, verbose),

        // Core VCS commands.
        Command::Init {
            path,
            initial_branch,
        } => cmd::vcs::cmd_init(&path, initial_branch.as_deref()),
        Command::Add {
            schema,
            dry_run,
            force,
            data,
        } => cmd::vcs::cmd_add(&schema, dry_run, force, data.as_deref(), verbose),
        Command::Commit {
            message,
            author,
            amend,
            allow_empty,
            skip_verify,
        } => cmd::vcs::cmd_commit(&message, &author, amend, allow_empty, skip_verify),
        Command::Status {
            short,
            porcelain,
            branch,
            data,
        } => cmd::vcs::cmd_status(short, porcelain, branch, data.as_deref()),
        Command::Log {
            limit,
            oneline,
            graph: _graph,
            all: _all,
            format,
            author,
            grep,
            data,
        } => cmd::vcs::cmd_log(&cmd::vcs::LogCmdOptions {
            limit,
            oneline,
            fmt: format.as_deref(),
            filter_author: author.as_deref(),
            filter_grep: grep.as_deref(),
            show_data: data,
        }),
        Command::Diff {
            old,
            new,
            stat,
            name_only,
            name_status,
            staged,
            detect_renames,
            theory,
            lens,
            save,
            optic_kind,
        } => {
            let result = cmd::schema::cmd_diff(
                old.as_deref(),
                new.as_deref(),
                &cmd::schema::DiffOptions {
                    stat,
                    name_only,
                    name_status,
                    staged,
                    verbose,
                    detect_renames,
                    theory,
                    optic_kind,
                },
            );
            if lens {
                if let (Some(old_path), Some(new_path)) = (old.as_deref(), new.as_deref()) {
                    // Generate a protolens chain between the two schemas/refs
                    // Reuse cmd_lens_diff for VCS refs, or generate directly for files
                    let range = format!(
                        "{old}..{new}",
                        old = old_path.display(),
                        new = new_path.display(),
                    );
                    cmd::lens::cmd_lens_diff(&range, true, save.as_deref(), verbose)?;
                }
            }
            result
        }
        Command::Show {
            target,
            format,
            stat,
        } => cmd::schema::cmd_show(&target, format.as_deref(), stat),

        // Expression commands.
        Command::Expr { action } => dispatch_expr_commands(action, verbose),

        // Enrichment commands.
        Command::Enrich { action } => dispatch_enrich_commands(action, verbose),

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
#[allow(clippy::too_many_lines)]
fn dispatch_schema_commands(command: Command, verbose: bool) -> Result<()> {
    match command {
        Command::Validate { protocol, schema } => {
            cmd::schema::cmd_validate(&protocol, &schema, verbose)
        }
        Command::Check {
            src,
            tgt,
            mapping,
            typecheck,
        } => cmd::schema::cmd_check(&src, &tgt, &mapping, verbose, typecheck),
        Command::Scaffold {
            protocol,
            schema,
            depth,
            max_terms,
            json,
        } => cmd::schema::cmd_scaffold(&protocol, &schema, depth, max_terms, json, verbose),
        Command::Normalize {
            protocol,
            schema,
            identifications,
            json,
        } => cmd::schema::cmd_normalize(&protocol, &schema, &identifications, json, verbose),
        Command::Typecheck {
            src,
            tgt,
            migration,
        } => cmd::schema::cmd_typecheck(&src, &tgt, &migration, verbose),
        Command::Verify {
            protocol,
            schema,
            max_assignments,
        } => cmd::schema::cmd_verify(&protocol, &schema, max_assignments, verbose),
        Command::Lift {
            migration,
            src_schema,
            tgt_schema,
            record,
            direction,
            instance_type,
        } => cmd::schema::cmd_lift(
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
        } => cmd::schema::cmd_integrate(&left, &right, auto_overlap, json, verbose),
        Command::AutoMigrate {
            old,
            new,
            monic,
            json,
        } => cmd::schema::cmd_auto_migrate(&old, &new, monic, json, verbose),
        Command::Migrate {
            data,
            protocol,
            range,
            dry_run,
            output,
            backward,
            coverage,
        } => cmd::migrate::cmd_migrate(
            &data,
            protocol.as_deref(),
            range.as_deref(),
            dry_run,
            output.as_deref(),
            backward,
            verbose,
        )
        .and_then(|()| {
            if coverage {
                cmd::migrate::cmd_migrate_coverage(
                    &data,
                    protocol.as_deref(),
                    range.as_deref(),
                    verbose,
                )
            } else {
                Ok(())
            }
        }),
        Command::Convert {
            data,
            from,
            to,
            protocol,
            chain,
            output,
            direction,
            defaults,
        } => cmd::convert::cmd_convert(
            &data,
            from.as_deref(),
            to.as_deref(),
            &protocol,
            chain.as_deref(),
            output.as_deref(),
            &direction,
            &defaults,
            verbose,
        ),
        Command::Lens {
            args,
            protocol,
            json,
            chain,
            requirements,
            fuse,
            try_overlap,
            defaults,
            apply,
            verify,
            compose,
            check,
            lift,
            save,
            schema,
            direction,
            complement,
            dry_run,
            data,
        } => cmd::lens::cmd_lens(
            &args,
            &protocol,
            json,
            chain,
            requirements,
            fuse,
            try_overlap,
            &defaults,
            apply.as_deref(),
            verify.as_deref(),
            compose,
            check,
            lift,
            save.as_deref(),
            schema.as_deref(),
            &direction,
            complement.as_deref(),
            dry_run,
            data.as_deref(),
            verbose,
        ),
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
        } => cmd::branch::cmd_branch(&cmd::branch::BranchCmdOptions {
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
        } => cmd::branch::cmd_tag(&cmd::branch::TagCmdOptions {
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
            migrate,
        } => cmd::branch::cmd_checkout(&target, create, detach, migrate.as_deref()),
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
            migrate,
        } => cmd::branch::cmd_merge(
            &cmd::branch::MergeCmdOptions {
                branch: branch.as_deref(),
                author: &author,
                no_commit,
                ff_only,
                no_ff,
                squash,
                abort,
                message: message.as_deref(),
                verbose,
            },
            migrate.as_deref(),
        ),
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
        } => cmd::history::cmd_rebase(onto.as_deref(), &author, abort, cont),
        Command::CherryPick {
            commit,
            author,
            no_commit,
            record_origin,
            abort,
        } => cmd::history::cmd_cherry_pick(
            commit.as_deref(),
            &author,
            no_commit,
            record_origin,
            abort,
        ),
        Command::Reset {
            target,
            soft,
            hard,
            mode,
            author,
        } => cmd::history::cmd_reset(&target, soft, hard, mode.as_deref(), &author),
        Command::Stash { action } => cmd::history::cmd_stash(action),
        Command::Reflog {
            ref_name,
            limit,
            all,
        } => cmd::history::cmd_reflog(&ref_name, limit, all),
        Command::Bisect { good, bad } => cmd::history::cmd_bisect(&good, &bad),
        Command::Blame {
            element_type,
            element_id,
            reverse,
        } => cmd::history::cmd_blame(&element_type, &element_id, reverse),
        Command::Gc { dry_run } => cmd::history::cmd_gc(dry_run),
        Command::Remote { action } => cmd::history::cmd_remote(action),
        Command::Push { remote, branch } => {
            cmd::history::cmd_push(remote.as_deref(), branch.as_deref())
        }
        Command::Pull { remote, branch } => {
            cmd::history::cmd_pull(remote.as_deref(), branch.as_deref())
        }
        Command::Fetch { remote } => cmd::history::cmd_fetch(remote.as_deref()),
        Command::Clone { url, path } => cmd::history::cmd_clone(&url, path.as_deref()),
        _ => unreachable!(),
    }
}

/// Dispatch expression subcommands.
fn dispatch_expr_commands(action: ExprAction, verbose: bool) -> Result<()> {
    match action {
        ExprAction::GatEval { file, env } => {
            cmd::expr::cmd_expr_gat_eval(&file, env.as_deref(), verbose)
        }
        ExprAction::GatCheck { file } => cmd::expr::cmd_expr_gat_check(&file, verbose),
        ExprAction::Repl => cmd::expr::cmd_expr_repl(),
        ExprAction::Parse { source } => cmd::expr::cmd_expr_parse(&source, verbose),
        ExprAction::Eval { source } => cmd::expr::cmd_expr_eval_source(&source, verbose),
        ExprAction::Fmt { source } => cmd::expr::cmd_expr_fmt(&source, verbose),
        ExprAction::Check { source } => cmd::expr::cmd_expr_check_source(&source, verbose),
    }
}

/// Dispatch enrichment subcommands.
fn dispatch_enrich_commands(action: EnrichAction, verbose: bool) -> Result<()> {
    match action {
        EnrichAction::AddDefault { vertex, expr } => {
            cmd::enrich::cmd_enrich_add_default(&vertex, &expr, verbose)
        }
        EnrichAction::AddCoercion { from, to, expr } => {
            cmd::enrich::cmd_enrich_add_coercion(&from, &to, &expr, verbose)
        }
        EnrichAction::AddMerger { vertex, expr } => {
            cmd::enrich::cmd_enrich_add_merger(&vertex, &expr, verbose)
        }
        EnrichAction::AddPolicy { vertex, strategy } => {
            cmd::enrich::cmd_enrich_add_policy(&vertex, &strategy, verbose)
        }
        EnrichAction::List => cmd::enrich::cmd_enrich_list(verbose),
        EnrichAction::Remove { name } => cmd::enrich::cmd_enrich_remove(&name, verbose),
    }
}
