//! Integration tests for the `class` and `instance` document surfaces.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use panproto_gat::{Equation, Implicit, Operation, Sort, SortExpr, Term, Theory};
use panproto_theory_dsl::{
    TheoryDslError, builtin_resolver, compile, document::TheoryBody, load, load_and_compile,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Build a small target theory that supplies `Int`, `Bool`, and matching
/// equality operations, used as the codomain for instance tests.
fn th_arith() -> Theory {
    let sorts = vec![Sort::simple("Int"), Sort::simple("Bool")];
    let equal_op = Operation::with_implicit(
        "int_eq",
        vec![
            (
                Arc::from("x"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
            (
                Arc::from("y"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
        ],
        SortExpr::Name(Arc::from("Bool")),
    );
    let not_equal_op = Operation::with_implicit(
        "int_neq",
        vec![
            (
                Arc::from("x"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
            (
                Arc::from("y"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
        ],
        SortExpr::Name(Arc::from("Bool")),
    );
    let sym = Equation::new(
        "int_eq_sym",
        Term::App {
            op: Arc::from("int_eq"),
            args: vec![Term::Var(Arc::from("x")), Term::Var(Arc::from("y"))],
        },
        Term::App {
            op: Arc::from("int_eq"),
            args: vec![Term::Var(Arc::from("y")), Term::Var(Arc::from("x"))],
        },
    );
    Theory::new("ThArith", sorts, vec![equal_op, not_equal_op], vec![sym])
}

/// Resolver that chains `ThArith` and the builtin resolver, plus an
/// externally supplied class theory.
fn resolver_with(extras: Vec<(String, Theory)>) -> impl Fn(&str) -> Option<Theory> {
    let builtin = builtin_resolver();
    move |name: &str| -> Option<Theory> {
        for (n, t) in &extras {
            if n == name {
                return Some(t.clone());
            }
        }
        if name == "ThArith" {
            return Some(th_arith());
        }
        builtin(name)
    }
}

#[test]
fn class_doc_compiles_to_theory() -> Result<(), Box<dyn std::error::Error>> {
    let path = fixture_path("th_eq_class.json");
    let resolver = resolver_with(vec![]);
    let compiled = load_and_compile(&path, &resolver)?;
    let th_eq = compiled
        .theories
        .get("ThEq")
        .expect("ThEq theory present in compiled set");

    assert_eq!(th_eq.sorts.len(), 2);
    assert!(th_eq.find_sort("A").is_some());
    assert!(th_eq.find_sort("Bool").is_some());
    assert_eq!(th_eq.ops.len(), 2);
    assert!(th_eq.find_op("eq").is_some());
    assert!(th_eq.find_op("neq").is_some());
    assert_eq!(th_eq.eqs.len(), 1);
    assert_eq!(&*th_eq.eqs[0].name, "sym");
    Ok(())
}

#[test]
fn instance_doc_compiles_and_checks() -> Result<(), Box<dyn std::error::Error>> {
    // Compile the class first so the instance resolver can find it.
    let class_doc = load(&fixture_path("th_eq_class.json"))?;
    let class_compiled = compile(&class_doc, &resolver_with(vec![]))?;
    let th_eq = class_compiled
        .theories
        .get("ThEq")
        .expect("ThEq theory present")
        .clone();

    let resolver = resolver_with(vec![("ThEq".to_owned(), th_eq)]);
    let inst_doc = load(&fixture_path("th_eq_instance.json"))?;
    let compiled = compile(&inst_doc, &resolver)?;

    let morph = compiled
        .morphisms
        .get("EqInt")
        .expect("EqInt morphism present");
    assert_eq!(&*morph.domain, "ThEq");
    assert_eq!(&*morph.codomain, "ThArith");
    assert_eq!(
        morph.sort_map.get(&Arc::from("A")).map(|s| &**s),
        Some("Int")
    );
    assert_eq!(
        morph.sort_map.get(&Arc::from("Bool")).map(|s| &**s),
        Some("Bool")
    );
    assert_eq!(
        morph
            .op_map
            .get(&Arc::from("eq"))
            .and_then(panproto_gat::OpAssignment::as_op)
            .map(|s| &**s),
        Some("int_eq")
    );
    assert_eq!(
        morph
            .op_map
            .get(&Arc::from("neq"))
            .and_then(panproto_gat::OpAssignment::as_op)
            .map(|s| &**s),
        Some("int_neq")
    );
    Ok(())
}

#[test]
fn instance_binding_missing_op_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Drop the `neq` binding; check_morphism should fail with
    // MissingOpMapping surfaced as MorphismCheck.
    let class_doc = load(&fixture_path("th_eq_class.json"))?;
    let class_compiled = compile(&class_doc, &resolver_with(vec![]))?;
    let th_eq = class_compiled
        .theories
        .get("ThEq")
        .expect("ThEq theory present")
        .clone();

    let resolver = resolver_with(vec![("ThEq".to_owned(), th_eq)]);
    let source = r#"{
        "id": "dev.panproto.test.instance_missing_op",
        "description": "Missing neq binding.",
        "instance": "EqIntPartial",
        "class": "ThEq",
        "target": "ThArith",
        "bindings": { "A": "Int", "Bool": "Bool", "eq": "int_eq" }
    }"#;
    let doc: panproto_theory_dsl::TheoryDocument = serde_json::from_str(source)?;
    let err = compile(&doc, &resolver).expect_err("should reject missing op binding");
    match err {
        TheoryDslError::MorphismCheck { ref morphism, .. } => {
            assert_eq!(morphism, "EqIntPartial");
        }
        other => panic!("expected MorphismCheck, got {other:?}"),
    }
    Ok(())
}

#[test]
fn instance_against_mismatched_target_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Map `eq` to a codomain op that does not exist; check_morphism should
    // fail, surfaced as MorphismCheck.
    let class_doc = load(&fixture_path("th_eq_class.json"))?;
    let class_compiled = compile(&class_doc, &resolver_with(vec![]))?;
    let th_eq = class_compiled
        .theories
        .get("ThEq")
        .expect("ThEq theory present")
        .clone();

    let resolver = resolver_with(vec![("ThEq".to_owned(), th_eq)]);
    let source = r#"{
        "id": "dev.panproto.test.instance_bad_target",
        "description": "eq binds to a nonexistent codomain op.",
        "instance": "EqIntBad",
        "class": "ThEq",
        "target": "ThArith",
        "bindings": {
            "A": "Int",
            "Bool": "Bool",
            "eq": "nope_eq",
            "neq": "int_neq"
        }
    }"#;
    let doc: panproto_theory_dsl::TheoryDocument = serde_json::from_str(source)?;
    let err = compile(&doc, &resolver).expect_err("should reject mismatched target op");
    assert!(
        matches!(err, TheoryDslError::MorphismCheck { .. }),
        "expected MorphismCheck, got {err:?}"
    );
    Ok(())
}

#[test]
fn instance_binding_to_unknown_name_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // A binding key that is neither a sort param nor an op of the class
    // surfaces as a specific InstanceBinding error.
    let class_doc = load(&fixture_path("th_eq_class.json"))?;
    let class_compiled = compile(&class_doc, &resolver_with(vec![]))?;
    let th_eq = class_compiled
        .theories
        .get("ThEq")
        .expect("ThEq theory present")
        .clone();

    let resolver = resolver_with(vec![("ThEq".to_owned(), th_eq)]);
    let source = r#"{
        "id": "dev.panproto.test.instance_bad_key",
        "description": "Binding has an unknown domain-side name.",
        "instance": "EqIntStray",
        "class": "ThEq",
        "target": "ThArith",
        "bindings": {
            "A": "Int",
            "Bool": "Bool",
            "eq": "int_eq",
            "neq": "int_neq",
            "stray": "whatever"
        }
    }"#;
    let doc: panproto_theory_dsl::TheoryDocument = serde_json::from_str(source)?;
    let err = compile(&doc, &resolver).expect_err("should reject unknown binding key");
    match err {
        TheoryDslError::InstanceBinding { instance, name, .. } => {
            assert_eq!(instance, "EqIntStray");
            assert_eq!(name, "stray");
        }
        other => panic!("expected InstanceBinding, got {other:?}"),
    }
    Ok(())
}

#[test]
fn class_body_roundtrips_through_document_enum() -> Result<(), Box<dyn std::error::Error>> {
    let doc = load(&fixture_path("th_eq_class.json"))?;
    assert!(matches!(doc.body, TheoryBody::Class(_)));
    Ok(())
}
