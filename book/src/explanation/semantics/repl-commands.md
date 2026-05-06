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

A line that does not begin with `:` is treated as a term and routed through the typecheck path under the active theory. Comments and blank lines are ignored. The grammar lives in `crates/panproto-repl/src/lib.rs`.

## Abstract syntax

```rust
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
\Sigma = (\mathsf{theories} : \mathsf{Map}(\mathsf{Name}, \mathsf{Theory}),\;
         \mathsf{morphisms} : \mathsf{Map}(\mathsf{Name}, \mathsf{TheoryMorphism}),\;
         \mathsf{active} : \mathsf{Option}(\mathsf{Name}))
$$

Each command is a function $\mathsf{ReplCommand} \to \Sigma \to (\Sigma', \mathsf{ReplOutcome})$ and each term-line is a function $\mathsf{Term} \to \Sigma \to (\Sigma, \mathsf{ReplOutcome})$ (terms do not modify state).

The `ReplOutcome` carries the user-visible effect: a printed string, a typed-term result, an error, or the quit signal.

## Interpretation

The semantic function $\llbracket \cdot \rrbracket : \mathsf{ReplCommand} \to \Sigma \to (\Sigma, \mathsf{ReplOutcome})$:

| Command | $\Sigma$ change | Outcome |
|---|---|---|
| `:load p` | Compile the theory document at $p$ via `panproto_theory_dsl::load_and_compile`; insert each compiled theory into `theories` and each morphism into `morphisms`. | Names of newly loaded items, or compile error. |
| `:theories` | none | Print the keys of `theories`. |
| `:use n` | Set `active = Some(n)`. | `Ok` if $n \in \mathsf{theories}$; `UnknownTheory` otherwise. |
| `:sorts` | none | Print the sort list of the active theory. |
| `:ops` | none | Print the op signatures of the active theory. |
| `:type t` | none | Parse $t$, run `panproto_gat::typecheck_term` against the active theory; print the inferred sort or the type error. |
| `:normalize t` | none | Parse $t$, run `panproto_gat::normalize` against the active theory's directed equations; print the normal form. |
| `:model d?` | none | Run `panproto_gat::free_model` on the active theory with `FreeModelConfig { depth: d.unwrap_or(default), ... }`; print one section per fiber. |
| `:instance class in target { bindings }` | Compile an ad-hoc instance morphism via `panproto_theory_dsl::compile_instance`; insert into `morphisms`. | The compiled morphism's name, or compile error. |
| `:quit` / `:q` | none | Quit signal. |

The bare-term path is

$$
\llbracket t \rrbracket(\Sigma) =
\begin{cases}
  (\Sigma, \mathsf{TypeOf}(\tau)) & \text{if } \mathsf{active} = \mathsf{Some}(n) \text{ and } \mathsf{typecheck\_term}(t, \mathsf{theories}(n)) = \mathsf{Ok}(\tau) \\
  (\Sigma, \mathsf{Error}(e)) & \text{otherwise}
\end{cases}
$$

`handle_term_typecheck` in `crates/panproto-repl/src/lib.rs` implements this pointwise.

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
- [Reference: CLI](../../reference/cli.md) for the `schema repl` invocation that wraps this.
- [Crate map](../../reference/crate-map.md) for `panproto-repl`.
