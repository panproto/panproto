# The git bridge

Panproto-vcs repositories should interoperate with [git](https://git-scm.com/). A repository under active development may be pushed to a GitHub or GitLab remote that runs ordinary git, reviewed through git-aware tooling, and later pulled back into a panproto-vcs working copy without losing the schema-level history. This chapter covers the bridge that makes this possible: [`panproto-git`](https://docs.rs/panproto-git/latest/panproto_git/) for the bidirectional translation of repository state, and the [`panproto-git-remote`](https://docs.rs/panproto-git-remote/latest/panproto_git_remote/) helper (installed as `git-remote-panproto`) that lets git itself speak to a panproto-vcs remote through the standard `git push` and `git fetch` commands.

## Two translations

The bridge performs two translations. The *import* translation takes a git repository (either local or fetched from a remote) and produces a panproto-vcs repository whose object database includes every git commit as a panproto-vcs commit, every git tree as a panproto-vcs tree, and every git blob as a panproto-vcs blob. Schemas, migrations, and instances the git repository did not encode remain absent; an imported git history is an ordinary byte-tree history, not a panproto-enhanced one.

The *export* translation goes the other way: a panproto-vcs repository is projected onto a git repository by dropping the schema, migration, and instance objects and keeping only the byte-level objects. The export is lossy by construction; what survives is the tree of ordinary files, with schemas serialised to their on-disk form (JSON for ATProto lexicons, Avro IDL for Avro schemas, and so on), migrations serialised to their declaration syntax, and instances serialised through each protocol's emitter.

The two translations compose on the byte level: importing an exported panproto-vcs repository yields back the same byte history, modulo the on-disk representation of schemas and migrations. They do not compose on the panproto-specific level: importing an exported repository loses the schema-level identifications, since the export has thrown away the schema object kind and produced bytes-on-disk representations that the import layer does not re-identify. A round trip therefore preserves bytes but not the enhanced schema history unless the importer is told explicitly how to re-parse the disk representations.

## Functoriality

Both translations are functors in the categorical sense. The import functor $I : \mathbf{Git} \to \mathbf{Panproto}$ sends every git commit DAG to a panproto-vcs commit DAG in a way that respects parent relationships and preserves content-addressing: two git commits with the same hash map to panproto-vcs commits with the same hash (modulo the SHA-1-to-blake3 conversion, which is applied through a deterministic per-object re-hashing pass). The export functor $E : \mathbf{Panproto} \to \mathbf{Git}$ respects the same relationships in the opposite direction.

The functor laws of [the Functors chapter](../foundations/functors.md) apply here. Importing a chain of git commits and importing the last one in the chain produces the same panproto-vcs state, which is the composition axiom. Importing an empty git history produces an empty panproto-vcs state, which is the identity axiom. The same holds for the export functor in its direction. The tests in [`panproto-git`](https://docs.rs/panproto-git/latest/panproto_git/) verify both laws on a standard suite of representative git histories.

## The git remote helper

[`panproto-git-remote`](https://docs.rs/panproto-git-remote/latest/panproto_git_remote/) ships a git remote-helper binary called `git-remote-panproto`. Git's remote-helper protocol lets third parties define new transports by shipping an executable named `git-remote-<scheme>` that speaks the helper protocol on its stdin and stdout. When the user runs `git push panproto::/path/to/panproto-vcs-repo main`, git invokes `git-remote-panproto` with the URL and the standard helper commands, and `git-remote-panproto` translates each command into the corresponding panproto-vcs operation through the crate's API. The legacy `cospan::` scheme is still accepted as an alias.

The result is that a panproto-vcs repository appears to git as an ordinary remote. `git push panproto::/path/to/repo main` sends the local `main` branch to the panproto-vcs repository, which runs the import translation on the branch's commits as they arrive. `git fetch panproto::/path/to/repo` retrieves panproto-vcs commits and makes them available as git refs, with the export translation running as commits cross the boundary.

The helper binary is installed through `cargo install panproto-git-remote`. Once installed, git picks it up automatically for any URL of the form `panproto::...` (or `cospan::...`).

## What the bridge preserves

On the import side, everything git records survives: commits, trees, blobs, refs, authorship metadata, messages. The content-addressing is translated from SHA-1 to blake3, which produces different hash values but preserves the DAG structure.

On the export side, only the byte-level content survives. Schemas, migrations, and instances are serialised to their disk representations and written as files; the panproto-specific object kinds are no longer addressable. A git reader who picks up the exported repository has access to the bytes but not to the schema-level operations.

A panproto-vcs repository that wants to use git as a remote for collaboration typically ensures that its schemas are exported in a format the collaborators can read directly (ATProto lexicons as JSON, for example). A panproto-vcs repository that uses git only for backup can ignore the export concerns: the exported repository is decoration, and the canonical form is the panproto-vcs database on the original machine.

## What the bridge does not preserve

Schema-level merges. A three-way merge that succeeds as a pushout in the panproto-vcs setting may not have a byte-level representation that git can merge cleanly. When the export translation runs on a panproto-vcs merge commit, it produces a git commit whose tree reflects the merged schema's byte form but whose diff against the parents is at the byte level. A git user pulling the exported repository sees a merge commit that touched a schema file and may find that git's three-way line merge on the schema file's bytes produces conflict markers; the schema-level resolution panproto-vcs reached is not visible as a git-level merge strategy.

The common mitigation is to encode the panproto-vcs merge as a pair of sequential commits in the exported git history: the left branch is merged first, producing an intermediate byte-level state; the right branch is merged against that intermediate state. This gives git a clean byte-level history even when the panproto-vcs merge operated at the schema level. The exporter does this automatically through a flag on [`panproto_git::export`](https://docs.rs/panproto-git/latest/panproto_git/).

## Further reading

For git's own extensibility mechanisms that the bridge uses, the [git-remote-helpers documentation](https://git-scm.com/docs/gitremote-helpers) is the authoritative reference. @chaconstraub2014pro covers git's object model at the depth the bridge depends on; understanding how git stores commits, trees, and blobs is a prerequisite for understanding what the bridge has to translate and what it cannot.

For the deeper question of what can and cannot be preserved in a bidirectional translation between two version-control systems with different object models, the bidirectional-transformations literature ([Bidirectional lenses](../core/lenses.md)) applies. The git bridge is itself a lens in the categorical sense: a `get` (export to git) and a `put` (import from git) with round-trip laws documented in the chapter above.

## Closing

Part V ends with this chapter. Part VI turns to the operational layer: the [WebAssembly boundary](../sdks/wasm-boundary.md) through which every non-Rust client interacts with panproto, the [Rust SDK](../sdks/rust.md), the [TypeScript SDK](../sdks/typescript.md), the [Python SDK](../sdks/python.md), and the [CLI](../sdks/cli.md).
