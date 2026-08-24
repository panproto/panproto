# REPL command language

The theory REPL loads compiled theories and morphisms, selects an active theory, and applies GAT operations to terms. Its state consists of two finite maps and an optional active-theory name. Commands and bare terms are processed one line at a time.

## Input syntax

```bnf
line     ::= command | term
command  ::= ":" cmd args
cmd      ::= "load" | "theories" | "use" | "sorts" | "ops"
           | "type" | "normalize" | "model" | "instance"
           | "quit" | "q"
```

A blank line produces no output. A nonblank line without a leading colon is parsed and typechecked as a term in the active theory. There is no REPL comment syntax: a nonblank comment-looking line is handled as a term and will ordinarily produce a parse error. Multiline commands and terms are not implemented.

The command arguments are parsed by the command handler rather than a shared grammar. In particular, `:instance` expects `<class> in <target> { source = target; ... }`, and `:model` accepts at most one decimal depth.

## State

Let $\Sigma$ denote the REPL state. It contains a theory map $\theta$, a morphism map $\mu$, and an optional active name $a$:

$$
\Sigma=(\theta:\mathsf{Name}\rightharpoonup\mathsf{Theory},
        \mu:\mathsf{Name}\rightharpoonup\mathsf{TheoryMorphism},
        a:\mathsf{Name}_\bot).
$$

The hooked arrow denotes a finite partial map, and $\bot$ denotes the absence of an active theory. `:load`, `:use`, and a successful `:instance` may change this state. Inspection, term typechecking, normalization, and model enumeration do not.

## Command behavior

`:load p` calls `load_and_compile` on path $p$. Compilation completes before either map is changed. On success, all compiled theories and morphisms are inserted; if there was no active theory and at least one theory was loaded, one loaded theory becomes active. Protocols and composition specifications in the compiled set are not retained by the REPL state. On compilation failure, the state is unchanged.

`:theories` lists loaded theory names in sorted order and marks the active one. `:use n` sets $a$ to $n$ only when $n$ belongs to the domain of $\theta$. `:sorts` and `:ops` render declarations from $\theta(a)$.

`:type t` parses term source $t$ and calls `typecheck_term` with an empty variable context. A bare term follows exactly the same path. Thus free variables are not introduced through REPL state.

`:normalize t` parses $t$ and calls `normalize` with the active theory's directed equations and a rewrite budget of 1,000 steps. It does not typecheck the term first, and the normalizer returns the term reached when its budget is exhausted rather than a distinct REPL error saying that normalization was incomplete.

`:model d` calls `free_model` with maximum depth $d$. The default depth is 3, and the REPL rejects values above 10 before calling the model builder. Each carrier display is truncated to five rendered elements. If `free_model` reports an incomplete model, the output includes a warning.

`:instance C in T { B }` compiles an instance morphism from class theory $C$ to target theory $T$ using the loaded theory map as its resolver. A successful result is inserted in $\mu$ under the generated name `C_to_T`. The binding parser splits entries at semicolons and the first equals sign; it does not implement quoting or nested syntax.

`:quit` and `:q` return the quit signal. Unknown commands and malformed arguments return `ReplOutcome::Error` strings.

## Failure boundaries

Commands that require $\theta(a)$ fail when no theory is active or when the active name is missing from the map. Parsing, typechecking, normalization, free-model construction, and instance compilation render their failures as `ReplOutcome` messages rather than exposing the underlying error enums to the caller.

The fixed normalization and model-depth bounds limit those particular operations. The implementation states no totality theorem for arbitrary input strings, and the REPL layer has no proof object for a successful typecheck or normalization.

[`Repl::handle_line`](https://github.com/panproto/panproto/blob/main/crates/panproto-cli/src/repl/engine.rs) implements this behavior. The surrounding `rustyline` driver supplies editing, command completion, and persistent history; those facilities are not part of the `Repl` state above.

## See also

- [Theory DSL](./theory-dsl.md)
- [CLI reference](../../reference/cli.md)
- [Crate map](../../reference/crate-map.md)
