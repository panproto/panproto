# Expression language: denotational semantics

## In plain terms

The expression language `panproto-expr` is what you use to describe field-level transforms inside a migration ("the new `full_name` field is the old `first` plus a space plus the old `last`") and predicates inside a query ("only records where `created_at > '2024-01-01'`"). Everything in the language is pure: no IO, no mutation, no clock reads, no random numbers. Two expressions that look the same always do the same thing. Every expression terminates within a fixed number of evaluation steps.

This page describes what those properties mean and what the evaluator actually computes.

## Surface syntax

The Haskell-style surface, parsed by `panproto-expr-parser`:

```bnf
expr  ::= literal
        | ident
        | expr expr                    -- application
        | "\\" ident "->" expr         -- lambda
        | "let" ident "=" expr "in" expr
        | "if" expr "then" expr "else" expr
        | "case" expr "of" alts        -- pattern match
        | builtin "(" expr,... ")"     -- builtin application
        | expr "." ident               -- field access
        | expr "[" expr "]"            -- index
literal ::= int | float | str | bool | "null"
        | "[" expr,... "]"             -- list
        | "{" ident ":" expr,... "}"   -- record
```

The parser desugars `if c then a else b` to a `Match` over a boolean scrutinee with two arms, and `case e of ...` to a `Match` directly. There is no separate `If` or `Case` node in the abstract syntax.

## Abstract syntax

```rust,ignore
pub enum Expr {
    Lit(Literal),
    Var(Arc<str>),
    App(Box<Expr>, Box<Expr>),
    Lam(Arc<str>, Box<Expr>),
    Let(Arc<str>, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<MatchArm>),
    Builtin(BuiltinOp, Vec<Expr>),
    List(Vec<Expr>),
    Field(Box<Expr>, Arc<str>),
    Index(Box<Expr>, Box<Expr>),
}
```

`Match` covers both `if/then/else` and `case/of`; pattern matching is the only branching primitive. The full enum lives at [`crates/panproto-expr/src/expr.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/expr.rs).

## Type system

The type-formation grammar:

$$
\tau \;::=\; \mathsf{Int} \mid \mathsf{Float} \mid \mathsf{Str} \mid \mathsf{Bool} \mid \mathsf{Null} \mid \mathsf{Any} \mid \mathsf{List}\,\tau \mid \mathsf{Record} \mid \tau \to \tau
$$

A typing context $\Gamma$ is a finite map from variable names to types. The typing relation $\Gamma \vdash e : \tau$ is defined inductively by the usual rules; selected:

$$
\frac{}{\Gamma \vdash n : \mathsf{Int}} \;(\text{T-Int})
\qquad
\frac{x : \tau \in \Gamma}{\Gamma \vdash x : \tau} \;(\text{T-Var})
\qquad
\frac{\Gamma, x : \tau_1 \vdash e : \tau_2}{\Gamma \vdash \lambda x.\,e : \tau_1 \to \tau_2} \;(\text{T-Lam})
$$

$$
\frac{\Gamma \vdash e_1 : \tau_1 \to \tau_2 \quad \Gamma \vdash e_2 : \tau_1}{\Gamma \vdash e_1\,e_2 : \tau_2} \;(\text{T-App})
\qquad
\frac{\Gamma \vdash e : \tau_1 \quad \Gamma, x : \tau_1 \vdash e' : \tau_2}{\Gamma \vdash \mathsf{let}\;x = e\;\mathsf{in}\;e' : \tau_2} \;(\text{T-Let})
$$

Builtin signatures have type schemes given in [reference/expression-language](../../reference/expression-language.md); each `Builtin(op, \overline{e})` rule plugs in $op$'s scheme and checks that the arguments match.

## Semantic domain

Let $\mathsf{Val}$ be the recursive sum

$$
\mathsf{Val} \;\cong\; \mathbb{Z} + \mathbb{R} + \mathsf{String} + \mathbb{B} + \{\star\} + \mathsf{List}(\mathsf{Val}) + \mathsf{Record}(\mathsf{Val}) + [\mathsf{Val} \rightharpoonup \mathsf{Val}]
$$

interpreting `Null` as the singleton $\{\star\}$ and the function space as partial continuous maps. Lift to $\mathsf{Val}_\bot = \mathsf{Val} + \{\bot\}$ to adjoin a bottom for divergence under the step budget. Environments live in $\mathsf{Env} = \mathsf{Var} \rightharpoonup \mathsf{Val}$, and *ranked* environments in $\mathsf{Env}_n = \mathsf{Env} \times \mathbb{N}$ to track the remaining step budget.

## Semantic function

The denotational semantics is the family

$$
\llbracket \cdot \rrbracket : \mathsf{Expr} \to \mathsf{Env}_n \to \mathsf{Val}_\bot
$$

defined by structural recursion on $\mathsf{Expr}$. Write $\rho_n = (\rho, n)$ and $\rho_n \!\downarrow\! 1 = (\rho, n - 1)$ for the budget decrement. The equations:

$$
\begin{aligned}
\llbracket \mathsf{Lit}(c) \rrbracket\, \rho_n        &= c \\
\llbracket \mathsf{Var}(x) \rrbracket\, \rho_n        &= \rho(x) \\
\llbracket \mathsf{Lam}(x, e) \rrbracket\, \rho_n     &= \lambda v.\, \llbracket e \rrbracket\,(\rho[x \mapsto v])_n \\
\llbracket \mathsf{Let}(x, e_1, e_2) \rrbracket\, \rho_n
                                                       &= \llbracket e_2 \rrbracket\,(\rho[x \mapsto \llbracket e_1 \rrbracket\,\rho_n])_{n-1} \\
\llbracket \mathsf{App}(e_1, e_2) \rrbracket\, \rho_n
                                                       &= (\llbracket e_1 \rrbracket\,\rho_n)\,(\llbracket e_2 \rrbracket\,\rho_n) \\
\llbracket \mathsf{Match}(e, \overline{(p_i, b_i)}) \rrbracket\, \rho_n
                                                       &= \mathsf{matchArms}(\llbracket e \rrbracket\,\rho_n,\ \overline{(p_i, b_i)},\ \rho_{n-1}) \\
\llbracket \mathsf{List}(\overline{e}) \rrbracket\, \rho_n
                                                       &= [\,\llbracket e_i \rrbracket\,\rho_n\,]_i \\
\llbracket \mathsf{Field}(e, x) \rrbracket\, \rho_n
                                                       &= (\llbracket e \rrbracket\,\rho_n).x \\
\llbracket \mathsf{Index}(e, i) \rrbracket\, \rho_n
                                                       &= (\llbracket e \rrbracket\,\rho_n)\,[\,\llbracket i \rrbracket\,\rho_n\,] \\
\llbracket \mathsf{Builtin}(op, \overline{e}) \rrbracket\, \rho_n
                                                       &= \mathsf{apply\_builtin}(op,\ \overline{\llbracket e_i \rrbracket\,\rho_n}) \\
\llbracket e \rrbracket\, (\rho, 0)                   &= \bot \quad \text{(budget rule)}
\end{aligned}
$$

The budget rule fires before any equation if the remaining steps are zero; otherwise the relevant equation applies and the recursive sub-denotations are evaluated with the budget decremented. Operationally this is `EvalState::tick` in [`crates/panproto-expr/src/eval.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/eval.rs); when $\bot$ is returned the implementation surfaces `ExprError::StepLimitExceeded(max_steps)`.

The auxiliary $\mathsf{matchArms}$ is the standard pattern-match search: try each $(p_i, b_i)$ in order, attempting to unify $p_i$ against the scrutinee value; on the first success bind the pattern variables into $\rho$ and evaluate $b_i$; on exhaustion raise `NonExhaustiveMatch`. The auxiliary $\mathsf{apply\_builtin}$ is the partial function defined in [`crates/panproto-expr/src/builtin.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/builtin.rs).

### Builtin side conditions

The builtins listed under [reference/expression-language](../../reference/expression-language.md) are individually total or partial. The partial ones return a non-$\bot$ error rather than $\bot$:

- `Div`, `Mod` with zero divisor: `DivisionByZero`.
- Integer arithmetic overflow: `Overflow` (`i64::checked_*`).
- `*ToInt` / `*ToFloat` on unparseable input: `ParseError`.
- List index out of bounds: `IndexOutOfBounds`; list operations past the configured maximum: `ListLengthExceeded`.
- Record access on a missing field: `FieldNotFound`.
- `Match` exhaustion: `NonExhaustiveMatch`.
- `App` of a non-function value: `NotAFunction`.

Errors are distinct from $\bot$. $\bot$ models *resource exhaustion* (`StepLimitExceeded` and `DepthExceeded`); errors model *defined failure* and propagate as `Err(ExprError)` from the implementation.

## Soundness

The evaluator satisfies:

- **Type preservation.** If $\Gamma \vdash e : \tau$ and $\rho \models \Gamma$ and $\rho \vdash e \Downarrow v$ with $v \neq \bot$, then $v \in \llbracket \tau \rrbracket$.
- **Totality (within the budget).** For every well-typed $e$ and well-typed $\rho$, $\llbracket e \rrbracket_\rho$ terminates in finitely many steps with either a value or $\bot$.
- **Determinism.** If $\rho \vdash e \Downarrow v_1$ and $\rho \vdash e \Downarrow v_2$ then $v_1 = v_2$.

Type preservation is enforced by the type-checker in [`crates/panproto-expr/src/typecheck.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/typecheck.rs) and by builtin signatures rejecting ill-typed arguments. Totality follows from the budget rule. Determinism follows from the absence of mutation and IO.

## What is intentionally not modelled

- **Performance.** Two expressions can have the same denotation but very different cost. The semantics fixes only what is computed, not how much it costs.
- **Step-budget tuning.** The budget is a parameter set at the outermost call. The semantics treats it as fixed; the language itself does not expose it.
- **Floating-point determinism across architectures.** `Float` operations follow IEEE 754, but bit-level reproducibility across hardware is not guaranteed.

## See also

- [Reference: expression-language](../../reference/expression-language.md) for the builtin catalogue.
- [How-to: apply field transforms](../../how-to/field-transforms.md) for usage.
- [Lens DSL](./lens-dsl.md) for how `panproto-expr` appears inside lens specifications.
