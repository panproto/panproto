# Syntax and semantics

Every migration in panproto, as developed in [Theory morphisms and instance migration](../core/morphisms-and-migration.md), carries a theory-morphism component and a pushforward choice at each extension site. The pushforward choice is where panproto needs a small programming language: an expression that, given the values visible at a site of the source instance, produces the value required at the target. This chapter introduces the language the engine uses there, [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/), and lays out its syntax and operational semantics.

The language is a typed lambda calculus extended with records, lists, pattern matching, and around fifty builtin operations on strings, numbers, collections, and panproto schemas. It is pure, total, and serializable. The surface syntax is Haskell-flavoured: field access, record literals, list comprehensions, and composition all read like Haskell. A reader comfortable with the [Haskell presentations](../foundations/categories.md) of the foundations chapters is already fluent in most of what this chapter defines.

A grammar in [Backus–Naur form](https://en.wikipedia.org/wiki/Backus%E2%80%93Naur_form) comes first, with annotations for the constructs a reader may not recognise. An operational semantics follows, as a small-step reduction relation in the style of @plotkin1975call and @pierce2002types. The language's surface design belongs to the [ISWIM](https://en.wikipedia.org/wiki/ISWIM) family traced to @landin1966next; its type system belongs to the polymorphic lineage of @girard1972interpretation, via the principal-type-scheme theorem of @hindley1969principal and the Hindley-Milner fragment @milner1978theory and @damasmilner1982principal give a type-inference algorithm for. The chapter closes with a catalogue of the builtins and what each evaluates to.

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

The grammar is the concrete syntax parsed by [`panproto_expr_parser`](https://docs.rs/panproto-expr-parser/latest/panproto_expr_parser/). The abstract-syntax representation, used everywhere else in the engine, is the [`Expr`](https://docs.rs/panproto-expr/latest/panproto_expr/expr/enum.Expr.html) enum in [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/).

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

Type-checking is implemented in [`panproto_expr::typecheck`](https://docs.rs/panproto-expr/latest/panproto_expr/typecheck/). The rules follow a standard bidirectional-style elaboration in the Hindley-Milner lineage [@milner1978theory; @damasmilner1982principal]; checking modes propagate an expected type inward, while synthesis modes infer a type from the term's shape and push it outward. A comprehensive modern reference for the relevant typing and evaluation theory is @harper2016practical. Function types follow the standard rules of the simply typed lambda calculus. Records are structurally typed without row polymorphism, and lists are homogeneous. The panproto-native types are opaque inside the language (a program cannot inspect the internal structure of a `Schema` or an `Instance`) but are passed to builtins that know how to manipulate them.

Every well-typed term has a principal type the checker produces. A term that fails to check carries an error message naming the subterm at fault; the error messages are produced by the diagnostics infrastructure in [`panproto_expr::error`](https://docs.rs/panproto-expr/latest/panproto_expr/error/).

## Operational semantics

The evaluator in [`panproto_expr::eval`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/) implements a small-step reduction relation $\longrightarrow$ on closed terms. A term is a *value* when it matches one of the value forms (a literal, a record of values, a list of values, or a closure); the reduction relation is empty on values. Every non-value term has exactly one reduction step, chosen by a left-to-right, outermost-first evaluation order.

The defining rules are the standard ones. Beta reduction:
$$(\lambda x.\, e)\; v \;\longrightarrow\; e[v / x]$$
for a value $v$ substituted for the bound variable. Let reduction:
$$(\mathtt{let}\; x = v \; \mathtt{in}\; e) \;\longrightarrow\; e[v / x].$$
Record projection:
$$\{ \ldots, l = v, \ldots \}.l \;\longrightarrow\; v.$$
Pattern-match reduction reduces to the body of the first clause whose pattern matches the scrutinee, with the pattern's bound variables substituted as described above.

Reduction of a builtin applied to values is whatever the builtin's specification says: `add` on two integers returns their sum, and the same pattern applies to every other arithmetic, string, list, and instance-level operation. Each builtin has a reduction rule in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

Reduction is deterministic. The evaluation order is left-to-right and outermost-first, and every term has at most one reduction step available. Running the evaluator on a closed term produces a unique trace of reductions, which ends at either a value (success) or a bounded-resource failure (see the [Totality chapter](./totality.md)).

## Builtins

The language ships around fifty builtins in four groups.

### Arithmetic and comparison

`add`, `sub`, `mul`, `div`, `mod`, `neg`, `abs`, `min`, `max`, `eq`, `lt`, `leq`, `gt`, `geq`. Each has the standard mathematical meaning on `Int` and `Float` operands. Division and modulus on integers follow the Rust standard-library convention (truncation toward zero).

### String operations

`concat`, `length` (on strings), `toLower`, `toUpper`, `trim`, `split`, `replace`, `startsWith`, `endsWith`, `contains`, `format`. All are UTF-8-aware and treat strings as sequences of Unicode scalar values.

### List operations

`map`, `filter`, `foldl`, `foldr`, `length` (on lists), `head`, `tail`, `reverse`, `sort`, `zip`, `unzip`, `concat` (on lists), `take`, `drop`. The combinators have their standard Haskell meanings; the partiality of `head` and `tail` on empty lists is treated as a bounded-resource failure rather than an exception.

### Panproto-native operations

`instanceOf` (tests whether an instance is of a given schema), `getField`, `setField`, `listSchemasUnder` (enumerates the schemas under a protocol), `applyMigration` (applies a compiled migration to an instance), `renameField`, `requireField`. These are what migrations and field transforms use to manipulate panproto values; they are documented individually in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/) and in the expression-language skill.

Every builtin is total on well-typed inputs, modulo the bounded-resource failures covered in the next chapter. A builtin that would otherwise fail on bad input fails at the type-check stage with a diagnostic identifying the specific argument.

## Closing

The next chapter develops [totality and termination](./totality.md): the step and depth limits the evaluator enforces, the guarantees those limits give (every evaluation terminates, every evaluation produces a unique outcome, and every evaluation is serializable and deterministic), and the trade-offs the limits impose.

<!--
STATUS: Syntax and semantics chapter drafted.

CITATIONS:
  - Pierce 2002 "Types and Programming Languages": standard reference
    for small-step operational semantics. BibTeX pending.
-->
