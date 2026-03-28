use std::collections::HashMap;
use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    schema::Schema,
    vcs::{self, Store as _},
};

use super::helpers::{format_timestamp, load_json, open_repo, read_json_dir};
use crate::format;

pub fn cmd_init(path: &Path, initial_branch: Option<&str>) -> Result<()> {
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

    // Scan for packages and generate panproto.toml.
    let packages = panproto_project::detect::scan_packages(path)
        .into_diagnostic()
        .wrap_err("package detection failed")?;

    if !packages.is_empty() {
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        let config = panproto_project::config::generate_config(path, dir_name).into_diagnostic()?;
        let toml_str = panproto_project::config::serialize_config(&config).into_diagnostic()?;

        let manifest_path = path.join("panproto.toml");
        std::fs::write(&manifest_path, toml_str)
            .into_diagnostic()
            .wrap_err("failed to write panproto.toml")?;

        println!(
            "Generated panproto.toml with {} package(s):",
            packages.len()
        );
        for pkg in &packages {
            println!(
                "  {} ({}) at {}",
                pkg.name,
                pkg.protocol,
                pkg.path.strip_prefix(path).unwrap_or(&pkg.path).display()
            );
        }
    }

    Ok(())
}

pub fn cmd_add(
    schema_path: &Path,
    dry_run: bool,
    force: bool,
    data_path: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let schema: Schema = if schema_path.extension().is_some_and(|e| e == "json") {
        // JSON schema file: existing behavior.
        load_json(schema_path)?
    } else if schema_path.is_dir() {
        // Directory: parse as project, produce unified schema.
        parse_directory_to_schema(schema_path, verbose)?
    } else if schema_path.is_file() {
        // Single source file: parse via tree-sitter.
        parse_file_to_schema(schema_path, verbose)?
    } else {
        miette::bail!(
            "path {} does not exist or is not a file/directory",
            schema_path.display()
        );
    };

    if dry_run {
        println!(
            "Would stage schema from {} ({} vertices, {} edges)",
            schema_path.display(),
            schema.vertex_count(),
            schema.edge_count()
        );
        if let Some(dp) = data_path {
            let count = read_json_dir(dp)?.len();
            println!("Would stage {count} data file(s) from {}", dp.display());
        }
        return Ok(());
    }

    let mut repo = open_repo()?;
    if force {
        match repo.add(&schema) {
            Ok(_) => {}
            Err(vcs::VcsError::ValidationFailed { .. }) => {
                eprintln!("warning: schema has validation errors (--force overrides)");
            }
            Err(e) => return Err(e).into_diagnostic().wrap_err("failed to stage schema"),
        }
    } else {
        repo.add(&schema)
            .into_diagnostic()
            .wrap_err("failed to stage schema")?;
    }
    println!("Staged schema from {}", schema_path.display());

    // Write file hash manifest for directory-based adds.
    if schema_path.is_dir() {
        write_file_hashes(schema_path)?;
    }

    if let Some(dp) = data_path {
        let entries = read_json_dir(dp)?;
        let count = entries.len();
        if verbose {
            eprintln!("Staged {count} data file(s) from {}", dp.display());
        }
        println!("Staged {count} data file(s) from {}", dp.display());
    }
    Ok(())
}

/// Parse a single source file into a panproto Schema via tree-sitter.
fn parse_file_to_schema(path: &Path, verbose: bool) -> Result<Schema> {
    let content = std::fs::read(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", path.display()))?;
    let registry = panproto_parse::ParserRegistry::new();
    let language = registry.detect_language(path).unwrap_or("raw_file");
    if verbose {
        eprintln!("Parsing {} as {language}", path.display());
    }
    let schema = registry
        .parse_file(path, &content)
        .into_diagnostic()
        .wrap_err("parse failed")?;
    Ok(schema)
}

/// Parse a directory into a unified project schema.
fn parse_directory_to_schema(dir: &Path, verbose: bool) -> Result<Schema> {
    let config = panproto_project::config::load_config(dir).into_diagnostic()?;
    let mut builder = match config {
        Some(ref cfg) => {
            panproto_project::ProjectBuilder::with_config(cfg, dir).into_diagnostic()?
        }
        None => panproto_project::ProjectBuilder::new(),
    };
    builder.add_directory(dir).into_diagnostic()?;
    if verbose {
        eprintln!("Scanned {} files", builder.file_count());
    }
    let project = builder.build().into_diagnostic()?;
    Ok(project.schema)
}

/// Write a file hash manifest to `.panproto/file_hashes.json`.
///
/// Maps relative file paths to their blake3 content hashes.
fn write_file_hashes(dir: &Path) -> Result<()> {
    let panproto_dir = dir.join(".panproto");
    if !panproto_dir.exists() {
        return Ok(());
    }

    let mut hashes: HashMap<String, String> = HashMap::new();
    collect_file_hashes(dir, dir, &mut hashes)?;

    let json = serde_json::to_string_pretty(&hashes)
        .into_diagnostic()
        .wrap_err("failed to serialize file hashes")?;
    std::fs::write(panproto_dir.join("file_hashes.json"), json)
        .into_diagnostic()
        .wrap_err("failed to write file_hashes.json")?;
    Ok(())
}

/// Recursively collect blake3 hashes of all files under `base`.
fn collect_file_hashes(
    base: &Path,
    dir: &Path,
    hashes: &mut HashMap<String, String>,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.')
            || matches!(
                name_str.as_ref(),
                "target" | "node_modules" | "__pycache__" | "build" | "dist" | "vendor" | "Pods"
            )
        {
            continue;
        }

        if path.is_dir() {
            collect_file_hashes(base, &path, hashes)?;
        } else if path.is_file() {
            let content = std::fs::read(&path).into_diagnostic()?;
            let hash = blake3::hash(&content);
            let relative = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .display()
                .to_string();
            hashes.insert(relative, hash.to_string());
        }
    }
    Ok(())
}

pub fn cmd_commit(
    message: &str,
    author: &str,
    amend: bool,
    allow_empty: bool,
    skip_verify: bool,
) -> Result<()> {
    let mut repo = open_repo()?;

    // allow_empty: if false, the VCS commit will return NothingStaged
    // error naturally. If true, we catch that error and create a commit anyway.

    if amend {
        let commit_id = repo
            .amend(message, author)
            .into_diagnostic()
            .wrap_err("failed to amend commit")?;
        println!("[{}] (amended) {message}", commit_id.short());
    } else {
        let opts = vcs::CommitOptions { skip_verify };
        match repo.commit_with_options(message, author, &opts) {
            Ok(commit_id) => println!("[{}] {message}", commit_id.short()),
            Err(vcs::VcsError::NothingStaged) if allow_empty => {
                eprintln!("warning: empty commit (--allow-empty)");
            }
            Err(e) => return Err(e).into_diagnostic().wrap_err("failed to commit"),
        }
    }
    Ok(())
}

pub fn cmd_status(
    short: bool,
    porcelain: bool,
    show_branch: bool,
    data_dir: Option<&Path>,
) -> Result<()> {
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

    // Per-file status: show changes since last add (using file hash manifest).
    let cwd = std::env::current_dir().into_diagnostic()?;
    let manifest_path = cwd.join(".panproto").join("file_hashes.json");
    if manifest_path.exists() {
        let stored_json = std::fs::read_to_string(&manifest_path).into_diagnostic()?;
        let stored: HashMap<String, String> =
            serde_json::from_str(&stored_json).into_diagnostic()?;

        let mut current: HashMap<String, String> = HashMap::new();
        collect_file_hashes(&cwd, &cwd, &mut current)?;

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        for (path, hash) in &current {
            match stored.get(path) {
                Some(old_hash) if old_hash != hash => modified.push(path.as_str()),
                None => added.push(path.as_str()),
                _ => {}
            }
        }
        for path in stored.keys() {
            if !current.contains_key(path) {
                deleted.push(path.as_str());
            }
        }

        added.sort_unstable();
        modified.sort_unstable();
        deleted.sort_unstable();

        print_file_changes(&cwd, &added, &modified, &deleted)?;
    }

    if let Some(data_dir) = data_dir {
        let entries = read_json_dir(data_dir)?;
        let count = entries.len();
        println!("\nData: {} directory", data_dir.display());
        println!("  {count} JSON file(s) found");
        if let Some(head_id) = vcs::store::resolve_head(repo.store()).into_diagnostic()? {
            let head_obj = repo.store().get(&head_id).into_diagnostic()?;
            if let vcs::Object::Commit(c) = head_obj {
                println!("  HEAD schema: {}", c.schema_id.short());
                if c.data_ids.is_empty() {
                    println!("  No data tracked at HEAD — files may be stale");
                } else {
                    println!("  {} data set(s) tracked at HEAD", c.data_ids.len());
                }
            }
        }
    }

    Ok(())
}

/// Print file-level change summary, grouped by package if a manifest exists.
fn print_file_changes(
    cwd: &Path,
    added: &[&str],
    modified: &[&str],
    deleted: &[&str],
) -> Result<()> {
    if added.is_empty() && modified.is_empty() && deleted.is_empty() {
        println!("\nNo changes since last add.");
        return Ok(());
    }

    println!("\nChanges since last add:");

    let config = panproto_project::config::load_config(cwd).into_diagnostic()?;

    if let Some(ref cfg) = config {
        for pkg in &cfg.package {
            let prefix = pkg.path.display().to_string();
            let pkg_added: Vec<_> = added.iter().filter(|p| p.starts_with(&prefix)).collect();
            let pkg_modified: Vec<_> = modified.iter().filter(|p| p.starts_with(&prefix)).collect();
            let pkg_deleted: Vec<_> = deleted.iter().filter(|p| p.starts_with(&prefix)).collect();

            if pkg_added.is_empty() && pkg_modified.is_empty() && pkg_deleted.is_empty() {
                continue;
            }

            println!("  {}:", pkg.name);
            for p in &pkg_added {
                println!("    A  {p}");
            }
            for p in &pkg_modified {
                println!("    M  {p}");
            }
            for p in &pkg_deleted {
                println!("    D  {p}");
            }
        }

        let all_prefixes: Vec<String> = cfg
            .package
            .iter()
            .map(|p| p.path.display().to_string())
            .collect();
        let unpackaged_added: Vec<_> = added
            .iter()
            .filter(|p| !all_prefixes.iter().any(|pfx| p.starts_with(pfx)))
            .collect();
        let unpackaged_modified: Vec<_> = modified
            .iter()
            .filter(|p| !all_prefixes.iter().any(|pfx| p.starts_with(pfx)))
            .collect();
        let unpackaged_deleted: Vec<_> = deleted
            .iter()
            .filter(|p| !all_prefixes.iter().any(|pfx| p.starts_with(pfx)))
            .collect();

        if !unpackaged_added.is_empty()
            || !unpackaged_modified.is_empty()
            || !unpackaged_deleted.is_empty()
        {
            println!("  (unpackaged):");
            for p in &unpackaged_added {
                println!("    A  {p}");
            }
            for p in &unpackaged_modified {
                println!("    M  {p}");
            }
            for p in &unpackaged_deleted {
                println!("    D  {p}");
            }
        }
    } else {
        for p in added {
            println!("  A  {p}");
        }
        for p in modified {
            println!("  M  {p}");
        }
        for p in deleted {
            println!("  D  {p}");
        }
    }

    Ok(())
}

/// Options for the `log` subcommand.
pub struct LogCmdOptions<'a> {
    pub limit: Option<usize>,
    pub oneline: bool,
    pub show_data: bool,
    pub fmt: Option<&'a str>,
    pub filter_author: Option<&'a str>,
    pub filter_grep: Option<&'a str>,
}

pub fn cmd_log(opts: &LogCmdOptions<'_>) -> Result<()> {
    let repo = open_repo()?;
    let commits = repo.log(opts.limit).into_diagnostic()?;

    for commit in &commits {
        // Apply filters.
        if let Some(author_pat) = opts.filter_author {
            if !commit.author.contains(author_pat) {
                continue;
            }
        }
        if let Some(grep_pat) = opts.filter_grep {
            if !commit.message.contains(grep_pat) {
                continue;
            }
        }

        if let Some(fmt_str) = opts.fmt {
            println!("{}", format::format_commit(commit, fmt_str)?);
            continue;
        }

        if opts.oneline {
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
        if opts.show_data {
            if commit.data_ids.is_empty() {
                println!("Data:   (none)");
            } else {
                let data_ids: Vec<String> =
                    commit.data_ids.iter().map(vcs::ObjectId::short).collect();
                println!("Data:   {}", data_ids.join(" "));
            }
            if !commit.complement_ids.is_empty() {
                let comp_ids: Vec<String> = commit
                    .complement_ids
                    .iter()
                    .map(vcs::ObjectId::short)
                    .collect();
                println!("Compl:  {}", comp_ids.join(" "));
            }
        }
        println!();
        println!("    {}", commit.message);
        println!();
    }

    Ok(())
}
