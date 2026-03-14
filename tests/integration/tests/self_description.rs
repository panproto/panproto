//! Integration test 1: GAT of GATs is a GAT.
//!
//! Verifies the self-description property: `ThGAT` (the theory of
//! generalized algebraic theories) is itself a well-formed GAT.
//! Also verifies that the theory of schema theories is a GAT.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    Model, ModelValue, Operation, Sort, SortParam, Theory, TheoryMorphism, check_morphism,
    resolve_theory,
};

/// `ThGAT`: the theory of generalized algebraic theories.
///
/// Sorts: `Sort`, `Op`, `Eq`, `Theory`, `Param(s: Sort)`, `Name`.
/// Operations: `sort_name`, `op_name`, `op_output`, `eq_name`, `theory_name`.
fn th_gat() -> Theory {
    let sort_sort = Sort::simple("Sort");
    let op_sort = Sort::simple("Op");
    let eq_sort = Sort::simple("Eq");
    let theory_sort = Sort::simple("Theory");
    let param_sort = Sort::dependent("Param", vec![SortParam::new("s", "Sort")]);
    let name_sort = Sort::simple("Name");

    let sort_name_op = Operation::unary("sort_name", "s", "Sort", "Name");
    let op_name_op = Operation::unary("op_name", "o", "Op", "Name");
    let op_output_op = Operation::unary("op_output", "o", "Op", "Sort");
    let eq_name_op = Operation::unary("eq_name", "e", "Eq", "Name");
    let theory_name_op = Operation::unary("theory_name", "t", "Theory", "Name");

    Theory::new(
        "ThGAT",
        vec![
            sort_sort,
            op_sort,
            eq_sort,
            theory_sort,
            param_sort,
            name_sort,
        ],
        vec![
            sort_name_op,
            op_name_op,
            op_output_op,
            eq_name_op,
            theory_name_op,
        ],
        Vec::new(),
    )
}

#[test]
fn th_gat_is_well_formed() -> Result<(), Box<dyn std::error::Error>> {
    let gat = th_gat();

    // Verify structural properties.
    assert_eq!(gat.sorts.len(), 6, "ThGAT should have 6 sorts");
    assert_eq!(gat.ops.len(), 5, "ThGAT should have 5 operations");

    // All sorts are findable.
    for name in &["Sort", "Op", "Eq", "Theory", "Param", "Name"] {
        assert!(gat.find_sort(name).is_some(), "sort {name} should exist");
    }

    // Param sort is dependent with arity 1.
    let param = gat.find_sort("Param").ok_or("Param sort not found")?;
    assert_eq!(param.arity(), 1, "Param should have arity 1");
    assert_eq!(&*param.params[0].sort, "Sort", "Param depends on Sort");

    // All operations have correct signatures.
    let sn = gat.find_op("sort_name").ok_or("sort_name not found")?;
    assert_eq!(sn.inputs.len(), 1);
    assert_eq!(&*sn.inputs[0].1, "Sort");
    assert_eq!(&*sn.output, "Name");

    let oo = gat.find_op("op_output").ok_or("op_output not found")?;
    assert_eq!(&*oo.output, "Sort");

    Ok(())
}

#[test]
fn th_gat_resolves_in_registry() -> Result<(), Box<dyn std::error::Error>> {
    let gat = th_gat();
    let mut registry = HashMap::new();
    registry.insert("ThGAT".to_owned(), gat.clone());

    let resolved = resolve_theory("ThGAT", &registry)?;
    assert_eq!(resolved.sorts.len(), gat.sorts.len());
    assert_eq!(resolved.ops.len(), gat.ops.len());

    Ok(())
}

#[test]
fn th_gat_admits_identity_morphism() -> Result<(), Box<dyn std::error::Error>> {
    let gat = th_gat();

    let sort_map: HashMap<Arc<str>, Arc<str>> = gat
        .sorts
        .iter()
        .map(|s| (Arc::clone(&s.name), Arc::clone(&s.name)))
        .collect();
    let op_map: HashMap<Arc<str>, Arc<str>> = gat
        .ops
        .iter()
        .map(|o| (Arc::clone(&o.name), Arc::clone(&o.name)))
        .collect();

    let id_morphism = TheoryMorphism::new("id", "ThGAT", "ThGAT", sort_map, op_map);
    check_morphism(&id_morphism, &gat, &gat)?;

    Ok(())
}

#[test]
fn th_gat_can_model_itself() -> Result<(), Box<dyn std::error::Error>> {
    // Build a model of ThGAT where each sort's carrier set contains
    // the names of the sorts/ops/eqs of ThGAT itself.
    let gat = th_gat();
    let mut model = Model::new("ThGAT");

    // Sort is interpreted as the set of sort names.
    let sort_values: Vec<ModelValue> = gat
        .sorts
        .iter()
        .map(|s| ModelValue::Str(s.name.to_string()))
        .collect();
    model.add_sort("Sort", sort_values.clone());

    // Op is interpreted as the set of operation names.
    let op_values: Vec<ModelValue> = gat
        .ops
        .iter()
        .map(|o| ModelValue::Str(o.name.to_string()))
        .collect();
    model.add_sort("Op", op_values);

    // Eq is interpreted as the set of equation names.
    let eq_values: Vec<ModelValue> = gat
        .eqs
        .iter()
        .map(|e| ModelValue::Str(e.name.to_string()))
        .collect();
    model.add_sort("Eq", eq_values);

    // Theory: singleton set containing "ThGAT".
    model.add_sort("Theory", vec![ModelValue::Str("ThGAT".into())]);

    // Param: empty because ThGAT's self-description has no parameterized sorts.
    model.add_sort("Param", Vec::new());

    // Name: all names.
    let all_names: Vec<ModelValue> = sort_values;
    model.add_sort("Name", all_names);

    // sort_name: Sort -> Name is identity on names.
    model.add_op("sort_name", |args: &[ModelValue]| Ok(args[0].clone()));

    // op_name: Op -> Name is identity on names.
    model.add_op("op_name", |args: &[ModelValue]| Ok(args[0].clone()));

    // op_output: Op -> Sort: map each op name to its output sort name.
    let ops_owned = gat.ops;
    model.add_op("op_output", move |args: &[ModelValue]| {
        if let ModelValue::Str(name) = &args[0] {
            if let Some(op) = ops_owned.iter().find(|o| &*o.name == name.as_str()) {
                return Ok(ModelValue::Str(op.output.to_string()));
            }
        }
        panic!("unexpected input to op_output op: {args:?}")
    });

    // eq_name: Eq -> Name is identity on names.
    model.add_op("eq_name", |args: &[ModelValue]| Ok(args[0].clone()));

    // theory_name: Theory -> Name is identity.
    model.add_op("theory_name", |args: &[ModelValue]| Ok(args[0].clone()));

    // Verify operations work.
    let result = model.eval("sort_name", &[ModelValue::Str("Sort".into())])?;
    assert_eq!(result, ModelValue::Str("Sort".into()));

    let result = model.eval("op_output", &[ModelValue::Str("sort_name".into())])?;
    assert_eq!(result, ModelValue::Str("Name".into()));

    let result = model.eval("theory_name", &[ModelValue::Str("ThGAT".into())])?;
    assert_eq!(result, ModelValue::Str("ThGAT".into()));

    Ok(())
}

#[test]
fn theory_of_schema_theories_is_a_gat() -> Result<(), Box<dyn std::error::Error>> {
    // The schema theory for ATProto is built via colimit, which is a GAT
    // operation. Verify it produces a well-formed theory.
    let mut registry = HashMap::new();
    panproto_protocols::atproto::register_theories(&mut registry);

    let schema_theory = registry
        .get("ThATProtoSchema")
        .ok_or("ThATProtoSchema not found in registry")?;

    // Verify it has the expected sorts from the colimit components.
    assert!(
        schema_theory.find_sort("Vertex").is_some(),
        "schema theory should have Vertex sort"
    );
    assert!(
        schema_theory.find_sort("Edge").is_some(),
        "schema theory should have Edge sort"
    );
    assert!(
        schema_theory.find_sort("Constraint").is_some(),
        "schema theory should have Constraint sort"
    );

    // Verify it has operations from all component theories.
    assert!(
        schema_theory.find_op("src").is_some(),
        "should have src from ThGraph"
    );
    assert!(
        schema_theory.find_op("tgt").is_some(),
        "should have tgt from ThGraph"
    );
    assert!(
        schema_theory.find_op("target").is_some(),
        "should have target from ThConstraint"
    );

    Ok(())
}
