#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Exact O16 witness ledger for the eight relational adjunction maps.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::migration::{self, Migration};
use panproto_reference::model::{GEdge, Graph, Node, SEdge, Table, Tree};
use panproto_reference::rel::{
    self, AdjunctionError, Conformance, FunctorError, Hom, RelFunctor, RelSchema, RelSchemaError,
    TransposeError,
};
use panproto_reference::schema::Schema;
use panproto_reference::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum O16Path {
    SigmaUnit,
    SigmaCounit,
    SigmaTransposeForward,
    SigmaTransposeInverse,
    PiUnit,
    PiCounit,
    PiTransposeForward,
    PiTransposeInverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum O16Witness {
    Nonidentity,
    LabelledNull,
    Multifibre,
    OutsideImage,
    Relation,
    TypedRefusal,
}

fn row(table: &str, cells: &[(&str, Value)]) -> (String, BTreeMap<String, Value>) {
    (
        table.to_owned(),
        cells
            .iter()
            .map(|(column, value)| ((*column).to_owned(), value.clone()))
            .collect(),
    )
}

fn table(
    rows: impl IntoIterator<Item = (u32, (String, BTreeMap<String, Value>))>,
    pairs: impl IntoIterator<Item = (SEdge, u32, u32)>,
) -> Table {
    Table {
        rows: rows.into_iter().collect(),
        pairs: pairs.into_iter().collect(),
    }
}

fn schema(tables: &[&str], relations: &[SEdge], columns: &[(&str, &str, SEdge)]) -> RelSchema {
    let mut by_table = tables
        .iter()
        .map(|name| ((*name).to_owned(), BTreeMap::new()))
        .collect::<BTreeMap<_, _>>();
    for (table, column, edge) in columns {
        by_table
            .get_mut(*table)
            .expect("fixture column table is declared")
            .insert((*column).to_owned(), edge.clone());
    }
    RelSchema {
        tables: tables.iter().map(|name| (*name).to_owned()).collect(),
        relations: relations.iter().cloned().collect(),
        columns: by_table,
        ..RelSchema::default()
    }
}

fn hom_from_rows(
    domain: &Table,
    codomain: &Table,
    rows: BTreeMap<u32, u32>,
) -> Result<Hom, rel::HomError> {
    let mut nulls = BTreeMap::new();
    for (source, (_, source_cells)) in &domain.rows {
        let target = rows
            .get(source)
            .copied()
            .ok_or(rel::HomError::NodeNotMapped(*source))?;
        let Some((_, target_cells)) = codomain.rows.get(&target) else {
            return Err(rel::HomError::ImageNodeMissing {
                src: *source,
                tgt: target,
            });
        };
        for (column, source_value) in source_cells {
            let Some(target_value) = target_cells.get(column) else {
                return Err(rel::HomError::AttributeMismatch {
                    src: *source,
                    tgt: target,
                });
            };
            if let Value::LabeledNull(null) = source_value {
                if nulls
                    .insert(*null, target_value.clone())
                    .is_some_and(|prior| prior != *target_value)
                {
                    return Err(rel::HomError::AttributeMismatch {
                        src: *source,
                        tgt: target,
                    });
                }
            } else if source_value != target_value {
                return Err(rel::HomError::AttributeMismatch {
                    src: *source,
                    tgt: target,
                });
            }
        }
    }
    let hom = Hom { rows, nulls };
    hom.check(domain, codomain)?;
    Ok(hom)
}

fn assert_relation_preserved(hom: &Hom, domain: &Table, codomain: &Table) {
    assert!(
        !domain.pairs.is_empty(),
        "relation witness must be inhabited"
    );
    for (edge, source, target) in &domain.pairs {
        let mapped = (
            edge.clone(),
            *hom.rows.get(source).expect("source row is mapped"),
            *hom.rows.get(target).expect("target row is mapped"),
        );
        assert!(
            codomain.pairs.contains(&mapped),
            "relation pair {edge:?}/{source}->{target} maps to {mapped:?}"
        );
    }
}

fn assert_rich_hom(
    actual: Result<Hom, TransposeError>,
    expected: &Hom,
    domain: &Table,
    codomain: &Table,
) {
    let actual = actual.expect("rich fixture is in the operation domain");
    assert_eq!(&actual, expected, "the induced hom has the literal map");
    assert_eq!(actual.check(domain, codomain), Ok(()));
    assert!(
        actual.rows.iter().any(|(source, target)| source != target),
        "the row map is genuinely nonidentity"
    );
    assert!(!actual.nulls.is_empty(), "the exact null map is nonempty");
    assert_relation_preserved(&actual, domain, codomain);
}

struct SigmaRich {
    functor: RelFunctor,
    input: Table,
    sigma_input: Table,
    delta_sigma_input: Table,
    target: Table,
    delta_target: Table,
    sigma_delta_target: Table,
    source_relation: SEdge,
    target_relation: SEdge,
}

fn sigma_target_tables(source_relation: &SEdge, target_relation: &SEdge) -> (Table, Table, Table) {
    let target = table(
        [
            (
                10,
                row("m", &[("x", Value::Int(42)), ("z", Value::LabeledNull(20))]),
            ),
            (
                11,
                row("m", &[("x", Value::Int(5)), ("z", Value::LabeledNull(21))]),
            ),
        ],
        [(target_relation.clone(), 10, 11)],
    );
    let delta_target = table(
        [
            (10, row("a", &[("x", Value::Int(42))])),
            (11, row("a", &[("x", Value::Int(5))])),
            (12, row("b", &[("x", Value::Int(42))])),
            (13, row("b", &[("x", Value::Int(5))])),
        ],
        [(source_relation.clone(), 10, 13)],
    );
    let sigma_delta_target = table(
        [
            (
                10,
                row("m", &[("x", Value::Int(42)), ("z", Value::LabeledNull(0))]),
            ),
            (
                11,
                row("m", &[("x", Value::Int(5)), ("z", Value::LabeledNull(1))]),
            ),
            (
                12,
                row("m", &[("x", Value::Int(42)), ("z", Value::LabeledNull(2))]),
            ),
            (
                13,
                row("m", &[("x", Value::Int(5)), ("z", Value::LabeledNull(3))]),
            ),
        ],
        [(target_relation.clone(), 10, 13)],
    );
    (target, delta_target, sigma_delta_target)
}

fn sigma_rich() -> SigmaRich {
    let ax = SEdge::new("a", "va", "prop", Some("x"));
    let bx = SEdge::new("b", "vb", "prop", Some("x"));
    let mx = SEdge::new("m", "vmx", "prop", Some("x"));
    let mz = SEdge::new("m", "vmz", "prop", Some("z"));
    let source_relation = SEdge::new("a", "b", "fk", Some("link"));
    let target_relation = SEdge::new("m", "m", "fk", Some("link"));
    let functor = RelFunctor {
        src: schema(
            &["a", "b"],
            std::slice::from_ref(&source_relation),
            &[("a", "x", ax), ("b", "x", bx)],
        ),
        tgt: schema(
            &["m"],
            std::slice::from_ref(&target_relation),
            &[("m", "x", mx), ("m", "z", mz)],
        ),
        tables: BTreeMap::from([
            ("a".to_owned(), "m".to_owned()),
            ("b".to_owned(), "m".to_owned()),
        ]),
        relations: BTreeMap::from([(source_relation.clone(), target_relation.clone())]),
        columns: BTreeMap::from([
            (("a".to_owned(), "x".to_owned()), "x".to_owned()),
            (("b".to_owned(), "x".to_owned()), "x".to_owned()),
        ]),
        inner_edges: BTreeMap::new(),
        value_vertices: BTreeMap::new(),
    };
    let input = table(
        [
            (0, row("a", &[("x", Value::LabeledNull(7))])),
            (1, row("b", &[("x", Value::Int(5))])),
        ],
        [(source_relation.clone(), 0, 1)],
    );
    let sigma_input = table(
        [
            (
                0,
                row(
                    "m",
                    &[("x", Value::LabeledNull(7)), ("z", Value::LabeledNull(8))],
                ),
            ),
            (
                1,
                row("m", &[("x", Value::Int(5)), ("z", Value::LabeledNull(9))]),
            ),
        ],
        [(target_relation.clone(), 0, 1)],
    );
    let delta_sigma_input = table(
        [
            (0, row("a", &[("x", Value::LabeledNull(7))])),
            (1, row("a", &[("x", Value::Int(5))])),
            (2, row("b", &[("x", Value::LabeledNull(7))])),
            (3, row("b", &[("x", Value::Int(5))])),
        ],
        [(source_relation.clone(), 0, 3)],
    );
    let (target, delta_target, sigma_delta_target) =
        sigma_target_tables(&source_relation, &target_relation);
    SigmaRich {
        functor,
        input,
        sigma_input,
        delta_sigma_input,
        target,
        delta_target,
        sigma_delta_target,
        source_relation,
        target_relation,
    }
}

fn record_success_cells(
    ledger: &mut BTreeSet<(O16Path, O16Witness)>,
    paths: &[O16Path],
    witnesses: &[O16Witness],
) {
    for path in paths {
        for witness in witnesses {
            assert!(ledger.insert((*path, *witness)));
        }
    }
}

fn assert_sigma_fixture_scope(fixture: &SigmaRich) {
    assert_eq!(
        rel::conform(&fixture.input, &fixture.functor.src.side()),
        Ok(())
    );
    assert_eq!(
        rel::conform(&fixture.target, &fixture.functor.tgt.side()),
        Ok(())
    );
    assert_eq!(
        fixture
            .functor
            .tables
            .values()
            .filter(|image| *image == "m")
            .count(),
        2,
        "the target table has a genuine two-table fibre"
    );
    assert!(
        fixture.functor.tgt.columns["m"].contains_key("z")
            && !fixture.functor.columns.values().any(|image| image == "z"),
        "Sigma must fill the literal outside-image target column"
    );
    assert_eq!(
        fixture.functor.relations.get(&fixture.source_relation),
        Some(&fixture.target_relation)
    );
}

fn record_sigma_successes(ledger: &mut BTreeSet<(O16Path, O16Witness)>) {
    let fixture = sigma_rich();
    assert_sigma_fixture_scope(&fixture);

    let unit = Hom {
        rows: BTreeMap::from([(0, 0), (1, 3)]),
        nulls: BTreeMap::from([(7, Value::LabeledNull(7))]),
    };
    assert_rich_hom(
        rel::sigma_unit(&fixture.functor, &fixture.input),
        &unit,
        &fixture.input,
        &fixture.delta_sigma_input,
    );

    let counit = Hom {
        rows: BTreeMap::from([(10, 10), (11, 11), (12, 10), (13, 11)]),
        nulls: BTreeMap::from([
            (0, Value::LabeledNull(20)),
            (1, Value::LabeledNull(21)),
            (2, Value::LabeledNull(20)),
            (3, Value::LabeledNull(21)),
        ]),
    };
    assert_rich_hom(
        rel::sigma_counit(&fixture.functor, &fixture.target),
        &counit,
        &fixture.sigma_delta_target,
        &fixture.target,
    );

    let sigma_to_target = hom_from_rows(
        &fixture.sigma_input,
        &fixture.target,
        BTreeMap::from([(0, 10), (1, 11)]),
    )
    .expect("literal Sigma-side hom is valid");
    assert_eq!(
        sigma_to_target.nulls,
        BTreeMap::from([
            (7, Value::Int(42)),
            (8, Value::LabeledNull(20)),
            (9, Value::LabeledNull(21)),
        ])
    );
    let input_to_delta = Hom {
        rows: BTreeMap::from([(0, 10), (1, 13)]),
        nulls: BTreeMap::from([(7, Value::Int(42))]),
    };
    assert_rich_hom(
        rel::sigma_transpose_forward(
            &fixture.functor,
            &fixture.input,
            &fixture.target,
            &sigma_to_target,
        ),
        &input_to_delta,
        &fixture.input,
        &fixture.delta_target,
    );
    assert_rich_hom(
        rel::sigma_transpose_inverse(
            &fixture.functor,
            &fixture.input,
            &fixture.target,
            &input_to_delta,
        ),
        &sigma_to_target,
        &fixture.sigma_input,
        &fixture.target,
    );

    record_success_cells(
        ledger,
        &[
            O16Path::SigmaUnit,
            O16Path::SigmaCounit,
            O16Path::SigmaTransposeForward,
            O16Path::SigmaTransposeInverse,
        ],
        &[
            O16Witness::Nonidentity,
            O16Witness::LabelledNull,
            O16Witness::Multifibre,
            O16Witness::OutsideImage,
            O16Witness::Relation,
        ],
    );
}

struct RefusalFixture {
    functor: RelFunctor,
    good_source: Table,
    bad_source: Table,
    good_target: Table,
    bad_target: Table,
    source_reason: rel::NonConforming,
    target_reason: rel::NonConforming,
}

fn incoherent_pair_fixture() -> RefusalFixture {
    let source_relation = SEdge::new("a", "b", "fk", Some("link"));
    let target_relation = SEdge::new("m", "n", "fk", Some("link"));
    let functor = RelFunctor {
        src: schema(&["a", "b"], std::slice::from_ref(&source_relation), &[]),
        tgt: schema(&["m", "n"], std::slice::from_ref(&target_relation), &[]),
        tables: BTreeMap::from([
            ("a".to_owned(), "m".to_owned()),
            ("b".to_owned(), "n".to_owned()),
        ]),
        relations: BTreeMap::from([(source_relation.clone(), target_relation.clone())]),
        columns: BTreeMap::new(),
        inner_edges: BTreeMap::new(),
        value_vertices: BTreeMap::new(),
    };
    let good_source = table(
        [(0, row("a", &[])), (1, row("b", &[]))],
        [(source_relation.clone(), 0, 1)],
    );
    let bad_source = table(
        [(0, row("a", &[])), (1, row("b", &[]))],
        [(source_relation.clone(), 1, 0)],
    );
    let good_target = table(
        [(10, row("m", &[])), (11, row("n", &[]))],
        [(target_relation.clone(), 10, 11)],
    );
    let bad_target = table(
        [(10, row("m", &[])), (11, row("n", &[]))],
        [(target_relation.clone(), 11, 10)],
    );
    RefusalFixture {
        functor,
        good_source,
        bad_source,
        good_target,
        bad_target,
        source_reason: rel::NonConforming {
            row: 1,
            column: None,
            reason: Conformance::IncoherentPair(Box::new(source_relation)),
        },
        target_reason: rel::NonConforming {
            row: 11,
            column: None,
            reason: Conformance::IncoherentPair(Box::new(target_relation)),
        },
    }
}

fn nonconforming(reason: &rel::NonConforming) -> Result<Hom, TransposeError> {
    Err(TransposeError::Adjunction(Box::new(
        AdjunctionError::NonConforming(Box::new(reason.clone())),
    )))
}

fn record_sigma_refusals(ledger: &mut BTreeSet<(O16Path, O16Witness)>) {
    let fixture = incoherent_pair_fixture();
    assert_eq!(
        rel::conform(&fixture.bad_source, &fixture.functor.src.side()),
        Err(Box::new(fixture.source_reason.clone()))
    );
    assert_eq!(
        rel::conform(&fixture.bad_target, &fixture.functor.tgt.side()),
        Err(Box::new(fixture.target_reason.clone()))
    );
    let invalid_hom = Hom::default();

    assert_eq!(
        rel::sigma_unit(&fixture.functor, &fixture.bad_source),
        nonconforming(&fixture.source_reason)
    );
    assert_eq!(
        rel::sigma_counit(&fixture.functor, &fixture.bad_target),
        nonconforming(&fixture.target_reason)
    );
    assert_eq!(
        rel::sigma_transpose_forward(
            &fixture.functor,
            &fixture.bad_source,
            &fixture.good_target,
            &invalid_hom,
        ),
        nonconforming(&fixture.source_reason),
        "the source-object refusal precedes validation of an invalid Hom"
    );
    assert_eq!(
        rel::sigma_transpose_inverse(
            &fixture.functor,
            &fixture.good_source,
            &fixture.bad_target,
            &invalid_hom,
        ),
        nonconforming(&fixture.target_reason),
        "the target-object refusal precedes validation of an invalid Hom"
    );

    for path in [
        O16Path::SigmaUnit,
        O16Path::SigmaCounit,
        O16Path::SigmaTransposeForward,
        O16Path::SigmaTransposeInverse,
    ] {
        assert!(ledger.insert((path, O16Witness::TypedRefusal)));
    }
}

struct PiRich {
    functor: RelFunctor,
    input: Table,
    pi_input: Table,
    delta_pi_input: Table,
    target: Table,
    delta_target: Table,
    pi_delta_target: Table,
    source_relation: SEdge,
    target_relation: SEdge,
}

fn pi_rich() -> PiRich {
    let ax = SEdge::new("a", "va", "prop", Some("x"));
    let bx = SEdge::new("b", "vb", "prop", Some("x"));
    let mx = SEdge::new("m", "vm", "prop", Some("x"));
    let source_relation = SEdge::new("a", "b", "fk", Some("link"));
    let target_relation = SEdge::new("m", "m", "fk", Some("link"));
    let functor = RelFunctor {
        src: schema(
            &["a", "b"],
            std::slice::from_ref(&source_relation),
            &[("a", "x", ax), ("b", "x", bx)],
        ),
        tgt: schema(
            &["m"],
            std::slice::from_ref(&target_relation),
            &[("m", "x", mx)],
        ),
        tables: BTreeMap::from([
            ("a".to_owned(), "m".to_owned()),
            ("b".to_owned(), "m".to_owned()),
        ]),
        relations: BTreeMap::from([(source_relation.clone(), target_relation.clone())]),
        columns: BTreeMap::from([
            (("a".to_owned(), "x".to_owned()), "x".to_owned()),
            (("b".to_owned(), "x".to_owned()), "x".to_owned()),
        ]),
        inner_edges: BTreeMap::new(),
        value_vertices: BTreeMap::new(),
    };
    let input = table(
        [
            (0, row("a", &[("x", Value::LabeledNull(7))])),
            (1, row("b", &[("x", Value::LabeledNull(7))])),
        ],
        [(source_relation.clone(), 0, 1)],
    );
    let pi_input = table(
        [(2, row("m", &[("x", Value::LabeledNull(7))]))],
        [(target_relation.clone(), 2, 2)],
    );
    let delta_pi_input = table(
        [
            (2, row("a", &[("x", Value::LabeledNull(7))])),
            (3, row("b", &[("x", Value::LabeledNull(7))])),
        ],
        [(source_relation.clone(), 2, 3)],
    );
    let target = table(
        [(10, row("m", &[("x", Value::LabeledNull(20))]))],
        [(target_relation.clone(), 10, 10)],
    );
    let delta_target = table(
        [
            (10, row("a", &[("x", Value::LabeledNull(20))])),
            (11, row("b", &[("x", Value::LabeledNull(20))])),
        ],
        [(source_relation.clone(), 10, 11)],
    );
    let pi_delta_target = table(
        [(12, row("m", &[("x", Value::LabeledNull(20))]))],
        [(target_relation.clone(), 12, 12)],
    );
    PiRich {
        functor,
        input,
        pi_input,
        delta_pi_input,
        target,
        delta_target,
        pi_delta_target,
        source_relation,
        target_relation,
    }
}

fn record_pi_successes(ledger: &mut BTreeSet<(O16Path, O16Witness)>) {
    let fixture = pi_rich();
    assert_eq!(
        rel::conform(&fixture.input, &fixture.functor.src.side()),
        Ok(())
    );
    assert_eq!(
        rel::conform(&fixture.target, &fixture.functor.tgt.side()),
        Ok(())
    );
    assert_eq!(
        fixture
            .functor
            .tables
            .values()
            .filter(|image| *image == "m")
            .count(),
        2
    );
    assert_eq!(
        fixture.functor.relations.get(&fixture.source_relation),
        Some(&fixture.target_relation)
    );

    let unit = Hom {
        rows: BTreeMap::from([(10, 12)]),
        nulls: BTreeMap::from([(20, Value::LabeledNull(20))]),
    };
    assert_rich_hom(
        rel::pi_unit(&fixture.functor, &fixture.target, 4),
        &unit,
        &fixture.target,
        &fixture.pi_delta_target,
    );

    let counit = Hom {
        rows: BTreeMap::from([(2, 0), (3, 1)]),
        nulls: BTreeMap::from([(7, Value::LabeledNull(7))]),
    };
    assert_rich_hom(
        rel::pi_counit(&fixture.functor, &fixture.input, 4),
        &counit,
        &fixture.delta_pi_input,
        &fixture.input,
    );

    let delta_to_input = hom_from_rows(
        &fixture.delta_target,
        &fixture.input,
        BTreeMap::from([(10, 0), (11, 1)]),
    )
    .expect("literal Delta-side hom is valid");
    assert_eq!(
        delta_to_input.nulls,
        BTreeMap::from([(20, Value::LabeledNull(7))])
    );
    let target_to_pi = Hom {
        rows: BTreeMap::from([(10, 2)]),
        nulls: BTreeMap::from([(20, Value::LabeledNull(7))]),
    };
    assert_rich_hom(
        rel::pi_transpose_forward(
            &fixture.functor,
            &fixture.target,
            &fixture.input,
            &delta_to_input,
            4,
        ),
        &target_to_pi,
        &fixture.target,
        &fixture.pi_input,
    );
    assert_rich_hom(
        rel::pi_transpose_inverse(
            &fixture.functor,
            &fixture.target,
            &fixture.input,
            &target_to_pi,
            4,
        ),
        &delta_to_input,
        &fixture.delta_target,
        &fixture.input,
    );

    record_success_cells(
        ledger,
        &[
            O16Path::PiUnit,
            O16Path::PiCounit,
            O16Path::PiTransposeForward,
            O16Path::PiTransposeInverse,
        ],
        &[
            O16Witness::Nonidentity,
            O16Witness::LabelledNull,
            O16Witness::Multifibre,
            O16Witness::Relation,
        ],
    );
}

struct OutsideImageFixture {
    functor: RelFunctor,
    input: Table,
    target: Table,
    delta_target: Table,
}

fn outside_image_fixture() -> OutsideImageFixture {
    let ax = SEdge::new("a", "va", "prop", Some("x"));
    let mx = SEdge::new("m", "vmx", "prop", Some("x"));
    let mz = SEdge::new("m", "vmz", "prop", Some("z"));
    let functor = RelFunctor {
        src: schema(&["a"], &[], &[("a", "x", ax)]),
        tgt: schema(&["m"], &[], &[("m", "x", mx), ("m", "z", mz)]),
        tables: BTreeMap::from([("a".to_owned(), "m".to_owned())]),
        relations: BTreeMap::new(),
        columns: BTreeMap::from([(("a".to_owned(), "x".to_owned()), "x".to_owned())]),
        inner_edges: BTreeMap::new(),
        value_vertices: BTreeMap::new(),
    };
    OutsideImageFixture {
        functor,
        input: table([(0, row("a", &[("x", Value::Int(1))]))], []),
        target: table(
            [(10, row("m", &[("x", Value::Int(1)), ("z", Value::Int(2))]))],
            [],
        ),
        delta_target: table([(10, row("a", &[("x", Value::Int(1))]))], []),
    }
}

fn outside_image() -> Result<Hom, TransposeError> {
    Err(TransposeError::Adjunction(Box::new(
        AdjunctionError::AttributeOutsideImage {
            table: "m".to_owned(),
            column: "z".to_owned(),
        },
    )))
}

fn assert_pi_outside_image_refusals() {
    let fixture = outside_image_fixture();
    let valid_hom = hom_from_rows(
        &fixture.delta_target,
        &fixture.input,
        BTreeMap::from([(10, 0)]),
    )
    .expect("literal Delta-side hom is valid");
    let invalid_hom = Hom::default();
    assert_eq!(
        rel::pi_unit(&fixture.functor, &fixture.target, 4),
        outside_image()
    );
    assert_eq!(
        rel::pi_counit(&fixture.functor, &fixture.input, 4),
        outside_image()
    );
    assert_eq!(
        rel::pi_transpose_forward(
            &fixture.functor,
            &fixture.target,
            &fixture.input,
            &valid_hom,
            4,
        ),
        outside_image()
    );
    assert_eq!(
        rel::pi_transpose_inverse(
            &fixture.functor,
            &fixture.target,
            &fixture.input,
            &invalid_hom,
            4,
        ),
        outside_image(),
        "outside-image refusal precedes validation of an invalid Hom"
    );
}

struct OverBoundFixture {
    functor: RelFunctor,
    input: Table,
    target: Table,
    delta_target: Table,
}

fn over_bound_fixture() -> OverBoundFixture {
    let functor = RelFunctor {
        src: schema(&["a", "b"], &[], &[]),
        tgt: schema(&["m"], &[], &[]),
        tables: BTreeMap::from([
            ("a".to_owned(), "m".to_owned()),
            ("b".to_owned(), "m".to_owned()),
        ]),
        relations: BTreeMap::new(),
        columns: BTreeMap::new(),
        inner_edges: BTreeMap::new(),
        value_vertices: BTreeMap::new(),
    };
    OverBoundFixture {
        functor,
        input: table(
            [
                (0, row("a", &[])),
                (1, row("a", &[])),
                (2, row("a", &[])),
                (3, row("b", &[])),
                (4, row("b", &[])),
                (5, row("b", &[])),
            ],
            [],
        ),
        target: table(
            [
                (10, row("m", &[])),
                (11, row("m", &[])),
                (12, row("m", &[])),
            ],
            [],
        ),
        delta_target: table(
            [
                (10, row("a", &[])),
                (11, row("a", &[])),
                (12, row("a", &[])),
                (13, row("b", &[])),
                (14, row("b", &[])),
                (15, row("b", &[])),
            ],
            [],
        ),
    }
}

fn over_bound() -> Result<Hom, TransposeError> {
    Err(TransposeError::Adjunction(Box::new(
        AdjunctionError::ProductSizeExceeded {
            vertex: "m".to_owned(),
            actual: 9,
            limit: 4,
        },
    )))
}

fn assert_pi_over_bound_refusals() {
    let fixture = over_bound_fixture();
    let valid_hom = hom_from_rows(
        &fixture.delta_target,
        &fixture.input,
        BTreeMap::from([(10, 0), (11, 1), (12, 2), (13, 3), (14, 4), (15, 5)]),
    )
    .expect("literal Delta-side hom is valid");
    let invalid_hom = Hom::default();
    assert_eq!(
        rel::pi_unit(&fixture.functor, &fixture.target, 4),
        over_bound()
    );
    assert_eq!(
        rel::pi_counit(&fixture.functor, &fixture.input, 4),
        over_bound()
    );
    assert_eq!(
        rel::pi_transpose_forward(
            &fixture.functor,
            &fixture.target,
            &fixture.input,
            &valid_hom,
            4,
        ),
        over_bound()
    );
    assert_eq!(
        rel::pi_transpose_inverse(
            &fixture.functor,
            &fixture.target,
            &fixture.input,
            &invalid_hom,
            4,
        ),
        over_bound(),
        "product-bound refusal precedes validation of an invalid Hom"
    );
}

fn expected_ledger() -> BTreeSet<(O16Path, O16Witness)> {
    let paths = [
        O16Path::SigmaUnit,
        O16Path::SigmaCounit,
        O16Path::SigmaTransposeForward,
        O16Path::SigmaTransposeInverse,
        O16Path::PiUnit,
        O16Path::PiCounit,
        O16Path::PiTransposeForward,
        O16Path::PiTransposeInverse,
    ];
    let witnesses = [
        O16Witness::Nonidentity,
        O16Witness::LabelledNull,
        O16Witness::Multifibre,
        O16Witness::OutsideImage,
        O16Witness::Relation,
        O16Witness::TypedRefusal,
    ];
    paths
        .into_iter()
        .flat_map(|path| witnesses.into_iter().map(move |witness| (path, witness)))
        .collect()
}

#[test]
fn o16_has_an_exact_six_witness_ledger_for_each_adjoint_map() {
    let mut ledger = BTreeSet::new();
    record_sigma_successes(&mut ledger);
    record_sigma_refusals(&mut ledger);
    record_pi_successes(&mut ledger);
    assert_pi_outside_image_refusals();
    assert_pi_over_bound_refusals();
    for path in [
        O16Path::PiUnit,
        O16Path::PiCounit,
        O16Path::PiTransposeForward,
        O16Path::PiTransposeInverse,
    ] {
        assert!(ledger.insert((path, O16Witness::OutsideImage)));
        assert!(ledger.insert((path, O16Witness::TypedRefusal)));
    }

    let expected = expected_ledger();
    assert_eq!(expected.len(), 48);
    assert_eq!(
        ledger, expected,
        "the O16 ledger is exactly 8 paths x 6 witnesses"
    );
}

fn nonidentity_bijection_fixture() -> (Table, Table, Hom, SEdge) {
    let relation = SEdge::new("a", "a", "fk", Some("next"));
    let left = table(
        [
            (0, row("a", &[("x", Value::LabeledNull(4))])),
            (1, row("a", &[("x", Value::LabeledNull(5))])),
        ],
        [(relation.clone(), 0, 1)],
    );
    let right = table(
        [
            (10, row("a", &[("x", Value::LabeledNull(8))])),
            (11, row("a", &[("x", Value::LabeledNull(9))])),
        ],
        [(relation.clone(), 11, 10)],
    );
    let expected = Hom {
        rows: BTreeMap::from([(0, 11), (1, 10)]),
        nulls: BTreeMap::from([(4, Value::LabeledNull(9)), (5, Value::LabeledNull(8))]),
    };
    (left, right, expected, relation)
}

fn square_graph(value: Value) -> Graph {
    let attribute = SEdge::new("a", "v", "prop", Some("x"));
    Graph {
        nodes: BTreeMap::from([
            (0, Node::new(0, "a").with_field("x", value)),
            (1, Node::new(1, "v")),
        ]),
        edges: BTreeMap::from([(
            4,
            GEdge {
                src: 0,
                tgt: 1,
                label: attribute,
                position: None,
            },
        )]),
    }
}

fn square_table(value: Value) -> Table {
    let relation = SEdge::new("a", "b", "fk", Some("link"));
    table(
        [(0, row("a", &[("x", value)])), (1, row("b", &[]))],
        [(relation, 0, 1)],
    )
}

fn square_tree(value: Value) -> Tree {
    let child = SEdge::new("a", "v", "child", Some("x"));
    Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "a").with_field("x", value)),
            (1, Node::new(1, "v")),
        ]),
        arcs: vec![(0, 1, child)],
        fans: Vec::new(),
        root: 0,
    }
}

fn table_image_order_fixture() -> (RelSchema, RelSchema, Migration) {
    let first_column = SEdge::new("a", "va", "prop", Some("x"));
    let second_column = SEdge::new("b", "vb", "prop", Some("y"));
    let source = schema(
        &["a", "b"],
        &[],
        &[
            ("a", "x", first_column.clone()),
            ("b", "y", second_column.clone()),
        ],
    );
    let target = schema(&["target"], &[], &[]);
    let migration = Migration {
        vertex_remap: BTreeMap::from([
            ("a".to_owned(), "missing-z".to_owned()),
            ("b".to_owned(), "missing-a".to_owned()),
        ]),
        edge_remap: BTreeMap::from([
            (
                first_column,
                SEdge::new("missing-z", "w", "prop", Some("bad-x")),
            ),
            (
                second_column,
                SEdge::new("missing-a", "w", "prop", Some("bad-y")),
            ),
        ]),
        ..Migration::default()
    };
    (source, target, migration)
}

fn attribute_image_order_fixture() -> (RelSchema, RelSchema, Migration, SEdge, SEdge) {
    let first = SEdge::new("a", "vx", "prop", Some("x"));
    let second = SEdge::new("a", "vy", "prop", Some("y"));
    let first_image = SEdge::new("target", "wx", "prop", Some("bad-x"));
    let second_image = SEdge::new("target", "wy", "prop", Some("bad-y"));
    let source = schema(
        &["a"],
        &[],
        &[("a", "x", first.clone()), ("a", "y", second.clone())],
    );
    let target = schema(&["target"], &[], &[]);
    let migration = Migration {
        vertex_remap: BTreeMap::from([("a".to_owned(), "target".to_owned())]),
        edge_remap: BTreeMap::from([(first.clone(), first_image.clone()), (second, second_image)]),
        ..Migration::default()
    };
    (source, target, migration, first, first_image)
}

fn classification_collision_fixture() -> (Schema, SEdge, SEdge, SEdge) {
    let reaches_table = SEdge::new("v0", "t0", "inner", Some("bad"));
    let first_column = SEdge::new("u", "v0", "prop", Some("c"));
    let duplicate_column = SEdge::new("u", "v1", "prop", Some("c"));
    let schema = Schema {
        vertices: BTreeMap::from([
            ("mystery".to_owned(), "unknown".to_owned()),
            ("t0".to_owned(), "record".to_owned()),
            ("u".to_owned(), "record".to_owned()),
            ("v0".to_owned(), "value".to_owned()),
            ("v1".to_owned(), "value".to_owned()),
        ]),
        edges: BTreeSet::from([
            reaches_table.clone(),
            first_column.clone(),
            duplicate_column.clone(),
        ]),
        ..Schema::default()
    };
    (schema, reaches_table, first_column, duplicate_column)
}

fn classification_kinds() -> (BTreeSet<String>, BTreeSet<String>) {
    (
        BTreeSet::from(["value".to_owned()]),
        BTreeSet::from(["record".to_owned()]),
    )
}

#[test]
fn row_bijection_carries_a_nonidentity_row_null_and_relation_isomorphism() {
    let (left, right, expected, relation) = nonidentity_bijection_fixture();
    let actual = rel::row_bijection(&left, &right).expect("the literal isomorphism exists");

    assert_eq!(actual, expected);
    assert_eq!(actual.check(&left, &right), Ok(()));
    assert_eq!(actual.rows[&0], 11);
    assert_eq!(actual.nulls[&4], Value::LabeledNull(9));
    assert!(right.pairs.contains(&(relation, 11, 10)));
}

#[test]
fn observable_squares_accept_equal_literals_and_reject_independent_mutations() {
    let graph_left = square_graph(Value::LabeledNull(4));
    let graph_equal = square_graph(Value::LabeledNull(4));
    let graph_unequal = square_graph(Value::LabeledNull(5));
    assert!(migration::graph_square(&graph_left, &graph_equal));
    assert!(!migration::graph_square(&graph_left, &graph_unequal));

    let table_left = square_table(Value::Int(7));
    let table_equal = square_table(Value::Int(7));
    let table_unequal = square_table(Value::Int(8));
    assert!(migration::table_square(&table_left, &table_equal));
    assert!(!migration::table_square(&table_left, &table_unequal));

    let tree_left = square_tree(Value::Str("seven".to_owned()));
    let tree_equal = square_tree(Value::Str("seven".to_owned()));
    let tree_unequal = square_tree(Value::Str("eight".to_owned()));
    assert!(migration::tree_square(&tree_left, &tree_equal));
    assert!(!migration::tree_square(&tree_left, &tree_unequal));
}

#[test]
fn entity_functor_reports_the_exact_first_table_and_attribute_refusals() {
    let (source, target, migration) = table_image_order_fixture();
    assert_eq!(
        rel::entity_functor(&source, &target, &migration),
        Err(FunctorError::VertexImageNotATable {
            vertex: "a".to_owned(),
            image: "missing-z".to_owned(),
        }),
        "table order wins before either incoherent attribute image"
    );

    let (source, target, migration, first, first_image) = attribute_image_order_fixture();
    assert_eq!(
        rel::entity_functor(&source, &target, &migration),
        Err(FunctorError::IncoherentAttributeImage {
            edge: Box::new(first),
            image: Some(Box::new(first_image)),
        }),
        "the least source attribute is the exact staged witness"
    );
}

#[test]
fn strict_classification_reports_each_first_error_in_contract_order() {
    let (fixture, reaches_table, first_column, duplicate_column) =
        classification_collision_fixture();
    assert!(
        first_column < duplicate_column,
        "the duplicate is staged second"
    );
    let (value_kinds, non_value_kinds) = classification_kinds();

    let mut ambiguous_values = value_kinds.clone();
    ambiguous_values.extend(["a-shared".to_owned(), "z-shared".to_owned()]);
    let mut ambiguous_non_values = non_value_kinds.clone();
    ambiguous_non_values.extend(["a-shared".to_owned(), "z-shared".to_owned()]);
    assert_eq!(
        rel::classify(&fixture, &ambiguous_values, &ambiguous_non_values),
        Err(RelSchemaError::AmbiguousKind {
            kind: "a-shared".to_owned(),
        })
    );

    assert_eq!(
        rel::classify(&fixture, &value_kinds, &non_value_kinds),
        Err(RelSchemaError::UnclassifiedKind {
            vertex: "mystery".to_owned(),
            kind: "unknown".to_owned(),
        })
    );

    let mut without_unknown = fixture;
    without_unknown.vertices.remove("mystery");
    assert_eq!(
        rel::classify(&without_unknown, &value_kinds, &non_value_kinds),
        Err(RelSchemaError::ValueReachesTable {
            edge: reaches_table.clone(),
        })
    );

    without_unknown.edges.remove(&reaches_table);
    assert_eq!(
        rel::classify(&without_unknown, &value_kinds, &non_value_kinds),
        Err(RelSchemaError::DuplicateColumn {
            table: "u".to_owned(),
            column: "c".to_owned(),
        })
    );
}
