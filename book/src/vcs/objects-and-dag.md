# Objects, refs, and the DAG

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

The object database panproto-vcs sits on top of is a Merkle DAG of content-addressed objects, referenced by refs, in the same overall shape as git's. What differs is the set of object types stored. Where git has three (blob, tree, commit), panproto-vcs has more, of which several are the new object kinds that carry the schemas, migrations, instances, and protocol definitions the engine uses to track interpretation alongside bytes. The present chapter specifies the principal object types, the hashing, the two storage backends, and the ref structure.

The code lives in [`panproto-vcs`](https://docs.rs/panproto-vcs/latest/panproto_vcs/).

## Object types

The principal object types follow.

A **blob** is a byte sequence. It is the same notion as git's blob, used for file contents that have no interpretation panproto's engine knows about. A blob is serialised as its raw bytes with no header.

A **tree** is a list of (name, permission, object-id, object-kind) tuples. Each entry in a tree points at another object in the database, with the object-kind tag saying which type the target is. Trees are git's trees generalised to carry the extra object kinds below.

A **commit** is a record with parent references, a root tree, author and committer metadata, a message, and a set of schema commits the working tree depends on. The parent references structure the Merkle DAG of commits; the schema-commits field is panproto-specific and names the schemas this commit is written against.

A **schema** object is a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) value, serialised through [serde](https://serde.rs/). Two schemas that are equal as models of the same protocol's theory hash to the same blake3 output, which makes schema deduplication work the same way blob deduplication does in git.

Schemas that derive from a working tree of source files have additional internal structure. A [`FileSchema`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/) leaf holds the schema produced by parsing a single file under a single protocol; a [`SchemaTree`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/) (in `SingleLeaf` or `Directory` form) is a Merkle tree over those leaves whose shape mirrors the working tree itself; a [`FlatSchema`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/) is the union schema obtained by walking a `SchemaTree` and merging its leaves. The walking and reconstruction live in [`panproto_vcs::tree::resolve_commit_schema`](https://docs.rs/panproto-vcs/latest/panproto_vcs/tree/) and [`panproto_vcs::tree::store_schema_as_tree`](https://docs.rs/panproto-vcs/latest/panproto_vcs/tree/). The point of the per-file decomposition is the same as git's per-file blob decomposition: a commit that touches one file in a thousand-file working tree shares the other 999 leaves with the previous commit's `SchemaTree`. A blob-OID cache keyed by `(git2::Oid, protocol)` makes the unchanged-file case a hash lookup rather than a re-parse.

A **migration** object is a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value, also serialised through serde. Every migration references the source and target schemas it operates between, and its hash includes those references; a migration between two identical schemas is the same object.

An **instance** object is a [`WInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/) or [`FInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/), again through serde. An instance's hash includes a reference to its schema, so the pair (schema, instance) is uniquely determined.

A **protocol** object is a registered [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) together with its parser and emitter identifiers. Protocol objects are usually stored once and referenced many times; their hashes identify the exact protocol version.

All object kinds are defined in [`panproto_vcs::object`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/), each as a variant of the [`Object`](https://docs.rs/panproto-vcs/latest/panproto_vcs/object/enum.Object.html) enum, which is `#[non_exhaustive]` so further kinds can be added without breaking downstream callers.

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

## Further reading

@chaconstraub2014pro chapter 10 covers git's object database in the detail this chapter's framing parallels. @merkle1987digital is the original paper on the Merkle-tree construction the content-addressing here specialises. For the BLAKE3 hash function specifically, the [BLAKE3 reference](https://github.com/BLAKE3-team/BLAKE3) at the BLAKE3 GitHub organisation is authoritative; BLAKE3 replaces SHA-1 and SHA-256 for panproto's object hashes on performance and cryptographic-property grounds that the reference documents.

## Closing

The next chapter, [Merge as pushout](./merge-as-pushout.md), takes the three-way-merge algorithm apart and shows that it is a pushout in the category of schemas ([Colimits and pushouts](../foundations/colimits.md) is the mathematical reference). The chapter after that, [Data versioning](./data-versioning.md), works through how panproto-vcs automatically infers migrations from schema diffs and lifts instance data across version boundaries.
