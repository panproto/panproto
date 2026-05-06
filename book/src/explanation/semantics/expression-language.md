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

```rust
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

## Types

```text
τ ::= Int | Float | Str | Bool | Null | Any | List τ | Record | τ → τ
```

Type-checking judgement: $\Gamma \vdash e : \tau$.

Selected rules:

$$
\frac{}{\Gamma \vdash n : \mathsf{Int}} \quad (\text{t-int})
\qquad
\frac{\Gamma \vdash e_1 : \tau_1 \to \tau_2 \quad \Gamma \vdash e_2 : \tau_1}{\Gamma \vdash e_1\,e_2 : \tau_2} \quad (\text{t-app})
$$

$$
\frac{\Gamma, x : \tau_1 \vdash e : \tau_2}{\Gamma \vdash \lambda x.\,e : \tau_1 \to \tau_2} \quad (\text{t-lam})
$$

Builtin signatures are tabulated in [`reference/expression-language`](../../reference/expression-language.md).

## Semantic domain

The value domain is

$$
V = \mathbb{Z} \cup \mathbb{R} \cup \mathsf{String} \cup \{\mathsf{true}, \mathsf{false}\} \cup \{\mathsf{null}\} \cup \mathsf{List}(V) \cup \mathsf{Record}(V) \cup (V \to V)
$$

with $V_\bot = V \cup \{\bot\}$ adjoined for non-terminating or budget-exceeded computations.

## Interpretation

The evaluation judgement is $\rho \vdash e \Downarrow v$, parameterised over a step counter $n \in \mathbb{N}$. We elide the counter unless it is the point.

$$
\frac{}{\rho \vdash n \Downarrow n} \quad (\text{e-int})
\qquad
\frac{x \in \rho}{\rho \vdash x \Downarrow \rho(x)} \quad (\text{e-var})
$$

$$
\frac{\rho \vdash e_1 \Downarrow \lambda x.\,e \quad \rho \vdash e_2 \Downarrow v_2 \quad \rho, x \mapsto v_2 \vdash e \Downarrow v}{\rho \vdash e_1\,e_2 \Downarrow v} \quad (\text{e-app})
$$

The step counter is held in `EvalState` (see `crates/panproto-expr/src/eval.rs`). Each rule application calls `EvalState::tick`, which decrements the remaining budget and raises when zero. This corresponds to the rule

$$
\frac{n = 0}{\rho \vdash e \Downarrow_n \bot} \quad (\text{e-budget})
$$

When this rule fires, evaluation aborts with `ExprError::StepLimitExceeded(max_steps)`. We model this as $\bot$ in the semantic domain so that $\llbracket e \rrbracket_\rho \in V_\bot$ is defined for every well-typed $e$. The default budget is $100{,}000$ steps (`EvalConfig::default`).

Builtins are interpreted by `apply_builtin`, defined in [`crates/panproto-expr/src/builtin.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/builtin.rs). Side conditions:

- `Div` and `Mod`: divisor zero raises `DivisionByZero`.
- Integer arithmetic: overflow raises `Overflow` (we use `i64::checked_*`).
- `*ToInt` / `*ToFloat`: invalid input raises `ParseError`.
- List builtins: out-of-bounds access raises `IndexOutOfBounds`; lists past the configured maximum length raise `ListLengthExceeded`.
- Record access: missing field raises `FieldNotFound`.
- `Match`: a non-exhaustive match raises `NonExhaustiveMatch`.
- Application of a non-function value raises `NotAFunction`.

These are runtime errors distinct from $\bot$; $\bot$ is reserved for resource exhaustion (`StepLimitExceeded` and `DepthExceeded`).

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
