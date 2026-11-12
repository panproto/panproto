# Correctness

This part of the book holds the written proofs of panproto's laws. Each page covers one family of laws: the value layer, theories, keys and registration, the representation of instances, the Kan extensions, restrict and contraction, edits, validation, segments, the lenses, reference resolution, migration search, the theory DSL, schema merge and instance version control, with further pages for each data format as its laws are proved. Each law has one block: the law's statement with its exact domain, a complete proof, and the code that implements and checks the law.

A proof on these pages is one of three obligations a law carries, and it is read together with the other two. First, the law has an executable reference semantics in the [`panproto-reference`](https://github.com/panproto/panproto/tree/main/crates/panproto-reference) crate: a deliberately naive function written straight from the law's definition, with exact arithmetic and no dependency on the code it checks. Second, exhaustive and property-based differential tests run the production code and that same function and require exact agreement, including the reason for a typed refusal. The exhaustive test visits every input of its stated finite scope, prints the bound and the number of inputs compared, and checks the count against a source independent of the enumerator. The property test records each seed and every shrunk counterexample and passes on three consecutive runs. Third, mutation testing of the files that implement the law checks that those tests detect changes to the implementation; a mutant that survives shows a gap in the tests, which is closed by a test that distinguishes it.

## Scope

A page states a law exactly as the implementation is required to satisfy it, including the domain on which it holds and the typed refusal returned outside that domain. A page does not restate the explanation of the construction, which lives in the [Explanation](../explanation/index.md) part, and it uses the notation of [Shared notation](../explanation/semantics/shared-notation.md). A proof cites only sources whose passage and locator were read and recorded, and claims no novelty.

## The proof format

Every law is one block. The block opens with a level-2 heading whose title names the law and whose heading attribute is the law's anchor:

```text
## Coercion is idempotent {#L-values-1}
```

The anchor has the form `L-<page>-<n>`, where `<page>` is the page's path under `correctness/` without `.md` (a `/` written as `-`) and `<n>` is a positive integer unique on the page. Anchors never change once published, so a law is cited by its anchor from anywhere in the book.

The heading is followed by exactly five level-3 sections, in this order and with these names.

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

A statement its author cannot prove is reported as a defect and is never weakened to fit a proof; a law is weakened only after the original is proved false, by a counterexample.

## Pages

The table lists each correctness page and the number of laws it proves for the tree, table and graph engine.

| Page | Laws |
|---|---|
| [Values](values.md) | 41 |
| [Value I/O](value-io.md) | 31 |
| [Theories](theories.md) | 8 |
| [Keys and registration](keys-and-registration.md) | 18 |
| [Instance representation](representation.md) | 8 |
| [Kan extensions](kan-extensions.md) | 12 |
| [Restriction and contraction](restrict-and-contraction.md) | 13 |
| [Edits](edits.md) | 11 |
| [Validation](validation.md) | 20 |
| [Segments](segments.md) | 31 |
| [Lenses](lenses.md) | 20 |
| [Reference resolution](resolution.md) | 16 |
| [Text formats](text-formats.md) | 4 |
| [PostgreSQL DDL](postgresql-ddl.md) | 3 |
| [PostgreSQL data](postgresql-data.md) | 4 |
| [RDF formats](rdf-formats.md) | 11 |
| [Edge lists](edge-lists.md) | 3 |
| [Kubernetes CRDs](kubernetes-crds.md) | 2 |
| [CloudFormation](cloudformation.md) | 2 |
| [Ansible](ansible.md) | 2 |
| [JSON Schema](json-schema.md) | 2 |
| [BSON](bson.md) | 3 |
| [Migration search](migration-search.md) | 4 |
| [Schema merge](schema-merge.md) | 4 |
| [Theory DSL](theory-dsl.md) | 4 |
| [Instance version control](instance-version-control.md) | 5 |
