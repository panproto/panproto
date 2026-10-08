//! Independent finite-table reference semantics for the lens-law gates.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::{
    BoundaryProtocols, CompiledMigration, EdgeId, FInstance, PutDomainError, SynthIndex, SynthKey,
    Value,
};
use panproto_lens::laws::{LawShape, Mutation, MutationCases, SmallScope, check_put_get_for_shape};
use panproto_lens::{
    DerivedMap, LawViolation, Lens, LensShape, TableContext, TableShape, table_lens,
};
use panproto_schema::{Protocol, ScalarKind, Schema, SchemaBuilder};

#[derive(Clone, Copy, Debug)]
struct FiniteLens {
    // Total, injective get: source -> (view, complement).
    get: [(u8, u8); 2],
    // Partial put: (view, complement) -> source. `None` means undefined.
    put: [Option<u8>; 6],
}

impl FiniteLens {
    fn cell(view: u8, complement: u8) -> usize {
        usize::from(view) * 3 + usize::from(complement)
    }

    fn put(self, view: u8, complement: u8) -> Option<u8> {
        self.put[Self::cell(view, complement)]
    }

    fn get_put(self) -> bool {
        self.get
            .iter()
            .enumerate()
            .all(|(source, &(view, complement))| {
                self.put(view, complement) == u8::try_from(source).ok()
            })
    }

    // WeakPutGet re-reads the view and puts that observation against the
    // original complement. It need not reproduce the originally supplied view.
    fn weak_put_get(self) -> bool {
        (0..2).all(|view| {
            (0..3).all(|complement| {
                self.put(view, complement).is_none_or(|source| {
                    let (observed, _) = self.get[usize::from(source)];
                    self.put(observed, complement) == Some(source)
                })
            })
        })
    }

    fn exact_put_get(self) -> bool {
        (0..2).all(|view| {
            (0..3).all(|complement| {
                self.put(view, complement)
                    .is_none_or(|source| self.get[usize::from(source)].0 == view)
            })
        })
    }

    fn put_twice(self) -> bool {
        (0..2).all(|view| {
            (0..3).all(|complement| {
                self.put(view, complement).is_none_or(|source| {
                    let next_complement = self.get[usize::from(source)].1;
                    self.put(view, next_complement) == Some(source)
                })
            })
        })
    }
}

fn decode_put_table(mut code: usize) -> [Option<u8>; 6] {
    let mut table = [None; 6];
    for cell in &mut table {
        *cell = match code % 3 {
            0 => None,
            1 => Some(0),
            2 => Some(1),
            _ => unreachable!("a base-three digit is in 0..=2"),
        };
        code /= 3;
    }
    table
}

fn finite_lenses() -> impl Iterator<Item = FiniteLens> {
    (0u8..6).flat_map(|first| {
        (0u8..6)
            .filter(move |&second| second != first)
            .flat_map(move |second| {
                let get = [(first / 3, first % 3), (second / 3, second % 3)];
                (0usize..3usize.pow(6)).map(move |code| FiniteLens {
                    get,
                    put: decode_put_table(code),
                })
            })
    })
}

#[test]
fn put_twice_is_not_implied_by_get_put_and_weak_put_get() {
    let mut total = 0usize;
    let mut get_put = 0usize;
    let mut weak_put_get = 0usize;
    let mut exact_put_get = 0usize;
    let mut put_twice = 0usize;
    let mut gp_and_weak = 0usize;
    let mut gp_weak_without_twice = 0usize;
    let mut gp_exact_without_twice = 0usize;

    for lens in finite_lenses() {
        total += 1;
        let gp = lens.get_put();
        let weak = lens.weak_put_get();
        let exact = lens.exact_put_get();
        let twice = lens.put_twice();
        get_put += usize::from(gp);
        weak_put_get += usize::from(weak);
        exact_put_get += usize::from(exact);
        put_twice += usize::from(twice);
        gp_and_weak += usize::from(gp && weak);
        gp_weak_without_twice += usize::from(gp && weak && !twice);
        gp_exact_without_twice += usize::from(gp && exact && !twice);
    }

    println!(
        "finite partial-lens bound: sources=2 views=2 complements=3 \
         injective-total-get=30 partial-put-tables=729 total={total}; \
         GetPut={get_put} WeakPutGet={weak_put_get} ExactPutGet={exact_put_get} \
         PutTwice={put_twice} GetPut+WeakPutGet={gp_and_weak} \
         GetPut+WeakPutGet-PutTwice={gp_weak_without_twice} \
         GetPut+ExactPutGet-PutTwice={gp_exact_without_twice}"
    );
    assert_eq!(total, 6 * 5 * 3usize.pow(6));
    assert_eq!(total, 21_870);
    assert_eq!(get_put, 2_430);
    assert_eq!(weak_put_get, 5_388);
    assert_eq!(exact_put_get, 1_476);
    assert_eq!(put_twice, 3_942);
    assert_eq!(gp_and_weak, 1_104);
    assert_eq!(gp_weak_without_twice, 312);
    assert_eq!(gp_exact_without_twice, 0);

    let counterexample = FiniteLens {
        get: [(0, 0), (0, 1)],
        put: [Some(0), Some(1), Some(0), None, Some(1), Some(0)],
    };
    assert!(counterexample.get_put());
    assert!(counterexample.weak_put_get());
    assert!(!counterexample.put_twice());
    assert_eq!(counterexample.put(1, 2), Some(0));
    assert_eq!(counterexample.get[0], (0, 0));
    assert_eq!(counterexample.put(1, 0), None);
}

struct IndexProbeShape;

impl LensShape for IndexProbeShape {
    type Instance = u8;
    type Complement = u8;
    type Context = ();

    fn get(
        _lens: &Lens,
        source: &Self::Instance,
        _ctx: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), panproto_lens::LensError> {
        Ok(match source {
            10 => (3, 1),
            12 => (3, 2),
            source => (*source, 0),
        })
    }

    fn put(
        _lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<Self::Instance, panproto_lens::LensError> {
        Ok(match (*view, *complement) {
            (1, 0) | (3, 1) => 10,
            (1 | 3, 2) => 12,
            (view, _) => view,
        })
    }

    fn check_put_domain(
        _lens: &Lens,
        _view: &Self::Instance,
        _complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>> {
        Ok(())
    }

    fn synth_index(_instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex {
        let mut index = SynthIndex::default();
        index.nodes.insert(
            u32::from(*complement),
            SynthKey::Expand {
                source_edge: EdgeId(0),
                hop: 0,
            },
        );
        index
    }

    fn equivalent(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> bool {
        if a % 2 != b % 2 {
            return false;
        }
        match (*a, *b) {
            (1, 3) => ia.nodes.contains_key(&0) && ib.nodes.contains_key(&1),
            (3, 1) => ia.nodes.contains_key(&1) && ib.nodes.contains_key(&0),
            _ => true,
        }
    }

    fn divergence(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> Option<String> {
        (!Self::equivalent(a, ia, b, ib)).then(|| format!("{a} and {b} have different parity"))
    }

    fn divergence_modulo_derived(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
        _derived: &DerivedMap,
    ) -> Option<String> {
        Self::divergence(a, ia, b, ib)
    }

    fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
        (a != b).then(|| format!("{a} != {b}"))
    }

    fn congruence_witnesses(
        _instance: &Self::Instance,
        _index: &SynthIndex,
    ) -> Vec<(Self::Instance, SynthIndex)> {
        Vec::new()
    }
}

impl LawShape for IndexProbeShape {
    fn mutation_cases(
        _lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        _complement: &Self::Complement,
        _ctx: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance> {
        if mutation == Mutation::ValueEdit && *view == 0 {
            MutationCases::Cases(vec![1])
        } else {
            MutationCases::Skip(panproto_lens::SkipReason::NoCandidate)
        }
    }

    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
        a == b
    }

    fn congruent_states(
        _view: &Self::Instance,
        _complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)> {
        Vec::new()
    }

    fn rekey_complement(
        complement: &Self::Complement,
        from: &SynthIndex,
        to: &SynthIndex,
    ) -> Self::Complement {
        if from == to { *complement } else { 2 }
    }

    fn locality_divergence(
        _lens: &Lens,
        _source: &Self::Instance,
        _original_view: &Self::Instance,
        _edited_view: &Self::Instance,
        _restored: &Self::Instance,
        _complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Option<String> {
        None
    }

    fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
        (*a).min(*b)
    }

    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
        (original != edited).then_some(*original)
    }

    fn enumerate_small(
        _schema: &Schema,
        _scope: SmallScope,
        _ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        Vec::new()
    }
}

#[test]
fn a_user_edited_view_is_compared_under_its_put_complement_index() {
    let lens = projection_lens(&any_protocol(), 0, 0);
    assert!(check_put_get_for_shape::<IndexProbeShape>(&lens, &0, &()).is_ok());

    let candidate = 1;
    let put_complement = 0;
    let once =
        IndexProbeShape::put(&lens, &candidate, &put_complement, &()).expect("candidate put");
    let (actual, actual_complement) =
        IndexProbeShape::get(&lens, &once, &()).expect("candidate re-get");
    let candidate_index = IndexProbeShape::synth_index(&candidate, &put_complement);
    let actual_index = IndexProbeShape::synth_index(&actual, &actual_complement);
    assert!(IndexProbeShape::equivalent(
        &candidate,
        &candidate_index,
        &actual,
        &actual_index,
    ));
    let unrelated_index = IndexProbeShape::synth_index(&candidate, &2);
    assert!(!IndexProbeShape::equivalent(
        &candidate,
        &unrelated_index,
        &actual,
        &actual_index,
    ));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    Pass,
    GetPut,
    PutGet,
    Totality,
    PutTwice,
    Congruence,
    Operational,
}

const fn production_gate(result: &Result<panproto_lens::laws::LawReport, LawViolation>) -> Gate {
    match result {
        Ok(_) => Gate::Pass,
        Err(LawViolation::GetPut { .. }) => Gate::GetPut,
        Err(LawViolation::PutGet { .. }) => Gate::PutGet,
        Err(LawViolation::Totality { .. }) => Gate::Totality,
        Err(LawViolation::PutTwice { .. }) => Gate::PutTwice,
        Err(LawViolation::Congruence { .. }) => Gate::Congruence,
        Err(_) => Gate::Operational,
    }
}

fn reference_alternate(value: Option<&Value>) -> Value {
    match value {
        None | Some(Value::Null) => Value::Bool(false),
        Some(Value::Bool(value)) => Value::Bool(!value),
        Some(Value::Int(value)) => Value::Int(value.wrapping_add(1)),
        Some(Value::Str(value)) => Value::Str(format!("{value}_reference")),
        Some(_) => Value::Null,
    }
}

// An independent generator for the positive Single-Step Basis on this file's
// one-table projection fixtures. The fixtures have one unguarded table and no
// foreign keys, so every candidate constructed here is admitted by the fixture
// schema. It deliberately calls neither `LawShape::mutation_cases` nor any
// helper in laws.rs.
fn reference_candidates(lens: &Lens, view: &FInstance) -> Vec<FInstance> {
    let mut candidates = Vec::new();
    let columns = lens
        .tgt_schema
        .edges
        .keys()
        .filter(|edge| edge.src.as_ref() == "row")
        .filter_map(|edge| edge.name.as_ref().map(ToString::to_string))
        .collect::<BTreeSet<_>>();
    for id in view.row_ids() {
        for column in &columns {
            let mut next = view.clone();
            if let Some(row) = next.row_mut(id) {
                row.insert(
                    column.clone(),
                    reference_alternate(view.row(id).and_then(|row| row.get(column))),
                );
                candidates.push(next);
            }
        }
    }
    if lens.tgt_schema.vertices.contains_key("row")
        && let Ok(id) = view.fresh_row_id()
    {
        let mut next = view.clone();
        if next
            .insert_row("row", id, std::collections::HashMap::default())
            .is_ok()
        {
            candidates.push(next);
        }
    }
    for id in view.row_ids() {
        let mut next = view.clone();
        if next.remove_row(id).is_some() {
            candidates.push(next);
        }
    }
    candidates
}

// This evaluator calls no checker, candidate generator, equivalence predicate
// or rekeying function from laws.rs. It directly interprets the five gates in
// order over table get/put and the independent candidate basis above.
// `TableShape::congruent_states` is empty, so Congruence is vacuous here.
fn reference_gate(lens: &Lens, source: &FInstance, context: &TableContext) -> Gate {
    let Ok((view, complement)) = TableShape::get(lens, source, context) else {
        return Gate::Operational;
    };
    let Ok(restored) = TableShape::put(lens, &view, &complement, context) else {
        return Gate::Operational;
    };
    if source != &restored {
        return Gate::GetPut;
    }

    let mut candidates = vec![view.clone()];
    candidates.extend(reference_candidates(lens, &view));
    for candidate in &candidates {
        let Ok(put) = TableShape::put(lens, candidate, &complement, context) else {
            continue;
        };
        let Ok((actual, _)) = TableShape::get(lens, &put, context) else {
            return Gate::Operational;
        };
        if candidate != &actual {
            return Gate::PutGet;
        }
    }
    for candidate in candidates.iter().skip(1) {
        if TableShape::put(lens, candidate, &complement, context).is_err() {
            return Gate::Totality;
        }
    }
    for candidate in &candidates {
        let Ok(once) = TableShape::put(lens, candidate, &complement, context) else {
            return Gate::Operational;
        };
        let Ok((_coherent, next_complement)) = TableShape::get(lens, &once, context) else {
            return Gate::Operational;
        };
        let Ok(twice) = TableShape::put(lens, candidate, &next_complement, context) else {
            return Gate::Operational;
        };
        if once != twice {
            return Gate::PutTwice;
        }
    }
    Gate::Pass
}

fn any_protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([("any".to_owned(), ScalarKind::Any)]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn table_schema(protocol: &Protocol, columns: usize) -> Schema {
    let mut builder = SchemaBuilder::new(protocol)
        .vertex("row", "record", None)
        .expect("row table");
    for index in 0..columns {
        let column = format!("c{index}");
        let vertex = format!("row.{column}");
        builder = builder
            .vertex(&vertex, "any", None)
            .expect("column vertex")
            .edge("row", &vertex, "prop", Some(&column))
            .expect("column edge");
    }
    builder.build().expect("table schema")
}

fn projection_lens(protocol: &Protocol, columns: usize, retained: usize) -> Lens {
    let source = table_schema(protocol, columns);
    let mut builder = SchemaBuilder::new(protocol)
        .vertex("row", "record", None)
        .expect("target row table");
    for index in 0..columns {
        if retained & (1 << index) == 0 {
            continue;
        }
        let column = format!("c{index}");
        let vertex = format!("row.{column}");
        builder = builder
            .vertex(&vertex, "any", None)
            .expect("target column vertex")
            .edge("row", &vertex, "prop", Some(&column))
            .expect("target column edge");
    }
    let target = builder.build().expect("target table schema");
    Lens {
        compiled: CompiledMigration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: target,
    }
}

fn reference_cell(digit: usize) -> Option<Value> {
    match digit {
        0 => None,
        1 => Some(Value::Null),
        2 => Some(Value::Bool(false)),
        3 => Some(Value::Bool(true)),
        4 => Some(Value::Int(0)),
        5 => Some(Value::Int(1)),
        6 => Some(Value::Str(String::new())),
        _ => unreachable!("a base-seven digit is in 0..=6"),
    }
}

// Independent labelled source enumeration: canonical row ids 0..rows and
// absence plus the six fixed values at every cell.
fn reference_sources(columns: usize) -> Vec<FInstance> {
    let mut sources = Vec::new();
    for rows in 0..=2usize {
        let assignments = 7usize.pow(u32::try_from(rows * columns).expect("small exponent"));
        for mut code in 0..assignments {
            let mut source = FInstance::new();
            for row_id in 0..rows {
                let mut row = BTreeMap::new();
                for column in 0..columns {
                    if let Some(value) = reference_cell(code % 7) {
                        row.insert(format!("c{column}"), value);
                    }
                    code /= 7;
                }
                source
                    .insert_row(
                        "row",
                        panproto_inst::RowId(u32::try_from(row_id).expect("at most two rows")),
                        row.into_iter().collect(),
                    )
                    .expect("canonical row ids are distinct");
            }
            sources.push(source);
        }
    }
    sources
}

#[test]
fn check_laws_agrees_with_the_gate_evaluator_on_small_table_lenses() {
    let protocol = any_protocol();
    let context = TableContext {
        src: protocol.clone(),
        tgt: protocol.clone(),
    };
    let mut lens_count = 0usize;
    let mut case_count = 0usize;
    for columns in 0..=2 {
        let sources = reference_sources(columns);
        for retained in 0..(1usize << columns) {
            let lens = projection_lens(&protocol, columns, retained);
            lens_count += 1;
            for source in &sources {
                let boundary = BoundaryProtocols {
                    src: &protocol,
                    tgt: &protocol,
                };
                let expected = reference_gate(&lens, source, &context);
                let actual = production_gate(&table_lens::check_laws(&lens, source, boundary));
                assert_eq!(actual, expected, "columns={columns} retained={retained:b}");
                case_count += 1;
            }
        }
    }
    println!(
        "small table-lens bound: rows<=2 columns<=2 six-value alphabet plus absence; \
         lenses={lens_count} source/lens cases={case_count}"
    );
    assert_eq!(lens_count, 1 + 2 + 4);
    assert_eq!(case_count, 3 + 2 * 57 + 4 * 2_451);
    assert_eq!(case_count, 9_921);
}
