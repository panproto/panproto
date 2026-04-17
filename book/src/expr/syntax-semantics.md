# Syntax and semantics

Every migration in panproto, as developed in [Theory morphisms and instance migration](../core/morphisms-and-migration.md), carries a theory-morphism component and a pushforward choice at each extension site. The pushforward choice is where panproto needs a small programming language. An expression language needs to be able to take the values visible at a site of the source instance and compute the value required at the target: a default, a computed field, a transformation of an existing value. Part III is about that language.

The language, called [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/), is a typed lambda calculus extended with records, lists, pattern matching, and around fifty builtin operations on strings, numbers, collections, and panproto schemas. It is pure (no I/O, no mutation, no side channels), total (every evaluation terminates within a configurable budget), deterministic (the same expression on the same input always produces the same output), and serializable (an evaluation in progress can be suspended, sent across a process boundary, and resumed). Each of those properties is load-bearing for how the migration engine uses it, and why each one matters is the subject of [Totality and termination](./totality.md) and [Why bounded pure evaluation](./design-choices.md).

The surface syntax is Haskell-flavoured. Field access, record literals, list comprehensions, and composition all read like Haskell to the reader who has worked through Part I. This choice is not a marketing decision; it is a practical one. A migration expression is usually short (a handful of operators combining values from the source record), and the Haskell surface is the most compact idiom the authors know that still reads as code to a reader who has never seen the engine before.

This chapter covers:

- the grammar of the language, in BNF with annotations for unfamiliar constructs
- the type system (base types, function types, records, lists, and the two panproto-native types)
- the small-step operational semantics that the evaluator implements
- the catalogue of builtins in four groups: arithmetic and comparison, string, list, and panproto-native

A reader who already knows Haskell can read this chapter quickly as a reference for how panproto's language differs from Haskell's. A reader who has not seen Haskell before will find the chapter slower going but self-contained; every construct is glossed on first sight.

## Grammar

A term of the language is one of the following, where variables range over an infinite set of names.

```
e ::= x                            -- variable
    | n | s | b                    -- literal (number, string, boolean)
    | \x -> e                      -- lambda abstraction
    | e1 e2                        -- application
    | e1 . e2                      -- composition
    | let x = e1 in e2             -- let binding
    | { l1 = e1, ..., ln = en }    -- record literal
    | e.l                          -- field access
    | [e1, ..., en]                -- list literal
    | [e1 | x <- e2]               -- list comprehension
    | case e of p1 -> e1; ...      -- pattern match
    | builtin args                 -- builtin operation
```

*Listing 5.1: The grammar of [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/). The surface syntax accepted by the parser in [`panproto-expr-parser`](https://docs.rs/panproto-expr-parser/latest/panproto_expr_parser/) adds syntactic sugar for common patterns, but every sugared form desugars into one of the term forms above.*

The only term forms that warrant comment are the record and list notations and the pattern language. Record literals write field labels as `{ l = e }` and read them back with a dotted projection `e.l`. A list comprehension `[e1 | x <- e2]` denotes the list obtained by evaluating `e1` for each `x` drawn from `e2`, and extends to multiple generators and filters in the usual way. Numeric literals parse as signed 64-bit integers when they have no fractional part and as 64-bit floats otherwise; string literals are UTF-8.

Pattern matching inherits its syntax from Haskell, restricted to the panproto types: a pattern is a variable (matches anything and binds it), a wildcard `_` (matches anything without binding), a literal (matches the corresponding value), a record pattern (matches records with the named fields), or a list pattern of the form `[x1, ..., xn]` or `x : xs`.

The tradition this surface belongs to is the ISWIM family of lambda-calculus-based expression languages traced to @landin1966next. The Haskell readability of the syntax is load-bearing: a migration author who knows Haskell reads a panproto expression directly, with no syntactic translation between languages.

The grammar in Listing 5.1 is the concrete syntax parsed by [`panproto_expr_parser`](https://docs.rs/panproto-expr-parser/latest/panproto_expr_parser/). The abstract-syntax representation, used everywhere else in the engine, is the [`Expr`](https://docs.rs/panproto-expr/latest/panproto_expr/expr/enum.Expr.html) enum in [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/).

## Types

Every term has a type. The type system is:

```
T ::= Int | Float | String | Bool       -- base types
    | T1 -> T2                          -- function type
    | { l1 : T1, ..., ln : Tn }         -- record type
    | [T]                               -- list type
    | Schema P | Instance P             -- panproto-native types
```

*Listing 5.2: The type language. The two panproto-native types `Schema P` and `Instance P` are indexed by the protocol they belong to.*

Four base types cover the scalars panproto expressions commonly manipulate. Function types are the usual arrow. Record types are structural: a record type is determined by its field labels and their types, and two record types with the same fields (in any order) are equal. List types are homogeneous: `[Int]` and `[String]` are different types, and a list literal cannot mix them. The two panproto-native types are opaque inside the language — an expression cannot inspect the internal structure of a `Schema` or an `Instance` — but they are passed to builtins that know how to manipulate them.

The type system sits in the polymorphic lineage traced to @girard1972interpretation's System F, via the principal-type-scheme theorem of @hindley1969principal and the Hindley-Milner fragment @milner1978theory and @damasmilner1982principal give a type-inference algorithm for. Panproto does not implement full System F; the Hindley-Milner restriction is enough for migration expressions and keeps type inference decidable and fast.

Type-checking is implemented in [`panproto_expr::typecheck`](https://docs.rs/panproto-expr/latest/panproto_expr/typecheck/). The rules follow a standard bidirectional-style elaboration: checking modes propagate an expected type inward from a context, while synthesis modes infer a type from the term's shape and push it outward. A comprehensive modern reference for the relevant typing and evaluation theory is @harper2016practical; @pierce2002types is the standard pedagogical source.

Every well-typed term has a principal type the checker produces. A term that fails to check carries an error message naming the subterm at fault; the error messages are produced by the diagnostics infrastructure in [`panproto_expr::error`](https://docs.rs/panproto-expr/latest/panproto_expr/error/). Good error messages are not an afterthought here — the expression language is a DSL a working developer writes under time pressure, and a diagnostic that points at the wrong subterm is as bad as no diagnostic at all.

## Operational semantics

The evaluator in [`panproto_expr::eval`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/) implements a small-step reduction relation $\longrightarrow$ on closed terms. The formulation is the one of @plotkin1975call: a single-step relation on closed terms, whose reflexive transitive closure is evaluation.

A term is a *value* when it matches one of the value forms (a literal, a record of values, a list of values, or a closure). The reduction relation is empty on values; a value does not reduce. Every non-value term has exactly one reduction step, chosen by a left-to-right, outermost-first evaluation order. This last choice is deliberate: left-to-right outermost-first is the eager-reduction strategy that makes evaluations serializable (every intermediate state is a closed term) and predictable (the same input produces the same reduction sequence on every machine).

The defining rules are the standard ones. Beta reduction:

$$(\lambda x.\, e)\; v \;\longrightarrow\; e[v / x]$$

for a value $v$ substituted for the bound variable. Let reduction:

$$(\mathtt{let}\; x = v \; \mathtt{in}\; e) \;\longrightarrow\; e[v / x].$$

Record projection:

$$\{ \ldots, l = v, \ldots \}.l \;\longrightarrow\; v.$$

Pattern-match reduction reduces to the body of the first clause whose pattern matches the scrutinee, with the pattern's bound variables substituted.

Reduction of a builtin applied to values is whatever the builtin's specification says: `add` on two integers returns their sum, and the same pattern applies to every other arithmetic, string, list, and instance-level operation. Each builtin has a reduction rule in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

Reduction is deterministic. The evaluation order is left-to-right and outermost-first; every term has at most one reduction step available. Running the evaluator on a closed term produces a unique trace of reductions, which ends at either a value (success) or a bounded-resource failure (the subject of [Totality and termination](./totality.md)).

## Builtins

The language ships around fifty builtins in four groups.

### Arithmetic and comparison

`add`, `sub`, `mul`, `div`, `mod`, `neg`, `abs`, `min`, `max`, `eq`, `lt`, `leq`, `gt`, `geq`. Each has the standard mathematical meaning on `Int` and `Float` operands. Division and modulus on integers follow the Rust standard-library convention (truncation toward zero).

### String operations

`concat`, `length` (on strings), `toLower`, `toUpper`, `trim`, `split`, `replace`, `startsWith`, `endsWith`, `contains`, `format`. All are UTF-8-aware and treat strings as sequences of Unicode scalar values. A reader who expects byte-level semantics should be aware that string indexing and length report Unicode scalar counts, not byte counts; for byte-level work, instances are represented separately as `[Int]` or equivalent.

### List operations

`map`, `filter`, `foldl`, `foldr`, `length` (on lists), `head`, `tail`, `reverse`, `sort`, `zip`, `unzip`, `concat` (on lists), `take`, `drop`. The combinators have their standard Haskell meanings; the partiality of `head` and `tail` on empty lists is treated as a bounded-resource failure rather than an exception.

### Panproto-native operations

`instanceOf` (tests whether an instance is of a given schema), `getField`, `setField`, `listSchemasUnder` (enumerates the schemas under a protocol), `applyMigration` (applies a compiled migration to an instance), `renameField`, `requireField`. These are what migrations and field transforms use to manipulate panproto values; they are documented individually in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

Every builtin is total on well-typed inputs, modulo the bounded-resource failures covered in the next chapter. A builtin that would otherwise fail on bad input fails at the type-check stage with a diagnostic identifying the specific argument. The combination — builtins total on well-typed input plus a type-checker that catches ill-typed use before evaluation — is what lets the engine treat the language as a pure total function from closed well-typed terms to values.

## Further reading

For the operational-semantics side, @plotkin1975call's technical report on call-by-value and call-by-name (later republished in *Theoretical Computer Science* 1:125–159) is the foundational reference. @pierce2002types is the standard textbook treatment of small-step semantics and is the right place to start for a reader who wants the theory of the evaluator worked out in full.

For the type-system side, @hindley1969principal is the original principal-type-scheme theorem; @milner1978theory and @damasmilner1982principal developed the algorithm panproto's type-checker is a variant of. @harper2016practical covers the whole area — operational semantics, type systems, subtyping, polymorphism — in a single modern reference.

For the ISWIM lineage the surface syntax sits in, @landin1966next is the foundational source. A reader who wants to see how ISWIM influenced modern functional-language design should work through the early chapters of @harper2016practical alongside Landin's original paper.

## Closing

The next chapter, [Totality and termination](./totality.md), introduces the step and depth limits the evaluator enforces, explains what the limits buy (terminating evaluation, unique outcomes, serializable intermediate states), and names the trade-offs the limits impose. Those trade-offs, and the reasons for accepting them rather than adopting a larger existing language, are the subject of the chapter after that, [Why bounded pure evaluation](./design-choices.md).
