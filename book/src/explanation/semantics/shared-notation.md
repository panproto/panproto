# Shared notation

The semantics chapters use inference rules for static relations and equations for mathematical or operational functions. An inference rule has premises above a horizontal line and a conclusion below it. A semantic equation uses double brackets, as in $\llbracket e \rrbracket\,\rho=v$, to give the value of syntax $e$ in an environment $\rho$. These notations specify behavior; they do not imply that a theorem prover checks the specification.

A *context* (also called an *environment*) is a finite map from names to data. A typing context maps variable names to types. An evaluation context maps variable names to values. A theory context maps sort names to their definitions.

## Environments

A typing environment, written $\Gamma$, maps variables to types; $\Gamma,x:\tau$ extends it with a variable $x$ of type $\tau$. A value environment, written $\rho$, maps variables to values, and $\rho,x\mapsto v$ extends it with a value. A theory context, written $\Theta$, records the sorts available in a generalized algebraic theory (GAT) presentation.

A judgment of the form $\Gamma \vdash e : \tau$ asserts that under typing context $\Gamma$, the expression $e$ has type $\tau$. The expression-language page does not assign this judgment to `panproto-expr`, whose current classifier is intentionally weaker.

## Inference rules

An inference rule has the form

$$
\frac{\text{premises}}{\text{conclusion}} \quad (\text{rule-name})
$$

Each premise and the conclusion are judgments. The rule asserts that whenever the premises hold, the conclusion follows. A derivation is a tree of rule applications whose leaves are axioms (rules with no premises) and whose root is the judgment being proved.

## Semantic functions

The semantic function for a syntactic category $C$ is written $\llbracket \cdot \rrbracket_C : C \to D$ where $D$ is the semantic domain. The subscript is omitted when context determines the category. In the expression chapter, $\mathsf{eval}(e,\rho,c)$ returns either a value or an `ExprError` under resource configuration $c$. In the lens chapter, the denotation of a lens is a triple consisting of a forward function, a backward function, and a complement-producing function. A theory denotation $\llbracket T\rrbracket$ is mathematical notation; the compiler returns a `panproto_gat::Theory` value rather than a separately represented semantic model.

## Errors and partiality

The symbol $\bot$ denotes undefinedness only on pages that introduce it explicitly. `panproto-expr` instead returns a concrete error sum, including `StepLimitExceeded` and `DepthExceeded`. Keeping errors separate preserves distinctions that a single bottom element would erase.

## Equality

Equality depends on the operation. Values ordinarily use the equality implemented by their Rust types. Some schema checks compare complete structures, while others compare identifiers, fingerprints, or specified multisets. Morphism checks compare endpoints and assignments. Each chapter states the equality used by the corresponding implementation check.

[Expression language](./expression-language.md) applies these conventions to the resource-bounded evaluator.
