# Correctness

This part of the book holds the written proofs of panproto's laws. Each page covers one family of laws: the value layer, theories, keys and registration, the representation of instances, the Kan extensions, restrict and contraction, edits, validation, segments, the lenses and reference resolution, with further pages for each data format as its laws are proved. Each law has one block: the law's statement with its exact domain, a complete proof, the code that implements the law, and a record of the independent audit the proof passed.

A proof on these pages is one of four obligations a law carries, and it is read together with the other three. First, the law has an executable reference semantics in the [`panproto-reference`](https://github.com/panproto/panproto/tree/main/crates/panproto-reference) crate: a deliberately naive function written straight from the law's definition, with exact arithmetic and no dependency on the code it checks. Second, exhaustive and property-based differential tests run the production code and that same function and require exact agreement, including the reason for a typed refusal. The exhaustive test visits every input of its stated finite scope, prints the bound and the number of inputs compared, and checks the count against a source independent of the enumerator. The property test records each seed and every shrunk counterexample and passes on three consecutive runs. Third, a mutation run over the files that implement the law shows that the tests detect each change to the code, and every mutant that survives is proved equivalent to the original on its block. A law whose block is missing, unaudited or stale is not established, whatever its tests report.

## Scope

A page states a law exactly as the implementation is required to satisfy it, including the domain on which it holds and the typed refusal returned outside that domain. A page does not restate the explanation of the construction, which lives in the [Explanation](../explanation/index.md) part, and it uses the notation of [Shared notation](../explanation/semantics/shared-notation.md). A proof cites only sources whose passage and locator were read and recorded, and claims no novelty.

## The proof format

Every law is one block. The block opens with a level-2 heading whose title names the law and whose heading attribute is the law's anchor:

```text
## Coercion is idempotent {#L-values-1}
```

The anchor has the form `L-<page>-<n>`, where `<page>` is the page's path under `correctness/` without `.md` (a `/` written as `-`) and `<n>` is a positive integer unique on the page. Anchors never change once published, so a law is cited by its anchor from anywhere in the book.

The heading is followed by exactly seven level-3 sections, in this order and with these names.

1. **Statement.** The law with its exact domain. Where the definition cross-references another law, the statement links to that law's anchor.
2. **Outside the domain.** For each input outside the domain, the typed refusal returned (by variant name), or the reason the case cannot arise.
3. **Definitions used.** Each term the statement and proof use, defined on the page or linked to [Shared notation](../explanation/semantics/shared-notation.md).
4. **Proof.** A complete proof in numbered steps. Each step is justified by a definition, by a law proved earlier on a correctness page (cited by its anchor), or by a published source with its locator, marked as stated or proved there. A case analysis lists every case. The phrases "clearly" and "it is easy to see" are not accepted, and "by symmetry" is accepted only with the symmetry stated.
5. **Executable counterpart.** This section uses the following exact line forms, in this order: one semantic law id, one line per reference function in manifest order, one finite enumerator and bound, one exhaustive differential test and its directly runnable command, one property differential test and its directly runnable command, and one line per source file the law covers.

   ```text
   - Law ID: `<semantic-id>`
   - Reference: `panproto_reference::<module>::<item>`
   - Enumerator: `panproto_reference::<module>::<item>; bound: <label>=<positive decimal>`
   - Exhaustive differential test: `<test-name>; command: cargo nextest run -p <package> --test <binary> --no-tests=fail -E 'test(=<test-name>)'`
   - Property differential test: `<test-name>; command: cargo nextest run -p <package> --test <binary> --no-tests=fail -E 'test(=<test-name>)'`
   - File: `crates/<crate>/src/<file>.rs`
   ```

6. **Mutation equivalences.** For each mutant that survives the mutation run, a proof that it is equivalent to the original code on the law's domain, or "None."
7. **Audit record.** The six lines below, each exactly once.

```text
- Author: <role>
- Auditor: <role>
- Date: YYYY-MM-DD
- Commit: <sha>
- Verdict: pass
- Defects: <each defect found, with the change that closed it, or None.>
```

The author and auditor are named by distinct roles. The verdict is `pass` or `fail`, and only a block whose verdict is `pass` establishes its law. A statement its author cannot prove is reported as a defect and is never weakened to fit a proof; a law is weakened only after the original is proved false, by a counterexample.

## The audit procedure

Every block is audited by an auditor who did not write it, at the commit that adds or changes it. The auditor:

1. compares the Statement against the law's definition, the only permitted differences being cross-references replaced by anchors or page links;
2. checks every step of the Proof against the justification it gives, and every case analysis for a missing case;
3. runs the enumerator at its printed bound, confirms that the printed count matches the independent count, and tries counterexamples one size above the bound where the page states that the cost allows it;
4. checks that every surviving mutant has an equivalence proof in Mutation equivalences;
5. checks each mutation-equivalence proof over every observable named by the law; and
6. writes the Audit record, with the commit audited and every defect found.

A failed audit returns the block to its author, and the block is audited again once the defects are fixed. An audit goes stale when a file named on a `- File:` line differs between the audited commit and the current one, or when the block itself, its Audit record left out, differs between them. The audited commit is the one before the commit that records the audit, so recording an audit does not make it stale. A stale or missing audit fails the correctness check, and a change that fails it does not merge.

## Status

The table lists each page represented in the `graph-core` scope, the number of
that page's laws included in the 191-law scope, and the commit at which those
included blocks were last audited.

| Page | Laws | Audited commit |
|---|---|---|
| [Values](values.md) | 41 | pending |
| [Value I/O](value-io.md) | 18 | pending |
| [Theories](theories.md) | 8 | pending |
| [Keys and registration](keys-and-registration.md) | 18 | pending |
| [Instance representation](representation.md) | 8 | pending |
| [Kan extensions](kan-extensions.md) | 12 | pending |
| [Restriction and contraction](restrict-and-contraction.md) | 13 | pending |
| [Edits](edits.md) | 8 | pending |
| [Validation](validation.md) | 20 | pending |
| [Segments](segments.md) | 10 | pending |
| [Lenses](lenses.md) | 20 | pending |
| [Reference resolution](resolution.md) | 15 | pending |
