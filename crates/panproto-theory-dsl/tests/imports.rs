//! Integration tests for theory imports and namespacing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use panproto_gat::{Sort, Theory};
use panproto_theory_dsl::{TheoryDslError, compile, document::TheoryDocument};

fn th_x() -> Theory {
    let sorts = vec![Sort::simple("X")];
    Theory::new("ThX", sorts, vec![], vec![])
}

fn resolver_with(theories: Vec<(String, Theory)>) -> impl Fn(&str) -> Option<Theory> {
    move |name: &str| -> Option<Theory> {
        for (n, t) in &theories {
            if n == name {
                return Some(t.clone());
            }
        }
        None
    }
}

#[test]
fn imports_expose_names() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"{
        "id": "dev.panproto.test.imports_expose",
        "description": "Import ThX and expose X.",
        "theory": "ThB",
        "imports": [{ "from": "ThX", "expose": ["X"] }],
        "sorts": [{ "name": "Y", "params": [{ "name": "x", "sort": "X" }] }]
    }"#;
    let doc: TheoryDocument = serde_json::from_str(source)?;
    let resolver = resolver_with(vec![("ThX".to_owned(), th_x())]);
    let compiled = compile(&doc, &resolver)?;
    let th_b = compiled.theories.get("ThB").expect("ThB compiled");
    // X is exposed: it appears as a sort in the merged theory under
    // its original name.
    assert!(th_b.find_sort("X").is_some());
    // Y has a dependent parameter of sort X.
    let y = th_b.find_sort("Y").expect("Y present");
    assert_eq!(y.params.len(), 1);
    assert_eq!(&*y.params[0].sort.head().clone(), "X");
    Ok(())
}

#[test]
fn imports_with_alias() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"{
        "id": "dev.panproto.test.imports_alias",
        "description": "Import ThX with an alias G and reference G.X.",
        "theory": "ThB",
        "imports": [{ "from": "ThX", "alias": "G" }],
        "sorts": [{ "name": "Y", "params": [{ "name": "x", "sort": "G.X" }] }]
    }"#;
    let doc: TheoryDocument = serde_json::from_str(source)?;
    let resolver = resolver_with(vec![("ThX".to_owned(), th_x())]);
    let compiled = compile(&doc, &resolver)?;
    let th_b = compiled.theories.get("ThB").expect("ThB compiled");
    // The imported sort appears under its canonical aliased name
    // `G_X`.
    assert!(th_b.find_sort("G_X").is_some());
    let y = th_b.find_sort("Y").expect("Y present");
    assert_eq!(&*y.params[0].sort.head().clone(), "G_X");
    Ok(())
}

#[test]
fn imports_missing_theory_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"{
        "id": "dev.panproto.test.imports_missing",
        "description": "Import a nonexistent theory.",
        "theory": "ThB",
        "imports": [{ "from": "ThNope" }],
        "sorts": []
    }"#;
    let doc: TheoryDocument = serde_json::from_str(source)?;
    let resolver = resolver_with(vec![]);
    let err = compile(&doc, &resolver).expect_err("should reject bad import");
    match err {
        TheoryDslError::TheoryNotFound { ref name, .. } => {
            assert_eq!(name, "ThNope");
        }
        other => panic!("expected TheoryNotFound, got {other:?}"),
    }
    let _ = Arc::<str>::from("unused");
    Ok(())
}
