use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{lens, schema::Schema, vcs};

use super::helpers::{
    convert_single_file, load_commit_obj, load_schema_from_store, open_repo, parse_range,
    print_complement_requirements, read_json_dir, resolve_protocol,
};

/// Resolve a commit range for migration. Returns (old, new) commit IDs.
pub fn resolve_migrate_range(
    store: &dyn vcs::Store,
    range: Option<&str>,
) -> Result<(vcs::ObjectId, vcs::ObjectId)> {
    if let Some(range_str) = range {
        let (old_ref, new_ref) = parse_range(range_str)?;
        let old_id = vcs::refs::resolve_ref(store, &old_ref)
            .into_diagnostic()
            .wrap_err_with(|| format!("cannot resolve '{old_ref}'"))?;
        let new_id = vcs::refs::resolve_ref(store, &new_ref)
            .into_diagnostic()
            .wrap_err_with(|| format!("cannot resolve '{new_ref}'"))?;
        Ok((old_id, new_id))
    } else {
        let head_id = vcs::store::resolve_head(store)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("empty repository"))?;
        let head_commit = load_commit_obj(store, head_id)?;
        let parent_id = head_commit
            .parents
            .first()
            .ok_or_else(|| miette::miette!("HEAD has no parent — nothing to migrate from"))?;
        Ok((*parent_id, head_id))
    }
}

/// Apply a lens to all JSON files in a directory, writing results to an
/// output directory.
pub fn apply_lens_to_dir(
    entries: &[std::fs::DirEntry],
    src_schema: &Schema,
    tgt_schema: &Schema,
    the_lens: &lens::Lens,
    direction: &str,
    out_dir: &Path,
) -> Result<(u64, u64)> {
    let total = entries.len();
    let mut migrated = 0u64;
    let mut skipped = 0u64;

    for (i, entry) in entries.iter().enumerate() {
        let fname = entry.file_name();
        let display = fname.to_string_lossy();
        print!("  [{}/{}] {}... ", i + 1, total, display);

        match convert_single_file(&entry.path(), src_schema, tgt_schema, the_lens, direction) {
            Ok(output_json) => {
                let out_path = out_dir.join(&fname);
                std::fs::write(&out_path, output_json).into_diagnostic()?;
                println!("done");
                migrated += 1;
            }
            Err(e) => {
                println!("skipped ({e})");
                skipped += 1;
            }
        }
    }

    Ok((migrated, skipped))
}

/// Migrate all JSON files in a directory between two schemas identified by
/// their store object IDs.
pub fn migrate_data_between_schemas(
    store: &dyn vcs::Store,
    old_schema_id: vcs::ObjectId,
    new_schema_id: vcs::ObjectId,
    protocol_name: &str,
    data_dir: &Path,
) -> Result<()> {
    let old_schema = load_schema_from_store(store, old_schema_id)?;
    let new_schema = load_schema_from_store(store, new_schema_id)?;
    let protocol = resolve_protocol(protocol_name)?;

    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(&old_schema, &new_schema, &protocol, &config)
        .into_diagnostic()
        .wrap_err("failed to generate lens for data migration")?;

    let entries = read_json_dir(data_dir)?;
    let total = entries.len();
    println!("\nMigrating {total} data file(s) in {}", data_dir.display());

    let mut migrated = 0u64;
    let mut skipped = 0u64;

    for (i, entry) in entries.iter().enumerate() {
        let fname = entry.file_name();
        let display = fname.to_string_lossy();
        print!("  [{}/{}] {}... ", i + 1, total, display);

        match convert_single_file(
            &entry.path(),
            &old_schema,
            &new_schema,
            &result.lens,
            "forward",
        ) {
            Ok(output_json) => {
                std::fs::write(entry.path(), output_json).into_diagnostic()?;
                println!("done");
                migrated += 1;
            }
            Err(e) => {
                println!("skipped ({e})");
                skipped += 1;
            }
        }
    }

    println!("Done: {migrated} migrated, {skipped} skipped");
    Ok(())
}

pub fn cmd_migrate(
    data_dir: &Path,
    protocol_name: Option<&str>,
    range: Option<&str>,
    dry_run: bool,
    output_dir: Option<&Path>,
    backward: bool,
    verbose: bool,
) -> Result<()> {
    let repo = open_repo()?;
    let (old_commit_id, new_commit_id) = resolve_migrate_range(repo.store(), range)?;

    let old_commit = load_commit_obj(repo.store(), old_commit_id)?;
    let new_commit = load_commit_obj(repo.store(), new_commit_id)?;

    if old_commit.schema_id == new_commit.schema_id {
        println!("Schemas are identical — no migration needed.");
        return Ok(());
    }

    let old_schema = load_schema_from_store(repo.store(), old_commit.schema_id)?;
    let new_schema = load_schema_from_store(repo.store(), new_commit.schema_id)?;

    let proto_name = protocol_name.unwrap_or(&old_commit.protocol);
    let protocol = resolve_protocol(proto_name)?;

    let (src_schema, tgt_schema) = if backward {
        (&new_schema, &old_schema)
    } else {
        (&old_schema, &new_schema)
    };

    // Generate lens.
    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(src_schema, tgt_schema, &protocol, &config)
        .into_diagnostic()
        .wrap_err("failed to generate lens")?;

    // Show requirements.
    print_complement_requirements(&result.chain, src_schema, &protocol);

    let entries = read_json_dir(data_dir)?;
    let direction = if backward { "backward" } else { "forward" };

    println!("\nMigrating {} records ({direction})", entries.len());

    if dry_run {
        println!("(dry run — no files modified)");
        return Ok(());
    }

    let out_dir = output_dir.unwrap_or(data_dir);
    if out_dir != data_dir {
        std::fs::create_dir_all(out_dir).into_diagnostic()?;
    }

    if verbose {
        eprintln!(
            "Source schema: {} vertices, {} edges",
            src_schema.vertex_count(),
            src_schema.edge_count()
        );
        eprintln!(
            "Target schema: {} vertices, {} edges",
            tgt_schema.vertex_count(),
            tgt_schema.edge_count()
        );
    }

    let (migrated, skipped) = apply_lens_to_dir(
        &entries,
        src_schema,
        tgt_schema,
        &result.lens,
        direction,
        out_dir,
    )?;

    println!("\nDone: {migrated} migrated, {skipped} skipped");
    Ok(())
}

/// Run coverage analysis on migration between two schema versions.
///
/// Reads JSON files from the data directory, attempts to lift each through
/// the migration, and reports coverage statistics without modifying files.
pub fn cmd_migrate_coverage(
    data_dir: &Path,
    protocol_name: Option<&str>,
    range: Option<&str>,
    verbose: bool,
) -> Result<()> {
    let repo = open_repo()?;
    let (old_commit_id, new_commit_id) = resolve_migrate_range(repo.store(), range)?;

    let old_commit = load_commit_obj(repo.store(), old_commit_id)?;
    let new_commit = load_commit_obj(repo.store(), new_commit_id)?;

    if old_commit.schema_id == new_commit.schema_id {
        println!("\nCoverage: schemas are identical — 100% coverage.");
        return Ok(());
    }

    let old_schema = load_schema_from_store(repo.store(), old_commit.schema_id)?;
    let new_schema = load_schema_from_store(repo.store(), new_commit.schema_id)?;

    let proto_name = protocol_name.unwrap_or(&old_commit.protocol);
    let protocol = resolve_protocol(proto_name)?;

    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(&old_schema, &new_schema, &protocol, &config)
        .into_diagnostic()
        .wrap_err("failed to generate lens for coverage analysis")?;

    let entries = read_json_dir(data_dir)?;
    let total = entries.len();
    let mut succeeded = 0u64;
    let mut failed = 0u64;
    let mut errors: Vec<String> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let json_str = std::fs::read_to_string(&path).into_diagnostic()?;
        let json_value: serde_json::Value = serde_json::from_str(&json_str).into_diagnostic()?;

        // Infer root vertex for parsing.
        let root = if let Some((id, _)) = old_schema
            .vertices
            .iter()
            .find(|(_, v)| v.kind.as_ref() == "object" || v.kind.as_ref() == "record")
        {
            id.to_string()
        } else {
            old_schema
                .vertices
                .keys()
                .next()
                .map_or_else(|| "root".to_owned(), ToString::to_string)
        };

        match panproto_core::inst::parse_json(&old_schema, &root, &json_value) {
            Ok(instance) => match lens::get(&result.lens, &instance) {
                Ok(_) => succeeded += 1,
                Err(e) => {
                    failed += 1;
                    if errors.len() < 20 {
                        errors.push(format!("{}: {e}", path.display()));
                    }
                }
            },
            Err(e) => {
                failed += 1;
                if errors.len() < 20 {
                    errors.push(format!("{}: parse error: {e}", path.display()));
                }
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let coverage_pct = if total > 0 {
        (succeeded as f64 / total as f64) * 100.0
    } else {
        100.0
    };

    println!("\nCoverage report:");
    println!("  Total records:  {total}");
    println!("  Succeeded:      {succeeded}");
    println!("  Failed:         {failed}");
    println!("  Coverage:       {coverage_pct:.1}%");

    if verbose && !errors.is_empty() {
        println!("\nFirst {} failure(s):", errors.len());
        for e in &errors {
            println!("  {e}");
        }
    }

    Ok(())
}
