//! LLVM protocol benchmarks: build the LLVM IR protocol and each lowering
//! morphism (TypeScript, Python, Rust → LLVM).

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_llvm::all_lowering_morphisms;
use panproto_llvm::lowering::{lower_python, lower_rust, lower_typescript};
use panproto_llvm::protocol::{instruction_opcodes, protocol, register_theories};

fn main() {
    divan::main();
}

#[divan::bench]
fn build_llvm_protocol(bencher: divan::Bencher) {
    bencher.bench(protocol);
}

#[divan::bench]
fn build_all_lowering_morphisms(bencher: divan::Bencher) {
    bencher.bench(all_lowering_morphisms);
}

#[divan::bench]
fn build_lower_typescript(bencher: divan::Bencher) {
    bencher.bench(lower_typescript);
}

#[divan::bench]
fn build_lower_python(bencher: divan::Bencher) {
    bencher.bench(lower_python);
}

#[divan::bench]
fn build_lower_rust(bencher: divan::Bencher) {
    bencher.bench(lower_rust);
}

#[divan::bench]
fn register_theories_bench(bencher: divan::Bencher) {
    bencher.bench(|| {
        let mut registry: HashMap<String, Theory> = HashMap::new();
        register_theories(&mut registry);
        registry.len()
    });
}

#[divan::bench]
fn enumerate_opcodes(bencher: divan::Bencher) {
    bencher.bench(instruction_opcodes);
}
