#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples of the table lens.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::migration::Migration;
use panproto_reference::model::{SEdge, Table};
use panproto_reference::rel;
use panproto_reference::restrict::NoValueLayer;
use panproto_reference::schema::Schema;
use panproto_reference::table::{self, TableLens};
use panproto_reference::value::Value;

#[test]
fn an_edit_to_a_renamed_column_is_written_to_its_source_column() {
    let s = Schema::default().vertex("t", "record").vertex("t.a", "int").edge("t", "t.a", "prop", Some("a"));
    let tg = Schema::default().vertex("t", "record").vertex("t.a", "int").edge("t", "t.a", "prop", Some("a2"));
    let kinds = (BTreeSet::from(["int".to_owned()]), BTreeSet::from(["record".to_owned()]));
    let rs = rel::classify(&s, &kinds.0, &kinds.1).unwrap();
    let rt = rel::classify(&tg, &kinds.0, &kinds.1).unwrap();
    let m = Migration {
        surviving_verts: ["t".to_owned(), "t.a".to_owned()].into(),
        edge_remap: [(SEdge::new("t", "t.a", "prop", Some("a")), SEdge::new("t", "t.a", "prop", Some("a2")))].into(),
        ..Migration::default()
    };
    let tp = table::compile_for_tables(&s, &tg, &rs, &rt, &m).unwrap();
    let mut src = Table::default();
    src.rows.insert(0, ("t".into(), [("a".to_owned(), Value::Int(1))].into()));
    let (view, c) = table::get(&s, &tg, &m, &tp, &NoValueLayer, &src).unwrap();
    assert_eq!(view.rows[&0].1, BTreeMap::from([("a2".to_owned(), Value::Int(1))]));
    assert_eq!(c.dropped_cells.get(&(0, "a".to_owned())), Some(&Value::Int(1)));
    let lens = TableLens { src: &s, tgt: &tg, rs: &rs, m: &m, tp: &tp, vl: &NoValueLayer };
    assert_eq!(lens.put(&view, &c).unwrap(), src);
    let mut edited = view.clone();
    edited.rows.get_mut(&0).unwrap().1.insert("a2".into(), Value::Int(2));
    let back = lens.put(&edited, &c).unwrap();
    assert_eq!(back.rows[&0].1, BTreeMap::from([("a".to_owned(), Value::Int(2))]));
    assert_eq!(table::get(&s, &tg, &m, &tp, &NoValueLayer, &back).unwrap().0, edited);
}
