//! Generate (or check) `tests/fixtures/complement_upgrade/`.
//!
//! Each case directory holds the inputs of one lens (a protocol, the build
//! operations of its source and target schemas, a migration and a source
//! instance) and the complement that lens's `get` produced for that instance
//! in the legacy form written before complements carried an identity
//! envelope. The complement is stored in the four encodings the workspace
//! reads it in:
//!
//! * `*.pos.msgpack`: `rmp_serde::to_vec`, positional (the VCS form);
//! * `*.named.msgpack`: `rmp_serde::to_vec_named` (the WASM/TS form);
//! * `*.cbor`: `ciborium::ser::into_writer`, the C ABI's canonical CBOR (the
//!   Swift/Haskell form);
//! * `*.json`: `serde_json::to_vec`, compact (the Python form).
//!
//! `native.*` files are the output of a 64-bit host. `wasm32.named.msgpack` is
//! the output of the wasm32 build, taken through `get_record` by
//! `produce-wasm32.mjs` in the fixture directory; the other `wasm32.*` files
//! are that complement transcoded into the remaining encodings.
//!
//! The files were first written by the build that produced legacy
//! complements. This generator now writes them from the current `get` by
//! dropping the envelope and writing the fingerprint with
//! `legacy_schema_fingerprint` at each width, so `check` is also the proof
//! that the reimplemented fingerprint and the current `get` reproduce the
//! legacy output byte for byte.
//!
//! Run from the repo root:
//!
//! ```sh
//! cargo run -p xtask --bin gen-complement-upgrade -- write
//! cargo run -p xtask --bin gen-complement-upgrade -- check
//! ```
//!
//! `write` regenerates every file but the frozen `wasm32.named.msgpack`,
//! which it checks instead; `check` regenerates in memory and fails on any
//! difference from the files on disk.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use panproto_gat::Name;
use panproto_inst::identity::{LegacyWidth, legacy_schema_fingerprint};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{Complement, Fan, Node, WInstance};
use panproto_lens::Lens;
use panproto_mig::Migration;
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use serde::{Deserialize, Serialize};

/// The fixture directory, relative to the repo root.
const FIXTURE_DIR: &str = "tests/fixtures/complement_upgrade";

/// One schema build operation, in the shape the WASM `build_schema` entry
/// point reads.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
enum BuildOp {
    /// Add a vertex.
    #[serde(rename = "vertex")]
    Vertex {
        /// Vertex identifier.
        id: String,
        /// Vertex kind.
        kind: String,
        /// Optional NSID.
        nsid: Option<String>,
    },
    /// Add a binary edge.
    #[serde(rename = "edge")]
    Edge {
        /// Source vertex.
        src: String,
        /// Target vertex.
        tgt: String,
        /// Edge kind.
        kind: String,
        /// Optional edge label.
        name: Option<String>,
    },
}

/// The complement exactly as the legacy format lays it out: twelve fields,
/// in this order, with these serde attributes.
///
/// This is the wire shape every file in the fixture directory carries, so
/// it is spelled out here rather than borrowed from [`Complement`], whose
/// later fields a legacy reader never saw.
#[derive(Debug, Default, Serialize, Deserialize)]
struct LegacyComplement {
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    dropped_nodes: HashMap<u32, Node>,
    dropped_arcs: Vec<(u32, u32, Edge)>,
    dropped_fans: Vec<Fan>,
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    contraction_choices: HashMap<(u32, u32), Edge>,
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    original_parent: HashMap<u32, u32>,
    #[serde(default)]
    source_fingerprint: u64,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_nested_map")]
    original_extra_fields: HashMap<u32, HashMap<String, Value>>,
    #[serde(default, with = "panproto_schema::serde_helpers::map_as_vec_default")]
    arc_edges: HashMap<(u32, u32), Edge>,
    #[serde(default)]
    arc_order: Vec<(u32, u32)>,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_map")]
    original_values: HashMap<u32, Option<FieldPresence>>,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_set")]
    synthesized_nodes: HashSet<u32>,
    #[serde(default, with = "panproto_schema::serde_helpers::sorted_map")]
    contracted_into: HashMap<u32, u32>,
}

impl LegacyComplement {
    /// The legacy form of a complement `get` produced: its envelope
    /// dropped, and its fingerprint the one a host of `width` wrote.
    ///
    /// A complement that records original anchors has no legacy form, since
    /// the legacy layout has nowhere to put them.
    fn from_complement(c: Complement, source: &Schema, width: LegacyWidth) -> Result<Self, String> {
        if !c.original_anchors.is_empty() {
            return Err("the legacy layout cannot carry original anchors".to_owned());
        }
        Ok(Self {
            dropped_nodes: c.dropped_nodes,
            dropped_arcs: c.dropped_arcs,
            dropped_fans: c.dropped_fans,
            contraction_choices: c.contraction_choices,
            original_parent: c.original_parent,
            source_fingerprint: legacy_schema_fingerprint(source, width),
            original_extra_fields: c.original_extra_fields,
            arc_edges: c.arc_edges,
            arc_order: c.arc_order,
            original_values: c.original_values,
            synthesized_nodes: c.synthesized_nodes,
            contracted_into: c.contracted_into,
        })
    }
}

/// One lens and one source instance.
struct Case {
    /// Directory name.
    name: &'static str,
    /// Source schema build operations.
    src_ops: Vec<BuildOp>,
    /// Target schema build operations.
    tgt_ops: Vec<BuildOp>,
    /// The migration from source to target.
    migration: Migration,
    /// The source instance.
    instance: WInstance,
}

fn protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        obj_kinds: vec!["object".into(), "string".into(), "record".into()],
        ..Default::default()
    }
}

fn vertex(id: &str, kind: &str, nsid: Option<&str>) -> BuildOp {
    BuildOp::Vertex {
        id: id.to_owned(),
        kind: kind.to_owned(),
        nsid: nsid.map(str::to_owned),
    }
}

fn edge_op(src: &str, tgt: &str, name: &str) -> BuildOp {
    BuildOp::Edge {
        src: src.to_owned(),
        tgt: tgt.to_owned(),
        kind: "prop".to_owned(),
        name: Some(name.to_owned()),
    }
}

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn string_node(id: u32, anchor: &str, s: &str) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(Value::Str(s.into())))
}

fn migration(
    vertices: &[(&str, &str)],
    edges: Vec<(Edge, Edge)>,
    resolver: Vec<((&str, &str), Edge)>,
) -> Migration {
    Migration {
        vertex_map: vertices
            .iter()
            .map(|(s, t)| (Name::from(*s), Name::from(*t)))
            .collect(),
        edge_map: edges.into_iter().collect(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: resolver
            .into_iter()
            .map(|((s, t), e)| ((Name::from(s), Name::from(t)), e))
            .collect(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
        coercions: HashMap::new(),
        domain: None,
        codomain: None,
    }
}

fn instance(nodes: Vec<Node>, arcs: Vec<(u32, u32, Edge)>, schema_root: &str) -> WInstance {
    let nodes = nodes.into_iter().map(|n| (n.id, n)).collect();
    WInstance::new(nodes, arcs, Vec::new(), 0, Name::from(schema_root))
}

fn cases() -> Vec<Case> {
    vec![drop_field(), rename_vertex(), contract(), ordered_list()]
}

fn drop_field() -> Case {
    let text = prop("post", "post.text", "text");
    Case {
        name: "drop_field",
        src_ops: vec![
            vertex("post", "record", Some("app.test.post")),
            vertex("post.text", "string", None),
            vertex("post.subtitle", "string", None),
            edge_op("post", "post.text", "text"),
            edge_op("post", "post.subtitle", "subtitle"),
        ],
        tgt_ops: vec![
            vertex("post", "record", Some("app.test.post")),
            vertex("post.text", "string", None),
            edge_op("post", "post.text", "text"),
        ],
        migration: migration(
            &[("post", "post"), ("post.text", "post.text")],
            vec![(text.clone(), text.clone())],
            vec![],
        ),
        instance: instance(
            vec![
                Node::new(0, "post").with_extra_field("$type", Value::Str("app.test.post".into())),
                string_node(1, "post.text", "hello"),
                string_node(2, "post.subtitle", "world"),
            ],
            vec![
                (0, 1, text),
                (0, 2, prop("post", "post.subtitle", "subtitle")),
            ],
            "post",
        ),
    }
}

fn rename_vertex() -> Case {
    let text = prop("post", "post.text", "text");
    Case {
        name: "rename_vertex",
        src_ops: vec![
            vertex("post", "record", Some("app.test.post")),
            vertex("post.text", "string", None),
            edge_op("post", "post.text", "text"),
        ],
        tgt_ops: vec![
            vertex("article", "record", Some("app.test.article")),
            vertex("article.body", "string", None),
            edge_op("article", "article.body", "body"),
        ],
        migration: migration(
            &[("post", "article"), ("post.text", "article.body")],
            vec![(text.clone(), prop("article", "article.body", "body"))],
            vec![],
        ),
        instance: instance(
            vec![Node::new(0, "post"), string_node(1, "post.text", "renamed")],
            vec![(0, 1, text)],
            "post",
        ),
    }
}

fn contract() -> Case {
    Case {
        name: "contract",
        src_ops: vec![
            vertex("post", "record", Some("app.test.post")),
            vertex("post.meta", "object", None),
            vertex("post.meta.lang", "string", None),
            edge_op("post", "post.meta", "meta"),
            edge_op("post.meta", "post.meta.lang", "lang"),
        ],
        tgt_ops: vec![
            vertex("post", "record", Some("app.test.post")),
            vertex("post.meta.lang", "string", None),
            edge_op("post", "post.meta.lang", "lang"),
        ],
        migration: migration(
            &[("post", "post"), ("post.meta.lang", "post.meta.lang")],
            vec![],
            vec![(
                ("post", "post.meta.lang"),
                prop("post", "post.meta.lang", "lang"),
            )],
        ),
        instance: instance(
            vec![
                Node::new(0, "post"),
                Node::new(1, "post.meta"),
                string_node(2, "post.meta.lang", "en"),
            ],
            vec![
                (0, 1, prop("post", "post.meta", "meta")),
                (1, 2, prop("post.meta", "post.meta.lang", "lang")),
            ],
            "post",
        ),
    }
}

fn ordered_list() -> Case {
    let tag = prop("post", "post.tag", "tags");
    let note = prop("post", "post.note", "notes");
    Case {
        name: "ordered_list",
        src_ops: vec![
            vertex("post", "record", Some("app.test.post")),
            vertex("post.tag", "string", None),
            vertex("post.note", "string", None),
            edge_op("post", "post.tag", "tags"),
            edge_op("post", "post.note", "notes"),
        ],
        tgt_ops: vec![
            vertex("post", "record", Some("app.test.post")),
            vertex("post.tag", "string", None),
            edge_op("post", "post.tag", "tags"),
        ],
        migration: migration(
            &[("post", "post"), ("post.tag", "post.tag")],
            vec![(tag.clone(), tag.clone())],
            vec![],
        ),
        instance: instance(
            vec![
                Node::new(0, "post"),
                string_node(1, "post.tag", "a"),
                string_node(2, "post.note", "x"),
                string_node(3, "post.tag", "b"),
                string_node(4, "post.note", "y"),
                string_node(5, "post.tag", "c"),
            ],
            vec![
                (0, 1, tag.clone()),
                (0, 2, note.clone()),
                (0, 3, tag.clone()),
                (0, 4, note),
                (0, 5, tag),
            ],
            "post",
        ),
    }
}

fn build_schema(ops: &[BuildOp]) -> Result<Schema, String> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    for op in ops {
        builder = match op {
            BuildOp::Vertex { id, kind, nsid } => builder
                .vertex(id, kind, nsid.as_deref())
                .map_err(|e| e.to_string())?,
            BuildOp::Edge {
                src,
                tgt,
                kind,
                name,
            } => builder
                .edge(src, tgt, kind, name.as_deref())
                .map_err(|e| e.to_string())?,
        };
    }
    builder.build().map_err(|e| e.to_string())
}

/// The four encodings of one value.
fn encodings<T: Serialize>(value: &T) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let pos = rmp_serde::to_vec(value).map_err(|e| e.to_string())?;
    let named = rmp_serde::to_vec_named(value).map_err(|e| e.to_string())?;
    let mut cbor = Vec::new();
    ciborium::ser::into_writer(value, &mut cbor).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    Ok(vec![
        ("pos.msgpack", pos),
        ("named.msgpack", named),
        ("cbor", cbor),
        ("json", json),
    ])
}

/// A value's JSON tree with every object's keys in ascending order.
///
/// The inputs carry plain `HashMap`s (a migration's `vertex_map`, for one),
/// whose serialized order follows the per-process hash seed. Sorting the
/// keys here is what makes the input files a function of the case alone.
fn sorted_tree<T: Serialize>(value: &T) -> Result<serde_json::Value, String> {
    fn sort(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut entries: Vec<(String, serde_json::Value)> = map.into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                serde_json::Value::Object(entries.into_iter().map(|(k, v)| (k, sort(v))).collect())
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.into_iter().map(sort).collect())
            }
            other => other,
        }
    }
    serde_json::to_value(value)
        .map(sort)
        .map_err(|e| e.to_string())
}

/// Named msgpack of an input, with map keys in ascending order.
fn named<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    rmp_serde::to_vec_named(&sorted_tree(value)?).map_err(|e| e.to_string())
}

/// Pretty JSON of an input, with map keys in ascending order.
fn pretty_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut out = serde_json::to_vec_pretty(&sorted_tree(value)?).map_err(|e| e.to_string())?;
    out.push(b'\n');
    Ok(out)
}

/// Every file of one case, keyed by file name.
fn case_files(dir: &Path, case: &Case) -> Result<Vec<(String, Vec<u8>)>, String> {
    // Inputs, as JSON for reading and as named msgpack for the WASM driver.
    // An instance serializes its maps in key order already, and its integer
    // node keys must stay integers for the WASM reader, so it is encoded
    // directly rather than through the sorted JSON tree.
    let mut files = vec![
        ("protocol.json".to_owned(), pretty_json(&protocol())?),
        ("src_ops.json".to_owned(), pretty_json(&case.src_ops)?),
        ("tgt_ops.json".to_owned(), pretty_json(&case.tgt_ops)?),
        ("migration.json".to_owned(), pretty_json(&case.migration)?),
        ("instance.json".to_owned(), pretty_json(&case.instance)?),
        ("protocol.msgpack".to_owned(), named(&protocol())?),
        ("src_ops.msgpack".to_owned(), named(&case.src_ops)?),
        ("tgt_ops.msgpack".to_owned(), named(&case.tgt_ops)?),
        ("migration.msgpack".to_owned(), named(&case.migration)?),
        (
            "instance.msgpack".to_owned(),
            rmp_serde::to_vec_named(&case.instance).map_err(|e| e.to_string())?,
        ),
    ];

    // The native complement.
    let src = build_schema(&case.src_ops)?;
    let tgt = build_schema(&case.tgt_ops)?;
    let compiled = panproto_mig::compile(&src, &tgt, &case.migration).map_err(|e| e.to_string())?;
    let lens = Lens {
        compiled,
        src_schema: src,
        tgt_schema: tgt,
    };
    let (_, complement) = panproto_lens::get(&lens, &case.instance).map_err(|e| e.to_string())?;
    let native = LegacyComplement::from_complement(
        complement.clone(),
        &lens.src_schema,
        LegacyWidth::Bits64,
    )?;
    for (ext, bytes) in encodings(&native)? {
        files.push((format!("native.{ext}"), bytes));
    }

    // The wasm32 complement. Its named msgpack is the WASM driver's output,
    // frozen; the legacy form of this build's complement at 32-bit width
    // must reproduce it byte for byte, and the other encodings are written
    // from that.
    let wasm =
        LegacyComplement::from_complement(complement, &lens.src_schema, LegacyWidth::Bits32)?;
    let wasm_named = dir.join("wasm32.named.msgpack");
    let frozen = std::fs::read(&wasm_named).map_err(|e| {
        format!(
            "{}: {e}; produce it with produce-wasm32.mjs",
            wasm_named.display()
        )
    })?;
    for (ext, encoded) in encodings(&wasm)? {
        if ext == "named.msgpack" {
            if encoded != frozen {
                return Err(format!(
                    "{}: the legacy wasm32 form of this build's complement differs from the \
                     frozen one",
                    wasm_named.display()
                ));
            }
            continue;
        }
        files.push((format!("wasm32.{ext}"), encoded));
    }

    let mut fingerprints = serde_json::Map::new();
    fingerprints.insert(
        "native".to_owned(),
        serde_json::Value::from(native.source_fingerprint),
    );
    fingerprints.insert(
        "wasm32".to_owned(),
        serde_json::Value::from(wasm.source_fingerprint),
    );
    files.push((
        "fingerprints.json".to_owned(),
        pretty_json(&serde_json::Value::Object(fingerprints))?,
    ));

    Ok(files)
}

fn run(check: bool) -> Result<(), String> {
    let root = PathBuf::from(FIXTURE_DIR);
    let mut drift = Vec::new();
    for case in cases() {
        let dir = root.join(case.name);
        if !check {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        for (file, bytes) in case_files(&dir, &case)? {
            let path = dir.join(&file);
            if check {
                match std::fs::read(&path) {
                    Ok(existing) if existing == bytes => {}
                    Ok(_) => drift.push(format!("{} differs", path.display())),
                    Err(e) => drift.push(format!("{}: {e}", path.display())),
                }
            } else {
                std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            }
        }
    }
    if drift.is_empty() {
        Ok(())
    } else {
        Err(drift.join("\n"))
    }
}

fn main() -> ExitCode {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let check = match mode.as_str() {
        "write" => false,
        "check" => true,
        _ => {
            eprintln!("usage: gen-complement-upgrade (write|check)");
            return ExitCode::from(2);
        }
    };
    match run(check) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
