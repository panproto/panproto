//! Diffing and classifying the keys and references two schemas declare.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_check::classify::{BreakingChange, NonBreakingChange, classify};
use panproto_check::diff::{KeyChange, ReferenceChange, SchemaDiff, diff};
use panproto_gat::Name;
use panproto_schema::{
    Cardinality, Edge, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, Protocol, RefSource,
    Reference, ResolveMode, Schema, SchemaBuilder,
};

fn protocol() -> Protocol {
    Protocol {
        name: "keys".to_owned(),
        ..Default::default()
    }
}

fn column(table: &str, name: &str) -> Edge {
    Edge {
        src: Name::from(table),
        tgt: Name::from(format!("{table}.{name}")),
        kind: Name::from("column"),
        name: Some(Name::from(name)),
    }
}

/// A `film` table and a `store` table whose `film_id` column names a film,
/// with no declarations.
fn schema() -> Schema {
    SchemaBuilder::new(&protocol())
        .vertex("doc", "object", None)
        .expect("doc")
        .vertex("film", "table", None)
        .expect("film")
        .vertex("film.id", "integer", None)
        .expect("film.id")
        .vertex("store", "table", None)
        .expect("store")
        .vertex("store.film_id", "integer", None)
        .expect("store.film_id")
        .edge("doc", "film", "item", Some("films"))
        .expect("e")
        .edge("film", "film.id", "column", Some("id"))
        .expect("e")
        .edge("store", "store.film_id", "column", Some("film_id"))
        .expect("e")
        .build()
        .expect("build")
}

fn film_key(role: KeyRole, scope: KeyScope) -> Key {
    Key {
        id: Name::from("pk"),
        parts: vec![KeyPart::Path(vec![column("film", "id")])],
        role,
        scope,
        equality: vec![],
    }
}

fn store_film(cardinality: Cardinality, nullable: bool, on_unresolved: OnUnresolved) -> Reference {
    Reference {
        id: Name::from("store_film"),
        holder: Name::from("store"),
        source: RefSource::KeyValue {
            parts: vec![vec![column("store", "film_id")]],
        },
        target_vertex: Name::from("film"),
        target_key: Some(Name::from("pk")),
        edge: None,
        cardinality,
        nullable,
        on_unresolved,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: Default::default(),
        null_vertex: None,
        scope_via: None,
    }
}

fn with(keys: &[Key], references: &[Reference]) -> Schema {
    let mut schema = schema();
    for key in keys {
        schema
            .declarations
            .insert_key(Name::from("film"), key.clone());
    }
    for reference in references {
        schema.declarations.insert_reference(reference.clone());
    }
    schema
}

#[test]
fn schema_diff_reports_key_and_reference_changes() {
    let primary = film_key(KeyRole::Primary, KeyScope::Document);
    let unique = film_key(KeyRole::Unique, KeyScope::Document);
    let one = store_film(Cardinality::One, false, OnUnresolved::Error);
    let many = store_film(Cardinality::Many, false, OnUnresolved::Error);

    let bare = with(&[], &[]);
    let keyed = with(std::slice::from_ref(&primary), std::slice::from_ref(&one));

    // Nothing declared on either side: nothing to report.
    assert!(diff(&bare, &bare).is_empty());
    assert!(diff(&keyed, &keyed).is_empty());

    let added = diff(&bare, &keyed);
    assert_eq!(added.added_keys, vec![("film".to_owned(), primary.clone())]);
    assert_eq!(added.added_references, vec![one.clone()]);
    assert!(!added.is_empty());
    assert_eq!(
        added,
        SchemaDiff {
            added_keys: vec![("film".to_owned(), primary.clone())],
            added_references: vec![one.clone()],
            ..SchemaDiff::default()
        }
    );

    let removed = diff(&keyed, &bare);
    assert_eq!(
        removed.removed_keys,
        vec![("film".to_owned(), primary.clone())]
    );
    assert_eq!(removed.removed_references, vec![one.clone()]);

    let changed = diff(
        &keyed,
        &with(std::slice::from_ref(&unique), std::slice::from_ref(&many)),
    );
    assert_eq!(
        changed.modified_keys,
        vec![KeyChange {
            vertex_id: "film".to_owned(),
            old: primary,
            new: unique,
        }]
    );
    assert_eq!(
        changed.modified_references,
        vec![ReferenceChange {
            id: "store_film".to_owned(),
            old: one,
            new: many,
        }]
    );
    assert!(changed.added_keys.is_empty() && changed.removed_keys.is_empty());

    // An empty declaration diff leaves the serialized diff unchanged.
    let json = serde_json::to_string(&diff(&bare, &bare)).expect("json");
    assert!(!json.contains("keys") && !json.contains("references"));
}

fn verdict(old: &Schema, new: &Schema) -> (Vec<BreakingChange>, Vec<NonBreakingChange>) {
    let report = classify(&diff(old, new), &protocol());
    (report.breaking, report.non_breaking)
}

#[test]
fn key_changes_are_classified() {
    let key_added = || BreakingChange::KeyAdded {
        vertex_id: "film".to_owned(),
        key_id: "pk".to_owned(),
    };
    let key_removed = || NonBreakingChange::KeyRemoved {
        vertex_id: "film".to_owned(),
        key_id: "pk".to_owned(),
    };
    let key_added_nonbreaking = || NonBreakingChange::KeyAdded {
        vertex_id: "film".to_owned(),
        key_id: "pk".to_owned(),
    };
    let bare = with(&[], &[]);

    // Adding a primary, unique or identifier key is breaking; removing one
    // is not.
    for role in [KeyRole::Primary, KeyRole::Unique, KeyRole::Identifier] {
        let keyed = with(&[film_key(role, KeyScope::Document)], &[]);
        assert_eq!(verdict(&bare, &keyed), (vec![key_added()], vec![]));
        assert_eq!(verdict(&keyed, &bare), (vec![], vec![key_removed()]));
    }

    // Sort keys and indexes do not impose an instance-admission obligation.
    // Adding or removing either role is therefore nonbreaking.
    for role in [KeyRole::SortKey, KeyRole::Index] {
        let keyed = with(&[film_key(role, KeyScope::Document)], &[]);
        assert_eq!(
            verdict(&bare, &keyed),
            (vec![], vec![key_added_nonbreaking()])
        );
        assert_eq!(verdict(&keyed, &bare), (vec![], vec![key_removed()]));
    }

    // A key on a vertex the new schema adds constrains no existing data.
    let mut grown = with(&[], &[]);
    grown.vertices.insert(
        Name::from("actor"),
        panproto_schema::Vertex {
            id: Name::from("actor"),
            kind: Name::from("table"),
            nsid: None,
        },
    );
    grown.declarations.insert_key(
        Name::from("actor"),
        Key {
            id: Name::from("pk"),
            parts: vec![],
            role: KeyRole::Primary,
            scope: KeyScope::Document,
            equality: vec![],
        },
    );
    let (breaking, non_breaking) = verdict(&bare, &grown);
    assert_eq!(breaking, vec![]);
    assert!(non_breaking.contains(&NonBreakingChange::KeyAdded {
        vertex_id: "actor".to_owned(),
        key_id: "pk".to_owned(),
    }));

    // Every role-to-role change fails closed in both directions because the
    // roles impose different uniqueness, ordering, identity, and resolution
    // contracts.
    let tightened = || BreakingChange::KeyTightened {
        vertex_id: "film".to_owned(),
        key_id: "pk".to_owned(),
    };
    let relaxed = || NonBreakingChange::KeyRelaxed {
        vertex_id: "film".to_owned(),
        key_id: "pk".to_owned(),
    };
    let roles = [
        KeyRole::Primary,
        KeyRole::Unique,
        KeyRole::Identifier,
        KeyRole::SortKey,
        KeyRole::Index,
    ];
    for old_role in &roles {
        for new_role in &roles {
            if old_role == new_role {
                continue;
            }
            let old = with(&[film_key(old_role.clone(), KeyScope::Document)], &[]);
            let new = with(&[film_key(new_role.clone(), KeyScope::Document)], &[]);
            assert_eq!(verdict(&old, &new), (vec![tightened()], vec![]));
        }
    }

    // Widening a scope is breaking, and narrowing one is a relaxation.
    let pairs = [
        (
            film_key(KeyRole::Unique, KeyScope::Document),
            film_key(KeyRole::Unique, KeyScope::Global),
        ),
        (
            film_key(KeyRole::Unique, KeyScope::Within(Name::from("doc"))),
            film_key(KeyRole::Unique, KeyScope::Document),
        ),
    ];
    for (looser, stricter) in pairs {
        let looser = with(&[looser], &[]);
        let stricter = with(&[stricter], &[]);
        assert_eq!(verdict(&looser, &stricter), (vec![tightened()], vec![]));
        assert_eq!(verdict(&stricter, &looser), (vec![], vec![relaxed()]));
    }

    // Changing the part sequence may reject values in either direction.
    let mut reparted = film_key(KeyRole::Unique, KeyScope::Document);
    reparted.parts.clear();
    let unique = with(&[film_key(KeyRole::Unique, KeyScope::Document)], &[]);
    assert_eq!(
        verdict(&unique, &with(&[reparted], &[])),
        (vec![tightened()], vec![])
    );
}

#[test]
fn reference_changes_are_classified() {
    let key = [film_key(KeyRole::Primary, KeyScope::Document)];
    let bare = with(&key, &[]);
    let id = || "store_film".to_owned();

    // Adding a non-nullable reference is breaking; removing it is not.
    let strict = with(
        &key,
        &[store_film(Cardinality::One, false, OnUnresolved::Error)],
    );
    assert_eq!(
        verdict(&bare, &strict),
        (
            vec![BreakingChange::ReferenceAdded { reference_id: id() }],
            vec![]
        )
    );
    assert_eq!(
        verdict(&strict, &bare),
        (
            vec![],
            vec![NonBreakingChange::ReferenceRemoved { reference_id: id() }]
        )
    );

    // A nullable reference that fails on a dangling value still rejects
    // data; one that keeps dangling values in the complement does not.
    let nullable_strict = with(
        &key,
        &[store_film(Cardinality::One, true, OnUnresolved::Error)],
    );
    assert_eq!(
        verdict(&bare, &nullable_strict),
        (
            vec![BreakingChange::ReferenceAdded { reference_id: id() }],
            vec![]
        )
    );
    let lenient = with(
        &key,
        &[store_film(Cardinality::One, true, OnUnresolved::Complement)],
    );
    assert_eq!(
        verdict(&bare, &lenient),
        (
            vec![],
            vec![NonBreakingChange::ReferenceAdded { reference_id: id() }]
        )
    );

    // Tightening Many to One, nullable to required, or complement to error
    // is breaking; each inverse is a relaxation.
    let tightened = vec![BreakingChange::ReferenceTightened { reference_id: id() }];
    let relaxed = vec![NonBreakingChange::ReferenceRelaxed { reference_id: id() }];
    let pairs = [
        (
            store_film(Cardinality::Many, false, OnUnresolved::Error),
            store_film(Cardinality::One, false, OnUnresolved::Error),
        ),
        (
            store_film(Cardinality::One, true, OnUnresolved::Error),
            store_film(Cardinality::One, false, OnUnresolved::Error),
        ),
        (
            store_film(Cardinality::One, false, OnUnresolved::Complement),
            store_film(Cardinality::One, false, OnUnresolved::Error),
        ),
    ];
    for (looser, stricter) in pairs {
        let looser = with(&key, &[looser]);
        let stricter = with(&key, &[stricter]);
        assert_eq!(verdict(&looser, &stricter), (tightened.clone(), vec![]));
        assert_eq!(verdict(&stricter, &looser), (vec![], relaxed.clone()));
    }

    // Retargeting a reference is breaking in both directions.
    let mut retargeted = store_film(Cardinality::One, false, OnUnresolved::Error);
    retargeted.mode = ResolveMode::Redirect;
    let retargeted = with(&key, &[retargeted]);
    assert_eq!(verdict(&strict, &retargeted), (tightened.clone(), vec![]));
    assert_eq!(verdict(&retargeted, &strict), (tightened, vec![]));
}

#[test]
fn declaration_changes_render_in_text_and_json_reports() {
    let key = [film_key(KeyRole::Primary, KeyScope::Document)];
    let bare = with(&[], &[]);
    let keyed = with(
        &key,
        &[store_film(Cardinality::One, false, OnUnresolved::Error)],
    );
    let report = classify(&diff(&bare, &keyed), &protocol());
    let text = panproto_check::report::report_text(&report);
    assert!(text.contains("Key added: film pk"), "{text}");
    assert!(text.contains("Reference added: store_film"), "{text}");
    // Compared as values, not as text: the order a JSON object renders its
    // keys in depends on whether serde_json's `preserve_order` feature is
    // unified into the build, which nothing in this crate controls.
    let json = panproto_check::report::report_json(&report);
    let breaking = json["breaking"].as_array().expect("breaking array");
    assert!(
        breaking.contains(&serde_json::json!(
            {"type": "key_added", "vertex_id": "film", "key_id": "pk"}
        )),
        "{json}"
    );
    assert!(
        breaking.contains(&serde_json::json!(
            {"type": "reference_added", "reference_id": "store_film"}
        )),
        "{json}"
    );

    let back = classify(&diff(&keyed, &bare), &protocol());
    let text = panproto_check::report::report_text(&back);
    assert!(text.contains("Key removed: film pk"), "{text}");
    assert!(text.contains("Reference removed: store_film"), "{text}");
}
