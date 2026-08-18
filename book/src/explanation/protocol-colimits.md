# Composing protocols by colimit

## In plain terms

Schema languages have a lot in common with each other. They all have records, fields, references between records, ways to constrain what values a field can take, and ways to express that something is one of a fixed set of alternatives. Building each protocol's schema model from scratch would mean reimplementing those shared pieces dozens of times.

panproto solves this by defining each shared piece as a small theory, then combining them. The theory of *graphs* (vertices and edges) is one piece. The theory of *constraints* (predicates on edge values) is another. The theory of *multigraphs* (graphs that allow multiple parallel edges between the same pair of vertices) is a third. Each protocol is built by gluing these pieces together at the points where they share structure.

The gluing operation is called a *colimit*. Conceptually, it takes several theories plus a description of how their shared parts identify, and produces a single combined theory whose vertices and edges are the union of the inputs, with the shared parts collapsed. The result is a new theory that has all the structure of the inputs and respects all their equations.

Protocol registration uses these colimits to assemble reusable theory groups. A new protocol still needs an edge interpretation and its parser/emitter integration, so theory composition is one part of registration rather than the whole implementation.

[Schemas as theories](./schemas-as-theories.md) treats morphisms as maps that preserve named structure. Colimits use that preservation to combine protocols; their universal property is the first substantial abstraction jump on the advanced path.

## The construction

A protocol's schema theory is built as the colimit of a diagram of building-block theories. The diagram is a small category whose objects are theories and whose morphisms are theory inclusions describing the shared structure.

For instance, the constrained-multigraph/W-type group used by several structured-data protocols is registered in two parts. Its schema theory is

```text
ThGraph + ThConstraint + ThMulti
```

where the first pushout identifies the shared `Vertex` sort and the second identifies both `Vertex` and `Edge`. Its separate instance theory is `ThWType`. Together they provide:

- The vertices, edges, and source/target operations from `ThGraph`.
- The constraint sort and predicate operations from `ThConstraint`.
- The multi-edge labeling from `ThMulti`.
- The instance-level node, arc, anchor, and value structure from `ThWType`.

If a colimit step fails, registration panics with a message naming the failing intermediate step (`colimit ThGraph + ThConstraint over ThVertex failed: ...`). This is intentional: a failed registration is a build-time bug in the theory composition, not user input.

## Why colimits, specifically

The colimit construction has three properties that matter for panproto:

1. **Universality.** Any compatible pair of maps out of the input theories factors through the colimit. This property states the sense in which the result contains no unconstrained choice beyond the identifications requested by the span.
2. **Checked construction.** `pushout_by_name` builds each inclusion with `identity_inclusion`, which requires every shared sort and operation to be present in the target, and then checks that the two legs agree on what they share. Either failure returns a `GatError`, which protocol registration turns into a named panic because built-in registration failure is a programming error. Validating the inclusions as morphisms is a separate, opt-in check.
3. **Reusable composition.** The same pushout implementation builds protocol theory groups and supplies the GAT-level construction used by later merge machinery.

The merge operation in [schema version control](./vcs-semantics.md) is also a colimit (specifically a pushout); the same machinery powers both.

## Reusable building blocks

The shared theory library lives in [`crates/panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs). The major pieces:

| Theory | Purpose |
|---|---|
| `ThGraph` | Vertices and edges, with source and target operations. |
| `ThConstraint` | Vertex-attached constraints (a dependent `Constraint(v: Vertex)` sort). |
| `ThMulti` | Parallel edges distinguished by edge labels. |
| `ThWType` | Nodes, arcs, values, and their links back to schema vertices and edges. |
| `ThMeta` | Discriminators and extra fields attached to instance nodes. |

The library also exposes higher-level pieces built by composing these (`ThSimpleGraph`, `ThHypergraph`, `ThInterface`, `ThFunctor`, `ThFlat`, `ThGraphInstance`) for protocols that want to start from a richer base.

A protocol's registration function is a recipe for combining these. To define a new protocol, see [Build a custom protocol](../how-to/build-protocol.md).

## Related work

Cross-protocol translation has two mature precedents. Fagin and colleagues supply universal solutions, the chase, and the second-order extension needed for composition [@faginkolaitispopa2005data; @faginkolaitispopatan2005composing]. CQL treats schemas as algebraic theories or categories, schema morphisms as functorial mappings, and data integration as a pushout [@schultzwisnesky2017algebraic; @schultzspivakvasilakopoulouwisnesky2017algebraic]. [Apache Calcite](https://calcite.apache.org/) [@begolicamachorodriguezhydemiorlemire2018calcite], [Substrait](https://substrait.io/), [Apache Arrow](https://arrow.apache.org/), and [MLIR](https://mlir.llvm.org/) motivate the hub-and-spoke engineering pattern. panproto's `colimit` and `pushout_by_name` apply these ideas to GAT-presented wire-format schemas. See [Related work](./related-work.md#cross-protocol-translation-and-data-exchange) for the full discussion.

## See also

- [Schemas as theories](./schemas-as-theories.md) for what a single theory is.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the formal pushout construction and the universal property panproto verifies.
- [Schema version control semantics](./vcs-semantics.md) for the use of pushouts in merge.
