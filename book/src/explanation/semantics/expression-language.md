# Expression language: operational meaning

## In plain terms

We use `panproto-expr` for pure value computations in field transforms and queries. For a fixed expression, environment, instance context, and resource configuration, evaluation is deterministic and performs no mutation or external input/output. It stops with an `ExprError` when a step, recursion-depth, or list-length limit is reached. The Haskell-style surface language lowers to an `Expr` abstract syntax tree; a lightweight classifier catches some mismatches but is not a proof system for the full language.

## Surface syntax

The following grammar is a representative fragment, not the complete parser grammar:

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

The optional expression in a record field permits punning: `{x}` means `{x = x}`. The full parser also handles operators, ranges, list comprehensions, `do` notation, and `where` clauses. It lowers those forms to the smaller abstract syntax described below. In particular, conditionals lower to `Match`; there is no `If` variant in `Expr`.

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

`ExprType` has seven cases:

$$
\tau \;::=\; \mathsf{Int} \mid \mathsf{Float} \mid \mathsf{Str} \mid \mathsf{Bool}
\mid \mathsf{List} \mid \mathsf{Record} \mid \mathsf{Any}.
$$

`List` does not record an element type, `Record` does not record field types, and the classifier has no function type. `Null`, byte values, and closures classify as `Any`.

The function `infer_type(e, env)` performs best-effort classification under an environment $\Gamma$ from variable names to `ExprType`. Literals, records, lists, variables, and builtins with declared result signatures receive specific cases. Lambdas, applications, field access, and index access return `Any`; a match uses the first arm's body; and a let extends the environment with the inferred class of its bound value. An unbound variable is an error.

`validate_coercion` accepts `Any` because the classifier cannot reject an opaque expression. Thus successful validation means that no detectable mismatch was found. It does not establish a typing judgment of the form $\Gamma \vdash e : \tau$, type preservation, or exhaustiveness.

## Evaluation domain

Values are Rust `Literal`s: integers, floats, strings, booleans, null, bytes, lists, records, and closures. Write $V$ for this set and $E$ for `ExprError`. A configuration contains maximum steps, maximum recursion depth, and maximum output-list length. The evaluator can be modeled as

$$
\mathsf{eval} : \mathsf{Expr} \times \mathsf{Env} \times \mathsf{Config}
\longrightarrow V + E.
$$

This error sum is more faithful to the implementation than a bottom element $\bot$: resource exhaustion is reported as a concrete `ExprError`, just like an unbound variable or an invalid builtin argument.

## Call-by-value evaluation

Evaluation is call by value. Each recursive call checks the depth bound, then consumes one step through `EvalState::tick`. Lists and records evaluate their components from left to right. A let evaluates its bound expression before extending the lexical environment, and an application evaluates the function and argument before applying a captured closure.

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

These equations suppress error propagation and the threaded resource state. The code passes one mutable budget through every subevaluation, so sibling expressions do not each receive a fresh copy of the original budget.

Pattern matching evaluates the scrutinee once, tries arms in order, extends the environment with bindings from the first matching pattern, and evaluates that arm. Exhaustion returns `NonExhaustiveMatch`. Higher-order list builtins apply captured closures through the same evaluator, while graph-traversal builtins require the instance-aware evaluation entry point.

## Checked properties and boundaries

For fixed input and configuration, evaluation follows one call-by-value order and returns before the configured step or depth budget is exceeded; list-producing operations also enforce `max_list_len`. `infer_type` rejects some invalid expressions before evaluation, and it is the only check that runs then, since it ignores a builtin's argument vector entirely. The builtin arity and argument checks run *during* evaluation, inside `apply_builtin`, after the arguments have been evaluated.

It does not currently support a general type-preservation claim. `infer_type` is deliberately best effort, returns `Any` for several constructs, and does not validate every subexpression of a builtin application. Likewise, the resource bounds establish termination of the evaluator invocation, not termination of an unbounded calculus obtained by removing those checks.

The evaluator lives in [`crates/panproto-expr/src/eval.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/eval.rs), and the classifier lives in [`crates/panproto-expr/src/typecheck.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/typecheck.rs).

## See also

- [Expression-language reference](../../reference/expression-language.md) for the builtin catalog.
- [Apply field transforms](../../how-to/field-transforms.md) for usage.
- [Lens DSL](./lens-dsl.md) for expressions inside lens specifications.
