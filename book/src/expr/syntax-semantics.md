# Syntax and semantics

The migrations of Part II are built around a theory morphism and a choice of pushforward at each extension site. The pushforward is where the engine needs a small programming language: an expression that, given the values visible at a site of the source instance, produces the value required at the target — a default, a computed field, a transformation of an existing value. Part III is about the language in which those expressions are written.

The language, called [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/), is a typed lambda calculus extended with records, lists, pattern matching, and around fifty builtin operations on strings, numbers, collections, and panproto schemas. It is pure, total, deterministic, and serialisable — four properties the engine depends on and which the next two chapters argue for at length. The surface syntax is Haskell-flavoured, not because of a preference for Haskell as such but because a migration expression is usually short, and the Haskell surface is the most compact idiom that still reads as code.

## Grammar

A term of the language is one of the following:

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

The grammar is small enough that most of it needs no comment. Variables and literals do what they do in every language. A lambda abstraction binds its variable in the body and is applied by juxtaposition. Let bindings introduce named intermediates. Composition `e1 . e2` is the function-composition operator, pronounced "after". Record literals and field access work as in Haskell, with the usual dotted syntax.

Two constructs deserve a moment. A list comprehension `[e1 | x <- e2]` denotes the list obtained by evaluating `e1` for each `x` drawn from `e2`; it extends to multiple generators and filters in the usual way, familiar from Haskell or from Python's analogous syntax. Pattern matching deconstructs a value by cases, with patterns that are variables (match anything and bind), wildcards (match and discard), literals (match the value), record patterns (match records with the named fields), or list patterns (`[x1, ..., xn]` or `x : xs`).

Numeric literals parse as signed 64-bit integers when they have no fractional part and as 64-bit floats otherwise; string literals are UTF-8 throughout. The grammar above is the concrete syntax the parser in [`panproto-expr-parser`](https://docs.rs/panproto-expr-parser/latest/panproto_expr_parser/) accepts; the abstract-syntax representation used everywhere else in the engine is the [`Expr`](https://docs.rs/panproto-expr/latest/panproto_expr/expr/enum.Expr.html) enum.

The tradition the surface belongs to is the ISWIM family, tracing back to @landin1966next's "The Next 700 Programming Languages": lambda-calculus-based expression languages where function abstraction and application carry the work, with a small layer of literals and structured values on top.

## Types

Every term has a type. The type language is

```
T ::= Int | Float | String | Bool       -- base types
    | T1 -> T2                          -- function type
    | { l1 : T1, ..., ln : Tn }         -- record type
    | [T]                               -- list type
    | Schema P | Instance P             -- panproto-native types
```

Four base types cover the scalars a migration expression will commonly need. Function types are the usual arrow. Record types are structural: a record type is determined by its field labels and their types, and two record types with the same fields (in any order) are equal. List types are homogeneous — a list of integers and a list of strings are different types, and a list literal cannot mix them. The two panproto-native types are opaque inside the language; an expression cannot inspect the internal structure of a `Schema` or an `Instance`, but both can be passed to builtins that know how to manipulate them.

The type system sits in the polymorphic lineage tracing to @girard1972interpretation's System F, via the principal-type-scheme theorem of @hindley1969principal and the Hindley-Milner fragment for which @milner1978theory and @damasmilner1982principal give a decidable inference algorithm. Panproto's type-checker uses Hindley-Milner rather than full System F; the restriction is enough for migration expressions and keeps type inference decidable and fast.

Type-checking is implemented in [`panproto_expr::typecheck`](https://docs.rs/panproto-expr/latest/panproto_expr/typecheck/) as a standard bidirectional-style elaboration: checking modes propagate an expected type inward from context, synthesis modes infer a type from the shape of a term and push it outward. @harper2016practical is the modern textbook reference for the relevant typing and evaluation theory, and @pierce2002types the standard pedagogical source.

Every well-typed term has a principal type the checker produces. A term that fails to check carries an error message naming the specific subterm at fault, produced by the diagnostics in [`panproto_expr::error`](https://docs.rs/panproto-expr/latest/panproto_expr/error/). Good error messages are not a decoration here — the expression language is a DSL that working developers write under time pressure, and a diagnostic pointing at the wrong subterm is barely better than no diagnostic at all.

## Operational semantics

The evaluator in [`panproto_expr::eval`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/) implements a small-step reduction relation on closed terms, in the style of @plotkin1975call. A term is a *value* when it matches a value form — a literal, a record of values, a list of values, a closure — and the reduction relation is empty on values. Every non-value term has exactly one reduction step, chosen by a left-to-right, outermost-first evaluation order. The choice is deliberate: left-to-right outermost-first is the eager strategy that keeps intermediate states serialisable and the reduction sequence predictable.

The defining rules are the standard ones. Beta reduction substitutes a value for a bound variable:

$$(\lambda x.\, e)\; v \;\longrightarrow\; e[v / x].$$

Let reduction does the same:

$$(\mathtt{let}\; x = v \; \mathtt{in}\; e) \;\longrightarrow\; e[v / x].$$

Record projection takes a field:

$$\{ \ldots, l = v, \ldots \}.l \;\longrightarrow\; v.$$

Pattern-match reduction picks the body of the first clause whose pattern matches the scrutinee, with the pattern's bound variables substituted into the body. Reduction of a builtin applied to values is whatever the builtin's specification says: `add` on two integers returns their sum, `concat` on two strings returns their concatenation, and so on through the catalogue. Each builtin has a reduction rule in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

Reduction is deterministic: the same closed term always produces the same reduction trace, because the evaluation order is fixed and no builtin has hidden non-determinism. Running the evaluator on a closed term therefore produces a unique outcome — either a value, when reduction terminates, or a bounded-resource failure, which is the subject of [the next chapter](./totality.md).

## Builtins

The language ships around fifty builtins. They fall into four groups, and the catalogue below is reference material rather than reading material; a developer who wants the full list can consult [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

### Arithmetic and comparison

`add`, `sub`, `mul`, `div`, `mod`, `neg`, `abs`, `min`, `max`, `eq`, `lt`, `leq`, `gt`, `geq`. Each has the standard mathematical meaning on `Int` and `Float` operands. Division and modulus on integers follow the Rust standard-library convention: truncation toward zero.

### Strings

`concat`, `length`, `toLower`, `toUpper`, `trim`, `split`, `replace`, `startsWith`, `endsWith`, `contains`, `format`. All are UTF-8-aware and treat strings as sequences of Unicode scalar values. A reader expecting byte-level semantics should be aware that string indexing and length report Unicode scalar counts, not byte counts; byte-level work is done separately through list-of-integer instances.

### Lists

`map`, `filter`, `foldl`, `foldr`, `length`, `head`, `tail`, `reverse`, `sort`, `zip`, `unzip`, `concat`, `take`, `drop`. The combinators have their standard Haskell meanings; the partiality of `head` and `tail` on empty lists is treated as a bounded-resource failure rather than an exception.

### Panproto-native

`instanceOf` tests whether an instance conforms to a given schema. `getField` and `setField` access and modify a field of an instance by name. `listSchemasUnder` enumerates the schemas registered under a protocol. `applyMigration` applies a compiled migration to an instance. `renameField` and `requireField` operate on the instance's field structure. These are the builtins migrations and field transforms reach for when manipulating panproto values, and each is documented in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/).

Every builtin is total on well-typed inputs, modulo the bounded-resource failures covered in the next chapter. A builtin that would fail on ill-typed input fails at the type-check stage, with a diagnostic identifying the specific argument; combined with the type-checker, the builtins become a pure total function from closed well-typed terms to values.

## Further reading

For the operational-semantics side, @plotkin1975call's call-by-value/call-by-name technical report (later republished in *Theoretical Computer Science*) is the foundational reference. @pierce2002types is the standard textbook treatment of small-step semantics. For the type-system side, @hindley1969principal is the original principal-type theorem; @milner1978theory and @damasmilner1982principal developed the algorithm the type-checker is a variant of. @harper2016practical covers the whole area — operational semantics, typing, polymorphism — in a single modern reference.

For the ISWIM lineage, @landin1966next is the foundational source.

## Closing

The next chapter introduces the step and depth limits the evaluator enforces, and explains what they buy and what they cost.
