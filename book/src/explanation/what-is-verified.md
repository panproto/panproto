# What panproto verifies

panproto's correctness rests on a small set of properties that are mechanically checked. Some are verified at compile time (panic on failure during protocol registration); some are verified at runtime when the operation is invoked; some are verified by property-based tests in CI. This page is the catalogue.

If a property is in this list, the implementation enforces it. If you can construct a counterexample, that is a bug.

| Property | Where checked | Failure mode | Source |
|---|---|---|---|
| Protocol registration produces a valid theory | Compile-time (panic at registration) | Named intermediate colimit step in panic message | [`panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs) |
| Schema validates against its protocol | Runtime | `schema validate` exits non-zero with the failing equation | `panproto-schema` |
| Migration existence conditions hold | Runtime, before any data is moved | `schema check` exits non-zero, naming the missing input | `panproto-check` |
| Migration type-checks at the GAT level | Runtime, on demand via `--typecheck` | `schema check --typecheck` exits non-zero, naming the offending sort or operation | `panproto-mig` |
| Lens GetPut law: `put(s, get(s), complement(s)) = s` | CI property tests, every lens combinator | Property-test failure with shrunk counterexample | `panproto-lens/src/laws.rs` |
| Lens PutGet law: `get(put(s, v, c)) = v` | CI property tests, every lens combinator | Property-test failure with shrunk counterexample | `panproto-lens/src/laws.rs` |
| Lens PutPut law: `put(put(s, v₁, c), v₂, c) = put(s, v₂, c)` | CI property tests, every lens combinator | Property-test failure with shrunk counterexample | `panproto-lens/src/laws.rs` |
| Source-code emit round-trips its grammar's full corpus (`emit(parse(emit(s))) == emit(s)` plus vertex-kind and edge-shape multiset preservation, on every corpus entry, for every protocol in the verified set) | CI test (`emit_corpus_audit`) | Test panic naming the protocol and first divergent corpus entry | [`panproto-parse/tests/emit_corpus_audit.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-parse/tests/emit_corpus_audit.rs) |
| Complement composition compatibility | Runtime, on `Complement::compose` | `LensError::ComplementFingerprintMismatch` | `panproto-lens/src/asymmetric.rs` |
| Complement composition agreement | Runtime, on `Complement::compose` | `LensError::ComplementConflict` (with offending key) | `panproto-lens/src/asymmetric.rs` |
| Protolens composition: structural equality of the intermediate endofunctor | Runtime, on `vertical_compose` | `LensError::CompositionMismatch` | `panproto-lens/src/protolens.rs` |
| Pushout cocone commutativity | Runtime, on every colimit construction | Returned as part of `ColimitResult` | `panproto-gat/src/colimit.rs` |
| Pushout universal property: every alternative cocone factors uniquely through the pushout | Runtime, on demand via `verify_universal` | `EquationNotPreserved` | `panproto-gat/src/colimit.rs` |
| Schema merge universal property | Runtime, on `vcs::merge::verify_pushout_universal` | `PushoutError::UniversalFactorizationFailure` | `panproto-vcs/src/merge.rs` |
| Expression evaluation totality (within step budget) | Runtime, on every evaluation | `ExprError::StepLimitExceeded` | `panproto-expr/src/eval.rs` |
| Expression arithmetic overflow check | Runtime, on every arithmetic op | `ExprError::Overflow` | `panproto-expr/src/builtin.rs` |
| Expression division by zero check | Runtime, on `Div`/`Mod` | `ExprError::DivisionByZero` | `panproto-expr/src/builtin.rs` |

## Source-code emit coverage

The source-code emitter (`emit_pretty`) is verified against the full upstream `test/corpus/` of **255 of the 261** vendored tree-sitter grammars: every corpus entry round-trips under the strict oracle described in the row above. The remaining six are blocked upstream, not by the emitter:

- **`comment`, `todotxt`, `wolfram`** model their content as opaque free-text spans, so the grammar gives the emitter no structure to reconstruct and the captured text is dropped on emit (a corruption the char-multiset detector flags).
- **`less`** is compiled against an older tree-sitter ABI than the 0.26 runtime loads, so its parser yields only error nodes; there is nothing to round-trip until the grammar is re-vendored.
- **`move`** has no `let`-binding production in the vendored grammar, so real source already parses to an error tree on the way in; this is a parse-layer defect, not an emit one.
- **`test`** parses tree-sitter's own corpus format, whose `===` and `---` delimiters collide with the corpus reader, so it cannot be exercised inside the harness.

The six are the irreducible residual under the current grammars and runtime; closing any of them needs an upstream grammar fix, an ABI re-vendor, or a harness change rather than emitter work.

## What is *not* verified

The following properties are *not* mechanically checked and should not be assumed:

- **Performance characteristics.** The implementation does not guarantee any particular complexity bound on lens composition, colimit construction, or migration application.
- **Round-trip stability of value-level transforms across data with information loss.** A migration that drops a field cannot round-trip the dropped data; the lens laws apply only to the surviving structure.
- **Equivalence of two protocols with isomorphic theories but different parsers.** Two protocols whose theories are the same up to isomorphism are still distinct from panproto's perspective.
- **Application-level invariants not expressible in the schema theory.** "Email addresses must contain `@`" is checked only if the schema actually carries a constraint expressing it.

## See also

- [Schema version control semantics](./vcs-semantics.md) for the merge case.
- [Lenses and round-trip laws](./lenses-roundtrip.md) for the lens case.
- [Migrations as morphisms](./migrations-as-morphisms.md) for the migration case.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the universal-property statement.
