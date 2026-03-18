use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::vcs::{self, Store as _};

use super::helpers::open_repo;
use super::migrate::migrate_data_between_schemas;

/// Options for the `branch` subcommand.
#[allow(clippy::struct_excessive_bools)]
pub struct BranchCmdOptions<'a> {
    pub name: Option<&'a str>,
    pub delete: bool,
    pub force_delete: bool,
    pub force: bool,
    pub rename: Option<&'a str>,
    pub verbose: bool,
    #[allow(dead_code)]
    pub all: bool,
}

pub fn cmd_branch(opts: &BranchCmdOptions<'_>) -> Result<()> {
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
pub struct TagCmdOptions<'a> {
    pub name: Option<&'a str>,
    pub delete: bool,
    pub annotate: bool,
    pub message: Option<&'a str>,
    pub list: bool,
    pub force: bool,
}

pub fn cmd_tag(opts: &TagCmdOptions<'_>) -> Result<()> {
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

pub fn cmd_checkout(
    target: &str,
    create: bool,
    detach: bool,
    migrate_dir: Option<&Path>,
) -> Result<()> {
    let mut repo = open_repo()?;

    // Capture pre-checkout HEAD schema for migration.
    let pre_checkout_schema_id = if migrate_dir.is_some() {
        vcs::store::resolve_head(repo.store())
            .into_diagnostic()?
            .and_then(|head_id| {
                let obj = repo.store().get(&head_id).ok()?;
                if let vcs::Object::Commit(c) = obj {
                    Some(c.schema_id)
                } else {
                    None
                }
            })
    } else {
        None
    };

    if create {
        // Create a new branch at HEAD and switch to it.
        let head_id = vcs::store::resolve_head(repo.store())
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("no commits yet"))?;
        vcs::refs::create_and_checkout_branch(repo.store_mut(), target, head_id)
            .into_diagnostic()?;
        println!("Switched to a new branch '{target}'");
    } else if detach {
        let id = vcs::refs::resolve_ref(repo.store(), target)
            .into_diagnostic()
            .wrap_err_with(|| format!("cannot resolve '{target}'"))?;
        vcs::refs::checkout_detached(repo.store_mut(), id).into_diagnostic()?;
        println!("HEAD is now at {}", id.short());
    } else {
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
    }

    // Migrate data if requested.
    if let Some(data_dir) = migrate_dir {
        maybe_migrate_data(repo.store(), pre_checkout_schema_id, data_dir)?;
    }

    Ok(())
}

/// Options for the `merge` subcommand.
#[allow(clippy::struct_excessive_bools)]
pub struct MergeCmdOptions<'a> {
    pub branch: Option<&'a str>,
    pub author: &'a str,
    pub no_commit: bool,
    pub ff_only: bool,
    pub no_ff: bool,
    pub squash: bool,
    pub abort: bool,
    pub message: Option<&'a str>,
    pub verbose: bool,
}

pub fn cmd_merge(cmd_opts: &MergeCmdOptions<'_>, migrate_dir: Option<&Path>) -> Result<()> {
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

    // Capture pre-merge HEAD schema for migration.
    let pre_merge_schema_id = if migrate_dir.is_some() {
        vcs::store::resolve_head(repo.store())
            .into_diagnostic()?
            .and_then(|head_id| {
                let obj = repo.store().get(&head_id).ok()?;
                if let vcs::Object::Commit(c) = obj {
                    Some(c.schema_id)
                } else {
                    None
                }
            })
    } else {
        None
    };

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

    // Migrate data if requested.
    if let Some(data_dir) = migrate_dir {
        maybe_migrate_data(repo.store(), pre_merge_schema_id, data_dir)?;
    }

    Ok(())
}

/// Resolve new HEAD's schema and migrate data if the schema changed.
pub fn maybe_migrate_data(
    store: &dyn vcs::Store,
    old_schema_id: Option<vcs::ObjectId>,
    data_dir: &Path,
) -> Result<()> {
    let Some(old_id) = old_schema_id else {
        return Ok(());
    };
    let new_head_id = vcs::store::resolve_head(store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no HEAD after operation"))?;
    let new_obj = store.get(&new_head_id).into_diagnostic()?;
    let vcs::Object::Commit(new_commit) = new_obj else {
        return Ok(());
    };
    if old_id == new_commit.schema_id {
        println!("Schemas are identical — no data migration needed.");
    } else {
        migrate_data_between_schemas(
            store,
            old_id,
            new_commit.schema_id,
            &new_commit.protocol,
            data_dir,
        )?;
    }
    Ok(())
}
