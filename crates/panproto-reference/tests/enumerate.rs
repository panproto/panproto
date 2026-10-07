#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The enumerators, each printing its bound and its count beside an
//! independent count, and the exhaustive self-checks that run over them.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::canonical;
use panproto_reference::coerce::{
    FidelityReason, LexClass, ScalarKind, ValueSpec, coerce, coerce_value,
};
use panproto_reference::edit::{AttrKey, AttrVal, GElem, GPayload, GraphDelta};
use panproto_reference::encode;
use panproto_reference::enumerate::{self as en};
use panproto_reference::float::{self, Format};
use panproto_reference::model::{Node, SEdge};
use panproto_reference::temporal::{self, TemporalKind};
use panproto_reference::validation;
use panproto_reference::value::Value;

fn report(name: &str, bound: &str, count: usize, independent: u64) {
    println!("{name}: bound {bound}; count {count}; independent count {independent}");
    assert_eq!(count as u64, independent, "{name}");
}

#[test]
fn enumerate_schemas_and_migrations() {
    let s = en::schemas(2, &["a", "b"], &["x"], 2);
    report(
        "schemas",
        "2 vertices, 2 kinds, 1 name, at most 2 edges",
        s.len(),
        en::schemas_count(2, 2, 1, 2),
    );
    let src = &s[0];
    let tgt = &s[s.len() - 1];
    let m = en::migrations(src, tgt);
    report(
        "migrations",
        "2 source and 2 target vertices",
        m.len(),
        en::migrations_count(2, 2),
    );
}

#[test]
fn enumerate_graphs_and_check_their_bytes_are_injective() {
    let labels = [
        SEdge::new("a", "a", "e", None),
        SEdge::new("a", "b", "f", Some("n")),
    ];
    let gs = en::graphs(&["a", "b"], &[Value::Int(0)], &labels, 2, 2);
    report(
        "graphs",
        "2 nodes, 2 anchors, values {none, 0}, 2 labels, at most 2 edges",
        gs.len(),
        en::graphs_count(2, 1, 2, 2, 2),
    );
    let mut seen = BTreeSet::new();
    for g in &gs {
        let b = canonical::graph_bytes(g).unwrap();
        assert_eq!(canonical::decode_graph(&b).as_ref(), Some(g));
        assert!(seen.insert(b));
    }
}

#[test]
fn enumerate_tables_and_check_their_bytes_are_injective() {
    let cols = BTreeMap::from([("t", vec!["c"]), ("u", vec![])]);
    let rel = [SEdge::new("t", "u", "fk", Some("f"))];
    let ts = en::tables(&["t", "u"], &cols, &[Value::Int(0), Value::Null], &rel, 2);
    report(
        "tables",
        "2 tables, 1 column, values {0, null}, 1 relation, at most 2 rows",
        ts.len(),
        en::tables_count(&["t", "u"], &cols, 2, &rel, 2),
    );
    let mut seen = BTreeSet::new();
    for t in &ts {
        let b = canonical::table_bytes(t).unwrap();
        assert_eq!(canonical::decode_table(&b).as_ref(), Some(t));
        assert!(seen.insert(b));
        let (g, _) = encode::table_to_graph(t);
        assert_eq!(encode::graph_to_table(&g).as_ref(), Some(t));
    }
}

#[test]
fn enumerate_trees_and_check_their_encodings() {
    let mut total = 0;
    for n in 1..=4u32 {
        let ts = en::trees(n as usize, &["r", "s"]);
        report(
            "trees",
            &format!("{n} nodes, 2 anchors"),
            ts.len(),
            en::trees_count(n, 2),
        );
        total += ts.len();
        for t in &ts {
            let b = canonical::tree_bytes(t).unwrap();
            assert_eq!(canonical::decode_tree(&b).as_ref(), Some(t));
            let (g, _) = encode::tree_to_graph(t, None).unwrap();
            assert_eq!(encode::graph_to_tree(&g, 0).as_ref(), Some(t));
        }
    }
    println!("trees: {total} in all");
}

fn delta_alphabet() -> Vec<GraphDelta> {
    let mut out = Vec::new();
    for anchor in ["a", "b"] {
        let mut add = GraphDelta::identity();
        add.add
            .insert(GElem::Node(0), GPayload::Node(Node::new(0, anchor)));
        out.push(add.invert());
        out.push(add);
    }
    let mut attr = GraphDelta::identity();
    attr.attrs.insert(
        AttrKey::Anchor(0),
        (AttrVal::Anchor("a".into()), AttrVal::Anchor("b".into())),
    );
    out.push(attr.invert());
    out.push(attr);
    out.push(GraphDelta::identity());
    out
}

#[test]
fn enumerate_edit_sequences_and_check_the_algebra() {
    let alphabet = delta_alphabet();
    let words = en::sequences(&alphabet, 3);
    report(
        "edit sequences",
        "alphabet of 7 deltas on one node, length at most 3",
        words.len(),
        en::sequences_count(7, 3),
    );
    for w in words.iter().filter(|w| w.len() == 3) {
        let (a, b, c) = (&w[0], &w[1], &w[2]);
        if let (Ok(ab), Ok(bc)) = (a.compose(b), b.compose(c)) {
            assert_eq!(
                ab.compose(c).ok(),
                a.compose(&bc).ok(),
                "associativity on {w:?}"
            );
        }
    }
    for w in words.iter().filter(|w| w.len() == 2) {
        if let Ok(ab) = w[0].compose(&w[1]) {
            assert_eq!(w[1].invert().compose(&w[0].invert()), Ok(ab.invert()));
        }
    }
    for d in &alphabet {
        assert_eq!(
            d.compose(&d.invert()).and_then(|x| x.compose(d)).as_ref(),
            Ok(d)
        );
        assert_eq!(&d.invert().invert(), d);
    }
}

#[test]
fn enumerate_numerals_and_check_fixed_points() {
    let ns = en::numerals(2, -3, 3);
    report(
        "numerals",
        "at most 2 digits, exponent -3 to 3",
        ns.len(),
        en::numerals_count(2, -3, 3),
    );
    for kind in [
        ScalarKind::Float { bits: 16 },
        ScalarKind::Float { bits: 32 },
        ScalarKind::Decimal,
        ScalarKind::BigInt,
    ] {
        let spec = ValueSpec::of(kind);
        for t in &ns {
            let Some(Ok(v)) = coerce(t, &LexClass::Number, &spec) else {
                continue;
            };
            assert_eq!(
                coerce_value(&v, &spec),
                Some(Ok(v.clone())),
                "{t} under {kind:?}"
            );
        }
    }
}

#[test]
fn enumerate_boundary_bits_and_check_the_binary32_narrowing() {
    // Every exponent field around binary32's range (biased 1023 - 152 to
    // 1023 + 130) and the four extreme fields.
    let exps: Vec<u64> = (871..=1153).chain([0, 1, 2046, 2047]).collect();
    let bits = en::boundary_bits(Format::B64, Some(&exps), &en::binary64_fractions());
    report(
        "binary64 boundary patterns",
        "either sign, exponent fields 871 to 1153 and 0, 1, 2046, 2047, 14 fractions",
        bits.len(),
        2 * 287 * 14,
    );
    let spec = ValueSpec::of(ScalarKind::Float { bits: 32 });
    for b in bits {
        let got = coerce_value(&Value::Float(b), &spec).unwrap();
        let expected_ok = match float::decode(Format::B64, b) {
            float::FloatValue::NaN { fraction, .. } => fraction & ((1 << 29) - 1) == 0,
            float::FloatValue::Infinite { .. } => true,
            float::FloatValue::Finite { value, .. } => float::holds_exactly(Format::B32, &value),
        };
        assert_eq!(got.is_ok(), expected_ok, "{b:#x}");
        if let Err(reason) = got {
            assert!(matches!(
                reason,
                FidelityReason::NonFinite
                    | FidelityReason::Overflow
                    | FidelityReason::PrecisionExceeded { max: 9 }
            ));
        }
    }
}

#[test]
fn enumerate_binary16_patterns_and_check_the_kind_holds_exactly_them() {
    let spec = ValueSpec::of(ScalarKind::Float { bits: 16 });
    let mut finite = 0u64;
    for b in 0u64..=0xffff {
        let wide = float::widen_bits(Format::B16, b);
        assert_eq!(
            coerce_value(&Value::Float(wide), &spec),
            Some(Ok(Value::Float(wide))),
            "{b:#x}"
        );
        if let float::FloatValue::Finite { value, .. } = float::decode(Format::B16, b) {
            finite += 1;
            if value.is_zero() {
                continue;
            }
            for n in [wide - 1, wide + 1] {
                assert!(
                    coerce_value(&Value::Float(n), &spec).is_some_and(|r| r.is_err()),
                    "{n:#x}"
                );
            }
        }
    }
    report("binary16 patterns", "all 65536", 65536, 65536);
    println!("binary16 finite patterns: {finite}");
}

#[test]
fn enumerate_temporal_lexemes_and_check_display_and_order() {
    let ls = en::temporal_lexemes();
    report(
        "temporal lexemes",
        "the order's scope",
        ls.len(),
        en::temporal_lexemes_count(),
    );
    let mut values = Vec::new();
    for l in &ls {
        let kind = if l.contains('T') {
            TemporalKind::DateTime
        } else if l.len() >= 4
            && l.as_bytes()[2] != b':'
            && l.as_bytes().get(2).is_some_and(u8::is_ascii_digit)
            || l.starts_with('-')
        {
            TemporalKind::Date
        } else {
            TemporalKind::Time
        };
        let Some(v) = temporal::parse(kind, l) else {
            continue;
        };
        assert_eq!(&v.to_string(), l);
        values.push(v);
    }
    println!("temporal lexemes read: {}", values.len());
    let small: Vec<&temporal::Temporal> = values
        .iter()
        .filter(|v| v.date.is_none() || v.time.is_none())
        .collect();
    for a in &small {
        for b in &small {
            let ab = temporal::cmp_chronological(a, b);
            assert_eq!(ab.is_some(), temporal::comparable(a, b));
            assert_eq!(
                ab.map(std::cmp::Ordering::reverse),
                temporal::cmp_chronological(b, a)
            );
        }
    }
}

#[test]
fn enumerate_admission_bundles_and_check_the_policies_coincide_on_a_passed_baseline() {
    let outcomes: Vec<Option<Vec<u8>>> = vec![None, Some(vec![1]), Some(vec![2])];
    let results = en::sequences(&outcomes, 3)
        .into_iter()
        .filter(|w| w.len() == 3)
        .collect::<Vec<_>>();
    report(
        "admission results",
        "3 required bundles, each passed or one of 2 outcomes",
        results.len(),
        27,
    );
    for r in &results {
        let bundles: BTreeMap<usize, Vec<u8>> = r
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.clone().map(|x| (i, x)))
            .collect();
        let empty = BTreeMap::new();
        assert_eq!(
            validation::admits(true, &empty, &empty, &bundles),
            validation::admits(false, &empty, &empty, &bundles)
        );
    }
}
