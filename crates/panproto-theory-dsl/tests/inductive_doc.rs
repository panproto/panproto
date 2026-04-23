//! Integration tests for the `inductive` document surface.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use panproto_gat::SortClosure;
use panproto_theory_dsl::{builtin_resolver, load_and_compile};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn nat_inductive_compiles_to_closed_sort() -> Result<(), Box<dyn std::error::Error>> {
    let compiled = load_and_compile(&fixture_path("nat_inductive.json"), &builtin_resolver())?;
    let nat = compiled
        .theories
        .get("Nat")
        .expect("Nat theory present in compiled set");
    let sort = nat.find_sort("Nat").expect("Nat sort present");
    match &sort.closure {
        SortClosure::Closed(ctors) => {
            let names: Vec<&str> = ctors.iter().map(|c| &**c).collect();
            assert_eq!(names, vec!["zero", "succ"]);
        }
        SortClosure::Open => panic!("Nat sort should be closed"),
    }
    assert!(nat.find_op("zero").is_some());
    assert!(nat.find_op("succ").is_some());
    Ok(())
}

#[test]
fn list_inductive_closed_with_two_ctors() -> Result<(), Box<dyn std::error::Error>> {
    let compiled = load_and_compile(&fixture_path("list_inductive.json"), &builtin_resolver())?;
    let list = compiled
        .theories
        .get("List")
        .expect("List theory present in compiled set");
    let sort = list.find_sort("List").expect("List sort present");
    match &sort.closure {
        SortClosure::Closed(ctors) => {
            let names: Vec<&str> = ctors.iter().map(|c| &**c).collect();
            assert_eq!(names, vec!["nil", "cons"]);
        }
        SortClosure::Open => panic!("List sort should be closed"),
    }
    // The inductive sort should carry its parameter `A` as a sort param.
    assert_eq!(sort.params.len(), 1);
    assert_eq!(&*sort.params[0].name, "A");
    // An auto-declared `A` sort accompanies the inductive.
    assert!(list.find_sort("A").is_some());
    assert!(list.find_op("nil").is_some());
    assert!(list.find_op("cons").is_some());
    Ok(())
}
