# Expression-language reference

The `panproto-expr` language is pure, total, and bounded: every well-typed expression terminates within a fixed step budget, no IO is permitted, and evaluation is deterministic. It is used inside migrations to describe field-level transforms, and inside queries to describe predicates and projections.

For the model behind the language, see [Expression language: denotational semantics](../explanation/semantics/expression-language.md).

## Surface syntax

The Haskell-style surface syntax supports literals, variables, lambdas, application, `let`, pattern matching, and list comprehensions. The full grammar lives in [`panproto-expr-parser`](https://github.com/panproto/panproto/tree/main/crates/panproto-expr-parser). Inside JSON or YAML migration files, expressions appear as strings; the parser is invoked on read.

## Types

| Type | Description |
|---|---|
| `Int` | 64-bit signed integer with checked arithmetic. Overflow is an error, not a wrap. |
| `Float` | IEEE 754 64-bit float. |
| `Str` | UTF-8 string. |
| `Bool` | Boolean. |
| `List a` | Heterogeneous list (the type parameter is the element constraint when known). |
| `Record` | Map from string keys to values. |
| `Null` | Singleton type for the absence of a value. |
| `Any` | Top type used in builtins that accept any input. |

## Builtins

The 59 built-in operations are in `panproto_expr::BuiltinOp`, grouped by family in `crates/panproto-expr/src/builtin.rs`.

### Arithmetic
`Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Abs`, `Floor`, `Ceil`, `Round`. `Div` and `Mod` raise `DivisionByZero` on a zero divisor. Integer arithmetic uses checked operations; overflow raises `Overflow`.

### Comparison
`Eq`, `Neq`, `Lt`, `Lte`, `Gt`, `Gte`. Returns `Bool`.

### Boolean
`And`, `Or`, `Not`. Short-circuit evaluation.

### String
`Concat`, `Len`, `Slice`, `Upper`, `Lower`, `Trim`, `Split`, `Join`, `Replace`, `Contains`.

### List
`Map`, `Filter`, `Fold`, `FlatMap`, `Append`, `Head`, `Tail`, `Reverse`, `Length`. `Head` and `Tail` raise on the empty list.

### Record
`MergeRecords`, `Keys`, `Values`, `HasField`.

### Utility
`DefaultVal`, `Clamp`, `TruncateStr`.

### Type coercion
`IntToFloat`, `FloatToInt`, `IntToStr`, `FloatToStr`, `StrToInt`, `StrToFloat`. The `*ToInt` and `*ToFloat` parses raise `ParseFailure` on invalid input.

### Type inspection
`TypeOf`, `IsNull`, `IsList`.

### Graph traversal (instance-aware)
`Edge`, `Children`, `HasEdge`, `EdgeCount`, `Anchor`. These return `Null` in the standard evaluator. To use them, evaluate in an instance environment via `panproto_inst::instance_env`.

## Errors

| Error | Cause |
|---|---|
| `ArityMismatch` | Builtin called with the wrong number of arguments. |
| `TypeError` | Operand had an incompatible type. |
| `DivisionByZero` | `Div` or `Mod` with a zero divisor. |
| `Overflow` | Checked integer arithmetic overflowed. |
| `ParseFailure` | A `*To*` coercion failed to parse its input. |
| `BudgetExceeded` | Evaluation exceeded the step budget (totality enforcement). |

## Authoritative source

Builtin enum: [`crates/panproto-expr/src/expr.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/expr.rs). Implementation: [`crates/panproto-expr/src/builtin.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/builtin.rs). Evaluator: [`crates/panproto-expr/src/eval.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/eval.rs).

## See also

- [Apply field transforms](../how-to/field-transforms.md) for using the language inside migrations.
- [Query instances](../how-to/query-instances.md) for using it inside queries.
- [Expression language: denotational semantics](../explanation/semantics/expression-language.md) for the formal model.
