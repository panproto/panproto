//! What the search does with a variable that has hundreds of candidates.
//!
//! A source vertex's domain is every kind-compatible target vertex, so the
//! width of a domain is a property of the schema pair and not of the solver. Two
//! shapes reach hundreds routinely and are checked here against real protocol
//! output rather than against a fixture built to be wide:
//!
//! 1. a text file parsed to one vertex per line, which is
//!    [`panproto_protocols::raw_file`]'s whole output shape; and
//! 2. the canonical tie-break, which used to fall out of the numeric encoding
//!    of `⊥` and is now stated by the domain order.
//!
//! Both were unreachable while a domain was one machine word: a sixty-four line
//! file could not be searched against itself, and `⊥` had to sit at the top of
//! the numbering for the tie-break to hold.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_mig::hom_search::{SearchOptions, find_best_morphism};
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::solve::{
    Assignment, Cfn, Cost, DEFAULT_MEM_BYTES, SearchBudget, SolverPath, ValId, choose_order,
    decode, dispatch_plan, eliminate, elimination_cost, solve,
};
use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints};
use panproto_protocols::raw_file;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

/// A text file of `lines` numbered lines.
fn text(lines: usize) -> String {
    use std::fmt::Write as _;
    (0..lines).fold(String::new(), |mut out, index| {
        let _ = writeln!(out, "line {index} of the file");
        out
    })
}

// ---------------------------------------------------------------------------
// 1. A line-per-vertex parse searches against itself
// ---------------------------------------------------------------------------

/// A two hundred line file, parsed by the shipped protocol, maps onto itself.
///
/// `raw_file::parse_text` makes one `line` vertex per line and one `file`
/// vertex, so every line vertex sees all two hundred line targets. This is the
/// exact shape a single-word domain refused: at sixty-three lines the domain was
/// full, and a sixty-fourth line made the pair unsearchable.
#[test]
fn a_two_hundred_line_file_maps_onto_itself() {
    let parsed = raw_file::parse_text(&text(200), "sample.txt").expect("parse");
    assert_eq!(
        parsed.vertices.len(),
        201,
        "the parse is one file vertex and two hundred line vertices"
    );

    let best = find_best_morphism(&parsed, &parsed, &SearchOptions::default())
        .expect("a two hundred value domain poses")
        .expect("a file maps onto itself");
    assert!(
        (best.quality - 1.0).abs() < 1e-9,
        "the identity of a file against itself is a perfect match: got {}",
        best.quality
    );
    assert_eq!(best.vertex_map.len(), 201);
    for index in 0..200usize {
        let id = format!("sample.txt::line_{index}");
        let name = panproto_gat::Name::from(id.as_str());
        assert_eq!(
            best.vertex_map.get(&name),
            Some(&name),
            "line {index} is not mapped to itself"
        );
    }
}

/// The network a file of `lines` lines searched against itself poses, at a
/// memory budget large enough to hold its cost tables.
fn file_network(lines: usize, mem_bytes: usize) -> Cfn {
    let parsed = raw_file::parse_text(&text(lines), "sample.txt").expect("parse");
    build_cfn(
        &parsed,
        &parsed,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        mem_bytes,
    )
    .expect("a line-per-vertex parse poses")
}

/// Every line variable takes its own line, and nothing is dropped.
fn assert_identity(cfn: &Cfn, best: &Assignment, lines: usize) {
    assert_eq!(best.dropped(), 0, "the identity drops nothing");
    assert!(
        (cfn.quality_of(best) - 1.0).abs() < 1e-9,
        "a file against itself is a perfect match: got {}",
        cfn.quality_of(best)
    );
    let mut checked = 0usize;
    for var in cfn.variable_ids() {
        let variable = cfn.variable(var).expect("variable");
        let value = best.get(var).expect("value");
        assert_eq!(
            variable.value_name(value).map(panproto_gat::Name::as_str),
            Some(variable.name().as_str()),
            "{var:?} is not mapped to itself"
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        lines + 1,
        "one file vertex and one vertex per line"
    );
}

/// An eight hundred line file is answered exactly, and quickly.
///
/// This is the line the dispatcher used to break at. Exact inference over this
/// network performs 1 281 602 operations and holds 1601 message entries, and
/// takes a few milliseconds; the reading that priced a bucket at
/// `d_max^(w + 1)` charged it 1 027 203 201, put it over the billion-operation
/// budget at exactly eight hundred lines, and sent it to a search that did not
/// answer. The assertion is therefore on the *route* as well as on the answer:
/// a correct identity found by the fallback would still be the defect.
#[test]
fn an_eight_hundred_line_file_is_answered_by_exact_inference() {
    let lines = 800;
    let cfn = file_network(lines, DEFAULT_MEM_BYTES);
    let budget = SearchBudget::default();

    let (order, width) = choose_order(&cfn);
    let cost = elimination_cost(&cfn, &order);
    assert_eq!(width, 1, "a star is width one however many leaves it has");
    assert_eq!(cost.entries, 1_601);
    assert_eq!(cost.operations, 1_281_602);
    assert!(
        dispatch_plan(&cfn, &budget).exact,
        "the priced cost is three orders of magnitude inside the budget"
    );

    let found = solve(&cfn, &budget);
    assert!(matches!(found.path, SolverPath::Eliminate { width: 1 }));
    assert!(found.proven_optimal);
    assert_eq!(found.limit_hit, None);
    assert_identity(&cfn, &found.best.expect("a file maps onto itself"), lines);
}

/// A two thousand line file is answered exactly too, once its tables fit.
///
/// The budget this raises is **not** the one the defect was about. A
/// 2048-line self-search holds 12 589 058 cost table entries, which is
/// 3n(n+1) + 2 for n = 2048 and comes to 100 MB, so the builder refuses it at
/// the default 64 MiB and would refuse it whatever exact inference were priced
/// at. That ceiling is a measurement of the dense binary tables the network is
/// posed from, and moving it is a change of representation rather than of a
/// cost model. What this fixes is the other half: given the tables, the
/// elimination over them costs 8 392 706 operations, which is a hundredth of
/// the budget, and the answer comes back exact.
#[test]
fn a_two_thousand_line_file_is_answered_by_exact_inference() {
    let lines = 2048;
    let cfn = file_network(lines, 256 * 1024 * 1024);
    let budget = SearchBudget::default();

    let (order, width) = choose_order(&cfn);
    let cost = elimination_cost(&cfn, &order);
    assert_eq!(width, 1);
    assert_eq!(cost.entries, 4_097);
    assert_eq!(cost.operations, 8_392_706);
    assert!(cost.fits(&budget), "the default budget takes it whole");

    let found = solve(&cfn, &budget);
    assert!(matches!(found.path, SolverPath::Eliminate { width: 1 }));
    assert!(found.proven_optimal);
    assert_identity(&cfn, &found.best.expect("a file maps onto itself"), lines);
}

/// Where the two ceilings bind on this shape, and which binds first.
///
/// A file of `n` lines poses `3n(n + 1) + 2` cost table entries and eliminates
/// in `2n(n + 1) + 2` operations over `2n + 1` message entries. Two of those
/// three grow quadratically and the message tables do not, so the two that can
/// bind are the builder's memory ceiling and the operation ceiling, and the
/// builder's is reached first by a wide margin.
///
/// The margin is measured here at a one megabyte memory ceiling, where the
/// builder refuses at 209 lines and the operation ceiling is 22 360 away, and
/// stated arithmetically at the shipped defaults, where the builder refuses at
/// 1672 lines and the operation ceiling is still 22 360. Measuring it at the
/// shipped ceiling would mean posing two networks of some 1670 vertices whose
/// domains are 1670 wide, and what that costs is the scoring rather than
/// anything this asserts.
///
/// The consequence is worth stating plainly: **no file of this shape that can
/// be posed at all is refused exact inference.**
#[test]
fn the_memory_ceiling_binds_long_before_the_operation_ceiling() {
    let budget = SearchBudget::default();
    let small = 1024 * 1024;

    let posed = file_network(208, small);
    let (order, _) = choose_order(&posed);
    assert!(elimination_cost(&posed, &order).fits(&budget));

    let parsed = raw_file::parse_text(&text(209), "sample.txt").expect("parse");
    let refused = build_cfn(
        &parsed,
        &parsed,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        small,
    );
    assert!(
        refused.is_err(),
        "209 lines needs more than a megabyte of cost tables, and 208 does not"
    );

    // The same two ceilings at the shipped defaults, in closed form.
    let cells = u64::try_from(size_of::<Cost>()).expect("a cost cell is a few bytes");
    let table_bytes = |n: u64| (3 * n * (n + 1) + 2) * cells;
    let operations = |n: u64| 2 * n * (n + 1) + 2;
    let memory = u64::try_from(budget.mem_bytes).expect("the budget is a byte count");
    assert!(table_bytes(1_671) <= memory);
    assert!(table_bytes(1_672) > memory);
    assert!(operations(22_360) <= budget.op_budget);
    assert!(operations(22_361) > budget.op_budget);
}

/// The domain of every line variable really is every line, at every width.
///
/// The identity above would also hold if something had quietly narrowed the
/// domains, so the widths are read off the network directly and the word
/// boundary is crossed on purpose.
#[test]
fn every_line_variable_sees_every_line() {
    for lines in [63usize, 64, 65, 200] {
        let parsed = raw_file::parse_text(&text(lines), "sample.txt").expect("parse");
        let cfn = build_cfn(
            &parsed,
            &parsed,
            &SearchOptions::default(),
            &DomainConstraints::default(),
            &NoEvidence,
            DEFAULT_WEIGHTS,
            DEFAULT_MEM_BYTES,
        )
        .expect("a line-per-vertex parse poses");

        assert_eq!(cfn.n_variables(), lines + 1);
        // `lines` real targets plus `⊥` for a line variable; the file vertex is
        // the one `file`-kind vertex, so it has one real target plus `⊥`.
        assert_eq!(cfn.max_domain(), lines + 1);

        let mut widest = 0usize;
        for var in cfn.variable_ids() {
            let domain = cfn.domain(var).expect("every variable has a domain");
            assert!(domain.contains(ValId::BOTTOM), "{var:?} cannot be dropped");
            assert_eq!(
                domain.iter().last(),
                Some(ValId::BOTTOM),
                "{var:?} does not order `⊥` last"
            );
            widest = widest.max(domain.len());
        }
        assert_eq!(widest, lines + 1);
    }
}

// ---------------------------------------------------------------------------
// 2. The canonical tie-break
// ---------------------------------------------------------------------------

fn tie_protocol() -> Protocol {
    Protocol {
        name: "ties".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["string".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned(), "string".to_owned()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// A body carrying one string property per name, all under one edge label.
fn body_with(fields: &[String]) -> Schema {
    let proto = tie_protocol();
    let mut builder = SchemaBuilder::new(&proto)
        .vertex("body", "object", None::<&str>)
        .expect("body");
    for id in fields {
        builder = builder
            .vertex(id, "string", None::<&str>)
            .expect("field vertex")
            .edge("body", id, "prop", Some("p"))
            .expect("field edge");
    }
    builder.entry("body").build().expect("build")
}

/// A record of `fields` interchangeable string properties.
fn record_of(fields: usize) -> Schema {
    let ids: Vec<String> = (0..fields).map(|index| format!("f{index:03}")).collect();
    body_with(&ids)
}

fn network(src: &Schema, tgt: &Schema) -> Cfn {
    build_cfn(
        src,
        tgt,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        DEFAULT_MEM_BYTES,
    )
    .expect("a tied network poses")
}

/// `⊥` sorts after every real value, however many there are.
///
/// It used to sort last because it was the top slot of a sixty-four value
/// numbering. It is now the *first* slot, so this is the property the hand
/// written `Ord` and `DomainIter` exist to hold, and it is checked across a word
/// boundary because that is where a walk that inherited the bit order would come
/// apart.
#[test]
fn bottom_sorts_last_at_every_width() {
    for fields in [3usize, 63, 64, 65, 200] {
        let schema = record_of(fields);
        let cfn = network(&schema, &schema);
        for var in cfn.variable_ids() {
            let domain = cfn.domain(var).expect("every variable has a domain");
            let seen: Vec<ValId> = domain.iter().collect();
            assert_eq!(seen.last().copied(), Some(ValId::BOTTOM), "{fields} fields");
            assert!(
                seen.windows(2).all(|pair| pair[0] < pair[1]),
                "the walk is not in the domain order at {fields} fields"
            );
            let mut sorted = seen.clone();
            sorted.sort_unstable();
            assert_eq!(
                sorted, seen,
                "`Ord` and the walk disagree at {fields} fields"
            );
        }
    }
}

/// Every assignment the network admits, with its cost, walked by hand.
///
/// Written out here rather than taken from the solver: the point of the test
/// below is that the solver's tie-break agrees with an order computed without
/// it, and a shared walk would move both sides together.
fn exhaustive(cfn: &Cfn) -> Vec<(Assignment, Cost)> {
    let mut out: Vec<Vec<ValId>> = vec![Vec::new()];
    for var in cfn.variable_ids() {
        let domain = cfn.domain(var).expect("every variable has a domain");
        let mut next = Vec::new();
        for partial in &out {
            for value in domain {
                let mut extended = partial.clone();
                extended.push(value);
                next.push(extended);
            }
        }
        out = next;
    }
    out.into_iter()
        .map(|values| {
            let assignment = Assignment::from_values(values);
            let cost = cfn.evaluate(&assignment);
            (assignment, cost)
        })
        .collect()
}

/// Among tied optima the search returns the canonical one, and canonical means
/// what it always did.
///
/// Three source fields against three target fields whose names are equidistant
/// from all three, so every one of the twenty-seven ways of mapping them costs
/// the same and the answer is decided by the tie-break alone. The tied set is
/// enumerated here without the solver, so a silent inversion of the order shows
/// up as a *different member of the tie* rather than as a failure to find one,
/// which is the failure mode a width change could introduce without breaking
/// anything else.
#[test]
fn the_canonical_optimum_among_ties_is_the_smallest_in_the_domain_order() {
    let src = body_with(&["q0".to_owned(), "q1".to_owned(), "q2".to_owned()]);
    let tgt = body_with(&["x".to_owned(), "y".to_owned(), "z".to_owned()]);
    let cfn = network(&src, &tgt);

    let scored = exhaustive(&cfn);
    let optimum = scored
        .iter()
        .map(|(_, cost)| *cost)
        .filter(|cost| *cost != Cost::TOP_SENTINEL)
        .min()
        .expect("the all-`⊥` assignment is always feasible");
    let argmins: Vec<Assignment> = scored
        .into_iter()
        .filter(|(_, cost)| *cost == optimum)
        .map(|(assignment, _)| assignment)
        .collect();
    assert_eq!(argmins.len(), 27, "the fixture is meant to tie");

    let (order, _) = choose_order(&cfn);
    let best = decode(&cfn, &eliminate(&cfn, &order), &order);
    assert_eq!(cfn.evaluate(&best), optimum);
    assert!(argmins.contains(&best));

    // The elimination decode fixes each variable in the reverse of the
    // elimination order, so the key it minimises reads the variables that way.
    let key = |assignment: &Assignment| -> Vec<u32> {
        order
            .iter()
            .rev()
            .filter_map(|var| assignment.get(*var).map(ValId::order_key))
            .collect()
    };
    let mut keys: Vec<Vec<u32>> = argmins.iter().map(key).collect();
    keys.sort();
    assert_eq!(key(&best), keys[0]);

    // Spelled out: every tied field takes the alphabetically earliest target it
    // can, and none of them is dropped. That is what "prefer a real image to
    // `⊥`, then the earlier target" comes to on this fixture.
    assert_eq!(best.dropped(), 0);
    for var in cfn.variable_ids() {
        let variable = cfn.variable(var).expect("variable");
        if variable.name().as_str() == "body" {
            continue;
        }
        let value = best.get(var).expect("value");
        assert_eq!(
            value,
            ValId::real(0),
            "{var:?} did not take the first target"
        );
        assert_eq!(
            variable.value_name(value).map(panproto_gat::Name::as_str),
            Some("x")
        );
    }
}

/// A value whose bit is past the first word decodes as itself.
///
/// The identity of a two hundred field record sends the last field to value
/// index 199, three words into the bit set. Reading it back as anything else —
/// as `⊥`, as a value in the first word, as nothing — is the failure a
/// multi-word domain makes possible and a single-word one could not.
#[test]
fn a_value_above_the_first_word_decodes_as_itself() {
    let schema = record_of(200);
    let cfn = network(&schema, &schema);
    let (order, _) = choose_order(&cfn);
    let best = decode(&cfn, &eliminate(&cfn, &order), &order);

    assert_eq!(best.dropped(), 0, "the identity drops nothing");
    let mut above_the_first_word = 0usize;
    for var in cfn.variable_ids() {
        let variable = cfn.variable(var).expect("variable");
        let value = best.get(var).expect("value");
        assert!(!value.is_bottom(), "{var:?} was dropped");
        assert_eq!(
            variable.value_name(value).map(panproto_gat::Name::as_str),
            Some(variable.name().as_str()),
            "{var:?} is not mapped to itself"
        );
        if value.raw() >= u64::BITS {
            above_the_first_word += 1;
        }
    }
    assert_eq!(
        above_the_first_word, 137,
        "the fields from index 63 up live outside the first word"
    );
}
