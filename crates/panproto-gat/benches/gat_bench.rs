#![allow(missing_docs)]

use divan::Bencher;
use panproto_gat::{Operation, Sort, Theory, colimit};

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
                vec![("x".into(), format!("Shared{i}"))],
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
        t1_sorts.push(Sort::simple(format!("T1Extra{i}")));
        t1_ops.push(Operation::new(
            format!("t1_extra_op{i}"),
            vec![("x".into(), format!("T1Extra{i}"))],
            format!("T1Extra{i}"),
        ));
    }
    let t1 = Theory::new("Theory1", t1_sorts, t1_ops, vec![]);

    // t2: shared sorts + extra sorts
    let mut t2_sorts: Vec<Sort> = base.sorts.clone();
    let mut t2_ops: Vec<Operation> = base.ops.clone();
    for i in 0..extra_size {
        t2_sorts.push(Sort::simple(format!("T2Extra{i}")));
        t2_ops.push(Operation::new(
            format!("t2_extra_op{i}"),
            vec![("x".into(), format!("T2Extra{i}"))],
            format!("T2Extra{i}"),
        ));
    }
    let t2 = Theory::new("Theory2", t2_sorts, t2_ops, vec![]);

    (t1, t2, base)
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
