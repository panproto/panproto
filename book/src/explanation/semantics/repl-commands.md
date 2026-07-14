# REPL command language: denotational semantics

## In plain terms

The REPL is the interactive surface for inspecting theories and terms: load a theory document, switch the active theory, ask for the type of a term, normalize a term under the directed equations, enumerate the free model. Every interactive command is a small operation on a stateful environment that holds the loaded theories, the loaded morphisms, and a pointer to the currently active theory.

This page pins down what each command does, what state it touches, and what the bare-term-typecheck path means.

## Surface syntax

```bnf
line     ::= command | term
command  ::= ":" cmd args
cmd      ::= "load" | "theories" | "use" | "sorts" | "ops"
           | "type" | "normalize" | "model" | "instance"
           | "quit" | "q"
args     ::= /* command-specific, see table below */
```

A line that does not begin with `:` is treated as a term and routed through the typecheck path under the active theory. Comments and blank lines are ignored. The grammar lives in `crates/panproto-cli/src/repl/engine.rs`.

## Abstract syntax

```rust,ignore
pub enum ReplCommand {
    Load(PathBuf),
    Theories,
    Use(String),
    Sorts,
    Ops,
    TypeOf(String),
    Normalize(String),
    Model(Option<u32>),
    Instance { class: String, target: String, bindings: Vec<(String, String)> },
    Quit,
}

pub enum ReplLine {
    Command(ReplCommand),
    Term(String),
}
```

(The actual implementation does not export this enum; commands are dispatched directly from string parsing in `Repl::handle_command`. The shape above is the denotational model.)

## Semantic domain

The REPL state is

$$
\Sigma \;=\; \bigl(\mathsf{theories} : \mathsf{Name} \rightharpoonup \mathsf{Theory},\;
                  \mathsf{morphisms} : \mathsf{Name} \rightharpoonup \mathsf{TheoryMorphism},\;
                  \mathsf{active} : \mathsf{Name}_\bot \bigr)
$$

The semantic codomain is the set $\mathsf{Outcome}$ of user-visible effects (printed string, typed-term result, error, quit signal). The denotational semantics is the pair of functions

$$
\llbracket \cdot \rrbracket_C : \mathsf{ReplCommand} \to \Sigma \to \Sigma \times \mathsf{Outcome}
\qquad
\llbracket \cdot \rrbracket_T : \mathsf{Term} \to \Sigma \to \Sigma \times \mathsf{Outcome}
$$

where $\llbracket \cdot \rrbracket_T$ is state-preserving: $\pi_1 \circ \llbracket t \rrbracket_T = \mathsf{id}_\Sigma$.

## Semantic equations

Write $\sigma = (\theta, \mu, a)$ for a state and $\theta[n \mapsto T]$ for the obvious update. The equations:

$$
\begin{aligned}
\llbracket \mathsf{Load}(p) \rrbracket_C\, \sigma
  &= \bigl(\sigma'',\ \mathsf{Loaded}(\mathsf{names}(\Delta))\bigr) \\
  &\quad \text{where } \Delta = \mathsf{compile}(p),\ \sigma'' = \sigma\,\text{with}\,\theta, \mu \mathbin{\cup} \Delta \\[2pt]
\llbracket \mathsf{Theories} \rrbracket_C\, \sigma
  &= \bigl(\sigma,\ \mathsf{List}(\mathsf{dom}(\theta))\bigr) \\[2pt]
\llbracket \mathsf{Use}(n) \rrbracket_C\, \sigma
  &= \begin{cases}
       (\sigma[a := n],\ \mathsf{Ok}) & n \in \mathsf{dom}(\theta) \\
       (\sigma,\ \mathsf{UnknownTheory}(n)) & \text{otherwise}
     \end{cases} \\[2pt]
\llbracket \mathsf{Sorts} \rrbracket_C\, \sigma
  &= \bigl(\sigma,\ \mathsf{List}(\mathsf{sorts}(\theta(a)))\bigr) \\[2pt]
\llbracket \mathsf{Ops} \rrbracket_C\, \sigma
  &= \bigl(\sigma,\ \mathsf{List}(\mathsf{ops}(\theta(a)))\bigr) \\[2pt]
\llbracket \mathsf{TypeOf}(t) \rrbracket_C\, \sigma
  &= \bigl(\sigma,\ \mathsf{Typed}(\mathsf{typecheck\_term}(t,\ \theta(a)))\bigr) \\[2pt]
\llbracket \mathsf{Normalize}(t) \rrbracket_C\, \sigma
  &= \bigl(\sigma,\ \mathsf{Normal}(\mathsf{normalize}(t,\ \theta(a)))\bigr) \\[2pt]
\llbracket \mathsf{Model}(d) \rrbracket_C\, \sigma
  &= \bigl(\sigma,\ \mathsf{Fibers}(\mathsf{free\_model}(\theta(a),\ \mathsf{depth} = d))\bigr) \\[2pt]
\llbracket \mathsf{Instance}(C, T, B) \rrbracket_C\, \sigma
  &= \bigl(\sigma\,\text{with}\,\mu[m \mapsto M],\ \mathsf{Compiled}(m)\bigr) \\
  &\quad \text{where } M = \mathsf{compile\_instance}(C, T, B),\ m = \mathsf{name}(M) \\[2pt]
\llbracket \mathsf{Quit} \rrbracket_C\, \sigma
  &= (\sigma,\ \mathsf{QuitSignal})
\end{aligned}
$$

The bare-term path:

$$
\llbracket t \rrbracket_T\, \sigma \;=\; \bigl(\sigma,\ \mathsf{Typed}(\mathsf{typecheck\_term}(t,\ \theta(a)))\bigr)
$$

When $a = \bot$ (no active theory), all $\theta(a)$-dependent equations short-circuit to $(\sigma, \mathsf{NoActiveTheory})$. When any auxiliary (`compile`, `typecheck_term`, `normalize`, `free_model`, `compile_instance`) returns an error, the outcome is $\mathsf{Error}(e)$ and the state is unchanged.

`Repl::handle_command` and `Repl::handle_term_typecheck` in [`crates/panproto-cli/src/repl/engine.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-cli/src/repl/engine.rs) implement these equations pointwise.

## Soundness

The REPL is a thin orchestration layer over the GAT engine and the theory DSL compiler. It introduces no new failure modes; every error it reports comes from one of:

- `panproto_theory_dsl::LoadError` (during `:load` or `:instance`)
- `panproto_gat::GatError` (during `:type`, `:normalize`, `:model`)
- A REPL-level `UnknownCommand` / `UnknownTheory` for command-shape errors

State updates are atomic per line: a failed compile in `:load` rolls back the partial insertions before returning the error, so the post-state is either fully updated or unchanged.

The bare-term path is total in the technical sense: every input string either parses and produces a `TypeOf` or `Error` outcome; the REPL does not deadlock or spin.

## What is intentionally not modelled

- **Multi-line input.** Commands and terms are single-line. Continuations are the user's responsibility (concatenate into a single line before submission).
- **Macro expansion.** There is no `:define` or `:macro`; the REPL is a pure inspection interface, not a programming environment.
- **Concurrent state.** A `Repl` instance is single-threaded. The shape above does not model concurrent line submission.
- **Persistent history.** The REPL relies on `rustyline` for history; the persistence model is `rustyline`'s, not panproto's.

## See also

- [Theory DSL: denotational semantics](./theory-dsl.md) for what `:load` consumes.
- [Reference: CLI](../../reference/cli.md) for the `schema theory repl` invocation that wraps this.
- [Crate map](../../reference/crate-map.md) for `panproto-cli`, which hosts the REPL.
