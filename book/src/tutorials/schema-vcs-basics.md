# Schema version control basics

Two schema files record two states; a repository records how those states relate over time. In this tutorial, the `schema` CLI commits a TypeScript interface, creates a feature branch, and merges the branch back into `main`.

## Prerequisite

Install the `schema` binary by following [Install the CLI](../how-to/install/cli.md). The commands below create a new `vcs-tutorial/` directory.

## Commit the first version

Create the repository and its first source file:

```sh
mkdir -p vcs-tutorial/src
cd vcs-tutorial
schema init

cat > src/user.ts <<'EOF'
export interface User {
  name: string;
  age: number;
}
EOF

schema add src/user.ts
schema commit -m "v1 user schema"
schema log --oneline
```

*Listing 5.1: Initializing a repository and committing a parsed TypeScript schema.*

`schema add` parses `src/user.ts` through the [tree-sitter](https://tree-sitter.github.io/tree-sitter/) registry and stages the resulting schema graph. The commit stores that graph under `.panproto/`; the source file remains an ordinary TypeScript file. Run `schema add` from the repository root so the command can find `.panproto/`.

## Commit the rename

Replace the file with v2, inspect the staged diff, and commit it:

```sh
cat > src/user.ts <<'EOF'
export interface User {
  name: string;
  years: number;
  email: string;
}
EOF

schema add src/user.ts
schema diff --staged
schema commit -m "rename age and add email"
schema log --oneline
```

*Listing 5.2: Staging and committing the second schema state.*

The staged diff compares schema structure rather than source lines. Full-AST parsers also preserve syntax-level structure needed for source round trips, so a source edit may produce more graph changes than the two interface fields alone suggest.

## Branch and merge

Create and switch to a feature branch, add `handle`, then merge the branch into `main`:

```sh
schema checkout -b feature/handle

cat > src/user.ts <<'EOF'
export interface User {
  name: string;
  years: number;
  email: string;
  handle: string;
}
EOF

schema add src/user.ts
schema commit -m "add handle"
schema checkout main
schema merge feature/handle
schema log --oneline
```

*Listing 5.3: Creating a feature branch and merging it into `main`.*

`main` has not moved since the branch was created, so this merge is a fast-forward. The command reports `Merge successful.` and moves the `main` ref to the feature commit.

There is one operational difference from [git](https://git-scm.com/): `schema checkout` moves the schema-history ref but does not rewrite `src/user.ts`. Panproto stores and merges parsed schemas; your editor, build system, or an explicit emit step remains responsible for working-source files. `schema log --graph` currently accepts `--graph` but renders the ordinary log, so the examples use `--oneline`.

## Next

The [schema version control how-to](../how-to/schema-vcs/index.md) covers non-fast-forward merges, data versioning, and the git bridge. [Schema version control semantics](../explanation/vcs-semantics.md) explains why the merge operation is structural, while [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md) develops the formal construction.
