#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Exact C11 finite-carrier counts and alphabets.

use std::collections::BTreeSet;

use panproto_reference::canonical;
use panproto_reference::enumerate::lenses::{
    C11_DYNAMIC_VERIFICATION_HOLD, C11_SCOPE_COMPLETE, Gf5Counts, Gf5Fixture,
    PARTIAL_LENS_COMPLEMENTS, PARTIAL_LENS_SOURCES, PARTIAL_LENS_VIEWS, PARTIAL_PUT_ALPHABET,
    PartialLensHistogram, TABLE_FIXTURE_BOUND, c11_path_count_certificate,
    for_each_gf5_deletion_edit, for_each_gf5_source, for_each_gf5_source_subinstance_probe,
    for_each_partial_lens, for_each_table_fixture, gf5_count_certificate,
    partial_lens_count_certificate, partial_lens_histogram, table_cell_alphabet,
    table_fixture_count_certificate,
};
use panproto_reference::migration::Migration;
use panproto_reference::model::Graph;
use panproto_reference::schema::Schema;
use panproto_reference::value::Value;

type CanonicalTableCases = [[BTreeSet<(usize, Vec<u8>)>; 3]; 3];
type CanonicalGraphTriples = [BTreeSet<(Vec<u8>, Vec<u8>, Vec<u8>)>; 2];

#[test]
fn c11_partial_lenses_have_the_exact_carrier_and_law_histogram() {
    assert_eq!((PARTIAL_LENS_SOURCES, PARTIAL_LENS_VIEWS), (2, 2));
    assert_eq!(PARTIAL_LENS_COMPLEMENTS, 3);
    assert_eq!(PARTIAL_PUT_ALPHABET, [None, Some(0), Some(1)]);

    let mut streamed = PartialLensHistogram::default();
    let mut first = None;
    let mut last = None;
    let mut ordinal = 0;
    for_each_partial_lens(|lens| {
        let get_code = ordinal / 729;
        let put_code = ordinal % 729;
        let first_cell = get_code / 5;
        let second_offset = get_code % 5;
        let second_cell = if second_offset < first_cell {
            second_offset
        } else {
            second_offset + 1
        };
        let expected_get = [
            (first_cell / 3, first_cell % 3),
            (second_cell / 3, second_cell % 3),
        ];
        let mut expected_put = [None; 6];
        let mut digits = put_code;
        for cell in &mut expected_put {
            *cell = match digits % 3 {
                0 => None,
                source => Some(source - 1),
            };
            digits /= 3;
        }
        assert_eq!(lens.get_table(), &expected_get);
        assert_eq!(lens.put_table(), &expected_put);
        ordinal += 1;

        first.get_or_insert(lens);
        last = Some(lens);
        let gp = lens.get_put();
        let weak = lens.weak_put_get();
        let exact = lens.exact_put_get();
        let twice = lens.put_twice();
        streamed.total += 1;
        streamed.get_put += usize::from(gp);
        streamed.weak_put_get += usize::from(weak);
        streamed.exact_put_get += usize::from(exact);
        streamed.put_twice += usize::from(twice);
        streamed.get_put_and_weak_put_get += usize::from(gp && weak);
        streamed.get_put_and_weak_put_get_without_put_twice += usize::from(gp && weak && !twice);
        streamed.get_put_and_exact_put_get_without_put_twice += usize::from(gp && exact && !twice);
    });

    let expected = PartialLensHistogram {
        total: 21_870,
        get_put: 2_430,
        weak_put_get: 5_388,
        exact_put_get: 1_476,
        put_twice: 3_942,
        get_put_and_weak_put_get: 1_104,
        get_put_and_weak_put_get_without_put_twice: 312,
        get_put_and_exact_put_get_without_put_twice: 0,
    };
    assert_eq!(partial_lens_count_certificate(), 30 * 729);
    assert_eq!(partial_lens_count_certificate(), 21_870);
    assert_eq!(ordinal, partial_lens_count_certificate());
    assert_eq!(partial_lens_histogram(), expected);
    assert_eq!(streamed, expected);
    assert_eq!(
        first.unwrap().get_table(),
        &[(0, 0), (0, 1)],
        "the ordered-get stream starts with cells zero and one"
    );
    assert_eq!(first.unwrap().put_table(), &[None; 6]);
    assert_eq!(last.unwrap().get_table(), &[(1, 2), (1, 1)]);
    assert_eq!(last.unwrap().put_table(), &[Some(1); 6]);
    println!(
        "C11 partial lenses: sources=2 views=2 complements=3; get=P(6,2)=30; \
         put=3^6=729; total=21870; GP=2430 WeakPG=5388 exactPG=1476 \
         PutTwice=3942 GP+Weak=1104 GP+Weak-not-Twice=312 GP+exact-not-Twice=0; \
         put alphabet=[undefined,s0,s1]"
    );
}

#[test]
fn c11_table_fixtures_have_the_exact_per_cell_counts_and_alphabet() {
    assert_eq!(TABLE_FIXTURE_BOUND, 2);
    assert_eq!(
        table_cell_alphabet(),
        [
            None,
            Some(Value::Null),
            Some(Value::Bool(false)),
            Some(Value::Bool(true)),
            Some(Value::Int(0)),
            Some(Value::Int(1)),
            Some(Value::Str(String::new())),
        ]
    );
    let mut actual = [[0_usize; 3]; 3];
    let mut actual_sources = [[0_usize; 3]; 3];
    let mut canonical_cases: CanonicalTableCases = Default::default();
    let mut canonical_sources: [[BTreeSet<Vec<u8>>; 3]; 3] = Default::default();
    for_each_table_fixture(|fixture| {
        let rows = fixture.source.rows.len();
        actual[fixture.columns][rows] += 1;
        let bytes = canonical::table_bytes(&fixture.source).expect("a finite table encodes");
        assert!(canonical_cases[fixture.columns][rows].insert((fixture.retained, bytes.clone())));
        if fixture.retained == 0 {
            actual_sources[fixture.columns][rows] += 1;
            assert!(canonical_sources[fixture.columns][rows].insert(bytes));
        }
        assert!(fixture.retained < 1 << fixture.columns);
        assert_eq!(
            fixture.source.rows.keys().copied().collect::<Vec<_>>(),
            (0..u32::try_from(rows).expect("the table bound fits u32")).collect::<Vec<_>>()
        );
        for (anchor, cells) in fixture.source.rows.values() {
            assert_eq!(anchor, "row");
            assert!(cells.keys().all(|column| {
                column
                    .strip_prefix('c')
                    .and_then(|index| index.parse::<usize>().ok())
                    .is_some_and(|index| index < fixture.columns)
            }));
            assert!(cells.values().all(|value| {
                table_cell_alphabet()[1..]
                    .iter()
                    .any(|candidate| candidate.as_ref() == Some(value))
            }));
        }
    });

    let certificate = table_fixture_count_certificate();
    assert_eq!(
        certificate.sources_by_columns_and_rows,
        [[1, 1, 1], [1, 7, 49], [1, 49, 2_401]]
    );
    assert_eq!(certificate.lenses_by_columns, [1, 2, 4]);
    assert_eq!(
        certificate.by_columns_and_rows,
        [[1, 1, 1], [2, 14, 98], [4, 196, 9_604]]
    );
    assert_eq!(certificate.by_columns, [3, 114, 9_804]);
    assert_eq!(certificate.total, 9_921);
    assert_eq!(actual_sources, certificate.sources_by_columns_and_rows);
    assert_eq!(actual, certificate.by_columns_and_rows);
    for columns in 0..=TABLE_FIXTURE_BOUND {
        for rows in 0..=TABLE_FIXTURE_BOUND {
            assert_eq!(
                canonical_sources[columns][rows].len(),
                certificate.sources_by_columns_and_rows[columns][rows]
            );
            assert_eq!(
                canonical_cases[columns][rows].len(),
                certificate.by_columns_and_rows[columns][rows]
            );
        }
    }
    println!(
        "C11 table fixtures: columns=0..2 rows=0..2 canonical; \
         alphabet=[absent,Null,F,T,Int0,Int1,Str(\"\")]; \
         sources(c,r)=[[1,1,1],[1,7,49],[1,49,2401]]; lenses(c)=[1,2,4]; \
         cases(c,r)=[[1,1,1],[2,14,98],[4,196,9604]]; \
         by-columns=3+114+9804; total=9921"
    );
}

const fn fixture_index(fixture: Gf5Fixture) -> usize {
    match fixture {
        Gf5Fixture::DropTarget => 0,
        Gf5Fixture::VisibleThenHidden => 1,
    }
}

fn is_subinstance(candidate: &Graph, source: &Graph) -> bool {
    candidate
        .nodes
        .iter()
        .all(|(id, node)| source.nodes.get(id) == Some(node))
        && candidate
            .edges
            .iter()
            .all(|(id, edge)| source.edges.get(id) == Some(edge))
}

fn assert_gf5_fixture_definitions() {
    let first = Gf5Fixture::DropTarget;
    let first_source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "rel", Some("b"));
    let first_target = Schema::default().vertex("a", "record");
    assert_eq!(first.source_schema(), first_source);
    assert_eq!(first.target_schema(), first_target);
    assert_eq!(
        first.migration(),
        Migration {
            surviving_verts: first_target.vertices.keys().cloned().collect(),
            surviving_edges: first_target.edges,
            ..Migration::default()
        }
    );

    let second = Gf5Fixture::VisibleThenHidden;
    let second_source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("c", "record")
        .edge("a", "c", "rel", Some("visible"))
        .edge("c", "b", "rel", Some("hidden"));
    let second_target = Schema::default()
        .vertex("a", "record")
        .vertex("c", "record")
        .edge("a", "c", "rel", Some("visible"));
    assert_eq!(second.source_schema(), second_source);
    assert_eq!(second.target_schema(), second_target);
    assert_eq!(
        second.migration(),
        Migration {
            surviving_verts: second_target.vertices.keys().cloned().collect(),
            surviving_edges: second_target.edges,
            ..Migration::default()
        }
    );
}

#[test]
fn gf5_exact_fixtures_have_the_three_certified_counts() {
    assert_gf5_fixture_definitions();

    let mut sources = [0; 2];
    let mut canonical_sources: [BTreeSet<Vec<u8>>; 2] = Default::default();
    for_each_gf5_source(|fixture, source| {
        let index = fixture_index(fixture);
        sources[index] += 1;
        assert!(
            canonical_sources[index]
                .insert(canonical::graph_bytes(source).expect("a finite source graph encodes"))
        );
        assert!(source.nodes.len() <= 3);
        assert!(source.edges.len() <= 3);
        assert_eq!(
            source.nodes.keys().copied().collect::<Vec<_>>(),
            (0..u32::try_from(source.nodes.len()).expect("the graph bound fits u32"))
                .collect::<Vec<_>>()
        );
        assert!(source.edges.values().all(|edge| {
            source.nodes.contains_key(&edge.src)
                && source.nodes.contains_key(&edge.tgt)
                && fixture.source_schema().edges.contains(&edge.label)
        }));
    });
    let mut edits = [0; 2];
    let mut canonical_edits: [BTreeSet<(Vec<u8>, Vec<u8>)>; 2] = Default::default();
    for_each_gf5_deletion_edit(|fixture, source, edited_view| {
        let fixture_index = fixture_index(fixture);
        edits[fixture_index] += 1;
        assert!(canonical_edits[fixture_index].insert((
            canonical::graph_bytes(source).expect("a finite source graph encodes"),
            canonical::graph_bytes(edited_view).expect("a finite edited view encodes"),
        )));
        assert!(is_subinstance(edited_view, source));
        assert!(
            edited_view
                .nodes
                .values()
                .all(|node| { fixture.target_schema().vertices.contains_key(&node.anchor) })
        );
    });
    let mut probes = [0; 2];
    let mut canonical_probes: CanonicalGraphTriples = Default::default();
    for_each_gf5_source_subinstance_probe(|fixture, source, edited_view, candidate| {
        let fixture = fixture_index(fixture);
        probes[fixture] += 1;
        assert!(canonical_probes[fixture].insert((
            canonical::graph_bytes(source).expect("a finite source graph encodes"),
            canonical::graph_bytes(edited_view).expect("a finite edited view encodes"),
            canonical::graph_bytes(candidate).expect("a finite source subinstance encodes"),
        )));
        assert!(is_subinstance(edited_view, source));
        assert!(is_subinstance(candidate, source));
    });

    let expected = Gf5Counts {
        sources: [35, 98],
        deletion_edits: [98, 506],
        source_subinstance_probes: [873, 4_919],
    };
    assert_eq!(gf5_count_certificate(), expected);
    assert_eq!(sources, expected.sources);
    assert_eq!(edits, expected.deletion_edits);
    assert_eq!(probes, expected.source_subinstance_probes);
    assert_eq!(canonical_sources.map(|cases| cases.len()), expected.sources);
    assert_eq!(
        canonical_edits.map(|cases| cases.len()),
        expected.deletion_edits
    );
    assert_eq!(
        canonical_probes.map(|cases| cases.len()),
        expected.source_subinstance_probes
    );
    assert_eq!(sources.iter().sum::<usize>(), 133);
    assert_eq!(edits.iter().sum::<usize>(), 604);
    assert_eq!(probes.iter().sum::<usize>(), 5_792);
    println!(
        "GF5 exact fixtures: nodes<=3 edges<=3; sources=35+98=133; \
         deletion-edits=98+506=604; source-subinstance-probes=873+4919=5792"
    );
}

#[test]
fn the_amended_c11_scope_is_defined_but_awaits_dynamic_verification() {
    const { assert!(C11_SCOPE_COMPLETE) }
    let counts = c11_path_count_certificate();
    assert_eq!(counts.stages, 31);
    assert_eq!(counts.words_by_length, [31, 961, 29_791]);
    assert_eq!(counts.composable_by_length, [31, 93, 413]);
    assert_eq!(counts.single_stage_states, 144_397);
    assert_eq!(counts.word_states_by_length, [144_397, 182_833, 291_373]);
    assert_eq!(counts.tree_stages, 29);
    assert_eq!(counts.stage_tree_states, 2_548);
    assert_eq!(
        C11_DYNAMIC_VERIFICATION_HOLD,
        "HOLD: build the amended C11 census and reproduce its independent counts before execution"
    );
}
