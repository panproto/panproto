# Totality and termination

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

The language of the previous chapter is a small typed lambda calculus. Nothing in the definition so far prevents a well-typed term from running for a very long time, or from consuming an amount of memory that embarrasses the machine it is running on. For an engine that embeds user-written expressions and runs them against millions of records, that is not acceptable; before a migration touches production data the engine has to know, with some confidence, that every expression in the migration will terminate within a known budget.

The mechanism that provides this confidence is two resource limits: a **step limit** on the number of reduction steps any one evaluation may take, and a **depth limit** on the nesting of active calls. Together, the two limits make every well-typed evaluation terminate, produce a unique outcome, and fit inside a serialisable record that can be replayed across processes. The present chapter explains the limits, the guarantees they purchase, and the trade-offs they impose; the trade-offs against a Turing-complete alternative are the subject of the next chapter.

## The two limits

Every evaluation in [`panproto_expr::eval`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/) runs inside a context that carries a `steps_remaining` counter and a `max_depth` bound. The counter decrements on every reduction step, and if it reaches zero before a value is produced, the evaluator returns a `StepsExhausted` error. The depth bound constrains how deeply lambda applications and list comprehensions can nest at any moment, and an application that would exceed the bound returns a `DepthExceeded` error without reducing further.

Both limits are configurable at the call site. The defaults — a step limit of $10^6$, a depth limit of $256$ — are set in [`panproto_expr::eval::Config`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/struct.Config.html) and are what the migration engine uses when a caller does not specify otherwise. A production caller expecting larger input instances can raise either limit; a caller running user-supplied expressions in an untrusted context can lower them.

The limits are the evaluator's only side-channel into the environment. Everything else is denied: I/O, mutation, network or filesystem access, thread spawning, callbacks into Rust code outside the fixed set of builtins. Every evaluation is a pure, bounded function from the input closure to an outcome.

## Why the limits make the language total

A typed lambda calculus without fixed-point combinators and without general recursion is already total in the sense of @turner2004total: every well-typed term reduces to a value in a finite number of steps. `panproto-expr` has no `let rec`, no `fix`, no self-reference in the term grammar; explicit infinite loops are ruled out by construction.

The limits are there for the implicit blow-ups that still fit inside the typing discipline. A Church-encoded natural number applied to itself can unfold to an exponentially deep term; cleverly constructed beta reductions can produce a term whose size grows polynomially per step. Without the limits, a developer could submit a well-typed term that consumed memory or time far beyond any reasonable budget. With the limits, the evaluator always halts inside the configured budget.

The limits therefore do not make the language total; they bound the total language. A well-typed term reducing to a value in ten steps evaluates to that value whenever the step limit is at least ten. A well-typed term reducing to a value only after ten billion steps never evaluates to that value in practice, regardless of the step limit, and the engine reports `StepsExhausted` at the outermost redex position when the budget runs out.

A reader might object that a language without recursion cannot express many computations a developer would reasonably want. The objection is true and is the trade-off totality imposes. In practice, migration expressions are short — a handful of builtins composed over a few field accesses — and do not need recursion. Where a migration genuinely needs recursion, the developer writes a Rust function and registers it as a foreign builtin, which sits outside `panproto-expr`'s total fragment. Very few migrations we have seen in practice require the escape hatch.

## Deterministic and serialisable

Two further properties follow from the way the evaluator is written, and both matter in production.

The evaluator is *deterministic*: the same closed term evaluated under the same limits always produces the same outcome. No builtin has hidden randomness, no reduction step depends on global state, and the fixed evaluation order gives every non-value term exactly one next step. Two machines running the same evaluation produce identical traces, up to the bit representation of intermediate values. Determinism is what lets the engine cache intermediate results — a value computed in one run of a migration can be reused by a subsequent run against the same input.

The evaluator is *serialisable*: the state of an ongoing evaluation — the current term, the configuration, the counters, the environment — is a [serde](https://serde.rs/)-encoded record that can be checkpointed, transmitted across a process boundary, and resumed. The serialisation format lives in [`panproto_expr::eval::State`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/struct.State.html). Serialisability is what lets the engine distribute work across workers: a migration across a million records splits into chunks, each worker handles a chunk, and intermediate states ship between workers without breaking semantics.

## How the limits interact with the term forms

A reduction step consumes exactly one unit of budget regardless of the term form. Beta reduction, let reduction, record projection, list element access, pattern match, and builtin application are each a single step. The uniform cost is a design choice: a caller wanting to reason about worst-case budget can multiply the step limit by the maximum work a single step of any term form might do, and every builtin's single-step work is documented in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

The depth limit is checked on lambda application and on nested list-comprehension entry. An application of a lambda that is itself the body of another lambda counts as nested; a list comprehension inside another list comprehension counts as nested iteration. Both counts are local to the current evaluation frame, and both are released when the evaluator returns from the inner scope.

Pattern matching and record projection do not count against the depth limit, because neither introduces a new scope. A pattern match that reduces to a body expression replaces the entire match by that body in a single step, and the body is evaluated in the outer frame.

## What happens when a limit is exceeded

A `StepsExhausted` error carries the outermost redex position as a source-location span together with the remaining state at the moment of exhaustion. The migration engine receiving this error reports the specific field transform whose evaluation ran out of budget, and suggests either raising the limit in the caller's configuration or rewriting the transform.

A `DepthExceeded` error reports the lambda or list comprehension whose entry would have exceeded the bound. The report is a source-location span and the depth at the time of attempted entry. The common cause is an unintentionally recursive pattern expressed through Church-style encoding, and the error message suggests rewriting with a linear combinator — `map`, `foldl`, `foldr` — that the engine can evaluate without nested recursion. The general recursion-scheme framework those combinators are the simplest cases of is developed in @meijer1991bananas and @birddemoor1997algebra; a developer whose expression keeps tripping the depth limit through implicit recursion will benefit from reading either source.

Both errors are fully inspectable in Rust code through [`panproto_expr::error::EvalError`](https://docs.rs/panproto-expr/latest/panproto_expr/error/enum.EvalError.html). A caller that wants to retry with a larger budget can do so; a caller that wants to surface the error to a user can format it with the pre-built diagnostic renderer.

## Further reading

@turner2004total is the foundational argument for total functional programming, and the most direct justification of the design choices in this chapter. For the recursion-scheme framework that `map`, `foldl`, and `foldr` are the simplest cases of, @meijer1991bananas is the foundational paper and @birddemoor1997algebra the book-length treatment.

## Closing

The next chapter situates the totality guarantees developed here against the main alternatives — Starlark, Dhall, Nickel, CUE — and explains why the engine needs this particular combination rather than one of the nearby options.
