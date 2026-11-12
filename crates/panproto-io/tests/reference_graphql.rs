//! The GraphQL reader and writer against the graphql reference.
//!
//! Each document of the reference scope is rendered as GraphQL text and
//! read through production; the outcome (the refusal's schema coordinate
//! and rule, or the schema's vertices, ordered out-edges and constraints)
//! must equal the reference's, and each accepted document must write as the
//! reference's canonical text and read back as the same schema.

#![cfg(feature = "tree-sitter")]

use panproto_io::api::graphql::{GraphQlErrorKind, read_graphql, write_graphql};
use panproto_reference::enumerate::graphql::{CHOICES, accepted_count, choices, document};
use panproto_reference::pages::graphql::{Def, Reading, canonical_text, read, render, string};
use panproto_schema::Schema;
use proptest::prelude::*;

/// Production's schema in the reference's form.
fn production_reading(schema: &Schema) -> Reading {
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
        if constraints.is_empty() {
            continue;
        }
        out.constraints.insert(
            id.to_string(),
            constraints
                .iter()
                .map(|c| (c.sort.to_string(), c.value.clone()))
                .collect(),
        );
    }
    out
}

/// Whether production and the reference agree on one document, and
/// whether it was accepted; the message names the first difference.
fn agree(doc: &[Def]) -> Result<bool, String> {
    let text = render(doc);
    match (read_graphql(text.as_bytes()), read(doc)) {
        (Err(p), Err(r)) => {
            let rule = match &p.kind {
                GraphQlErrorKind::Invalid(rule) => format!("{rule:?}"),
                GraphQlErrorKind::NotRead(what) => format!("{what:?}"),
                other => format!("{other:?}"),
            };
            if (p.path.as_str(), rule.as_str()) == (r.path.as_str(), r.rule) {
                Ok(false)
            } else {
                Err(format!("{text}\nproduction {p}\nreference {r:?}"))
            }
        }
        (Ok(p), Ok(r)) => {
            if production_reading(&p.schema) != r {
                return Err(format!(
                    "{text}\nproduction {:#?}\nreference {r:#?}",
                    production_reading(&p.schema)
                ));
            }
            let written = write_graphql(&p.schema).map_err(|e| e.to_string())?;
            let expected = canonical_text(doc).unwrap_or_default();
            if written != expected {
                return Err(format!("{text}\nwrote {written}\nexpected {expected}"));
            }
            let again = read_graphql(written.as_bytes()).map_err(|e| format!("{written}\n{e}"))?;
            if production_reading(&again.schema) != r {
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
fn graphql_reading_and_canonical_writing_match_the_reference_on_every_bounded_document() {
    let mut accepted = 0;
    let mut total = 0;
    for choice in choices() {
        total += 1;
        match agree(&document(choice)) {
            Ok(true) => accepted += 1,
            Ok(false) => {}
            Err(message) => panic!("{choice:?}\n{message}"),
        }
    }
    assert_eq!((total, accepted), (36_864, accepted_count()));
}

/// A choice of each of the nine, with a description for `Query` drawn
/// from text the canonical writer can state.
fn generated(choice: [usize; 9], description: &str) -> Vec<Def> {
    let mut doc = document(choice);
    for def in &mut doc {
        if let Def::Type(t) = def
            && t.name == "Query"
        {
            t.desc = Some(description.to_owned());
        }
    }
    doc
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_graphql_documents_read_and_write_as_the_reference(
        picks in proptest::collection::vec(0usize..4, 9),
        description in "[a \"\\\\\n\t]{0,8}",
    ) {
        let mut choice = [0; 9];
        for (i, pick) in picks.iter().enumerate() {
            choice[i] = pick % CHOICES[i].0;
        }
        prop_assume!(string(&description).is_some());
        let doc = generated(choice, &description);
        if let Err(message) = agree(&doc) {
            prop_assert!(false, "{}", message);
        }
    }
}
