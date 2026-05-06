# Composing protocols by colimit

## In plain terms

Every schema language has a lot in common with every other. They all have records, fields, references between records, ways to constrain what values a field can take, and ways to express that something is one of a fixed set of alternatives. Building each protocol's schema model from scratch would mean reimplementing those shared pieces dozens of times.

panproto solves this by defining each shared piece as a small theory, then combining them. The theory of *graphs* (vertices and edges) is one piece. The theory of *constraints* (predicates on edge values) is another. The theory of *multigraphs* (graphs that allow multiple parallel edges between the same pair of vertices) is a third. Each protocol is built by gluing these pieces together at the points where they share structure.

The gluing operation is called a *colimit*. Conceptually, it takes several theories plus a description of how their shared parts identify, and produces a single combined theory whose vertices and edges are the union of the inputs, with the shared parts collapsed. The result is a new theory that has all the structure of the inputs and respects all their equations.

This is why adding a new protocol to panproto is mostly a matter of declaring which building-block theories it uses and how they fit together: the colimit construction does the rest.

## The construction

A protocol's schema theory is built as the colimit of a diagram of building-block theories. The diagram is a small category whose objects are theories and whose morphisms are theory inclusions describing the shared structure.

For example, the typed-multigraph-with-W-types theory used by JSON Schema is constructed as:

```text
ThGraph + ThConstraint + ThMulti + ThWType
```

where each `+` is a pushout (a binary colimit) along the shared sort `Vertex`. The result has:

- The vertices, edges, and identity laws from `ThGraph`.
- The constraint sort and predicate operations from `ThConstraint`.
- The multi-edge labelling from `ThMulti`.
- The W-type operations (recursive type constructors) from `ThWType`.

If a colimit step fails, registration panics with a message naming the failing intermediate step ("could not push out `ThGraph + ThConstraint` along `Vertex`: ..."). This is intentional: a failed registration is a build-time bug in the theory composition, not user input.

## Why colimits, specifically

The colimit construction has three properties that matter for panproto:

1. **Universality.** The colimit is the *minimal* theory containing all the inputs and respecting their shared structure. No spurious extra equations are introduced.
2. **Existence checking.** The construction is mechanical and can fail predictably. If two building blocks have incompatible equations on a shared sort, the colimit step fails at registration time.
3. **Functoriality of migration.** Because protocols are colimits, a migration between two protocols can be defined componentwise on the building blocks; the colimit assembles the components into a single protocol-level migration.

The merge operation in [schema version control](./vcs-semantics.md) is also a colimit (specifically a pushout); the same machinery powers both.

## Reusable building blocks

The shared theory library lives in [`crates/panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs). The major pieces:

| Theory | Purpose |
|---|---|
| `ThGraph` | Vertices and edges with identity. |
| `ThMulti` | Multiple parallel edges between the same vertex pair. |
| `ThConstraint` | Predicates carried on edges. |
| `ThWType` | Recursive type constructors (W-types). |
| `ThVariant` | Sum-typed alternatives between vertices. |
| `ThNamed` | String labels on vertices and edges. |
| `ThOrder` | Total ordering on items in a collection edge. |

A protocol's registration function is a recipe for combining these. To define a new protocol, see [Build a custom protocol](../how-to/build-protocol.md).

## See also

- [Schemas as theories](./schemas-as-theories.md) for what a single theory is.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the formal pushout construction and the universal property panproto verifies.
- [Schema version control semantics](./vcs-semantics.md) for the use of pushouts in merge.
