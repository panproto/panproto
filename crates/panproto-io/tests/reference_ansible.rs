//! The Ansible reader and writer against the ansible reference.
//!
//! Each document of the reference scope is rendered as block YAML and read
//! through production under its class; the outcome (the refusal's path and
//! class, or the schema's vertices, ordered out-edges and kept
//! constraints, and the external references) must equal the reference's,
//! and an accepted document's canonical text must equal the reference's
//! and read back as the same schema.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;

use panproto_io::config::ansible::{
    AnsibleErrorKind, AnsibleReading, FileClass, read_ansible, write_ansible,
};
use panproto_reference::enumerate::ansible::{base, document_count, documents};
use panproto_reference::pages::ansible::{
    Class, Doc, Kind, Reading, canonical_text, read, render_yaml,
};
use proptest::prelude::*;

const fn class_of(class: Class) -> FileClass {
    match class {
        Class::Playbook => FileClass::Playbook,
        Class::Tasks => FileClass::Tasks,
        Class::Vars => FileClass::Vars,
    }
}

/// Production's reading in the reference's form.
fn production_reading(reading: &AnsibleReading) -> Reading {
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
    out.external = reading
        .external
        .iter()
        .map(|e| (e.path.clone(), e.kind, e.name.clone()))
        .collect();
    out
}

/// Whether production's refusal class is the reference's.
fn same_class(production: &AnsibleErrorKind, reference: &Kind) -> bool {
    match (production, reference) {
        (AnsibleErrorKind::Invalid(_), Kind::Invalid) => true,
        (AnsibleErrorKind::Missing(a), Kind::Missing(b))
        | (AnsibleErrorKind::WrongType(a), Kind::WrongType(b))
        | (AnsibleErrorKind::Unsupported(a), Kind::Unsupported(b)) => a == b,
        (AnsibleErrorKind::UnknownField(a), Kind::UnknownField(b))
        | (AnsibleErrorKind::VariableName(a), Kind::VariableName(b)) => a == b,
        _ => false,
    }
}

/// Compare production with the reference on one document, and on an
/// accepted one compare the canonical text and its reading back. Returns
/// whether the document was accepted.
fn check(doc: &Doc, class: Class) -> Result<bool, String> {
    let text = render_yaml(doc);
    let file_class = class_of(class);
    match (read_ansible(text.as_bytes(), file_class), read(doc, class)) {
        (Ok(reading), Ok(reference)) => {
            let got = production_reading(&reading);
            if got != reference {
                return Err(format!(
                    "{text}\nproduction {got:#?}\nreference {reference:#?}"
                ));
            }
            let canonical = write_ansible(&reading.schema).map_err(|e| format!("{text}\n{e}"))?;
            if canonical != canonical_text(doc, class) {
                return Err(format!(
                    "{text}\nproduction {canonical}\nreference {}",
                    canonical_text(doc, class)
                ));
            }
            let again = read_ansible(canonical.as_bytes(), file_class)
                .map_err(|e| format!("{canonical}\n{e}"))?;
            // The canonical text may spell a reference another way (a role
            // named by a string is written as a mapping), so the external
            // references' paths are not compared; the schema is.
            let mut back = production_reading(&again);
            back.external.clone_from(&got.external);
            if back != got {
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
fn ansible_reading_and_canonical_writing_match_the_reference_on_every_bounded_document() {
    let docs = documents();
    assert_eq!(docs.len(), document_count(17, 23));
    let mut accepted = 0;
    let mut failures = BTreeMap::new();
    for (i, (doc, class)) in docs.iter().enumerate() {
        match check(doc, *class) {
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
    // 9 of the 17 tasks are accepted, so 1 + 9 + 9² = 91 playbooks, and 9
    // of the 23 envelopes are.
    assert_eq!(accepted, 100, "accepted documents of the scope");
}

/// A task of depth at most `depth`: a module call with a notify, a loop or
/// variables, or a block of such tasks.
fn task(depth: u32) -> BoxedStrategy<Doc> {
    let module = prop::sample::select(vec![
        "ping",
        "copy",
        "include_tasks",
        "ansible.builtin.debug",
    ]);
    let notify = prop::sample::select(vec!["restart", "h", "missing"]);
    let var = prop::sample::select(vec!["x", "y", "not", "_z"]);
    let leaf = (module, notify, var, any::<bool>()).prop_map(|(m, n, v, looped)| {
        let mut entries = vec![
            (m.to_owned(), Doc::Str("a".into())),
            ("notify".to_owned(), Doc::Str(n.into())),
            (
                "vars".to_owned(),
                Doc::Map(vec![(v.to_owned(), Doc::Int(1))]),
            ),
        ];
        if looped {
            entries.push(("with_items".to_owned(), Doc::Seq(vec![Doc::Int(1)])));
        }
        Doc::Map(entries)
    });
    leaf.prop_recursive(depth, 12, 3, |inner| {
        (
            prop::collection::vec(inner.clone(), 1..3),
            prop::collection::vec(inner, 0..2),
        )
            .prop_map(|(block, rescue)| {
                let mut entries = vec![("block".to_owned(), Doc::Seq(block))];
                if !rescue.is_empty() {
                    entries.push(("rescue".to_owned(), Doc::Seq(rescue)));
                }
                Doc::Map(entries)
            })
    })
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_playbooks_read_and_write_as_the_reference(
        tasks in prop::collection::vec(task(2), 0..3),
    ) {
        let outcome = check(&base(tasks), Class::Playbook);
        prop_assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
    }
}
