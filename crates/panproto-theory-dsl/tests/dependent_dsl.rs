//! DSL-level end-to-end tests for dependent-sort theories.
//!
//! Loads fixtures from `tests/fixtures/`, compiles them through the
//! theory DSL pipeline, typechecks the results, and exercises the sort
//! expression parser for applied forms like `Tm(Ctx, A)`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use panproto_theory_dsl::{builtin_resolver, load_and_compile};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn load_and_compile_stlc_json_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let path = fixture_path("stlc.json");
    let resolver = builtin_resolver();
    let compiled = load_and_compile(&path, &resolver)?;
    let stlc = compiled
        .theories
        .get("STLC")
        .expect("STLC theory present in compiled set");

    let tm_sort = stlc
        .sorts
        .iter()
        .find(|s| &*s.name == "Tm")
        .expect("Tm sort present");
    assert_eq!(tm_sort.params.len(), 2);

    let lam = stlc
        .ops
        .iter()
        .find(|o| &*o.name == "lam")
        .expect("lam op present");
    assert_eq!(lam.inputs.len(), 4);
    assert_eq!(&**lam.output.head(), "Tm");
    assert_eq!(lam.output.args().len(), 2);

    panproto_gat::typecheck_theory(stlc)?;
    Ok(())
}

#[test]
fn stlc_round_trip_through_json_preserves_structure() -> Result<(), Box<dyn std::error::Error>> {
    let path = fixture_path("stlc.json");
    let resolver = builtin_resolver();
    let compiled = load_and_compile(&path, &resolver)?;
    let stlc = compiled.theories.get("STLC").expect("STLC present");

    let json = serde_json::to_string(stlc)?;
    let back: panproto_gat::Theory = serde_json::from_str(&json)?;
    assert_eq!(stlc.sorts.len(), back.sorts.len());
    assert_eq!(stlc.ops.len(), back.ops.len());
    panproto_gat::typecheck_theory(&back)?;
    Ok(())
}

#[test]
fn parse_applied_sort_expr_produces_app_variant() {
    use panproto_gat::SortExpr;
    use panproto_theory_dsl::document::{OpSpec, ParamSpec};

    let spec = OpSpec {
        name: "id".to_owned(),
        input: None,
        inputs: Some(vec![ParamSpec {
            name: "x".to_owned(),
            sort: "Ob".to_owned(),
            implicit: false,
        }]),
        output: "Hom(x, x)".to_owned(),
    };
    let theory_spec = panproto_theory_dsl::document::TheorySpec {
        theory: "Cat".to_owned(),
        extends: vec![],
        imports: vec![],
        sorts: vec![
            panproto_theory_dsl::document::SortSpec {
                name: "Ob".to_owned(),
                params: vec![],
                kind: panproto_theory_dsl::document::SortKindSpec::Structural,
                closed: None,
            },
            panproto_theory_dsl::document::SortSpec {
                name: "Hom".to_owned(),
                params: vec![
                    ParamSpec {
                        name: "a".to_owned(),
                        sort: "Ob".to_owned(),
                        implicit: false,
                    },
                    ParamSpec {
                        name: "b".to_owned(),
                        sort: "Ob".to_owned(),
                        implicit: false,
                    },
                ],
                kind: panproto_theory_dsl::document::SortKindSpec::Structural,
                closed: None,
            },
        ],
        ops: vec![spec],
        equations: vec![],
        directed_equations: vec![],
        policies: vec![],
    };
    let doc = panproto_theory_dsl::document::TheoryDocument {
        id: "test".to_owned(),
        description: String::new(),
        body: panproto_theory_dsl::document::TheoryBody::Theory(theory_spec),
    };
    let resolver = builtin_resolver();
    let compiled = panproto_theory_dsl::compile(&doc, &resolver).expect("compile ok");
    let cat = compiled.theories.get("Cat").expect("Cat present");
    let id_op = cat.ops.iter().find(|o| &*o.name == "id").expect("id op");
    assert!(matches!(&id_op.output, SortExpr::App { .. }));
    assert_eq!(id_op.output.args().len(), 2);
}

#[test]
fn compile_rejects_dsl_theory_with_ill_typed_equation() {
    use panproto_theory_dsl::document::{
        EquationSpec, OpSpec, SortKindSpec, SortSpec, TheoryBody, TheoryDocument, TheorySpec,
    };
    let spec = TheorySpec {
        theory: "Bad".to_owned(),
        extends: vec![],
        imports: vec![],
        sorts: vec![
            SortSpec {
                name: "A".to_owned(),
                params: vec![],
                kind: SortKindSpec::Structural,
                closed: None,
            },
            SortSpec {
                name: "B".to_owned(),
                params: vec![],
                kind: SortKindSpec::Structural,
                closed: None,
            },
        ],
        ops: vec![
            OpSpec {
                name: "f".to_owned(),
                input: Some("A".to_owned()),
                inputs: None,
                output: "B".to_owned(),
            },
            OpSpec {
                name: "g".to_owned(),
                input: Some("A".to_owned()),
                inputs: None,
                output: "A".to_owned(),
            },
        ],
        equations: vec![EquationSpec {
            name: "mismatch".to_owned(),
            lhs: "f(x)".to_owned(),
            rhs: "g(x)".to_owned(),
        }],
        directed_equations: vec![],
        policies: vec![],
    };
    let doc = TheoryDocument {
        id: "test".to_owned(),
        description: String::new(),
        body: TheoryBody::Theory(spec),
    };
    let resolver = builtin_resolver();
    let result = panproto_theory_dsl::compile(&doc, &resolver);
    assert!(
        result.is_err(),
        "DSL compile should reject sort-mismatched equation",
    );
}
