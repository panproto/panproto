//! The JSON Schema reader and writer against the json-schema reference.
//!
//! Each bundle of the reference scope is rendered as JSON text and read
//! through production; the outcome (the refusal's document, path and
//! class, or the schema's vertices, ordered out-edges, kept constraints,
//! required properties, entries and roots) must equal the reference's, and
//! each document of an accepted bundle must write as the reference's
//! canonical text and read back as the same schema.

#![cfg(feature = "tree-sitter")]

use std::collections::{BTreeMap, BTreeSet};

use panproto_io::data_schema::json_schema::{
    BundleDocument, JsonSchemaErrorKind, JsonSchemaReading, read_bundle, write_document,
};
use panproto_reference::enumerate::json_schema::{Bundle, base, document_count, documents};
use panproto_reference::pages::json_schema::{Doc, Kind, Reading, canonical_text, read, render};
use proptest::prelude::*;

/// Production's reading in the reference's form.
fn production_reading(reading: &JsonSchemaReading) -> Reading {
    let schema = &reading.schema;
    let mut out = Reading::default();
    for (id, vertex) in &schema.vertices {
        out.vertices.insert(id.to_string(), vertex.kind.to_string());
        let edges: Vec<_> = schema
            .outgoing_edges(id)
            .iter()
            .map(|e| {
                (
                    e.kind.to_string(),
                    e.tgt.to_string(),
                    e.name.as_ref().map(ToString::to_string),
                )
            })
            .collect();
        if !edges.is_empty() {
            out.edges.insert(id.to_string(), edges);
        }
    }
    for (id, constraints) in &schema.constraints {
        out.constraints.insert(
            id.to_string(),
            constraints
                .iter()
                .map(|c| (c.sort.to_string(), c.value.clone()))
                .collect(),
        );
    }
    for (id, edges) in &schema.required {
        let names: BTreeSet<String> = edges
            .iter()
            .filter_map(|e| e.name.as_ref().map(ToString::to_string))
            .collect();
        out.required.insert(id.to_string(), names);
    }
    out.entries = schema.entries.iter().map(ToString::to_string).collect();
    out.roots.clone_from(&reading.roots);
    out
}

/// Whether production's refusal class is the reference's.
fn same_class(production: &JsonSchemaErrorKind, reference: &Kind) -> bool {
    match (production, reference) {
        (JsonSchemaErrorKind::WrongType(a), Kind::WrongType(b))
        | (JsonSchemaErrorKind::Superseded(a), Kind::Superseded(b)) => a == b,
        (JsonSchemaErrorKind::Invalid(_), Kind::Invalid)
        | (JsonSchemaErrorKind::UnsupportedDialect(_), Kind::UnsupportedDialect)
        | (JsonSchemaErrorKind::DynamicScope(_), Kind::DynamicScope)
        | (JsonSchemaErrorKind::DuplicateId(_), Kind::DuplicateId)
        | (JsonSchemaErrorKind::DuplicateAnchor(_), Kind::DuplicateAnchor)
        | (JsonSchemaErrorKind::NoBase(_), Kind::NoBase)
        | (JsonSchemaErrorKind::RemoteRef(_), Kind::RemoteRef)
        | (JsonSchemaErrorKind::RefTarget(_), Kind::RefTarget)
        | (JsonSchemaErrorKind::UnknownAnchor(_), Kind::UnknownAnchor)
        | (JsonSchemaErrorKind::DuplicateDocument(_), Kind::DuplicateDocument) => true,
        _ => false,
    }
}

/// Compare production with the reference on one bundle, and on an accepted
/// one compare each document's canonical text and the bundle's reading
/// back. Returns whether the bundle was accepted.
fn check(bundle: &Bundle) -> Result<bool, String> {
    let texts: Vec<String> = bundle.iter().map(|(_, doc)| render(doc)).collect();
    let documents: Vec<BundleDocument<'_>> = bundle
        .iter()
        .zip(&texts)
        .map(|((uri, _), text)| BundleDocument {
            retrieval_uri: uri.as_deref(),
            source: text.as_bytes(),
        })
        .collect();
    match (read_bundle(&documents), read(bundle)) {
        (Ok(reading), Ok(reference)) => {
            let got = production_reading(&reading);
            if got != reference {
                return Err(format!(
                    "{texts:?}\nproduction {got:#?}\nreference {reference:#?}"
                ));
            }
            let mut canonical = Vec::new();
            for ((root, (_, doc)), text) in reading.roots.iter().zip(bundle).zip(&texts) {
                let written =
                    write_document(&reading.schema, root).map_err(|e| format!("{text}\n{e}"))?;
                if written != canonical_text(doc) {
                    return Err(format!(
                        "{text}\nproduction {written}\nreference {}",
                        canonical_text(doc)
                    ));
                }
                canonical.push(written);
            }
            let again: Vec<BundleDocument<'_>> = bundle
                .iter()
                .zip(&canonical)
                .map(|((uri, _), text)| BundleDocument {
                    retrieval_uri: uri.as_deref(),
                    source: text.as_bytes(),
                })
                .collect();
            let back = read_bundle(&again).map_err(|e| format!("{canonical:?}\n{e}"))?;
            if production_reading(&back) != got {
                return Err(format!("{canonical:?}\nreads back as another schema"));
            }
            Ok(true)
        }
        (Err(error), Err(refusal)) => {
            if error.document == refusal.document
                && error.path == refusal.path
                && same_class(&error.kind, &refusal.kind)
            {
                Ok(false)
            } else {
                Err(format!(
                    "{texts:?}\nproduction {error:?}\nreference {refusal:?}"
                ))
            }
        }
        (production, reference) => Err(format!(
            "{texts:?}\nproduction {:?}\nreference {reference:?}",
            production.map(|r| production_reading(&r))
        )),
    }
}

#[test]
fn json_schema_reading_and_canonical_writing_match_the_reference_on_every_bounded_bundle() {
    let bundles = documents();
    assert_eq!(bundles.len(), document_count(24, 22));
    let mut accepted = 0;
    let mut failures = BTreeMap::new();
    for (i, bundle) in bundles.iter().enumerate() {
        match check(bundle) {
            Ok(true) => accepted += 1,
            Ok(false) => {}
            Err(message) => {
                failures.insert(i, message);
            }
        }
    }
    assert_eq!(
        failures.len(),
        0,
        "{} disagreements: {failures:#?}",
        failures.len()
    );
    assert_eq!(accepted, ACCEPTED, "accepted bundles of the scope");
}

/// The accepted bundles of the scope, as the json-schema page derives
/// them: 16 of the 24 nodes read under `a` and 15 under `b` (a pointer to
/// `/properties/a` names nothing without `a`), and two properties both
/// accepted read unless they declare the same `$id` (twice `n`, twice `m`),
/// the same dynamic anchor in the root resource (twice `D`), or the
/// dynamic anchor `D` in two resources: `16^2 - 5 = 251` pairs in each
/// order. With the base and 10 of the 22 envelopes, `1 + 31 + 502 + 10`.
const ACCEPTED: usize = 544;

/// A subschema of depth at most `depth`, with property names that need
/// escaping in JSON pointers.
fn subschema(depth: u32) -> BoxedStrategy<Doc> {
    let name = prop::sample::select(vec!["a", "b/c", "~x", "p q", "k"]);
    let leaf = prop_oneof![
        prop::sample::select(vec!["string", "integer", "number", "boolean", "null"])
            .prop_map(|t| Doc::Map(vec![("type".into(), Doc::Str(t.into()))])),
        any::<bool>().prop_map(Doc::Bool),
        Just(Doc::Map(vec![(
            "$ref".into(),
            Doc::Str("#/$defs/d".into())
        )])),
        Just(Doc::Map(vec![("$ref".into(), Doc::Str("#A".into()))])),
    ];
    leaf.prop_recursive(depth, 16, 3, move |inner| {
        prop_oneof![
            prop::collection::vec((name.clone(), inner.clone()), 0..3).prop_map(|props| {
                let mut seen = BTreeSet::new();
                let props: Vec<(String, Doc)> = props
                    .into_iter()
                    .filter(|(k, _)| seen.insert(*k))
                    .map(|(k, v)| (k.to_owned(), v))
                    .collect();
                let required = Doc::Seq(
                    props
                        .iter()
                        .take(1)
                        .map(|(k, _)| Doc::Str(k.clone()))
                        .chain([Doc::Str("u".into())])
                        .collect(),
                );
                Doc::Map(vec![
                    ("properties".into(), Doc::Map(props)),
                    ("required".into(), required),
                ])
            }),
            inner.clone().prop_map(|items| Doc::Map(vec![
                ("type".into(), Doc::Str("array".into())),
                ("items".into(), items),
            ])),
            prop::collection::vec(inner, 1..3)
                .prop_map(|arms| Doc::Map(vec![("anyOf".into(), Doc::Seq(arms))])),
        ]
    })
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_json_schemas_read_and_write_as_the_reference(node in subschema(3)) {
        let bundle = vec![(None, base(vec![("spec".into(), node)]))];
        prop_assert_eq!(check(&bundle).err(), None::<String>);
    }
}
