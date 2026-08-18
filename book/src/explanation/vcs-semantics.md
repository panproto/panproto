# Schema version control semantics

## In plain terms

panproto-vcs is git, but for schemas. It tracks a history of schemas the way git tracks a history of source files: commits, branches, tags, merges, diffs, blame. The CLI verbs are the same (`init`, `add`, `commit`, `branch`, `merge`, `log`, `diff`).

Two things make it different from git applied to the schema files themselves:

1. **The diff and merge operate on the schema, not the text.** `schema diff` does not show you a unified diff of the JSON; it shows you what changed structurally: which vertices were added, which edges renamed, which constraints tightened. Merge does not three-way-merge the bytes; it merges the schema graph at the structural level, so you cannot end up with a syntactically valid but semantically broken schema after a merge.
2. **Data and migrations are versioned alongside the schemas.** Every commit records a schema snapshot and (optionally) the data instances that conformed to it; migrations between schemas are stored as their own content-addressed objects, paired with the complements needed to invert them. Branches diverge with their data; merges reconcile both.

The merge operation is the place where this gets interesting. Three-way text merge fails when both sides edit the same line. The schema-level analog is two branches that both add a field with the same name but different types. panproto-vcs has a precise, well-defined operation for resolving this: the schemas are *pushed out* along their common ancestor. The result is the smallest schema containing both branches' additions, with the conflict surfaced as an explicit refinement constraint that the user resolves.

## The DAG

panproto-vcs is structured exactly like git: a content-addressed DAG of immutable objects.

| Object | What it holds |
|---|---|
| `FileSchema` / `SchemaTree` / `FlatSchema` | A schema at a point in time, in per-file, tree, or migration-endpoint form. |
| `Migration` | A morphism between two schemas, identified by their object IDs. |
| `Complement` | The complement data needed to invert a data migration. |
| `DataSet` | A set of instances conforming to a specific schema. |
| `CstComplement` | The format-preserving CST data for byte-identical reconstruction. |
| `Protocol` / `Theory` / `TheoryMorphism` / `Expr` / `EditLog` | Supporting objects referenced by commits and migrations. |
| `Commit` | A pointer to a schema, an optional pointer to data, a parent commit list, an author, a message. |
| `Tag` | An annotated tag object pointing to another object. |
| Branch | A mutable reference to a commit; lives under `.panproto/refs/heads/`. |

Every object is content-addressed with a blake3 hash of its canonical serialization. Refs (branches under `refs/heads/`, tags under `refs/tags/`) live under `.panproto/refs/`. Objects live under `.panproto/objects/`. The structural similarity to `.git/` is intentional: the existing mental model transfers.

## Validation at stage, commit, and merge

A commit records a schema, and usually a migration from the previous schema together with the data carried forward through it. Before any of these is written, panproto-vcs checks the schema and the migration, and the checks block the operation rather than warn.

A migration is checked as a theory morphism on the fragment it maps: its vertex map must reference vertices that exist in the source and target schemas, and each mapped edge must land on the images of its own endpoints rather than on any pair of mapped vertices. A migration that violates this is recorded as a migration error, which marks the staged schema invalid and makes `commit` and `merge` fail with `VcsError::ValidationFailed`. The same morphism obligation is enforced earlier, at migration compile time, where `mig::compile` derives the induced theory morphism and validates it with `check_morphism`, failing with `NotAMorphism` when the mapped fragment is not structure-preserving. The single bypass is `CommitOptions.skip_verify`, which suppresses the staged check for a deliberate override.

Schemas are also checked against their protocol's equations. When the schema's protocol is registered with a theory carrying equations, as `atproto` is on the CLI path, the schema is read as a set-theoretic model and its equations are checked at commit and at merge; a violation blocks with `VcsError::ValidationFailed`. The equation check is bounded: it enumerates at most 10,000 variable assignments per equation, and an equation whose assignment space exceeds that bound raises `ModelCheckLimitExceeded` naming the equation rather than passing as if satisfied. When no theory is registered for the protocol, the commit records an advisory note that no equations were checked, and the structural checks still run.

## Merge as pushout

A three-way merge in git is: take base $B$, ours $O$, theirs $T$, and produce a result $M$ that contains the changes from $O$ relative to $B$ and the changes from $T$ relative to $B$. When the changes overlap on the same line, conflict.

The schema analog: $B$, $O$, $T$ are schemas; $O$ and $T$ are both descendants of $B$. The merge result $M$ is the *pushout* of $O$ and $T$ along $B$:

```text
        B ------> O
        |         |
        |         |
        v         v
        T ------> M
```

The pushout is the *unique smallest* schema containing both $O$ and $T$ and respecting their shared structure from $B$. "Unique smallest" is made precise by a *universal property*: any other schema $M'$ that also contains $O$ and $T$ admits a unique morphism from $M$ to $M'$.

panproto-vcs does not just compute the pushout: at merge time it runs `vcs::merge::verify_pushout`, a cocone-level check that the generated migrations are total, every merged vertex comes from one of the branches, surviving base vertices remain present, and the two paths agree on base vertices and edges. A failure returns `VcsError::PushoutVerification` rather than a wrong result. The stronger universal property, that the result mediates uniquely to a caller-supplied alternative cocone, is available on demand through `vcs::merge::verify_pushout_universal`, which schema merge does not itself call. That on-demand check constructs the mediator on vertices; edge-level factorization requires an extended alternative-cocone API (see [What panproto verifies](./what-is-verified.md)).

For the formal pushout construction, the cocone definition, and exactly what is checked, see [Pushouts and merge](./semantics/pushouts-and-merge.md).

## Conflicts

A merge conflict arises when the pushout would introduce an inconsistency: two branches add a field with the same name but incompatible types, or one branch removes a vertex the other branch still references. Conflicts are reported as explicit objects (rather than text markers) and resolved by editing the conflict descriptor.

Resolution is exhaustive over the conflict variants. Choosing a side for a conflict copies that side's element into the resolved schema, or deletes it, for every conflict kind the merge can raise; there is no silent fall-through that would return a resolved conflict at its base value. Where a merge computes the pullback overlap of the two branches to detect shared additions, a failure of that computation is recorded on the merge result (`pullback_error`) and surfaced by the CLI, rather than folded into an empty overlap that would read as "no shared additions".

## Data versioning

Commits can carry data instances. When a branch's schema migrates, the data carried by its commits is automatically lifted forward by the migration's lens. Branches can therefore diverge in both schema and data; merging both kinds of divergence in one operation is what `schema merge` does.

A consequence: history rewriting on a branch carrying data must lift the data through the rewritten history rather than copy it verbatim. This is what `rebase`, `merge`, and `cherry-pick` do, and what `amend` and `commit` preserve: each carries versioned data through the schema change by running the forward migration generated from that change's lens, applying `data_mig::migrate_forward` with the stored complements to lift every affected record. The data moves with the schema; it does not sit inert.

## Related work

Two threads sit directly behind panproto-vcs. The categorical-VCS lineage (Mimram and Di Giusto on patches as morphisms with merge as pushout [@mimramdigiusto2013categorical], Angiuli and colleagues' homotopical patch theory [@angiuli2014homotopical], Roundy's Darcs [@roundy2005darcs]) supplies the "merge is the pushout of the divergent patches against the common ancestor" semantics and the diagnosis of conflicts as failures of the pushout to exist. The schema-evolution lineage (Curino, Moon, and Zaniolo's PRISM workbench [@curinomoonzaniolo2008graceful] and Litt, van Hardenberg, and Henry's Cambria [@littvanhardenberghenry2020cambria; @littvanhardenberghenry2021cambria]) supplies the engineering vocabulary: schema-modification operators with forward and backward mappings, quasi-inverses for the operators that lose information, and a directed graph of schema versions connected by lenses. panproto-vcs is the four-artifact unification of these lines, with the protocol theory, schema, data, and lens complement committed together into a single content-addressed DAG. See [Related work](./related-work.md#schema-versioning-as-structured-merge) for the full discussion.

## See also

- [Init and commit](../how-to/schema-vcs/init-and-commit.md) for the practical workflow.
- [Branch and merge](../how-to/schema-vcs/branch-and-merge.md).
- [Bridge to git](../how-to/schema-vcs/git-bridge.md) for using panproto-vcs alongside git.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the formal model.
- [What panproto verifies](./what-is-verified.md) for what schema merge checks at merge time.
