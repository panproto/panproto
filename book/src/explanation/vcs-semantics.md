# Schema version control semantics

`panproto-vcs` stores immutable schema-related objects in a content-addressed directed acyclic graph (DAG). Mutable branch and tag references point into that graph. Its command vocabulary includes familiar operations such as `init`, `add`, `commit`, `branch`, `merge`, `log`, and `diff`, but the stored objects and merge algorithm operate on parsed schema structure rather than lines of source text.

This distinction changes the form of a conflict. A text merge reports overlapping edits to lines. A schema merge compares vertices, edges, constraints, and the other fields of the common representation, then reports incompatible structural edits as typed conflict values. Syntax and protocol validation still run separately; structural merge alone does not guarantee that every merged schema is valid.

## Objects and references

Every object identifier is a BLAKE3 digest of the object's canonical serialization. Objects are stored under `.panproto/objects/`, branch references under `.panproto/refs/heads/`, and tag references under `.panproto/refs/tags/`.

| Object | Contents |
|---|---|
| `FileSchema`, `SchemaTree`, `FlatSchema` | Per-file schema content, a tree root used by commits, and a flattened migration endpoint. |
| `Migration` | A map between identified source and target schemas. |
| `Complement`, `CstComplement` | Saved data for inverse migration and concrete-syntax reconstruction. |
| `DataSet` | Instances associated with a particular schema. |
| `Protocol`, `Theory`, `TheoryMorphism`, `Expr`, `EditLog` | Protocol and transformation metadata referenced by other objects. |
| `Commit` | A `SchemaTree` root, parent commits, protocol and author metadata, and identifiers for associated migrations, data, complements, edit logs, theories, and renames. |
| `Tag` | An annotated reference to another stored object. |

A branch is a mutable reference rather than an immutable object. A commit may have more than one parent, so the commit relation forms a DAG rather than a simple sequence.

## Validation at stage and commit

Migration validation checks that mapped vertex and edge identifiers exist at both endpoints and that each mapped edge lands between the images of its source endpoints. The same structural obligation is checked during migration compilation, where a failure is reported as `NotAMorphism`. By default, migration errors make staged content invalid and block commit with `VcsError::ValidationFailed`.

Verification can be bypassed deliberately at two points. `AddOptions::skip_verify` permits an object to remain pending at stage time, and `CommitOptions::skip_verify` bypasses the commit-time check. These options weaken the repository invariant and should be treated as explicit overrides rather than ordinary workflow.

When a registered protocol theory contains equations, model validation checks the schema against them. The evaluator considers at most 10,000 variable assignments for an equation; exceeding that bound returns `ModelCheckLimitExceeded` instead of accepting the equation. If no theory is registered, validation records an advisory that no equations were checked while retaining the available structural checks. The current foundational theories and the built-in ATProto composition do not themselves supply equations, so the presence of a registered theory does not imply that an equation check has substantive cases.

## Structural three-way merge

Let $B$ be a common base and $O$ and $T$ the schemas on the two branches. The categorical account reads merge as a pushout of the divergent changes over their base [@mimramdigiusto2013categorical]:

$$
\begin{CD}
 B @>>> O \\
 @VVV @VVV \\
 T @>>> M.
\end{CD}
$$

The implementation constructs $M$ with a field-by-field structural three-way merge. Compatible additions and modifications are combined. Incompatible edits become `MergeConflict` variants, and conflicted elements retain their base values until the caller supplies a resolution. `apply_resolutions` requires a choice of ours or theirs for every reported conflict and then verifies the resulting square.

Combining compatible additions means that two branches adding the same name compatibly contribute one element rather than one each, so $M$ is the pushout quotiented by same-name identification. `MergeResult::identified_additions` reports every name collapsed this way. [Pushouts and merge](./semantics/pushouts-and-merge.md) states what that quotient costs.

The routine `verify_pushout` checks the generated cocone: both branch maps must be total, every merged vertex must come from a branch, surviving base vertices must remain present, and the two paths from the base must agree on mapped vertices and edges. A failure returns `VcsError::PushoutVerification`. This is a cocone check, not a complete runtime proof of the universal property.

`verify_pushout_universal` provides an additional on-demand check against a caller-supplied alternative cocone. It constructs and checks a mediator on vertices. The current API does not establish edge-level factorization, and ordinary merge does not call this verifier. [Pushouts and merge](./semantics/pushouts-and-merge.md) states the distinction formally.

Merge also computes a pullback overlap to recognize additions shared by both branches. If that computation fails, the result stores a `pullback_error` and the CLI reports it. The failure is not interpreted as an empty overlap.

## Data associated with history

Commits may reference data sets and migration complements. During merge, data from both parents is lifted to the merged schema, fresh complement objects are recorded, and duplicate migrated data sets are removed. Rebase and cherry-pick similarly lift the replayed commit's data and verify the relevant migration square. These operations follow the schema-evolution account of migrations connected by lenses in Cambria [@littvanhardenberghenry2020cambria; @littvanhardenberghenry2021cambria].

Ordinary commit records data that was staged for that commit; it does not automatically copy or migrate all data from the previous commit. Amend preserves existing data identifiers unless the caller stages replacements. A schema change made through either operation thus does not by itself imply that associated data was transformed.

## Related work

The categorical account of patches and merge also includes homotopical patch theory [@angiuli2014homotopical] and Darcs [@roundy2005darcs]. Work on schema evolution includes the PRISM workbench and its schema-modification operators [@curinomoonzaniolo2008graceful]. panproto combines these ideas with content-addressed storage for protocols, schemas, data, migrations, and complements. [Related work](./related-work.md#schema-versioning-as-structured-merge) gives the broader comparison.

## See also

- [Init and commit](../how-to/schema-vcs/init-and-commit.md) for the practical workflow.
- [Branch and merge](../how-to/schema-vcs/branch-and-merge.md) for conflict handling.
- [Bridge to git](../how-to/schema-vcs/git-bridge.md) for using panproto-vcs alongside git.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the formal model.
- [What panproto verifies](./what-is-verified.md) for merge-time checks.
