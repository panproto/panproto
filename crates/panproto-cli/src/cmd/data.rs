use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{inst, lens, vcs, vcs::Store as _};

use super::helpers::{
    infer_root_vertex, load_commit_obj, load_json, load_schema_from_store, open_repo,
    read_json_dir, resolve_protocol,
};
use super::migrate::{apply_lens_to_dir, resolve_migrate_range};

/// Sync data files to match a target schema version.
///
/// Loads the repository, resolves old and new schemas (from HEAD's parent
/// and HEAD, or a specific target ref), generates a lens, and migrates all
/// JSON files in the data directory. When `edits` is true, an
/// `EditLogObject` recording the sync is stored in the VCS.
pub fn cmd_data_sync(
    data_dir: &Path,
    edits: bool,
    target: Option<&str>,
    verbose: bool,
) -> Result<()> {
    let mut repo = open_repo()?;

    let (old_commit_id, new_commit_id) = if let Some(tgt) = target {
        let range_str = format!("{tgt}~1..{tgt}");
        resolve_migrate_range(repo.store(), Some(&range_str))?
    } else {
        resolve_migrate_range(repo.store(), None)?
    };

    let old_commit = load_commit_obj(repo.store(), old_commit_id)?;
    let new_commit = load_commit_obj(repo.store(), new_commit_id)?;

    if old_commit.schema_id == new_commit.schema_id {
        println!("Schemas are identical; data is already in sync.");
        return Ok(());
    }

    let old_schema = load_schema_from_store(repo.store(), old_commit.schema_id)?;
    let new_schema = load_schema_from_store(repo.store(), new_commit.schema_id)?;
    let protocol = resolve_protocol(&old_commit.protocol)?;

    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(&old_schema, &new_schema, &protocol, &config)
        .into_diagnostic()
        .wrap_err("failed to generate lens for data sync")?;

    let entries = read_json_dir(data_dir)?;
    println!(
        "Syncing {} data file(s) in {}",
        entries.len(),
        data_dir.display()
    );

    let (migrated, skipped) = apply_lens_to_dir(
        &entries,
        &old_schema,
        &new_schema,
        &result.lens,
        "forward",
        data_dir,
    )?;

    println!("Done: {migrated} synced, {skipped} skipped");

    if edits {
        store_edit_log(
            &mut repo,
            &old_schema,
            &new_schema,
            &protocol,
            &config,
            &entries,
            &new_commit,
            verbose,
        )?;
    }

    Ok(())
}

/// Build an `EditLens`, translate edits, and store the `EditLogObject` in the VCS.
#[allow(clippy::too_many_arguments)]
fn store_edit_log(
    repo: &mut vcs::Repository,
    old_schema: &panproto_core::schema::Schema,
    new_schema: &panproto_core::schema::Schema,
    protocol: &panproto_core::schema::Protocol,
    config: &lens::AutoLensConfig,
    entries: &[std::fs::DirEntry],
    new_commit: &vcs::CommitObject,
    verbose: bool,
) -> Result<()> {
    let edit_result = lens::auto_generate(old_schema, new_schema, protocol, config)
        .into_diagnostic()
        .wrap_err("failed to regenerate lens for edit logging")?;
    let mut edit_lens = lens::EditLens::from_lens(edit_result.lens, protocol.clone());

    let root_vertex = infer_root_vertex(old_schema)?;
    let mut all_edits: Vec<inst::TreeEdit> = Vec::new();

    for entry in entries {
        let json_val: serde_json::Value = load_json(&entry.path())?;
        let instance = inst::parse_json(old_schema, root_vertex.as_str(), &json_val).ok();
        if let Some(source_inst) = instance {
            edit_lens
                .initialize(&source_inst)
                .map_err(|e| miette::miette!("failed to initialize edit lens: {e}"))?;
            for &id in source_inst.nodes.keys() {
                if id == source_inst.root {
                    continue;
                }
                let edit = inst::TreeEdit::SetField {
                    node_id: id,
                    field: panproto_core::gat::Name::from("__synced"),
                    value: inst::Value::Bool(true),
                };
                if let Ok(translated) = edit_lens.get_edit(edit) {
                    if !translated.is_identity() {
                        all_edits.push(translated);
                    }
                }
            }
        }
    }

    let encoded = vcs::encode_edit_log(&all_edits)
        .into_diagnostic()
        .wrap_err("failed to encode edit log")?;

    let complement_bytes = serde_json::to_vec(&edit_lens.complement)
        .into_diagnostic()
        .wrap_err("failed to serialize complement")?;
    let complement_obj = vcs::Object::Complement(vcs::ComplementObject {
        migration_id: new_commit.migration_id.unwrap_or(vcs::ObjectId::ZERO),
        data_id: new_commit
            .data_ids
            .first()
            .copied()
            .unwrap_or(vcs::ObjectId::ZERO),
        complement: complement_bytes,
    });
    let complement_id = repo
        .store_mut()
        .put(&complement_obj)
        .into_diagnostic()
        .wrap_err("failed to store complement")?;

    let edit_log = vcs::EditLogObject {
        schema_id: new_commit.schema_id,
        data_id: new_commit
            .data_ids
            .first()
            .copied()
            .unwrap_or(new_commit.schema_id),
        edits: encoded,
        edit_count: u64::try_from(all_edits.len()).unwrap_or(0),
        final_complement: complement_id,
    };
    let obj = vcs::Object::EditLog(edit_log);
    let edit_id = repo
        .store_mut()
        .put(&obj)
        .into_diagnostic()
        .wrap_err("failed to store edit log")?;
    if verbose {
        eprintln!(
            "Stored edit log: {edit_id} ({} edits, complement: {complement_id})",
            all_edits.len()
        );
    }

    Ok(())
}

/// Report data staleness: check which files might need migration.
///
/// Loads JSON files from the data directory, compares the HEAD schema
/// version, and reports which files are up to date or potentially stale.
pub fn cmd_data_status(data_dir: &Path, verbose: bool) -> Result<()> {
    let repo = open_repo()?;
    let head_id = vcs::store::resolve_head(repo.store())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("empty repository; no commits yet"))?;
    let head_obj = repo.store().get(&head_id).into_diagnostic()?;
    let vcs::Object::Commit(commit) = head_obj else {
        miette::bail!("HEAD does not point to a commit")
    };

    let entries = read_json_dir(data_dir)?;
    let count = entries.len();

    println!("Data directory: {}", data_dir.display());
    println!("  {count} JSON file(s) found");
    println!("  HEAD schema: {}", commit.schema_id.short());

    if commit.data_ids.is_empty() {
        println!("  No data tracked at HEAD; files may be stale.");
    } else {
        println!("  {} data set(s) tracked at HEAD", commit.data_ids.len());
    }

    if verbose && count > 0 {
        println!("\nFiles:");
        for entry in &entries {
            let fname = entry.file_name();
            println!("  {}", fname.to_string_lossy());
        }
    }

    Ok(())
}
