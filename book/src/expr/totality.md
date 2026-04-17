# Totality and termination

The language of [Syntax and semantics](./syntax-semantics.md) gains the properties panproto requires of it only when the evaluator enforces two resource limits: a **step limit** on the number of reduction steps any one evaluation may take, and a **depth limit** on the nesting of active calls. Together the two limits make every well-typed evaluation terminate, produce a unique outcome, and fit inside a serializable record that can be replayed across processes. This chapter explains the limits, the guarantees they buy, and the trade-offs they impose.

The trade-offs against a Turing-complete language are left to the next chapter, [Why bounded pure evaluation](./design-choices.md); this chapter covers the formal guarantees the limits provide.

## The two limits

Every evaluation in [`panproto_expr::eval`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/) runs inside an evaluation context that carries a `steps_remaining` counter and a `max_depth` bound. The counter decrements on every reduction step; if it reaches zero before a value is produced, the evaluator returns a `StepsExhausted` error. The depth bound constrains how deeply lambda applications and list comprehensions can nest at any moment; an application that would exceed the bound returns a `DepthExceeded` error without reducing.

Both limits are configurable at the call site. Reasonable defaults (a step limit of $10^6$, a depth limit of 256) are set in [`panproto_expr::eval::Config`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/struct.Config.html) and are what the migration engine uses when it evaluates a field transform on behalf of a user who has not specified otherwise. A production caller who anticipates larger input instances can raise either limit; a caller running user-supplied expressions in an untrusted context can lower them.

The limits are the evaluator's only side-channel into the environment. The language itself is denied everything else: I/O, mutation, network or filesystem access, thread spawning, and any form of callback into Rust code outside the fixed set of builtins. Every evaluation is a pure, bounded function from the input closure to an outcome.

## Why the limits make the language total

A typed lambda calculus without fixed-point combinators and without general recursion is already total in the sense of @turner2004total: every well-typed term reduces to a value in a finite number of steps. [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) has no `let rec`, no `fix`, and no self-reference in the term grammar, which rules out explicit infinite loops. The limits guard against implicit blow-ups that still fit within the typing discipline, such as an exponentially deep unfolding through repeated function application or a Church-encoded natural number applied to itself. Without the limits a developer could submit a term that was well-typed but that would consume memory or time far beyond what the engine is willing to spend. With the limits the evaluator always halts within the configured budget.

The limits therefore do not make the language total; they bound the total language. A well-typed term that would naturally reduce to a value in ten steps evaluates to that value whenever the step limit is at least ten. A well-typed term that would reduce to a value only after ten billion steps never evaluates to that value in practice, regardless of the step limit, and the engine reports a `StepsExhausted` error that names the term at the outermost redex position when the budget was reached.

## Deterministic and serializable

Two further properties follow from the way the evaluator is written.

The evaluator is **deterministic**: the same closed term evaluated under the same limits always produces the same outcome. No builtin has hidden randomness, no reduction step depends on a global state, and the left-to-right outermost-first reduction order gives every non-value term exactly one next step. Two machines running the same evaluation produce identical traces, up to the bit representation of the intermediate values.

The evaluator is **serializable**: the state of an ongoing evaluation (the current term plus the configuration plus the counters plus the environment of bindings) is a [serde](https://serde.rs/)-encoded record that can be checkpointed, transmitted across a process boundary, and resumed. The serialization format is covered in the [`panproto_expr::eval::State`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/struct.State.html) type. This property is what lets panproto's migration engine run expensive field transforms in parallel batches: an evaluation partway through a large list comprehension can be paused, serialised, and resumed elsewhere with the exact same semantics.

## How the limits interact with the term forms

A reduction step consumes exactly one step of budget, regardless of the term form. Beta reduction, let reduction, record projection, list element access, pattern match, and builtin application are each a single step. This uniform cost is a design choice: a caller who wants to reason about worst-case budget can multiply the step limit by the maximum work a single step of any term form might do, and every builtin's single-step work is documented in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

The depth limit is checked on lambda application and on nested list comprehension entry. An application of a lambda that is itself the body of another lambda counts as a nested call; a list comprehension inside another list comprehension counts as a nested iteration. Both counts are local to the current evaluation frame, and both are released when the evaluator returns from the inner scope.

Pattern matching and record projection do not count against the depth limit, since neither introduces a new scope. A pattern match that reduces to a body expression replaces the entire match by that body in a single step, and the body's evaluation happens in the outer frame.

## What happens when a limit is exceeded

A `StepsExhausted` error carries the outermost redex position as a source-location span and the remaining state at the time of exhaustion. A migration engine that receives this error reports the specific field transform whose evaluation ran out of budget, together with a suggestion to raise the limit in the caller's configuration or to rewrite the transform.

A `DepthExceeded` error reports the lambda or list comprehension whose entry would have exceeded the bound. The report is a source-location span plus the depth at the time of attempted entry. The common cause is an unintentionally recursive pattern expressed through Church-style encoding, and the error message suggests rewriting with a linear combinator (`map`, `foldl`, `foldr`) that the engine can evaluate without nested recursion. The general recursion-scheme framework those combinators are the basic cases of is developed in @meijer1991bananas and @birddemoor1997algebra.

Both errors are fully inspectable in Rust code through [`panproto_expr::error::EvalError`](https://docs.rs/panproto-expr/latest/panproto_expr/error/enum.EvalError.html). A caller that wants to retry with a larger budget may do so, and a caller that wants to surface the error to the user may format it with a pre-built diagnostic renderer.

## Closing

The next chapter ([Why bounded pure evaluation](./design-choices.md)) situates the language's totality guarantees against the main alternatives (Starlark, Dhall, Nickel, CUE), and explains why panproto's migration engine needs this particular combination of purity, totality, determinism, and serialisability rather than one of the nearby options.

<!--
STATUS: Totality chapter drafted.

CITATIONS:
  - Pierce 2002 "Types and Programming Languages" (pending).
  - Turner 2004 "Total Functional Programming" on the totality
    argument (pending).
-->
