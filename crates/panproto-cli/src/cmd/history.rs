use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    gat::Name,
    vcs::{self, Store as _},
};

use super::helpers::{format_timestamp, load_json, open_repo};
use crate::{RemoteAction, StashAction};

pub fn cmd_rebase(onto: Option<&str>, author: &str, abort: bool, cont: bool) -> Result<()> {
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

pub fn cmd_cherry_pick(
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

pub fn cmd_reset(
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

pub fn cmd_stash(action: StashAction) -> Result<()> {
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

pub fn cmd_reflog(ref_name: &str, limit: Option<usize>, all: bool) -> Result<()> {
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

pub fn cmd_bisect(good: &str, bad: &str) -> Result<()> {
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

pub fn cmd_blame(element_type: &str, element_id: &str, reverse: bool) -> Result<()> {
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

pub fn cmd_gc(dry_run: bool, _prune: bool) -> Result<()> {
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

pub fn cmd_remote(_action: RemoteAction) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

pub fn cmd_push(_remote: Option<&str>, _branch: Option<&str>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

pub fn cmd_pull(_remote: Option<&str>, _branch: Option<&str>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

pub fn cmd_fetch(_remote: Option<&str>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}

pub fn cmd_clone(_url: &str, _path: Option<&Path>) -> Result<()> {
    miette::bail!(
        "remote operations are not yet implemented. Schema repositories are currently local-only."
    )
}
