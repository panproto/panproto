# What panproto verifies

panproto's correctness rests on a small set of properties that are mechanically checked. Some are verified at compile time (a panic during protocol registration, or an error returned when a migration or theory is compiled); some are verified at runtime when the operation is invoked; some are verified by property-based tests in CI. This page is the catalogue.

If a property is in this list, the implementation enforces it. If you can construct a counterexample, that is a bug.

| Property | Where checked | Failure mode | Source |
|---|---|---|---|
| Protocol registration produces a valid theory | Compile-time (panic at registration) | Named intermediate colimit step in panic message | [`panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs) |
| Schema validates against its protocol | Runtime | `schema validate` runs structural checks plus theory typechecking and exits non-zero on failure; failing-equation reporting lives in `schema verify` | `panproto-schema` |
| Migration existence conditions hold | Runtime, before any data is moved | `schema check` exits non-zero, naming the missing input | `panproto-check` |
| Migration type-checks at the GAT level | Runtime, on demand via `--typecheck` | `schema check --typecheck` exits non-zero, naming the offending sort or operation | `panproto-mig` |
| Migration is a theory morphism on its mapped fragment | Compile time, in `mig::compile` | Compilation fails with `NotAMorphism`, naming the offending edge or sort | `panproto-mig` |
| Migration structure is well-formed (vertex maps reference existing vertices; each mapped edge lands on the images of its own endpoints) | VCS stage, commit, and merge | Recorded as a migration error; stage marks the schema invalid, and commit and merge return `VcsError::ValidationFailed`. `CommitOptions.skip_verify` is the only bypass | `panproto-vcs` |
| Schema satisfies its registered protocol theory's equations | VCS commit and merge, when the protocol is registered with an equation-bearing theory | Set-theoretic model check bounded at 10,000 assignments per equation; a violation blocks with `VcsError::ValidationFailed`, and an equation whose assignment space exceeds the bound raises `ModelCheckLimitExceeded` naming it rather than passing silently. An unregistered protocol records an advisory note that no equations were checked | `panproto-vcs`, `panproto-gat` |
| Lens GetPut law: `put(s, get(s), complement(s)) = s` | CI property tests over generated scenarios (identity and projection families, depth-3 nested trees, vertex and edge remaps, field transforms) with generated put-side views, plus DSL-compiled lenses across the step constructors; sample-based (evidence, not proof), alongside a deterministic runtime check on a given instance | Property-test failure with shrunk counterexample | `panproto-lens/src/laws.rs`, `panproto-lens-dsl/tests/step_laws.rs` |
| Lens PutGet law: `get(put(s, v, c)) = v` | CI property tests over generated scenarios (identity and projection families, depth-3 nested trees, vertex and edge remaps, field transforms) with generated put-side views, plus DSL-compiled lenses across the step constructors; sample-based (evidence, not proof), alongside a deterministic runtime check on a given instance | Property-test failure with shrunk counterexample | `panproto-lens/src/laws.rs`, `panproto-lens-dsl/tests/step_laws.rs` |
| Lens PutPut law: `put(put(s, v₁, c), v₂, c) = put(s, v₂, c)` | CI property tests over generated scenarios (identity and projection families, depth-3 nested trees, vertex and edge remaps, field transforms) with generated put-side views, plus DSL-compiled lenses across the step constructors; sample-based (evidence, not proof), alongside a deterministic runtime check on a given instance | Property-test failure with shrunk counterexample | `panproto-lens/src/laws.rs`, `panproto-lens-dsl/tests/step_laws.rs` |
| Edit-lens `TreeEdit` monoid-action coherence: `apply(compose(e₁, e₂), s)` agrees with `apply(e₂, apply(e₁, s))`, with the identity and associativity laws | CI property tests over generated edit words; sample-based (evidence, not proof) | Property-test failure with shrunk counterexample | `panproto-lens/src/edit_laws.rs` |
| Edit-lens `get_edit` functoriality: `get_edit(compose(e₁, e₂))` acts on the view as `get_edit(e₁)` then `get_edit(e₂)`, and preserves the identity edit | CI property tests over generated edit words; sample-based (evidence, not proof) | Property-test failure with shrunk counterexample | `panproto-lens/src/edit_laws.rs` |
| Edit-lens consistency: a translated source edit applied downstream agrees with the source edit applied and re-viewed, compared over full instance structure (values, extra-fields, arcs, fans, parents) rather than node counts | Runtime, on a given edit and instance | `EditLawViolation::Consistency`, with the diverging detail | `panproto-lens/src/edit_laws.rs` |
| Edit-lens complement coherence: the edit-lens complement and the whole-state complement agree over full complement structure (dropped nodes with values, arcs, fans, contraction choices) rather than dropped-node counts | Runtime, on a given edit and instance | `EditLawViolation::ComplementCoherence`, naming the divergent field | `panproto-lens/src/edit_laws.rs` |
| Declared coercion class round-trips on sampled inputs (a declared `Iso` or `Retraction` whose expression fails its round-trip laws is refused) | Lens-DSL compile (`coerce_sort` and each `directed_equations` entry), the theory-DSL default `compile` (with `compile_unchecked` as the documented escape hatch), and the checked construction path (`*_coercion_checked` constructors); sample-based (evidence, not proof) | `LensDslError::CoercionNotHonest` and `TheoryDslError::CoercionLawViolation` at compile; `CoercionHonestyError` at construction | `panproto-lens/src/coercion_laws.rs`, `panproto-theory-dsl/src/compile.rs` |
| Complement-cost subadditivity: `cost(complement(g ∘ f)) ≤ cost(f) + cost(g)`, over vertical and horizontal composition, chain fusion, and data-level composition | CI property tests (≥256 cases); sample-based (evidence, not proof) | Property-test failure with shrunk counterexample | `panproto-lens/src/cost.rs` |
| Source-code emit round-trips its grammar's full corpus (`emit(parse(emit(s))) == emit(s)` plus vertex-kind and edge-shape multiset preservation, on every corpus entry, for each of the 248 corpus-gated protocols) | CI test (`emit_corpus_audit`) | Test panic naming the protocol and first divergent corpus entry | [`panproto-parse/tests/emit_corpus_audit.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-parse/tests/emit_corpus_audit.rs) |
| Complement composition compatibility | Runtime, on `Complement::compose` | `LensError::ComplementFingerprintMismatch` | `panproto-lens/src/asymmetric.rs` |
| Complement composition agreement | Runtime, on `Complement::compose` | `LensError::ComplementConflict` (with offending key) | `panproto-lens/src/asymmetric.rs` |
| Protolens composition: structural equality of the intermediate endofunctor | Runtime, on `vertical_compose` | `LensError::CompositionMismatch` | `panproto-lens/src/protolens.rs` |
| Pushout cocone commutativity | Runtime, on constructions through `colimit()` / `pushout_by_name` (including protocol registration); the raw `colimit_by_name` union used by the C FFI does not run the cocone check | Returned as part of `ColimitResult` | `panproto-gat/src/colimit.rs` |
| Colimit inclusion morphisms preserve signatures and equations | Runtime, on constructions through `colimit()` / `pushout_by_name` | `check_morphism` on each inclusion against the pushout theory; a non-injective leg is rejected deterministically with `GatError::NonInjectiveIdentification` naming the element and its conflicting preimages | `panproto-gat/src/colimit.rs` |
| Pushout universal property: every alternative cocone factors uniquely through the pushout | Runtime, on demand via `verify_universal`; the mediator it builds is validated with `check_morphism` before the factorization comparisons, so a mediator that factors while violating signature or equation preservation is rejected | `EquationNotPreserved`, or a `GatError` from the morphism check | `panproto-gat/src/colimit.rs` |
| Schema merge pushout (cocone level: migration totality and base-vertex commutativity) | Runtime, at merge time via `vcs::merge::verify_pushout` | `VcsError::PushoutVerification` | `panproto-vcs/src/merge.rs` |
| Schema merge universal property (vertex level) | On demand via `vcs::merge::verify_pushout_universal`; not called by schema merge | `PushoutError::UniversalFactorizationFailure` | `panproto-vcs/src/merge.rs` |
| Expression evaluation totality (within step budget) | Runtime, on every evaluation | `ExprError::StepLimitExceeded` | `panproto-expr/src/eval.rs` |
| Expression arithmetic overflow check | Runtime, on every arithmetic op | `ExprError::Overflow` | `panproto-expr/src/builtin.rs` |
| Expression division by zero check | Runtime, on `Div`/`Mod` | `ExprError::DivisionByZero` | `panproto-expr/src/builtin.rs` |
| Expression builtin dispatch is total (a misrouted op returns an error rather than panicking) | Runtime, on every builtin application | `ExprError::InternalDispatch` naming the op | `panproto-expr/src/builtin.rs` |

## Source-code emit coverage

The source-code emitter (`emit_pretty`) qualifies **255 of the 261** vendored tree-sitter grammars on the two bases described in [Source-code emission](./emit-pretty.md#the-verification-tier-api). **248** are corpus-gated: every entry in the grammar's upstream `test/corpus/` round-trips under the strict oracle described in the row above. The other **7** (`python`, `stan`, `bugs`, `jags`, `julia`, `scheme`, `javascript`) are backend-verified: covered by dedicated emit regression tests over the construct surface the quivers transpile backends actually emit, with full corpus pass tracked as follow-on work. The remaining six of the 261 are blocked upstream, not by the emitter:

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
- **Cocone commutativity on the raw `colimit_by_name` union.** The name-based `colimit_by_name` path used by the C FFI (`pp_gat_colimit`) builds no inclusion morphisms and runs no `verify_cocone`; only constructions through `colimit()` / `pushout_by_name` (including protocol registration) are checked.
- **Edge, coverage, and deletion conditions at merge time.** The merge-time `verify_pushout` is vertex level: it checks migration totality and base-vertex commutativity only. Strengthening it with merged-vertex coverage, deletion, and edge-leg checks is planned; until then, restore this note to name those conditions once the strengthening lands.
- **Format-preserving round-trips in the default `schema` binary.** Byte-for-byte round-trips require the `tree-sitter` feature, which the shipped `schema` binary does not enable. A format-preserving parse requested from that binary returns canonical output with no layout complement and prints a notice to stderr; enabling the feature in a source build restores preservation. See [Round-trip with format preservation](../how-to/format-preserving.md).

## See also

- [Schema version control semantics](./vcs-semantics.md) for the merge case.
- [Lenses and round-trip laws](./lenses-roundtrip.md) for the lens case.
- [Migrations as morphisms](./migrations-as-morphisms.md) for the migration case.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the universal-property statement.
