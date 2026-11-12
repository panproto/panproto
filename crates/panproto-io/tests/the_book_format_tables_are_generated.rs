//! The book's format tables are generated from the registries and the
//! recorded results of each format.
//!
//! `book/formats.toml` names the formats under the format contract and the
//! formats and variants that are not supported. This test checks every
//! record against what the code and the repository hold: the protocol
//! registry (each protocol's reader and declared shapes), the instance
//! codecs of [`default_registry`], the grammar registry (each grammar
//! compiled into this build, with its upstream, revision and licence), the
//! conformance corpora and their limits files, and the correctness pages
//! and the correctness inventory. It then writes the tables of
//! `book/src/reference/formats.md` from what it found, and fails when the
//! committed page differs. Every protocol of the registry without a format
//! record is listed as not under the format contract, so a protocol cannot
//! be advertised as supported by being left out.
//!
//! Every excluded format or variant is tested here as well: a name no
//! protocol or codec answers to, a grammar the grammar registry does not
//! have, or a reader run on a document of the variant that returns the
//! typed refusal its probe names.
//!
//! Regenerate the page after a deliberate change with
//! `PP_FORMAT_TABLES=bless cargo test -p panproto-io --features tree-sitter --test the_book_format_tables_are_generated`.

#![cfg(feature = "tree-sitter")]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Write as _};
use std::path::{Path, PathBuf};

use panproto_io::config::{cloudformation, k8s_crd, tree};
use panproto_io::data_schema::json_schema;
use panproto_io::rdf::RdfReadErrorKind;
use panproto_io::{ParseInstanceError, ProtocolRegistry, default_registry};
use panproto_io::{api, postgresql};
use panproto_protocols::database::postgresql::OtherStatements;
use panproto_protocols::registry::{Parser, ProtocolDescriptor, descriptor, descriptors};
use panproto_schema::InstanceShapeKind;
use serde::Deserialize;

/// The marker lines the generated part of the page sits between.
const BEGIN: &str =
    "<!-- The tables below are generated from book/formats.toml; do not edit them by hand. -->";
const END: &str = "<!-- End of the generated tables. -->";

/// Where the repository's files are linked from the book.
const GITHUB: &str = "https://github.com/panproto/panproto/blob/main";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Records {
    format: Vec<Format>,
    excluded: Vec<Excluded>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Format {
    name: String,
    protocols: Vec<String>,
    standard: String,
    pinned: String,
    pages: Vec<String>,
    corpora: Vec<String>,
    limits: Vec<String>,
    grammars: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Excluded {
    name: String,
    reason: String,
    page: Option<String>,
    protocol: Option<String>,
    #[serde(default)]
    unregistered: Vec<String>,
    #[serde(default)]
    ungrammared: Vec<String>,
    #[serde(default)]
    refusals: Vec<String>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| panic!("the repository root is two levels above panproto-io"))
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn conformance() -> PathBuf {
    root().join("crates/panproto-io/tests/fixtures/conformance")
}

const fn shape(kind: InstanceShapeKind) -> &'static str {
    match kind {
        InstanceShapeKind::Tree => "tree",
        InstanceShapeKind::Table => "table",
        InstanceShapeKind::Graph => "graph",
    }
}

/// The instance shapes the default codecs read for `protocol`.
fn codecs(registry: &ProtocolRegistry, protocol: &str) -> String {
    let shapes: Vec<&str> = [
        (registry.supports_tree(protocol), "tree"),
        (registry.supports_table(protocol), "table"),
        (registry.supports_graph(protocol), "graph"),
    ]
    .into_iter()
    .filter_map(|(supported, name)| supported.then_some(name))
    .collect();
    if shapes.is_empty() {
        "none".to_owned()
    } else {
        shapes.join(", ")
    }
}

/// A `crate::module::function` path as a docs.rs link.
fn docs_link(path: &str) -> String {
    let mut parts: Vec<&str> = path.split("::").collect();
    let function = parts.pop().unwrap_or_default();
    let krate = parts.first().copied().unwrap_or_default().replace('_', "-");
    let module = parts.join("/");
    format!("[`{function}`](https://docs.rs/{krate}/latest/{module}/fn.{function}.html)")
}

/// The first `# ` heading of a correctness page, and the page's text.
fn page(name: &str) -> (String, String) {
    let (file, _) = name.split_once('#').unwrap_or((name, ""));
    let text = read(&root().join(format!("book/src/correctness/{file}.md")));
    let title = text
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .unwrap_or_else(|| panic!("the correctness page {file} has no title"))
        .to_owned();
    (title, text)
}

fn page_link(name: &str) -> String {
    let (file, anchor) = name.split_once('#').unwrap_or((name, ""));
    let (title, _) = page(file);
    if anchor.is_empty() {
        format!("[{title}](../correctness/{file}.md)")
    } else {
        format!("[{title}](../correctness/{file}.md#{anchor})")
    }
}

/// The correctness pages the inventory lists, each with at least one law.
fn inventory_pages() -> BTreeSet<String> {
    let text =
        read(&root().join("crates/panproto-reference/tests/fixtures/correctness_manifest.json"));
    let manifest = panproto_reference::manifest::load(&text)
        .unwrap_or_else(|e| panic!("the correctness inventory does not load: {e:?}"));
    manifest
        .pages
        .into_iter()
        .filter(|(_, page)| !page.laws.is_empty())
        .map(|(name, _)| name)
        .collect()
}

/// The entries of a limits file: its lines that are neither blank nor
/// comments.
fn limit_count(file: &str) -> usize {
    read(&conformance().join(file))
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .count()
}

/// The conformance tests that name `corpus`.
fn conformance_tests(corpus: &str) -> Vec<String> {
    let dir = root().join("crates/panproto-io/tests");
    let needles = [
        format!("\"{corpus}\""),
        format!("\"{corpus}/"),
        format!("conformance/{corpus}\""),
        format!("conformance/{corpus}/"),
    ];
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("listing {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with("conformance_")
                && Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        })
        .filter(|name| {
            let text = read(&dir.join(name));
            needles.iter().any(|needle| text.contains(needle.as_str()))
        })
        .collect();
    out.sort();
    out
}

/// The grammar registry's entries: each grammar's upstream repository.
fn grammar_manifest() -> BTreeMap<String, String> {
    let text = read(&root().join("grammars.toml"));
    let table: toml::Table =
        toml::from_str(&text).unwrap_or_else(|e| panic!("grammars.toml does not parse: {e}"));
    table
        .into_iter()
        .filter_map(|(name, entry)| {
            let repo = entry.get("repo")?.as_str()?.to_owned();
            Some((name, repo))
        })
        .collect()
}

/// A grammar compiled into this build, with its upstream, the revision
/// vendored and its licence file.
fn grammar_cell(name: &str, manifest: &BTreeMap<String, String>) -> String {
    assert!(
        panproto_grammars::has_grammar(name),
        "the grammar {name} is not compiled into panproto-io's tree-sitter build"
    );
    let repo = manifest
        .get(name)
        .unwrap_or_else(|| panic!("the grammar {name} has no entry in grammars.toml"));
    let dir = root().join("grammars").join(name);
    assert!(
        dir.join("LICENSE").is_file(),
        "the grammar {name} has no LICENSE file"
    );
    let revision = read(&dir.join("REVISION"));
    let revision = revision.trim();
    let short = revision.get(..8).unwrap_or(revision);
    format!("[`{name}`]({repo}) at `{short}`")
}

fn protocol_reader(d: &ProtocolDescriptor) -> String {
    match d.parser {
        Parser::Grammar { reader } => docs_link(reader),
        Parser::Programmatic => "the instance codecs of [`default_registry`](https://docs.rs/panproto-io/latest/panproto_io/fn.default_registry.html)".to_owned(),
        Parser::Document(_) | Parser::Source(_) => {
            panic!("{} is read by a reader that predates the format contract", d.name)
        }
    }
}

/// One refusal probe: a reader run on a document of an excluded variant.
struct Probe {
    id: &'static str,
    /// The reader, as the table names it.
    reader: &'static str,
    /// The document's distinguishing part, as the table names it.
    input: &'static str,
    /// The refusal the reader must return.
    refusal: &'static str,
    /// Runs the reader: the refusal it returned, or a description of what
    /// it did instead.
    run: fn() -> Result<String, String>,
}

/// The name of an enum value's variant, from its `Debug` form.
fn variant<T: Debug>(value: &T) -> String {
    let text = format!("{value:?}");
    text.split(['(', ' ', '{'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn refused<T: Debug, E>(
    result: Result<T, E>,
    kind: impl Fn(&E) -> String,
) -> Result<String, String> {
    match result {
        Ok(read) => Err(format!("it was read: {read:?}")),
        Err(error) => Ok(kind(&error)),
    }
}

fn rdf(protocol: &str, schema: &panproto_schema::Schema, text: &str) -> Result<String, String> {
    refused(
        default_registry().parse_graph(protocol, schema, text.as_bytes()),
        |error| match error {
            ParseInstanceError::Rdf { error, .. } => match &error.kind {
                RdfReadErrorKind::JsonLd { refusal, .. } => variant(refusal),
                kind => variant(kind),
            },
            other => format!("{other:?}"),
        },
    )
}

/// Run the default instance codec of `protocol` on `bytes`, under the
/// schema the protocol's own schema reader gives.
fn instance(
    protocol: &str,
    schema: Result<panproto_schema::Schema, panproto_protocols::ProtocolError>,
    bytes: &[u8],
) -> Result<String, String> {
    let schema = schema.unwrap_or_else(|e| panic!("the {protocol} schema does not read: {e}"));
    refused(
        default_registry().parse_wtype(protocol, &schema, bytes),
        variant,
    )
}

fn graph_schema(
    built: Result<panproto_schema::Schema, panproto_schema::SchemaError>,
) -> panproto_schema::Schema {
    built.unwrap_or_else(|e| panic!("the RDF graph schema does not build: {e}"))
}

const OPENAPI_INFO: &str = r#""info":{"title":"t","version":"1"},"paths":{}"#;

const CRD_WITH_WEBHOOK: &str = "apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: widgets.example.com
spec:
  group: example.com
  names:
    plural: widgets
    singular: widget
    kind: Widget
  scope: Namespaced
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
  conversion:
    strategy: Webhook
";

const PROBES: &[Probe] = &[
    Probe {
        id: "yaml-1.1",
        reader: "panproto_io::config::tree::read_yaml",
        input: "`%YAML 1.1`",
        refusal: "Parse",
        run: || refused(tree::read_yaml(b"%YAML 1.1\n---\na: 1\n"), variant),
    },
    Probe {
        id: "mysql-quoted-identifier",
        reader: "panproto_io::postgresql::read_ddl",
        input: "a MySQL backquoted identifier",
        refusal: "Unsupported",
        run: || {
            refused(
                postgresql::read_ddl("CREATE TABLE `t` (id integer);", OtherStatements::Refuse),
                variant,
            )
        },
    },
    Probe {
        id: "nquads-graph-label",
        reader: "the `ntriples` graph codec",
        input: "a quad with a graph label",
        refusal: "Syntax",
        run: || {
            rdf(
                "ntriples",
                &graph_schema(panproto_protocols::rdf::ntriples::graph_schema()),
                "<http://e/a> <http://e/b> <http://e/c> <http://e/g> .\n",
            )
        },
    },
    Probe {
        id: "trig-graph-block",
        reader: "the `turtle` graph codec",
        input: "a TriG graph block",
        refusal: "NotInFormat",
        run: || {
            rdf(
                "turtle",
                &graph_schema(panproto_protocols::rdf::turtle::graph_schema()),
                "<http://e/g> { <http://e/a> <http://e/b> <http://e/c> . }\n",
            )
        },
    },
    Probe {
        id: "rdf-star-quoted-triple",
        reader: "the `ntriples` graph codec",
        input: "a quoted triple `<< ... >>`",
        refusal: "Syntax",
        run: || {
            rdf(
                "ntriples",
                &graph_schema(panproto_protocols::rdf::ntriples::graph_schema()),
                "<< <http://e/a> <http://e/b> <http://e/c> >> <http://e/p> <http://e/o> .\n",
            )
        },
    },
    Probe {
        id: "jsonld-named-graph",
        reader: "the `jsonld` graph codec",
        input: "a named graph",
        refusal: "NamedGraph",
        run: || {
            rdf(
                "jsonld",
                &graph_schema(panproto_protocols::rdf::jsonld::graph_schema()),
                r#"{"@id":"http://e/g","@graph":[{"@id":"http://e/a","http://e/b":"c"}]}"#,
            )
        },
    },
    Probe {
        id: "swagger-2.0",
        reader: "panproto_io::api::openapi::read_openapi",
        input: "`swagger: 2.0`",
        refusal: "Swagger",
        run: || {
            refused(
                api::openapi::read_openapi(
                    format!(r#"{{"swagger":"2.0",{OPENAPI_INFO}}}"#).as_bytes(),
                ),
                |e| variant(&e.kind),
            )
        },
    },
    Probe {
        id: "openapi-3.0",
        reader: "panproto_io::api::openapi::read_openapi",
        input: "`openapi: 3.0.3`",
        refusal: "Version",
        run: || {
            refused(
                api::openapi::read_openapi(
                    format!(r#"{{"openapi":"3.0.3",{OPENAPI_INFO}}}"#).as_bytes(),
                ),
                |e| variant(&e.kind),
            )
        },
    },
    Probe {
        id: "openapi-3.2",
        reader: "panproto_io::api::openapi::read_openapi",
        input: "`openapi: 3.2.0`",
        refusal: "Version",
        run: || {
            refused(
                api::openapi::read_openapi(
                    format!(r#"{{"openapi":"3.2.0",{OPENAPI_INFO}}}"#).as_bytes(),
                ),
                |e| variant(&e.kind),
            )
        },
    },
    Probe {
        id: "asyncapi-2",
        reader: "panproto_io::api::asyncapi::read_asyncapi",
        input: "`asyncapi: 2.6.0`",
        refusal: "Version",
        run: || {
            refused(
                api::asyncapi::read_asyncapi(
                    br#"{"asyncapi":"2.6.0","info":{"title":"t","version":"1"},"channels":{}}"#,
                ),
                |e| variant(&e.kind),
            )
        },
    },
    Probe {
        id: "json-schema-draft-07",
        reader: "panproto_io::data_schema::json_schema::read_schema",
        input: "a draft-07 `$schema`",
        refusal: "UnsupportedDialect",
        run: || {
            refused(
                json_schema::read_schema(
                    br#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object"}"#,
                ),
                |e| variant(&e.kind),
            )
        },
    },
    Probe {
        id: "cloudformation-transform",
        reader: "panproto_io::config::cloudformation::read_template",
        input: "a `Transform` section",
        refusal: "Unsupported",
        run: || {
            refused(
                cloudformation::read_template(
                    br#"{"AWSTemplateFormatVersion":"2010-09-09","Transform":"AWS::Serverless-2016-10-31","Resources":{"B":{"Type":"AWS::S3::Bucket"}}}"#,
                ),
                |e| variant(&e.kind),
            )
        },
    },
    Probe {
        id: "k8s-conversion-webhook",
        reader: "panproto_io::config::k8s_crd::read_crd",
        input: "`conversion.strategy: Webhook`",
        refusal: "Unsupported",
        run: || {
            refused(k8s_crd::read_crd(CRD_WITH_WEBHOOK.as_bytes()), |e| {
                variant(&e.kind)
            })
        },
    },
    Probe {
        id: "brat-ann",
        reader: "panproto_protocols::parse_schema_source",
        input: "a brat `.ann` line given as `brat` source text",
        refusal: "Parse",
        run: || {
            refused(
                panproto_protocols::parse_schema_source("brat", "T1\tPerson 0 5\tAlice\n"),
                variant,
            )
        },
    },
    Probe {
        id: "avro-binary",
        reader: "the `avro` instance codec",
        input: "an Avro object container file",
        refusal: "Json",
        run: || {
            instance(
                "avro",
                panproto_protocols::parse_schema_document(
                    "avro",
                    &serde_json::json!({"type": "record", "name": "R", "fields": [{"name": "a", "type": "int"}]}),
                ),
                b"Obj\x01\x00\x00",
            )
        },
    },
    Probe {
        id: "protobuf-wire",
        reader: "the instance registry",
        input: "a Protobuf wire-format message under `protobuf`",
        refusal: "UnknownProtocol",
        run: || {
            instance(
                "protobuf",
                panproto_protocols::parse_schema_source(
                    "protobuf",
                    "syntax = \"proto3\";\nmessage R { int32 a = 1; }\n",
                ),
                b"\x08\x05",
            )
        },
    },
    Probe {
        id: "asn1-der",
        reader: "the `asn1` instance codec",
        input: "a DER encoding",
        refusal: "Json",
        run: || {
            instance(
                "asn1",
                panproto_protocols::parse_schema_source(
                    "asn1",
                    "M DEFINITIONS ::= BEGIN\nR ::= SEQUENCE { a INTEGER }\nEND\n",
                ),
                b"\x30\x03\x02\x01\x05",
            )
        },
    },
    Probe {
        id: "bond-binary",
        reader: "the `bond` instance codec",
        input: "a Bond compact binary payload",
        refusal: "Json",
        run: || {
            instance(
                "bond",
                panproto_protocols::parse_schema_source(
                    "bond",
                    "namespace n\nstruct R { 0: int32 a; }\n",
                ),
                b"\x10\x05\x00",
            )
        },
    },
];

fn probe(id: &str) -> &'static Probe {
    PROBES
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no refusal probe is named {id}"))
}

/// The reader cell of a probe: a docs.rs link for a function path, the
/// text as it is otherwise.
fn probe_reader(probe: &Probe) -> String {
    if probe.reader.contains("::") {
        docs_link(probe.reader)
    } else {
        probe.reader.to_owned()
    }
}

/// Run a probe and check it returns the refusal it names: the table's
/// cell, or what went wrong.
fn run_probe(probe: &Probe) -> Result<String, String> {
    match (probe.run)() {
        Ok(observed) if observed == probe.refusal => Ok(format!(
            "{} refuses {} with `{}`",
            probe_reader(probe),
            probe.input,
            probe.refusal
        )),
        Ok(observed) => Err(format!(
            "the probe {} expected the refusal {} and observed {observed}",
            probe.id, probe.refusal
        )),
        Err(accepted) => Err(format!(
            "the probe {} expected a refusal: {accepted}",
            probe.id
        )),
    }
}

/// What the tables are generated from.
struct Context {
    records: Records,
    registry: ProtocolRegistry,
    grammars: BTreeMap<String, String>,
    inventory: BTreeSet<String>,
}

impl Context {
    fn load() -> Self {
        Self {
            records: toml::from_str(&read(&root().join("book/formats.toml")))
                .unwrap_or_else(|e| panic!("book/formats.toml does not parse: {e}")),
            registry: default_registry(),
            grammars: grammar_manifest(),
            inventory: inventory_pages(),
        }
    }

    /// The protocols that have a format record.
    fn covered(&self) -> BTreeSet<&'static str> {
        let mut covered = BTreeSet::new();
        for name in self.records.format.iter().flat_map(|f| &f.protocols) {
            let d = descriptor(name).unwrap_or_else(|| panic!("{name} is not a protocol"));
            assert_eq!(
                d.name, name,
                "{name} is not the canonical name of {}",
                d.name
            );
            assert!(covered.insert(d.name), "{name} has two format records");
        }
        // A grammar reader is written under the contract, so a protocol read
        // by one has a format record.
        for d in descriptors() {
            if matches!(d.parser, Parser::Grammar { .. }) {
                assert!(
                    covered.contains(d.name),
                    "{} is read by a grammar reader and has no format record in book/formats.toml",
                    d.name
                );
            }
        }
        covered
    }

    /// The row of the supported table for `format`.
    fn supported_row(&self, format: &Format) -> String {
        assert!(
            !format.protocols.is_empty(),
            "{} names no protocol",
            format.name
        );
        assert!(
            !format.pages.is_empty(),
            "{} names no correctness page",
            format.name
        );
        let mut protocols = Vec::new();
        let mut readers: Vec<String> = Vec::new();
        let mut shapes: Vec<String> = Vec::new();
        let mut instance: Vec<String> = Vec::new();
        for name in &format.protocols {
            let d = descriptor(name).unwrap_or_else(|| panic!("{name} is not a protocol"));
            let protocol = (d.protocol)();
            protocols.push(format!("`{}`", d.name));
            for (list, cell) in [
                (&mut readers, protocol_reader(d)),
                (
                    &mut shapes,
                    format!(
                        "{} / {}",
                        shape(protocol.syntactic_shape),
                        shape(protocol.semantic_shape)
                    ),
                ),
                (&mut instance, codecs(&self.registry, d.name)),
            ] {
                if !list.contains(&cell) {
                    list.push(cell);
                }
            }
        }
        let (_, first_page) = page(&format.pages[0]);
        assert!(
            first_page.contains(&format.pinned),
            "the correctness page {} does not state {:?}, the version {} pins",
            format.pages[0],
            format.pinned,
            format.name
        );
        let pages: Vec<String> = format
            .pages
            .iter()
            .map(|p| {
                assert!(
                    self.inventory.contains(p),
                    "the correctness page {p} of {} proves no law in the correctness inventory",
                    format.name
                );
                page_link(p)
            })
            .collect();
        format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            format.name,
            protocols.join(", "),
            format.standard,
            shapes.join("; "),
            instance.join("; "),
            readers.join("; "),
            pages.join(", ")
        )
    }

    /// The row of the evidence table for `format`.
    fn evidence_row(&self, format: &Format) -> String {
        assert!(
            !format.corpora.is_empty(),
            "{} names no conformance corpus",
            format.name
        );
        let grammars: Vec<String> = format
            .grammars
            .iter()
            .map(|g| grammar_cell(g, &self.grammars))
            .collect();
        let corpora: Vec<String> = format
            .corpora
            .iter()
            .map(|corpus| {
                assert!(
                    conformance().join(corpus).join("UPSTREAM.md").is_file(),
                    "the corpus {corpus} has no UPSTREAM.md"
                );
                let tests = conformance_tests(corpus);
                assert!(!tests.is_empty(), "no conformance test runs the corpus {corpus}");
                let tests: Vec<String> = tests.iter().map(|t| format!("`{t}`")).collect();
                format!(
                    "[{corpus}]({GITHUB}/crates/panproto-io/tests/fixtures/conformance/{corpus}/UPSTREAM.md), run by {}",
                    tests.join(" and ")
                )
            })
            .collect();
        let limits = if format.limits.is_empty() {
            "none".to_owned()
        } else {
            format
                .limits
                .iter()
                .map(|file| {
                    format!(
                        "{} in [`{file}`]({GITHUB}/crates/panproto-io/tests/fixtures/conformance/{file})",
                        limit_count(file)
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        format!(
            "| {} | {} | {} | {} |\n",
            format.name,
            grammars.join(", "),
            corpora.join("; "),
            limits
        )
    }

    /// The checks an exclusion passes, as the table states them.
    fn exclusion_tests(&self, excluded: &Excluded, covered: &BTreeSet<&str>) -> Vec<String> {
        let mut tests: Vec<String> = Vec::new();
        let mut coded: Vec<String> = Vec::new();
        for name in &excluded.unregistered {
            assert!(
                descriptor(name).is_none(),
                "{name} is excluded but resolves to the protocol {:?}",
                descriptor(name).map(|d| d.name)
            );
            assert!(
                !self.registry.protocol_names().any(|n| n == name),
                "{name} is excluded but has an instance codec"
            );
            if self.grammars.contains_key(name.as_str()) {
                coded.push(format!("`{name}`"));
            }
        }
        if !excluded.unregistered.is_empty() {
            let names: Vec<String> = excluded
                .unregistered
                .iter()
                .map(|n| format!("`{n}`"))
                .collect();
            let mut cell = format!(
                "no protocol or instance codec is named {}",
                listed(&names, "or")
            );
            if !coded.is_empty() {
                let (names, grammars) = if coded.len() == 1 {
                    ("names", "a code grammar, which parses")
                } else {
                    ("name", "code grammars, which parse")
                };
                write!(
                    cell,
                    " ({} {names} only {grammars} a file as source code)",
                    listed(&coded, "and")
                )
                .unwrap_or_default();
            }
            tests.push(cell);
        }
        for name in &excluded.ungrammared {
            assert!(
                !self.grammars.contains_key(name.as_str()),
                "{name} is excluded for want of a grammar but grammars.toml lists one"
            );
            tests.push(format!("no grammar named `{name}` is vendored"));
        }
        if let Some(name) = &excluded.protocol {
            let d = descriptor(name).unwrap_or_else(|| panic!("{name} is not a protocol"));
            assert!(
                !covered.contains(d.name),
                "{name} is both under the format contract and excluded"
            );
            let reads = match d.parser {
                Parser::Document(_) => "`parse_schema_document`",
                Parser::Source(_) => "`parse_schema_source`",
                Parser::Grammar { .. } | Parser::Programmatic => {
                    panic!("{name} is excluded but has no reader that predates the contract")
                }
            };
            tests.push(format!(
                "`{}` is read only by its earlier reader, through {reads}",
                d.name
            ));
        }
        let failures: Vec<String> = excluded
            .refusals
            .iter()
            .filter_map(|id| match run_probe(probe(id)) {
                Ok(cell) => {
                    tests.push(cell);
                    None
                }
                Err(failure) => Some(failure),
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        assert!(
            !tests.is_empty(),
            "the exclusion {} is tested by nothing",
            excluded.name
        );
        tests
    }

    /// The rows of the exclusion table.
    fn exclusion_rows(&self, covered: &BTreeSet<&str>) -> String {
        let mut used: BTreeSet<&str> = BTreeSet::new();
        for id in self.records.excluded.iter().flat_map(|e| &e.refusals) {
            assert!(used.insert(id.as_str()), "the probe {id} is used twice");
        }
        let unused: Vec<&str> = PROBES
            .iter()
            .map(|p| p.id)
            .filter(|id| !used.contains(id))
            .collect();
        assert!(
            unused.is_empty(),
            "these probes are used by no exclusion: {unused:?}"
        );
        let mut rows = String::new();
        for excluded in &self.records.excluded {
            let tests = self.exclusion_tests(excluded, covered);
            let reason = excluded.page.as_ref().map_or_else(
                || excluded.reason.clone(),
                |p| format!("{} ({})", excluded.reason, page_link(p)),
            );
            writeln!(
                rows,
                "| {} | {} | {} |",
                excluded.name,
                reason,
                capitalise(&tests.join("; "))
            )
            .unwrap_or_default();
        }
        rows
    }

    /// The rows of the table of protocols not under the contract.
    fn pending_rows(&self, covered: &BTreeSet<&str>) -> String {
        let mut rows = String::new();
        for d in descriptors().iter().filter(|d| !covered.contains(d.name)) {
            let reads = match d.parser {
                Parser::Document(_) => "a JSON document, through `parse_schema_document`",
                Parser::Source(_) => "text, through `parse_schema_source`",
                Parser::Programmatic => "no schema documents; schemas are built in code",
                Parser::Grammar { .. } => {
                    panic!(
                        "{} is read by a grammar reader and has no format record",
                        d.name
                    )
                }
            };
            let status = self
                .records
                .excluded
                .iter()
                .find(|e| e.protocol.as_deref() == Some(d.name))
                .map_or_else(
                    || {
                        "Read by a reader that predates the format contract; not yet run \
                         against a conformance corpus or a reference"
                            .to_owned()
                    },
                    |excluded| format!("{}; see the exclusions above", excluded.reason),
                );
            let protocol = (d.protocol)();
            writeln!(
                rows,
                "| `{}` | {reads} | {} / {} | {} | {status} |",
                d.name,
                shape(protocol.syntactic_shape),
                shape(protocol.semantic_shape),
                codecs(&self.registry, d.name)
            )
            .unwrap_or_default();
        }
        rows
    }
}

/// The generated part of the page.
fn generate() -> String {
    let context = Context::load();
    let covered = context.covered();
    let supported: String = context
        .records
        .format
        .iter()
        .map(|f| context.supported_row(f))
        .collect();
    let evidence: String = context
        .records
        .format
        .iter()
        .map(|f| context.evidence_row(f))
        .collect();
    let exclusions = context.exclusion_rows(&covered);
    let pending = context.pending_rows(&covered);
    format!(
        "{BEGIN}

## Supported formats

| Format | Protocols | Standard read | Syntax / data read as | Instance codecs | Reader | Correctness |
|---|---|---|---|---|---|---|
{supported}
### Evidence

| Format | Grammars | Conformance corpora | Declared limits |
|---|---|---|---|
{evidence}
## Formats not supported

| Format or variant | Reason | Tested by |
|---|---|---|
{exclusions}
## Protocols not under the format contract

| Protocol | Reads schemas from | Syntax / data declared | Instance codecs | Status |
|---|---|---|---|---|
{pending}
{END}"
    )
}

/// `items` as an English list joined by `conjunction`.
fn listed(items: &[String], conjunction: &str) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} {conjunction} {last}", init.join(", ")),
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

fn blessing() -> bool {
    std::env::var("PP_FORMAT_TABLES").is_ok_and(|v| v == "bless")
}

#[test]
fn the_format_tables_match_the_records_and_the_registries() {
    let path = root().join("book/src/reference/formats.md");
    let text = read(&path);
    let start = text
        .find(BEGIN)
        .unwrap_or_else(|| panic!("{} has no start marker", path.display()));
    let end = text.find(END).map_or_else(
        || panic!("{} has no end marker", path.display()),
        |at| at + END.len(),
    );
    let generated = generate();
    if blessing() {
        let updated = format!("{}{generated}{}", &text[..start], &text[end..]);
        std::fs::write(&path, updated)
            .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
        return;
    }
    let committed = &text[start..end];
    if committed != generated {
        let first = committed
            .lines()
            .zip(generated.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| committed.lines().count().min(generated.lines().count()));
        panic!(
            "the format tables of {} differ from what the records and the registries give, \
             first at line {} of the generated part:\n  committed: {:?}\n  generated: {:?}\n\
             Regenerate with PP_FORMAT_TABLES=bless cargo test -p panproto-io --features \
             tree-sitter --test the_book_format_tables_are_generated",
            path.display(),
            first + 1,
            committed.lines().nth(first),
            generated.lines().nth(first),
        );
    }
}
