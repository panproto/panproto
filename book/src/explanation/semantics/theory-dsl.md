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

Before a document value is deserialized, [`read_document`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/shape/fn.read_document.html) checks it against the document grammar, the grammar the serde types accept together with two rules those types cannot state: a document carries exactly one of the body keys `theory`, `morphism`, `compose`, `protocol`, `bundle`, `class`, `instance` and `inductive` (an `instance` document's `class` member is that body's own field), and no object carries a member the grammar does not name. The check refuses the first value that breaks the grammar, in an order fixed by the grammar and not by the source: an object is checked for an unknown member first (the least such name, comparing names byte by byte), then field by field in the order of its field table, a missing required field being refused at the object; a map's members are checked in the order of their names, and an array's elements in order. When an object names a member twice, the last occurrence is the member read, as a JSON reader keeps it. Each refusal is a `TheoryDslError::Shape` carrying the path of the refused value and a `ShapeProblem`.

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

[`compile_theory_with_resolver`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/compile_theory/fn.compile_theory_with_resolver.html) elaborates the theory body as the next section describes, checks the signature with `typecheck_signature`, refuses an implicit input that cannot be inferred or a closed sort with the wrong constructors, and then types each equation in order with `typecheck_equation`. An equation's variables take their sorts from the argument positions they fill; an argument that is itself an application also constrains the implicit inputs of the operation it is passed to, so that with `comp` taking its objects `a b c` implicitly, `comp(id(a), f) = f` types `f` at `Hom(x, a)` for a fresh `x`. Implicit inputs are solved by first-order unification in which only the operation's own implicit parameters are unknowns: the equation's variables are fixed, so `comp(id(a), id(b))` is refused rather than read as identifying `a` with `b`.

## Parents, imports and names

A theory body may name parents in `extends` and other theories in `imports`; the resolver supplies both. The compiled theory's declarations are those of each parent, in `extends` order, then those of each import, renamed, in `imports` order, then the spec's own sorts, operations and equations. A declaration equal to one already included is included once, so two parents that share a grandparent contribute its declarations once; two declarations of one name that remain are refused by the signature check as a duplicate, including a spec that restates a parent's sort. An unknown parent or import is refused with `TheoryNotFound`.

An import of theory $F$ with alias $A$ and exposed names $X$ renames each declaration name $n$ of $F$ (sorts, operations, equations, directed equations and policies) to $ho(n)$, which is $n$ when $n \in X$, `A_n` when there is an alias and `F_n` otherwise, and applies the renaming to every sort and operation a declaration mentions. Exposing a name $F$ does not declare is refused with `UnknownExposed`. In the spec's own declarations, a sort head or operation head $t$ stands for $t$ when the spec or one of its parents declares $t$ as a symbol of that kind; otherwise for $ho(n)$ when $t$ is `F.n` or `A.n` for the first import with that qualifier that declares $n$; otherwise for $ho(t)$ for the first import that declares $t$ and either has no alias or exposes $t$; otherwise for $t$ itself, which the signature check then refuses if nothing declares it. Inherited names thus behave as the spec's own and are never shadowed by an import.

An operation declares its inputs either with the shorthand `input`, one input whose binder is the first character of the sort as written, lowercased, when that character is an ASCII letter and `x` otherwise, or with an `inputs` list; declaring both is refused with `AmbiguousOpInputs`. In a term, an application's arguments are separated by commas and none may be empty, so `f(x,)` is refused as a term parse error.

## Compilation by body variant

Write $\mathsf{compile}(D,R)$ for compilation of document $D$ with resolver $R$. The result is a finite set of named theories, morphisms, protocols, and composition specifications:

$$
\mathsf{compile} : \mathsf{TheoryDocument} \times \mathsf{Resolver}
\longrightarrow \mathsf{Result}(\mathsf{CompiledTheorySet},\mathsf{TheoryDslError}).
$$

The dispatcher handles the eight variants shown above. Theory, class, and inductive bodies produce theories; morphism and instance bodies produce morphisms checked by `check_morphism`. A morphism document's map entry naming no symbol of the domain is refused with `UnknownMapKey` (the least such sort-map key, then the least operation-map key) before the check runs. Composition bodies resolve their inputs and replay specified colimit steps. Protocol bodies compile their theories and edge rules. A bundle compiles its theories in listed order, each seeing the bundle's earlier theories before the resolver's, then its compositions, its morphisms and its protocols; a theory, composition result, morphism or protocol named twice is refused with `Duplicate`.

## Refusals and their location

Every refusal names the document path of the element refused, written as a JSON Pointer: a `Shape` refusal carries its path, and every other refusal is wrapped in `TheoryDslError::At`. A declaration a parent or import contributes is located at its `extends` or `imports` entry; a signature defect at the declaration it concerns (the second of two duplicates, the `input` member for the shorthand); an ill-typed equation at the equation; a morphism check refusal at the map entry it concerns (the map itself for a missing entry, the spec for an equation the morphism does not preserve). [`read_source`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/compile/fn.read_source.html), [`compile_source`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/compile/fn.compile_source.html) and `compile_with_source` locate a refusal of JSON text by the byte range of the value at that path, quotes and brackets included, in `TheoryDslError::Spanned`; a JSON syntax error is located at the byte where the reader stopped. A refusal of YAML text carries its path only, and a Nickel evaluation error, a contract breach among them, keeps the span Nickel reports.

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
