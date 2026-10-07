#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples and algebraic identities of the relational Kan
//! extensions on tiny instances.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::migration::Migration;
use panproto_reference::model::{SEdge, Table};
use panproto_reference::rel::{self, KanError, RelFunctor};
use panproto_reference::schema::Schema;
use panproto_reference::value::Value;

fn kinds() -> (BTreeSet<String>, BTreeSet<String>) {
    (["int".to_owned()].into(), ["record".to_owned()].into())
}

fn rel_of(s: &Schema) -> rel::RelSchema {
    let (k, n) = kinds();
    rel::classify(s, &k, &n).unwrap()
}

fn functor(src: &Schema, tgt: &Schema, m: &Migration) -> RelFunctor {
    rel::entity_functor(&rel_of(src), &rel_of(tgt), m).unwrap()
}

fn row(t: &str, cells: &[(&str, Value)]) -> (String, BTreeMap<String, Value>) {
    (t.to_owned(), cells.iter().map(|(c, v)| ((*c).to_owned(), v.clone())).collect())
}

/// a(x), b(y) and an FK a -f-> b.
fn ab() -> Schema {
    Schema::default()
        .vertex("a", "record").vertex("b", "record").vertex("a.x", "int").vertex("b.y", "int")
        .edge("a", "a.x", "prop", Some("x")).edge("b", "b.y", "prop", Some("y")).edge("a", "b", "fk", Some("f"))
}

fn identity(s: &Schema) -> Migration {
    Migration { surviving_verts: s.vertices.keys().cloned().collect(), surviving_edges: s.edges.clone(), ..Migration::default() }
}

fn sample() -> Table {
    let mut t = Table::default();
    t.rows.insert(0, row("a", &[("x", Value::Int(1))]));
    t.rows.insert(1, row("a", &[("x", Value::LabeledNull(0))]));
    t.rows.insert(2, row("b", &[("y", Value::Int(2))]));
    t.pairs.insert((SEdge::new("a", "b", "fk", Some("f")), 0, 2));
    t
}

#[test]
fn every_extension_along_the_identity_returns_the_input() {
    let s = ab();
    let f = functor(&s, &s, &identity(&s));
    let i = sample();
    assert_eq!(rel::delta(&f, &i).unwrap().0, i);
    assert_eq!(rel::sigma(&f, &i).unwrap().0, i);
    let (p, _) = rel::pi(&f, &i).unwrap();
    assert!(rel::isomorphism(&p, &i).is_some());
}

#[test]
fn a_merge_of_two_tables_makes_product_rows_and_null_cells() {
    // a(x), b(y) -> m(x, y): Pi takes the product, joining on no shared
    // column; Sigma keeps each row with a fresh null for the other column.
    let s = ab();
    let t = Schema::default()
        .vertex("m", "record").vertex("m.x", "int").vertex("m.y", "int").edge("m", "m.x", "prop", Some("x")).edge("m", "m.y", "prop", Some("y"));
    let m = Migration {
        vertex_remap: [("a", "m"), ("b", "m"), ("a.x", "m.x"), ("b.y", "m.y")].iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect(),
        edge_remap: [(SEdge::new("a", "a.x", "prop", Some("x")), SEdge::new("m", "m.x", "prop", Some("x"))), (SEdge::new("b", "b.y", "prop", Some("y")), SEdge::new("m", "m.y", "prop", Some("y")))].into(),
        ..Migration::default()
    };
    let f = functor(&s, &t, &m);
    let i = sample();
    let (p, prov) = rel::pi(&f, &i).unwrap();
    assert_eq!(p.rows.len(), 2);
    assert_eq!(prov.values().map(|(_, tu)| tu.clone()).collect::<Vec<_>>(), vec![vec![0, 2], vec![1, 2]]);
    let (sg, sp) = rel::sigma(&f, &i).unwrap();
    assert_eq!(sg.rows[&0].1["y"], Value::LabeledNull(1));
    assert_eq!(sg.rows[&2].1["x"], Value::LabeledNull(3));
    assert_eq!(sp.fresh.len(), 3);
}

#[test]
fn pi_equalises_an_intra_fibre_relation_carried_to_a_loop() {
    // src_a, src_b both to m, two rows each, FK a -> b = {(a0, b1)} carried
    // to the loop m -> m: four product rows, and the loop holds exactly the
    // pairs (u, v) with (u at a, v at b) = (a0, b1).
    let s = Schema::default().vertex("a", "record").vertex("b", "record").edge("a", "b", "fk", Some("f"));
    let t = Schema::default().vertex("m", "record").edge("m", "m", "fk", Some("f"));
    let fk = SEdge::new("a", "b", "fk", Some("f"));
    let lp = SEdge::new("m", "m", "fk", Some("f"));
    let m = Migration {
        vertex_remap: [("a".to_owned(), "m".to_owned()), ("b".to_owned(), "m".to_owned())].into(),
        edge_remap: [(fk.clone(), lp.clone())].into(),
        ..Migration::default()
    };
    let f = functor(&s, &t, &m);
    let mut i = Table::default();
    for (id, tb) in [(0, "a"), (1, "a"), (2, "b"), (3, "b")] {
        i.rows.insert(id, row(tb, &[]));
    }
    i.pairs.insert((fk, 0, 3));
    let (p, prov) = rel::pi(&f, &i).unwrap();
    assert_eq!(p.rows.len(), 4);
    let tuple = |id: u32| prov[&id].1.clone();
    let loops: Vec<(Vec<u32>, Vec<u32>)> = p.pairs.iter().map(|(e, u, v)| { assert_eq!(*e, lp); (tuple(*u), tuple(*v)) }).collect();
    assert_eq!(loops.len(), 4);
    assert!(loops.iter().all(|(u, v)| u[0] == 0 && v[1] == 3));
}

#[test]
fn two_columns_merged_into_one_clash_unless_equal() {
    let s = Schema::default().vertex("r", "record").vertex("r.x", "int").vertex("r.y", "int").edge("r", "r.x", "prop", Some("x")).edge("r", "r.y", "prop", Some("y"));
    let t = Schema::default().vertex("r", "record").vertex("r.z", "int").edge("r", "r.z", "prop", Some("z"));
    let z = SEdge::new("r", "r.z", "prop", Some("z"));
    let m = Migration {
        surviving_verts: ["r".to_owned()].into(),
        vertex_remap: [("r.x".to_owned(), "r.z".to_owned()), ("r.y".to_owned(), "r.z".to_owned())].into(),
        edge_remap: [(SEdge::new("r", "r.x", "prop", Some("x")), z.clone()), (SEdge::new("r", "r.y", "prop", Some("y")), z)].into(),
        ..Migration::default()
    };
    let f = functor(&s, &t, &m);
    let mut i = Table::default();
    i.rows.insert(0, row("r", &[("x", Value::Int(1)), ("y", Value::Int(1))]));
    assert_eq!(rel::sigma(&f, &i).unwrap().0.rows[&0].1["z"], Value::Int(1));
    i.rows.insert(0, row("r", &[("x", Value::Int(1)), ("y", Value::Int(2))]));
    assert!(matches!(rel::sigma(&f, &i), Err(KanError::SigmaConstantClash { row: 0, .. })));
    // Float cells clash on bits: 0.0 against -0.0, NaN against -NaN.
    i.rows.insert(0, row("r", &[("x", Value::Float(0.0f64.to_bits())), ("y", Value::Float((-0.0f64).to_bits()))]));
    assert!(rel::sigma(&f, &i).is_err());
    i.rows.insert(0, row("r", &[("x", Value::Float(0x7ff8_0000_0000_0000)), ("y", Value::Float(0x7ff8_0000_0000_0000))]));
    assert!(rel::sigma(&f, &i).is_ok());
}

#[test]
fn sigma_and_delta_hom_sets_have_equal_sizes_on_tiny_instances() {
    // Hom(Sigma I, J) and Hom(I, Delta J) are in bijection; on a rename the
    // two counts agree for every small J.
    let s = ab();
    let f = functor(&s, &s, &identity(&s));
    let i = sample();
    let (si, _) = rel::sigma(&f, &i).unwrap();
    for j in [sample(), { let mut t = sample(); t.rows.insert(5, row("b", &[("y", Value::Int(2))])); t }] {
        let (dj, _) = rel::delta(&f, &j).unwrap();
        assert_eq!(rel::homs(&si, &j).len(), rel::homs(&i, &dj).len());
    }
}

#[test]
fn pi_refuses_a_target_column_outside_the_image() {
    let s = Schema::default().vertex("a", "record");
    let t = Schema::default().vertex("a", "record").vertex("a.z", "int").edge("a", "a.z", "prop", Some("z"));
    let m = Migration { surviving_verts: ["a".to_owned()].into(), ..Migration::default() };
    let f = functor(&s, &t, &m);
    assert_eq!(rel::pi(&f, &Table::default()), Err(KanError::AttributeOutsideImage { table: "a".into(), column: "z".into() }));
}
