# Denotational semantics

This cluster gives formal and operational models for panproto's DSLs and structural constructions. Because a denotational model may be broader than the executable fragment, each page labels implemented behavior, checked properties, and explanatory interpretation separately.

Read [Shared notation](./shared-notation.md) first. The remaining pages assume comfort with typed abstract syntax and elementary category theory; [The vocabulary in plain terms](../decoder-ring.md) and [Schemas as theories](../schemas-as-theories.md) provide the intermediate bridge.

The pages in turn:

| Page | What it pins down |
|---|---|
| [Shared notation](./shared-notation.md) | The judgment forms, environments, and meta-notation used across the others. Read this first. |
| [Expression language](./expression-language.md) | `panproto-expr`: abstract syntax, best-effort type classification, and resource-bounded evaluation. |
| [Lens DSL](./lens-dsl.md) | `panproto-lens-dsl`: the lens triple `(get, put, complement)`, the three round-trip laws, complement composition as a partial commutative monoid. |
| [Theory DSL](./theory-dsl.md) | `panproto-theory-dsl`: GAT presentations, compilation, typechecking, and the boundary of the CwF interpretation. |
| [Pushouts and merge](./pushouts-and-merge.md) | The pushout construction in the category of GATs, the universal property, and what the implementation verifies. |
| [Protolens composition](./protolens-composition.md) | Protolenses as natural transformations between schema endofunctors, the structural-equality criterion for composition, sequential vs fused instantiation. |
| [REPL command language](./repl-commands.md) | The REPL (`schema theory repl`, part of `panproto-cli`): state model, command interpretation, and the bare-term typecheck path. |

The [What panproto verifies](../what-is-verified.md) chapter remains the guarantee catalog. These pages supply the definitions needed to interpret that catalog.
