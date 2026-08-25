# Theory DSL: compilation semantics

The theory DSL declares the sorts, operations, and equations of a schema language. Compilation produces concrete `panproto_gat::Theory`, `TheoryMorphism`, and `Protocol` values used by term typechecking, morphism checking, and theory composition.

The mathematical reading is a generalized algebraic theory (GAT) presentation. The Rust implementation stores and checks finite presentations. The categorical interpretation below is not represented by a runtime data type.

[Shared notation](./shared-notation.md) fixes the symbols used below, while [Schemas as theories](../schemas-as-theories.md) supplies the intermediate-level motivation for the compiler model.

## Surface syntax

[Nickel](https://nickel-lang.org/) is the canonical authoring form; JSON and YAML deserialize to the same document types. The following document declares a small graph theory with identity edges:

```nickel
{
  id = "dev.example.identity-graph",
  description = "Directed graph with identity edges",
  theory = "IdentityGraph",
  sorts = [ { name = "Vertex" }, { name = "Edge" } ],
  ops = [
    { name = "src", inputs = [{ name = "e", sort = "Edge" }], output = "Vertex" },
    { name = "tgt", inputs = [{ name = "e", sort = "Edge" }], output = "Vertex" },
    { name = "id", inputs = [{ name = "v", sort = "Vertex" }], output = "Edge" },
  ],
  equations = [
    { name = "src-id", lhs = "src(id(v))", rhs = "v" },
    { name = "tgt-id", lhs = "tgt(id(v))", rhs = "v" },
  ],
}
```

This is an illustrative custom theory, not the built-in `ThGraph`, which contains only `Vertex`, `Edge`, `src`, and `tgt`. The full document grammar is defined in [`crates/panproto-theory-dsl/src/document.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-theory-dsl/src/document.rs).

## Document shapes

The deserialized Rust types have the following schematic shape. The listing omits representation details and is not runnable Rust.

```text
TheoryDocument { id, description, body }

TheoryBody = Theory(TheorySpec)
           | Morphism(MorphismSpec)
           | Composition(CompositionBody)
           | Protocol(ProtocolSpec)
           | Bundle(BundleSpec)
           | Class(ClassSpec)
           | Instance(InstanceSpec)
           | Inductive(InductiveSpec)

TheorySpec { theory, extends, imports, sorts, ops,
             equations, directed_equations, policies }
```

`TheorySpec` and its supporting structs are deserialization targets. A theory body produces one theory. Other body variants may produce morphisms, protocols, composition specifications, or a bundle containing several definitions.

## Well-formed presentations

Let $\Theta$ be the set of declarations already available in the theory and $\Gamma$ a term-variable context. A sort $S$ is well formed when it is declared in $\Theta$:

$$
\frac{S \in \mathsf{sorts}(\Theta)}{\Theta \vdash S\;\mathsf{sort}}
\quad (\text{sort-wf}).
$$

An operation declaration is well formed when its input and output sort expressions are well formed, including dependent parameters:

$$
\frac{
  \Theta \vdash S_1\;\mathsf{sort} \quad \cdots \quad \Theta \vdash S_n\;\mathsf{sort}
  \quad
  \Theta; x_1:S_1,\ldots,x_n:S_n \vdash T\;\mathsf{sort}
}{
  \Theta \vdash f:(x_1:S_1,\ldots,x_n:S_n)\to T\;\mathsf{op}
}
\quad (\text{op-wf}).
$$

An equation is well formed when both sides typecheck at the same sort:

$$
\frac{
  \Theta;\Gamma \vdash t_1:T
  \quad
  \Theta;\Gamma \vdash t_2:T
}{
  \Theta \vdash t_1=t_2:T\;[\Gamma]\;\mathsf{eqn}
}
\quad (\text{eqn-wf}).
$$

`compile_theory_inner` constructs a `Theory` and calls `panproto_gat::typecheck_theory`. Term parsing and typechecking failures are returned as `TheoryDslError`. `compile_with_source` can attach a span found in JSON or YAML source to a typecheck diagnostic; Nickel evaluation does not retain source positions for this path.

## Compilation by body variant

Write $\mathsf{compile}(D,R)$ for compilation of document $D$ with resolver $R$. The result is a finite set of named theories, morphisms, protocols, and composition specifications:

$$
\mathsf{compile} : \mathsf{TheoryDocument} \times \mathsf{Resolver}
\longrightarrow \mathsf{Result}(\mathsf{CompiledTheorySet},\mathsf{TheoryDslError}).
$$

The dispatcher handles the eight variants shown above. Theory, class, and inductive bodies produce theories; morphism and instance bodies produce morphisms checked by `check_morphism`. Composition bodies resolve their inputs and replay specified colimit steps. Protocol bodies compile their theories and edge rules. Bundles process definitions in dependency order.

`compile` also sample-checks declared coercion laws using the default coercion registry. `compile_with_registry` substitutes caller-provided samples, while `compile_unchecked` skips this particular law check. All three routes still typecheck theories and gate their directed rewrite systems. A passing coercion sample check is evidence for the sampled values, not a proof of the declared coercion class.

## Mathematical interpretation

Cartmell develops generalized algebraic theories as a categorical framework for algebraic structure [@cartmell1986generalised]. A category with families (CwF) gives a related categorical model of dependent type theory [@dybjer1996internal]. In the finite-presentation interpretation used here, a theory morphism maps the sorts and operations of one presentation into another while preserving signatures and equations.

The runtime `Theory` type is a named collection of sorts, operations, undirected equations, directed equations, and policies with lookup indices. The implementation does not expose a `CwF` type, construct an initial CwF, or verify an equivalence between schemas and CwF morphisms into finite sets.

## Composition and verified boundaries

Composition bodies use the colimit machinery described in [Pushouts and merge](./pushouts-and-merge.md). The `colimit` constructor checks cocone commutativity but does not run `check_morphism` on its inclusions, since a building-block instance theory may refer to sorts supplied only by the schema theory with which it is later combined. `ColimitResult::verify_universal` validates a constructed mediator for one caller-supplied alternative cocone. Built-in protocol registration assembles schema and instance theories from registered building blocks.

Compilation checks sort and operation references, types both sides of equations, validates resolved morphisms, and tests declared coercion laws on registered samples unless `compile_unchecked` is used. Passing a finite coercion sample is evidence, not proof. Theory compilation also calls `validate_rewrite_system`. An analysis failure returns `RewriteSystemCheck`. `UnsoundRewriteSystem` reports either divergent rewrite paths that do not rejoin (a non-joining critical pair) or failure of the lexicographic-path-order termination check. Built-in registration treats either result as an internal programming error. These finite checks are not a proof in a proof assistant.

The textual term parser rejects terms nested beyond `MAX_TERM_NESTING_DEPTH`, currently 128. This is an input-safety bound on the recursive parser and downstream traversals, not a bound on normalization steps. The REPL applies a separate 1,000-step normalization budget.

## See also

- [Pushouts and merge](./pushouts-and-merge.md) for the colimit construction.
- [Composing protocols by colimit](../protocol-colimits.md) for the built-in theory components.
- [Protocol catalog](../../reference/protocols.md) for registered protocols.
- [Build a custom protocol](../../how-to/build-protocol.md) for the operational workflow.
