# Expression-language reference

`panproto-expr` evaluates a call-by-value expression AST with no I/O operations. Evaluation is bounded and fallible. The default limits are 100,000 reduction steps, recursion depth 256, and 10,000 elements in list literals and selected list-producing paths. Reaching a limit returns an `ExprError`. It does not establish that every expression terminates before the limit.

Builtin names are first-class values. A lexical environment binding shadows a builtin of the same name, and an unapplied builtin may be partially applied until it receives its declared arity. [`eval_with_resolver`](https://docs.rs/panproto-expr/latest/panproto_expr/fn.eval_with_resolver.html) routes context-dependent builtins through the supplied resolver wherever they occur, including inside lambdas, bindings, and comprehensions.

The language is used for field transforms and instance queries. [Expression language: denotational semantics](../explanation/semantics/expression-language.md) gives the model behind the evaluator.

## Surface grammar

[`panproto-expr-parser`](https://docs.rs/panproto-expr-parser/latest/panproto_expr_parser/) accepts a Haskell-style syntax and lowers it to [`Expr`](https://docs.rs/panproto-expr/latest/panproto_expr/enum.Expr.html).

| Form | Syntax |
|---|---|
| Literals | `42`, `3.5`, `"text"`, `True`, `False`, `Nothing` |
| Lambda and application | `\x -> body`, `f x` |
| Binding | `let x = value in body`. Layout or braces permit several bindings. |
| Conditional and match | `if p then a else b` and `case value of { pattern -> body }` |
| Records | `{ name = value, count }`. The second field uses record punning. |
| Lists | `[a, b]`, inclusive range `[a..b]`, comprehension `[f x | x <- xs, p x]` |
| Access | `record.field`, `list[index]`, `node->edge_name` |
| Sequencing | list-oriented `do` notation and postfix `where` bindings |

Open-ended ranges such as `[a..]` are parse errors. The parser lowers `map f xs`, `filter p xs`, and `flat_map f xs` to an AST with the list first and function last. It lowers `fold f z xs` to `[xs, z, f]`. This AST argument order is the serialized compatibility contract.

### Operators

From lower to higher precedence, the infix operators are pipe `&`, `||`, `&&`, comparisons (`==`, `/=`, `<`, `<=`, `>`, `>=`), string concatenation `++`, addition and subtraction, then multiplication, division, and remainder (`*`, `/`, `%`, `div`, `mod`). Unary `-` and `not` bind more tightly. `And` and `Or` evaluate both AST arguments before applying the builtin. They do not short-circuit.

## Values and type tags

Runtime [`Literal`](https://docs.rs/panproto-expr/latest/panproto_expr/enum.Literal.html) values include booleans, 64-bit integers and floats, UTF-8 strings, bytes, null, records, lists, and closures. Lists can contain values of different kinds. The lightweight [`ExprType`](https://docs.rs/panproto-expr/latest/panproto_expr/enum.ExprType.html) inference API has fewer tags:

| `ExprType` | Meaning |
|---|---|
| `Int`, `Float`, `Str`, `Bool` | Scalar tags. |
| `List` | List with no element-type parameter. |
| `Record` | Ordered string-keyed fields. |
| `Any` | Unknown or polymorphic. Inference also uses it for null, bytes, and closures. |

Type inference is best effort. Application, field access, and indexing generally infer as `Any`, while evaluation still performs runtime type and arity checks.

## Builtins

[`BuiltinOp`](https://docs.rs/panproto-expr/latest/panproto_expr/enum.BuiltinOp.html) currently has 60 variants.

| Family | Operations | Contract notes |
|---|---|---|
| Arithmetic and rounding | `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Abs`, `Floor`, `Ceil`, `Round` | Integer arithmetic is checked. Division and remainder reject zero divisors. Float-to-integer rounding rejects NaN, infinity, and out-of-range results. |
| Comparison | `Eq`, `Neq`, `Lt`, `Lte`, `Gt`, `Gte` | Ordering accepts integer and float pairs, including mixed numeric pairs, or two strings. |
| Boolean | `And`, `Or`, `Not` | Arguments are eager. |
| String | `Concat`, `Len`, `Slice`, `Upper`, `Lower`, `Trim`, `Split`, `Join`, `Replace`, `Contains` | `Len` counts UTF-8 bytes, while `Slice` indexes Unicode scalar values. `Contains` also tests list membership. |
| List | `Map`, `Filter`, `Fold`, `FlatMap`, `Append`, `Head`, `Tail`, `Reverse`, `Length`, `Range` | `Range` includes both bounds and returns an empty list when `stop < start`. The evaluator enforces `max_list_len` for list literals, `Map`, `FlatMap`, and `Range`. The generic builtin handler does not apply that limit to every list-returning operation. |
| Record | `MergeRecords`, `Keys`, `Values`, `HasField` | In a merge, fields from the second record replace equal keys from the first. |
| Utility | `DefaultVal`, `Clamp`, `TruncateStr` | `DefaultVal` substitutes only for null. |
| Coercion | `IntToFloat`, `FloatToInt`, `IntToStr`, `FloatToStr`, `StrToInt`, `StrToFloat` | String parses can return `ParseError`. `FloatToInt` can return `FloatNotRepresentable`. |
| Inspection | `TypeOf`, `IsNull`, `IsList` | Inspect runtime values. |
| Instance traversal | `Edge`, `Children`, `HasEdge`, `EdgeCount`, `Anchor` | The pure evaluator returns `NoInstanceContext`. Use [`eval_with_instance`](https://docs.rs/panproto-inst/latest/panproto_inst/instance_env/fn.eval_with_instance.html), `eval_with_element_ops`, or another `BuiltinResolver` to supply graph context. |

## Evaluation errors

[`ExprError`](https://docs.rs/panproto-expr/latest/panproto_expr/enum.ExprError.html) is non-exhaustive.

| Variant | Cause |
|---|---|
| `StepLimitExceeded` | The reduction budget was exhausted. |
| `DepthExceeded` | Recursive evaluation exceeded `max_depth`. |
| `ListLengthExceeded` | A list constructor or a budgeted list-producing path exceeded `max_list_len`. |
| `UnboundVariable` | The environment has no binding for a variable. |
| `TypeError`, `ArityMismatch`, `NotAFunction` | A value or call has the wrong runtime shape. |
| `IndexOutOfBounds`, `FieldNotFound`, `NonExhaustiveMatch` | Lookup or pattern matching failed. |
| `DivisionByZero`, `Overflow`, `FloatNotRepresentable`, `ParseError` | A numeric operation or coercion failed. |
| `NoInstanceContext` | An instance-traversal builtin was evaluated without an instance resolver. |
| `InternalDispatch` | A builtin reached the wrong internal family handler. |

## Source

The AST and builtin enum live in [`expr.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/expr.rs), builtin implementations in [`builtin.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/builtin.rs), evaluation and defaults in [`eval.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr/src/eval.rs), and the surface grammar in [`parser.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-expr-parser/src/parser.rs).

## See also

- [Apply field transforms](../how-to/field-transforms.md)
- [Query instances](../how-to/query-instances.md)
