//! The corpus the identity tests hash: every protocol fixture schema, the
//! lenses of the legacy-complement fixtures, and lenses generated between
//! fixture schemas.
//!
//! `tests/fixtures/identity/golden.json` lists the corpus and the digest
//! of each entry. The file drives the tests: a schema entry names its
//! protocol and source file, and a lens entry names its `kind` and that
//! kind's parameters. Adding an entry of an existing kind needs no code;
//! adding a kind needs a builder in [`build_lens`], which rejects kinds it
//! does not know by name so that a missing builder cannot pass silently.

#![allow(
    dead_code,
    reason = "each test binary that includes this module uses a different part of it"
)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use panproto_inst::identity::{ComplementShape, IdentityEnvelope};
use panproto_lens::Lens;
use panproto_mig::Migration;
use panproto_schema::{Protocol, Schema, SchemaBuilder};
use serde::{Deserialize, Serialize};

/// The golden file, relative to the repository root.
pub const GOLDEN: &str = "tests/fixtures/identity/golden.json";

/// The repository root.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

/// The golden file's contents.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Golden {
    /// The key-derivation contexts, by role. A change to any of them is a
    /// change to every digest.
    pub contexts: BTreeMap<String, String>,
    /// The envelope layout version.
    pub envelope_version: u16,
    /// Schemas, each with its digest.
    pub schemas: Vec<SchemaEntry>,
    /// Lenses, each with its envelope.
    pub lenses: Vec<LensEntry>,
}

/// A schema read from a fixture file by its protocol's parser.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemaEntry {
    /// A name unique in the file.
    pub name: String,
    /// The protocol whose parser reads the file.
    pub protocol: String,
    /// The fixture file, relative to the repository root.
    pub source: String,
    /// The schema's digest, as hex.
    pub digest: String,
}

/// A lens, and the envelope it stamps.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LensEntry {
    /// A name unique in the file.
    pub name: String,
    /// How the lens is built, and from what.
    #[serde(flatten)]
    pub kind: LensKind,
    /// The complement shape the envelope is for.
    pub shape: ComplementShape,
    /// The envelope.
    pub envelope: IdentityEnvelope,
}

/// How a corpus lens is built.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LensKind {
    /// A migration compiled between two schemas built from operations: a
    /// case directory of `tests/fixtures/complement_upgrade`.
    Migration {
        /// The case directory, relative to the repository root.
        case: String,
    },
    /// A lens `auto_generate` finds between two corpus schemas under the
    /// default configuration.
    Auto {
        /// The protocol both schemas belong to.
        protocol: String,
        /// The source schema's corpus name.
        src: String,
        /// The target schema's corpus name.
        tgt: String,
    },
    /// A kind this build has no builder for.
    #[serde(other)]
    Unknown,
}

/// Read the golden file.
pub fn read_golden() -> Golden {
    let path = repo_root().join(GOLDEN);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The protocol a registered name resolves to.
pub fn protocol_named(name: &str) -> Protocol {
    let descriptor = panproto_protocols::registry::descriptor(name)
        .unwrap_or_else(|| panic!("no protocol named {name}"));
    (descriptor.protocol)()
}

/// Read a corpus schema with its protocol's parser.
pub fn load_schema(entry: &SchemaEntry) -> Schema {
    let path = repo_root().join(&entry.source);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let parsed =
        if panproto_protocols::document_parser_protocols().contains(&entry.protocol.as_str()) {
            let doc: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            panproto_protocols::parse_schema_document(&entry.protocol, &doc)
        } else {
            panproto_protocols::parse_schema_source(&entry.protocol, &text)
        };
    parsed.unwrap_or_else(|e| panic!("{}: {e}", entry.name))
}

/// One schema build operation, as the complement-upgrade cases store them.
#[derive(Deserialize)]
#[serde(tag = "op")]
enum BuildOp {
    #[serde(rename = "vertex")]
    Vertex {
        id: String,
        kind: String,
        nsid: Option<String>,
    },
    #[serde(rename = "edge")]
    Edge {
        src: String,
        tgt: String,
        kind: String,
        name: Option<String>,
    },
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn build_from_ops(protocol: &Protocol, ops: &[BuildOp]) -> Schema {
    let mut builder = SchemaBuilder::new(protocol);
    for op in ops {
        builder = match op {
            BuildOp::Vertex { id, kind, nsid } => builder
                .vertex(id, kind, nsid.as_deref())
                .expect("a case vertex builds"),
            BuildOp::Edge {
                src,
                tgt,
                kind,
                name,
            } => builder
                .edge(src, tgt, kind, name.as_deref())
                .expect("a case edge builds"),
        };
    }
    builder.build().expect("a case schema builds")
}

/// The lens of a `tests/fixtures/complement_upgrade` case directory.
pub fn case_lens(case_dir: &Path) -> Lens {
    let protocol: Protocol = read_json(&case_dir.join("protocol.json"));
    let src_ops: Vec<BuildOp> = read_json(&case_dir.join("src_ops.json"));
    let tgt_ops: Vec<BuildOp> = read_json(&case_dir.join("tgt_ops.json"));
    let migration: Migration = read_json(&case_dir.join("migration.json"));
    let src = build_from_ops(&protocol, &src_ops);
    let tgt = build_from_ops(&protocol, &tgt_ops);
    let compiled = panproto_mig::compile(&src, &tgt, &migration).expect("a case compiles");
    Lens {
        compiled,
        src_schema: src,
        tgt_schema: tgt,
    }
}

/// Build a corpus lens.
///
/// # Errors
///
/// A message naming the entry when its kind has no builder or the build
/// fails.
pub fn build_lens(entry: &LensEntry, schemas: &HashMap<String, Schema>) -> Result<Lens, String> {
    match &entry.kind {
        LensKind::Migration { case } => Ok(case_lens(&repo_root().join(case))),
        LensKind::Auto { protocol, src, tgt } => {
            let schema = |name: &str| {
                schemas
                    .get(name)
                    .ok_or_else(|| format!("{}: no corpus schema named {name}", entry.name))
            };
            panproto_lens::auto_generate(
                schema(src)?,
                schema(tgt)?,
                &protocol_named(protocol),
                &panproto_lens::AutoLensConfig::default(),
            )
            .map(|result| result.lens)
            .map_err(|e| format!("{}: {e}", entry.name))
        }
        LensKind::Unknown => Err(format!(
            "{}: this build has no builder for the lens's kind; add one to \
             tests/integration/tests/identity_corpus/mod.rs",
            entry.name
        )),
    }
}

/// Every corpus schema, by name.
pub fn load_schemas(golden: &Golden) -> HashMap<String, Schema> {
    golden
        .schemas
        .iter()
        .map(|entry| (entry.name.clone(), load_schema(entry)))
        .collect()
}

/// Every digest the corpus defines, as `name -> hex`, in file order: each
/// schema's digest, and each lens's envelope rendered as JSON.
pub fn every_identity(golden: &Golden) -> Vec<(String, String)> {
    let schemas = load_schemas(golden);
    let mut out = Vec::new();
    for entry in &golden.schemas {
        out.push((
            format!("schema {}", entry.name),
            panproto_inst::identity::schema_digest(&schemas[&entry.name]).to_hex(),
        ));
    }
    for entry in &golden.lenses {
        let lens = build_lens(entry, &schemas).unwrap_or_else(|e| panic!("{e}"));
        let envelope = lens.identity(entry.shape);
        out.push((
            format!("lens {}", entry.name),
            serde_json::to_string(&envelope).expect("an envelope serializes"),
        ));
    }
    out
}
