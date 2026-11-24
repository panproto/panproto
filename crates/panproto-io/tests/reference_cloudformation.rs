//! The template reader and writer against the cloudformation reference.
//!
//! Each document of the reference scope is rendered both as block YAML and
//! as compact JSON and read through production, so both shared routes are
//! exercised; the outcome (the refusal's path and class, or the schema's
//! vertices, ordered out-edges and kept constraints, and the imports) must
//! equal the reference's, and an accepted document's canonical text must
//! equal the reference's and read back as the same schema.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;

use panproto_io::config::cloudformation::{
    CfnErrorKind, CfnReading, read_template, write_template,
};
use panproto_reference::enumerate::cloudformation::{base, document_count, documents};
use panproto_reference::pages::cloudformation::{
    Doc, Kind, Reading, canonical_text, json, read, render_yaml,
};
use proptest::prelude::*;

/// Production's reading in the reference's form.
fn production_reading(reading: &CfnReading) -> Reading {
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
    out.imports = reading
        .imports
        .iter()
        .map(|i| (i.path.clone(), i.name.clone()))
        .collect();
    out
}

/// Whether production's refusal class is the reference's.
fn same_class(production: &CfnErrorKind, reference: &Kind) -> bool {
    match (production, reference) {
        (CfnErrorKind::FormatVersion(_), Kind::FormatVersion)
        | (CfnErrorKind::Invalid(_), Kind::Invalid) => true,
        (CfnErrorKind::Missing(a), Kind::Missing(b))
        | (CfnErrorKind::WrongType(a), Kind::WrongType(b))
        | (CfnErrorKind::Unsupported(a), Kind::Unsupported(b)) => a == b,
        (CfnErrorKind::UnknownField(a), Kind::UnknownField(b))
        | (CfnErrorKind::UnknownFunction(a), Kind::UnknownFunction(b))
        | (CfnErrorKind::Arguments(a), Kind::Arguments(b))
        | (CfnErrorKind::DuplicateName(a), Kind::DuplicateName(b))
        | (CfnErrorKind::Unresolved(a), Kind::Unresolved(b)) => a == b,
        _ => false,
    }
}

/// Compare production on `text` with the reference on `doc`, and on an
/// accepted one compare the canonical text and its reading back. Returns
/// whether the document was accepted.
fn check_text(doc: &Doc, text: &str) -> Result<bool, String> {
    match (read_template(text.as_bytes()), read(doc)) {
        (Ok(reading), Ok(reference)) => {
            let got = production_reading(&reading);
            if got != reference {
                return Err(format!(
                    "{text}\nproduction {got:#?}\nreference {reference:#?}"
                ));
            }
            let canonical = write_template(&reading.schema).map_err(|e| format!("{text}\n{e}"))?;
            if canonical != canonical_text(doc) {
                return Err(format!(
                    "{text}\nproduction {canonical}\nreference {}",
                    canonical_text(doc)
                ));
            }
            let again =
                read_template(canonical.as_bytes()).map_err(|e| format!("{canonical}\n{e}"))?;
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

/// Check `doc` through the YAML route and through the JSON route.
fn check(doc: &Doc) -> Result<bool, String> {
    let yaml = check_text(doc, &render_yaml(doc))?;
    let json_route = check_text(doc, &json(doc))?;
    if yaml == json_route {
        Ok(yaml)
    } else {
        Err(format!("{}\nthe two routes disagree", json(doc)))
    }
}

#[test]
fn template_reading_and_canonical_writing_match_the_reference_on_every_bounded_document() {
    let docs = documents();
    assert_eq!(docs.len(), document_count(17, 26));
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
    // 11 of the 17 values are accepted, so 1 + 2·11 + 2·11² = 265 property
    // documents and 4 of the 26 envelopes are.
    assert_eq!(accepted, 269, "accepted documents of the scope");
}

/// A property value of depth at most `depth`, built from the intrinsic
/// functions of the scope, literals, mappings and sequences.
fn value(depth: u32) -> BoxedStrategy<Doc> {
    let name = prop::sample::select(vec!["P", "R", "S", "M", "C", "Z", "AWS::Region"]);
    let leaf = prop_oneof![
        prop::sample::select(vec!["x", "${P}", "${R.Arn}", "${Z}"])
            .prop_map(|t| Doc::Str(t.into())),
        name.clone()
            .prop_map(|n| Doc::Map(vec![("Ref".into(), Doc::Str(n.into()))])),
        name.clone().prop_map(|n| Doc::Map(vec![(
            "Fn::GetAtt".into(),
            Doc::Seq(vec![Doc::Str(n.into()), Doc::Str("Arn".into())])
        )])),
        name.clone()
            .prop_map(|n| Doc::Map(vec![("Fn::Sub".into(), Doc::Str(format!("a-${{{n}}}")))])),
        name.prop_map(|n| Doc::Map(vec![(
            "Fn::FindInMap".into(),
            Doc::Seq(vec![
                Doc::Str(n.into()),
                Doc::Str("k".into()),
                Doc::Str("v".into())
            ])
        )])),
    ];
    leaf.prop_recursive(depth, 16, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(Doc::Seq),
            prop::collection::vec(
                (prop::sample::select(vec!["k1", "k2", "k3"]), inner.clone()),
                0..3
            )
            .prop_map(|entries| {
                let mut seen = Vec::new();
                Doc::Map(
                    entries
                        .into_iter()
                        .filter(|(k, _)| {
                            let fresh = !seen.contains(k);
                            seen.push(*k);
                            fresh
                        })
                        .map(|(k, v)| (k.to_owned(), v))
                        .collect(),
                )
            }),
            (
                prop::sample::select(vec!["C", "Z"]),
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(c, a, b)| Doc::Map(vec![(
                    "Fn::If".into(),
                    Doc::Seq(vec![Doc::Str(c.into()), a, b])
                )])),
            inner.prop_map(|v| Doc::Map(vec![(
                "Fn::Join".into(),
                Doc::Seq(vec![Doc::Str("-".into()), Doc::Seq(vec![v])])
            )])),
        ]
    })
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_templates_read_and_write_as_the_reference(a in value(3), b in value(2)) {
        let doc = base(&[("a", a), ("b", b)]);
        let outcome = check(&doc);
        prop_assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
    }
}
