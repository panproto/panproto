//! Keys and references: their serde and canonical forms, the builder,
//! well-formedness, and how induction, morphisms and pushouts carry them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::Name;
use panproto_schema::{
    CanonicalWriter, Cardinality, DeclarationReport, Edge, Key, KeyPart, KeyRefError, KeyRole,
    KeyScope, NullMatch, OffsetUnit, OnUnresolved, PathSyntax, Protocol, RefSource, RefTextSource,
    Reference, ResolveMode, Schema, SchemaBuilder, SchemaDeclarations, SchemaError, SchemaOverlap,
    SpanEnd, UriBase, VariableBinding, canonical_bytes, induce_on_vertices, schema_pushout,
};
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

fn protocol() -> Protocol {
    Protocol {
        name: "keys".to_owned(),
        ..Default::default()
    }
}

fn edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: Name::from(src),
        tgt: Name::from(tgt),
        kind: Name::from(kind),
        name: Some(Name::from(name)),
    }
}

/// A film table with a composite key and a store table whose
/// `film_id` column references it: the SQL shape of a foreign key.
fn base_schema() -> Schema {
    SchemaBuilder::new(&protocol())
        .vertex("doc", "object", None)
        .expect("doc")
        .vertex("film", "table", None)
        .expect("film")
        .vertex("film.id", "integer", None)
        .expect("film.id")
        .vertex("film.lang", "string", None)
        .expect("film.lang")
        .vertex("store", "table", None)
        .expect("store")
        .vertex("store.film_id", "integer", None)
        .expect("store.film_id")
        .vertex("store.film_lang", "string", None)
        .expect("store.film_lang")
        .edge("film", "film.id", "column", Some("id"))
        .expect("film -> id")
        .edge("film", "film.lang", "column", Some("lang"))
        .expect("film -> lang")
        .edge("store", "store.film_id", "column", Some("film_id"))
        .expect("store -> film_id")
        .edge("store", "store.film_lang", "column", Some("film_lang"))
        .expect("store -> film_lang")
        .edge("store", "film", "fk", Some("film"))
        .expect("store -> film")
        .edge("doc", "film", "item", None)
        .expect("doc -> film")
        .edge("doc", "store", "prop", Some("store"))
        .expect("doc -> store")
        .build()
        .expect("build")
}

fn film_key() -> Key {
    Key {
        id: Name::from("pk"),
        parts: vec![
            KeyPart::Path(vec![edge("film", "film.id", "column", "id")]),
            KeyPart::Path(vec![edge("film", "film.lang", "column", "lang")]),
        ],
        role: KeyRole::Primary,
        scope: KeyScope::Document,
        equality: vec![],
    }
}

fn store_reference() -> Reference {
    Reference {
        id: Name::from("store_film"),
        holder: Name::from("store"),
        source: RefSource::KeyValue {
            parts: vec![
                vec![edge("store", "store.film_id", "column", "film_id")],
                vec![edge("store", "store.film_lang", "column", "film_lang")],
            ],
        },
        target_vertex: Name::from("film"),
        target_key: Some(Name::from("pk")),
        edge: Some(edge("store", "film", "fk", "film")),
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: NullMatch::default(),
        null_vertex: None,
        scope_via: None,
    }
}

fn keyed_schema() -> Schema {
    let mut schema = base_schema();
    schema
        .declarations
        .keys
        .insert(Name::from("film"), vec![film_key()]);
    schema
        .declarations
        .references
        .insert(Name::from("store_film"), store_reference());
    schema
}

fn write_edges(out: &mut CanonicalWriter, edges: &[Edge]) {
    out.len(edges.len());
    for edge in edges {
        out.edge(edge);
    }
}

/// The encoding of `keyed_schema`'s declarations, written out field by
/// field: the keys map, then the references map.
fn expected_declaration_bytes() -> Vec<u8> {
    let mut out = CanonicalWriter::new();
    // keys: one vertex
    out.len(1);
    out.name(&Name::from("film"));
    out.len(1);
    out.name(&Name::from("pk"));
    out.len(2);
    out.tag(0);
    write_edges(&mut out, &[edge("film", "film.id", "column", "id")]);
    out.tag(0);
    write_edges(&mut out, &[edge("film", "film.lang", "column", "lang")]);
    out.tag(0); // role: primary
    out.tag(0); // scope: document
    // references: one reference
    out.len(1);
    out.name(&Name::from("store_film"));
    out.name(&Name::from("store_film"));
    out.name(&Name::from("store"));
    out.tag(0); // source: key_value
    out.len(2);
    write_edges(
        &mut out,
        &[edge("store", "store.film_id", "column", "film_id")],
    );
    write_edges(
        &mut out,
        &[edge("store", "store.film_lang", "column", "film_lang")],
    );
    out.name(&Name::from("film"));
    out.opt_name(Some(&Name::from("pk")));
    out.tag(1);
    out.edge(&edge("store", "film", "fk", "film"));
    out.tag(0); // cardinality: one
    out.bool(false);
    out.tag(0); // on_unresolved: error
    out.tag(0); // mode: add_edge
    out.into_bytes()
}

#[test]
fn schema_bytes_end_with_keys_and_references() {
    let bare = canonical_bytes(&base_schema());
    let keyed = canonical_bytes(&keyed_schema());

    // A schema that declares nothing ends in two empty maps.
    let mut empty_tail = CanonicalWriter::new();
    empty_tail.len(0);
    empty_tail.len(0);
    let empty_tail = empty_tail.into_bytes();
    assert!(bare.ends_with(&empty_tail), "two empty trailing maps");

    // Everything before the declarations is shared, and the
    // declarations are the last two fields.
    let prefix = &bare[..bare.len() - empty_tail.len()];
    assert!(keyed.starts_with(prefix), "declarations come last");
    assert_eq!(
        &keyed[prefix.len()..],
        expected_declaration_bytes().as_slice()
    );
}

#[test]
fn induce_keeps_the_declarations_whose_parts_survive() {
    let schema = keyed_schema();
    let all: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
    let apex = induce_on_vertices(&schema, &protocol(), &all).expect("induce");
    assert_eq!(apex.declarations, schema.declarations);
}

#[test]
fn a_pushout_carries_the_declarations_of_both_sides() {
    let left = keyed_schema();
    let right = keyed_schema();
    let overlap = SchemaOverlap {
        vertex_pairs: left
            .vertices
            .keys()
            .map(|id| (id.clone(), id.clone()))
            .collect(),
        edge_pairs: vec![],
    };
    let (pushout, _, _) = schema_pushout(&left, &right, &overlap).expect("pushout");
    assert_eq!(pushout.declarations, left.declarations);
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("json")
}

/// Every value round-trips through JSON and positional and named
/// msgpack.
fn round_trips<T>(value: &T)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let back: T = serde_json::from_str(&json(value)).expect("from json");
    assert_eq!(&back, value, "json");
    let back: T =
        rmp_serde::from_slice(&rmp_serde::to_vec(value).expect("msgpack")).expect("from msgpack");
    assert_eq!(&back, value, "msgpack");
    let back: T = rmp_serde::from_slice(&rmp_serde::to_vec_named(value).expect("named"))
        .expect("from named msgpack");
    assert_eq!(&back, value, "named msgpack");
}

const E: &str = r#"{"src":"a","tgt":"b","kind":"prop","name":"b"}"#;

#[test]
fn key_and_reference_types_serialize_in_contract_form() {
    let e = edge("a", "b", "prop", "b");

    // Structs are named, fields in declaration order.
    let key = Key {
        id: Name::from("k"),
        parts: vec![
            KeyPart::Path(vec![e.clone()]),
            KeyPart::Position { within: e.clone() },
        ],
        role: KeyRole::Primary,
        scope: KeyScope::Within(Name::from("a")),
        equality: vec![],
    };
    assert_eq!(
        json(&key),
        format!(
            r#"{{"id":"k","parts":[{{"path":[{E}]}},{{"position":{{"within":{E}}}}}],"role":"primary","scope":{{"within":"a"}}}}"#
        )
    );
    round_trips(&key);

    // Enums are externally tagged in snake_case.
    assert_eq!(json(&KeyRole::Unique), r#""unique""#);
    assert_eq!(json(&KeyRole::Identifier), r#""identifier""#);
    assert_eq!(json(&KeyScope::Document), r#""document""#);
    assert_eq!(json(&KeyScope::Global), r#""global""#);
    assert_eq!(json(&PathSyntax::JsonPointer), r#""json_pointer""#);
    assert_eq!(json(&PathSyntax::XPath), r#""x_path""#);
    assert_eq!(json(&PathSyntax::IdRef), r#""id_ref""#);
    assert_eq!(
        json(&PathSyntax::Uri {
            base: UriBase::Absolute,
        }),
        r#"{"uri":{"base":null}}"#
    );
    assert_eq!(json(&OffsetUnit::Byte), r#""byte""#);
    assert_eq!(json(&OffsetUnit::Utf8Char), r#""utf8_char""#);
    assert_eq!(json(&OffsetUnit::Utf16), r#""utf16""#);
    assert_eq!(json(&OffsetUnit::Token), r#""token""#);
    assert_eq!(json(&Cardinality::One), r#""one""#);
    assert_eq!(json(&Cardinality::Many), r#""many""#);
    assert_eq!(json(&OnUnresolved::Error), r#""error""#);
    assert_eq!(json(&OnUnresolved::Complement), r#""complement""#);
    assert_eq!(json(&ResolveMode::AddEdge), r#""add_edge""#);
    assert_eq!(json(&ResolveMode::Redirect), r#""redirect""#);
    assert_eq!(
        json(&RefTextSource::Local(vec![e])),
        format!(r#"{{"local":[{E}]}}"#)
    );
    assert_eq!(json(&RefTextSource::External(vec![])), r#"{"external":[]}"#);
}

#[test]
fn reference_sources_serialize_in_contract_form() {
    let e = edge("a", "b", "prop", "b");
    let sources = [
        (
            RefSource::KeyValue {
                parts: vec![vec![e.clone()]],
            },
            format!(r#"{{"key_value":{{"parts":[[{E}]]}}}}"#),
        ),
        (
            RefSource::Path {
                field: vec![e.clone()],
                syntax: PathSyntax::Uri {
                    base: UriBase::Field(vec![e.clone()]),
                },
            },
            format!(r#"{{"path":{{"field":[{E}],"syntax":{{"uri":{{"base":[{E}]}}}}}}}}"#),
        ),
        (
            RefSource::Standoff {
                text: RefTextSource::External(vec![e.clone()]),
                start: vec![e],
                end: vec![],
                unit: OffsetUnit::Utf16,
                end_inclusive: true,
                end_form: SpanEnd::default(),
                origin: 0,
                fragments: None,
            },
            format!(
                r#"{{"standoff":{{"text":{{"external":[{E}]}},"start":[{E}],"end":[],"unit":"utf16","end_inclusive":true}}}}"#
            ),
        ),
        (
            RefSource::Variable {
                field: vec![],
                binder_key: Name::from("var"),
                binding: VariableBinding::default(),
            },
            r#"{"variable":{"field":[],"binder_key":"var"}}"#.to_owned(),
        ),
    ];
    for (source, expected) in &sources {
        assert_eq!(&json(source), expected);
        round_trips(source);
    }
}

#[test]
fn references_and_reports_serialize_in_contract_form() {
    let e = edge("a", "b", "prop", "b");
    let reference = Reference {
        id: Name::from("r"),
        holder: Name::from("a"),
        source: RefSource::Path {
            field: vec![],
            syntax: PathSyntax::IdRef,
        },
        target_vertex: Name::from("b"),
        target_key: None,
        edge: Some(e),
        cardinality: Cardinality::Many,
        nullable: true,
        on_unresolved: OnUnresolved::Complement,
        mode: ResolveMode::Redirect,
        guard: None,
        null_match: NullMatch::default(),
        null_vertex: None,
        scope_via: None,
    };
    assert_eq!(
        json(&reference),
        format!(
            r#"{{"id":"r","holder":"a","source":{{"path":{{"field":[],"syntax":"id_ref"}}}},"target_vertex":"b","target_key":null,"edge":{E},"cardinality":"many","nullable":true,"on_unresolved":"complement","mode":"redirect"}}"#
        )
    );
    round_trips(&reference);

    let decls = keyed_schema().declarations;
    round_trips(&decls);
    assert!(json(&decls).starts_with(r#"{"keys":{"film":[{"id":"pk""#));

    let report = DeclarationReport {
        dropped_keys: vec![(Name::from("v"), Name::from("k"))],
        dropped_references: vec![Name::from("r")],
        dropped_target_set_members: Vec::new(),
        dropped_target_sets: Vec::new(),
    };
    assert_eq!(
        json(&report),
        r#"{"dropped_keys":[["v","k"]],"dropped_references":["r"]}"#
    );
    round_trips(&report);

    let error = KeyRefError::ArityMismatch {
        reference: Name::from("r"),
        expected: 2,
        found: 1,
    };
    assert_eq!(
        json(&error),
        r#"{"arity_mismatch":{"reference":"r","expected":2,"found":1}}"#
    );
    round_trips(&error);
}

#[test]
fn schema_declarations_keep_keys_sorted_and_unique() {
    let key = |id: &str, role: KeyRole| Key {
        id: Name::from(id),
        parts: vec![],
        role,
        scope: KeyScope::Document,
        equality: vec![],
    };
    let mut decls = SchemaDeclarations::default();
    assert!(decls.is_empty());

    for id in ["m", "c", "x", "a"] {
        assert!(
            decls
                .insert_key(Name::from("v"), key(id, KeyRole::Unique))
                .is_none()
        );
    }
    assert!(!decls.is_empty());
    let ids: Vec<&str> = decls.keys_of("v").iter().map(|k| k.id.as_ref()).collect();
    assert_eq!(ids, ["a", "c", "m", "x"], "sorted by id");

    // Re-inserting an id replaces, never duplicates.
    let replaced = decls.insert_key(Name::from("v"), key("c", KeyRole::Primary));
    assert_eq!(replaced, Some(key("c", KeyRole::Unique)));
    assert_eq!(decls.keys_of("v").len(), 4);
    assert_eq!(decls.key("v", "c"), Some(&key("c", KeyRole::Primary)));

    // Merging an equal key is a no-op; a different one conflicts.
    decls
        .merge_key(Name::from("v"), key("c", KeyRole::Primary))
        .expect("equal key merges");
    let conflict = decls
        .merge_key(Name::from("v"), key("c", KeyRole::Unique))
        .expect_err("different key conflicts");
    assert!(matches!(
        conflict,
        SchemaError::KeyConflict { ref vertex, ref key } if vertex == "v" && key == "c"
    ));
    assert_eq!(decls.keys_of("v").len(), 4);

    // Emptiness is about both maps.
    let mut only_references = SchemaDeclarations::default();
    only_references.insert_reference(store_reference());
    assert!(!only_references.is_empty());
    assert!(SchemaDeclarations::default().is_empty());
}

/// The encodings v0.74.4 gave `released_fixture`, produced by building
/// that release's panproto-schema and serializing the same schema with
/// `serde_json::to_vec`, `rmp_serde::to_vec` and `rmp_serde::to_vec_named`.
const RELEASED_JSON: &str = r#"{"protocol":"fixture","vertices":{"leaf":{"id":"leaf","kind":"string","nsid":null},"root":{"id":"root","kind":"object","nsid":null}},"edges":[[{"src":"root","tgt":"leaf","kind":"prop","name":"leaf"},"prop"]],"hyper_edges":{},"constraints":{"leaf":[{"sort":"maxLength","value":"10"}]},"required":{},"nsids":{},"entries":["root"],"variants":{},"orderings":[],"recursion_points":{},"spans":{},"usage_modes":[],"nominal":{},"coercions":[],"mergers":{},"defaults":{},"policies":{},"outgoing":{"root":[{"src":"root","tgt":"leaf","kind":"prop","name":"leaf"}]},"incoming":{"leaf":[{"src":"root","tgt":"leaf","kind":"prop","name":"leaf"}]},"between":[[["root","leaf"],[{"src":"root","tgt":"leaf","kind":"prop","name":"leaf"}]]]}"#;
const RELEASED_MSGPACK: &[&str] = &[
    "dc0015a76669787475726582a46c65616693a46c656166a6737472696e67c0a4726f6f7493a4726f6f74a66f626a6563",
    "74c0919294a4726f6f74a46c656166a470726f70a46c656166a470726f708081a46c6561669192a96d61784c656e6774",
    "68a23130808091a4726f6f748090808090809080808081a4726f6f749194a4726f6f74a46c656166a470726f70a46c65",
    "616681a46c6561669194a4726f6f74a46c656166a470726f70a46c656166919292a4726f6f74a46c6561669194a4726f",
    "6f74a46c656166a470726f70a46c656166",
];
const RELEASED_MSGPACK_NAMED: &[&str] = &[
    "de0015a870726f746f636f6ca766697874757265a8766572746963657382a46c65616683a26964a46c656166a46b696e",
    "64a6737472696e67a46e736964c0a4726f6f7483a26964a4726f6f74a46b696e64a66f626a656374a46e736964c0a565",
    "64676573919284a3737263a4726f6f74a3746774a46c656166a46b696e64a470726f70a46e616d65a46c656166a47072",
    "6f70ab68797065725f656467657380ab636f6e73747261696e747381a46c6561669182a4736f7274a96d61784c656e67",
    "7468a576616c7565a23130a8726571756972656480a56e7369647380a7656e747269657391a4726f6f74a87661726961",
    "6e747380a96f72646572696e677390b0726563757273696f6e5f706f696e747380a57370616e7380ab75736167655f6d",
    "6f64657390a76e6f6d696e616c80a9636f657263696f6e7390a76d65726765727380a864656661756c747380a8706f6c",
    "696369657380a86f7574676f696e6781a4726f6f749184a3737263a4726f6f74a3746774a46c656166a46b696e64a470",
    "726f70a46e616d65a46c656166a8696e636f6d696e6781a46c6561669184a3737263a4726f6f74a3746774a46c656166",
    "a46b696e64a470726f70a46e616d65a46c656166a76265747765656e919292a4726f6f74a46c6561669184a3737263a4",
    "726f6f74a3746774a46c656166a46b696e64a470726f70a46e616d65a46c656166",
];

fn released_fixture() -> Schema {
    let protocol = Protocol {
        name: "fixture".to_owned(),
        ..Default::default()
    };
    SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .vertex("leaf", "string", None)
        .expect("leaf")
        .edge("root", "leaf", "prop", Some("leaf"))
        .expect("edge")
        .constraint("leaf", "maxLength", "10")
        .entry("root")
        .build()
        .expect("build")
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        write!(out, "{b:02x}").expect("write to a string");
        out
    })
}

#[test]
fn a_schema_without_declarations_keeps_its_bytes() {
    let schema = released_fixture();
    assert_eq!(
        String::from_utf8(serde_json::to_vec(&schema).expect("json")).expect("utf8"),
        RELEASED_JSON
    );
    assert_eq!(
        hex(&rmp_serde::to_vec(&schema).expect("msgpack")),
        RELEASED_MSGPACK.concat()
    );
    assert_eq!(
        hex(&rmp_serde::to_vec_named(&schema).expect("named")),
        RELEASED_MSGPACK_NAMED.concat()
    );

    // A release reader's bytes decode to a schema that declares nothing.
    let back: Schema = serde_json::from_str(RELEASED_JSON).expect("decode");
    assert!(back.declarations.is_empty());

    // A schema that declares something gains exactly one trailing
    // element in both the named and the positional forms, and decodes
    // back to the same declarations.
    let mut keyed = released_fixture();
    keyed.declarations = keyed_schema().declarations;
    let positional = rmp_serde::to_vec(&keyed).expect("msgpack");
    assert_eq!(&positional[..3], &[0xdc, 0x00, 0x16], "22 elements, not 21");
    let back: Schema = rmp_serde::from_slice(&positional).expect("decode");
    assert_eq!(back.declarations, keyed.declarations);
    let named = serde_json::to_string(&keyed).expect("json");
    assert!(named.starts_with(&RELEASED_JSON[..RELEASED_JSON.len() - 1]));
    assert!(named.contains(r#","declarations":{"keys":"#));
    let back: Schema = serde_json::from_str(&named).expect("decode");
    assert_eq!(back.declarations, keyed.declarations);
}

#[test]
fn the_builder_records_keys_and_references() {
    let unique = Key {
        id: Name::from("by_lang"),
        parts: vec![KeyPart::Path(vec![edge(
            "film",
            "film.lang",
            "column",
            "lang",
        )])],
        role: KeyRole::Unique,
        scope: KeyScope::Within(Name::from("doc")),
        equality: vec![],
    };
    let build = |extra: Option<Key>| {
        let builder = SchemaBuilder::new(&protocol())
            .vertex("doc", "object", None)
            .expect("doc")
            .vertex("film", "table", None)
            .expect("film")
            .vertex("film.id", "integer", None)
            .expect("film.id")
            .vertex("film.lang", "string", None)
            .expect("film.lang")
            .vertex("store", "table", None)
            .expect("store")
            .vertex("store.film_id", "integer", None)
            .expect("store.film_id")
            .vertex("store.film_lang", "string", None)
            .expect("store.film_lang")
            .edge("film", "film.id", "column", Some("id"))
            .expect("e")
            .edge("film", "film.lang", "column", Some("lang"))
            .expect("e")
            .edge("store", "store.film_id", "column", Some("film_id"))
            .expect("e")
            .edge("store", "store.film_lang", "column", Some("film_lang"))
            .expect("e")
            .edge("store", "film", "fk", Some("film"))
            .expect("e")
            .edge("doc", "film", "item", None)
            .expect("e")
            .edge("doc", "store", "prop", Some("store"))
            .expect("e")
            // Declared out of id order, and one of them twice.
            .key("film", unique.clone())
            .key("film", film_key())
            .key("film", film_key())
            .reference(store_reference())
            .reference(store_reference());
        match extra {
            Some(key) => builder.key("film", key),
            None => builder,
        }
        .build()
    };

    let schema = build(None).expect("build");
    let ids: Vec<&str> = schema
        .declarations
        .keys_of("film")
        .iter()
        .map(|key| key.id.as_ref())
        .collect();
    assert_eq!(ids, ["by_lang", "pk"]);
    assert_eq!(
        schema.declarations.references.get("store_film"),
        Some(&store_reference())
    );
    assert_eq!(panproto_schema::validate(&schema, &protocol()), []);

    let mut clashing = film_key();
    clashing.role = KeyRole::Unique;
    let err = build(Some(clashing)).expect_err("a second pk that differs");
    assert!(matches!(
        err,
        SchemaError::KeyConflict { ref vertex, ref key } if vertex == "film" && key == "pk"
    ));

    let mut other = store_reference();
    other.nullable = true;
    let err = SchemaBuilder::new(&protocol())
        .vertex("a", "object", None)
        .expect("a")
        .reference(store_reference())
        .reference(other)
        .build()
        .expect_err("a second store_film that differs");
    assert!(matches!(
        err,
        SchemaError::ReferenceConflict { ref reference } if reference == "store_film"
    ));
}

fn findings(schema: &Schema, protocol: &Protocol) -> Vec<KeyRefError> {
    panproto_schema::validate(schema, protocol)
        .into_iter()
        .filter_map(|finding| match finding {
            panproto_schema::ValidationError::KeyRef(error) => Some(error),
            _ => None,
        })
        .collect()
}

/// `keyed_schema` with its declarations edited by `edit`, validated
/// against the plain test protocol.
fn findings_after(edit: impl FnOnce(&mut Schema)) -> Vec<KeyRefError> {
    let mut schema = keyed_schema();
    edit(&mut schema);
    findings(&schema, &protocol())
}

fn film_keys(schema: &mut Schema) -> &mut Vec<Key> {
    schema.declarations.keys.get_mut("film").expect("film keys")
}

fn store_ref(schema: &mut Schema) -> &mut Reference {
    schema
        .declarations
        .references
        .get_mut("store_film")
        .expect("store_film")
}

#[test]
fn a_well_formed_schema_reports_nothing() {
    assert_eq!(findings_after(|_| {}), vec![]);
}

#[test]
fn validate_reports_unknown_vertex() {
    let found = findings_after(|schema| {
        schema
            .declarations
            .keys
            .insert(Name::from("ghost"), vec![film_key()]);
        store_ref(schema).target_vertex = Name::from("phantom");
    });
    assert_eq!(
        found,
        vec![
            KeyRefError::UnknownVertex {
                vertex: Name::from("ghost")
            },
            KeyRefError::UnknownVertex {
                vertex: Name::from("phantom")
            },
        ]
    );
}

#[test]
fn validate_reports_duplicate_key_id() {
    let found = findings_after(|schema| film_keys(schema).push(film_key()));
    assert_eq!(
        found,
        vec![KeyRefError::DuplicateKeyId {
            vertex: Name::from("film"),
            key: Name::from("pk"),
        }]
    );
}

#[test]
fn validate_reports_path_does_not_resolve() {
    // A key part that starts at another vertex.
    let stray = vec![edge("store", "store.film_id", "column", "film_id")];
    let found = findings_after(|schema| {
        film_keys(schema)[0].parts[0] = KeyPart::Path(stray.clone());
    });
    assert_eq!(
        found,
        vec![KeyRefError::PathDoesNotResolve {
            owner: Name::from("film"),
            path: stray,
        }]
    );

    // A reference path through an edge the schema does not have.
    let missing = vec![edge("store", "store.film_id", "column", "nope")];
    let found = findings_after(|schema| {
        store_ref(schema).source = RefSource::KeyValue {
            parts: vec![
                missing.clone(),
                vec![edge("store", "store.film_lang", "column", "film_lang")],
            ],
        };
    });
    assert_eq!(
        found,
        vec![KeyRefError::PathDoesNotResolve {
            owner: Name::from("store"),
            path: missing,
        }]
    );
}

#[test]
fn validate_reports_position_not_ordered() {
    let item = edge("doc", "film", "item", "unused");
    let item = Edge { name: None, ..item };
    let position = Key {
        id: Name::from("index"),
        parts: vec![KeyPart::Position {
            within: item.clone(),
        }],
        role: KeyRole::Unique,
        scope: KeyScope::Within(Name::from("doc")),
        equality: vec![],
    };
    let found = findings_after(|schema| film_keys(schema).push(position.clone()));
    assert_eq!(
        found,
        vec![KeyRefError::PositionNotOrdered {
            key: Name::from("index"),
            edge: item.clone(),
        }]
    );

    // Ordering the edge makes the same key well formed.
    let found = findings_after(|schema| {
        film_keys(schema).push(position);
        schema.orderings.insert(item, 0);
    });
    assert_eq!(found, vec![]);
}

#[test]
fn validate_reports_unknown_target_key() {
    let found = findings_after(|schema| {
        store_ref(schema).target_key = Some(Name::from("nope"));
    });
    assert_eq!(
        found,
        vec![KeyRefError::UnknownTargetKey {
            reference: Name::from("store_film"),
            key: Name::from("nope"),
        }]
    );
}

#[test]
fn validate_reports_arity_mismatch() {
    let found = findings_after(|schema| {
        store_ref(schema).source = RefSource::KeyValue {
            parts: vec![vec![edge("store", "store.film_id", "column", "film_id")]],
        };
    });
    assert_eq!(
        found,
        vec![KeyRefError::ArityMismatch {
            reference: Name::from("store_film"),
            expected: 2,
            found: 1,
        }]
    );

    // Without a named key the reference resolves through the target's
    // primary key; with no key at all there is nothing to align with.
    let found = findings_after(|schema| store_ref(schema).target_key = None);
    assert_eq!(found, vec![]);
    let found = findings_after(|schema| {
        store_ref(schema).target_key = None;
        schema.declarations.keys.clear();
    });
    assert_eq!(
        found,
        vec![KeyRefError::ArityMismatch {
            reference: Name::from("store_film"),
            expected: 0,
            found: 2,
        }]
    );
}

#[test]
fn validate_reports_binder_not_identifier() {
    let variable = |binder: &str| RefSource::Variable {
        field: vec![edge("store", "store.film_id", "column", "film_id")],
        binder_key: Name::from(binder),
        binding: VariableBinding::default(),
    };
    let found = findings_after(|schema| {
        let reference = store_ref(schema);
        reference.source = variable("pk");
        reference.target_key = None;
    });
    assert_eq!(
        found,
        vec![KeyRefError::BinderNotIdentifier {
            reference: Name::from("store_film"),
            key: Name::from("pk"),
        }]
    );

    let found = findings_after(|schema| {
        film_keys(schema)[0].role = KeyRole::Identifier;
        let reference = store_ref(schema);
        reference.source = variable("pk");
        reference.target_key = None;
    });
    assert_eq!(found, vec![]);

    let found = findings_after(|schema| {
        let reference = store_ref(schema);
        reference.source = variable("unbound");
        reference.target_key = None;
    });
    assert_eq!(
        found,
        vec![KeyRefError::UnknownTargetKey {
            reference: Name::from("store_film"),
            key: Name::from("unbound"),
        }]
    );
}

#[test]
fn validate_reports_edge_mismatch() {
    let wrong = Edge {
        name: None,
        ..edge("doc", "film", "item", "unused")
    };
    let found = findings_after(|schema| store_ref(schema).edge = Some(wrong.clone()));
    assert_eq!(
        found,
        vec![KeyRefError::EdgeMismatch {
            reference: Name::from("store_film"),
            edge: wrong,
        }]
    );

    // An edge with the right ends that the schema does not have.
    let absent = edge("store", "film", "fk", "other");
    let found = findings_after(|schema| store_ref(schema).edge = Some(absent.clone()));
    assert_eq!(
        found,
        vec![KeyRefError::PathDoesNotResolve {
            owner: Name::from("store"),
            path: vec![absent],
        }]
    );
}

#[test]
fn validate_reports_edge_required() {
    let resolving = Protocol {
        name: "keys".to_owned(),
        syntactic_shape: panproto_schema::InstanceShapeKind::Table,
        semantic_shape: panproto_schema::InstanceShapeKind::Graph,
        ..Default::default()
    };
    let mut schema = keyed_schema();
    assert_eq!(findings(&schema, &resolving), vec![]);
    store_ref(&mut schema).edge = None;
    assert_eq!(
        findings(&schema, &resolving),
        vec![KeyRefError::EdgeRequired {
            reference: Name::from("store_film"),
        }]
    );
    // A protocol whose shapes agree needs no edge.
    assert_eq!(findings(&schema, &protocol()), vec![]);
}

#[test]
fn validate_reports_within_not_ancestor() {
    let found = findings_after(|schema| {
        film_keys(schema)[0].scope = KeyScope::Within(Name::from("film.id"));
    });
    assert_eq!(
        found,
        vec![KeyRefError::WithinNotAncestor {
            key: Name::from("pk"),
            vertex: Name::from("film.id"),
        }]
    );
    // A vertex is not its own proper ancestor.
    let found = findings_after(|schema| {
        film_keys(schema)[0].scope = KeyScope::Within(Name::from("film"));
    });
    assert_eq!(
        found,
        vec![KeyRefError::WithinNotAncestor {
            key: Name::from("pk"),
            vertex: Name::from("film"),
        }]
    );
    let found = findings_after(|schema| {
        film_keys(schema)[0].scope = KeyScope::Within(Name::from("doc"));
    });
    assert_eq!(found, vec![]);
    let found = findings_after(|schema| {
        film_keys(schema)[0].scope = KeyScope::Within(Name::from("nowhere"));
    });
    assert_eq!(
        found,
        vec![KeyRefError::UnknownVertex {
            vertex: Name::from("nowhere"),
        }]
    );
}

#[test]
fn a_reference_to_a_subtype_vertex_does_not_resolve() {
    // `node` is an interface with an identifier; `user` is one of its
    // variants and declares no key of its own.
    let mut schema = SchemaBuilder::new(&protocol())
        .vertex("node", "union", None)
        .expect("node")
        .vertex("node.id", "string", None)
        .expect("node.id")
        .vertex("user", "object", None)
        .expect("user")
        .vertex("post", "object", None)
        .expect("post")
        .vertex("post.author", "string", None)
        .expect("post.author")
        .edge("node", "node.id", "prop", Some("id"))
        .expect("e")
        .edge("post", "post.author", "prop", Some("author"))
        .expect("e")
        .key(
            "node",
            Key {
                id: Name::from("id"),
                parts: vec![KeyPart::Path(vec![edge("node", "node.id", "prop", "id")])],
                role: KeyRole::Identifier,
                scope: KeyScope::Global,
                equality: vec![],
            },
        )
        .build()
        .expect("build");
    schema.variants.insert(
        Name::from("node"),
        vec![panproto_schema::Variant {
            id: Name::from("user"),
            parent_vertex: Name::from("node"),
            tag: Some(Name::from("User")),
        }],
    );
    let author = |target: &str| Reference {
        id: Name::from("author"),
        holder: Name::from("post"),
        source: RefSource::KeyValue {
            parts: vec![vec![edge("post", "post.author", "prop", "author")]],
        },
        target_vertex: Name::from(target),
        target_key: Some(Name::from("id")),
        edge: None,
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: NullMatch::default(),
        null_vertex: None,
        scope_via: None,
    };

    schema.declarations.insert_reference(author("node"));
    assert_eq!(findings(&schema, &protocol()), vec![]);

    schema.declarations.insert_reference(author("user"));
    assert_eq!(
        findings(&schema, &protocol()),
        vec![KeyRefError::UnknownTargetKey {
            reference: Name::from("author"),
            key: Name::from("id"),
        }]
    );
}

fn identity_morphism(schema: &Schema) -> panproto_schema::SchemaMorphism {
    panproto_schema::SchemaMorphism {
        name: "id".into(),
        src_protocol: schema.protocol.clone(),
        tgt_protocol: schema.protocol.clone(),
        vertex_map: schema
            .vertices
            .keys()
            .map(|v| (v.clone(), v.clone()))
            .collect(),
        edge_map: schema
            .edges
            .keys()
            .map(|e| (e.clone(), e.clone()))
            .collect(),
        renames: vec![],
    }
}

#[test]
fn unmapped_declarations_are_dropped_and_reported() {
    let schema = keyed_schema();

    // A morphism that renames `film` carries every path through its
    // edge map.
    let mut rename = identity_morphism(&schema);
    let renamed = |e: &Edge| Edge {
        src: if e.src == "film" {
            Name::from("movie")
        } else {
            e.src.clone()
        },
        tgt: if e.tgt == "film" {
            Name::from("movie")
        } else {
            e.tgt.clone()
        },
        ..e.clone()
    };
    rename
        .vertex_map
        .insert(Name::from("film"), Name::from("movie"));
    for (source, image) in &mut rename.edge_map {
        *image = renamed(source);
    }
    let (mapped, report) = rename.map_declarations(&schema.declarations).expect("map");
    assert!(report.is_empty());
    let key = mapped
        .key("movie", "pk")
        .expect("key moved with its vertex");
    assert_eq!(
        key.parts[0],
        KeyPart::Path(vec![edge("movie", "film.id", "column", "id")])
    );
    let reference = mapped.references.get("store_film").expect("reference");
    assert_eq!(reference.target_vertex, "movie");
    assert_eq!(reference.edge, Some(edge("store", "movie", "fk", "film")));

    // Leaving one key column out of the edge map drops the key, and the
    // reference that resolves through it goes with it.
    let mut partial = identity_morphism(&schema);
    partial
        .edge_map
        .remove(&edge("film", "film.lang", "column", "lang"));
    let (mapped, report) = partial.map_declarations(&schema.declarations).expect("map");
    assert!(mapped.is_empty());
    assert_eq!(
        report,
        DeclarationReport {
            dropped_keys: vec![(Name::from("film"), Name::from("pk"))],
            dropped_references: vec![Name::from("store_film")],
            dropped_target_set_members: Vec::new(),
            dropped_target_sets: Vec::new(),
        }
    );

    // Leaving only the reference's edge out drops only the reference.
    let mut no_fk = identity_morphism(&schema);
    no_fk.edge_map.remove(&edge("store", "film", "fk", "film"));
    let (mapped, report) = no_fk.map_declarations(&schema.declarations).expect("map");
    assert_eq!(mapped.keys, schema.declarations.keys);
    assert!(mapped.references.is_empty());
    assert_eq!(report.dropped_references, vec![Name::from("store_film")]);
    assert_eq!(report.dropped_keys, []);

    // A morphism that identifies two keyed vertices merges equal keys
    // and refuses different ones.
    let mut twin = keyed_schema();
    twin.declarations.keys.insert(
        Name::from("store"),
        vec![Key {
            id: Name::from("pk"),
            parts: vec![],
            role: KeyRole::Unique,
            scope: KeyScope::Document,
            equality: vec![],
        }],
    );
    let mut collapse = identity_morphism(&twin);
    collapse
        .vertex_map
        .insert(Name::from("store"), Name::from("film"));
    let err = collapse
        .map_declarations(&twin.declarations)
        .expect_err("two different pk keys land on film");
    assert!(matches!(
        err,
        SchemaError::KeyConflict { ref vertex, ref key } if vertex == "film" && key == "pk"
    ));
}

#[test]
fn induce_and_pushout_report_dropped_declarations() {
    let schema = keyed_schema();

    // Cutting `film.lang` drops the composite key and the reference that
    // resolves through it.
    let keep: FxHashSet<Name> = schema
        .vertices
        .keys()
        .filter(|v| *v != "film.lang")
        .cloned()
        .collect();
    let keep_e: FxHashSet<Edge> = schema.edges.keys().cloned().collect();
    let (apex, report) =
        panproto_schema::induce_with_report(&schema, &protocol(), &keep, &keep_e).expect("induce");
    assert!(apex.declarations.is_empty());
    assert_eq!(
        report,
        DeclarationReport {
            dropped_keys: vec![(Name::from("film"), Name::from("pk"))],
            dropped_references: vec![Name::from("store_film")],
            dropped_target_set_members: Vec::new(),
            dropped_target_sets: Vec::new(),
        }
    );
    // `induce` itself keeps its signature and agrees.
    let plain = panproto_schema::induce(&schema, &protocol(), &keep, &keep_e).expect("induce");
    assert_eq!(plain.declarations, apex.declarations);

    // Cutting only the foreign-key edge keeps the key.
    let keep_all: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
    let no_fk: FxHashSet<Edge> = schema
        .edges
        .keys()
        .filter(|e| e.kind != "fk")
        .cloned()
        .collect();
    let (apex, report) =
        panproto_schema::induce_with_report(&schema, &protocol(), &keep_all, &no_fk)
            .expect("induce");
    assert_eq!(apex.declarations.keys, schema.declarations.keys);
    assert_eq!(report.dropped_references, vec![Name::from("store_film")]);
    assert_eq!(report.dropped_keys, []);

    // A pushout whose sides are disjoint renames the right side's
    // colliding vertices, keeps both sides' keys and reports nothing.
    let left = keyed_schema();
    let mut right = base_schema();
    right
        .declarations
        .insert_key(Name::from("film"), film_key());
    let (pushout, left_morphism, right_morphism, report) =
        panproto_schema::schema_pushout_with_report(&left, &right, &SchemaOverlap::default())
            .expect("pushout");
    assert!(report.is_empty());
    assert_eq!(left_morphism.vertex_map["film"], "film");
    assert_eq!(right_morphism.vertex_map["film"], "right.film");
    assert!(pushout.declarations.key("film", "pk").is_some());
    let moved = pushout
        .declarations
        .key("right.film", "pk")
        .expect("right key follows its vertex");
    assert_eq!(
        moved.parts[0],
        KeyPart::Path(vec![edge("right.film", "right.film.id", "column", "id")])
    );
    assert_eq!(pushout.declarations.references.len(), 1);
    let (plain, _, _) = schema_pushout(&left, &right, &SchemaOverlap::default()).expect("pushout");
    assert_eq!(plain.declarations, pushout.declarations);
}

#[test]
fn conflicting_keys_fail_the_pushout() {
    let identify = |left: &Schema| SchemaOverlap {
        vertex_pairs: left
            .vertices
            .keys()
            .map(|id| (id.clone(), id.clone()))
            .collect(),
        edge_pairs: vec![],
    };
    let left = keyed_schema();

    // Equal keys on identified vertices merge.
    let (pushout, ..) =
        panproto_schema::schema_pushout_with_report(&left, &keyed_schema(), &identify(&left))
            .expect("equal declarations merge");
    assert_eq!(pushout.declarations, left.declarations);

    // A key with the same id and a different role conflicts.
    let mut right = keyed_schema();
    film_keys(&mut right)[0].role = KeyRole::Unique;
    let err = schema_pushout(&left, &right, &identify(&left)).expect_err("key conflict");
    assert!(matches!(
        err,
        SchemaError::KeyConflict { ref vertex, ref key } if vertex == "film" && key == "pk"
    ));

    // So does a different scope.
    let mut right = keyed_schema();
    film_keys(&mut right)[0].scope = KeyScope::Global;
    assert!(matches!(
        schema_pushout(&left, &right, &identify(&left)),
        Err(SchemaError::KeyConflict { .. })
    ));

    // A reference with the same id that differs conflicts too.
    let mut right = keyed_schema();
    store_ref(&mut right).cardinality = Cardinality::Many;
    let err = schema_pushout(&left, &right, &identify(&left)).expect_err("reference conflict");
    assert!(matches!(
        err,
        SchemaError::ReferenceConflict { ref reference } if reference == "store_film"
    ));
}

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    edge(src, tgt, "prop", name)
}

/// A non-nullable, strict, one-to-one reference that adds no edge.
fn plain_reference(id: &str, holder: &str, source: RefSource, target: &str) -> Reference {
    Reference {
        id: Name::from(id),
        holder: Name::from(holder),
        source,
        target_vertex: Name::from(target),
        target_key: None,
        edge: None,
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: NullMatch::default(),
        null_vertex: None,
        scope_via: None,
    }
}

fn plain_key(id: &str, parts: Vec<KeyPart>, role: KeyRole, scope: KeyScope) -> Key {
    Key {
        id: Name::from(id),
        parts,
        role,
        scope,
        equality: vec![],
    }
}

/// A builder holding the given vertices and edges.
fn with_graph(ids: &[(&str, &str)], edges: &[Edge]) -> SchemaBuilder {
    let mut builder = SchemaBuilder::new(&protocol());
    for (id, kind) in ids {
        builder = builder.vertex(id, kind, None).expect("vertex");
    }
    for e in edges {
        builder = builder
            .edge(&e.src, &e.tgt, &e.kind, e.name.as_deref())
            .expect("edge");
    }
    builder
}

fn xml_family() -> Schema {
    with_graph(
        &[
            ("sec", "element"),
            ("sec.id", "attribute"),
            ("ref", "element"),
            ("ref.to", "attribute"),
        ],
        &[prop("sec", "sec.id", "id"), prop("ref", "ref.to", "to")],
    )
    .key(
        "sec",
        plain_key(
            "xml_id",
            vec![KeyPart::Path(vec![prop("sec", "sec.id", "id")])],
            KeyRole::Identifier,
            KeyScope::Document,
        ),
    )
    .reference(Reference {
        cardinality: Cardinality::Many,
        target_key: Some(Name::from("xml_id")),
        ..plain_reference(
            "idrefs",
            "ref",
            RefSource::Path {
                field: vec![prop("ref", "ref.to", "to")],
                syntax: PathSyntax::IdRef,
            },
            "sec",
        )
    })
    .build()
    .expect("xml")
}

fn standoff_family() -> Schema {
    with_graph(
        &[
            ("doc", "object"),
            ("doc.text", "string"),
            ("span", "object"),
            ("span.start", "integer"),
            ("span.end", "integer"),
            ("ann", "object"),
            ("ann.start", "integer"),
            ("ann.end", "integer"),
        ],
        &[
            prop("doc", "doc.text", "text"),
            prop("span", "span.start", "start"),
            prop("span", "span.end", "end"),
            prop("ann", "ann.start", "start"),
            prop("ann", "ann.end", "end"),
            prop("doc", "span", "span"),
            prop("doc", "ann", "ann"),
        ],
    )
    .key(
        "span",
        plain_key(
            "offsets",
            vec![
                KeyPart::Path(vec![prop("span", "span.start", "start")]),
                KeyPart::Path(vec![prop("span", "span.end", "end")]),
            ],
            KeyRole::Unique,
            KeyScope::Within(Name::from("doc")),
        ),
    )
    .reference(Reference {
        target_key: Some(Name::from("offsets")),
        on_unresolved: OnUnresolved::Complement,
        ..plain_reference(
            "brat_span",
            "ann",
            RefSource::Standoff {
                text: RefTextSource::Local(vec![]),
                start: vec![prop("ann", "ann.start", "start")],
                end: vec![prop("ann", "ann.end", "end")],
                unit: OffsetUnit::Utf8Char,
                end_inclusive: false,
                end_form: SpanEnd::default(),
                origin: 0,
                fragments: None,
            },
            "span",
        )
    })
    .build()
    .expect("standoff")
}

fn penman_family() -> Schema {
    with_graph(
        &[("concept", "node"), ("concept.var", "string")],
        &[
            prop("concept", "concept.var", "var"),
            edge("concept", "concept", "role", "ARG0"),
        ],
    )
    .key(
        "concept",
        plain_key(
            "var",
            vec![KeyPart::Path(vec![prop("concept", "concept.var", "var")])],
            KeyRole::Identifier,
            KeyScope::Document,
        ),
    )
    .reference(Reference {
        mode: ResolveMode::Redirect,
        edge: Some(edge("concept", "concept", "role", "ARG0")),
        ..plain_reference(
            "reentrancy",
            "concept",
            RefSource::Variable {
                field: vec![prop("concept", "concept.var", "var")],
                binder_key: Name::from("var"),
                binding: VariableBinding::default(),
            },
            "concept",
        )
    })
    .build()
    .expect("penman")
}

fn conllu_family() -> Schema {
    with_graph(
        &[
            ("sentence", "object"),
            ("token", "object"),
            ("token.id", "integer"),
            ("token.head", "integer"),
        ],
        &[
            prop("sentence", "token", "tokens"),
            prop("token", "token.id", "id"),
            prop("token", "token.head", "head"),
        ],
    )
    .key(
        "token",
        plain_key(
            "id",
            vec![KeyPart::Path(vec![prop("token", "token.id", "id")])],
            KeyRole::Primary,
            KeyScope::Within(Name::from("sentence")),
        ),
    )
    .reference(Reference {
        nullable: true,
        ..plain_reference(
            "head",
            "token",
            RefSource::KeyValue {
                parts: vec![vec![prop("token", "token.head", "head")]],
            },
            "token",
        )
    })
    .build()
    .expect("conllu")
}

fn positional_family() -> Schema {
    let mut schema = with_graph(
        &[("list", "array"), ("item", "object")],
        &[edge("list", "item", "item", "items")],
    )
    .key(
        "item",
        plain_key(
            "index",
            vec![KeyPart::Position {
                within: edge("list", "item", "item", "items"),
            }],
            KeyRole::Unique,
            KeyScope::Within(Name::from("list")),
        ),
    )
    .build()
    .expect("positional");
    schema
        .orderings
        .insert(edge("list", "item", "item", "items"), 0);
    schema
}

fn rdf_family() -> Schema {
    with_graph(
        &[
            ("subject", "resource"),
            ("subject.iri", "iri"),
            ("subject.knows", "iri"),
            ("subject.base", "iri"),
        ],
        &[
            prop("subject", "subject.iri", "iri"),
            prop("subject", "subject.knows", "knows"),
            prop("subject", "subject.base", "base"),
        ],
    )
    .key(
        "subject",
        plain_key(
            "iri",
            vec![KeyPart::Path(vec![prop("subject", "subject.iri", "iri")])],
            KeyRole::Identifier,
            KeyScope::Global,
        ),
    )
    .reference(Reference {
        target_key: Some(Name::from("iri")),
        cardinality: Cardinality::Many,
        ..plain_reference(
            "knows",
            "subject",
            RefSource::Path {
                field: vec![prop("subject", "subject.knows", "knows")],
                syntax: PathSyntax::Uri {
                    base: UriBase::Field(vec![prop("subject", "subject.base", "base")]),
                },
            },
            "subject",
        )
    })
    .build()
    .expect("rdf")
}

/// One schema per family of identifier, key and reference markup:
/// SQL composite foreign keys, XML `ID`/`IDREFS`, standoff offsets,
/// PENMAN variables, `CoNLL-U` heads within a sentence, array positions
/// and RDF subject IRIs.
fn every_family() -> Vec<(&'static str, Schema)> {
    vec![
        ("postgresql", keyed_schema()),
        ("xml", xml_family()),
        ("standoff", standoff_family()),
        ("penman", penman_family()),
        ("conllu", conllu_family()),
        ("positional", positional_family()),
        ("rdf", rdf_family()),
    ]
}

#[test]
fn every_family_of_key_and_reference_markup_is_expressible() {
    for (family, schema) in every_family() {
        assert!(
            !schema.declarations.is_empty(),
            "{family}: declares something"
        );
        assert_eq!(
            findings(&schema, &protocol()),
            vec![],
            "{family}: well formed"
        );

        // The declarations survive every serde form and are part of
        // the schema's canonical identity.
        let back: Schema =
            serde_json::from_str(&serde_json::to_string(&schema).expect("json")).expect("json");
        assert_eq!(back.declarations, schema.declarations, "{family}: json");
        let back: Schema =
            rmp_serde::from_slice(&rmp_serde::to_vec(&schema).expect("msgpack")).expect("msgpack");
        assert_eq!(back.declarations, schema.declarations, "{family}: msgpack");
        let mut bare = schema.clone();
        bare.declarations = SchemaDeclarations::default();
        assert_ne!(
            canonical_bytes(&schema),
            canonical_bytes(&bare),
            "{family}: identity"
        );

        // An identity cut keeps them.
        let all: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
        let apex = induce_on_vertices(&schema, &protocol(), &all).expect("induce");
        assert_eq!(apex.declarations, schema.declarations, "{family}: induce");
    }
}

/// The canonical bytes of declarations are frozen: a schema's identity
/// is keyed by them.
#[test]
fn declaration_bytes_are_pinned() {
    let hex = |schema: &Schema| blake3::hash(&canonical_bytes(schema)).to_hex().to_string();
    let digests: Vec<(&str, String)> = every_family()
        .iter()
        .map(|(family, schema)| (*family, hex(schema)))
        .collect();
    let expected: Vec<(&str, String)> = PINNED_FAMILY_DIGESTS
        .iter()
        .map(|(family, digest)| (*family, (*digest).to_owned()))
        .collect();
    assert_eq!(digests, expected);
}

const PINNED_FAMILY_DIGESTS: [(&str, &str); 7] = [
    (
        "postgresql",
        "7b6c4bf9d8832531c9ddef6b12c25a853d9f82b46363329b25c6e49646afa3c0",
    ),
    (
        "xml",
        "5808af1bcb4436524c806c9ca3a9471ae5f78691bfa41ea008be368149b8aa48",
    ),
    (
        "standoff",
        "f7087f353df8442c40b4a5b010c37674dd611e2bb96e130a12c321e11ff00d09",
    ),
    (
        "penman",
        "827b5b3d023488906cd85810ff178375741376d74ff32ed1676bed710d31490b",
    ),
    (
        "conllu",
        "a4ad5ed991ca30099b1fcbdea457d609fe10c298409ae9b753bf69854db44a46",
    ),
    (
        "positional",
        "b5508e2953ab8ff79a8c52b8745defea48a9bc87539a457ce73acfab18b96a52",
    ),
    (
        "rdf",
        "c8906d9d4766c4bd125bba9cf25a6c2bb0fc84f9c5c41fa7867a71b82ffe0ed4",
    ),
];

#[test]
fn induce_drops_a_key_whose_scope_stops_being_an_ancestor() {
    let mut schema = keyed_schema();
    film_keys(&mut schema)[0].scope = KeyScope::Within(Name::from("doc"));
    assert_eq!(findings(&schema, &protocol()), vec![]);

    // Both `doc` and `film` survive, but the only path from `doc` to `film`
    // does not, so the scope no longer contains the keyed vertex.
    let keep: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
    let keep_e: FxHashSet<Edge> = schema
        .edges
        .keys()
        .filter(|e| e.tgt != "film" || e.src == "film")
        .cloned()
        .collect();
    let (apex, report) =
        panproto_schema::induce_with_report(&schema, &protocol(), &keep, &keep_e).expect("induce");
    assert!(apex.declarations.is_empty());
    assert_eq!(
        report,
        DeclarationReport {
            dropped_keys: vec![(Name::from("film"), Name::from("pk"))],
            dropped_references: vec![Name::from("store_film")],
            dropped_target_set_members: Vec::new(),
            dropped_target_sets: Vec::new(),
        }
    );
}
fn declaration_matrix_schema() -> Schema {
    with_graph(
        &[
            ("c", "object"),
            ("h", "object"),
            ("t", "object"),
            ("u", "object"),
            ("v", "scalar"),
        ],
        &[
            prop("c", "h", "h"),
            prop("c", "t", "t"),
            prop("h", "v", "value"),
            prop("t", "v", "value"),
            prop("u", "v", "value"),
        ],
    )
    .build()
    .unwrap()
}

fn empty_target_key(id: &str, role: KeyRole) -> Key {
    plain_key(id, vec![], role, KeyScope::Document)
}

fn empty_reference(id: &str, holder: &str, target: &str) -> Reference {
    plain_reference(id, holder, RefSource::KeyValue { parts: vec![] }, target)
}

const fn standoff_source(text: RefTextSource) -> RefSource {
    RefSource::Standoff {
        text,
        start: vec![],
        end: vec![],
        unit: OffsetUnit::Byte,
        end_inclusive: false,
        end_form: SpanEnd::Offset,
        origin: 0,
        fragments: None,
    }
}

#[test]
fn extended_reference_equality_and_serde_keep_nondefault_coordinates() {
    use panproto_expr::Value;
    use panproto_schema::RefGuard;
    let first = RefGuard {
        field: vec![],
        equals: Value::Int(1),
    };
    let other_value = RefGuard {
        field: vec![],
        equals: Value::Int(2),
    };
    let other_field = RefGuard {
        field: vec![prop("h", "v", "value")],
        equals: Value::Int(1),
    };
    assert_eq!(
        first,
        RefGuard {
            field: vec![],
            equals: Value::Int(1)
        }
    );
    assert_ne!(first, other_value);
    assert_ne!(first, other_field);
    let mut reference = empty_reference("r", "h", "t");
    reference.null_match = NullMatch::Full;
    reference.source = standoff_source(RefTextSource::Local(vec![]));
    if let RefSource::Standoff { origin, .. } = &mut reference.source {
        *origin = 7;
    }
    round_trips(&reference);
    let encoded = serde_json::to_value(&reference).unwrap();
    assert_eq!(encoded["null_match"], "full");
    assert_eq!(encoded["source"]["standoff"]["origin"], 7);
}

#[test]
fn target_set_merging_and_mapping_keep_the_abstract_target() {
    use panproto_schema::TargetSet;
    let set = TargetSet {
        members: vec![("t".into(), "pk".into())],
        scope: KeyScope::Document,
    };
    let mut declarations = SchemaDeclarations::default();
    declarations
        .merge_target_set("targets".into(), set.clone())
        .unwrap();
    assert_eq!(declarations.target_set("targets"), Some(&set));
    declarations
        .merge_target_set("targets".into(), set.clone())
        .unwrap();
    let changed = TargetSet {
        scope: KeyScope::Global,
        ..set.clone()
    };
    assert!(matches!(
        declarations.merge_target_set("targets".into(), changed),
        Err(SchemaError::TargetSetConflict { .. })
    ));
    assert_eq!(declarations.target_set("targets"), Some(&set));
    declarations
        .keys
        .insert("t".into(), vec![empty_target_key("pk", KeyRole::Primary)]);
    let mut reference = empty_reference("r", "h", "targets");
    reference.edge = Some(prop("h", "targets", "resolved"));
    declarations.references.insert("r".into(), reference);
    let (mapped, report) = declarations
        .map_through(
            |name| Some(format!("mapped-{name}").into()),
            |edge| {
                (edge.tgt.as_ref() != "targets").then(|| Edge {
                    src: format!("mapped-{}", edge.src).into(),
                    tgt: format!("mapped-{}", edge.tgt).into(),
                    ..edge.clone()
                })
            },
        )
        .unwrap();
    assert!(report.is_empty());
    let reference = &mapped.references["r"];
    assert_eq!(reference.holder.as_ref(), "mapped-h");
    assert_eq!(reference.target_vertex.as_ref(), "targets");
    assert_eq!(
        reference.edge,
        Some(prop("mapped-h", "targets", "resolved"))
    );
    assert_eq!(
        mapped.target_sets["targets"].members,
        vec![("mapped-t".into(), "pk".into())]
    );
}

#[test]
fn reports_observe_each_drop_family_and_primary_selection_skips_other_roles() {
    let reports = [
        DeclarationReport {
            dropped_keys: vec![("t".into(), "k".into())],
            ..DeclarationReport::default()
        },
        DeclarationReport {
            dropped_references: vec!["r".into()],
            ..DeclarationReport::default()
        },
        DeclarationReport {
            dropped_target_set_members: vec![("s".into(), "t".into(), "k".into())],
            ..DeclarationReport::default()
        },
        DeclarationReport {
            dropped_target_sets: vec!["s".into()],
            ..DeclarationReport::default()
        },
    ];
    assert!(DeclarationReport::default().is_empty());
    for report in reports {
        assert!(!report.is_empty());
    }
    let mut declarations = SchemaDeclarations::default();
    declarations.keys.insert(
        "t".into(),
        vec![
            empty_target_key("index", KeyRole::Index),
            empty_target_key("primary", KeyRole::Primary),
        ],
    );
    assert_eq!(
        declarations
            .resolving_key(&empty_reference("r", "h", "t"))
            .map(|key| key.id.as_ref()),
        Some("primary")
    );
    assert_eq!(
        KeyRefError::UnknownVertex {
            vertex: "missing".into()
        }
        .to_string(),
        "declaration names unknown vertex missing"
    );
}

#[test]
fn protocol_and_declaration_gates_report_each_independent_failure() {
    let schema = declaration_matrix_schema();
    let mut object_only = protocol();
    object_only.obj_kinds = vec!["scalar".into()];
    assert!(
        panproto_schema::validate(&schema, &object_only)
            .iter()
            .any(|error| matches!(
                error,
                panproto_schema::ValidationError::InvalidVertexKind { .. }
            ))
    );
    for missing_source in [false, true] {
        let mut changed = schema.clone();
        let required = if missing_source {
            prop("missing", "t", "required")
        } else {
            prop("h", "missing", "required")
        };
        changed.required.insert("h".into(), vec![required]);
        assert!(
            panproto_schema::validate(&changed, &protocol())
                .iter()
                .any(|error| matches!(
                    error,
                    panproto_schema::ValidationError::DanglingRequiredEdge { .. }
                ))
        );
    }
    assert!(panproto_schema::validate_declarations(&schema).is_ok());
    let mut invalid = schema;
    invalid.declarations.keys.insert(
        "absent".into(),
        vec![empty_target_key("k", KeyRole::Primary)],
    );
    assert_eq!(
        panproto_schema::validate_declarations(&invalid),
        Err(vec![KeyRefError::UnknownVertex {
            vertex: "absent".into()
        }])
    );
}

#[test]
fn position_and_sort_keys_require_the_exact_ordered_carrier() {
    for within in [prop("h", "v", "value"), prop("c", "t", "missing")] {
        let mut schema = declaration_matrix_schema();
        schema.orderings.insert(within.clone(), 0);
        schema.declarations.keys.insert(
            "t".into(),
            vec![plain_key(
                "position",
                vec![KeyPart::Position {
                    within: within.clone(),
                }],
                KeyRole::Unique,
                KeyScope::Document,
            )],
        );
        assert_eq!(
            findings(&schema, &protocol()),
            vec![KeyRefError::PathDoesNotResolve {
                owner: "t".into(),
                path: vec![within]
            }]
        );
    }
    for ordered in [false, true] {
        let mut schema = declaration_matrix_schema();
        schema.declarations.keys.insert(
            "t".into(),
            vec![plain_key(
                "sort",
                vec![],
                KeyRole::SortKey,
                KeyScope::Within("c".into()),
            )],
        );
        if ordered {
            schema.orderings.insert(prop("c", "t", "t"), 0);
        }
        let errors = findings(&schema, &protocol());
        assert_eq!(errors.is_empty(), ordered);
    }
    // An ordered edge into another vertex cannot order the keyed vertex.
    let mut schema = declaration_matrix_schema();
    schema.orderings.insert(prop("c", "h", "h"), 0);
    schema.declarations.keys.insert(
        "t".into(),
        vec![plain_key(
            "sort",
            vec![],
            KeyRole::SortKey,
            KeyScope::Within("c".into()),
        )],
    );
    assert_eq!(
        findings(&schema, &protocol()),
        vec![KeyRefError::SortKeyNotOrdered {
            key: "sort".into(),
            vertex: "t".into()
        }]
    );
}

#[test]
fn optional_null_vertices_and_named_missing_targets_are_checked() {
    for null_vertex in ["t", "missing"] {
        let mut schema = declaration_matrix_schema();
        let mut reference = empty_reference("r", "h", "t");
        reference.null_vertex = Some(null_vertex.into());
        schema.declarations.references.insert("r".into(), reference);
        assert_eq!(
            findings(&schema, &protocol()),
            if null_vertex == "t" {
                vec![]
            } else {
                vec![KeyRefError::UnknownVertex {
                    vertex: "missing".into(),
                }]
            }
        );
    }
    for source in [
        RefSource::Path {
            field: vec![],
            syntax: PathSyntax::JsonPointer,
        },
        standoff_source(RefTextSource::Local(vec![])),
    ] {
        let mut schema = declaration_matrix_schema();
        let mut reference = plain_reference("r", "h", source, "t");
        reference.target_key = Some("missing".into());
        schema.declarations.references.insert("r".into(), reference);
        assert_eq!(
            findings(&schema, &protocol()),
            vec![KeyRefError::UnknownTargetKey {
                reference: "r".into(),
                key: "missing".into()
            }]
        );
    }
}

#[test]
fn standoff_end_and_fragment_requirements_are_independent() {
    for (length, inclusive) in [(false, true), (true, false), (true, true)] {
        let mut schema = declaration_matrix_schema();
        let mut source = standoff_source(RefTextSource::Local(vec![]));
        if let RefSource::Standoff {
            end_form,
            end_inclusive,
            ..
        } = &mut source
        {
            *end_form = if length {
                SpanEnd::Length
            } else {
                SpanEnd::Offset
            };
            *end_inclusive = inclusive;
        }
        schema
            .declarations
            .references
            .insert("r".into(), plain_reference("r", "h", source, "t"));
        assert_eq!(
            findings(&schema, &protocol()),
            if length && inclusive {
                vec![KeyRefError::LengthInclusive {
                    reference: "r".into(),
                }]
            } else {
                vec![]
            }
        );
    }
    for defect in 0..4 {
        let mut schema = declaration_matrix_schema();
        let carrier = if defect == 2 {
            prop("t", "v", "value")
        } else {
            prop("h", "v", "value")
        };
        if defect != 3 {
            schema.orderings.insert(carrier.clone(), 0);
        }
        let mut source = standoff_source(RefTextSource::Local(vec![]));
        if let RefSource::Standoff { fragments, .. } = &mut source {
            *fragments = Some(carrier);
        }
        let mut reference = plain_reference("r", "h", source, "t");
        reference.cardinality = if defect == 1 {
            Cardinality::One
        } else {
            Cardinality::Many
        };
        schema.declarations.references.insert("r".into(), reference);
        assert_eq!(
            findings(&schema, &protocol()),
            if defect == 0 {
                vec![]
            } else if defect == 2 {
                vec![
                    KeyRefError::PathDoesNotResolve {
                        owner: "h".into(),
                        path: vec![prop("t", "v", "value")],
                    },
                    KeyRefError::InvalidFragments {
                        reference: "r".into(),
                    },
                ]
            } else {
                vec![KeyRefError::InvalidFragments {
                    reference: "r".into(),
                }]
            }
        );
    }
}

#[test]
fn guard_groups_require_equal_holder_source_and_field() {
    use panproto_expr::Value;
    use panproto_schema::RefGuard;
    for coordinate in 0..3 {
        let mut schema = declaration_matrix_schema();
        for (id, value) in [("a", 0), ("b", 1), ("c", 1)] {
            let mut reference = empty_reference(id, "h", "t");
            let mut field = vec![];
            if id != "a" {
                match coordinate {
                    0 => reference.holder = "u".into(),
                    1 => {
                        reference.source = RefSource::Path {
                            field: vec![],
                            syntax: PathSyntax::JsonPointer,
                        }
                    }
                    _ => field = vec![prop("h", "v", "value")],
                }
            }
            reference.guard = Some(RefGuard {
                field,
                equals: Value::Int(value),
            });
            schema.declarations.references.insert(id.into(), reference);
        }
        assert_eq!(
            findings(&schema, &protocol()),
            vec![KeyRefError::DuplicateGuard {
                references: vec!["b".into(), "c".into()]
            }]
        );
    }
    let mut schema = declaration_matrix_schema();
    let mut reference = empty_reference("single", "h", "t");
    reference.guard = Some(RefGuard {
        field: vec![],
        equals: Value::Int(1),
    });
    schema
        .declarations
        .references
        .insert("single".into(), reference);
    assert_eq!(findings(&schema, &protocol()), vec![]);
}

fn binder_matrix_schema() -> Schema {
    let mut schema = declaration_matrix_schema();
    schema.declarations.keys.insert(
        "t".into(),
        vec![
            empty_target_key("binder", KeyRole::Identifier),
            empty_target_key("other", KeyRole::Identifier),
        ],
    );
    schema.declarations.keys.insert(
        "u".into(),
        vec![empty_target_key("binder", KeyRole::Identifier)],
    );
    let reference = plain_reference(
        "variable",
        "h",
        RefSource::Variable {
            field: vec![],
            binder_key: "binder".into(),
            binding: VariableBinding::FirstDefines,
        },
        "t",
    );
    schema
        .declarations
        .references
        .insert("variable".into(), reference);
    schema
}

#[test]
fn binder_conflicts_require_both_target_coordinates_and_a_nonvariable_use() {
    use panproto_schema::TargetSet;
    for (vertex, key, conflict) in [
        ("t", "binder", true),
        ("t", "other", false),
        ("u", "binder", false),
    ] {
        for through_set in [false, true] {
            let mut schema = binder_matrix_schema();
            let target = if through_set {
                schema.declarations.target_sets.insert(
                    "targets".into(),
                    TargetSet {
                        members: vec![(vertex.into(), key.into())],
                        scope: KeyScope::Document,
                    },
                );
                "targets"
            } else {
                vertex
            };
            let mut reference = empty_reference("ordinary", "h", target);
            if !through_set {
                reference.target_key = Some(key.into());
            }
            schema
                .declarations
                .references
                .insert("ordinary".into(), reference);
            assert_eq!(
                findings(&schema, &protocol()),
                if conflict {
                    vec![KeyRefError::BinderModeConflict {
                        key: "binder".into(),
                        references: vec!["variable".into()],
                    }]
                } else {
                    vec![]
                }
            );
        }
    }
    let mut schema = binder_matrix_schema();
    let reference = plain_reference(
        "nearest",
        "h",
        RefSource::Variable {
            field: vec![],
            binder_key: "binder".into(),
            binding: VariableBinding::NearestPreceding,
        },
        "t",
    );
    schema
        .declarations
        .references
        .insert("nearest".into(), reference);
    assert_eq!(
        findings(&schema, &protocol()),
        vec![KeyRefError::BinderModeConflict {
            key: "binder".into(),
            references: vec!["nearest".into(), "variable".into()]
        }]
    );
}

fn text_reference_matrix_schema() -> Schema {
    let mut schema = declaration_matrix_schema();
    schema.declarations.references.insert(
        "inner".into(),
        plain_reference(
            "inner",
            "h",
            RefSource::Path {
                field: vec![],
                syntax: PathSyntax::JsonPointer,
            },
            "t",
        ),
    );
    schema.declarations.references.insert(
        "outer".into(),
        plain_reference(
            "outer",
            "h",
            standoff_source(RefTextSource::Referenced {
                reference: "inner".into(),
                text: vec![],
                external: None,
            }),
            "u",
        ),
    );
    schema
}

#[test]
fn text_references_check_holder_cardinality_kind_and_acyclicity() {
    use panproto_schema::TargetSet;
    for defect in 0..6 {
        let mut schema = text_reference_matrix_schema();
        match defect {
            1 => {
                schema
                    .declarations
                    .references
                    .get_mut("inner")
                    .unwrap()
                    .holder = "u".into();
            }
            2 => {
                schema
                    .declarations
                    .references
                    .get_mut("inner")
                    .unwrap()
                    .cardinality = Cardinality::Many;
            }
            3 => {
                schema
                    .declarations
                    .references
                    .get_mut("inner")
                    .unwrap()
                    .source = standoff_source(RefTextSource::Local(vec![]));
            }
            4 => {
                schema
                    .declarations
                    .keys
                    .insert("t".into(), vec![empty_target_key("pk", KeyRole::Primary)]);
                schema.declarations.target_sets.insert(
                    "targets".into(),
                    TargetSet {
                        members: vec![("t".into(), "pk".into())],
                        scope: KeyScope::Document,
                    },
                );
                schema
                    .declarations
                    .references
                    .get_mut("inner")
                    .unwrap()
                    .target_vertex = "targets".into();
            }
            5 => {
                schema
                    .declarations
                    .references
                    .get_mut("inner")
                    .unwrap()
                    .source = standoff_source(RefTextSource::Span {
                    reference: "outer".into(),
                });
                schema
                    .declarations
                    .references
                    .get_mut("outer")
                    .unwrap()
                    .source = standoff_source(RefTextSource::Span {
                    reference: "inner".into(),
                });
            }
            _ => {}
        }
        let errors = findings(&schema, &protocol());
        let outer_invalid = errors.iter().any(|error| matches!(error, KeyRefError::TextReferenceInvalid { reference, .. } if reference.as_ref() == "outer"));
        assert_eq!(outer_invalid, defect != 0, "case {defect}: {errors:?}");
    }
    let mut schema = text_reference_matrix_schema();
    schema
        .declarations
        .references
        .get_mut("inner")
        .unwrap()
        .source = standoff_source(RefTextSource::Local(vec![]));
    schema
        .declarations
        .references
        .get_mut("outer")
        .unwrap()
        .source = standoff_source(RefTextSource::Span {
        reference: "inner".into(),
    });
    assert_eq!(findings(&schema, &protocol()), vec![]);
}

fn scope_via_matrix_schema() -> Schema {
    use panproto_schema::ScopeVia;
    let mut schema = with_graph(
        &[
            ("c", "object"),
            ("h", "object"),
            ("t", "object"),
            ("u", "object"),
        ],
        &[
            prop("c", "h", "h"),
            prop("u", "h", "h"),
            prop("u", "t", "t"),
        ],
    )
    .build()
    .unwrap();
    schema.declarations.keys.insert(
        "t".into(),
        vec![plain_key(
            "scoped",
            vec![],
            KeyRole::Primary,
            KeyScope::Within("u".into()),
        )],
    );
    schema.declarations.references.insert(
        "hop".into(),
        plain_reference(
            "hop",
            "c",
            RefSource::Path {
                field: vec![],
                syntax: PathSyntax::JsonPointer,
            },
            "u",
        ),
    );
    let mut reference = empty_reference("r", "h", "t");
    reference.scope_via = Some(ScopeVia {
        ancestor: "c".into(),
        hops: vec!["hop".into()],
    });
    schema.declarations.references.insert("r".into(), reference);
    schema
}

#[test]
fn scope_via_requires_a_nonempty_single_valued_chain_from_a_proper_ancestor() {
    use panproto_schema::ScopeVia;
    for defect in 0..6 {
        let mut schema = scope_via_matrix_schema();
        match defect {
            1 => {
                schema
                    .declarations
                    .references
                    .get_mut("r")
                    .unwrap()
                    .scope_via = Some(ScopeVia {
                    ancestor: "u".into(),
                    hops: vec![],
                });
            }
            2 => {
                schema
                    .declarations
                    .references
                    .get_mut("r")
                    .unwrap()
                    .scope_via
                    .as_mut()
                    .unwrap()
                    .ancestor = "h".into();
                schema
                    .declarations
                    .references
                    .get_mut("hop")
                    .unwrap()
                    .holder = "h".into();
            }
            3 => {
                schema
                    .declarations
                    .references
                    .get_mut("hop")
                    .unwrap()
                    .cardinality = Cardinality::Many;
            }
            4 => {
                schema
                    .declarations
                    .references
                    .get_mut("hop")
                    .unwrap()
                    .holder = "t".into();
            }
            5 => {
                schema
                    .declarations
                    .references
                    .get_mut("hop")
                    .unwrap()
                    .target_vertex = "c".into();
            }
            _ => {}
        }
        assert_eq!(
            findings(&schema, &protocol()),
            if defect == 0 {
                vec![]
            } else {
                vec![KeyRefError::ScopeViaInvalid {
                    reference: "r".into(),
                }]
            },
            "case {defect}"
        );
    }
}

#[test]
fn empty_target_sets_never_resolve_by_the_keyless_reference_exception() {
    use panproto_schema::TargetSet;
    for named_key in [false, true] {
        let mut schema = declaration_matrix_schema();
        schema.declarations.target_sets.insert(
            "targets".into(),
            TargetSet {
                members: vec![],
                scope: KeyScope::Document,
            },
        );
        let mut reference = empty_reference("r", "h", "targets");
        if named_key {
            reference.target_key = Some("unused".into());
        }
        schema.declarations.references.insert("r".into(), reference);
        let mut expected = vec![KeyRefError::EmptyTargetSet {
            set: "targets".into(),
        }];
        if named_key {
            expected.push(KeyRefError::TargetSetHasKey {
                reference: "r".into(),
            });
        }
        expected.push(KeyRefError::UnknownTarget {
            reference: "r".into(),
        });
        assert_eq!(findings(&schema, &protocol()), expected);
    }
}

#[test]
fn canonical_key_equality_envelope_has_the_exact_extended_role_byte() {
    use panproto_schema::KeyEquality;
    let bare = declaration_matrix_schema();
    let bare_bytes = canonical_bytes(&bare);
    let mut empty_tail = CanonicalWriter::new();
    empty_tail.len(0);
    empty_tail.len(0);
    let empty_tail = empty_tail.into_bytes();
    assert!(bare_bytes.ends_with(&empty_tail));
    let prefix = &bare_bytes[..bare_bytes.len() - empty_tail.len()];
    for (role, byte) in [
        (KeyRole::Primary, 128),
        (KeyRole::Unique, 129),
        (KeyRole::Identifier, 130),
        (KeyRole::SortKey, 131),
        (KeyRole::Index, 132),
    ] {
        let mut schema = bare.clone();
        let mut key = empty_target_key("k", role);
        key.parts = vec![KeyPart::Path(vec![])];
        key.equality = vec![KeyEquality::Representation];
        schema.declarations.keys.insert("t".into(), vec![key]);
        let mut expected = CanonicalWriter::new();
        expected.len(1);
        expected.name(&"t".into());
        expected.len(1);
        expected.name(&"k".into());
        expected.len(1);
        expected.tag(0);
        expected.len(0);
        expected.tag(byte);
        expected.tag(0);
        expected.len(1);
        expected.tag(0);
        expected.len(0);
        let encoded = canonical_bytes(&schema);
        assert!(encoded.starts_with(prefix));
        assert_eq!(&encoded[prefix.len()..], expected.into_bytes());
    }
}

#[test]
fn variable_binding_serialization_keeps_the_nondefault_selection_rule() {
    let mut source = RefSource::Variable {
        field: vec![],
        binder_key: "binder".into(),
        binding: VariableBinding::FirstDefines,
    };
    round_trips(&source);
    assert_eq!(
        json(&source),
        r#"{"variable":{"field":[],"binder_key":"binder"}}"#
    );
    if let RefSource::Variable { binding, .. } = &mut source {
        *binding = VariableBinding::NearestPreceding;
    }
    round_trips(&source);
    assert_eq!(
        json(&source),
        r#"{"variable":{"field":[],"binder_key":"binder","binding":"nearest_preceding"}}"#
    );
}

#[test]
fn exact_float_reading_omits_only_the_default_serialized_field() {
    use panproto_schema::value_kinds::FloatReading;
    use panproto_schema::{ScalarKind, ValueSpec};

    let mut spec = ValueSpec::new(ScalarKind::Float { bits: 64 });
    assert!(spec.float_reading.is_exact());
    round_trips(&spec);
    let exact = serde_json::to_value(&spec).unwrap();
    assert!(exact.get("float_reading").is_none());
    spec.float_reading = FloatReading::NearestKeepingLexeme;
    assert!(!spec.float_reading.is_exact());
    round_trips(&spec);
    let nearest = serde_json::to_value(&spec).unwrap();
    assert_eq!(nearest["float_reading"], "nearest_keeping_lexeme");
}

#[test]
fn reference_extension_slots_round_trip_for_every_presence_pattern() {
    use panproto_expr::Value;
    use panproto_schema::{RefGuard, ScopeVia};

    for mask in 0..16 {
        let mut reference = empty_reference("r", "h", "t");
        if mask & 1 != 0 {
            reference.guard = Some(RefGuard {
                field: vec![],
                equals: Value::Int(3),
            });
        }
        if mask & 2 != 0 {
            reference.null_match = NullMatch::Full;
        }
        if mask & 4 != 0 {
            reference.null_vertex = Some("null".into());
        }
        if mask & 8 != 0 {
            reference.scope_via = Some(ScopeVia {
                ancestor: "c".into(),
                hops: vec!["hop".into()],
            });
        }
        round_trips(&reference);
        let encoded = serde_json::to_value(&reference).unwrap();
        for (bit, field) in ["guard", "null_match", "null_vertex", "scope_via"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(encoded.get(field).is_some(), mask & (1 << bit) != 0);
        }
        if mask == 0 {
            let legacy = (
                &reference.id,
                &reference.holder,
                &reference.source,
                &reference.target_vertex,
                &reference.target_key,
                &reference.edge,
                &reference.cardinality,
                reference.nullable,
                &reference.on_unresolved,
                &reference.mode,
            );
            assert_eq!(
                rmp_serde::to_vec(&reference).unwrap(),
                rmp_serde::to_vec(&legacy).unwrap()
            );
        }
    }
}

#[test]
fn standoff_extension_slots_round_trip_for_every_presence_pattern() {
    for mask in 0..8 {
        let source = RefSource::Standoff {
            text: RefTextSource::Local(vec![]),
            start: vec![],
            end: vec![],
            unit: OffsetUnit::Byte,
            end_inclusive: false,
            end_form: if mask & 1 == 0 {
                SpanEnd::Offset
            } else {
                SpanEnd::Length
            },
            origin: if mask & 2 == 0 { 0 } else { 7 },
            fragments: (mask & 4 != 0).then(|| prop("h", "t", "fragments")),
        };
        round_trips(&source);
        let encoded = serde_json::to_value(&source).unwrap();
        for (bit, field) in ["end_form", "origin", "fragments"].into_iter().enumerate() {
            assert_eq!(
                encoded["standoff"].get(field).is_some(),
                mask & (1 << bit) != 0
            );
        }
        if mask == 0 {
            // Externally tagged variant with the original five positional fields.
            let legacy = std::collections::BTreeMap::from([(
                "standoff",
                (
                    RefTextSource::Local(vec![]),
                    Vec::<Edge>::new(),
                    Vec::<Edge>::new(),
                    OffsetUnit::Byte,
                    false,
                ),
            )]);
            assert_eq!(
                rmp_serde::to_vec(&source).unwrap(),
                rmp_serde::to_vec(&legacy).unwrap()
            );
        }
    }
}

#[test]
fn value_spec_extension_slots_round_trip_for_every_presence_pattern() {
    use panproto_expr::exact::TimeUnit;
    use panproto_schema::value_kinds::{FloatReading, NumeralDialect};
    use panproto_schema::{ScalarKind, ValueSpec};
    use std::collections::BTreeMap;

    for mask in 0..32 {
        let mut spec = ValueSpec::new(ScalarKind::Float { bits: 64 });
        if mask & 1 != 0 {
            spec.float_reading = FloatReading::NearestKeepingLexeme;
        }
        if mask & 2 != 0 {
            spec.numeral = Some(NumeralDialect::Json);
        }
        if mask & 4 != 0 {
            spec.discriminators = Some(BTreeMap::from([("s".to_owned(), ScalarKind::Str)]));
        }
        if mask & 8 != 0 {
            spec.time_unit = Some(TimeUnit::Millis);
        }
        if mask & 16 != 0 {
            spec.time_zone = Some("UTC".to_owned());
        }
        round_trips(&spec);
        let encoded = serde_json::to_value(&spec).unwrap();
        for (bit, field) in [
            "float_reading",
            "numeral",
            "discriminators",
            "time_unit",
            "time_zone",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(encoded.get(field).is_some(), mask & (1 << bit) != 0);
        }
        if mask == 0 {
            let legacy = (
                &spec.kind,
                spec.precision,
                spec.scale,
                spec.max_length,
                spec.with_time_zone,
                spec.int_bits,
                spec.unsigned,
            );
            assert_eq!(
                rmp_serde::to_vec(&spec).unwrap(),
                rmp_serde::to_vec(&legacy).unwrap()
            );
        }
    }
}

#[test]
fn declaration_report_extension_slots_round_trip_independently() {
    for mask in 0..4 {
        let mut report = DeclarationReport::default();
        if mask & 1 != 0 {
            report.dropped_target_set_members = vec![("set".into(), "v".into(), "k".into())];
        }
        if mask & 2 != 0 {
            report.dropped_target_sets = vec!["set".into()];
        }
        round_trips(&report);
        let encoded = serde_json::to_value(&report).unwrap();
        assert_eq!(
            encoded.get("dropped_target_set_members").is_some(),
            mask & 1 != 0
        );
        assert_eq!(encoded.get("dropped_target_sets").is_some(), mask & 2 != 0);
        if mask == 0 {
            let legacy = (&report.dropped_keys, &report.dropped_references);
            assert_eq!(
                rmp_serde::to_vec(&report).unwrap(),
                rmp_serde::to_vec(&legacy).unwrap()
            );
        }
    }
}

#[test]
fn protocol_edge_rules_distinguish_empty_allowed_and_forbidden_endpoint_kinds() {
    use panproto_schema::{EdgeRule, ValidationError};

    let schema = with_graph(
        &[("source", "object"), ("target", "scalar")],
        &[prop("source", "target", "value")],
    )
    .build()
    .expect("fixture graph");
    for source_rule in [vec![], vec!["object".to_owned()], vec!["scalar".to_owned()]] {
        for target_rule in [vec![], vec!["scalar".to_owned()], vec!["object".to_owned()]] {
            let mut protocol = protocol();
            protocol.obj_kinds = vec!["object".to_owned(), "scalar".to_owned()];
            protocol.edge_rules = vec![EdgeRule {
                edge_kind: "prop".to_owned(),
                src_kinds: source_rule.clone(),
                tgt_kinds: target_rule.clone(),
            }];
            let mut expected = Vec::new();
            if source_rule == ["scalar"] {
                expected.push(ValidationError::InvalidEdge {
                    src: "source".to_owned(),
                    tgt: "target".to_owned(),
                    kind: "prop".to_owned(),
                    reason: "source kind 'object' not in permitted: [\"scalar\"]".to_owned(),
                });
            }
            if target_rule == ["object"] {
                expected.push(ValidationError::InvalidEdge {
                    src: "source".to_owned(),
                    tgt: "target".to_owned(),
                    kind: "prop".to_owned(),
                    reason: "target kind 'scalar' not in permitted: [\"object\"]".to_owned(),
                });
            }
            assert_eq!(panproto_schema::validate(&schema, &protocol), expected);
        }
    }
}

#[test]
fn target_set_virtual_edges_require_the_declared_holder_and_each_member_edge() {
    use panproto_schema::TargetSet;

    for source in ["h", "other"] {
        for member_edge_present in [false, true] {
            let edges = if member_edge_present {
                vec![prop("h", "t", "picked"), prop("other", "t", "picked")]
            } else {
                vec![]
            };
            let mut schema = with_graph(
                &[("h", "object"), ("other", "object"), ("t", "object")],
                &edges,
            )
            .build()
            .expect("fixture graph");
            schema
                .declarations
                .keys
                .insert("t".into(), vec![empty_target_key("key", KeyRole::Primary)]);
            schema.declarations.target_sets.insert(
                "targets".into(),
                TargetSet {
                    members: vec![("t".into(), "key".into())],
                    scope: KeyScope::Document,
                },
            );
            let virtual_edge = prop(source, "targets", "picked");
            let mut reference = empty_reference("r", "h", "targets");
            reference.edge = Some(virtual_edge.clone());
            schema.declarations.references.insert("r".into(), reference);
            let expected = if source == "h" && member_edge_present {
                vec![]
            } else {
                vec![KeyRefError::EdgeMismatch {
                    reference: "r".into(),
                    edge: virtual_edge,
                }]
            };
            assert_eq!(findings(&schema, &protocol()), expected);
        }
    }
}

#[test]
fn target_set_named_key_and_arity_errors_are_reported_independently() {
    use panproto_schema::TargetSet;

    for named_key in [false, true] {
        for part_count in 0..=1 {
            let mut schema = declaration_matrix_schema();
            schema
                .declarations
                .keys
                .insert("t".into(), vec![empty_target_key("key", KeyRole::Primary)]);
            schema.declarations.target_sets.insert(
                "targets".into(),
                TargetSet {
                    members: vec![("t".into(), "key".into())],
                    scope: KeyScope::Document,
                },
            );
            let mut reference = empty_reference("r", "h", "targets");
            reference.target_key = named_key.then(|| "unused".into());
            reference.source = RefSource::KeyValue {
                parts: vec![vec![]; part_count],
            };
            schema.declarations.references.insert("r".into(), reference);
            let mut expected = vec![];
            if named_key {
                expected.push(KeyRefError::TargetSetHasKey {
                    reference: "r".into(),
                });
            }
            if part_count != 0 {
                expected.push(KeyRefError::ArityMismatch {
                    reference: "r".into(),
                    expected: 0,
                    found: part_count,
                });
            }
            assert_eq!(findings(&schema, &protocol()), expected);
        }
    }
}
