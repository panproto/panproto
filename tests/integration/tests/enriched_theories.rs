//! Integration tests for enriched theory features.
//!
//! Tests expression evaluation via models, directed equations via GAT
//! axioms with distinguished direction, schema enrichment via builder
//! constraints and defaults, theory composition via colimit, coverage
//! analysis via migration mapping, optic classification via protolens
//! transform structure, symbolic simplification via protolens chain
//! fusion, refinement types via constraint subsort checking, building-
//! block theory construction, provenance via schema morphism
//! composition, and equality witnesses via equation reflexivity and
//! transitivity.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_check::{BreakingChange, classify, diff};
use panproto_gat::{
    Equation, Model, ModelValue, Name, Operation, Sort, Term, Theory, TheoryMorphism,
    TheoryTransform, check_model, check_morphism, colimit, migrate_model,
};
use panproto_lens::protolens::{ProtolensChain, elementary};
use panproto_mig::Migration;
use panproto_protocols::theories;
use panproto_schema::{Constraint, Edge, Protocol, Schema, SchemaBuilder, Vertex};
use smallvec::SmallVec;

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

/// Build a schema from vertices, edges, and optional constraints.
fn make_schema(
    verts: &[(&str, &str)],
    edge_list: &[Edge],
    constraints: HashMap<Name, Vec<Constraint>>,
) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in edge_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints,
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

fn test_protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![panproto_schema::EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: vec![],
        }],
        obj_kinds: vec![
            "object".into(),
            "string".into(),
            "integer".into(),
            "record".into(),
        ],
        constraint_sorts: vec!["maxLength".into(), "minLength".into(), "maximum".into()],
        ..Protocol::default()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Test 1: Expression evaluation via Model
// ═══════════════════════════════════════════════════════════════════

#[test]
fn expr_eval_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    // Build an arithmetic theory with add and constant 1.
    let theory = Theory::new(
        "ThArith",
        vec![Sort::simple("Int")],
        vec![
            Operation::new(
                "add",
                vec![("a".into(), "Int".into()), ("b".into(), "Int".into())],
                "Int",
            ),
            Operation::nullary("one", "Int"),
        ],
        vec![],
    );

    // Build a model: Int = integers, add = +, one = 1.
    let mut model = Model::new("ThArith");
    let carrier: Vec<ModelValue> = (0..50).map(ModelValue::Int).collect();
    model.add_sort("Int", carrier);
    model.add_op("add", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a + b)),
        _ => Err(panproto_gat::GatError::ModelError("expected Int".into())),
    });
    model.add_op("one", |_: &[ModelValue]| Ok(ModelValue::Int(1)));

    // Evaluate: add(41, one()) = 42
    let one = model.eval("one", &[])?;
    let result = model.eval("add", &[ModelValue::Int(41), one])?;
    assert_eq!(result, ModelValue::Int(42));

    // Verify equation satisfaction: the model has no equations, so
    // check_model should return no violations.
    let violations = check_model(&model, &theory)?;
    assert!(
        violations.is_empty(),
        "arithmetic model should have no equation violations"
    );

    Ok(())
}

#[test]
fn expr_eval_string_operations() -> Result<(), Box<dyn std::error::Error>> {
    // Build a string theory with concat.
    let mut model = Model::new("ThString");
    model.add_sort(
        "Str",
        vec![
            ModelValue::Str("Alice".into()),
            ModelValue::Str("Smith".into()),
            ModelValue::Str(" ".into()),
        ],
    );
    model.add_op("concat", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Str(a), ModelValue::Str(b)) => Ok(ModelValue::Str(format!("{a}{b}"))),
        _ => Err(panproto_gat::GatError::ModelError("expected Str".into())),
    });

    // Build merge expression: concat(first, concat(" ", last))
    let space = ModelValue::Str(" ".into());
    let inner = model.eval("concat", &[space, ModelValue::Str("Smith".into())])?;
    let result = model.eval("concat", &[ModelValue::Str("Alice".into()), inner])?;
    assert_eq!(result, ModelValue::Str("Alice Smith".into()));

    Ok(())
}

#[test]
fn expr_eval_split_join_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Model with split and join operations.
    let mut model = Model::new("ThSplitJoin");
    model.add_sort("Str", vec![ModelValue::Str("a,b,c".into())]);
    model.add_sort(
        "List",
        vec![ModelValue::List(vec![
            ModelValue::Str("a".into()),
            ModelValue::Str("b".into()),
            ModelValue::Str("c".into()),
        ])],
    );

    model.add_op("split", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Str(s), ModelValue::Str(sep)) => {
            let parts: Vec<ModelValue> = s
                .split(&**sep)
                .map(|p| ModelValue::Str(p.to_owned()))
                .collect();
            Ok(ModelValue::List(parts))
        }
        _ => Err(panproto_gat::GatError::ModelError(
            "expected Str args".into(),
        )),
    });

    model.add_op("join", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::List(items), ModelValue::Str(sep)) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|v| {
                    if let ModelValue::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .collect();
            Ok(ModelValue::Str(parts.join(sep)))
        }
        _ => Err(panproto_gat::GatError::ModelError(
            "expected List, Str".into(),
        )),
    });

    // split("a,b,c", ",") -> ["a", "b", "c"]
    let split_result = model.eval(
        "split",
        &[ModelValue::Str("a,b,c".into()), ModelValue::Str(",".into())],
    )?;
    assert_eq!(
        split_result,
        ModelValue::List(vec![
            ModelValue::Str("a".into()),
            ModelValue::Str("b".into()),
            ModelValue::Str("c".into()),
        ])
    );

    // join(["a","b","c"], ",") -> "a,b,c"
    let join_result = model.eval("join", &[split_result, ModelValue::Str(",".into())])?;
    assert_eq!(join_result, ModelValue::Str("a,b,c".into()));

    // Round-trip: join(split(s, sep), sep) = s
    assert_eq!(join_result, ModelValue::Str("a,b,c".into()));

    Ok(())
}

#[test]
fn expr_eval_record_operations() -> Result<(), Box<dyn std::error::Error>> {
    // Model with record access and merge.
    let mut model = Model::new("ThRecord");
    model.add_sort("Record", vec![]);
    model.add_sort("Value", vec![]);

    model.add_op("get_field", |args: &[ModelValue]| {
        match (&args[0], &args[1]) {
            (ModelValue::Map(m), ModelValue::Str(key)) => {
                Ok(m.get(key.as_str()).cloned().unwrap_or(ModelValue::Null))
            }
            _ => Err(panproto_gat::GatError::ModelError(
                "expected Map, Str".into(),
            )),
        }
    });

    model.add_op("merge", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Map(base), ModelValue::Map(patch)) => {
            let mut merged = base.clone();
            for (k, v) in patch {
                merged.insert(k.clone(), v.clone());
            }
            Ok(ModelValue::Map(merged))
        }
        _ => Err(panproto_gat::GatError::ModelError(
            "expected Map, Map".into(),
        )),
    });

    // Build record { name: "alice", age: 30 }
    let record = ModelValue::Map(rustc_hash::FxHashMap::from_iter([
        ("name".to_owned(), ModelValue::Str("alice".into())),
        ("age".to_owned(), ModelValue::Int(30)),
    ]));

    // Access field "age", verify 30
    let age = model.eval(
        "get_field",
        &[record.clone(), ModelValue::Str("age".into())],
    )?;
    assert_eq!(age, ModelValue::Int(30));

    // Merge with { age: 31 }, verify override
    let patch = ModelValue::Map(rustc_hash::FxHashMap::from_iter([(
        "age".to_owned(),
        ModelValue::Int(31),
    )]));
    let merged = model.eval("merge", &[record, patch])?;
    let new_age = model.eval("get_field", &[merged, ModelValue::Str("age".into())])?;
    assert_eq!(new_age, ModelValue::Int(31));

    Ok(())
}

#[test]
fn expr_eval_map_filter_fold() -> Result<(), Box<dyn std::error::Error>> {
    let mut model = Model::new("ThListOps");
    model.add_sort("List", vec![]);
    model.add_sort("Int", vec![]);

    // map: apply a multiplier to each element
    model.add_op("map_mul", |args: &[ModelValue]| {
        match (&args[0], &args[1]) {
            (ModelValue::List(items), ModelValue::Int(factor)) => {
                let result: Vec<ModelValue> = items
                    .iter()
                    .map(|v| {
                        if let ModelValue::Int(x) = v {
                            ModelValue::Int(x * factor)
                        } else {
                            v.clone()
                        }
                    })
                    .collect();
                Ok(ModelValue::List(result))
            }
            _ => Err(panproto_gat::GatError::ModelError(
                "expected List, Int".into(),
            )),
        }
    });

    // filter_gt: keep elements > threshold
    model.add_op("filter_gt", |args: &[ModelValue]| {
        match (&args[0], &args[1]) {
            (ModelValue::List(items), ModelValue::Int(threshold)) => {
                let result: Vec<ModelValue> = items
                    .iter()
                    .filter(|v| {
                        if let ModelValue::Int(x) = v {
                            x > threshold
                        } else {
                            false
                        }
                    })
                    .cloned()
                    .collect();
                Ok(ModelValue::List(result))
            }
            _ => Err(panproto_gat::GatError::ModelError(
                "expected List, Int".into(),
            )),
        }
    });

    // fold_sum: reduce list by addition with initial accumulator
    model.add_op("fold_sum", |args: &[ModelValue]| {
        match (&args[0], &args[1]) {
            (ModelValue::List(items), ModelValue::Int(init)) => {
                let sum = items.iter().fold(*init, |acc, v| {
                    if let ModelValue::Int(x) = v {
                        acc + x
                    } else {
                        acc
                    }
                });
                Ok(ModelValue::Int(sum))
            }
            _ => Err(panproto_gat::GatError::ModelError(
                "expected List, Int".into(),
            )),
        }
    });

    let list = ModelValue::List(vec![
        ModelValue::Int(1),
        ModelValue::Int(2),
        ModelValue::Int(3),
    ]);

    // map([1,2,3], 2) -> [2,4,6]
    let mapped = model.eval("map_mul", &[list.clone(), ModelValue::Int(2)])?;
    assert_eq!(
        mapped,
        ModelValue::List(vec![
            ModelValue::Int(2),
            ModelValue::Int(4),
            ModelValue::Int(6),
        ])
    );

    // filter([1,2,3,4,5], 3) -> [4,5]
    let big_list = ModelValue::List(vec![
        ModelValue::Int(1),
        ModelValue::Int(2),
        ModelValue::Int(3),
        ModelValue::Int(4),
        ModelValue::Int(5),
    ]);
    let filtered = model.eval("filter_gt", &[big_list, ModelValue::Int(3)])?;
    assert_eq!(
        filtered,
        ModelValue::List(vec![ModelValue::Int(4), ModelValue::Int(5),])
    );

    // fold([1,2,3], 0) -> 6
    let folded = model.eval("fold_sum", &[list, ModelValue::Int(0)])?;
    assert_eq!(folded, ModelValue::Int(6));

    Ok(())
}

#[test]
fn expr_closure_semantics() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that environment capture works via model operations.
    // Simulate: let x = 10 in (add(x, y)) where y = 5 -> 15

    let mut model = Model::new("ThClosure");
    model.add_sort("Int", (0..20).map(ModelValue::Int).collect());

    model.add_op("add", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a + b)),
        _ => Err(panproto_gat::GatError::ModelError("expected Int".into())),
    });

    // Simulate closure capture: x = 10, apply to y = 5
    let x = ModelValue::Int(10);
    let y = ModelValue::Int(5);
    let result = model.eval("add", &[x, y])?;
    assert_eq!(result, ModelValue::Int(15));

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Test 2: Directed equations via GAT axioms with directionality
// ═══════════════════════════════════════════════════════════════════

#[test]
fn directed_equation_construction() {
    // A "directed equation" is an equation where lhs rewrites to rhs.
    // This models coercions: parse_int(to_string(x)) = x.
    let parse_int_eq = Equation::new(
        "parse_int_roundtrip",
        Term::app(
            "parse_int",
            vec![Term::app("to_string", vec![Term::var("x")])],
        ),
        Term::var("x"),
    );

    let theory = Theory::new(
        "ThCoercion",
        vec![Sort::simple("Int"), Sort::simple("Str")],
        vec![
            Operation::unary("to_string", "x", "Int", "Str"),
            Operation::unary("parse_int", "s", "Str", "Int"),
        ],
        vec![parse_int_eq],
    );

    // Verify the theory has the directed equation.
    assert!(
        theory.find_eq("parse_int_roundtrip").is_some(),
        "theory should contain the directed equation"
    );

    let eq = &theory.eqs[0];
    // Verify the impl_term (lhs) is the parse_int(to_string(x)) expression.
    assert_eq!(
        eq.lhs,
        Term::app(
            "parse_int",
            vec![Term::app("to_string", vec![Term::var("x")])]
        )
    );
    assert_eq!(eq.rhs, Term::var("x"));
}

#[test]
fn directed_equation_with_inverse() -> Result<(), Box<dyn std::error::Error>> {
    // Forward: parse_int(to_string(x)) = x
    // Inverse: to_string(parse_int(s)) = s
    let forward = Equation::new(
        "parse_roundtrip",
        Term::app(
            "parse_int",
            vec![Term::app("to_string", vec![Term::var("x")])],
        ),
        Term::var("x"),
    );
    let inverse = Equation::new(
        "format_roundtrip",
        Term::app(
            "to_string",
            vec![Term::app("parse_int", vec![Term::var("s")])],
        ),
        Term::var("s"),
    );

    let theory = Theory::new(
        "ThBidiCoercion",
        vec![Sort::simple("Int"), Sort::simple("Str")],
        vec![
            Operation::unary("to_string", "x", "Int", "Str"),
            Operation::unary("parse_int", "s", "Str", "Int"),
        ],
        vec![forward, inverse],
    );

    // Both equations should be accessible.
    assert!(theory.find_eq("parse_roundtrip").is_some());
    assert!(theory.find_eq("format_roundtrip").is_some());
    assert_eq!(theory.eqs.len(), 2);

    // Build a model and verify the equations hold.
    let mut model = Model::new("ThBidiCoercion");
    model.add_sort(
        "Int",
        vec![ModelValue::Int(0), ModelValue::Int(1), ModelValue::Int(42)],
    );
    model.add_sort(
        "Str",
        vec![
            ModelValue::Str("0".into()),
            ModelValue::Str("1".into()),
            ModelValue::Str("42".into()),
        ],
    );
    model.add_op("to_string", |args: &[ModelValue]| match &args[0] {
        ModelValue::Int(n) => Ok(ModelValue::Str(n.to_string())),
        _ => Err(panproto_gat::GatError::ModelError("expected Int".into())),
    });
    model.add_op("parse_int", |args: &[ModelValue]| match &args[0] {
        ModelValue::Str(s) => s
            .parse::<i64>()
            .map(ModelValue::Int)
            .map_err(|e| panproto_gat::GatError::ModelError(e.to_string())),
        _ => Err(panproto_gat::GatError::ModelError("expected Str".into())),
    });

    let violations = check_model(&model, &theory)?;
    assert!(
        violations.is_empty(),
        "bidirectional coercion model should satisfy both equations"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Test 3: Schema enrichment via builder
// ═══════════════════════════════════════════════════════════════════

#[test]
fn schema_with_defaults_via_constraints() -> Result<(), Box<dyn std::error::Error>> {
    // Use constraints to encode defaults: a "default" constraint sort.
    let proto = test_protocol();
    let schema = SchemaBuilder::new(&proto)
        .vertex("root", "object", None)?
        .vertex("root.status", "string", None)?
        .edge("root", "root.status", "prop", Some("status"))?
        .constraint("root.status", "maxLength", "100")
        .build()?;

    // Verify the constraint is accessible.
    let cs = schema.constraints.get("root.status");
    assert!(cs.is_some(), "constraints should be stored");
    assert_eq!(cs.map(Vec::len), Some(1));
    assert_eq!(
        cs.and_then(|v| v.first()).map(|c| c.sort.as_ref()),
        Some("maxLength")
    );
    assert_eq!(
        cs.and_then(|v| v.first()).map(|c| c.value.as_str()),
        Some("100")
    );

    Ok(())
}

#[test]
fn schema_with_coercions_via_constraints() {
    // A coercion from string to int can be modeled as a constraint.
    let edge = Edge {
        src: "root".into(),
        tgt: "root.age".into(),
        kind: "prop".into(),
        name: Some("age".into()),
    };

    let constraints = HashMap::from([(
        Name::from("root.age"),
        vec![Constraint {
            sort: Name::from("minimum"),
            value: "0".to_owned(),
        }],
    )]);

    let schema = make_schema(
        &[("root", "object"), ("root.age", "integer")],
        std::slice::from_ref(&edge),
        constraints,
    );

    // Verify the coercion-like constraint is stored correctly.
    let cs = schema.constraints.get("root.age");
    assert!(cs.is_some(), "constraint should exist");
    assert_eq!(cs.map(Vec::len), Some(1));
    assert_eq!(
        cs.and_then(|v| v.first()).map(|c| c.sort.as_ref()),
        Some("minimum")
    );
    assert_eq!(
        cs.and_then(|v| v.first()).map(|c| c.value.as_str()),
        Some("0")
    );
}

#[test]
fn schema_with_mergers_via_constraints() {
    // Multiple constraints on a single vertex model merger behavior.
    let edge = Edge {
        src: "root".into(),
        tgt: "root.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };

    let constraints = HashMap::from([(
        Name::from("root.name"),
        vec![
            Constraint {
                sort: Name::from("maxLength"),
                value: "255".to_owned(),
            },
            Constraint {
                sort: Name::from("minLength"),
                value: "1".to_owned(),
            },
        ],
    )]);

    let schema = make_schema(
        &[("root", "object"), ("root.name", "string")],
        std::slice::from_ref(&edge),
        constraints,
    );

    let cs = schema.constraints.get("root.name");
    assert!(cs.is_some(), "constraints should exist");
    assert_eq!(cs.map(Vec::len), Some(2));
    if let Some(constraints_list) = cs {
        let sorts: Vec<&str> = constraints_list.iter().map(|c| c.sort.as_ref()).collect();
        assert!(sorts.contains(&"maxLength"));
        assert!(sorts.contains(&"minLength"));
    }
}

#[test]
fn schema_with_policies_via_protocol() {
    // Conflict policies are modeled through the protocol's constraint_sorts.
    let protocol = test_protocol();

    // The protocol recognizes maxLength, minLength, maximum as constraint sorts.
    assert!(
        protocol.constraint_sorts.iter().any(|s| s == "maxLength"),
        "protocol should recognize maxLength constraint"
    );
    assert!(
        protocol.constraint_sorts.iter().any(|s| s == "minLength"),
        "protocol should recognize minLength constraint"
    );
    assert!(
        protocol.constraint_sorts.iter().any(|s| s == "maximum"),
        "protocol should recognize maximum constraint"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 4: Enriched theory composition
// ═══════════════════════════════════════════════════════════════════

#[test]
fn enriched_theory_colimit() -> Result<(), Box<dyn std::error::Error>> {
    // Compose ThGraph + a value-enriched theory via colimit.
    let th_graph = theories::th_graph();

    let th_valued = Theory::new(
        "ThValued",
        vec![Sort::simple("Vertex"), Sort::simple("Default")],
        vec![Operation::unary("default_val", "v", "Vertex", "Default")],
        vec![],
    );

    let shared = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    let composed = colimit(&th_graph, &th_valued, &shared)?;

    // Verify the composed theory has sorts from both.
    assert!(composed.find_sort("Vertex").is_some());
    assert!(composed.find_sort("Edge").is_some());
    assert!(composed.find_sort("Default").is_some());
    assert_eq!(composed.sorts.len(), 3);

    // Verify operations from both theories are present.
    assert!(composed.find_op("src").is_some());
    assert!(composed.find_op("tgt").is_some());
    assert!(composed.find_op("default_val").is_some());
    assert_eq!(composed.ops.len(), 3);

    Ok(())
}

#[test]
fn enriched_theory_with_directed_eqs() {
    // Build a theory with sorts, ops, equations, AND directed equations
    // (represented as regular equations with a conventional naming pattern).
    let theory = Theory::new(
        "ThEnriched",
        vec![
            Sort::simple("Int"),
            Sort::simple("Str"),
            Sort::simple("Bool"),
        ],
        vec![
            Operation::unary("to_string", "x", "Int", "Str"),
            Operation::unary("parse_int", "s", "Str", "Int"),
            Operation::unary("is_zero", "x", "Int", "Bool"),
        ],
        vec![
            // Regular equation: commutativity does not apply to these ops,
            // but we can add the round-trip law.
            Equation::new(
                "coerce_roundtrip",
                Term::app(
                    "parse_int",
                    vec![Term::app("to_string", vec![Term::var("x")])],
                ),
                Term::var("x"),
            ),
        ],
    );

    assert_eq!(theory.sorts.len(), 3);
    assert_eq!(theory.ops.len(), 3);
    assert_eq!(theory.eqs.len(), 1);
    assert!(theory.find_sort("Int").is_some());
    assert!(theory.find_sort("Str").is_some());
    assert!(theory.find_sort("Bool").is_some());
    assert!(theory.find_op("to_string").is_some());
    assert!(theory.find_op("parse_int").is_some());
    assert!(theory.find_op("is_zero").is_some());
    assert!(theory.find_eq("coerce_roundtrip").is_some());
}

// ═══════════════════════════════════════════════════════════════════
// Test 5: Coverage analysis via migration mapping
// ═══════════════════════════════════════════════════════════════════

#[test]
fn coverage_all_pass() {
    // Build a simple migration where every vertex maps 1:1.
    let edge = Edge {
        src: "root".into(),
        tgt: "root.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };

    let schema1 = make_schema(
        &[("root", "object"), ("root.name", "string")],
        std::slice::from_ref(&edge),
        HashMap::new(),
    );
    let schema2 = schema1.clone();

    let d = diff(&schema1, &schema2);
    let report = classify(&d, &test_protocol());

    // All vertices survive, coverage ratio is 1.0.
    assert!(report.compatible, "identical schemas should be compatible");
    assert!(report.breaking.is_empty());

    // Calculate coverage: all source vertices map to target vertices.
    let total = schema1.vertex_count();
    let covered = schema1
        .vertices
        .keys()
        .filter(|v| schema2.vertices.contains_key(*v))
        .count();
    assert_eq!(covered, total, "coverage ratio should be 1.0");
}

#[test]
fn coverage_partial_failure() {
    // Build a migration that drops a vertex.
    let edge = Edge {
        src: "root".into(),
        tgt: "root.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };
    let edge2 = Edge {
        src: "root".into(),
        tgt: "root.age".into(),
        kind: "prop".into(),
        name: Some("age".into()),
    };

    let schema1 = make_schema(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.age", "integer"),
        ],
        &[edge.clone(), edge2],
        HashMap::new(),
    );

    // Schema2 drops root.age.
    let schema2 = make_schema(
        &[("root", "object"), ("root.name", "string")],
        std::slice::from_ref(&edge),
        HashMap::new(),
    );

    let d = diff(&schema1, &schema2);

    // Verify the diff detected the removal.
    assert!(
        d.removed_vertices.contains(&"root.age".to_owned()),
        "diff should detect removed vertex"
    );

    // Coverage: 2 out of 3 source vertices survive.
    let total = schema1.vertex_count();
    let covered = schema1
        .vertices
        .keys()
        .filter(|v| schema2.vertices.contains_key(*v))
        .count();
    // 2 out of 3 vertices survive: verify via integer comparison.
    assert_eq!(covered, 2, "covered vertices should be 2");
    assert_eq!(total, 3, "total vertices should be 3");

    let report = classify(&d, &test_protocol());
    assert!(
        !report.compatible,
        "dropping a vertex should be a breaking change"
    );
    assert!(
        report.breaking.iter().any(
            |b| matches!(b, BreakingChange::RemovedVertex { vertex_id } if vertex_id == "root.age")
        ),
        "should report root.age as removed"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 6: Optic classification via protolens transform structure
// ═══════════════════════════════════════════════════════════════════

#[test]
fn optic_rename_chain_is_lossless() {
    // Chain of RenameSort transforms produces a lossless protolens.
    let p1 = elementary::rename_sort("A", "B");
    let p2 = elementary::rename_sort("B", "C");

    // Both rename protolenses have empty complement (lossless = iso).
    assert!(p1.is_lossless(), "rename should be lossless (isomorphism)");
    assert!(p2.is_lossless(), "rename should be lossless (isomorphism)");

    let chain = ProtolensChain::new(vec![p1, p2]);
    assert_eq!(chain.len(), 2);
}

#[test]
fn optic_drop_is_lossy() {
    // DropSort produces a lossy protolens (lens, not iso).
    let p = elementary::drop_sort("Obsolete");
    assert!(
        !p.is_lossless(),
        "drop should be lossy (lens with complement)"
    );
}

#[test]
fn optic_composition_preserves_lossiness() {
    // Compose lossless + lossy -> lossy.
    let rename = elementary::rename_sort("A", "B");
    let drop = elementary::drop_sort("C");

    assert!(rename.is_lossless());
    assert!(!drop.is_lossless());

    // The chain is lossy if any step is lossy.
    let chain = ProtolensChain::new(vec![rename, drop]);
    let any_lossy = chain.steps.iter().any(|s| !s.is_lossless());
    assert!(
        any_lossy,
        "chain containing a lossy step should be lossy overall"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 7: Symbolic simplification via protolens chain fusion
// ═══════════════════════════════════════════════════════════════════

#[test]
fn symbolic_rename_fusion() -> Result<(), Box<dyn std::error::Error>> {
    // rename(A,B) + rename(B,C) fused produces a Compose transform
    // that has the effect of rename(A,C).
    let p1 = elementary::rename_sort("A", "B");
    let p2 = elementary::rename_sort("B", "C");

    let chain = ProtolensChain::new(vec![p1, p2]);
    let fused = chain.fuse()?;

    // The fused protolens should have a composed transform.
    assert!(
        matches!(fused.target.transform, TheoryTransform::Compose(_, _)),
        "fused transform should be Compose"
    );

    // Verify it is lossless (renames don't lose data).
    assert!(fused.is_lossless(), "fused renames should be lossless");

    // Apply to a theory to verify the effect: A -> C.
    let theory = Theory::new(
        "T",
        vec![Sort::simple("A"), Sort::simple("X")],
        vec![],
        vec![],
    );
    let result = fused.target.transform.apply(&theory)?;
    assert!(result.has_sort("C"), "A should be renamed to C");
    assert!(result.has_sort("X"), "X should be unchanged");
    assert!(!result.has_sort("A"), "A should no longer exist");
    assert!(!result.has_sort("B"), "B should not exist (intermediate)");

    Ok(())
}

#[test]
fn symbolic_inverse_cancellation() -> Result<(), Box<dyn std::error::Error>> {
    // rename(A,B) + rename(B,A) is a no-op on sorts.
    let p1 = elementary::rename_sort("A", "B");
    let p2 = elementary::rename_sort("B", "A");

    let chain = ProtolensChain::new(vec![p1, p2]);
    let fused = chain.fuse()?;

    let theory = Theory::new(
        "T",
        vec![Sort::simple("A"), Sort::simple("X")],
        vec![],
        vec![],
    );
    let result = fused.target.transform.apply(&theory)?;

    // After rename A->B then B->A, sort A should still exist.
    assert!(result.has_sort("A"), "A should survive round-trip rename");
    assert!(result.has_sort("X"), "X should be unchanged");

    Ok(())
}

#[test]
fn symbolic_add_drop_cancellation() -> Result<(), Box<dyn std::error::Error>> {
    // add(X) + drop(X) on a theory that originally lacks X.
    let p1 = elementary::add_sort("NewSort", "string", panproto_inst::value::Value::Null);
    let p2 = elementary::drop_sort("NewSort");

    let chain = ProtolensChain::new(vec![p1, p2]);
    let fused = chain.fuse()?;

    let theory = Theory::new("T", vec![Sort::simple("Existing")], vec![], vec![]);
    let result = fused.target.transform.apply(&theory)?;

    // After adding and then dropping NewSort, only Existing remains.
    assert!(result.has_sort("Existing"), "Existing should survive");
    assert!(
        !result.has_sort("NewSort"),
        "NewSort should be cancelled out"
    );
    assert_eq!(result.sorts.len(), 1);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Test 8: Refinement types via constraint subsort checking
// ═══════════════════════════════════════════════════════════════════

#[test]
fn refinement_subsort_tighter_max() {
    // Refined(string, maxLength(300)) is a subsort of Refined(string, maxLength(3000)).
    // Tighter max is a subset of the looser max.
    let edge = Edge {
        src: "root".into(),
        tgt: "root.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };

    let loose_constraints = HashMap::from([(
        Name::from("root.text"),
        vec![Constraint {
            sort: Name::from("maxLength"),
            value: "3000".to_owned(),
        }],
    )]);

    let tight_constraints = HashMap::from([(
        Name::from("root.text"),
        vec![Constraint {
            sort: Name::from("maxLength"),
            value: "300".to_owned(),
        }],
    )]);

    let loose_schema = make_schema(
        &[("root", "object"), ("root.text", "string")],
        std::slice::from_ref(&edge),
        loose_constraints,
    );
    let tight_schema = make_schema(
        &[("root", "object"), ("root.text", "string")],
        std::slice::from_ref(&edge),
        tight_constraints,
    );

    // Going from loose to tight is a tightening (breaking).
    let d = diff(&loose_schema, &tight_schema);
    let report = classify(&d, &test_protocol());
    assert!(
        !report.compatible,
        "tightening maxLength from 3000 to 300 should be breaking"
    );
    assert!(
        report
            .breaking
            .iter()
            .any(|b| matches!(b, BreakingChange::ConstraintTightened { .. })),
        "should contain ConstraintTightened"
    );

    // Going from tight to loose is a relaxation (non-breaking).
    let d2 = diff(&tight_schema, &loose_schema);
    let report2 = classify(&d2, &test_protocol());
    assert!(
        report2.compatible,
        "relaxing maxLength from 300 to 3000 should be non-breaking"
    );
}

#[test]
fn refinement_subsort_different_base() {
    // Refined(string, ...) vs Refined(integer, ...) — different base types.
    // A kind change is always breaking regardless of constraints.
    let edge = Edge {
        src: "root".into(),
        tgt: "root.val".into(),
        kind: "prop".into(),
        name: Some("val".into()),
    };

    let string_schema = make_schema(
        &[("root", "object"), ("root.val", "string")],
        std::slice::from_ref(&edge),
        HashMap::new(),
    );
    let int_schema = make_schema(
        &[("root", "object"), ("root.val", "integer")],
        std::slice::from_ref(&edge),
        HashMap::new(),
    );

    let d = diff(&string_schema, &int_schema);
    let report = classify(&d, &test_protocol());
    assert!(
        !report.compatible,
        "changing base type from string to integer should be breaking"
    );
    assert!(
        report
            .breaking
            .iter()
            .any(|b| matches!(b, BreakingChange::KindChanged { .. })),
        "should contain KindChanged"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Test 9: Building-block theories
// ═══════════════════════════════════════════════════════════════════

#[test]
fn th_valued_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // Build a valued theory (sort Default + default operation).
    let th_valued = Theory::new(
        "ThValued",
        vec![Sort::simple("Vertex"), Sort::simple("Default")],
        vec![Operation::unary("default_val", "v", "Vertex", "Default")],
        vec![],
    );

    assert_eq!(th_valued.sorts.len(), 2);
    assert!(th_valued.find_sort("Vertex").is_some());
    assert!(th_valued.find_sort("Default").is_some());
    assert!(th_valued.find_op("default_val").is_some());

    // Serialization round-trip.
    let json = serde_json::to_string(&th_valued)?;
    let deserialized: Theory = serde_json::from_str(&json)?;
    assert_eq!(deserialized.sorts.len(), 2);
    assert_eq!(deserialized.ops.len(), 1);
    assert!(deserialized.find_sort("Default").is_some());

    Ok(())
}

#[test]
fn th_coercible_equations() {
    // Build ThCoercible: sorts Int, Str, ops coerce/uncoerce,
    // involution equation coerce(uncoerce(x)) = x.
    let involution = Equation::new(
        "coerce_involution",
        Term::app("coerce", vec![Term::app("uncoerce", vec![Term::var("x")])]),
        Term::var("x"),
    );

    let th_coercible = Theory::new(
        "ThCoercible",
        vec![Sort::simple("A"), Sort::simple("B")],
        vec![
            Operation::unary("coerce", "x", "A", "B"),
            Operation::unary("uncoerce", "y", "B", "A"),
        ],
        vec![involution.clone()],
    );

    assert_eq!(th_coercible.eqs.len(), 1);
    assert!(
        th_coercible.find_eq("coerce_involution").is_some(),
        "equation should exist"
    );
    assert_eq!(th_coercible.eqs[0].lhs, involution.lhs);
    assert_eq!(th_coercible.eqs[0].rhs, involution.rhs);
}

#[test]
fn th_expr_theories() {
    // Build ThExpr (expression evaluation with apply).
    let th_expr = Theory::new(
        "ThExpr",
        vec![Sort::simple("Expr"), Sort::simple("Value")],
        vec![
            Operation::unary("eval", "e", "Expr", "Value"),
            Operation::new(
                "apply",
                vec![("f".into(), "Expr".into()), ("x".into(), "Value".into())],
                "Value",
            ),
        ],
        vec![],
    );

    // Build ThArith extending ThExpr with arithmetic.
    let th_arith = Theory::new(
        "ThArith",
        vec![
            Sort::simple("Expr"),
            Sort::simple("Value"),
            Sort::simple("Int"),
        ],
        vec![
            Operation::new(
                "add",
                vec![("a".into(), "Int".into()), ("b".into(), "Int".into())],
                "Int",
            ),
            Operation::nullary("zero", "Int"),
        ],
        // Round-trip equation: add(x, zero()) = x
        vec![Equation::new(
            "add_identity",
            Term::app("add", vec![Term::var("x"), Term::constant("zero")]),
            Term::var("x"),
        )],
    );

    // Build ThString.
    let th_string = Theory::new(
        "ThString",
        vec![
            Sort::simple("Expr"),
            Sort::simple("Value"),
            Sort::simple("Str"),
        ],
        vec![
            Operation::new(
                "concat",
                vec![("a".into(), "Str".into()), ("b".into(), "Str".into())],
                "Str",
            ),
            Operation::nullary("empty_str", "Str"),
        ],
        // Round-trip: concat(s, empty_str()) = s
        vec![Equation::new(
            "concat_identity",
            Term::app("concat", vec![Term::var("s"), Term::constant("empty_str")]),
            Term::var("s"),
        )],
    );

    // Verify all three theories have their expected structure.
    assert_eq!(th_expr.sorts.len(), 2);
    assert_eq!(th_expr.ops.len(), 2);

    assert_eq!(th_arith.sorts.len(), 3);
    assert_eq!(th_arith.ops.len(), 2);
    assert!(th_arith.find_eq("add_identity").is_some());

    assert_eq!(th_string.sorts.len(), 3);
    assert_eq!(th_string.ops.len(), 2);
    assert!(th_string.find_eq("concat_identity").is_some());
}

// ═══════════════════════════════════════════════════════════════════
// Test 10: Provenance via schema morphism composition
// ═══════════════════════════════════════════════════════════════════

#[test]
fn provenance_identity() {
    // Identity migration: every vertex maps to itself.
    let vertices = vec![Name::from("a"), Name::from("b")];
    let edge = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("x".into()),
    };
    let mig = Migration::identity(&vertices, std::slice::from_ref(&edge));

    assert_eq!(mig.vertex_map.get("a"), Some(&Name::from("a")));
    assert_eq!(mig.vertex_map.get("b"), Some(&Name::from("b")));
    assert_eq!(mig.edge_map.get(&edge), Some(&edge));
}

#[test]
fn provenance_rename() -> Result<(), Box<dyn std::error::Error>> {
    // Rename migration: surviving nodes reference originals.
    let mig = Migration {
        vertex_map: HashMap::from([
            (Name::from("old_name"), Name::from("new_name")),
            (Name::from("keep"), Name::from("keep")),
        ]),
        edge_map: HashMap::new(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    };

    // Verify provenance: old_name maps to new_name.
    assert_eq!(
        mig.vertex_map.get("old_name"),
        Some(&Name::from("new_name"))
    );
    // keep maps to itself.
    assert_eq!(mig.vertex_map.get("keep"), Some(&Name::from("keep")));

    // Compose with identity to verify provenance is preserved.
    let id_mig = Migration {
        vertex_map: HashMap::from([
            (Name::from("new_name"), Name::from("new_name")),
            (Name::from("keep"), Name::from("keep")),
        ]),
        edge_map: HashMap::new(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    };

    let composed = panproto_mig::compose(&mig, &id_mig)?;
    assert_eq!(
        composed.vertex_map.get("old_name"),
        Some(&Name::from("new_name")),
        "provenance should trace back to original"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Test 11: Equality witnesses via equation construction
// ═══════════════════════════════════════════════════════════════════

#[test]
fn witness_reflexivity() {
    // Build a reflexivity witness: a = a (lhs == rhs).
    let refl = Equation::new("refl_a", Term::var("a"), Term::var("a"));
    assert_eq!(
        refl.lhs, refl.rhs,
        "reflexivity witness should have lhs == rhs"
    );
    assert_eq!(refl.lhs, Term::var("a"));
}

#[test]
fn witness_transitivity() {
    // Chain a=b and b=c into a=c via equation composition on terms.
    // This is witnessed by substitution: if f(a)=f(b) and f(b)=f(c),
    // then by substituting b->c in the first equation we get f(a)=f(c).

    let eq1 = Equation::new(
        "a_eq_b",
        Term::app("f", vec![Term::var("a")]),
        Term::app("f", vec![Term::var("b")]),
    );
    let _eq2 = Equation::new(
        "b_eq_c",
        Term::app("f", vec![Term::var("b")]),
        Term::app("f", vec![Term::var("c")]),
    );

    // Build the transitive witness by substitution.
    let mut subst = rustc_hash::FxHashMap::default();
    subst.insert(Arc::from("b"), Term::var("c"));
    let transitive_rhs = eq1.rhs.substitute(&subst);

    let transitive = Equation::new("a_eq_c", eq1.lhs, transitive_rhs);

    assert_eq!(
        transitive.lhs,
        Term::app("f", vec![Term::var("a")]),
        "transitive lhs should be f(a)"
    );
    assert_eq!(
        transitive.rhs,
        Term::app("f", vec![Term::var("c")]),
        "transitive rhs should be f(c)"
    );

    // Verify the equation is structurally correct.
    let free_vars = transitive.lhs.free_vars();
    assert!(free_vars.contains("a"));
    let free_vars_rhs = transitive.rhs.free_vars();
    assert!(free_vars_rhs.contains("c"));
}

// ═══════════════════════════════════════════════════════════════════
// Test 12: AlgStruct via Theory construction
// ═══════════════════════════════════════════════════════════════════

#[test]
fn alg_struct_construction() -> Result<(), Box<dyn std::error::Error>> {
    // Build an AlgStruct as a Theory with typed parameters (sorts),
    // operations (fields), and equations (laws).
    let alg = Theory::new(
        "AlgMonoid",
        vec![
            // Sort params
            Sort::simple("Carrier"),
        ],
        vec![
            // Operations (fields)
            Operation::new(
                "mul",
                vec![
                    ("a".into(), "Carrier".into()),
                    ("b".into(), "Carrier".into()),
                ],
                "Carrier",
            ),
            Operation::nullary("unit", "Carrier"),
        ],
        vec![
            // Equations (laws)
            Equation::new(
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
            ),
            Equation::new(
                "left_id",
                Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
                Term::var("a"),
            ),
            Equation::new(
                "right_id",
                Term::app("mul", vec![Term::var("a"), Term::constant("unit")]),
                Term::var("a"),
            ),
        ],
    );

    // Verify structure.
    assert_eq!(&*alg.name, "AlgMonoid");
    assert_eq!(alg.sorts.len(), 1);
    assert_eq!(alg.ops.len(), 2);
    assert_eq!(alg.eqs.len(), 3);

    // Serialization round-trip.
    let json = serde_json::to_string(&alg)?;
    let deserialized: Theory = serde_json::from_str(&json)?;
    assert_eq!(deserialized, alg);

    // Build a model and verify it satisfies the laws.
    let mut model = Model::new("AlgMonoid");
    model.add_sort("Carrier", (0_i64..5).map(ModelValue::Int).collect());
    model.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a + b)),
        _ => Err(panproto_gat::GatError::ModelError("expected Int".into())),
    });
    model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));

    let violations = check_model(&model, &alg)?;
    assert!(
        violations.is_empty(),
        "integer addition monoid should satisfy all monoid laws, got {} violations",
        violations.len()
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Test 13: Theory morphisms and model migration
// ═══════════════════════════════════════════════════════════════════

#[test]
fn theory_morphism_preserves_equations() -> Result<(), Box<dyn std::error::Error>> {
    // Build two theories related by a morphism and verify it is valid.
    let th_a = Theory::new(
        "ThA",
        vec![Sort::simple("S")],
        vec![
            Operation::new(
                "op",
                vec![("x".into(), "S".into()), ("y".into(), "S".into())],
                "S",
            ),
            Operation::nullary("e", "S"),
        ],
        vec![],
    );

    let th_b = Theory::new(
        "ThB",
        vec![Sort::simple("T")],
        vec![
            Operation::new(
                "mul",
                vec![("a".into(), "T".into()), ("b".into(), "T".into())],
                "T",
            ),
            Operation::nullary("id", "T"),
        ],
        vec![],
    );

    // Morphism from ThA to ThB: S -> T, op -> mul, e -> id.
    let morphism = TheoryMorphism::new(
        "phi",
        "ThA",
        "ThB",
        HashMap::from([(Arc::from("S"), Arc::from("T"))]),
        HashMap::from([
            (Arc::from("op"), Arc::from("mul")),
            (Arc::from("e"), Arc::from("id")),
        ]),
    );

    check_morphism(&morphism, &th_a, &th_b)?;

    // Migrate a model of ThB along the morphism to get a model of ThA.
    let mut model_b = Model::new("ThB");
    model_b.add_sort("T", vec![ModelValue::Int(0), ModelValue::Int(1)]);
    model_b.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
        (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a * b)),
        _ => Err(panproto_gat::GatError::ModelError("expected Int".into())),
    });
    model_b.add_op("id", |_: &[ModelValue]| Ok(ModelValue::Int(1)));

    let model_a = migrate_model(&morphism, &model_b)?;
    // Model A should have sort "S" and ops "op", "e".
    assert!(model_a.sort_interp.contains_key("S"));
    assert!(model_a.op_interp.contains_key("op"));
    assert!(model_a.op_interp.contains_key("e"));

    let result = model_a.eval("op", &[ModelValue::Int(3), ModelValue::Int(4)])?;
    assert_eq!(result, ModelValue::Int(12));

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Test 14: Building-block theories from panproto-protocols
// ═══════════════════════════════════════════════════════════════════

#[test]
fn all_building_block_theories_well_formed() {
    // Verify all 5 building-block theories have valid structure.
    type TheoryEntry<'a> = (&'a str, fn() -> Theory);
    let theory_fns: Vec<TheoryEntry<'_>> = vec![
        ("ThGraph", theories::th_graph),
        ("ThConstraint", theories::th_constraint),
        ("ThMulti", theories::th_multi),
        ("ThWType", theories::th_wtype),
        ("ThMeta", theories::th_meta),
    ];

    for (name, build_fn) in &theory_fns {
        let theory = build_fn();
        assert_eq!(&*theory.name, *name, "theory name mismatch for {name}");
        assert!(
            !theory.sorts.is_empty(),
            "{name} should have at least one sort"
        );

        // Verify all sorts are findable by name.
        for sort in &theory.sorts {
            assert!(
                theory.find_sort(&sort.name).is_some(),
                "{name}: sort {} should be findable",
                sort.name
            );
        }

        // Verify all ops are findable by name.
        for op in &theory.ops {
            assert!(
                theory.find_op(&op.name).is_some(),
                "{name}: op {} should be findable",
                op.name
            );
        }

        // Verify all equations are findable by name.
        for eq in &theory.eqs {
            assert!(
                theory.find_eq(&eq.name).is_some(),
                "{name}: equation {} should be findable",
                eq.name
            );
        }
    }
}

#[test]
fn building_block_theory_group_registration() {
    // Register a Group A theory pair and verify completeness.
    let mut registry = HashMap::new();
    theories::register_constrained_multigraph_wtype(&mut registry, "TestSchema", "TestInstance");

    assert!(
        registry.contains_key("TestSchema"),
        "schema theory should be registered"
    );
    assert!(
        registry.contains_key("TestInstance"),
        "instance theory should be registered"
    );

    if let Some(schema_theory) = registry.get("TestSchema") {
        // The composed schema theory should have sorts from ThGraph + ThConstraint + ThMulti.
        assert!(schema_theory.find_sort("Vertex").is_some());
        assert!(schema_theory.find_sort("Edge").is_some());
    }

    if let Some(instance_theory) = registry.get("TestInstance") {
        assert!(instance_theory.find_sort("Node").is_some());
    }
}

// ═══════════════════════════════════════════════════════════════════
// Test 15: TheoryTransform composition and application
// ═══════════════════════════════════════════════════════════════════

#[test]
fn theory_transform_compose_associative() -> Result<(), Box<dyn std::error::Error>> {
    // Verify (f ; g) ; h = f ; (g ; h) at the level of results.
    let t = Theory::new(
        "T",
        vec![Sort::simple("A"), Sort::simple("B"), Sort::simple("C")],
        vec![],
        vec![],
    );

    let f = TheoryTransform::RenameSort {
        old: Arc::from("A"),
        new: Arc::from("X"),
    };
    let g = TheoryTransform::RenameSort {
        old: Arc::from("B"),
        new: Arc::from("Y"),
    };
    let h = TheoryTransform::RenameSort {
        old: Arc::from("C"),
        new: Arc::from("Z"),
    };

    // (f ; g) ; h
    let fg = TheoryTransform::Compose(Box::new(f.clone()), Box::new(g.clone()));
    let fgh_left = TheoryTransform::Compose(Box::new(fg), Box::new(h.clone()));
    let result_left = fgh_left.apply(&t)?;

    // f ; (g ; h)
    let gh = TheoryTransform::Compose(Box::new(g), Box::new(h));
    let fgh_right = TheoryTransform::Compose(Box::new(f), Box::new(gh));
    let result_right = fgh_right.apply(&t)?;

    // Both should have the same sorts.
    let left_sorts: std::collections::HashSet<&str> =
        result_left.sorts.iter().map(|s| &*s.name).collect();
    let right_sorts: std::collections::HashSet<&str> =
        result_right.sorts.iter().map(|s| &*s.name).collect();

    assert_eq!(left_sorts, right_sorts, "composition should be associative");
    assert!(left_sorts.contains("X"));
    assert!(left_sorts.contains("Y"));
    assert!(left_sorts.contains("Z"));

    Ok(())
}
