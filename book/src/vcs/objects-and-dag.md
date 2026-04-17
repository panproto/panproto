# Objects, refs, and the DAG

Panproto-vcs's object database mirrors [git](https://git-scm.com/)'s in its overall shape (a Merkle DAG of content-addressed objects referred to by refs) and differs in the specific object types it stores. This chapter specifies the object types, the hashing, the two storage backends panproto-vcs ships with, and the ref structure.

The code is in [`panproto-vcs`](https://docs.rs/panproto-vcs/latest/panproto_vcs/). Throughout the chapter, references to specific modules link to that crate's documentation.

## Object types

The object types are seven.

A **blob** is a byte sequence. It is the same notion as git's blob, used for file contents that have no interpretation panproto's engine knows about. A blob is serialised as its raw bytes with no header.

A **tree** is a list of (name, permission, object-id, object-kind) tuples. Each entry in a tree points at another object in the database, with the object-kind tag saying which of the seven types the target is. Trees are git's trees generalised to carry the extra object kinds below.

A **commit** is a record with parent references, a root tree, author and committer metadata, a message, and a set of schema commits the working tree depends on. The parent references structure the Merkle DAG of commits; the schema-commits field is panproto-specific and names the schemas this commit is written against.

A **schema** object is a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) value, serialised through [serde](https://serde.rs/). Two schemas that are equal as models of the same protocol's theory hash to the same blake3 output, which makes schema deduplication work the same way blob deduplication does in git.

A **migration** object is a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value, also serialised through serde. Every migration references the source and target schemas it operates between, and its hash includes those references; a migration between two identical schemas is the same object.

An **instance** object is a [`WInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/) or [`FInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/), again through serde. An instance's hash includes a reference to its schema, so the pair (schema, instance) is uniquely determined.

A **protocol** object is a registered [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) together with its parser and emitter identifiers. Protocol objects are usually stored once and referenced many times; their hashes identify the exact protocol version.

The seven types are defined in [`panproto_vcs::object`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/), each as a variant of the [`Object`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/enum.Object.html) enum.

## Hashing

Every object is identified by the [blake3](https://github.com/BLAKE3-team/BLAKE3) hash of its canonical serialisation. Blake3 replaces git's historical SHA-1 (and the partial SHA-256 migration git is in the process of) with a hash that is both faster and cryptographically stronger. The choice is not ideological: blake3's streaming API makes incremental hashing during object construction considerably simpler than SHA-family APIs, and its speed pays back in repositories that contain many large instance objects.

The canonical serialisation of each object kind is defined in [`panproto_vcs::hash`](https://docs.rs/panproto-vcs/latest/panproto_vcs/hash/). Two objects with the same semantic content have the same canonical serialisation, and therefore the same hash. This extends to panproto-specific deduplication: two `Schema` values that are model-equivalent under their theory produce the same hash, which a raw Rust `==` check would not guarantee.

## Storage backends

Two storage implementations are shipped. The filesystem backend, [`panproto_vcs::fs_store`](https://docs.rs/panproto-vcs/latest/panproto_vcs/fs_store/), writes each object to a file at a path derived from its hash, under a `.panproto` directory at the repository root. The layout mirrors git's (`xx/xxxxxx...`) for cache-locality; objects are stored without compression by default, with a zstd-compressed variant available through a feature flag.

The in-memory backend, [`panproto_vcs::mem_store`](https://docs.rs/panproto-vcs/latest/panproto_vcs/mem_store/), keeps objects in a `HashMap` keyed by hash. It is used by the WASM build of panproto, by tests that do not need persistence, and by callers who want to assemble a repository state transiently before committing it to the filesystem. Every operation the filesystem backend supports is also supported in memory, so code written against the [`Store`](https://docs.rs/panproto-vcs/latest/panproto_vcs/store/trait.Store.html) trait works against both.

Both backends are append-only at the object level. An object once written cannot be modified, only deleted by a garbage collection pass ([`panproto_vcs::gc`](https://docs.rs/panproto-vcs/latest/panproto_vcs/gc/)) that traces from the live refs and removes any object no ref reaches.

## Refs

A **ref** is a named pointer to a commit. The ref namespace is organised into branches (`refs/heads/<name>`), tags (`refs/tags/<name>`), and an implementation-internal set for panproto-specific state (`refs/panproto/<name>`). Every ref is a mutable mapping: its value can be updated to point at a different commit, though the commits themselves remain immutable. The ref store is implemented in [`panproto_vcs::refs`](https://docs.rs/panproto-vcs/latest/panproto_vcs/refs/).

Panproto-vcs keeps a separate ref family for schema history: `refs/panproto/schemas/<protocol>/<schema-name>`. Each such ref points at a commit whose root-tree leaves include the latest blessed schema for the given name under the given protocol. This is the mechanism [`schema diff`](https://docs.rs/panproto-vcs/latest/panproto_vcs/) uses to show the evolution of a specific schema across a repository's history, independently of the file-tree changes that carried the schema through its lifetime.

## The DAG

The DAG of commits (connected by parent edges) mirrors git's structure exactly. Operations on the DAG (topological walks, common-ancestor computation, reachability) live in [`panproto_vcs::dag`](https://docs.rs/panproto-vcs/latest/panproto_vcs/dag/) and are implemented with the same algorithms git uses. A reader familiar with git's `git-log --graph` or `git-merge-base` has no new graph-level ideas to learn here.

What differs is the DAG of *schemas*. Every commit references the schemas its working tree depends on, and every schema object references its protocol (and, for schemas produced by migration, the source schema the migration was applied from). The schema-DAG is therefore a parallel structure to the commit-DAG, with its own topological operations. A three-way merge, the subject of [the next chapter](./merge-as-pushout.md), operates on both DAGs simultaneously, with the commit-level merge choosing the common-ancestor commit and the schema-level merge computing the pushout in the category of schemas.

## Closing

The next chapter, [Merge as pushout](./merge-as-pushout.md), takes the three-way-merge algorithm apart and shows that it is a pushout in the category of schemas ([Colimits and pushouts](../foundations/colimits.md) is the mathematical reference). The chapter after that, [Data versioning](./data-versioning.md), works through how panproto-vcs automatically infers migrations from schema diffs and lifts instance data across version boundaries.
