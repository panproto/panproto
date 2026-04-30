# Expressions

panproto includes a pure functional expression language used in migration resolvers, enriched theory defaults, coercion functions, and conflict resolution policies. The language is a lambda calculus with ~50 built-in operations, pattern matching, records, and lists.

## Parsing

Expressions use a Haskell-style surface syntax with layout-sensitive indentation:

```python
import panproto

expr = panproto.parse_expr(r"\x -> x + 1")
print(expr.pretty())    # \x -> x + 1
```

## Evaluation

```python
result = expr.eval()
print(result)   # {"Int": 3}  (for "1 + 2")
```

Evaluation is deterministic and bounded. The default `EvalConfig` limits step count and recursion depth to prevent non-termination.

## Syntax

| Feature | Example |
|---------|---------|
| Lambda | `\x -> x + 1` |
| Application | `f x y` |
| Let binding | `let x = 1 in x + 2` |
| Where clause | `x + y where x = 1; y = 2` |
| Case/of | `case x of { 0 -> "zero"; _ -> "other" }` |
| Records | `{ name = "alice", age = 30 }` |
| Field access | `record.name` |
| Lists | `[1, 2, 3]` |
| List comprehension | `[x * 2 | x <- xs, x > 0]` |
| Composition | `f . g` |
| Operator sections | `(+ 1)`, `(2 *)` |
| Graph traversal | `node -> edge` |

## Built-in operations

Approximately 50 operations organized by category:

**Arithmetic**: `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Abs`, `Floor`, `Ceil`

**Comparison**: `Eq`, `Neq`, `Lt`, `Lte`, `Gt`, `Gte`

**Boolean**: `And`, `Or`, `Not`

**String**: `Concat`, `Len`, `Slice`, `Upper`, `Lower`, `Trim`, `Split`, `Join`, `Replace`, `Contains`

**List**: `Map`, `Filter`, `Fold`, `Append`, `Reverse`, `Head`, `Tail`, `Nth`, `Length`

**Conversion**: `ToStr`, `ToInt`, `ToFloat`, `FromStr`, `TypeOf`

## AST inspection

```python
d = expr.to_dict()   # full AST as a Python dict
```

## Pretty-printing

```python
panproto.pretty_print_expr(expr)   # round-trip to surface syntax
```
