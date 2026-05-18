# Theory DSL: denotational semantics

## In plain terms

The theory DSL is what you use to declare a new protocol's schema language. You write down the basic kinds of thing the protocol talks about (its *sorts*), the constructors that build them (its *operations*), and the equations they satisfy. The DSL compiles to a generalized algebraic theory (GAT) that the rest of panproto consumes uniformly.

This page pins down what a GAT presentation is, what category it generates, and what counts as a valid presentation.

## Surface syntax

The Nickel surface (canonical authoring form). JSON and YAML surfaces are isomorphic via `serde`. Every document carries an `id`, a `description`, and exactly one body variant (theory, morphism, composition, protocol, bundle, class, instance, or inductive type).

A bare theory body:

```nickel
{
  id = "dev.example.thgraph",
  description = "Directed multigraph with identity edges",
  theory = "ThGraph",
  sorts = [ { name = "Vertex" }, { name = "Edge" } ],
  ops = [
    { name = "src", inputs = [{ name = "e", sort = "Edge" }], output = "Vertex" },
    { name = "tgt", inputs = [{ name = "e", sort = "Edge" }], output = "Vertex" },
    { name = "id",  inputs = [{ name = "v", sort = "Vertex" }], output = "Edge" },
  ],
  equations = [
    { name = "src-id", lhs = "src(id(v))", rhs = "v", context = [{ name = "v", sort = "Vertex" }] },
    { name = "tgt-id", lhs = "tgt(id(v))", rhs = "v", context = [{ name = "v", sort = "Vertex" }] },
  ],
}
```

The full grammar is in [`crates/panproto-theory-dsl/src/document.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-theory-dsl/src/document.rs).

## Abstract syntax

```rust
pub struct TheoryDocument {
    pub id: String,
    pub description: String,
    pub body: TheoryBody,
}

pub enum TheoryBody {
    Theory(TheorySpec),
    Morphism(MorphismSpec),
    Composition(CompositionBody),
    Protocol(Box<ProtocolSpec>),
    Bundle(Box<BundleSpec>),
    Class(ClassSpec),
    Instance(InstanceSpec),
    Inductive(InductiveSpec),
}

pub struct TheorySpec {
    pub theory: String,                       // theory name
    pub extends: Vec<String>,                 // parent theories
    pub imports: Vec<ImportSpec>,             // imports with optional aliases
    pub sorts: Vec<SortSpec>,                 // sort declarations (with dependent params)
    pub ops: Vec<OpSpec>,                     // operation declarations
    pub equations: Vec<EquationSpec>,         // judgemental equalities
    pub directed_equations: Vec<DirectedEqSpec>,  // rewrite rules
    pub policies: Vec<PolicySpec>,            // conflict policies
}
```

The DSL document compiles to `panproto_gat::Theory` (and, for non-`Theory` bodies, to `TheoryMorphism` or `Protocol`). The intermediate surface types (`TheorySpec`, `OpSpec`, etc.) are deserialisation targets, not the categorical objects; the categorical objects are the GAT types.

## Sort, operation, and equation judgements

A sort is well-formed in a theory context $\Theta$ when it appears in the sort list:

$$
\frac{S \in \mathsf{sorts}(\Theta)}{\Theta \vdash S\,\mathsf{sort}} \quad (\text{sort-wf})
$$

An operation is well-typed when its inputs are well-typed sorts and its output is a well-typed sort dependent on the inputs:

$$
\frac{
  \Theta \vdash S_1\,\mathsf{sort} \;\;\cdots\;\; \Theta \vdash S_n\,\mathsf{sort}
  \quad
  \Theta, x_1 : S_1, \ldots, x_n : S_n \vdash T\,\mathsf{sort}
}{
  \Theta \vdash f : (x_1 : S_1, \ldots, x_n : S_n) \to T\,\mathsf{op}
} \quad (\text{op-wf})
$$

An equation is well-formed when both sides type-check at the same sort under the same context:

$$
\frac{
  \Theta; \Gamma \vdash t_1 : T \quad \Theta; \Gamma \vdash t_2 : T
}{
  \Theta \vdash t_1 = t_2 : T \,[\Gamma]\,\mathsf{eqn}
} \quad (\text{eqn-wf})
$$

## Semantic domain

The semantic universe is $\mathsf{CwF}$, the (2-)category of *categories with families* (Cartmell-style: contexts as objects, substitutions as morphisms, types and terms forming a presheaf over substitutions). The denotational semantics is the functor

$$
\llbracket \cdot \rrbracket : \mathsf{TheoryDocument} \to \mathsf{CwF}
$$

defined by case on the document body (with the bundle case mapping to a tuple of CwFs). For the bare theory case:

$$
\llbracket \mathsf{TheoryDocument}\{\mathsf{body} = \mathsf{Theory}(T)\} \rrbracket \;=\; \mathbf{CwF}(T)
$$

where $\mathbf{CwF}(T)$ is the *initial* CwF satisfying the sort, operation, and equation declarations of $T$. Initiality is the *semantic content* of the GAT construction: for every CwF $\mathcal{C}$ that interprets $T$'s sorts and operations and validates its equations, there is a unique structure-preserving functor $\mathbf{CwF}(T) \to \mathcal{C}$. The GAT presentation framework is due to Cartmell (@cartmell1986generalised), and the category-with-families packaging of dependent type theory is Dybjer's (@dybjer1996internal); the two compose to give the initial-model semantics used here.

A *schema* in panproto is a model of $\mathbf{CwF}(T)$ in the CwF of finite sets and functions: equivalently, a CwF morphism $\mathbf{CwF}(T) \to \mathbf{FinSet}$.

## Semantic equations for the body cases

$$
\begin{aligned}
\llbracket \mathsf{Theory}(T) \rrbracket
  &= \mathbf{CwF}(T) \\[2pt]
\llbracket \mathsf{Morphism}(M : T_1 \to T_2) \rrbracket
  &= F_M : \mathbf{CwF}(T_1) \to \mathbf{CwF}(T_2) \\[2pt]
\llbracket \mathsf{Composition}(\mathsf{compose} = \{r, b, [c_1, \ldots, c_k]\}) \rrbracket
  &= \mathbf{CwF}\bigl(\mathrm{colim}\,(b, c_1, \ldots, c_k)\bigr) \\[2pt]
\llbracket \mathsf{Class}(C) \rrbracket
  &= \mathbf{CwF}(\mathrm{class\text{-}to\text{-}theory}(C)) \\[2pt]
\llbracket \mathsf{Instance}(I : C \to T) \rrbracket
  &= F_I : \mathbf{CwF}(\mathrm{class\text{-}to\text{-}theory}(C)) \to \mathbf{CwF}(T) \\[2pt]
\llbracket \mathsf{Inductive}(D) \rrbracket
  &= \mathbf{CwF}(\mathrm{inductive\text{-}to\text{-}theory}(D)) \\[2pt]
\llbracket \mathsf{Protocol}(P) \rrbracket
  &= \bigl(\mathbf{CwF}(P.\mathsf{schema\_theory}),\,\mathbf{CwF}(P.\mathsf{instance\_theory}),\,\mathbf{Edge}(P)\bigr) \\[2pt]
\llbracket \mathsf{Bundle}(B) \rrbracket
  &= \prod_{x \in B} \llbracket x \rrbracket
\end{aligned}
$$

where $\mathrm{colim}$ is the iterated pushout described under [Pushouts and merge](./pushouts-and-merge.md) and the auxiliary `class-to-theory` and `inductive-to-theory` are the desugarings implemented in `panproto-theory-dsl/src/compile_class.rs` and `compile_inductive.rs`.

The interpretation of the composition body factors through the colimit construction in $\mathsf{Th}$: $\llbracket T_1 +_{S} T_2 \rrbracket = \llbracket T_1 \rrbracket +_{\llbracket S \rrbracket} \llbracket T_2 \rrbracket$, exploiting the fact that $\mathbf{CwF}$ preserves the relevant colimits.

## Soundness and registration

A protocol registration is the construction of a colimit diagram. If any pushout step in the diagram fails to satisfy the universal property (because two equations contradict on a shared sort), registration panics with a message naming the failing intermediate step:

```text
panic: colimit ThGraph + ThConstraint over ThVertex failed:
       equation `src(id(v)) = v` contradicts `src(id(v)) = source(v)`
```

This is a build-time bug in the theory composition; the panic is intentional. See [`crates/panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs).

## What is intentionally not modelled

- **Higher dimensions.** The DSL is for 1-categorical GATs only. We do not model homotopical structure, 2-cells, or coherent equations.
- **Infinitary signatures.** Operations have finite arity. There is no way to declare an operation that takes an unbounded list of arguments.
- **Term rewriting decidability.** Equation orientation and confluence are the user's responsibility; the colimit construction does not check for a complete rewriting system.

## See also

- [Pushouts and merge](./pushouts-and-merge.md) for the colimit construction and verified universal property.
- [Composing protocols by colimit (plain-terms)](../protocol-colimits.md).
- [Reference: protocol catalogue](../../reference/protocols.md).
- [How-to: build a custom protocol](../../how-to/build-protocol.md).
- @cartmell1986generalised for the original GAT formulation.
