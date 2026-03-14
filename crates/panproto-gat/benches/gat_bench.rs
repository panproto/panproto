#![allow(missing_docs, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use divan::Bencher;
use panproto_gat::{
    Equation, Operation, Sort, Term, Theory, TheoryMorphism, check_morphism, colimit,
    resolve_theory,
};

fn main() {
    divan::main();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a shared base theory (common sorts).
fn shared_theory(n: usize) -> Theory {
    let sorts: Vec<Sort> = (0..n).map(|i| Sort::simple(format!("Shared{i}"))).collect();
    let ops: Vec<Operation> = (0..n)
        .map(|i| {
            Operation::new(
                format!("shared_op{i}"),
                vec![(Arc::from("x"), Arc::from(format!("Shared{i}").as_str()))],
                format!("Shared{i}"),
            )
        })
        .collect();
    Theory::new("Shared", sorts, ops, vec![])
}

/// Build two theories that extend a shared base, each with extra sorts.
fn colimit_setup(shared_size: usize, extra_size: usize) -> (Theory, Theory, Theory) {
    let base = shared_theory(shared_size);

    // t1: shared sorts + extra sorts
    let mut t1_sorts: Vec<Sort> = base.sorts.clone();
    let mut t1_ops: Vec<Operation> = base.ops.clone();
    for i in 0..extra_size {
        let name = format!("T1Extra{i}");
        t1_sorts.push(Sort::simple(name.as_str()));
        t1_ops.push(Operation::new(
            format!("t1_extra_op{i}"),
            vec![(Arc::from("x"), Arc::from(name.as_str()))],
            name,
        ));
    }
    let t1 = Theory::new("Theory1", t1_sorts, t1_ops, vec![]);

    // t2: shared sorts + extra sorts
    let mut t2_sorts: Vec<Sort> = base.sorts.clone();
    let mut t2_ops: Vec<Operation> = base.ops.clone();
    for i in 0..extra_size {
        let name = format!("T2Extra{i}");
        t2_sorts.push(Sort::simple(name.as_str()));
        t2_ops.push(Operation::new(
            format!("t2_extra_op{i}"),
            vec![(Arc::from("x"), Arc::from(name.as_str()))],
            name,
        ));
    }
    let t2 = Theory::new("Theory2", t2_sorts, t2_ops, vec![]);

    (t1, t2, base)
}

/// Build a theory with n sorts, n unary ops, and optional equations.
fn scaled_theory(name: &str, n: usize, with_equations: bool) -> Theory {
    let sorts: Vec<Sort> = (0..n).map(|i| Sort::simple(format!("S{i}"))).collect();

    let ops: Vec<Operation> = (0..n)
        .map(|i| {
            let out_sort = format!("S{}", (i + 1) % n);
            Operation::unary(format!("op{i}"), "x", format!("S{i}"), out_sort)
        })
        .collect();

    let eqs: Vec<Equation> = if with_equations {
        (0..n.min(20))
            .map(|i| {
                let op_a = format!("op{i}");
                let op_b = format!("op{}", (i + 1) % n);
                Equation::new(
                    format!("eq{i}"),
                    Term::app(op_a.as_str(), vec![Term::var("x")]),
                    Term::app(op_b.as_str(), vec![Term::var("x")]),
                )
            })
            .collect()
    } else {
        vec![]
    };

    Theory::new(name, sorts, ops, eqs)
}

/// Build a registry with a linear inheritance chain of depth d.
fn inheritance_chain(depth: usize) -> HashMap<String, Theory> {
    let mut registry = HashMap::new();

    // Base theory
    let base = Theory::new(
        "T0",
        vec![Sort::simple("Base")],
        vec![Operation::nullary("base_const", "Base")],
        vec![],
    );
    registry.insert("T0".to_owned(), base);

    for i in 1..=depth {
        let name = format!("T{i}");
        let parent = format!("T{}", i - 1);
        let child = Theory::extending(
            name.as_str(),
            vec![Arc::from(parent.as_str())],
            vec![Sort::simple(format!("S{i}"))],
            vec![Operation::nullary(format!("const{i}"), format!("S{i}"))],
            vec![],
        );
        registry.insert(name, child);
    }

    registry
}

// ---------------------------------------------------------------------------
// Benchmarks: colimit
// ---------------------------------------------------------------------------

#[divan::bench]
fn colimit_small(bencher: Bencher) {
    let (t1, t2, shared) = colimit_setup(3, 5);
    bencher.bench(|| colimit(&t1, &t2, &shared));
}

#[divan::bench]
fn colimit_medium(bencher: Bencher) {
    let (t1, t2, shared) = colimit_setup(10, 20);
    bencher.bench(|| colimit(&t1, &t2, &shared));
}

#[divan::bench]
fn colimit_large(bencher: Bencher) {
    let (t1, t2, shared) = colimit_setup(50, 100);
    bencher.bench(|| colimit(&t1, &t2, &shared));
}

// ---------------------------------------------------------------------------
// Benchmarks: colimit with equations
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50, 100])]
fn colimit_with_equations(bencher: Bencher, n: usize) {
    let base = {
        let sorts: Vec<Sort> = (0..n).map(|i| Sort::simple(format!("Shared{i}"))).collect();
        let ops: Vec<Operation> = (0..n)
            .map(|i| {
                Operation::unary(
                    format!("shared_op{i}"),
                    "x",
                    format!("Shared{i}"),
                    format!("Shared{}", (i + 1) % n),
                )
            })
            .collect();
        let eqs: Vec<Equation> = (0..n.min(10))
            .map(|i| {
                Equation::new(
                    format!("shared_eq{i}"),
                    Term::app(format!("shared_op{i}").as_str(), vec![Term::var("x")]),
                    Term::var("x"),
                )
            })
            .collect();
        Theory::new("Shared", sorts, ops, eqs)
    };

    // t1 extends base with extra sorts + equations
    let mut t1_sorts = base.sorts.clone();
    let mut t1_ops = base.ops.clone();
    let mut t1_eqs = base.eqs.clone();
    for i in 0..5 {
        let name = format!("T1Extra{i}");
        t1_sorts.push(Sort::simple(name.as_str()));
        t1_ops.push(Operation::nullary(format!("t1_op{i}"), name.as_str()));
        t1_eqs.push(Equation::new(
            format!("t1_eq{i}"),
            Term::constant(format!("t1_op{i}").as_str()),
            Term::constant(format!("t1_op{i}").as_str()),
        ));
    }
    let t1 = Theory::new("T1", t1_sorts, t1_ops, t1_eqs);

    let mut t2_sorts = base.sorts.clone();
    let mut t2_ops = base.ops.clone();
    let mut t2_eqs = base.eqs.clone();
    for i in 0..5 {
        let name = format!("T2Extra{i}");
        t2_sorts.push(Sort::simple(name.as_str()));
        t2_ops.push(Operation::nullary(format!("t2_op{i}"), name.as_str()));
        t2_eqs.push(Equation::new(
            format!("t2_eq{i}"),
            Term::constant(format!("t2_op{i}").as_str()),
            Term::constant(format!("t2_op{i}").as_str()),
        ));
    }
    let t2 = Theory::new("T2", t2_sorts, t2_ops, t2_eqs);

    bencher.bench(|| colimit(&t1, &t2, &base));
}

// ---------------------------------------------------------------------------
// Benchmarks: resolve_theory
// ---------------------------------------------------------------------------

#[divan::bench(args = [3, 10, 20])]
fn resolve_theory_shallow(bencher: Bencher, depth: usize) {
    let registry = inheritance_chain(depth);
    let leaf = format!("T{depth}");
    bencher.bench(|| resolve_theory(&leaf, &registry));
}

#[divan::bench(args = [50, 100])]
fn resolve_theory_deep(bencher: Bencher, depth: usize) {
    let registry = inheritance_chain(depth);
    let leaf = format!("T{depth}");
    bencher.bench(|| resolve_theory(&leaf, &registry));
}

// ---------------------------------------------------------------------------
// Benchmarks: check_morphism
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50, 100])]
fn check_morphism_identity(bencher: Bencher, n: usize) {
    let theory = scaled_theory("T", n, true);

    let sort_map: HashMap<Arc<str>, Arc<str>> = theory
        .sorts
        .iter()
        .map(|s| (Arc::clone(&s.name), Arc::clone(&s.name)))
        .collect();
    let op_map: HashMap<Arc<str>, Arc<str>> = theory
        .ops
        .iter()
        .map(|o| (Arc::clone(&o.name), Arc::clone(&o.name)))
        .collect();

    let m = TheoryMorphism::new("id", "T", "T", sort_map, op_map);

    bencher.bench(|| check_morphism(&m, &theory, &theory));
}

#[divan::bench(args = [10, 50, 100])]
fn check_morphism_renaming(bencher: Bencher, n: usize) {
    let domain = scaled_theory("D", n, true);

    // Build a codomain with renamed sorts/ops but identical structure
    let cod_sorts: Vec<Sort> = (0..n).map(|i| Sort::simple(format!("R{i}"))).collect();
    let cod_ops: Vec<Operation> = (0..n)
        .map(|i| {
            Operation::unary(
                format!("rop{i}"),
                "x",
                format!("R{i}"),
                format!("R{}", (i + 1) % n),
            )
        })
        .collect();
    let cod_eqs: Vec<Equation> = (0..n.min(20))
        .map(|i| {
            let op_a = format!("rop{i}");
            let op_b = format!("rop{}", (i + 1) % n);
            Equation::new(
                format!("eq{i}"),
                Term::app(op_a.as_str(), vec![Term::var("x")]),
                Term::app(op_b.as_str(), vec![Term::var("x")]),
            )
        })
        .collect();
    let codomain = Theory::new("C", cod_sorts, cod_ops, cod_eqs);

    let sort_map: HashMap<Arc<str>, Arc<str>> = (0..n)
        .map(|i| {
            (
                Arc::from(format!("S{i}").as_str()),
                Arc::from(format!("R{i}").as_str()),
            )
        })
        .collect();
    let op_map: HashMap<Arc<str>, Arc<str>> = (0..n)
        .map(|i| {
            (
                Arc::from(format!("op{i}").as_str()),
                Arc::from(format!("rop{i}").as_str()),
            )
        })
        .collect();

    let m = TheoryMorphism::new("rename", "D", "C", sort_map, op_map);

    bencher.bench(|| check_morphism(&m, &domain, &codomain));
}

// ---------------------------------------------------------------------------
// Benchmarks: find_sort (O(1) lookup)
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50, 100, 500])]
fn find_sort_by_name(bencher: Bencher, n: usize) {
    let theory = scaled_theory("T", n, false);
    // Look up the last sort to avoid any ordering bias
    let target = format!("S{}", n - 1);

    bencher.bench(|| theory.find_sort(&target));
}

// ---------------------------------------------------------------------------
// Benchmarks: Theory construction parameterized by size
// ---------------------------------------------------------------------------

#[divan::bench(args = [10, 50, 100, 500])]
fn theory_construction(bencher: Bencher, n: usize) {
    let sorts: Vec<Sort> = (0..n).map(|i| Sort::simple(format!("S{i}"))).collect();
    let ops: Vec<Operation> = (0..n)
        .map(|i| {
            Operation::unary(
                format!("op{i}"),
                "x",
                format!("S{i}"),
                format!("S{}", (i + 1) % n),
            )
        })
        .collect();

    bencher.bench(|| Theory::new("Bench", sorts.clone(), ops.clone(), vec![]));
}
