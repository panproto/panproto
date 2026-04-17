# What git already versions and what it does not

[Git](https://git-scm.com/) is the version-control system most developers reach for by default. It works well, it is ubiquitous, and its internal object model is clean enough that a reader who has not looked at it is often surprised by how simple it is. This chapter lays out git's object model precisely enough to state what git versions and what git does not, and uses the gap to motivate the object model [`panproto-vcs`](https://docs.rs/panproto-vcs/latest/panproto_vcs/) puts in its place.

The chapter assumes no prior knowledge of git's internals beyond a working familiarity with `commit`, `branch`, and `merge` at the command line. Readers already comfortable with the Merkle DAG structure of git's object database can skim the first two sections and pick up at [What git does not version](#what-git-does-not-version). For a fuller account of git's internals than this chapter develops, @chaconstraub2014pro is the canonical reference. The patch-theoretic alternative pioneered by Darcs [@roundy2005darcs] offers a different perspective on distributed version control and is a point of reference throughout this part of the book. Operational concerns around replicated, eventually-consistent data at production scale are treated at length in @kleppmann2017designing.

## The three kinds of object

Git stores every piece of repository state as a *content-addressed object* in the object database. There are three kinds of object in the ordinary case: blobs, trees, and commits. A blob is a byte sequence (the contents of a file, with no filename or permissions attached). A tree is a list of (name, permission, object hash) triples corresponding to the entries of a directory; the object hash in each triple points at either another tree (a subdirectory) or a blob (a file). A commit is a record with a pointer to a tree (the repository state), zero or more parent commit hashes, author metadata, and a commit message.

Every object is identified by the SHA-1 (or SHA-256 in modern git) hash of its contents. The hash is the object's identity: two blobs with the same bytes are the same object; two trees that list the same entries are the same object. The Merkle-tree construction the arrangement is a special case of is due to @merkle1987digital. This content-addressing is what makes git's storage compact (identical files across branches share storage) and makes its integrity guarantees strong (any corruption of an object's bytes changes its hash, so references break visibly).

## The Merkle DAG

Every commit in a git repository points at a tree (its root directory), and every tree points at blobs and subtrees. Commits also point at their parent commits. The whole graph of objects is therefore a directed acyclic graph whose edges are the pointers between objects; the graph is a Merkle DAG in the sense that every object's identity is determined by the hashes of the objects it points at.

The Merkle DAG is what makes git's history immutable by construction. Rewriting any part of the history changes the hashes of every commit from the rewrite point forward. A repository refers to its current state through *refs* (branch and tag names), each of which is a pointer to a commit hash; a ref's value can be updated, but the commits the ref points to cannot be altered without producing different hashes and therefore different commits.

## What git versions

Git versions byte sequences. More precisely, it versions *directory trees of byte sequences*, with history tracked at the granularity of commits. A commit encodes the complete state of the repository at a moment in time, together with the lineage that produced it. Merges, branches, tags, diffs, blames, and bisects are all operations on this graph of content-addressed objects.

The operations work well. Merging two branches through a three-way algorithm whose inputs are the two branch tips and their common ancestor produces a merged tree that reflects both branches' edits wherever those edits commute; where they conflict (both branches modified the same byte range of the same file), the algorithm reports the conflict at the line level and asks a human to resolve it. Diffs are computed on bytes, usually line-by-line with heuristic matching for reordered chunks. Blame traces each line to the commit that introduced it.

All of this operates at the byte level. Git's idea of the content a repository holds is: arbitrary bytes, organised into named files, organised into named directories.

## What git does not version

Three things a working repository depends on are outside the model.

The first is *schema*. A repository may contain JSON records that conform to some agreed-upon JSON Schema, protobuf messages that conform to a `.proto` file, or tree-sitter-parsed source code that conforms to a grammar; git sees only the bytes. When the schema changes, git does not know to migrate the data; the developer must do so by hand, usually in a commit that simultaneously updates the schema file and rewrites every affected data file.

The second is *interpretation*. A CSV file and a TSV file may differ by one byte per row and yet be read differently by every consumer downstream; git has no representation of that difference, and treats the two as unrelated byte sequences. A schema migration that changes a field's type is visible to git only as a diff to a schema file (if that file is stored at all); the fact that all readers of the repository are now obliged to interpret the affected records differently does not enter git's accounting.

The third is the *lineage* that connects one interpretation to the next. A schema's history is not the history of its file; if the schema file moves, splits, or merges with another, git tracks the file moves but not the semantic lineage of what the file meant. A reader who wants to understand why a field was added in commit $A$, renamed in $B$, and removed in $C$ cannot trace that history through git's default tooling without reconstructing it from commit messages, which are unstructured text.

Git versions byte sequences well. Every category of thing a working repository needs beyond byte sequences (schemas, interpretations, the lineage between schema versions) is either not represented at all or represented incidentally as bytes-that-happen-to-be-a-schema-file. The gap is what [`panproto-vcs`](https://docs.rs/panproto-vcs/latest/panproto_vcs/) fills.

## What panproto-vcs adds

Panproto-vcs keeps git's Merkle DAG structure intact and adds three new object types to the database: *schemas* (models of a registered GAT, in the sense of [Protocols as theories, schemas as instances](../core/schemas-as-instances.md)), *migrations* (morphisms of models, in the sense of [Theory morphisms and instance migration](../core/morphisms-and-migration.md)), and *instances* (records under a given schema, in the sense of [the instance functor](../core/schemas-as-instances.md#the-instance-functor)). The object types are content-addressed by [blake3](https://github.com/BLAKE3-team/BLAKE3) hashes rather than SHA-1; they form a DAG alongside the commits; and they carry the schema-lineage information git does not.

Commits in panproto-vcs point at trees whose leaves can be any of the new object types in addition to the ordinary blobs. The result is a repository whose history captures both the bytes and the interpretations the bytes are values of. Merges in panproto-vcs run at the schema level, as pushouts in the category of schemas (see [Merge as pushout](./merge-as-pushout.md)). Diffs and blames apply to schemas as well as to bytes. The next chapter works through the object model in detail.

The next chapter, [Objects, refs, and the DAG](./objects-and-dag.md), specifies panproto-vcs's object types, its hashing scheme, its storage backends, and the reference structure around them. It is the chapter to read alongside [`panproto_vcs::object`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/), [`panproto_vcs::store`](https://docs.rs/panproto-vcs/latest/panproto_vcs/store/), and [`panproto_vcs::dag`](https://docs.rs/panproto-vcs/latest/panproto_vcs/dag/) for a working understanding of the implementation.
