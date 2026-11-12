//! The CRD reader and writer against the kubernetes-crds reference.
//!
//! Each document of the reference scope is rendered as block YAML and read
//! through production; the outcome (the refusal's path and class, or the
//! schema's vertices, ordered out-edges, kept constraints, required
//! properties, list-map keys and unresolved entries) must equal the
//! reference's, and an accepted document's canonical text must equal the
//! reference's and read back as the same schema.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use panproto_io::config::k8s_crd::{CrdErrorKind, CrdReading, read_crd, write_crd};
use panproto_reference::enumerate::kubernetes_crds::{document_count, documents};
use panproto_reference::pages::kubernetes_crds::{
    Doc, Kind, Reading, canonical_text, read, render_yaml,
};
use panproto_schema::{KeyPart, KeyScope};
use proptest::prelude::*;

/// Production's reading in the reference's form.
fn production_reading(reading: &CrdReading) -> Reading {
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
        for key in schema.declarations.keys_of(id) {
            let KeyScope::Within(list) = &key.scope else {
                panic!("a list-map key is scoped within its list");
            };
            let names = key
                .parts
                .iter()
                .map(|part| match part {
                    KeyPart::Path(path) => path[0].name.as_ref().unwrap().to_string(),
                    KeyPart::Position { .. } => panic!("a list-map key part is a path"),
                })
                .collect();
            out.keys.insert(id.to_string(), (list.to_string(), names));
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
            .map(|e| e.name.as_ref().unwrap().to_string())
            .collect();
        out.required.insert(id.to_string(), names);
    }
    out.unresolved = reading
        .unresolved
        .iter()
        .map(|u| (u.path.clone(), u.name.clone()))
        .collect();
    out
}

/// Whether production's refusal class is the reference's.
fn same_class(production: &CrdErrorKind, reference: &Kind) -> bool {
    match (production, reference) {
        (CrdErrorKind::ApiVersion(_), Kind::ApiVersion)
        | (CrdErrorKind::Kind(_), Kind::Kind)
        | (CrdErrorKind::Invalid(_), Kind::Invalid) => true,
        (CrdErrorKind::Missing(a), Kind::Missing(b))
        | (CrdErrorKind::WrongType(a), Kind::WrongType(b))
        | (CrdErrorKind::Forbidden(a), Kind::Forbidden(b))
        | (CrdErrorKind::Unsupported(a), Kind::Unsupported(b)) => a == b,
        (CrdErrorKind::UnknownField(a), Kind::UnknownField(b))
        | (CrdErrorKind::DuplicateVersion(a), Kind::DuplicateVersion(b))
        | (CrdErrorKind::ListMapKey(a), Kind::ListMapKey(b)) => a == b,
        (CrdErrorKind::StorageVersions(a), Kind::StorageVersions(b)) => a == b,
        _ => false,
    }
}

/// Compare production with the reference on one document, and on an
/// accepted one compare the canonical text and its reading back. Returns
/// whether the document was accepted.
fn check(doc: &Doc) -> Result<bool, String> {
    let text = render_yaml(doc);
    let expected = read(doc);
    match (read_crd(text.as_bytes()), expected) {
        (Ok(reading), Ok(reference)) => {
            let got = production_reading(&reading);
            if got != reference {
                return Err(format!(
                    "{text}\nproduction {got:#?}\nreference {reference:#?}"
                ));
            }
            let canonical = write_crd(&reading.schema).map_err(|e| format!("{text}\n{e}"))?;
            if canonical != canonical_text(doc) {
                return Err(format!(
                    "{text}\nproduction {canonical}\nreference {}",
                    canonical_text(doc)
                ));
            }
            let again = read_crd(canonical.as_bytes()).map_err(|e| format!("{canonical}\n{e}"))?;
            if production_reading(&again) != got {
                return Err(format!("{canonical}\nreads back as another schema"));
            }
            Ok(true)
        }
        (Err(error), Err(refusal)) => {
            if error.path == refusal.path && same_class(&error.kind, &refusal.kind) {
                Ok(false)
            } else {
                Err(format!(
                    "{text}\nproduction {error:?}\nreference {refusal:?}"
                ))
            }
        }
        (production, reference) => Err(format!(
            "{text}\nproduction {:?}\nreference {:?}",
            production.map(|r| production_reading(&r)),
            reference
        )),
    }
}

#[test]
fn crd_reading_and_canonical_writing_match_the_reference_on_every_bounded_document() {
    let docs = documents();
    assert_eq!(docs.len(), document_count(18, 24));
    let mut accepted = 0;
    let mut failures = BTreeMap::new();
    for (i, doc) in docs.iter().enumerate() {
        match check(doc) {
            Ok(true) => accepted += 1,
            Ok(false) => {}
            Err(message) => {
                failures.insert(i, message);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreements: {failures:#?}",
        failures.len()
    );
    // 10 of the 18 schema nodes are accepted, so 1 + 2·10 + 2·10² = 221
    // property documents and 6 of the 24 envelopes are.
    assert_eq!(accepted, 227, "accepted documents of the scope");
}

/// A schema node of depth at most `depth`, with property names that need
/// escaping in vertex ids.
fn schema_node(depth: u32) -> BoxedStrategy<Doc> {
    let name = prop::sample::select(vec!["a", "b.c", "~x", "[y", "{z", "k"]);
    let leaf = prop::sample::select(vec!["string", "integer", "number", "boolean"])
        .prop_map(|t| Doc::Map(vec![("type".into(), Doc::Str(t.into()))]));
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
                    ("type".into(), Doc::Str("object".into())),
                    ("properties".into(), Doc::Map(props)),
                    ("required".into(), required),
                ])
            }),
            inner.clone().prop_map(|items| Doc::Map(vec![
                ("type".into(), Doc::Str("array".into())),
                ("items".into(), items),
            ])),
            inner.prop_map(|values| Doc::Map(vec![
                ("type".into(), Doc::Str("object".into())),
                ("additionalProperties".into(), values),
            ])),
        ]
    })
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_crds_read_and_write_as_the_reference(node in schema_node(3)) {
        let root = Doc::Map(vec![
            ("type".into(), Doc::Str("object".into())),
            ("properties".into(), Doc::Map(vec![("spec".into(), node)])),
        ]);
        let doc = Doc::Map(vec![
            ("apiVersion".into(), Doc::Str("apiextensions.k8s.io/v1".into())),
            ("kind".into(), Doc::Str("CustomResourceDefinition".into())),
            ("metadata".into(), Doc::Map(vec![("name".into(), Doc::Str("examples.test.com".into()))])),
            ("spec".into(), Doc::Map(vec![
                ("group".into(), Doc::Str("test.com".into())),
                ("names".into(), Doc::Map(vec![
                    ("plural".into(), Doc::Str("examples".into())),
                    ("kind".into(), Doc::Str("Example".into())),
                ])),
                ("scope".into(), Doc::Str("Cluster".into())),
                ("versions".into(), Doc::Seq(vec![Doc::Map(vec![
                    ("name".into(), Doc::Str("v1".into())),
                    ("served".into(), Doc::Bool(true)),
                    ("storage".into(), Doc::Bool(true)),
                    ("schema".into(), Doc::Map(vec![("openAPIV3Schema".into(), root)])),
                ])])),
            ])),
        ]);
        let outcome = check(&doc);
        prop_assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
    }
}
