# Schema version control semantics

## In plain terms

panproto-vcs is git, but for schemas. It tracks a history of schemas the way git tracks a history of source files: commits, branches, tags, merges, diffs, blame. The CLI verbs are the same (`init`, `add`, `commit`, `branch`, `merge`, `log`, `diff`).

Two things make it different from git applied to the schema files themselves:

1. **The diff and merge operate on the schema, not the text.** `schema diff` does not show you a unified diff of the JSON; it shows you what changed structurally: which vertices were added, which edges renamed, which constraints tightened. Merge does not three-way-merge the bytes; it merges the schema graph at the structural level, so you cannot end up with a syntactically valid but semantically broken schema after a merge.
2. **Data and lenses are versioned alongside the schemas.** Every commit records a schema snapshot, the lenses generated against that schema, and (optionally) the data instances that conformed to it. Branches diverge with their data; merges reconcile both.

The merge operation is the place where this gets interesting. Three-way text merge fails when both sides edit the same line. The schema-level analogue is two branches that both add a field with the same name but different types. panproto-vcs has a precise, well-defined operation for resolving this: the schemas are *pushed out* along their common ancestor. The result is the smallest schema containing both branches' additions, with the conflict surfaced as an explicit refinement constraint that the user resolves.

## The DAG

panproto-vcs is structured exactly like git: a content-addressed DAG of immutable objects.

| Object | What it holds |
|---|---|
| Schema | The schema graph at a point in time, hashed with blake3. |
| Migration | A morphism between two schemas. |
| Lens | A bidirectional transform between two schemas. |
| Data | An instance of a schema; an instance graph also content-addressed. |
| Commit | A pointer to a schema, an optional pointer to data, a parent commit list, an author, a message. |
| Tag | A named pointer to a commit. |
| Branch | A mutable reference to a commit. |

Refs (branches and tags) live under `.panproto/refs/`. Objects live under `.panproto/objects/`. The structural similarity to `.git/` is intentional: the existing mental model transfers.

## Merge as pushout

A three-way merge in git is: take base $B$, ours $O$, theirs $T$, and produce a result $M$ that contains the changes from $O$ relative to $B$ and the changes from $T$ relative to $B$. When the changes overlap on the same line, conflict.

The schema analogue: $B$, $O$, $T$ are schemas; $O$ and $T$ are both descendants of $B$. The merge result $M$ is the *pushout* of $O$ and $T$ along $B$:

```text
        B ------> O
        |         |
        |         |
        v         v
        T ------> M
```

The pushout is the *unique smallest* schema containing both $O$ and $T$ and respecting their shared structure from $B$. "Unique smallest" is made precise by a *universal property*: any other schema $M'$ that also contains $O$ and $T$ admits a unique morphism from $M$ to $M'$.

panproto-vcs does not just compute the pushout: it *verifies* the universal property. `vcs::merge::verify_pushout_universal` checks that the merge result mediates uniquely from any alternative cocone, returning the mediator vertex map. If the universal-property check fails, the merge raises `UniversalFactorizationFailure` rather than producing a wrong result.

For the formal pushout construction, the cocone definition, and exactly what is checked, see [Pushouts and merge](./semantics/pushouts-and-merge.md).

## Conflicts

A merge conflict arises when the pushout would introduce an inconsistency: two branches add a field with the same name but incompatible types, or one branch removes a vertex the other branch still references. Conflicts are reported as explicit objects (rather than text markers) and resolved by editing the conflict descriptor.

## Data versioning

Commits can carry data instances. When a branch's schema migrates, the data carried by its commits is automatically lifted forward by the migration's lens. Branches can therefore diverge in both schema and data; merging both kinds of divergence in one operation is what `schema merge` does.

A consequence: history rewriting (rebase, amend) on a branch carrying data must lift the data through the rewritten history. panproto-vcs does this; the data is *not* a passive blob.

## See also

- [Init and commit](../how-to/schema-vcs/init-and-commit.md) for the practical workflow.
- [Branch and merge](../how-to/schema-vcs/branch-and-merge.md).
- [Bridge to git](../how-to/schema-vcs/git-bridge.md) for using panproto-vcs alongside git.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the formal model.
- [What panproto verifies](./what-is-verified.md) for the universal-property guarantee.
