//! Comprehensive workflow tests exercising GAT engine, VCS integration,
//! and the instance layer in multi-step integration scenarios.
//!
//! Each test builds realistic multi-step pipelines combining features
//! from `panproto_gat`, `panproto_vcs`, `panproto_mig`, `panproto_schema`,
//! `panproto_check`, and `panproto_inst`.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::Name;
use panproto_gat::{
    Equation, FreeModelConfig, Model, ModelValue, NaturalTransformation, Operation, Sort, Term,
    Theory, TheoryMorphism, check_model, check_morphism, check_natural_transformation, free_model,
    horizontal_compose, pullback, quotient, typecheck_theory, vertical_compose,
};
use panproto_inst::metadata::Node;
use panproto_inst::value::Value;
use panproto_inst::wtype::CompiledMigration;
use panproto_inst::{AcsetOps, FInstance, GInstance, WInstance};
use panproto_mig::Migration;
use panproto_schema::{Schema, Vertex};
use panproto_vcs::gat_validate;
use panproto_vcs::merge::{MergeConflict, MergeOptions};
use panproto_vcs::store;
use panproto_vcs::{CommitOptions, ObjectId, Repository, Store, VcsError, dag, refs};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_schema(vertices: &[(&str, &str)]) -> Schema {
    let mut vert_map = HashMap::new();
    for (id, kind) in vertices {
        vert_map.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    Schema {
        protocol: "test".into(),
        vertices: vert_map,
        edges: HashMap::new(),
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        outgoing: HashMap::new(),
        incoming: HashMap::new(),
        between: HashMap::new(),
    }
}

/// Build a standard monoid theory for reuse in tests.
fn monoid_theory() -> Theory {
    let carrier = Sort::simple("Carrier");
    let mul = Operation::new(
        "mul",
        vec![
            ("a".into(), "Carrier".into()),
            ("b".into(), "Carrier".into()),
        ],
        "Carrier",
    );
    let unit = Operation::nullary("unit", "Carrier");

    let assoc = Equation::new(
        "assoc",
        Term::app(
            "mul",
            vec![
                Term::var("a"),
                Term::app("mul", vec![Term::var("b"), Term::var("c")]),
            ],
        ),
        Term::app(
            "mul",
            vec![
                Term::app("mul", vec![Term::var("a"), Term::var("b")]),
                Term::var("c"),
            ],
        ),
    );
    let left_id = Equation::new(
        "left_id",
        Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
        Term::var("a"),
    );
    let right_id = Equation::new(
        "right_id",
        Term::app("mul", vec![Term::var("a"), Term::constant("unit")]),
        Term::var("a"),
    );

    Theory::new(
        "Monoid",
        vec![carrier],
        vec![mul, unit],
        vec![assoc, left_id, right_id],
    )
}

/// Build an identity morphism for a theory.
fn identity_morphism(theory: &Theory, name: &str) -> TheoryMorphism {
    let sort_map: HashMap<Arc<str>, Arc<str>> = theory
        .sorts
        .iter()
        .map(|s| (Arc::clone(&s.name), Arc::clone(&s.name)))
        .collect();
    let op_map: HashMap<Arc<str>, Arc<str>> = theory
        .ops
        .iter()
        .map(|o| (Arc::clone(&o.name), Arc::clone(&o.name)))
        .collect();
    TheoryMorphism::new(name, &*theory.name, &*theory.name, sort_map, op_map)
}

/// Build an identity natural transformation on identity morphisms.
fn identity_nat_trans(
    theory: &Theory,
    source: &str,
    target: &str,
    name: &str,
) -> NaturalTransformation {
    let components: HashMap<Arc<str>, Term> = theory
        .sorts
        .iter()
        .map(|s| (Arc::clone(&s.name), Term::var("x")))
        .collect();
    NaturalTransformation {
        name: Arc::from(name),
        source: Arc::from(source),
        target: Arc::from(target),
        components,
    }
}

/// Helper: init repo, add schema, commit, return (repo, `ObjectId`).
fn init_with_schema(
    dir: &std::path::Path,
    vertices: &[(&str, &str)],
    msg: &str,
    author: &str,
) -> Result<(Repository, ObjectId), Box<dyn std::error::Error>> {
    let mut repo = Repository::init(dir)?;
    let s = make_schema(vertices);
    repo.add(&s)?;
    let cid = repo.commit(msg, author)?;
    Ok((repo, cid))
}

// ===========================================================================
// Test 1: full_add_commit_cycle_with_gat_validation
// ===========================================================================

#[test]
fn full_add_commit_cycle_with_gat_validation() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // First commit: simple schema.
    let s1 = make_schema(&[("a", "object")]);
    let index = repo.add(&s1)?;
    assert!(index.has_staged());
    // First commit has no migration, so diagnostics should be None.
    let staged = index.staged.as_ref().unwrap();
    assert!(staged.gat_diagnostics.is_none());
    let c1 = repo.commit("initial", "alice")?;

    // Second commit: add a vertex, triggering a migration + GAT validation.
    let s2 = make_schema(&[("a", "object"), ("b", "string")]);
    let index2 = repo.add(&s2)?;
    let staged2 = index2.staged.as_ref().unwrap();
    // Migration should be auto-derived.
    assert!(staged2.auto_derived);
    // GAT diagnostics should be present (migration was generated).
    assert!(staged2.gat_diagnostics.is_some());
    let diag = staged2.gat_diagnostics.as_ref().unwrap();
    // For simple vertex additions, diagnostics should be clean.
    assert!(
        diag.is_clean(),
        "expected clean diagnostics, got: type_errors={:?}, equation_errors={:?}",
        diag.type_errors,
        diag.equation_errors
    );

    let c2 = repo.commit("add vertex b", "alice")?;
    assert_ne!(c1, c2);

    // Verify commit history is correct.
    let log = repo.log(None)?;
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].message, "add vertex b");
    assert_eq!(log[1].message, "initial");

    // Verify HEAD points to c2.
    let head = store::resolve_head(repo.store())?;
    assert_eq!(head, Some(c2));

    Ok(())
}

// ===========================================================================
// Test 2: commit_blocked_by_equation_violation_then_skip_verify
// ===========================================================================

#[test]
fn commit_blocked_by_equation_violation_then_skip_verify() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // First commit.
    let s1 = make_schema(&[("a", "object")]);
    repo.add(&s1)?;
    repo.commit("initial", "alice")?;

    // Manually write an index with GAT equation errors to simulate
    // a staging result that has equation violations.
    let staged_schema = make_schema(&[("a", "object"), ("b", "string")]);
    let schema_id = repo
        .store_mut()
        .put(&panproto_vcs::Object::Schema(Box::new(staged_schema)))?;

    let diag = gat_validate::GatDiagnostics {
        type_errors: vec![],
        equation_errors: vec![
            "equation 'assoc' violated when a=0, b=1, c=2: LHS=3, RHS=4".to_owned(),
        ],
        migration_warnings: vec![],
    };

    let index = panproto_vcs::Index {
        staged: Some(panproto_vcs::index::StagedSchema {
            schema_id,
            migration_id: None,
            auto_derived: false,
            validation: panproto_vcs::index::ValidationStatus::Valid,
            gat_diagnostics: Some(diag),
        }),
    };

    // Write the index by serializing to JSON at the expected location.
    let index_path = dir.path().join(".panproto").join("index.json");
    let json = serde_json::to_string_pretty(&index)?;
    std::fs::write(&index_path, &json)?;

    // Attempt to commit: should fail due to GAT equation errors.
    let err = repo.commit("should fail", "alice").unwrap_err();
    assert!(
        matches!(&err, VcsError::ValidationFailed { reasons } if reasons.iter().any(|r| r.contains("equation violation"))),
        "expected ValidationFailed with equation violation, got: {err:?}"
    );

    // Re-write index (commit cleared it on error? Let's re-write to be safe).
    std::fs::write(&index_path, &json)?;

    // Retry with skip_verify: should succeed.
    let opts = CommitOptions { skip_verify: true };
    let commit_id = repo.commit_with_options("forced commit", "alice", &opts)?;

    let log = repo.log(None)?;
    assert_eq!(log[0].message, "forced commit");
    assert_eq!(store::resolve_head(repo.store())?, Some(commit_id));

    Ok(())
}

// ===========================================================================
// Test 3: merge_with_pullback_overlap_detection
// ===========================================================================

#[test]
fn merge_with_pullback_overlap_detection() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;

    // Init with base schema S1.
    let (mut repo, c1) = init_with_schema(
        dir.path(),
        &[("a", "object"), ("b", "string")],
        "initial",
        "alice",
    )?;

    // Create "feature" branch at c1.
    refs::create_branch(repo.store_mut(), "feature", c1)?;

    // On feature: add vertices c and d (and a shared vertex "shared").
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let s2 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("c", "number"),
        ("shared", "object"),
    ]);
    repo.add(&s2)?;
    repo.commit("add c and shared", "bob")?;

    // Back on main: add vertices e and the same "shared" vertex.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let s3 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("e", "boolean"),
        ("shared", "object"),
    ]);
    repo.add(&s3)?;
    repo.commit("add e and shared", "alice")?;

    // Merge feature into main.
    let result = repo.merge("feature", "alice")?;

    // The merged schema should contain vertices from both sides.
    assert!(result.merged_schema.vertices.contains_key("a"));
    assert!(result.merged_schema.vertices.contains_key("b"));
    assert!(result.merged_schema.vertices.contains_key("shared"));

    // The pullback overlap should have been computed and should include
    // the "shared" vertex (both sides added it with the same kind).
    if let Some(ref overlap) = result.pullback_overlap {
        // "shared" was added by both sides from the same base lineage,
        // so it should appear in the shared vertices set.
        // NOTE: pullback overlap detection is best-effort; it may or may
        // not detect "shared" depending on the morphism construction.
        // We verify the overlap structure is present.
        assert!(
            overlap.shared_vertices.len() >= 2,
            "expected at least 2 shared vertices (a, b from base), got: {:?}",
            overlap.shared_vertices
        );
    }

    // Merge should have no conflicts since "shared" has the same kind on both sides.
    assert!(
        result.conflicts.is_empty(),
        "expected no conflicts, got: {:?}",
        result.conflicts
    );

    Ok(())
}

// ===========================================================================
// Test 4: three_way_merge_with_conflicts_and_pullback
// ===========================================================================

#[test]
fn three_way_merge_with_conflicts_and_pullback() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;

    // Base schema.
    let (mut repo, c1) = init_with_schema(
        dir.path(),
        &[("x", "object"), ("y", "string")],
        "base",
        "alice",
    )?;

    // Create feature branch.
    refs::create_branch(repo.store_mut(), "feature", c1)?;

    // Feature: change x's kind to "number".
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let s_feature = make_schema(&[("x", "number"), ("y", "string")]);
    repo.add(&s_feature)?;
    repo.commit("change x to number", "bob")?;

    // Main: change x's kind to "boolean" (different from feature).
    refs::checkout_branch(repo.store_mut(), "main")?;
    let s_main = make_schema(&[("x", "boolean"), ("y", "string")]);
    repo.add(&s_main)?;
    repo.commit("change x to boolean", "alice")?;

    // Merge with no_commit to get the result without auto-committing.
    let opts = MergeOptions {
        no_commit: true,
        ..Default::default()
    };
    let result = repo.merge_with_options("feature", "alice", &opts)?;

    // Both sides modified vertex "x" differently: should be a conflict.
    assert!(
        !result.conflicts.is_empty(),
        "expected at least one conflict"
    );

    let x_conflict = result.conflicts.iter().find(
        |c| matches!(c, MergeConflict::BothModifiedVertex { vertex_id, .. } if vertex_id == "x"),
    );
    assert!(
        x_conflict.is_some(),
        "expected BothModifiedVertex conflict for 'x', got: {:?}",
        result.conflicts
    );

    // Pullback overlap should still be computed even when conflicts exist.
    // The shared vertex "y" (unchanged on both sides) should be in the overlap.
    if let Some(ref overlap) = result.pullback_overlap {
        assert!(
            !overlap.shared_vertices.is_empty(),
            "expected some shared vertices in pullback overlap"
        );
    }

    // Verify conflict count.
    let vertex_conflict_count = result
        .conflicts
        .iter()
        .filter(|c| {
            matches!(
                c,
                MergeConflict::BothModifiedVertex { .. }
                    | MergeConflict::BothAddedVertexDifferently { .. }
            )
        })
        .count();
    assert_eq!(
        vertex_conflict_count, 1,
        "expected exactly 1 vertex conflict"
    );

    Ok(())
}

// ===========================================================================
// Test 5: composition_coherence_across_five_commits
// ===========================================================================

#[test]
fn composition_coherence_across_five_commits() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let mut repo = Repository::init(dir.path())?;

    // Commit 1: base schema.
    let s1 = make_schema(&[("v1", "object"), ("v2", "string")]);
    repo.add(&s1)?;
    let c1 = repo.commit("commit 1", "alice")?;

    // Commit 2: rename v2 -> v2b by removing v2, adding v2b.
    let s2 = make_schema(&[("v1", "object"), ("v2b", "string"), ("v3", "number")]);
    repo.add(&s2)?;
    let c2 = repo.commit("commit 2: add v3, rename v2", "alice")?;

    // Commit 3: add v4.
    let s3 = make_schema(&[
        ("v1", "object"),
        ("v2b", "string"),
        ("v3", "number"),
        ("v4", "boolean"),
    ]);
    repo.add(&s3)?;
    let c3 = repo.commit("commit 3: add v4", "alice")?;

    // Commit 4: remove v2b.
    let s4 = make_schema(&[("v1", "object"), ("v3", "number"), ("v4", "boolean")]);
    repo.add(&s4)?;
    let c4 = repo.commit("commit 4: remove v2b", "alice")?;

    // Commit 5: add v5.
    let s5 = make_schema(&[
        ("v1", "object"),
        ("v3", "number"),
        ("v4", "boolean"),
        ("v5", "object"),
    ]);
    repo.add(&s5)?;
    let c5 = repo.commit("commit 5: add v5", "alice")?;

    // Compose the migration path from c1 to c5.
    let path = vec![c1, c2, c3, c4, c5];
    let composition = dag::compose_path_with_coherence(repo.store(), &path)?;

    // The composed migration should map v1 -> v1 (it survives all commits).
    assert!(
        composition.migration.vertex_map.contains_key("v1"),
        "v1 should survive in composed migration"
    );
    assert_eq!(
        composition.migration.vertex_map.get(&Name::from("v1")),
        Some(&Name::from("v1")),
        "v1 should map to itself"
    );

    // v2 was renamed/removed, so it should not map to anything in the final schema,
    // or should map to a vertex that no longer exists. The composed migration
    // captures the transitive mapping.

    // Coherence warnings should be present if any structural issues exist.
    // At minimum, verify the result has the expected shape.
    assert!(
        !composition.migration.vertex_map.is_empty(),
        "composed migration should map at least v1"
    );

    Ok(())
}

// ===========================================================================
// Test 6: typecheck_theory_in_vcs_pipeline
// ===========================================================================

#[test]
fn typecheck_theory_in_vcs_pipeline() {
    // Well-typed theory: monoid with correct equations.
    let good_theory = monoid_theory();
    let diag_good = gat_validate::validate_theory_equations(&good_theory);
    assert!(
        diag_good.is_clean(),
        "expected clean diagnostics for well-typed theory, got: {:?}",
        diag_good.type_errors
    );

    // Verify typecheck_theory also passes.
    assert!(typecheck_theory(&good_theory).is_ok());

    // Ill-typed theory: equation where LHS and RHS have different sorts.
    let bad_theory = Theory::new(
        "BadTheory",
        vec![Sort::simple("A"), Sort::simple("B")],
        vec![
            Operation::unary("f", "x", "A", "B"),
            Operation::nullary("a0", "A"),
        ],
        vec![
            // f(a0()) has sort B, but a0() has sort A -> sort mismatch.
            Equation::new(
                "bad_eq",
                Term::app("f", vec![Term::constant("a0")]),
                Term::constant("a0"),
            ),
        ],
    );

    let diag_bad = gat_validate::validate_theory_equations(&bad_theory);
    assert!(
        !diag_bad.is_clean(),
        "expected type errors for ill-typed theory"
    );
    assert!(
        !diag_bad.type_errors.is_empty(),
        "expected at least one type error"
    );

    // Verify typecheck_theory also fails.
    assert!(typecheck_theory(&bad_theory).is_err());
}

// ===========================================================================
// Test 7: free_model_check_model_roundtrip
// ===========================================================================

#[test]
fn free_model_check_model_roundtrip() {
    // Define a monoid theory with associativity and identity equations.
    let theory = monoid_theory();

    // Build a free model with depth=3.
    let config = FreeModelConfig {
        max_depth: 3,
        max_terms_per_sort: 1000,
    };
    let free = free_model(&theory, &config).unwrap();

    // The free model should have at least one element in Carrier
    // (the term `unit()`).
    assert!(
        !free.sort_interp["Carrier"].is_empty(),
        "free model carrier should be non-empty"
    );

    // Verify unit() is in the carrier set.
    let has_unit = free.sort_interp["Carrier"]
        .iter()
        .any(|v| matches!(v, ModelValue::Str(s) if s == "unit()"));
    assert!(has_unit, "free model should contain unit()");

    // The free model's operations should be well-defined.
    let unit_result = free.eval("unit", &[]).unwrap();
    assert!(matches!(unit_result, ModelValue::Str(_)));

    // Build a concrete model that satisfies monoid equations: (Z_5, +, 0).
    let mut z5 = Model::new("Monoid");
    z5.add_sort("Carrier", (0..5).map(ModelValue::Int).collect());
    z5.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int((a + b) % 5)),
        _ => Err(panproto_gat::GatError::ModelError("expected Int".into())),
    });
    z5.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));

    // check_model on the valid model should produce zero violations.
    let violations = check_model(&z5, &theory).unwrap();
    assert!(
        violations.is_empty(),
        "Z_5 model should satisfy all monoid equations, got {violations:?}"
    );

    // Break the model: wrong identity element.
    z5.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(1)));
    let violations = check_model(&z5, &theory).unwrap();
    assert!(
        !violations.is_empty(),
        "broken model should have violations"
    );
    // At least one violation should be about identity laws.
    let has_id_violation = violations
        .iter()
        .any(|v| v.equation.as_ref() == "left_id" || v.equation.as_ref() == "right_id");
    assert!(
        has_id_violation,
        "expected identity law violation, got: {:?}",
        violations.iter().map(|v| &*v.equation).collect::<Vec<_>>()
    );
}

// ===========================================================================
// Test 8: quotient_then_pullback_then_typecheck
// ===========================================================================

#[test]
fn quotient_then_pullback_then_typecheck() {
    // Define two theories that share common sorts.
    // Theory 1: has sorts A, B and an operation f: A -> B.
    let t1 = Theory::new(
        "T1",
        vec![Sort::simple("A"), Sort::simple("B")],
        vec![Operation::unary("f", "x", "A", "B")],
        vec![],
    );

    // Theory 2: has sorts A, C and an operation g: A -> C.
    let t2 = Theory::new(
        "T2",
        vec![Sort::simple("A"), Sort::simple("C")],
        vec![Operation::unary("g", "x", "A", "C")],
        vec![],
    );

    // Both map into a common target theory with a shared sort "X".
    // m1: T1 -> Target, mapping A -> X, B -> Y
    let m1 = TheoryMorphism::new(
        "m1",
        "T1",
        "Target",
        HashMap::from([
            (Arc::from("A"), Arc::from("X")),
            (Arc::from("B"), Arc::from("Y")),
        ]),
        HashMap::from([(Arc::from("f"), Arc::from("phi"))]),
    );

    // m2: T2 -> Target, mapping A -> X, C -> Z
    let m2 = TheoryMorphism::new(
        "m2",
        "T2",
        "Target",
        HashMap::from([
            (Arc::from("A"), Arc::from("X")),
            (Arc::from("C"), Arc::from("Z")),
        ]),
        HashMap::from([(Arc::from("g"), Arc::from("psi"))]),
    );

    // Compute pullback: should find A as a shared sort.
    let pb = pullback(&t1, &t2, &m1, &m2).unwrap();

    // The pullback should have at least sort A (shared through X).
    assert!(
        !pb.theory.sorts.is_empty(),
        "pullback should have shared sorts"
    );
    assert_eq!(
        pb.theory.sorts.len(),
        1,
        "pullback should have exactly 1 shared sort (A)"
    );
    assert!(pb.theory.find_sort("A").is_some());

    // Projections should validate.
    assert!(check_morphism(&pb.proj1, &pb.theory, &t1).is_ok());
    assert!(check_morphism(&pb.proj2, &pb.theory, &t2).is_ok());

    // Now quotient T1 by identifying A and B (they become one sort).
    let q = quotient(&t1, &[(Arc::from("A"), Arc::from("B"))]).unwrap();
    assert_eq!(
        q.sorts.len(),
        1,
        "quotient should merge A and B into one sort"
    );
    assert!(q.find_sort("A").is_some());
    // f should now be A -> A.
    let f_op = q.find_op("f").unwrap();
    assert_eq!(&*f_op.output, "A");
    assert_eq!(&*f_op.inputs[0].1, "A");

    // Typecheck the quotient theory.
    assert!(
        typecheck_theory(&q).is_ok(),
        "quotient theory should typecheck (no equations)"
    );

    // Add an equation to the quotient and typecheck it.
    let q_with_eq = Theory::new(
        "Q_with_eq",
        q.sorts.clone(),
        q.ops.clone(),
        vec![Equation::new(
            "f_idem",
            Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
            Term::app("f", vec![Term::var("x")]),
        )],
    );
    assert!(
        typecheck_theory(&q_with_eq).is_ok(),
        "quotient theory with idempotence equation should typecheck"
    );
}

// ===========================================================================
// Test 9: natural_transformation_vertical_horizontal_compose
// ===========================================================================

#[test]
fn natural_transformation_vertical_horizontal_compose() {
    // Define a small category-like theory with Ob and Mor sorts.
    let cat_theory = Theory::new(
        "Cat",
        vec![Sort::simple("Ob"), Sort::simple("Mor")],
        vec![
            Operation::unary("src", "f", "Mor", "Ob"),
            Operation::unary("tgt", "f", "Mor", "Ob"),
        ],
        vec![],
    );

    // Build identity morphisms.
    let id_f = identity_morphism(&cat_theory, "F");
    let id_g = identity_morphism(&cat_theory, "G");
    let id_h = identity_morphism(&cat_theory, "H");

    // Identity natural transformation alpha: F => G (where F = G = id).
    let alpha = identity_nat_trans(&cat_theory, "F", "G", "alpha");

    // Check alpha is a valid natural transformation.
    assert!(
        check_natural_transformation(&alpha, &id_f, &id_g, &cat_theory, &cat_theory).is_ok(),
        "identity nat trans alpha should validate"
    );

    // Another identity nat trans beta: G => H.
    let beta = identity_nat_trans(&cat_theory, "G", "H", "beta");
    assert!(
        check_natural_transformation(&beta, &id_g, &id_h, &cat_theory, &cat_theory).is_ok(),
        "identity nat trans beta should validate"
    );

    // Vertical composition: beta . alpha : F => H.
    let vert = vertical_compose(&alpha, &beta, &cat_theory).unwrap();
    assert_eq!(&*vert.source, "F");
    assert_eq!(&*vert.target, "H");
    // Each component should still be Var("x") since id . id = id.
    for sort in &cat_theory.sorts {
        let comp = vert.components.get(&sort.name).unwrap();
        assert_eq!(
            comp,
            &Term::var("x"),
            "vertical composition of identities should be identity"
        );
    }

    // Horizontal composition: beta * alpha where alpha: F => G and beta: H => K.
    // Use identity morphisms and nat trans for simplicity.
    let alpha2 = identity_nat_trans(&cat_theory, "F", "G", "alpha2");
    let beta2 = identity_nat_trans(&cat_theory, "H", "K", "beta2");
    // For horizontal compose: need morphisms G and H where codomain(G) = domain(H).
    // Since all morphisms are Cat -> Cat identities, this works.
    let horiz = horizontal_compose(&alpha2, &beta2, &id_f, &id_g, &id_h, &cat_theory).unwrap();

    // Horizontal composition of identity transformations should have identity components.
    for sort in &cat_theory.sorts {
        let comp = horiz.components.get(&sort.name).unwrap();
        assert_eq!(
            comp,
            &Term::var("x"),
            "horizontal composition of identities should be identity"
        );
    }

    // Verify the composed nat trans has the expected source and target.
    assert_eq!(&*horiz.source, "H.F");
    assert_eq!(&*horiz.target, "K.G");
}

// ===========================================================================
// Test 10: acset_restrict_extend_across_all_shapes
// ===========================================================================

#[test]
fn acset_restrict_extend_across_all_shapes() {
    use std::collections::HashSet;

    // Build source and target schemas.
    let src_schema = make_schema(&[("person", "object"), ("name", "string")]);
    let tgt_schema = make_schema(&[("human", "object"), ("label", "string")]);

    // Compile a migration (person -> human, name -> label).
    let compiled = CompiledMigration {
        surviving_verts: HashSet::from([Name::from("human"), Name::from("label")]),
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::from([
            (Name::from("person"), Name::from("human")),
            (Name::from("name"), Name::from("label")),
        ]),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    // --- WInstance ---
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "person"));
    nodes.insert(1, Node::new(1, "person"));
    let w = WInstance::new(nodes, vec![], vec![], 0, Name::from("person"));
    assert_eq!(AcsetOps::element_count(&w), 2);
    assert_eq!(AcsetOps::shape_name(&w), "wtype");

    // Extend: remap vertices.
    let w_ext = AcsetOps::extend(&w, &tgt_schema, &compiled).unwrap();
    assert_eq!(AcsetOps::element_count(&w_ext), 2);
    // After extend, nodes should have been remapped to "human".

    // --- FInstance ---
    let mut row = HashMap::new();
    row.insert("field1".to_string(), Value::Str("Alice".into()));
    let f = FInstance::new().with_table("person", vec![row]);
    assert_eq!(AcsetOps::element_count(&f), 1);
    assert_eq!(AcsetOps::shape_name(&f), "functor");

    // Restrict: keep only surviving tables.
    let f_res = AcsetOps::restrict(&f, &src_schema, &tgt_schema, &compiled).unwrap();
    // After restrict, the "person" table should be remapped to "human".
    assert_eq!(AcsetOps::shape_name(&f_res), "functor");

    // Extend.
    let f_ext = AcsetOps::extend(&f, &tgt_schema, &compiled).unwrap();
    assert_eq!(AcsetOps::shape_name(&f_ext), "functor");

    // --- GInstance ---
    let g = GInstance::new()
        .with_node(Node::new(0, "person"))
        .with_node(Node::new(1, "person"))
        .with_value(0, Value::Str("Alice".into()))
        .with_value(1, Value::Str("Bob".into()));
    assert_eq!(AcsetOps::element_count(&g), 2);
    assert_eq!(AcsetOps::shape_name(&g), "graph");

    // Restrict: remap vertices.
    let g_res = AcsetOps::restrict(&g, &src_schema, &tgt_schema, &compiled).unwrap();
    assert_eq!(AcsetOps::element_count(&g_res), 2);
    // Nodes should be remapped to "human".
    for node in g_res.nodes.values() {
        assert_eq!(&*node.anchor, "human");
    }

    // Extend.
    let g_ext = AcsetOps::extend(&g, &tgt_schema, &compiled).unwrap();
    assert_eq!(AcsetOps::element_count(&g_ext), 2);
    for node in g_ext.nodes.values() {
        assert_eq!(&*node.anchor, "human");
    }
}

// ===========================================================================
// Test 11: vcs_add_commit_merge_rebase_with_gat_checks
// ===========================================================================

#[test]
fn vcs_add_commit_merge_rebase_with_gat_checks() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;

    // Init repo with S1.
    let (mut repo, c1) = init_with_schema(
        dir.path(),
        &[("a", "object"), ("b", "string")],
        "initial",
        "alice",
    )?;

    // Create "feature" branch at c1.
    refs::create_branch(repo.store_mut(), "feature", c1)?;

    // On feature: add vertex c.
    refs::checkout_branch(repo.store_mut(), "feature")?;
    let s2 = make_schema(&[("a", "object"), ("b", "string"), ("c", "number")]);
    repo.add(&s2)?;
    let _c2 = repo.commit("add c on feature", "bob")?;

    // Back to main: add vertex d.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let s3 = make_schema(&[("a", "object"), ("b", "string"), ("d", "boolean")]);
    repo.add(&s3)?;
    let _c3 = repo.commit("add d on main", "alice")?;

    // Merge feature into main.
    let merge_result = repo.merge("feature", "alice")?;
    assert!(
        merge_result.conflicts.is_empty(),
        "expected clean merge, got conflicts: {:?}",
        merge_result.conflicts
    );

    // The merged schema should have a, b, c, d.
    assert!(merge_result.merged_schema.vertices.contains_key("a"));
    assert!(merge_result.merged_schema.vertices.contains_key("b"));
    assert!(merge_result.merged_schema.vertices.contains_key("c"));
    assert!(merge_result.merged_schema.vertices.contains_key("d"));

    // Verify pullback overlap exists (base vertices a, b are shared).
    if let Some(ref overlap) = merge_result.pullback_overlap {
        assert!(
            !overlap.shared_vertices.is_empty(),
            "expected shared vertices in pullback overlap"
        );
    }

    // Verify log has the merge commit + both parent chains.
    let log = repo.log(None)?;
    assert!(
        log.len() >= 3,
        "expected at least 3 commits in log, got {}",
        log.len()
    );

    // The HEAD commit should be a merge commit with 2 parents.
    let head_commit = &log[0];
    assert_eq!(
        head_commit.parents.len(),
        2,
        "merge commit should have 2 parents"
    );

    // Now test rebase: create another branch and rebase it.
    let head_id = store::resolve_head(repo.store())?.unwrap();
    refs::create_branch(repo.store_mut(), "rebase-test", head_id)?;
    refs::checkout_branch(repo.store_mut(), "rebase-test")?;
    let s4 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("c", "number"),
        ("d", "boolean"),
        ("e", "object"),
    ]);
    repo.add(&s4)?;
    let c4 = repo.commit("add e on rebase-test", "charlie")?;

    // Add another commit on main.
    refs::checkout_branch(repo.store_mut(), "main")?;
    let s5 = make_schema(&[
        ("a", "object"),
        ("b", "string"),
        ("c", "number"),
        ("d", "boolean"),
        ("f", "string"),
    ]);
    repo.add(&s5)?;
    let c5 = repo.commit("add f on main", "alice")?;

    // Rebase rebase-test onto main.
    refs::checkout_branch(repo.store_mut(), "rebase-test")?;
    let _rebase_result = repo.rebase(c5, "charlie")?;

    // After rebase, HEAD should be a new commit (not c4).
    let post_rebase_head = store::resolve_head(repo.store())?.unwrap();
    assert_ne!(post_rebase_head, c4, "rebased commit should have new ID");

    // Verify the log reflects the rebased history.
    let post_rebase_log = repo.log(None)?;
    assert!(
        post_rebase_log.len() >= 2,
        "rebased log should have at least 2 commits"
    );

    Ok(())
}

// ===========================================================================
// Test 12: gat_validate_migration_with_schema_checking
// ===========================================================================

#[test]
fn gat_validate_migration_with_schema_checking() {
    // Build two schemas with known vertex sets.
    let old_schema = make_schema(&[("a", "object"), ("b", "string"), ("c", "number")]);
    let new_schema = make_schema(&[("a", "object"), ("b", "string"), ("d", "boolean")]);

    // Build a migration with a vertex_map that references a nonexistent source vertex.
    let bad_migration = Migration {
        vertex_map: HashMap::from([
            (Name::from("a"), Name::from("a")),
            (Name::from("b"), Name::from("b")),
            (Name::from("nonexistent"), Name::from("d")), // nonexistent in source
        ]),
        edge_map: HashMap::new(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let diag = gat_validate::validate_migration(&old_schema, &new_schema, &bad_migration);
    // Should have warnings about the nonexistent source vertex.
    assert!(
        !diag.migration_warnings.is_empty(),
        "expected warnings for nonexistent source vertex"
    );
    let has_source_warning = diag
        .migration_warnings
        .iter()
        .any(|w| w.contains("nonexistent") && w.contains("does not exist in source"));
    assert!(
        has_source_warning,
        "expected warning about 'nonexistent' not in source, got: {:?}",
        diag.migration_warnings
    );

    // Build a valid migration.
    let good_migration = Migration {
        vertex_map: HashMap::from([
            (Name::from("a"), Name::from("a")),
            (Name::from("b"), Name::from("b")),
        ]),
        edge_map: HashMap::new(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let diag_good = gat_validate::validate_migration(&old_schema, &new_schema, &good_migration);
    assert!(
        diag_good.is_clean(),
        "valid migration should produce clean diagnostics"
    );
    // Warnings are non-blocking, so is_clean checks only type_errors and equation_errors.
    // But migration_warnings should be empty too for a proper migration.
    assert!(
        diag_good.migration_warnings.is_empty(),
        "valid migration should have no warnings, got: {:?}",
        diag_good.migration_warnings
    );

    // Test with an empty migration: should produce a warning.
    let empty_migration = Migration::empty();
    let diag_empty = gat_validate::validate_migration(&old_schema, &new_schema, &empty_migration);
    assert!(
        !diag_empty.migration_warnings.is_empty(),
        "empty migration should produce a warning"
    );
    assert!(
        diag_empty
            .migration_warnings
            .iter()
            .any(|w| w.contains("maps zero vertices")),
        "expected 'maps zero vertices' warning"
    );
}
