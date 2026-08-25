# Composing protocols by colimit

Several protocol registrations use the same small theories. `ThGraph` supplies `Vertex`, `Edge`, `src`, and `tgt`. `ThConstraint` supplies a dependent constraint sort and its target vertex. `ThMulti` adds edge labels to distinguish parallel edges. Separate instance theories, including `ThWType`, describe nodes, arcs, and values.

Rather than duplicate these declarations, panproto combines theory presentations over explicitly shared sorts and operations. The relevant construction is a **colimit**. Using colimits to assemble specifications from component theories follows the Clear structured-specification work [@burstallgoguen1977putting; @burstallgoguen1980semantics]. Given a diagram of theories and structure-preserving maps, its colimit identifies the specified common structure and retains the remaining declarations and equations.

## The registered construction

The constrained-multigraph group illustrates the process. Its first pushout combines `ThGraph` with `ThConstraint` by identifying `Vertex`. A second pushout combines that result with `ThMulti` by identifying both `Vertex` and `Edge`. `ThWType` is registered separately as the group's instance theory. Schema structure and instance structure thus remain distinct even when a protocol registration names both.

The helper `pushout_by_name` constructs identity-by-name inclusions for the shared sorts and operations. It first requires every requested name to exist on both sides, then invokes the GAT colimit construction. Construction checks that the resulting cocone commutes. The returned `ColimitResult` also exposes `verify_universal` for checking factorization against a supplied alternative cocone; that stronger check is not implicit in every call to `pushout_by_name`.

The shared registration helpers treat a failed built-in composition as a programming error and panic with a message naming the failed step. Other protocol-specific registration paths handle composition errors locally.

## Universal characterization

Suppose compatible maps send each input theory into another theory $X$. The universal property states that these maps factor uniquely through the colimit. This property characterizes which identifications the construction introduces: the mediating map is determined by the compatible input maps, rather than by an additional choice made during composition.

Runtime construction and universal characterization have different scopes. `pushout_by_name` validates names and constructs a commuting cocone. `verify_universal` evaluates the additional mediator condition for an alternative cocone supplied to it. [Pushouts and merge](./semantics/pushouts-and-merge.md) states the formal condition and the scope of the implementation checks.

## Reusable theories

The shared library is defined in [`crates/panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs).

| Theory | Declared structure |
|---|---|
| `ThGraph` | Vertices and directed edges with source and target. |
| `ThConstraint` | Vertex-indexed constraints and their targets. |
| `ThMulti` | Edge labels for parallel edges. |
| `ThWType` | Instance nodes, arcs, and values linked to schema structure, with two equations relating arc endpoints to schema-edge endpoints. |
| `ThMeta` | Discriminators, extra fields, and values attached to nodes. |

`ThGraph`, `ThConstraint`, `ThMulti`, and `ThMeta` declare no equations. `ThWType` declares `arc_src_anchor` and `arc_tgt_anchor`. The library also defines composed or higher-level theories including `ThSimpleGraph`, `ThHypergraph`, `ThInterface`, `ThFunctor`, `ThFlat`, and `ThGraphInstance`. A protocol registration selects the theory group appropriate to its structures and separately supplies parsing, emission, and protocol-specific rules. [Build a custom protocol](../how-to/build-protocol.md) describes that registration process.

## Related work

Data exchange supplies universal solutions and chase-based composition [@faginkolaitispopa2005data; @faginkolaitispopatan2005composing]. CQL treats schemas as algebraic theories or categories and uses pushouts for data integration [@schultzwisnesky2017algebraic; @schultzspivakvasilakopoulouwisnesky2017algebraic]. [Apache Calcite](https://calcite.apache.org/) [@begolicamachorodriguezhydemiorlemire2018calcite], [Substrait](https://substrait.io/), [Apache Arrow](https://arrow.apache.org/), and [MLIR](https://mlir.llvm.org/) provide related engineering precedents for intermediate representations. panproto applies GAT colimits to reusable descriptions of wire-format schemas. [Related work](./related-work.md#cross-protocol-translation-and-data-exchange) develops these comparisons.

## See also

- [Schemas as theories](./schemas-as-theories.md) for theory presentations and their models.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the universal property and its checks.
- [Schema version control semantics](./vcs-semantics.md) for structural merge of concrete schemas.
