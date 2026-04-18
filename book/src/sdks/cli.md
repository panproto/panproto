# The CLI

Most first-time users of panproto encounter it through the `schema` command — a single binary built from [`panproto-cli`](https://docs.rs/panproto-cli/latest/panproto_cli/) that exposes the engine's operations as subcommands, covering the same ground the SDKs of the previous chapters cover programmatically. The present chapter is a reference for the subcommand surface, organised by workflow rather than alphabetically.

## Installation

The binary is installed through `cargo install`:

```
cargo install panproto-cli
```

*Listing 10.1: Installing the `schema` command from crates.io. Pre-built binaries are also published on GitHub Releases for common platforms; consult the installation page of the main documentation for platform-specific instructions.*

The binary's name is `schema`, not `panproto`. The choice follows the convention of domain-specific tooling naming the command after the domain object rather than the tool: `git` operates on repositories, `docker` on containers, `schema` on panproto schemas. A developer who finds `schema` ambiguous may alias it in their shell.

## Define a schema

A schema is defined by writing a source file in whichever protocol the schema belongs to (an ATProto lexicon JSON, an Avro IDL file, a Rust source file, and so on) and running:

```
schema define my-schema.json
```

The subcommand invokes the relevant protocol's parser, validates the resulting schema against the protocol's theory, and stores the schema in the working directory's `.panproto` object database. The schema is addressable by the identifier declared in the source file.

## Migrate across schema versions

A migration is declared in a separate file (migration DSL, typically in [Nickel](https://nickel-lang.org/) or JSON) and applied with:

```
schema migrate --from old-schema --to new-schema --using migration.ncl data.json
```

The subcommand reads the migration declaration, compiles it through [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/), runs the compiled migration against the input instance (in `data.json`), and writes the output instance to stdout or to a file named by `--output`. If the migration cannot be compiled or the input cannot be lifted, the failure is reported at the specific stage responsible, with the diagnostics of [The restrict/lift pipeline](../core/restrict-lift.md).

## Check compatibility

`schema check` compares two schemas and reports their compatibility classification, along the lines of [Apache Avro's](./avro.md) backward-forward-full compatibility. The subcommand is the front-end for [`panproto-check`](https://docs.rs/panproto-check/latest/panproto_check/) and is what the CI integration described in the `panproto-breaking-change-ci` skill invokes.

```
schema check old-schema new-schema
```

The output is a compatibility verdict (backward, forward, full, or incompatible) together with the specific schema changes the classification rests on. The exit code is zero on full compatibility and non-zero otherwise, so the command can gate a CI pipeline.

## Version control

A panproto-vcs repository is initialised with `schema init`, which creates a `.panproto` directory in the current working directory. Commits, branches, merges, and log inspection work through the sub-tree of `schema vcs` commands:

```
schema vcs commit -m "Add email field to Patient"
schema vcs branch --from main feature-branch
schema vcs merge main feature-branch
schema vcs log
```

The interface follows [git](https://git-scm.com/)'s conventions closely. A reader comfortable with git's CLI will find the `schema vcs` subcommands to have the same shape, with the addition of a small number of schema-aware commands (`schema vcs diff-schemas`, `schema vcs blame-schema`) that operate on panproto's extended object model rather than on byte trees.

## Convert between formats

`schema convert` is the umbrella subcommand for cross-protocol translations. It reads data in one format, applies a cross-protocol lens (covered in [Part II](../core/lenses.md) and the `panproto-cross-protocol` skill), and writes the data out in another format.

```
schema convert --from avro --to protobuf schema.avsc > schema.proto
schema convert --from avro-data --to protobuf-data --with schema-map.ncl data.avro > data.pb
```

The first form converts a schema definition; the second converts instance data against a pair of mapped schemas. Both invoke the same protocol-registry machinery.

## Stable and experimental subcommands

Some subcommands are marked `[experimental]` in the help output. These include `schema llvm` (lowering to LLVM IR through the experimental [`panproto-llvm`](https://docs.rs/panproto-llvm/latest/panproto_llvm/) crate), `schema jit` (native-code compilation of expressions through [`panproto-jit`](https://docs.rs/panproto-jit/latest/panproto_jit/)), and `schema xrpc` (remote-repository operations through [`panproto-xrpc`](https://docs.rs/panproto-xrpc/latest/panproto_xrpc/)). These subcommands are subject to change between releases and are not covered by the semver guarantees of the main API surface. A note in [Experimental and feature-gated subsystems](../contributing/experimental.md) gives more detail on each.

## Shell completion

The binary emits shell completions for bash, zsh, and fish through `schema completions <shell>`. A typical installation runs the command once and sources the output in the shell's rc file.

## Closing

The next chapter opens Part VII, which is for contributors. It starts with the [workspace layout](../contributing/workspace.md), then covers the [CI pipeline](../contributing/ci.md), [how to extend panproto with a new protocol or lens combinator](../contributing/extending.md), and the [experimental and feature-gated subsystems](../contributing/experimental.md) the main chapters have gestured at.
