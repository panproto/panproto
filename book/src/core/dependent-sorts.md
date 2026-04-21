# Dependent sorts in practice

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

The GATs chapter said, abstractly, that a sort may be indexed by an earlier sort's inhabitants. This chapter pins down what that buys the working engineer, using the simply-typed lambda calculus as a worked example, and shows how `panproto-gat` typechecks a theory that exercises every dependent-sort feature the engine supports.

The point of the example is not that panproto is a type-theory engine; it is that the same dependent-sort machinery the engine uses to keep track of domain and codomain in a category protocol, or source and target vertex kinds in a graph schema, is expressive enough to encode a system programmers already recognise. If the machinery handles STLC, it handles the smaller dependencies that arise in ordinary protocols without strain.

## STLC as a GAT

The theory has three sorts. The context sort $\mathsf{Ctx}$ and the type sort $\mathsf{Ty}$ are global. The term sort $\mathsf{Tm}(\Gamma, A)$ is parameterised by a context and a type: it stands for the collection of closed terms in context $\Gamma$ at type $A$. A term in the empty context at type $\mathrm{int} \to \mathrm{int}$ is a different inhabitant from a term in a one-binding context at the same type, and the indexing records the difference.

The operations are familiar:

```
arrow    : (A : Ty, B : Ty) -> Ty
extend   : (G : Ctx, A : Ty) -> Ctx
emptyCtx : () -> Ctx
var_zero : (G : Ctx, A : Ty) -> Tm(extend(G, A), A)
lam      : (G, A, B, body : Tm(extend(G, A), B)) -> Tm(G, arrow(A, B))
app      : (G, A, B, f : Tm(G, arrow(A, B)), x : Tm(G, A)) -> Tm(G, B)
subst    : (G, A, B, body : Tm(extend(G, A), B), x : Tm(G, A)) -> Tm(G, B)
```

Every output sort references the input parameters explicitly: `var_zero` lives in the extended context with the fresh variable's type on the right; `lam` lives in the original context at the arrow type; `app` lives in the original context at the range. The meta-theoretic invariants that would require a side condition in an extrinsic presentation (e.g., "the source of the substitution matches the context of the body") are structural in this presentation, because the sort itself names the context.

There is one equation, the $\beta$-law stated as a rewrite between two already-well-typed terms:

```
app(G, A, B, lam(G, A, B, body), x)  =  subst(G, A, B, body, x)
```

The equation typechecks because both sides have the same output sort, $\mathrm{Tm}(\Gamma, B)$, under a single inferred variable context. There is no metavariable capture to worry about, because the substitution is itself a named operation in the signature rather than a primitive of the meta-language. A type-theorist will recognise this as a "formal" or "explicit" substitution presentation, due ultimately to @martinlof1984intuitionistic and elaborated categorically by @dybjer1996internal; the categorical story of the same encoding is worked out in depth by @hofmann1997syntax.

## What `panproto-gat` does with this

The typechecker in [`panproto_gat::typecheck`](https://docs.rs/panproto-gat/latest/panproto_gat/typecheck/) accepts the theory as stated. For each operation, it walks the signature accumulating a substitution from input parameter names to concrete argument terms; the expected sort for each argument is the declared input sort under that substitution, and the inferred output sort of the application is the declared output sort under the final substitution. For the $\beta$ equation specifically, it runs Robinson unification over the two sides' sort expressions, yielding a single context assignment that types both halves to the same $\mathrm{Tm}(\Gamma, B)$.

Concretely, applying `app` to a well-chosen argument tuple reduces to a substitution computation. Take `f` with context-declared sort $\mathrm{Tm}(\Gamma, \mathrm{arrow}(A, B))$ and `x` with sort $\mathrm{Tm}(\Gamma, A)$: the typechecker instantiates `G := Γ`, `A := A`, `B := B`, `f := f`, `x := x`, and the output sort is $\mathrm{Tm}(\Gamma, B)$. Give `x` the wrong sort — say $\mathrm{Tm}(\Gamma, B)$ when `app` expects $\mathrm{Tm}(\Gamma, A)$ — and the typechecker rejects the application with an `ArgTypeMismatch` error that pins the failure to the specific argument position.

The integration test at `tests/integration/tests/stlc_gat.rs` exercises exactly this flow, end to end, including the $\beta$ equation and a deliberate ill-typed rejection.

## Why this encoding sidesteps capture-avoiding substitution

The textbook concern with encoding $\lambda$-calculus as an algebraic theory is that naive substitution captures free variables: substituting a term with a free `x` into a context that already binds `x` silently changes the term's meaning. The usual remedies are de Bruijn indices, nominal sets, higher-order abstract syntax, or ad-hoc freshness side conditions. Each remedy has its own costs and complexities.

The encoding above pays a different price and sidesteps the problem entirely. There is no binder in the meta-language: `lam` takes a `body` whose sort names the extended context directly, and `var_zero` is an explicit variable-projection operation rather than a name that has to be compared structurally. Because the meta-language's only operation on variables is the one the theory declares, capture is impossible: the engine never has the opportunity to alpha-rename anything. The $\beta$ rule's right-hand side calls the declared `subst` operation, whose semantics are whatever the theory's equations fix; no appeal to a meta-level substitution function is required.

The cost is that the theory has to carry an explicit `extend` / `var_zero` / `subst` triple, and the bookkeeping that a structural substitution would provide for free in a traditional presentation has to be stated as equations. For a language as small as STLC, this is a fair trade. For a full dependent type theory the same style scales up, at the price of more equations, as @dybjer1996internal and subsequent categories-with-families work up in detail.

## What this unlocks for protocols

The machinery that makes STLC work is not STLC-specific. Any protocol whose sorts carry indexing — a relational schema where a column's sort depends on its table, a graph schema where an edge's sort depends on its source and target vertex kinds, a message schema where a field's sort depends on a discriminator — uses the same mechanism. The GATs chapter ([How panproto-gat represents dependent sorts](../foundations/gats.md)) states the mechanism abstractly; this chapter is the worked case that makes the abstraction concrete.

The typical indexing depth for the protocols covered in Part IV is shallow (one or two parameters), far short of what STLC demands. Reading the STLC example is therefore a way of confirming that the engine is not near its limits when it handles the simpler cases; it is doing, in production, a much weaker version of what the worked example exercises.

## Further reading

@cartmell1986generalised gives the framework. @martinlof1984intuitionistic and @dybjer1996internal develop the type-theoretic program GATs serve. @hofmann1997syntax works out the syntactic–categorical correspondence in the detail a reader implementing a dependent-type engine would want. The integration tests at `tests/integration/tests/stlc_gat.rs` and `tests/integration/tests/dependent_sorts.rs` are the runnable counterparts of the examples in this chapter.
