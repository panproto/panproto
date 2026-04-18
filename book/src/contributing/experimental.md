# Experimental and feature-gated subsystems

Not every crate in the workspace is production-ready. Three are feature-gated and marked experimental — [`panproto-llvm`](https://docs.rs/panproto-llvm/latest/panproto_llvm/), [`panproto-jit`](https://docs.rs/panproto-jit/latest/panproto_jit/), and [`panproto-xrpc`](https://docs.rs/panproto-xrpc/latest/panproto_xrpc/) — for reasons specific to each: an LLVM installation is a substantial dependency, the JIT compiler is promising but still incomplete, and the XRPC remote-repository support is not yet stable across registry differences. The present chapter describes each crate, says where it is currently reliable, and names the acceptance criteria for promoting it to the stable default-on configuration. A contributor working in any of them should treat the API surface as subject to change between minor versions.

## panproto-llvm

[`panproto-llvm`](https://docs.rs/panproto-llvm/latest/panproto_llvm/) registers [LLVM](https://llvm.org/) IR as a panproto protocol: a theory whose sorts are LLVM IR's kinds (modules, functions, basic blocks, instructions, types), whose operations are the structural accessors of those kinds, and whose equations encode LLVM's well-formedness conditions. A schema under this protocol is an LLVM module's structure at the theory level, and a migration is a structural transformation of the module (rename an intrinsic, reorganise a function's control flow, add a pass's effect to the module's metadata).

The crate is feature-gated, since it depends on an LLVM installation at build time: a significant cost for the 99% of panproto users who do not need it. When the feature is enabled, the LLVM bindings go through [`inkwell`](https://docs.rs/inkwell/latest/inkwell/) against whichever LLVM version the user has installed, and the schema-level operations dispatch to LLVM's own module manipulation APIs for the operations panproto cannot express purely in its own term language.

The current state is that theory construction is complete and migration between LLVM versions (an LLVM IR migration from LLVM 16 to LLVM 17, for example) works for the well-typed subset of modules. The parts that do not yet work reliably are complex inter-procedural optimizations expressed as migrations; the existence-checker rejects many plausible such migrations for reasons that appear to be conservative rather than necessary.

Acceptance criterion: the LLVM 17-to-LLVM 18 migration should apply to the full kernel of the Linux kernel's IR output without rejection, and the round-trip (parse then emit) through the panproto representation should produce IR that the LLVM assembler accepts without adjustment.

## panproto-jit

[`panproto-jit`](https://docs.rs/panproto-jit/latest/panproto_jit/) compiles [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) evaluations to native code through LLVM. When a migration engine needs to run the same expression against a large number of instance records, the cost of the small-step evaluator becomes noticeable; JIT-compiling the expression into a single native function amortises the cost across the records and improves throughput on large datasets. The compiler architecture follows the standard pipeline treatment of @appel1998modern.

The crate is feature-gated alongside `panproto-llvm` and shares the LLVM-installation dependency. When enabled, it replaces the small-step evaluator of [Totality and termination](../expr/totality.md) for expressions above a configurable size threshold, falling back to the interpreter for expressions below the threshold (where the compile cost would not pay back).

The current state covers integer arithmetic, string operations, and simple list operations. Pattern matching and list comprehensions are not yet supported; an expression containing either falls back to the interpreter. Totality guarantees are preserved (the JIT respects the step and depth limits through explicit counter decrements in the compiled code), but the performance of the compiled code on the supported subset is noticeably faster than the interpreter only for expressions that repeat over many records.

Acceptance criterion: pattern matching support, a benchmark where the JIT outperforms the interpreter by at least 5x on a representative migration workload, and deterministic equivalence between JIT and interpreter outputs on the standard test suite.

## panproto-xrpc

[`panproto-xrpc`](https://docs.rs/panproto-xrpc/latest/panproto_xrpc/) implements a client for remote panproto-vcs operations through the [XRPC](https://atproto.com/specs/xrpc) protocol used by the AT Protocol ecosystem. The crate allows a panproto-vcs repository to be pushed to and pulled from a remote server that exposes a standard XRPC endpoint, providing an alternative to the git-bridge approach of [The git bridge](../vcs/git-bridge.md) for collaborations that are already running their own XRPC-based infrastructure.

The crate is feature-gated for two reasons: the XRPC dependency is specific to AT Protocol and does not benefit users not working in that ecosystem, and the remote-repository semantics for panproto-vcs are not yet stable. A push to a remote XRPC repository works; a pull works; a merge across a remote branch and a local branch works in the common cases. The cases that do not yet work reliably are the ones where the remote and local sides have different sets of registered protocols, so that a commit on the remote references a protocol the local side does not know about.

Acceptance criterion: cross-protocol-registry merge resolution, a stability guarantee on the XRPC wire format across minor versions, and a round-trip test suite comparable to the git bridge's.

## The pattern

Each of these subsystems lives in the workspace rather than in a separate repository so that it can evolve alongside the main panproto stack. A change to [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/)'s theory representation, for example, is propagated to [`panproto-llvm`](https://docs.rs/panproto-llvm/latest/panproto_llvm/) at the same time as to the other theory consumers, which avoids the drift that a separate-repository arrangement would produce.

A contributor who wants to work on any of these crates has to opt in through feature flags and be aware that their work is against a moving target. In exchange, promising ideas can be tried out in tree, measured against the rest of the stack, and either promoted or abandoned without the friction of separate-repository maintenance.

A crate is promoted to stable when its acceptance criterion is met and a release includes the promotion as a changelog entry. Demotion back to experimental is possible but rare; it has not happened in the crates currently listed as experimental.

## Further reading

For the JIT compiler architecture, @appel1998modern is the standard pipeline treatment. The LLVM project's own documentation at https://llvm.org/docs/ covers the IR and the pass infrastructure panproto-llvm integrates with. For the XRPC protocol that panproto-xrpc implements, @atproto is the authoritative specification.

## Closing

Part VII ends with this chapter. The appendices that follow contain a [notation reference](../appendices/notation-table.md), a [glossary](../appendices/glossary.md), and an [open-problems list](../appendices/open-problems.md) covering places where panproto's implementation or the book's mathematical account is ahead of the other.
