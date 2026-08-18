# REPL command language: denotational semantics

## In plain terms

The REPL is the interactive surface for inspecting theories and terms. It can load a theory document, switch the active theory, infer the sort of a term, normalize under directed equations, and enumerate a bounded free model. A `Repl` value holds loaded theories, loaded morphisms, and an optional active-theory name.

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

The command model has the schematic shape below. The implementation dispatches directly from strings and does not export these enums, so the listing is not runnable Rust.

```text
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

Write $\sigma = (\theta, \mu, a)$ for a state and $\theta[n \mapsto T]$ for the update that binds $n$ to $T$. With this notation, the command equations are:

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

When $a = \bot$ (no active theory), all $\theta(a)$-dependent equations short-circuit to $(\sigma, \mathsf{NoActiveTheory})$. Typechecking, normalization, free-model enumeration, and instance compilation preserve state on failure. Loading needs a qualification: `cmd_load` inserts compiled theories and morphisms in loops, so a later insertion failure does not roll back earlier successful insertions from the same document.

`Repl::handle_command` and `Repl::handle_term_typecheck` in [`crates/panproto-cli/src/repl/engine.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-cli/src/repl/engine.rs) implement these equations pointwise.

## Soundness

The REPL is an orchestration layer over the GAT engine and theory DSL compiler, but it renders most failures into `ReplOutcome` messages rather than exposing a typed sum of `LoadError` and `GatError` variants. Command parsing also contributes REPL-level messages for unknown commands, unknown theories, missing arguments, and malformed depth values.

Normalization uses a fixed rewrite budget of 1,000 steps. Bare terms either produce a rendered type or a parse/type error. These paths are bounded by their underlying parsers and normalizer, but the implementation does not state a separate totality theorem for arbitrary input strings.

## What is intentionally not modeled

- **Multi-line input.** Commands and terms are single-line. Continuations are the user's responsibility (concatenate into a single line before submission).
- **Macro expansion.** There is no `:define` or `:macro`; the REPL is a pure inspection interface, not a programming environment.
- **Concurrent state.** A `Repl` instance is single-threaded. The shape above does not model concurrent line submission.
- **Persistent history.** The REPL relies on `rustyline` for history; the persistence model is `rustyline`'s, not panproto's.

## See also

- [Theory DSL: denotational semantics](./theory-dsl.md) for what `:load` consumes.
- [Reference: CLI](../../reference/cli.md) for the `schema theory repl` invocation that wraps this.
- [Crate map](../../reference/crate-map.md) for `panproto-cli`, which hosts the REPL.
