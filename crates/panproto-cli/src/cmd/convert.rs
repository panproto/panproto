use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{inst, lens, schema::Schema};

use super::helpers::{infer_root_vertex, load_json, parse_defaults, resolve_protocol};

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn cmd_convert(
    data_path: &Path,
    from_path: Option<&Path>,
    to_path: Option<&Path>,
    protocol_name: &str,
    chain_path: Option<&Path>,
    output_path: Option<&Path>,
    direction: &str,
    defaults: &[String],
    verbose: bool,
) -> Result<()> {
    let protocol = resolve_protocol(protocol_name)?;
    let default_map = parse_defaults(defaults)?;

    // Build or load the lens.
    let (the_lens, src_schema, tgt_schema) = if let Some(cp) = chain_path {
        // Load a pre-built protolens chain from JSON.
        let chain_json_str = std::fs::read_to_string(cp)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to read chain from {}", cp.display()))?;
        let chain = lens::ProtolensChain::from_json(&chain_json_str)
            .into_diagnostic()
            .wrap_err("failed to parse protolens chain JSON")?;
        // Chain mode still needs from/to schemas for instantiation.
        let (Some(fp), Some(tp)) = (from_path, to_path) else {
            miette::bail!("--chain requires --from/--to for schema instantiation");
        };
        let src: Schema = load_json(fp)?;
        let tgt: Schema = load_json(tp)?;
        let lens = chain
            .instantiate(&src, &protocol)
            .into_diagnostic()
            .wrap_err("failed to instantiate protolens chain")?;
        (lens, src, tgt)
    } else if let (Some(fp), Some(tp)) = (from_path, to_path) {
        let src: Schema = load_json(fp)?;
        let tgt: Schema = load_json(tp)?;
        let config = lens::AutoLensConfig {
            defaults: default_map,
            try_overlap: false,
            ..Default::default()
        };
        let result = lens::auto_generate(&src, &tgt, &protocol, &config)
            .into_diagnostic()
            .wrap_err("failed to generate lens between schemas")?;
        (result.lens, src, tgt)
    } else {
        miette::bail!("specify --from/--to or --chain");
    };

    if verbose {
        eprintln!("Lens ready for conversion");
    }

    let (forward_schema, backward_schema) = match direction {
        "forward" => (&src_schema, &tgt_schema),
        "backward" => (&tgt_schema, &src_schema),
        other => miette::bail!("unknown direction: {other:?}. Use: forward or backward"),
    };

    // Helper closure to convert a single record.
    let convert_one = |data_json: &serde_json::Value| -> Result<String> {
        let root_vertex = infer_root_vertex(forward_schema)?;
        let instance = inst::parse_json(forward_schema, root_vertex.as_str(), data_json)
            .into_diagnostic()
            .wrap_err("failed to parse data as W-type instance")?;

        let output_instance = match direction {
            "forward" => {
                let (view, _complement) = lens::get(&the_lens, &instance)
                    .into_diagnostic()
                    .wrap_err("lens get (forward) failed")?;
                view
            }
            "backward" => {
                let complement = lens::Complement::empty();
                lens::put(&the_lens, &instance, &complement)
                    .into_diagnostic()
                    .wrap_err("lens put (backward) failed")?
            }
            _ => unreachable!(),
        };

        let output = inst::to_json(backward_schema, &output_instance);
        serde_json::to_string_pretty(&output)
            .into_diagnostic()
            .wrap_err("failed to serialize output")
    };

    if data_path.is_dir() {
        // Directory mode: iterate *.json files.
        let output_dir =
            output_path.ok_or_else(|| miette::miette!("specify -o for directory mode"))?;
        std::fs::create_dir_all(output_dir)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!("failed to create output directory {}", output_dir.display())
            })?;

        let mut entries: Vec<_> = std::fs::read_dir(data_path)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to read directory {}", data_path.display()))?
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);

        let total = entries.len();
        println!(
            "Converting {total} records from {} to {}",
            data_path.display(),
            output_dir.display()
        );

        let mut converted = 0u64;
        let mut skipped = 0u64;
        for (i, entry) in entries.iter().enumerate() {
            let filename = entry.file_name();
            let fname = filename.to_string_lossy();
            print!("  [{}/{}] {}... ", i + 1, total, fname);
            let data_json: serde_json::Value = match load_json(&entry.path()) {
                Ok(v) => v,
                Err(e) => {
                    println!("skipped ({e})");
                    skipped += 1;
                    continue;
                }
            };
            match convert_one(&data_json) {
                Ok(pretty) => {
                    let out_file = output_dir.join(&filename);
                    std::fs::write(&out_file, &pretty)
                        .into_diagnostic()
                        .wrap_err_with(|| format!("failed to write {}", out_file.display()))?;
                    println!("done");
                    converted += 1;
                }
                Err(e) => {
                    println!("skipped ({e})");
                    skipped += 1;
                }
            }
        }
        println!("Done: {converted} converted, {skipped} skipped");
    } else {
        // Single file mode.
        let data_json: serde_json::Value = load_json(data_path)?;
        let pretty = convert_one(&data_json)?;

        if let Some(op) = output_path {
            std::fs::write(op, &pretty)
                .into_diagnostic()
                .wrap_err_with(|| format!("failed to write {}", op.display()))?;
            if verbose {
                eprintln!("Wrote output to {}", op.display());
            }
        } else {
            println!("{pretty}");
        }
    }

    Ok(())
}
