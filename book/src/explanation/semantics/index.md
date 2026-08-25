# Language and construction semantics

These chapters specify the behavior of panproto's expression language, DSL compilers, lens composition, pushout construction, and theory REPL. Formal notation is used where it makes an implemented rule more precise. A displayed equation is not, by itself, a claim that the implementation has been formally verified.

[Shared notation](./shared-notation.md) introduces the symbols used in the other chapters. [The vocabulary in plain terms](../decoder-ring.md) and [Schemas as theories](../schemas-as-theories.md) introduce the mathematical vocabulary.

| Page | What it pins down |
|---|---|
| [Shared notation](./shared-notation.md) | Judgment forms, environments, semantic functions, errors, and equality. |
| [Expression language](./expression-language.md) | `panproto-expr`: abstract syntax, best-effort type classification, and resource-bounded evaluation. |
| [Lens DSL](./lens-dsl.md) | `panproto-lens-dsl`: `get` and `put` with an explicit returned complement, the round-trip checks, and complement composition as a checked partial operation. |
| [Theory DSL](./theory-dsl.md) | `panproto-theory-dsl`: GAT presentations, compilation, typechecking, and the boundary of the CwF interpretation. |
| [Pushouts and merge](./pushouts-and-merge.md) | The GAT colimit construction, its on-demand universal-property check, and the narrower checks run by schema merge. |
| [Protolens composition](./protolens-composition.md) | Protolenses as natural transformations between schema endofunctors, the structural-equality criterion for composition, sequential vs fused instantiation. |
| [REPL command language](./repl-commands.md) | The REPL (`schema theory repl`, part of `panproto-cli`): state model, command interpretation, and the bare-term typecheck path. |

[What panproto verifies](../what-is-verified.md) records where each runtime check or test is applied and distinguishes exhaustive checks from sampled tests.
