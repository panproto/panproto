//! Does the hash-seed divergence in the span's right leg escape, or does a
//! later guard catch it?
//!
//! Follows one unchanged pair of schemas all the way out: the derived
//! `Migration`, the migration's content address, and the record lifted through
//! it both ways. Each child process prints its answer; the parent runs many
//! children and compares.
//!
//! Two lines of the dump are reported but not asserted on. They measure the
//! bytes a store writes for a schema, which is a different divergence with a
//! different cause: `Schema` keeps its vertices, edges and adjacency indices in
//! `HashMap`s, and serialising one walks those maps, so the bytes vary per
//! process whether the schema came from a merge or straight from
//! `SchemaBuilder`. Sorting what goes *into* an index does not settle the order
//! the map is later read out in. The counts are printed so that a reader can
//! see the two divergences are separate.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::{BTreeMap, HashMap};
use std::process::Command;

use panproto_inst::{FieldPresence, Node, Value, WInstance};
use panproto_schema::{Edge, EdgeRule, Protocol, Schema, SchemaBuilder};
use panproto_vcs::auto_mig::derive_migration;
use panproto_vcs::hash::{hash_migration, hash_schema};
use panproto_vcs::merge::three_way_merge;

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 48;

fn protocol() -> Protocol {
    Protocol {
        name: "determinism".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["string".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned(), "string".to_owned()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// The new side: one object, one string, joined by the named property edges.
fn new_side(names: &[&str]) -> Schema {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto)
        .vertex("rec", "object", None::<&str>)
        .expect("rec")
        .vertex("new_field", "string", None::<&str>)
        .expect("new_field");
    for name in names {
        builder = builder
            .edge("rec", "new_field", "prop", Some(*name))
            .expect("edge");
    }
    builder.entry("rec").build().expect("build")
}

/// The new schema, as a merge of two sides that each added two parallel edges.
fn merged_new() -> Schema {
    let base = new_side(&[]);
    let ours = new_side(&["alpha", "beta"]);
    let theirs = new_side(&["gamma", "delta"]);
    let merged = three_way_merge(&base, &ours, &theirs);
    assert!(merged.conflicts.is_empty(), "disjoint additions");
    merged.merged_schema
}

/// The old schema: the same object, a differently named string field, and one
/// property edge whose name the new side does not carry.
fn old_schema() -> Schema {
    SchemaBuilder::new(&protocol())
        .vertex("rec", "object", None::<&str>)
        .expect("rec")
        .vertex("old_field", "string", None::<&str>)
        .expect("old_field")
        .edge("rec", "old_field", "prop", Some("omega"))
        .expect("edge")
        .entry("rec")
        .build()
        .expect("build")
}

/// One record under the old schema: `rec.omega = "hello"`.
fn record(old: &Schema) -> WInstance {
    let edge = old
        .edges
        .keys()
        .find(|e| e.name.as_deref() == Some("omega"))
        .expect("the omega edge")
        .clone();
    let mut root = Node::new(0, "rec");
    root.value = None;
    let mut leaf = Node::new(1, "old_field");
    leaf.value = Some(FieldPresence::Present(Value::Str("hello".to_owned())));
    let nodes: HashMap<u32, Node> = HashMap::from([(0, root), (1, leaf)]);
    WInstance::new(nodes, vec![(0, 1, edge)], Vec::new(), 0, "rec".into())
}

fn show(edge: &Edge) -> String {
    format!(
        "{}->{}({}:{})",
        edge.src,
        edge.tgt,
        edge.kind,
        edge.name.as_deref().unwrap_or("-")
    )
}

fn hex(id: &panproto_vcs::ObjectId) -> String {
    id.to_string()
}

/// Everything one process answers about one unchanged pair of schemas.
fn dump() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let old = old_schema();
    let new = merged_new();

    // 1. The merged schema's content address, and the bytes a store writes.
    let address = hash_schema(&new).expect("hash");
    let _ = writeln!(out, "merged schema content address = {}", hex(&address));

    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = panproto_vcs::FsStore::init(dir.path()).expect("init");
    let root = panproto_vcs::tree::store_schema_as_tree(&mut store, new.clone()).expect("store");
    let _ = writeln!(out, "stored tree object id = {}", hex(&root));
    let mut on_disk: BTreeMap<String, String> = BTreeMap::new();
    for entry in walkdir(dir.path().join(".panproto").join("objects")) {
        let bytes = std::fs::read(&entry).expect("read object");
        let name = entry
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file = entry.file_name().map(|s| s.to_string_lossy().into_owned());
        on_disk.insert(
            format!("{name}{}", file.unwrap_or_default()),
            blake3::hash(&bytes).to_hex().to_string(),
        );
    }
    let _ = writeln!(out, "object id -> blake3(stored bytes) = {on_disk:?}");

    // 1b. The same measurement on a schema that was never merged, built only by
    // `SchemaBuilder`. If its bytes vary too, the byte-level divergence is a
    // property of serialising a `Schema` at all rather than of the merge.
    let dir2 = tempfile::tempdir().expect("tempdir");
    let mut store2 = panproto_vcs::FsStore::init(dir2.path()).expect("init");
    let _ = panproto_vcs::tree::store_schema_as_tree(&mut store2, old.clone()).expect("store");
    let mut unmerged: BTreeMap<String, String> = BTreeMap::new();
    for entry in walkdir(dir2.path().join(".panproto").join("objects")) {
        let bytes = std::fs::read(&entry).expect("read object");
        let name = entry
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file = entry.file_name().map(|s| s.to_string_lossy().into_owned());
        unmerged.insert(
            format!("{name}{}", file.unwrap_or_default()),
            blake3::hash(&bytes).to_hex().to_string(),
        );
    }
    let _ = writeln!(
        out,
        "builder-only schema: object id -> blake3(stored bytes) = {unmerged:?}"
    );

    // 2. The derived migration and its content address.
    let diff = panproto_check::diff::diff(&old, &new);
    let mig = derive_migration(&old, &new, &diff);
    let edges: BTreeMap<String, String> = mig
        .edge_map
        .iter()
        .map(|(k, v)| (show(k), show(v)))
        .collect();
    let _ = writeln!(out, "derived migration edge map = {edges:?}");
    let old_id = hash_schema(&old).expect("hash");
    let mig_id = hash_migration(old_id, address, &mig).expect("hash");
    let _ = writeln!(out, "migration content address = {}", hex(&mig_id));

    // 3. The record, lifted through that migration, both ways.
    let compiled = panproto_mig::compile(&old, &new, &mig).expect("compile");
    let describe = |lifted: &WInstance| {
        let mut arcs: Vec<String> = lifted
            .arcs
            .iter()
            .map(|(p, c, e)| {
                let value = lifted.nodes.get(c).and_then(|n| n.value.clone());
                format!("{p}->{c} via {} carrying {value:?}", show(e))
            })
            .collect();
        arcs.sort();
        format!("{arcs:?}")
    };
    let pi = panproto_mig::lift_wtype(&compiled, &old, &new, &record(&old))
        .map_or_else(|e| format!("Err({e})"), |i| describe(&i));
    let _ = writeln!(out, "restrict-lifted record arcs = {pi}");
    let sigma = panproto_mig::lift_wtype_sigma(&compiled, &new, &record(&old))
        .map_or_else(|e| format!("Err({e})"), |i| describe(&i));
    let _ = writeln!(out, "sigma-lifted record arcs = {sigma}");

    out
}

/// Every regular file under `root`, recursively.
fn walkdir(root: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The child half: one process's answer, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_ESCAPE_DUMP").is_none() {
        return;
    }
    print!("<<<{}>>>", dump());
}

fn child_answer(exe: &std::path::Path) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_ESCAPE_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

/// One pair of schemas, followed out to the migration, the commit-level
/// content address, the lifted record and the stored bytes, in many processes.
#[test]
fn nothing_downstream_of_the_span_depends_on_the_hash_seed() {
    if std::env::var_os("PP_ESCAPE_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut distinct: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *distinct.entry(child_answer(&exe)).or_insert(0) += 1;
    }

    let mut per_line: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    for (answer, count) in &distinct {
        for line in answer.lines() {
            let (key, _) = line.split_once('=').unwrap_or((line, ""));
            *per_line
                .entry(key.trim())
                .or_default()
                .entry(line.to_owned())
                .or_insert(0) += count;
        }
    }

    let mut report = String::new();
    let mut disagreements = 0usize;
    for (key, values) in &per_line {
        use std::fmt::Write as _;
        if is_stored_bytes(key) {
            let _ = writeln!(
                report,
                "[{}] {key} (reported, not asserted: a separate divergence)",
                values.len()
            );
            continue;
        }
        let _ = writeln!(report, "[{}] {key}", values.len());
        for (value, count) in values {
            let _ = writeln!(report, "    x{count}  {value}");
        }
        if values.len() > 1 {
            disagreements += 1;
        }
    }

    assert_eq!(
        disagreements, 0,
        "{PROCESSES} processes disagreed about one unchanged pair of schemas in \
         {disagreements} of the quantities the span decides.\n{report}"
    );
}

/// Whether a dump line measures stored bytes rather than something the span
/// decides.
fn is_stored_bytes(key: &str) -> bool {
    key.starts_with("object id ->") || key.starts_with("builder-only schema:")
}
