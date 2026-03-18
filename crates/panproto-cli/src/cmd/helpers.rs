use std::collections::HashMap;
use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use panproto_core::{
    gat::{Name, Theory},
    inst, lens, protocols,
    schema::{Protocol, Schema},
    vcs,
};

/// Load and parse a JSON file into a typed value.
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = std::fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read {}", path.display()))?;

    serde_json::from_str(&contents)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to parse JSON from {}", path.display()))
}

/// Resolve a protocol by name from built-in definitions.
pub fn resolve_protocol(name: &str) -> Result<Protocol> {
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
pub fn build_theory_registry(protocol_name: &str) -> Result<HashMap<String, Theory>> {
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
pub fn open_repo() -> Result<vcs::Repository> {
    // Try current directory first.
    let cwd = std::env::current_dir().into_diagnostic()?;
    vcs::Repository::open(&cwd)
        .into_diagnostic()
        .wrap_err("not a panproto repository (or any parent up to mount point)")
}

/// Parse default values from `key=value` strings into a map.
pub fn parse_defaults(
    defaults: &[String],
) -> Result<HashMap<Name, panproto_core::inst::value::Value>> {
    let mut map = HashMap::new();
    for entry in defaults {
        let parts: Vec<&str> = entry.splitn(2, '=').collect();
        if parts.len() != 2 {
            miette::bail!("invalid default '{entry}': expected 'key=value' format");
        }
        let key = Name::from(parts[0]);
        let value = panproto_core::inst::value::Value::Str(parts[1].to_string());
        map.insert(key, value);
    }
    Ok(map)
}

/// Infer the root vertex of a schema (the vertex with no incoming edges, or
/// the first vertex alphabetically).
pub fn infer_root_vertex(schema: &Schema) -> Result<Name> {
    let targets: std::collections::HashSet<&Name> = schema.edges.keys().map(|e| &e.tgt).collect();
    let root = schema
        .vertices
        .keys()
        .find(|v| !targets.contains(v))
        .or_else(|| schema.vertices.keys().next())
        .ok_or_else(|| miette::miette!("schema has no vertices"))?;
    Ok(root.clone())
}

/// Build a serializable summary of an `AutoLensResult` for JSON output.
pub fn auto_lens_result_to_json(result: &lens::AutoLensResult) -> serde_json::Value {
    let steps: Vec<serde_json::Value> = result
        .chain
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            serde_json::json!({
                "step": i + 1,
                "name": step.name.as_str(),
                "lossless": step.is_lossless(),
            })
        })
        .collect();
    serde_json::json!({
        "alignment_quality": result.alignment_quality,
        "steps": steps,
        "step_count": result.chain.steps.len(),
    })
}

/// Build a serializable chain representation for `--chain` output.
pub fn chain_to_json(chain: &lens::ProtolensChain) -> serde_json::Value {
    let steps: Vec<serde_json::Value> = chain
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            serde_json::json!({
                "step": i + 1,
                "name": step.name.as_str(),
                "lossless": step.is_lossless(),
            })
        })
        .collect();
    serde_json::json!({
        "type": "protolens_chain",
        "steps": steps,
        "step_count": chain.steps.len(),
    })
}

pub fn format_timestamp(ts: u64) -> String {
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

/// Parse a range string like "old..new" into two ref strings.
pub fn parse_range(s: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = s.splitn(2, "..").collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        miette::bail!("invalid range '{s}': expected 'old..new' format");
    }
    Ok((parts[0].to_owned(), parts[1].to_owned()))
}

/// Load a commit object from the store by its ID.
pub fn load_commit_obj(store: &dyn vcs::Store, id: vcs::ObjectId) -> Result<vcs::CommitObject> {
    let obj = store.get(&id).into_diagnostic()?;
    match obj {
        vcs::Object::Commit(c) => Ok(c),
        other => miette::bail!(
            "expected commit at {}, found {}",
            id.short(),
            other.type_name()
        ),
    }
}

/// Load a schema object from the store by its ID.
pub fn load_schema_from_store(store: &dyn vcs::Store, id: vcs::ObjectId) -> Result<Schema> {
    let obj = store.get(&id).into_diagnostic()?;
    match obj {
        vcs::Object::Schema(s) => Ok(*s),
        other => miette::bail!(
            "expected schema at {}, found {}",
            id.short(),
            other.type_name()
        ),
    }
}

/// Read all *.json files from a directory, sorted by filename.
pub fn read_json_dir(dir: &Path) -> Result<Vec<std::fs::DirEntry>> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read directory {}", dir.display()))?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    Ok(entries)
}

/// Convert a single JSON file through a lens.
pub fn convert_single_file(
    path: &Path,
    src_schema: &Schema,
    tgt_schema: &Schema,
    the_lens: &lens::Lens,
    direction: &str,
) -> Result<String> {
    let data_json: serde_json::Value = load_json(path)?;
    let is_forward = direction == "forward";
    let forward_schema = if is_forward { src_schema } else { tgt_schema };
    let backward_schema = if is_forward { tgt_schema } else { src_schema };

    let root_vertex = infer_root_vertex(forward_schema)?;
    let instance = inst::parse_json(forward_schema, root_vertex.as_str(), &data_json)
        .into_diagnostic()
        .wrap_err("failed to parse data as W-type instance")?;

    let output_instance = if is_forward {
        let (view, _complement) = lens::get(the_lens, &instance)
            .into_diagnostic()
            .wrap_err("lens get (forward) failed")?;
        view
    } else {
        let complement = lens::Complement {
            dropped_nodes: HashMap::new(),
            dropped_arcs: Vec::new(),
            dropped_fans: Vec::new(),
            contraction_choices: HashMap::new(),
            original_parent: HashMap::new(),
        };
        lens::put(the_lens, &instance, &complement)
            .into_diagnostic()
            .wrap_err("lens put (backward) failed")?
    };

    let output = inst::to_json(backward_schema, &output_instance);
    serde_json::to_string_pretty(&output)
        .into_diagnostic()
        .wrap_err("failed to serialize output")
}

/// Build a model from a schema for a given theory.
///
/// Maps vertex-like sorts to schema vertex IDs and edge-like sorts to
/// schema edge representations. Other sorts get a small free model carrier.
pub fn build_schema_model(
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

/// Print a theory-level diff between two schemas (sorts/operations at the GAT level).
pub fn print_theory_diff(old_schema: &Schema, new_schema: &Schema) {
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

/// Print complement requirements for a protolens chain.
pub fn print_complement_requirements(
    chain: &lens::ProtolensChain,
    src_schema: &Schema,
    protocol: &Protocol,
) {
    let spec = lens::chain_complement_spec(chain, src_schema, protocol);
    if !spec.forward_defaults.is_empty() {
        println!("Requirements:");
        for req in &spec.forward_defaults {
            println!(
                "  + {} ({}, default needed)",
                req.element_name, req.element_kind
            );
        }
    }
    if !spec.captured_data.is_empty() {
        for cap in &spec.captured_data {
            println!("  - {} (captured in complement)", cap.element_name);
        }
    }
}
