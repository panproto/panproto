//! Integration test 10: CQL subsumption.
//!
//! Verifies that the CQL (Categorical Query Language) operations --
//! Sigma, Delta, and Pi -- can be expressed as theory morphisms and
//! restrict operations within the panproto functor model.
//!
//! CQL's data migration functors:
//! - `Delta_F` (restrict/pullback): precomposition along a functor F
//! - `Sigma_F` (left adjoint): left Kan extension along F
//! - `Pi_F` (right adjoint): right Kan extension along F
//!
//! We verify that `Delta_F` (the core operation) is exactly
//! panproto's `functor_restrict`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use panproto_gat::{Operation, Sort, Theory, TheoryMorphism, check_morphism};
use panproto_inst::value::Value;
use panproto_inst::{BoundaryProtocols, CompiledMigration, FInstance, functor_extend};
use panproto_mig::lift_functor;
use panproto_schema::{Edge, Protocol, ScalarKind, SchemaBuilder};

fn table_protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

const fn protocols(protocol: &Protocol) -> BoundaryProtocols<'_> {
    BoundaryProtocols {
        src: protocol,
        tgt: protocol,
    }
}

fn column(table: &str, name: &str) -> Edge {
    Edge {
        src: table.into(),
        tgt: format!("{table}.{name}").into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

#[test]
fn delta_f_is_functor_restrict() -> Result<(), Box<dyn std::error::Error>> {
    // CQL scenario: Two categories (schemas), a functor between them,
    // and an instance of the target. Delta_F pulls the instance back
    // to the source.

    // Source schema: "employees" table
    // Target schema: "workers" table (renamed)
    // Functor: employees -> workers

    let src_rows = vec![
        HashMap::from([
            ("name".into(), Value::Str("Alice".into())),
            ("dept".into(), Value::Str("Engineering".into())),
        ]),
        HashMap::from([
            ("name".into(), Value::Str("Bob".into())),
            ("dept".into(), Value::Str("Marketing".into())),
        ]),
    ];

    let instance = FInstance::new().with_table("workers", src_rows)?;

    // The "functor" F maps employees -> workers.
    // Delta_F should pull the "workers" table back to "employees".
    let protocol = table_protocol();
    let worker_name = column("workers", "name");
    let worker_dept = column("workers", "dept");
    let employee_name = column("employees", "name");
    let employee_dept = column("employees", "dept");
    let src_schema = SchemaBuilder::new(&protocol)
        .vertex("workers", "record", None)?
        .vertex("workers.name", "string", None)?
        .vertex("workers.dept", "string", None)?
        .edge("workers", "workers.name", "prop", Some("name"))?
        .edge("workers", "workers.dept", "prop", Some("dept"))?
        .build()?;
    let tgt_schema = SchemaBuilder::new(&protocol)
        .vertex("employees", "record", None)?
        .vertex("employees.name", "string", None)?
        .vertex("employees.dept", "string", None)?
        .edge("employees", "employees.name", "prop", Some("name"))?
        .edge("employees", "employees.dept", "prop", Some("dept"))?
        .build()?;
    let compiled = CompiledMigration {
        surviving_verts: HashSet::from([
            "employees".into(),
            "employees.name".into(),
            "employees.dept".into(),
        ]),
        surviving_edges: HashSet::from([employee_name.clone(), employee_dept.clone()]),
        vertex_remap: HashMap::from([
            ("workers".into(), "employees".into()),
            ("workers.name".into(), "employees.name".into()),
            ("workers.dept".into(), "employees.dept".into()),
        ]),
        edge_remap: HashMap::from([(worker_name, employee_name), (worker_dept, employee_dept)]),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let restricted = lift_functor(
        &compiled,
        &src_schema,
        &tgt_schema,
        &instance,
        protocols(&protocol),
    )?;

    // Delta_F should produce an instance with "employees" table
    // containing the same rows as "workers".
    assert_eq!(restricted.table_count(), 1);
    assert!(restricted.has_table("employees"));
    assert_eq!(restricted.row_count("employees"), 2);

    Ok(())
}

#[test]
fn theory_morphism_as_schema_functor() -> Result<(), Box<dyn std::error::Error>> {
    // A theory morphism between ThGraph instances represents a
    // functor between the corresponding category presentations.
    // This is the CQL notion of a "schema mapping".

    // Source theory: sort Employee, sort Department
    // Target theory: sort Worker, sort Team
    // Morphism: Employee -> Worker, Department -> Team

    let source = Theory::new(
        "EmployeeSchema",
        vec![Sort::simple("Employee"), Sort::simple("Department")],
        vec![Operation::unary("works_in", "e", "Employee", "Department")],
        vec![],
    );

    let target = Theory::new(
        "WorkerSchema",
        vec![Sort::simple("Worker"), Sort::simple("Team")],
        vec![Operation::unary("assigned_to", "w", "Worker", "Team")],
        vec![],
    );

    let sort_map = HashMap::from([
        ("Employee".into(), "Worker".into()),
        ("Department".into(), "Team".into()),
    ]);
    let op_map: HashMap<std::sync::Arc<str>, std::sync::Arc<str>> =
        HashMap::from([("works_in".into(), "assigned_to".into())]);

    let morphism = TheoryMorphism::new(
        "schema_mapping",
        "EmployeeSchema",
        "WorkerSchema",
        sort_map,
        op_map,
    );

    check_morphism(&morphism, &source, &target)?;

    Ok(())
}

#[test]
fn functor_restrict_with_fk_is_delta() -> Result<(), Box<dyn std::error::Error>> {
    // A more complete CQL scenario with foreign keys.
    //
    // Source: employees --works_in--> departments
    // Target: staff (= employees, departments merged)
    // Delta: pull staff back into employees + departments

    let fk_edge = Edge {
        src: "employees".into(),
        tgt: "departments".into(),
        kind: "fk".into(),
        name: Some("works_in".into()),
    };

    let emp_rows = vec![
        HashMap::from([("name".into(), Value::Str("Alice".into()))]),
        HashMap::from([("name".into(), Value::Str("Bob".into()))]),
    ];
    let dept_rows = vec![HashMap::from([(
        "name".into(),
        Value::Str("Engineering".into()),
    )])];

    // Employees are rows 0 and 1, the department row 2.
    let instance = FInstance::new()
        .with_table("employees", emp_rows)?
        .with_table("departments", dept_rows)?
        .with_foreign_key(
            fk_edge.clone(),
            [
                (panproto_inst::RowId(0), panproto_inst::RowId(2)),
                (panproto_inst::RowId(1), panproto_inst::RowId(2)),
            ],
        )?;

    let protocol = table_protocol();
    let employee_name = column("employees", "name");
    let department_name = column("departments", "name");
    let schema = SchemaBuilder::new(&protocol)
        .vertex("employees", "record", None)?
        .vertex("employees.name", "string", None)?
        .vertex("departments", "record", None)?
        .vertex("departments.name", "string", None)?
        .edge("employees", "employees.name", "prop", Some("name"))?
        .edge("departments", "departments.name", "prop", Some("name"))?
        .edge("employees", "departments", "fk", Some("works_in"))?
        .build()?;

    // Identity migration: keep both tables.
    let compiled = CompiledMigration {
        surviving_verts: HashSet::from([
            "employees".into(),
            "employees.name".into(),
            "departments".into(),
            "departments.name".into(),
        ]),
        surviving_edges: HashSet::from([fk_edge.clone(), employee_name, department_name]),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let restricted = lift_functor(&compiled, &schema, &schema, &instance, protocols(&protocol))?;
    assert_eq!(restricted.table_count(), 2, "both tables should survive");
    assert_eq!(restricted.row_count("employees"), 2);
    assert_eq!(restricted.row_count("departments"), 1);
    assert_eq!(
        restricted.pairs(&fk_edge).len(),
        2,
        "FK should be preserved"
    );

    Ok(())
}

#[test]
fn sigma_as_theory_morphism_left_adjoint() -> Result<(), Box<dyn std::error::Error>> {
    // Sigma_F (left Kan extension) for set-valued functors corresponds
    // to "copying data forward" along a theory morphism.
    //
    // Currently panproto only implements Delta (restrict). Sigma would
    // require a `functor_extend` operation. This test verifies the
    // theory-level structure is in place for when Sigma is implemented.

    let source = Theory::new("Small", vec![Sort::simple("A")], vec![], vec![]);

    let target = Theory::new(
        "Big",
        vec![Sort::simple("A"), Sort::simple("B")],
        vec![],
        vec![],
    );

    // Inclusion morphism: Small -> Big.
    let morphism = TheoryMorphism::new(
        "include",
        "Small",
        "Big",
        HashMap::from([("A".into(), "A".into())]),
        HashMap::<std::sync::Arc<str>, std::sync::Arc<str>>::new(),
    );

    check_morphism(&morphism, &source, &target)?;

    // Verify Sigma_F (functor_extend): extending a Small-instance
    // along this inclusion morphism produces a Big-instance with
    // the A-table copied and B-table empty.
    let a_rows = vec![HashMap::from([(
        "value".into(),
        Value::Str("hello".into()),
    )])];
    let small_instance = FInstance::new().with_table("A", a_rows)?;

    let protocol = table_protocol();
    let a_value = column("A", "value");
    let src_schema = SchemaBuilder::new(&protocol)
        .vertex("A", "record", None)?
        .vertex("A.value", "string", None)?
        .edge("A", "A.value", "prop", Some("value"))?
        .build()?;
    let tgt_schema = SchemaBuilder::new(&protocol)
        .vertex("A", "record", None)?
        .vertex("A.value", "string", None)?
        .vertex("B", "record", None)?
        .edge("A", "A.value", "prop", Some("value"))?
        .build()?;
    let migration = CompiledMigration {
        surviving_verts: HashSet::from(["A".into(), "A.value".into(), "B".into()]),
        surviving_edges: HashSet::from([a_value.clone()]),
        vertex_remap: HashMap::from([
            ("A".into(), "A".into()),
            ("A.value".into(), "A.value".into()),
        ]),
        edge_remap: HashMap::from([(a_value.clone(), a_value)]),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let extended = functor_extend(
        &small_instance,
        &src_schema,
        &tgt_schema,
        &migration,
        protocols(&protocol),
    )?;

    // A table is a vertex with at least one row, so the empty B does not count.
    assert_eq!(extended.table_count(), 1, "only A holds rows");
    assert_eq!(extended.row_count("A"), 1, "A table should have 1 row");
    assert_eq!(extended.row_count("B"), 0, "B table should be empty");

    Ok(())
}
