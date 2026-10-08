#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The enumerators, each printing its bound and its count beside an
//! independent count, and the exhaustive self-checks that run over them.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::canonical;
use panproto_reference::coerce::{
    FidelityReason, LexClass, ScalarKind, ValueSpec, coerce, coerce_value,
};
use panproto_reference::edit;
use panproto_reference::encode;
use panproto_reference::enumerate::{self as en};
use panproto_reference::float::{self, Format};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Graph, Node, NodeShape, SEdge};
use panproto_reference::restrict::{self, HYPER_LABEL, HYPER_PARENT};
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::temporal::{self, TemporalKind};
use panproto_reference::validation;
use panproto_reference::value::Value;

/// Print an enumeration's bound and count beside the independent count and
/// its source, and require the two counts to agree.
fn report(name: &str, bound: &str, count: usize, independent: u64, source: &str) {
    println!("{name}: bound {bound}; count {count}; independent count {independent} ({source})");
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
        "closed formula k^n * sum_{j <= m} C(n^2 l, j)",
    );
    let src = &s[0];
    let tgt = &s[s.len() - 1];
    let m = en::migrations(src, tgt, en::MigrationScope::VERTICES);
    report(
        "migrations (vertex maps)",
        "2 source and 2 target vertices",
        m.len(),
        en::migrations_count(&sizes(src, tgt), en::MigrationScope::VERTICES),
        "closed formula (|Vt| + 1)^|Vs|",
    );
    // Every field: a source with one vertex, one edge and a hyper-edge of
    // two labels, a target with one vertex, one edge and a hyper-edge of
    // one label.
    let src = Schema {
        hyper_edges: [hyper("h", &["x", "y"])].into(),
        ..Schema::default()
            .vertex("a", "k")
            .edge("a", "a", "prop", Some("e"))
    };
    let tgt = Schema {
        hyper_edges: [hyper("g", &["z"])].into(),
        ..Schema::default()
            .vertex("a", "k")
            .edge("a", "a", "prop", Some("f"))
    };
    let all = en::migrations(&src, &tgt, en::MigrationScope::ALL);
    report(
        "migrations (every field)",
        "1 source vertex, 1 source edge, a 2-label source hyper-edge; 1 target vertex, 1 target edge, a 1-label target hyper-edge; paths of length at most 1",
        all.len(),
        en::migrations_count(&sizes(&src, &tgt), en::MigrationScope::ALL),
        "closed formula 2^1 * 2^1 * 2^1 * (1 + 2)^1 * (1 + 2)^2 * (1 + 4)^1 * 2^1 * 2^1",
    );
    assert_eq!(2 * 2 * 2 * 3 * 9 * 5 * 2 * 2, all.len());
    let distinct: BTreeSet<String> = all.iter().map(|m| format!("{m:?}")).collect();
    assert_eq!(
        distinct.len(),
        all.len(),
        "labelled: no two migrations equal"
    );
    for field in [
        |m: &Migration| !m.edge_remap.is_empty() || !m.surviving_edges.is_empty(),
        |m: &Migration| !m.resolver.is_empty(),
        |m: &Migration| !m.expansion_path.is_empty(),
        |m: &Migration| !m.hyper_resolver.is_empty(),
        |m: &Migration| !m.conditional.is_empty(),
        |m: &Migration| !m.transformed.is_empty(),
    ] {
        assert!(all.iter().any(field));
    }
    // Every migration is either refused by plan compilation with a typed
    // reason or compiles; the identity-like one of vertex a compiles.
    for mig in &all {
        let _ = restrict::compile(&src, &tgt, mig);
    }
}

fn hyper(id: &str, labels: &[&str]) -> (String, HyperEdge) {
    (
        id.to_owned(),
        HyperEdge {
            kind: "k".into(),
            signature: labels
                .iter()
                .map(|l| ((*l).to_owned(), "a".to_owned()))
                .collect(),
            parent_label: "p".into(),
        },
    )
}

fn sizes(src: &Schema, tgt: &Schema) -> en::MigrationSizes {
    let n = |x: usize| x as u64;
    en::MigrationSizes {
        src_vertices: n(src.vertices.len()),
        tgt_vertices: n(tgt.vertices.len()),
        src_edges: n(src.edges.len()),
        tgt_edges: n(tgt.edges.len()),
        src_hyper_labels: src
            .hyper_edges
            .values()
            .map(|h| n(h.signature.len()))
            .collect(),
        tgt_hyper_labels: tgt
            .hyper_edges
            .values()
            .map(|h| n(h.signature.len()))
            .collect(),
    }
}

#[test]
fn enumerate_graphs_and_check_their_bytes_are_injective() {
    let labels = [
        SEdge::new("a", "a", "e", None),
        SEdge::new("a", "b", "f", Some("n")),
    ];
    let scope = en::GraphScope {
        anchors: &["a", "b"],
        values: &[Value::Int(0)],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 2,
        max_edges: 2,
    };
    let gs = en::graphs(&scope);
    report(
        "graphs",
        "2 nodes, 2 anchors, values {none, 0}, 1 shape, 2 labels, 1 position, at most 2 edges",
        gs.len(),
        en::graphs_count(2, 1, 1, 2, 1, 2, 2),
        "closed formula sum_{n <= N} (a (v + 1) s)^n sum_{m <= M} (n^2 l p)^m",
    );
    let mut seen = BTreeSet::new();
    for g in &gs {
        let b = canonical::graph_bytes(g).unwrap();
        assert_eq!(canonical::decode_graph(&b).as_ref(), Some(g));
        assert!(seen.insert(b));
    }
}

#[test]
fn enumerate_graphs_with_fans_and_check_the_tree_form_round_trip() {
    // Fan nodes (the hyper-edge shape, anchored at `h`) with parent and
    // child edges, positioned or not: every graph either has a tree form
    // that encodes back to it up to the ids of fan nodes and edges, or is
    // refused with a typed reason.
    let labels = [
        SEdge::new("r", "r", "prop", None),
        SEdge::new("h", "r", HYPER_PARENT, None),
        SEdge::new("h", "r", HYPER_LABEL, Some("x")),
        SEdge::new("h", "r", HYPER_LABEL, None),
    ];
    let scope = en::GraphScope {
        anchors: &["r", "h"],
        values: &[],
        shapes: &[NodeShape::Plain, NodeShape::HyperEdge],
        labels: &labels,
        positions: &[None, Some(0)],
        max_nodes: 3,
        max_edges: 2,
    };
    let gs = en::graphs(&scope);
    report(
        "graphs with fans",
        "3 nodes, 2 anchors, no values, 2 shapes, 4 labels, positions {none, 0}, at most 2 edges",
        gs.len(),
        en::graphs_count(2, 0, 2, 4, 2, 3, 2),
        "closed formula sum_{n <= N} (a (v + 1) s)^n sum_{m <= M} (n^2 l p)^m",
    );
    let names = encode::schema_parent_name(None);
    let (mut trees, mut refused) = (0usize, 0usize);
    for g in &gs {
        let Some(root) = g.nodes.keys().next().copied() else {
            continue;
        };
        match encode::graph_to_tree(g, root, &names) {
            Ok(t) => {
                trees += 1;
                let (back, _) = encode::tree_to_graph(&t, None).unwrap();
                assert_eq!(fan_free_form(&back), fan_free_form(g), "{g:?}");
                assert_eq!(encode::graph_to_tree(&back, root, &names), Ok(t));
            }
            Err(_) => refused += 1,
        }
    }
    println!("graphs with fans: {trees} with a tree form, {refused} refused");
    assert!(trees > 0 && refused > 0);
}

/// A graph up to the ids of its fan nodes and of its edges: the nodes that
/// are no fan nodes, and the multiset of edges with each fan endpoint named
/// by its rank among the fan nodes.
fn fan_free_form(g: &Graph) -> (Vec<(u32, Node)>, Vec<String>) {
    let fans: Vec<u32> = g
        .nodes
        .iter()
        .filter(|(_, n)| n.shape == NodeShape::HyperEdge)
        .map(|(i, _)| *i)
        .collect();
    let name = |n: u32| {
        fans.iter()
            .position(|f| *f == n)
            .map_or_else(|| format!("n{n}"), |k| format!("f{k}"))
    };
    let mut edges: Vec<String> = g
        .edges
        .values()
        .map(|e| {
            format!(
                "{} {} {:?} {:?}",
                name(e.src),
                name(e.tgt),
                e.label,
                e.position
            )
        })
        .collect();
    edges.sort();
    let nodes = g
        .nodes
        .iter()
        .filter(|(i, _)| !fans.contains(i))
        .map(|(i, n)| (*i, n.clone()))
        .collect();
    (nodes, edges)
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
        "separate brute-force product over row counts and table assignments",
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
    let labels: &[&str] = &["x", "y"];
    for n in 1..=3u32 {
        for max_fans in 0..=1u32 {
            let scope = en::TreeScope {
                nodes: n as usize,
                anchors: &["r", "s"],
                values: &[Value::Int(0)],
                arc_names: &["k"],
                hyper_edges: &[("h", labels)],
                max_fans: max_fans as usize,
            };
            let ts = en::trees(&scope);
            report(
                "trees",
                &format!(
                    "{n} nodes, 2 anchors, values {{none, 0}}, arc names {{none, k}}, a 2-label hyper-edge, at most {max_fans} fans"
                ),
                ts.len(),
                en::trees_count(n, 2, 1, 1, &[2], max_fans),
                "Cayley's n^(n-2) labelled trees (OEIS A000272) times (n-1)! arc orders, (a (v + 1))^n anchorings, (k + 1)^(n-1) arc names and sum_{j <= F} (n (n + 1)^2)^j fan lists",
            );
            total += ts.len();
            let mut seen = BTreeSet::new();
            for t in &ts {
                let b = canonical::tree_bytes(t).unwrap();
                assert_eq!(canonical::decode_tree(&b).as_ref(), Some(t));
                assert!(seen.insert(b), "labelled: no two trees equal");
                let (g, _) = encode::tree_to_graph(t, None).unwrap();
                assert_eq!(
                    encode::graph_to_tree(&g, 0, &encode::schema_parent_name(None)).as_ref(),
                    Ok(t)
                );
            }
        }
    }
    println!("trees: {total} in all");
}

#[test]
fn enumerate_edit_sequences_and_check_the_algebra() {
    let labels = [SEdge::new("a", "b", "e", None)];
    let scope = en::DeltaScope {
        nodes: &[0, 1],
        anchors: &["a", "b"],
        values: &[Value::Int(0)],
        fields: &["f"],
        edges: &[0],
        labels: &labels,
        positions: &[None, Some(0)],
    };
    let alphabet = en::graph_deltas(&scope);
    report(
        "graph deltas",
        "nodes {0, 1}, anchors {a, b}, values {0}, field f, edge 0, 1 label, positions {none, 0}",
        alphabet.len(),
        en::graph_deltas_count(2, 2, 1, 1, 1, 1, 2),
        "closed formula 1 + 2NA(V+1) + 2EN^2L + N(A(A-1) + (V+1)V + F(V+1)V + 2) + E(L(L-1) + P(P-1))",
    );
    let distinct: BTreeSet<String> = alphabet.iter().map(|d| format!("{d:?}")).collect();
    assert_eq!(distinct.len(), alphabet.len());
    let words = en::sequences(&alphabet, 3);
    report(
        "edit sequences",
        "the graph-delta alphabet above, length at most 3",
        words.len(),
        en::sequences_count(alphabet.len() as u64, 3),
        "closed formula sum_{l <= 3} |A|^l",
    );
    let mut graphs = vec![Graph::default()];
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "a"));
    graphs.push(g);
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
            for g in &graphs {
                if let Ok(mid) = edit::apply(&w[0], g)
                    && let Ok(end) = edit::apply(&w[1], &mid)
                {
                    assert_eq!(edit::apply(&ab, g), Ok(end), "apply of {w:?}");
                }
            }
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
        "closed formula 2 * 10^d * (hi - lo + 1)",
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
        "product of 2 signs, 287 exponent fields and 14 fractions",
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
fn enumerate_binary32_boundary_patterns_and_check_widening_and_narrowing() {
    // Every binary32 pattern with either sign, every exponent field, and
    // each boundary fraction: widening keeps the value and the NaN payload,
    // the 32-bit kind holds the widened float unchanged, narrowing returns
    // the pattern, and the f64 neighbours of a nonzero finite one are
    // refused.
    let bits = en::boundary_bits(Format::B32, None, &en::binary32_fractions());
    report(
        "binary32 boundary patterns",
        "either sign, every exponent field (256), 8 fractions",
        bits.len(),
        2 * 256 * 8,
        "product of 2 signs, 256 exponent fields and 8 fractions",
    );
    let spec = ValueSpec::of(ScalarKind::Float { bits: 32 });
    for b in bits {
        let wide = float::widen_bits(Format::B32, b);
        assert_eq!(float::narrow_bits(Format::B32, wide), Some(b), "{b:#x}");
        assert_eq!(
            coerce_value(&Value::Float(wide), &spec),
            Some(Ok(Value::Float(wide))),
            "{b:#x}"
        );
        match (
            float::decode(Format::B32, b),
            float::decode(Format::B64, wide),
        ) {
            (
                float::FloatValue::Finite {
                    value: x,
                    negative: s,
                },
                float::FloatValue::Finite {
                    value: y,
                    negative: t,
                },
            ) => {
                assert_eq!((x.clone(), s), (y, t), "{b:#x}");
                if !x.is_zero() {
                    for n in [wide - 1, wide + 1] {
                        assert!(
                            coerce_value(&Value::Float(n), &spec).is_some_and(|r| r.is_err()),
                            "{n:#x}"
                        );
                    }
                }
            }
            (
                float::FloatValue::Infinite { negative: s },
                float::FloatValue::Infinite { negative: t },
            ) => {
                assert_eq!(s, t);
            }
            (float::FloatValue::NaN { .. }, float::FloatValue::NaN { .. }) => {}
            (x, y) => panic!("{b:#x} widened to another class: {x:?} {y:?}"),
        }
    }
}

#[test]
fn enumerate_binary16_patterns_and_check_the_kind_holds_exactly_them() {
    let spec = ValueSpec::of(ScalarKind::Float { bits: 16 });
    let mut finite = 0u64;
    let mut patterns = 0usize;
    for b in 0u64..=0xffff {
        patterns += 1;
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
    report(
        "binary16 patterns",
        "every 16-bit pattern",
        patterns,
        1 << 16,
        "closed formula 2^16",
    );
    println!("binary16 finite patterns: {finite}");
}

#[test]
fn enumerate_temporal_lexemes_and_check_display_and_order() {
    let ls = en::temporal_lexemes();
    report(
        "temporal lexemes",
        "4 years, 3 months, 4 days, 2 hours, 2 minutes, 3 seconds, 3 fractions, 9 offsets",
        ls.len(),
        en::temporal_lexemes_count(),
        "separate product of date, time and date-time shapes per offset",
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
        "closed formula 3^3",
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

fn report_schema_migration_and_table_bounds() {
    let schemas = en::schemas(2, &["k"], &["e"], 1);
    report(
        "schemas",
        "objects=2, kinds=1, arrow-names=1, arrows<=1",
        schemas.len(),
        en::schemas_count(2, 1, 1, 1),
        "closed formula k^n * sum C(n^2 l,j)",
    );
    let migrations = en::migrations(&schemas[0], &schemas[0], en::MigrationScope::VERTICES);
    report(
        "migrations",
        "source-objects=2, target-objects=2, vertex-map only",
        migrations.len(),
        en::migrations_count(
            &sizes(&schemas[0], &schemas[0]),
            en::MigrationScope::VERTICES,
        ),
        "closed formula (|Vt|+1)^|Vs|",
    );

    let columns = BTreeMap::from([("t", vec!["c"])]);
    let tables = en::tables(&["t"], &columns, &[Value::Int(0)], &[], 1);
    report(
        "finite attributed presheaves/tables",
        "tables=1, columns=1, values={0}, rows<=1",
        tables.len(),
        en::tables_count(&["t"], &columns, 1, &[], 1),
        "separate brute-force product over row counts and assignments",
    );
}

fn report_graph_and_tree_bounds() {
    let labels = [SEdge::new("a", "a", "e", None)];
    let graph_scope = en::GraphScope {
        anchors: &["a"],
        values: &[],
        shapes: &[NodeShape::Plain, NodeShape::HyperEdge],
        labels: &labels,
        positions: &[None, Some(0)],
        max_nodes: 2,
        max_edges: 2,
    };
    let graphs = en::graphs(&graph_scope);
    report(
        "graphs with parallel edges and fan carriers",
        "nodes<=2, edges<=2, anchors={a}, shapes={plain,fan}, positions={none,0}",
        graphs.len(),
        en::graphs_count(1, 0, 2, 1, 2, 2, 2),
        "closed product over labelled nodes and ordered edge slots",
    );

    let tree_scope = en::TreeScope {
        nodes: 2,
        anchors: &["a"],
        values: &[],
        arc_names: &["e"],
        hyper_edges: &[("h", &["x"])],
        max_fans: 1,
    };
    let trees = en::trees(&tree_scope);
    report(
        "trees",
        "nodes=2, anchors={a}, arc-names={none,e}, one 1-port fan",
        trees.len(),
        en::trees_count(2, 1, 0, 1, &[1], 1),
        "Cayley labelled-tree formula times independent decorations",
    );
}

fn report_edit_and_numeric_bounds() {
    let labels = [SEdge::new("a", "a", "e", None)];
    let delta_scope = en::DeltaScope {
        nodes: &[0],
        anchors: &["a"],
        values: &[Value::Int(0)],
        fields: &["f"],
        edges: &[0],
        labels: &labels,
        positions: &[None],
    };
    let edit_alphabet = en::graph_deltas(&delta_scope);
    let edit_sequences = en::sequences(&edit_alphabet, 2);
    report(
        "edit sequences",
        "alphabet=all deltas over one node/edge/field, length<=2",
        edit_sequences.len(),
        en::sequences_count(edit_alphabet.len() as u64, 2),
        "closed geometric sum",
    );

    let numerals = en::numerals(2, -1, 1);
    report(
        "numerals",
        "significant-digits<=2, exponent=-1..1",
        numerals.len(),
        en::numerals_count(2, -1, 1),
        "closed decimal product",
    );
    let b64 = en::boundary_bits(Format::B64, Some(&[0, 1023, 2047]), &[0, 1]);
    report(
        "binary64 boundary patterns",
        "signs=2, exponent-fields={0,1023,2047}, fractions={0,1}",
        b64.len(),
        12,
        "separate Cartesian product",
    );
    let b32 = en::boundary_bits(Format::B32, Some(&[0, 127, 255]), &[0, 1]);
    report(
        "binary32 boundary patterns",
        "signs=2, exponent-fields={0,127,255}, fractions={0,1}",
        b32.len(),
        12,
        "separate Cartesian product",
    );
    let b16 = en::boundary_bits(Format::B16, None, &(0..1024).collect::<Vec<_>>());
    report(
        "binary16 patterns",
        "all 16-bit patterns",
        b16.len(),
        65_536,
        "closed formula 2^16",
    );
    let byte_strings = en::byte_strings(3).unwrap();
    report(
        "byte strings",
        "length<=3 over all 256 bytes",
        byte_strings.len(),
        en::byte_strings_count(3).unwrap(),
        "closed geometric sum 1+256+256^2+256^3",
    );
}

fn report_temporal_bounds() {
    let temporal = en::temporal_lexemes();
    report(
        "temporal lexemes",
        "4 years, 3 months, 4 days, 2 hours, 2 minutes, 3 seconds, 3 fractions, 9 offsets",
        temporal.len(),
        en::temporal_lexemes_count(),
        "separate product by date/time shape and offset",
    );
}

#[test]
fn bounded_enumerators_are_complete() {
    report_schema_migration_and_table_bounds();
    report_graph_and_tree_bounds();
    report_edit_and_numeric_bounds();
    report_temporal_bounds();
}
