//! The `AsyncAPI` reader and writer against the asyncapi reference.
//!
//! Each document of the reference scope is rendered as JSON text and read
//! through production; the outcome (the refusal's document, JSON pointer
//! and class, or the schema's vertices, ordered out-edges, kept
//! constraints, required properties, entries and roots) must equal the
//! reference's, and each accepted document must write as the reference's
//! canonical text and read back as the same schema.

#![cfg(feature = "tree-sitter")]

use std::collections::BTreeSet;

use panproto_io::api::asyncapi::{AsyncApiReading, read_asyncapi, write_asyncapi};
use panproto_reference::enumerate::asyncapi::{
    accepted_count, additions, channels, document, documents, messages, operations, versions,
};
use panproto_reference::pages::asyncapi::{canonical_text, read};
use panproto_reference::pages::json_schema::{Doc, Reading, render};
use proptest::prelude::*;

/// Production's reading in the reference's form.
fn production_reading(reading: &AsyncApiReading) -> Reading {
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

/// Whether production and the reference agree on one document, and
/// whether it was accepted; the message names the first difference.
fn agree(doc: &Doc) -> Result<bool, String> {
    let text = render(doc);
    let production = read_asyncapi(text.as_bytes());
    match (production, read(&[(None, doc.clone(), false)])) {
        (Err(p), Err(r)) => {
            let class = format!("{:?}", p.kind);
            let class = class.split(['(', ' ']).next().unwrap_or_default();
            if (p.document, p.path.as_str(), class) == (r.document, r.path.as_str(), r.class) {
                Ok(false)
            } else {
                Err(format!("{text}\nproduction {p}\nreference {r:?}"))
            }
        }
        (Ok(p), Ok(r)) => {
            if production_reading(&p) != r {
                return Err(format!(
                    "{text}\nproduction {:#?}\nreference {r:#?}",
                    production_reading(&p)
                ));
            }
            let written = write_asyncapi(&p.schema, &p.roots[0]).map_err(|e| e.to_string())?;
            let expected = canonical_text(doc);
            if written != expected {
                return Err(format!("{text}\nwrote {written}\nexpected {expected}"));
            }
            let again = read_asyncapi(written.as_bytes()).map_err(|e| e.to_string())?;
            if production_reading(&again) != r {
                return Err(format!("{written}\nreads back as another schema"));
            }
            Ok(true)
        }
        (p, r) => Err(format!(
            "{text}\nproduction {}\nreference {}",
            p.map_or_else(|e| e.to_string(), |_| "reads".to_owned()),
            r.map_or_else(|e| format!("{e:?}"), |_| "reads".to_owned())
        )),
    }
}

#[test]
fn asyncapi_reading_and_canonical_writing_match_the_reference_on_every_bounded_document() {
    let mut accepted = 0;
    for doc in documents() {
        match agree(&doc) {
            Ok(true) => accepted += 1,
            Ok(false) => {}
            Err(message) => panic!("{message}"),
        }
    }
    assert_eq!(accepted, accepted_count());
}

/// Further channels a generated document adds, each named by its index in
/// [`NAMES`] and drawn from the scope's channel forms.
const NAMES: [&str; 4] = ["e", "f/g", "h~i", "j"];

fn generated(choice: (usize, usize, usize, usize, usize), extra: &[(usize, usize)]) -> Doc {
    let (version, channel, operation, message, addition) = choice;
    let mut doc = document(
        &versions()[version],
        &channels()[channel],
        &operations()[operation],
        &messages()[message],
        additions()[addition].as_ref(),
    );
    if let Doc::Map(members) = &mut doc
        && let Some((_, Doc::Map(found))) = members.iter_mut().find(|(k, _)| k == "channels")
    {
        for (name, form) in extra {
            let name = NAMES[*name].to_owned();
            if found.iter().any(|(k, _)| *k == name) {
                continue;
            }
            found.push((name, channels()[*form].clone()));
        }
    }
    doc
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_asyncapi_documents_read_and_write_as_the_reference(
        choice in (0usize..4, 0usize..6, 0usize..6, 0usize..7, 0usize..5),
        extra in proptest::collection::vec((0usize..4, 0usize..6), 0..4),
    ) {
        let doc = generated(choice, &extra);
        if let Err(message) = agree(&doc) {
            prop_assert!(false, "{}", message);
        }
    }
}
