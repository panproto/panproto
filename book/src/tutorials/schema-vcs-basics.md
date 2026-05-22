# Schema version control basics

You will turn a small TypeScript source file into a panproto repository, commit two versions of the schema parsed from that file, branch off a feature, and merge it back. About twenty minutes.

By the end you will have: a `.panproto/` directory tracking schema history, two commits on `main`, a feature branch with a third commit merged back, and an attempted conflict you can inspect.

## Prerequisites

The `schema` CLI installed ([Install the CLI](../how-to/install/cli.md)). This tutorial does not depend on the SDK; it stays at the CLI throughout.

## A note on schema input

`schema add` accepts three kinds of input:

- a source file in a language the parser registry knows (TypeScript, Rust, Python, etc.) — parsed via tree-sitter into a full-AST schema;
- a directory — parsed as a project into a unified schema; or
- a `.json` file whose content is a serialised `panproto-schema::Schema` struct.

The hand-authored Schema JSON format is non-trivial (it's the in-memory representation, not a pretty document language), so most workflows use the first two. This tutorial uses the source-file path.

## Step 1: initialise

```sh
mkdir vcs-tutorial/src -p && cd vcs-tutorial
schema init
ls .panproto/
```

A `.panproto/` directory appears, structurally similar to `.git/`. It has `objects/`, `refs/`, `HEAD`, and a small config.

## Step 2: commit v1

Save the v1 user shape as `src/user.ts`:

```ts
export interface User {
  name: string;
  age: number;
}
```

Stage and commit:

```sh
schema add src/user.ts
schema commit -m "v1 user schema"
schema log --oneline
```

`schema add` runs the TypeScript tree-sitter parser, extracts a schema from the AST (vertices for the interface and its property types, edges for the properties, plus a fine layer of byte-position constraints used for round-tripping), and stages it. `schema log --oneline` now shows one commit with a blake3 hash.

## Step 3: commit v2

Edit `src/user.ts` to rename `age` and add `email`:

```ts
export interface User {
  name: string;
  years: number;
  email: string;
}
```

Stage and commit:

```sh
schema add src/user.ts
schema diff --staged
schema commit -m "v2: rename age to years, add email"
schema log --oneline
```

`schema diff --staged` shows the structural changes (a renamed property, a new property, plus the tree-sitter byte-position constraint deltas). Two commits now.

## Step 4: branch

```sh
schema branch feature/handle
schema checkout feature/handle
```

You are on a new branch sharing history with `main` up to the v2 commit. Add a `handle` field:

```ts
export interface User {
  name: string;
  years: number;
  email: string;
  handle: string;
}
```

Then:

```sh
schema add src/user.ts
schema commit -m "add handle"
schema log --oneline
```

You see the linear history with the new commit on `feature/handle`.

## Step 5: merge

```sh
schema checkout main
schema merge feature/handle
schema log --oneline
```

Since `main` is an ancestor of `feature/handle` (no conflicting work was done on `main` in parallel), the merge is a fast-forward and produces no merge commit. The output ends with `Merge successful.` and reports the merged schema's vertex/edge counts. For a non-trivial three-way merge, pass `--no-ff` to force a merge commit.

## Step 6: provoke a conflict

For a clean conflict demonstration, start fresh in a new repo so the conflicting branches diverge from the same commit:

```sh
cd .. && mkdir vcs-conflict/src -p && cd vcs-conflict
schema init
cat > src/user.ts <<'EOF'
export interface User {
  name: string;
  age: number;
}
EOF
schema add src/user.ts && schema commit -m "v1"

# Create both branches at v1 (each branch starts at HEAD).
schema branch feature/handle-str
schema branch feature/handle-int

schema checkout feature/handle-str
# edit src/user.ts: add  handle: string;
schema add src/user.ts && schema commit -m "handle as string"
schema checkout main
schema merge feature/handle-str    # fast-forward, succeeds

schema checkout feature/handle-int
# edit src/user.ts: add  handle: number;
schema add src/user.ts && schema commit -m "handle as number"
schema checkout main
schema merge feature/handle-int
```

The second merge fails with conflict objects. For the type clash above you see entries like:

```text
Merge produced 3 conflict(s):
  BothAddedConstraintDifferently {
    vertex_id: "src/user.ts::User::$2::$11",
    sort: "literal-value",
    ours_value: "string",
    theirs_value: "number"
  }
  ...
Error:   × merge failed with 3 conflict(s)
```

Each conflict is a structural object reported by the pushout construction, not a textual three-way merge marker. The variants you can hit include `BothAddedConstraintDifferently` (used here, when two branches set the same constraint to different values) and `UniversalFactorizationFailure` (raised by `panproto-vcs`'s pushout when no universal merge object exists at all). To resolve, choose one branch's shape, edit the source file, `schema add`, `schema commit`.

## What you built

A schema history that you can navigate, branch, and merge with the same affordances as a git history, but where the merge operation is a precise structural construction rather than a three-way text merge.

## Next

- [Cross-protocol translation](./cross-protocol-translation.md) for translating the schema from JSON Schema to Protobuf.
- The plain-terms explanation of merge is at [Schema version control semantics](../explanation/vcs-semantics.md).
- The formal pushout construction is in [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md).
