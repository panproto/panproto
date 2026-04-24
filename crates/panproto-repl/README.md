# panproto-repl

[![crates.io](https://img.shields.io/crates/v/panproto-repl.svg)](https://crates.io/crates/panproto-repl)
[![docs.rs](https://docs.rs/panproto-repl/badge.svg)](https://docs.rs/panproto-repl)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Interactive REPL for loading theories, typechecking terms, and exercising the panproto engine at the command line.

## What it does

Developing a theory against `panproto-gat` is usually a recompile-and-rerun loop: edit a Nickel or JSON spec, run the integration test, read the error, edit again. This crate collapses that loop into a line editor. It loads theories through `panproto-theory-dsl`, keeps them in a session, and evaluates single-line commands against the currently active theory: parse and typecheck a term, normalize it through the theory's directed equations, browse the free-model fibers, or compile an ad-hoc instance morphism on the spot.

The binary (`panproto-repl`) wires `rustyline` to the `Repl::handle_line` entry point and prints results to stdout. The library crate is deliberately thin: all state lives in the `Repl` struct, and `handle_line` takes a single string and returns a `ReplOutcome`. That shape is what the integration tests use to script sessions without a terminal, and what any embedding (an editor plugin, a notebook front-end, a web playground) can use to drive the same commands.

Bare input (no leading `:`) is parsed as a term and typechecked in the active theory. Every other command starts with a colon. The command set is small and intentionally Haskell-GHCi-shaped: load a file, list what is loaded, switch the active theory, inspect its sorts and ops, type a term, normalize it, browse the free model, compile an instance, quit.

## Usage

```text
$ panproto-repl --load theories/th_arith.json
loaded: ThArith
ThArith> :sorts
Int
ThArith> :type add(zero, one)
: Int
ThArith> :normalize add(zero, one)
=> one
ThArith> :quit
```

## Commands

| Command | What it does |
|---------|-------------|
| `:load <path>` | Load a theory document from `.ncl`, `.json`, or `.yaml`; adds every theory and morphism in the bundle to the session |
| `:theories` | List loaded theories; the active one is marked with `*` |
| `:use <name>` | Switch the active theory |
| `:sorts` | List the active theory's sorts, with parameters if any |
| `:ops` | List the active theory's operations and their signatures |
| `:type <term>` | Parse and typecheck `<term>` in the active theory; print its inferred sort |
| `:normalize <term>` | Reduce `<term>` through the active theory's directed equations |
| `:model [depth]` | Build the free model and print the first few inhabitants of each fiber |
| `:instance <name>: <class> in <target> { <bindings> }` | Compile an instance morphism from the class to the target on the fly |
| `:quit` / `:q` | Leave the REPL |
| `<term>` | Bare input: parse and typecheck as a term in the active theory |

## License

[MIT](../../LICENSE)
