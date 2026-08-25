# Expression language: operational meaning

`panproto-expr` evaluates pure value computations used by field transforms and queries. For a fixed expression, environment, instance context, and resource configuration, evaluation is deterministic and performs no external input or output. Resource exhaustion is reported through `ExprError`. The surface parser lowers Haskell-like notation to a small abstract syntax tree, but the accompanying type classifier detects only a subset of type errors.

## Surface syntax

The grammar below is a fragment of the implemented parser:

```bnf
expr  ::= literal
        | ident
        | expr expr
        | "\\" ident "->" expr
        | "let" ident "=" expr "in" expr
        | "if" expr "then" expr "else" expr
        | "case" expr "of" alts
        | expr "." ident
        | expr "[" expr "]"
literal ::= int | float | str | bool | "Nothing"
          | "[" expr,... "]"
          | "{" ident ["=" expr],... "}"
```

The optional expression in a record field permits punning: `{x}` means `{x = x}`. The parser also implements operators, closed ranges, list comprehensions, `do` notation, and `where` clauses. Open-ended ranges such as `[a..]` are rejected. These forms lower to the smaller abstract syntax below; in particular, conditionals lower to `Match`, since `Expr` has no `If` variant.

## Abstract syntax

The current Rust enum has the schematic shape below; omitted module paths and derives make the listing non-runnable.

```text
Expr = Var(name)
     | Lam(parameter, body)
     | App(function, argument)
     | Lit(literal)
     | Record(fields)
     | List(items)
     | Field(record, name)
     | Index(list, index)
     | Match { scrutinee, arms }
     | Let { name, value, body }
     | Builtin(operation, arguments)
```

`Match` tries arms in source order. Patterns include wildcards, variables, literals, records, lists, and constructors. The authoritative definitions are in [`crates/panproto-expr/src/expr.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/expr.rs).

## Lightweight type classification

The classifier's result type, `ExprType`, has seven cases. We write the possible cases as the type grammar $\tau$:

$$
\tau \;::=\; \mathsf{Int} \mid \mathsf{Float} \mid \mathsf{Str} \mid \mathsf{Bool}
\mid \mathsf{List} \mid \mathsf{Record} \mid \mathsf{Any}.
$$

`List` does not record an element type, `Record` does not record field types, and the classifier has no function type. `Null`, byte values, and closures classify as `Any`.

The function `infer_type(e, env)` classifies an expression under an environment $\Gamma$ from variable names to `ExprType`. Literals, records, lists, variables, and builtins with declared result signatures receive specific cases. Lambdas, applications, field access, and index access return `Any`. A match takes the class of its first arm's body, while a let extends the environment with the class inferred for its bound expression. An unbound variable is an error. The classifier does not inspect a builtin application's argument vector.

`validate_coercion` accepts `Any` because the classifier cannot reject an opaque expression. The classifier also does not apply the evaluator's first-class-builtin fallback to `Var`: a builtin represented as a free variable is unbound for `infer_type` unless an earlier parser or compiler lowered it to `Expr::Builtin` or supplied an environment entry. Thus successful validation means that no detectable mismatch was found. It does not establish a typing judgment of the form $\Gamma \vdash e : \tau$, type preservation, or exhaustiveness.

## Evaluation domain

Values are Rust `Literal`s: integers, floats, strings, booleans, null, bytes, lists, records, and closures. Let $V$ be this set of values and $E$ the set of `ExprError` variants. A configuration sets the maximum number of evaluation steps, recursion depth, and output-list length. The defaults are 100,000 steps, depth 256, and 10,000 list elements. The evaluator has the operational type

$$
\mathsf{eval} : \mathsf{Expr} \times \mathsf{Env} \times \mathsf{Config}
\longrightarrow V + E.
$$

The plus sign denotes a tagged result containing either a value from $V$ or an error from $E$. Resource exhaustion is thus distinct from an unbound variable or invalid builtin argument.

## Call-by-value evaluation

Evaluation is call by value [@plotkin1975call]. Each recursive call checks the depth bound, then consumes one step through `EvalState::tick`. Lists and records evaluate their components from left to right. A let evaluates its bound expression before extending the lexical environment, and an application evaluates the function and argument before applying a captured closure.

Using $\rho[x \mapsto v]$ for environment extension, representative equations are:

$$
\begin{aligned}
\mathsf{eval}(\mathsf{Var}(x), \rho) &= \rho(x), \\
\mathsf{eval}(\mathsf{Lam}(x,e), \rho) &= \mathsf{closure}(x,e,\rho), \\
\mathsf{eval}(\mathsf{Let}(x,e_1,e_2), \rho)
  &= \mathsf{eval}(e_2,\rho[x \mapsto \mathsf{eval}(e_1,\rho)]), \\
\mathsf{eval}(\mathsf{App}(e_1,e_2), \rho)
  &= \mathsf{apply}(\mathsf{eval}(e_1,\rho),\mathsf{eval}(e_2,\rho)).
\end{aligned}
$$

These equations suppress error propagation, the threaded resource state, and the builtin fallback for a variable absent from $\rho$. The code passes one mutable budget through every subevaluation, so sibling expressions do not each receive a fresh copy of the original budget.

Pattern matching evaluates the scrutinee once, tries arms in order, extends the environment with bindings from the first matching pattern, and evaluates that arm. Exhaustion returns `NonExhaustiveMatch`. Higher-order list builtins apply captured closures through the same evaluator.

Builtins are first-class curried values. A free variable whose name denotes a builtin evaluates to the corresponding closure, and an explicit `Builtin` node with too few arguments evaluates to a closure over the remaining arguments. Lexical environment bindings take precedence, so a lambda or let binding can shadow a builtin name. Graph-traversal builtins use a `BuiltinResolver` at every application depth. Pure `eval` returns `NoInstanceContext` when one is reached; the instance-aware entry points provide the resolver that interprets it.

The exported `substitute` operation is capture-avoiding for lambda, let, and match-arm binders. It alpha-renames a binder when a free variable of the replacement would otherwise be captured. `free_vars` applies the same binding rules. These utilities support compiler transformations; the evaluator itself uses closure environments rather than substitution for ordinary function application.

## Checked properties and boundaries

For fixed input and configuration, evaluation follows one call-by-value order. Each recursive invocation checks the depth bound and consumes from the shared step budget; list construction, ranges, `map`, and `flatMap` enforce `max_list_len`. Builtin arity and argument checks run during evaluation, after their arguments have been evaluated.

These checks do not establish type preservation for the language. `infer_type` returns `Any` for several constructs, chooses the first match arm without comparing the others, and does not validate builtin arguments. The resource bounds establish termination of a bounded evaluator invocation, not strong normalization of the language with those bounds removed.

The evaluator lives in [`crates/panproto-expr/src/eval.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/eval.rs), and the classifier lives in [`crates/panproto-expr/src/typecheck.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/typecheck.rs).

## See also

- [Expression-language reference](../../reference/expression-language.md) for the builtin catalog.
- [Apply field transforms](../../how-to/field-transforms.md) for usage.
- [Lens DSL](./lens-dsl.md) for expressions inside lens specifications.
