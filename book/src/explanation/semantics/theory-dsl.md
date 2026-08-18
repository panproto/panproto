# Theory DSL: compilation semantics

## In plain terms

The theory DSL declares the sorts, operations, and equations that define a schema language. We compile those declarations to concrete `panproto_gat::Theory`, `TheoryMorphism`, and `Protocol` values so the rest of panproto can typecheck terms, validate morphisms, and compose theories independently of the source format.

The mathematical reading is a generalized algebraic theory (GAT) presentation. panproto implements the presentation and its checks directly. It does not construct a category-with-families object in Rust, so the account below distinguishes the conceptual model from the executable artifact.

[Shared notation](./shared-notation.md) fixes the symbols used below, while [Schemas as theories](../schemas-as-theories.md) supplies the intermediate-level motivation for the compiler model.

## Surface syntax

Nickel is the canonical authoring form, while JSON and YAML deserialize to the same document types. The following document declares a small graph theory with identity edges:

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

The deserialized Rust types have the schematic shape below; omitted derives, imports, boxes, and supporting types make the listing non-runnable.

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

`TheorySpec` and its supporting structs are source-level deserialization targets. Compilation turns a theory body into a `Theory`; other bodies may add morphisms, protocols, composition specifications, or several such values to a `CompiledTheorySet`.

## Well-formed presentations

Let $\Theta$ be a theory context and $\Gamma$ a term context. A declared sort is available when it belongs to the presentation:

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

`compile_theory_inner` constructs the `Theory` and then calls `panproto_gat::typecheck_theory`. Term-parse and typecheck failures are returned as `TheoryDslError`; JSON and YAML callers can use `compile_with_source` to attach a source span to a typecheck diagnostic.

## Compilation by body variant

Write $\mathsf{compile}(D,R)$ for compilation of document $D$ with resolver $R$. The result is a finite set of named theories, morphisms, protocols, and composition specifications:

$$
\mathsf{compile} : \mathsf{TheoryDocument} \times \mathsf{Resolver}
\longrightarrow \mathsf{Result}(\mathsf{CompiledTheorySet},\mathsf{TheoryDslError}).
$$

The dispatcher handles eight bodies. It typechecks theories directly and checks resolved morphisms for preservation. Composition bodies resolve their pieces before computing a colimit, while protocol bodies compile their theories and edge rules. Class and inductive bodies desugar to theories; instances desugar to morphisms; bundles process definitions in dependency order.

`compile` also sample-checks declared coercion laws using the default coercion registry. `compile_with_registry` substitutes caller-provided samples, while `compile_unchecked` skips this particular law check. A passing sample check is evidence for the sampled values, not a proof of the declared coercion class.

## Mathematical interpretation

Cartmell's account develops generalized algebraic theories as a categorical framework for algebraic structure (@cartmell1986generalised); categories with families provide a related packaging for dependent type theory (@dybjer1996internal). Under this presentation-based reading, a theory morphism interprets the sorts and operations of one presentation in another while preserving equations.

panproto relies on the finite presentation and preservation checks that this perspective motivates. The runtime `Theory` type is a named collection of sorts, operations, undirected equations, directed equations, and policies with lookup indices. The implementation does not expose a `CwF` type, construct an initial CwF, or verify an equivalence between `Schema` and CwF morphisms into finite sets. Those are explanatory semantics rather than current executable claims.

## Composition and verified boundaries

Composition bodies use the colimit machinery described in [Pushouts and merge](./pushouts-and-merge.md). The colimit constructor validates the inclusion morphisms, and callers can check a proposed alternative cocone through `ColimitResult::verify_universal`. Built-in protocol registration assembles its schema and instance theories from the registered building blocks.

The current checks establish well-formed sort and operation references, term typing for equations, morphism preservation, and sampled honesty for declared coercions on the checked compilation path. They do not establish confluence or termination for every directed rewrite system. Rewrite-system validation can produce warnings without rejecting an otherwise well-typed theory.

## See also

- [Pushouts and merge](./pushouts-and-merge.md) for the colimit construction.
- [Composing protocols by colimit](../protocol-colimits.md) for the built-in theory components.
- [Protocol catalog](../../reference/protocols.md) for registered protocols.
- [Build a custom protocol](../../how-to/build-protocol.md) for the operational workflow.
