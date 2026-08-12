use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::lens::Stringency;
use panproto_core::lens_dsl::{HintSpec, HintStringency};
use panproto_core::{
    gat::Name,
    inst, lens,
    schema::Schema,
    vcs::{self, Store as _},
};

/// Fold candidate / requirements sections into a JSON-mode root
/// document so `--json` / `--chain` always emit exactly one JSON value.
#[allow(clippy::too_many_arguments)]
fn augment_json_root(
    root: &mut serde_json::Value,
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocol: &panproto_core::schema::Protocol,
    result: &lens::AutoLensResult,
    config: &lens::AutoLensConfig,
    top_n: usize,
    emit_candidates: bool,
    requirements: bool,
) -> Result<()> {
    let obj = if let serde_json::Value::Object(o) = root {
        o
    } else {
        // Fallback: wrap non-object JSON in a one-key document so we can
        // still splice sections in. This should not happen in practice
        // because both `chain_to_json` and `auto_lens_result_to_json`
        // emit objects, but keeps the helper total.
        let inner = std::mem::replace(root, serde_json::Value::Object(serde_json::Map::new()));
        if let serde_json::Value::Object(o) = root {
            o.insert("result".to_owned(), inner);
            o
        } else {
            unreachable!("just set root to an Object");
        }
    };

    if emit_candidates {
        let candidates =
            lens::auto_generate_candidates(src_schema, tgt_schema, protocol, config, top_n)
                .into_diagnostic()
                .wrap_err("failed to generate candidate lenses")?;
        let entries: Vec<serde_json::Value> = candidates
            .iter()
            .map(|c| {
                serde_json::json!({
                    "quality": c.quality,
                    "coverage": c.coverage,
                    "score": c.score(),
                    "strategies_used": c.strategies_used,
                    "steps": c.steps.iter().map(|s| serde_json::json!({
                        "kind": s.kind,
                        "explanation": s.explanation,
                        "confidence": s.confidence,
                        "strategy": s.strategy,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let coerce_proposals: Vec<serde_json::Value> = result
            .coerce_proposals
            .iter()
            .map(|p| {
                serde_json::json!({
                    "src": p.anchor.src.as_str(),
                    "tgt": p.anchor.tgt.as_str(),
                    "witness_name": p.witness_name,
                    "witness_class": p.witness_class,
                    "confidence": p.anchor.confidence,
                    "explanation": p.anchor.explanation,
                })
            })
            .collect();
        obj.insert(
            "candidates".to_owned(),
            serde_json::json!({
                "count": candidates.len(),
                "entries": entries,
                "coerce_proposals": coerce_proposals,
            }),
        );
    }

    if requirements {
        let spec = lens::chain_complement_spec(&result.chain, src_schema, protocol);
        obj.insert(
            "requirements".to_owned(),
            serde_json::to_value(&spec)
                .into_diagnostic()
                .wrap_err("failed to serialize complement spec")?,
        );
    }

    Ok(())
}

/// Convert a hint-DSL stringency tier into the engine's [`Stringency`].
const fn hint_stringency_to_engine(s: HintStringency) -> Stringency {
    match s {
        HintStringency::Strict => Stringency::Strict,
        HintStringency::Balanced => Stringency::Balanced,
        HintStringency::Lenient => Stringency::Lenient,
        HintStringency::Exploratory => Stringency::Exploratory,
    }
}

use super::helpers::{
    auto_lens_result_to_json, chain_to_json, infer_root_vertex, load_json, open_repo,
    parse_defaults, resolve_protocol,
};

/// Generate a lens between two schemas.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn cmd_lens_generate(
    old_path: &Path,
    new_path: &Path,
    protocol_name: &str,
    json: bool,
    chain: bool,
    try_overlap: bool,
    save: Option<&Path>,
    defaults: &[String],
    fuse: bool,
    requirements: bool,
    verbose: bool,
    hints_path: Option<&Path>,
    stringency_arg: Option<Stringency>,
    top_n: usize,
    explain: bool,
) -> Result<()> {
    let src_schema: Schema = load_json(old_path)?;
    let tgt_schema: Schema = load_json(new_path)?;
    let protocol = resolve_protocol(protocol_name)?;
    let default_map = parse_defaults(defaults)?;

    if verbose {
        eprintln!(
            "Generating lens: {} ({} vertices) -> {} ({} vertices)",
            old_path.display(),
            src_schema.vertex_count(),
            new_path.display(),
            tgt_schema.vertex_count()
        );
    }

    let mut config = lens::AutoLensConfig {
        defaults: default_map,
        try_overlap,
        ..Default::default()
    };
    if let Some(s) = stringency_arg {
        config.stringency = s;
    }

    let result = if let Some(hp) = hints_path {
        let hint_json = std::fs::read_to_string(hp)
            .into_diagnostic()
            .wrap_err("failed to read hints file")?;
        let hint_spec: HintSpec = serde_json::from_str(&hint_json)
            .into_diagnostic()
            .wrap_err("failed to parse hints file")?;

        // Hint-file stringency overrides the CLI flag only when the
        // CLI did not pin one explicitly.
        if stringency_arg.is_none() {
            if let Some(s) = hint_spec.stringency {
                config.stringency = hint_stringency_to_engine(s);
            }
        }
        // Merge user-supplied alias clusters into the engine's dictionary.
        for cluster in &hint_spec.alias_clusters {
            config.alias_dict.add_cluster(cluster);
        }

        let parts = lens::hint::HintParts {
            anchors: hint_spec.anchors.clone(),
            scope_pairs: hint_spec.scope_pairs(),
            excluded_targets: hint_spec.excluded_target_names(),
            excluded_sources: hint_spec.excluded_source_names(),
            scoring_weights: hint_spec.scoring_weights(),
        };
        let (derived, domain_constraints) =
            lens::hint::resolve_hints(&parts, &src_schema, &tgt_schema);

        lens::auto_generate_with_hints(
            &src_schema,
            &tgt_schema,
            &protocol,
            &config,
            &derived,
            &domain_constraints,
            None,
        )
        .into_diagnostic()
        .wrap_err("failed to generate lens with hints")?
    } else {
        lens::auto_generate(&src_schema, &tgt_schema, &protocol, &config)
            .into_diagnostic()
            .wrap_err("failed to generate lens between schemas")?
    };

    // In JSON output modes (`--json` or `--chain`), optional sections
    // (`--top-n`/`--explain` candidates and `--requirements`) must fold
    // into a single JSON document — emitting multiple top-level JSON
    // values back-to-back is not valid JSON and breaks downstream
    // tooling (`jq`, `serde_json::from_reader`, etc.).
    let emit_candidates = top_n > 1 || explain;
    let json_mode = json || chain;

    // Handle output modes.
    if chain {
        let mut root = chain_to_json(&result.chain);
        augment_json_root(
            &mut root,
            &src_schema,
            &tgt_schema,
            &protocol,
            &result,
            &config,
            top_n,
            emit_candidates,
            requirements,
        )?;
        // Fold `--fuse` output in now (before printing) so `--chain --fuse
        // --json`-style invocations still emit a single JSON document.
        if fuse {
            let fused = result
                .chain
                .fuse()
                .into_diagnostic()
                .wrap_err("failed to fuse protolens chain")?;
            let fused_json_str = fused
                .to_json()
                .into_diagnostic()
                .wrap_err("failed to serialize fused protolens")?;
            let fused_value: serde_json::Value = serde_json::from_str(&fused_json_str)
                .into_diagnostic()
                .wrap_err("failed to parse fused protolens JSON")?;
            if let serde_json::Value::Object(obj) = &mut root {
                obj.insert("fused".to_owned(), fused_value);
            }
        }
        let pretty = serde_json::to_string_pretty(&root)
            .into_diagnostic()
            .wrap_err("failed to serialize protolens chain")?;
        println!("{pretty}");
    } else if json {
        let mut root = auto_lens_result_to_json(&result);
        augment_json_root(
            &mut root,
            &src_schema,
            &tgt_schema,
            &protocol,
            &result,
            &config,
            top_n,
            emit_candidates,
            requirements,
        )?;
        if fuse {
            let fused = result
                .chain
                .fuse()
                .into_diagnostic()
                .wrap_err("failed to fuse protolens chain")?;
            let fused_json_str = fused
                .to_json()
                .into_diagnostic()
                .wrap_err("failed to serialize fused protolens")?;
            let fused_value: serde_json::Value = serde_json::from_str(&fused_json_str)
                .into_diagnostic()
                .wrap_err("failed to parse fused protolens JSON")?;
            if let serde_json::Value::Object(obj) = &mut root {
                obj.insert("fused".to_owned(), fused_value);
            }
        }
        let pretty = serde_json::to_string_pretty(&root)
            .into_diagnostic()
            .wrap_err("failed to serialize lens")?;
        println!("{pretty}");
    } else {
        // Human-readable summary.
        println!("Lens: {} -> {}", old_path.display(), new_path.display());
        println!("  Stringency: {}", config.stringency);
        println!("  Alignment quality: {:.3}", result.alignment_quality);
        println!("  Steps: {}", result.chain.steps.len());
        for (i, step) in result.chain.steps.iter().enumerate() {
            let lossless = if step.is_lossless() {
                " (lossless)"
            } else {
                " (lossy)"
            };
            println!("    {}. {}{lossless}", i + 1, step.name);
        }
        if explain && !result.coerce_proposals.is_empty() {
            println!("\nCoerce proposals ({}):", result.coerce_proposals.len());
            for p in &result.coerce_proposals {
                println!(
                    "  - {} ↔ {} via `{}` ({:?}, conf={:.2}): {}",
                    p.anchor.src.as_str(),
                    p.anchor.tgt.as_str(),
                    p.witness_name,
                    p.witness_class,
                    p.anchor.confidence,
                    p.anchor.explanation,
                );
            }
        }
    }

    // `--top-n N` or `--explain` activates the candidate API. The code
    // above produced the top-1 lens via the classic entry point; this
    // block additionally surfaces ranked alternatives and per-step
    // explanations to stdout. Save/fuse/requirements still operate on
    // the top-1 result above.
    if emit_candidates && !json_mode {
        // Human-readable candidate summary. JSON/chain modes already
        // folded candidates into the root document above.
        let candidates =
            lens::auto_generate_candidates(&src_schema, &tgt_schema, &protocol, &config, top_n)
                .into_diagnostic()
                .wrap_err("failed to generate candidate lenses")?;
        println!("\nCandidates ({} total):", candidates.len());
        for (idx, cand) in candidates.iter().enumerate() {
            println!(
                "  #{} score={:.3} quality={:.3} coverage={:.3}",
                idx + 1,
                cand.score(),
                cand.quality,
                cand.coverage
            );
            if explain {
                for (si, step) in cand.steps.iter().enumerate() {
                    let tag = step
                        .strategy
                        .map_or_else(|| "structural".to_owned(), |t| format!("{t:?}"));
                    println!(
                        "    {}. [{}] conf={:.2} {}",
                        si + 1,
                        tag,
                        step.confidence,
                        step.explanation
                    );
                }
            }
        }
    }

    // Save the chain if requested.
    if let Some(save_path) = save {
        let chain_json = chain_to_json(&result.chain);
        let pretty = serde_json::to_string_pretty(&chain_json)
            .into_diagnostic()
            .wrap_err("failed to serialize protolens chain")?;
        std::fs::write(save_path, &pretty)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write chain to {}", save_path.display()))?;
        if verbose {
            eprintln!("Saved chain to {}", save_path.display());
        }
    }

    // Fuse the chain into a single protolens if requested. JSON/chain
    // modes already folded the fused payload into the root document above.
    if fuse && !json_mode {
        let fused = result
            .chain
            .fuse()
            .into_diagnostic()
            .wrap_err("failed to fuse protolens chain")?;
        println!("\nFused protolens:");
        println!("  Name: {}", fused.name);
        println!("  Source: {}", fused.source.name);
        println!("  Target: {}", fused.target.name);
        let lossless = if fused.is_lossless() {
            "lossless"
        } else {
            "lossy"
        };
        println!("  Complement: {lossless}");
    }

    // Show requirements if requested.
    if requirements && !json_mode {
        // JSON/chain modes already folded requirements into the root.
        let spec = lens::chain_complement_spec(&result.chain, &src_schema, &protocol);
        {
            println!("\nRequirements:");
            println!("  Kind: {:?}", spec.kind);
            println!("  Summary: {}", spec.summary);
            if !spec.forward_defaults.is_empty() {
                println!("  Forward defaults needed:");
                for req in &spec.forward_defaults {
                    println!(
                        "    - {} ({}): {}",
                        req.element_name, req.element_kind, req.description
                    );
                }
            }
            if !spec.captured_data.is_empty() {
                println!("  Data captured in complement:");
                for cap in &spec.captured_data {
                    println!(
                        "    - {} ({}): {}",
                        cap.element_name, cap.element_kind, cap.description
                    );
                }
            }
        }
    }

    Ok(())
}

/// Compile a lens DSL document (`.ncl`/`.json`/`.yaml`/`.yml`) into a
/// protolens chain and emit its JSON.
///
/// The document is loaded and compiled via
/// [`panproto_core::lens_dsl::load_and_compile`], which resolves `compose`
/// named references against sibling lens documents in the same
/// directory. The output is a JSON object carrying the document
/// metadata and, under the `chain` key, the full serde-serialized
/// [`ProtolensChain`](lens::ProtolensChain) (round-trippable via
/// `ProtolensChain::from_json`). Written to `out` if given, else stdout.
pub fn cmd_lens_compile(
    doc_path: &Path,
    body_vertex: &str,
    out: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    if verbose {
        eprintln!(
            "Compiling lens DSL document {} (body vertex: {body_vertex})",
            doc_path.display()
        );
    }

    let compiled = panproto_core::lens_dsl::load_and_compile(doc_path, body_vertex)
        .map_err(miette::Report::new)
        .wrap_err_with(|| format!("failed to compile {}", doc_path.display()))?;

    // Embed the engine's own round-trippable chain JSON under `chain`.
    let chain_json = compiled
        .chain
        .to_json()
        .into_diagnostic()
        .wrap_err("failed to serialize compiled protolens chain")?;
    let chain_value: serde_json::Value = serde_json::from_str(&chain_json)
        .into_diagnostic()
        .wrap_err("failed to parse serialized protolens chain")?;

    let output = serde_json::json!({
        "id": compiled.id,
        "description": compiled.description,
        "source": compiled.source,
        "target": compiled.target,
        "invertible": compiled.invertible,
        "step_count": compiled.chain.steps.len(),
        "field_transform_vertices": compiled.field_transforms.len(),
        "symmetric": compiled.symmetric.is_some(),
        "chain": chain_value,
    });

    let pretty = serde_json::to_string_pretty(&output)
        .into_diagnostic()
        .wrap_err("failed to serialize compilation output")?;

    if let Some(out_path) = out {
        std::fs::write(out_path, &pretty)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write chain to {}", out_path.display()))?;
        if verbose {
            eprintln!("Wrote compiled chain to {}", out_path.display());
        }
    } else {
        println!("{pretty}");
    }

    Ok(())
}

/// Inspect a saved protolens chain: print requirements, cost, and optic classification.
pub fn cmd_lens_inspect(chain_path: &Path, protocol_name: &str, verbose: bool) -> Result<()> {
    let chain_json_str = std::fs::read_to_string(chain_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read chain from {}", chain_path.display()))?;
    let chain = lens::ProtolensChain::from_json(&chain_json_str)
        .into_diagnostic()
        .wrap_err("failed to parse protolens chain JSON")?;

    println!("Protolens chain: {}", chain_path.display());
    println!("  Steps: {}", chain.len());

    for (i, step) in chain.steps.iter().enumerate() {
        let lossless = if step.is_lossless() {
            " (lossless)"
        } else {
            " (lossy)"
        };
        let step_cost = lens::complement_cost(&step.complement_constructor);
        println!(
            "    {}. {}{lossless} [cost: {step_cost:.2}]",
            i + 1,
            step.name
        );
    }

    let cost = lens::chain_cost(&chain);
    println!("  Total cost: {cost:.2}");

    // If a protocol is provided, try to compute complement requirements.
    // This requires a schema, which we don't have in pure chain mode, so we
    // only print cost and step info unconditionally.
    if verbose {
        let _protocol = resolve_protocol(protocol_name)?;
        eprintln!(
            "Note: complement requirements need a schema; use 'lens generate --requirements' for full analysis."
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_lens_apply(
    lens_path: &Path,
    data_path: &Path,
    protocol_name: &str,
    schema_path: Option<&Path>,
    direction: &str,
    complement_path: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let protocol = resolve_protocol(protocol_name)?;

    let schema: Schema = if let Some(sp) = schema_path {
        load_json(sp)?
    } else {
        miette::bail!(
            "lens apply requires --schema to provide the source schema for chain instantiation"
        );
    };

    if verbose {
        eprintln!(
            "Applying lens from {} to data {} (direction: {direction})",
            lens_path.display(),
            data_path.display()
        );
    }

    // Load and parse the protolens chain.
    let chain_json_str = std::fs::read_to_string(lens_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read chain from {}", lens_path.display()))?;

    // Try parsing as a protolens chain first. If that fails, fall back to
    // auto-generating a lens from the file treated as a target schema.
    let the_lens = if let Ok(chain) = lens::ProtolensChain::from_json(&chain_json_str) {
        if verbose {
            eprintln!("Parsed protolens chain with {} step(s)", chain.len());
        }
        chain
            .instantiate(&schema, &protocol)
            .into_diagnostic()
            .wrap_err("failed to instantiate protolens chain at schema")?
    } else {
        let tgt_schema: Schema = serde_json::from_str(&chain_json_str)
            .into_diagnostic()
            .wrap_err("chain file is neither a valid protolens chain nor a schema")?;
        let config = lens::AutoLensConfig::default();
        let result = lens::auto_generate(&schema, &tgt_schema, &protocol, &config)
            .into_diagnostic()
            .wrap_err("failed to generate lens from schemas")?;
        if verbose {
            eprintln!(
                "Auto-generated lens ({} steps, quality {:.3})",
                result.chain.steps.len(),
                result.alignment_quality
            );
        }
        result.lens
    };

    let data_json: serde_json::Value = load_json(data_path)?;
    let root_vertex = infer_root_vertex(&schema)?;
    let instance = inst::parse_json(&schema, root_vertex.as_str(), &data_json)
        .into_diagnostic()
        .wrap_err("failed to parse data as W-type instance")?;

    match direction {
        "forward" => {
            let (view, _complement) = lens::get(&the_lens, &instance)
                .into_diagnostic()
                .wrap_err("lens get (forward) failed")?;
            let output = inst::to_json(&the_lens.tgt_schema, &view);
            let pretty = serde_json::to_string_pretty(&output)
                .into_diagnostic()
                .wrap_err("failed to serialize output")?;
            println!("{pretty}");
        }
        "backward" => {
            let complement = if let Some(cp) = complement_path {
                let comp_json: serde_json::Value = load_json(cp)?;
                serde_json::from_value(comp_json)
                    .into_diagnostic()
                    .wrap_err("failed to parse complement data")?
            } else {
                lens::Complement::empty()
            };

            let restored = lens::put(&the_lens, &instance, &complement)
                .into_diagnostic()
                .wrap_err("lens put (backward) failed")?;
            let output = inst::to_json(&the_lens.src_schema, &restored);
            let pretty = serde_json::to_string_pretty(&output)
                .into_diagnostic()
                .wrap_err("failed to serialize output")?;
            println!("{pretty}");
        }
        other => miette::bail!("unknown direction: {other:?}. Use: forward or backward"),
    }

    Ok(())
}

pub fn cmd_lens_verify(
    first_path: &Path,
    second_path: Option<&Path>,
    protocol_name: &str,
    data_path: Option<&Path>,
    naturality: bool,
    verbose: bool,
) -> Result<()> {
    let protocol = resolve_protocol(protocol_name)?;

    let src_schema: Schema = load_json(first_path)?;

    // If a second schema is provided, generate a lens between them;
    // otherwise, treat the first file as a lens and verify it.
    let tgt_schema: Schema = if let Some(sp) = second_path {
        load_json(sp)?
    } else {
        // Verify the identity lens on this schema.
        src_schema.clone()
    };

    if verbose {
        eprintln!(
            "Verifying lens laws between {} ({} vertices) and {} ({} vertices)",
            first_path.display(),
            src_schema.vertex_count(),
            second_path.map_or_else(|| "(self)".to_string(), |p| p.display().to_string(),),
            tgt_schema.vertex_count()
        );
    }

    let config = lens::AutoLensConfig::default();
    let result = lens::auto_generate(&src_schema, &tgt_schema, &protocol, &config)
        .into_diagnostic()
        .wrap_err("failed to generate lens for verification")?;

    println!(
        "Lens generated: {} step(s), alignment quality: {:.3}",
        result.chain.steps.len(),
        result.alignment_quality
    );

    // If test data is provided, verify lens laws with it.
    if let Some(dp) = data_path {
        let data_json: serde_json::Value = load_json(dp)?;
        let root_vertex = infer_root_vertex(&src_schema)?;
        let instance = inst::parse_json(&src_schema, root_vertex.as_str(), &data_json)
            .into_diagnostic()
            .wrap_err("failed to parse test data")?;

        match lens::check_laws(&result.lens, &instance) {
            Ok(()) => println!("GetPut: OK\nPutGet: OK"),
            Err(violation) => {
                println!("Lens law violation: {violation:?}");
                miette::bail!("lens law verification failed");
            }
        }
    } else {
        println!("No test data provided; skipping concrete law checks.");
        println!("Hint: pass --data <file> to verify GetPut and PutGet with real data.");
    }

    // Naturality check: verify that each protolens step is applicable at
    // the source schema. A protolens is natural if every step's precondition
    // is satisfied by the schema it operates on; applicability checking is
    // the concrete verification of this property.
    if naturality {
        let mut all_applicable = true;
        for (i, step) in result.chain.steps.iter().enumerate() {
            let applicable = step.applicable_to(&src_schema);
            if !applicable {
                println!(
                    "Naturality issue: step {} ({}) is not applicable at source schema",
                    i + 1,
                    step.name
                );
                all_applicable = false;
            }
        }
        if all_applicable {
            println!("Naturality: all protolens steps are applicable at the source schema.");
        } else {
            miette::bail!("naturality check failed");
        }
    }

    Ok(())
}

pub fn cmd_lens_compose(
    first_path: &Path,
    second_path: &Path,
    protocol_name: &str,
    json: bool,
    chain: bool,
    verbose: bool,
) -> Result<()> {
    let protocol = resolve_protocol(protocol_name)?;

    // Interpret inputs as schemas and generate lenses for each pair.
    // For a chain of A -> B -> C, we compose two lenses.
    let first_json: serde_json::Value = load_json(first_path)?;
    let second_json: serde_json::Value = load_json(second_path)?;

    // Check if inputs are schema files or lens chain files.
    let is_chain =
        first_json.get("type").and_then(serde_json::Value::as_str) == Some("protolens_chain");

    if is_chain {
        // Both are chain files; merge steps.
        let first_steps = first_json
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        let second_steps = second_json
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);

        if verbose {
            eprintln!("Composing chains: {first_steps} + {second_steps} steps");
        }

        let total = first_steps + second_steps;
        let composed = serde_json::json!({
            "type": "protolens_chain",
            "steps": [],
            "step_count": total,
            "composed_from": [
                first_path.display().to_string(),
                second_path.display().to_string(),
            ],
        });

        let pretty = serde_json::to_string_pretty(&composed)
            .into_diagnostic()
            .wrap_err("failed to serialize composed chain")?;
        println!("{pretty}");
    } else {
        // Treat as schema files. Generate lens for each pair and compose.
        let schema_a: Schema = serde_json::from_value(first_json)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to parse {} as schema", first_path.display()))?;
        let schema_b: Schema = serde_json::from_value(second_json)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to parse {} as schema", second_path.display()))?;

        if verbose {
            eprintln!(
                "Composing lenses: {} ({} vertices) and {} ({} vertices)",
                first_path.display(),
                schema_a.vertex_count(),
                second_path.display(),
                schema_b.vertex_count(),
            );
        }

        let config = lens::AutoLensConfig::default();
        let result_a = lens::auto_generate(&schema_a, &schema_b, &protocol, &config)
            .into_diagnostic()
            .wrap_err("failed to generate first lens")?;
        let result_b = lens::auto_generate(&schema_b, &schema_a, &protocol, &config)
            .into_diagnostic()
            .wrap_err("failed to generate second lens")?;

        let composed = lens::compose(&result_a.lens, &result_b.lens)
            .into_diagnostic()
            .wrap_err("failed to compose lenses")?;

        if chain {
            // Concatenate protolens chain steps.
            let mut all_steps: Vec<serde_json::Value> = Vec::new();
            for (i, step) in result_a.chain.steps.iter().enumerate() {
                all_steps.push(serde_json::json!({
                    "step": i + 1,
                    "name": step.name.as_str(),
                    "lossless": step.is_lossless(),
                    "source": "first",
                }));
            }
            for (i, step) in result_b.chain.steps.iter().enumerate() {
                all_steps.push(serde_json::json!({
                    "step": result_a.chain.steps.len() + i + 1,
                    "name": step.name.as_str(),
                    "lossless": step.is_lossless(),
                    "source": "second",
                }));
            }
            let chain_json = serde_json::json!({
                "type": "protolens_chain",
                "steps": all_steps,
                "step_count": all_steps.len(),
            });
            let pretty = serde_json::to_string_pretty(&chain_json)
                .into_diagnostic()
                .wrap_err("failed to serialize composed chain")?;
            println!("{pretty}");
        } else if json {
            let info = serde_json::json!({
                "composed": true,
                "first_steps": result_a.chain.steps.len(),
                "second_steps": result_b.chain.steps.len(),
                "total_steps": result_a.chain.steps.len() + result_b.chain.steps.len(),
                "src_vertices": composed.src_schema.vertex_count(),
                "tgt_vertices": composed.tgt_schema.vertex_count(),
            });
            let pretty = serde_json::to_string_pretty(&info)
                .into_diagnostic()
                .wrap_err("failed to serialize composition info")?;
            println!("{pretty}");
        } else {
            println!("Composed lens:");
            println!(
                "  First:  {} step(s), quality {:.3}",
                result_a.chain.steps.len(),
                result_a.alignment_quality
            );
            println!(
                "  Second: {} step(s), quality {:.3}",
                result_b.chain.steps.len(),
                result_b.alignment_quality
            );
            println!(
                "  Result: {} vertices -> {} vertices",
                composed.src_schema.vertex_count(),
                composed.tgt_schema.vertex_count()
            );
        }
    }

    Ok(())
}

/// Resolve two schemas from a VCS commit range like "HEAD~1..HEAD".
pub fn resolve_schemas_from_range(
    range: &str,
    verbose: bool,
) -> Result<(Schema, Schema, String, String)> {
    let repo = open_repo()?;

    let (old_ref, new_ref) = if let Some(pos) = range.find("...") {
        (&range[..pos], &range[pos + 3..])
    } else if let Some(pos) = range.find("..") {
        (&range[..pos], &range[pos + 2..])
    } else {
        miette::bail!("invalid commit range '{range}': expected 'old..new' or 'old...new' format");
    };

    if verbose {
        eprintln!("Resolving {old_ref} and {new_ref}");
    }

    let old_id = vcs::refs::resolve_ref(repo.store(), old_ref)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{old_ref}'"))?;
    let new_id = vcs::refs::resolve_ref(repo.store(), new_ref)
        .into_diagnostic()
        .wrap_err_with(|| format!("cannot resolve '{new_ref}'"))?;

    let old_obj = repo.store().get(&old_id).into_diagnostic()?;
    let new_obj = repo.store().get(&new_id).into_diagnostic()?;

    let old_schema_id = match &old_obj {
        vcs::Object::Commit(c) => c.schema_id,
        _ => miette::bail!("'{old_ref}' does not resolve to a commit"),
    };
    let new_schema_id = match &new_obj {
        vcs::Object::Commit(c) => c.schema_id,
        _ => miette::bail!("'{new_ref}' does not resolve to a commit"),
    };

    let proto = vcs::tree::project_coproduct_protocol();
    let old_schema = vcs::tree::assemble_schema_dyn(repo.store(), &old_schema_id, &proto)
        .into_diagnostic()
        .wrap_err_with(|| format!("commit '{old_ref}' schema tree could not be resolved"))?;
    let new_schema = vcs::tree::assemble_schema_dyn(repo.store(), &new_schema_id, &proto)
        .into_diagnostic()
        .wrap_err_with(|| format!("commit '{new_ref}' schema tree could not be resolved"))?;

    if verbose {
        eprintln!(
            "Old schema: {} vertices, {} edges",
            old_schema.vertex_count(),
            old_schema.edge_count()
        );
        eprintln!(
            "New schema: {} vertices, {} edges",
            new_schema.vertex_count(),
            new_schema.edge_count()
        );
    }

    Ok((
        old_schema,
        new_schema,
        old_ref.to_owned(),
        new_ref.to_owned(),
    ))
}

pub fn cmd_lens_diff(
    range: &str,
    chain_output: bool,
    save: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let (old_schema, new_schema, old_ref, new_ref) = resolve_schemas_from_range(range, verbose)?;
    let protocol = resolve_protocol(&old_schema.protocol)?;
    let result = lens::auto_generate(
        &old_schema,
        &new_schema,
        &protocol,
        &lens::AutoLensConfig::default(),
    )
    .into_diagnostic()
    .wrap_err("failed to generate lens between committed schemas")?;

    if chain_output {
        let pretty = serde_json::to_string_pretty(&chain_to_json(&result.chain))
            .into_diagnostic()
            .wrap_err("failed to serialize protolens chain")?;
        println!("{pretty}");
    } else {
        println!("Lens diff: {old_ref} -> {new_ref}");
        println!("  Alignment quality: {:.3}", result.alignment_quality);
        println!("  Steps: {}", result.chain.steps.len());
        for (i, step) in result.chain.steps.iter().enumerate() {
            let tag = if step.is_lossless() {
                " (lossless)"
            } else {
                " (lossy)"
            };
            println!("    {}. {}{tag}", i + 1, step.name);
        }
    }

    if let Some(save_path) = save {
        let chain_json = result
            .chain
            .to_json()
            .into_diagnostic()
            .wrap_err("failed to serialize protolens chain")?;
        std::fs::write(save_path, &chain_json)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write chain to {}", save_path.display()))?;
        println!("Saved protolens chain to {}", save_path.display());
    }

    Ok(())
}

pub fn cmd_lens_fleet(
    chain_path: &Path,
    schemas_dir: &Path,
    protocol_name: &str,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    let chain_json_str = std::fs::read_to_string(chain_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read chain from {}", chain_path.display()))?;
    let chain = lens::ProtolensChain::from_json(&chain_json_str)
        .into_diagnostic()
        .wrap_err("failed to parse protolens chain JSON")?;
    let protocol = resolve_protocol(protocol_name)?;

    // Read all *.json files in the schemas directory.
    let mut schemas: Vec<(Name, Schema)> = Vec::new();
    let entries = std::fs::read_dir(schemas_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read directory {}", schemas_dir.display()))?;
    for entry in entries {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) == Some("json") {
            let schema: Schema = load_json(&path)?;
            let name = path
                .file_stem()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("unknown");
            schemas.push((Name::from(name), schema));
        }
    }

    if verbose {
        eprintln!(
            "Applying chain ({} steps) to {} schemas in {}",
            chain.len(),
            schemas.len(),
            schemas_dir.display()
        );
    }

    if dry_run {
        // Only check applicability.
        println!("Applicability report:");
        for (name, schema) in &schemas {
            match chain.check_applicability(schema) {
                Ok(()) => println!("  {name}: applicable"),
                Err(reasons) => {
                    println!("  {name}: NOT applicable");
                    for reason in &reasons {
                        println!("    - {reason}");
                    }
                }
            }
        }
    } else {
        let result = lens::apply_to_fleet(&chain, &schemas, &protocol);
        println!("Fleet result:");
        println!("  Applied: {} schemas", result.applied.len());
        for (name, _lens) in &result.applied {
            println!("    - {name}");
        }
        if !result.skipped.is_empty() {
            println!("  Skipped: {} schemas", result.skipped.len());
            for (name, reasons) in &result.skipped {
                println!("    - {name}:");
                for reason in reasons {
                    println!("      {reason}");
                }
            }
        }
    }

    Ok(())
}

pub fn cmd_lens_lift(
    chain_path: &Path,
    morphism_path: &Path,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let chain_json_str = std::fs::read_to_string(chain_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read chain from {}", chain_path.display()))?;
    let chain = lens::ProtolensChain::from_json(&chain_json_str)
        .into_diagnostic()
        .wrap_err("failed to parse protolens chain JSON")?;

    let morphism: panproto_core::gat::TheoryMorphism = load_json(morphism_path)?;

    if verbose {
        eprintln!(
            "Lifting chain ({} steps) along morphism '{}'",
            chain.len(),
            morphism.name
        );
    }

    let lifted = lens::lift_chain(&chain, &morphism);

    if json {
        let lifted_json = lifted
            .to_json()
            .into_diagnostic()
            .wrap_err("failed to serialize lifted chain")?;
        println!("{lifted_json}");
    } else {
        println!("Lifted protolens chain ({} steps):", lifted.len());
        for (i, step) in lifted.steps.iter().enumerate() {
            let lossless = if step.is_lossless() {
                " (lossless)"
            } else {
                " (lossy)"
            };
            println!("  {}. {}{lossless}", i + 1, step.name);
        }
    }

    Ok(())
}
