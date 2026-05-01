//! Regression: the ocaml and `ocaml_interface` grammars emit
//! `RESERVED` rule kinds that an earlier deserialiser rejected.

use panproto_parse::emit_pretty::Grammar;

#[test]
fn ocaml_grammar_json_loads() {
    // The repository ships full ocaml grammar.json files at
    // `grammars/ocaml/src/grammar.json` and
    // `grammars/ocaml_interface/src/grammar.json` (vendored from
    // tree-sitter-ocaml). Skip the test if the file isn't present
    // (so this test passes in CI environments that don't unpack
    // the grammars/ tree).
    let candidates = [
        "../../grammars/ocaml/src/grammar.json",
        "../../grammars/ocaml_interface/src/grammar.json",
    ];
    let mut tested = 0;
    for rel in candidates {
        let Ok(bytes) = std::fs::read(rel) else {
            continue;
        };
        let g = Grammar::from_bytes("ocaml", &bytes)
            .unwrap_or_else(|e| panic!("ocaml grammar at {rel} should load: {e}"));
        assert!(!g.rules.is_empty(), "ocaml grammar at {rel} has no rules");
        tested += 1;
    }
    if tested == 0 {
        eprintln!("note: no ocaml grammar.json found; skipping integration test");
    }
}
