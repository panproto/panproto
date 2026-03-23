# panproto-expr-parser

[![crates.io](https://img.shields.io/crates/v/panproto-expr-parser.svg)](https://crates.io/crates/panproto-expr-parser)
[![docs.rs](https://docs.rs/panproto-expr-parser/badge.svg)](https://docs.rs/panproto-expr-parser)

Haskell-style surface syntax parser for panproto expressions.

Parses a human-readable functional language into `panproto_expr::Expr` AST nodes. The surface syntax supports lambda expressions, let/where bindings, if/then/else, case/of with pattern matching, list comprehensions, do-notation, record literals with punning, field access, graph edge traversal (`->`), and infix operators with correct precedence. A pretty printer converts AST nodes back to canonical surface syntax with minimal parentheses.

## API

| Item | Description |
|------|-------------|
| `tokenize` | Logos-based lexer with GHC-style layout insertion (Indent/Dedent/Newline virtual tokens) |
| `parse` | Chumsky 1.0 recursive-descent + Pratt precedence parser producing `Expr` |
| `pretty_print` | Precedence-aware pretty printer with minimal parenthesization |
| `Token` | 50+ token kinds: keywords, literals, operators, delimiters, layout tokens |
| `Span` / `Spanned` | Source location tracking for error reporting |
| `LexError` / `ParseError` | Structured error types with source spans |

## Example

```rust,ignore
use panproto_expr_parser::{tokenize, parse, pretty_print};

// Parse a Haskell-style expression
let tokens = tokenize(r#"\x -> x + 1"#).unwrap();
let expr = parse(&tokens).unwrap();

// Pretty-print back to canonical form
let source = pretty_print(&expr);
assert_eq!(source, r#"\x -> x + 1"#);
```

## Surface syntax

```haskell
-- Arithmetic and comparison
1 + 2 * 3
x == 0 && y > 10

-- Lambda and application
\x y -> x + y
map (\x -> x * 2) xs

-- Let bindings and where clauses
let x = 1 in x + 2
result where result = a + b

-- Conditionals and pattern matching
if age > 18 then "adult" else "minor"
case shape of
  Circle r -> 3.125 * r * r
  Rect w h -> w * h

-- Records with punning
{ name = "alice", age = 30 }
{ name, age }

-- Lists and comprehensions
[1, 2, 3]
[ x * 2 | x <- xs, x > 0 ]

-- Graph edge traversal
doc -> layers -> annotations

-- Field access
node.name
record.field.subfield
```

## Operator precedence

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `&` (pipe) | left |
| 3 | `\|\|` | left |
| 4 | `&&` | left |
| 5 | `== /= < <= > >=` | right |
| 6 | `++` | right |
| 7 | `+ -` | left |
| 8 | `* / % mod div` | left |
| 9 | unary `-`, `not` | prefix |

## Design

- **Logos** for fast regex-based tokenization with a second pass for GHC-style layout insertion.
- **Chumsky 1.0** (alpha) for parser combinators with Pratt parsing for operator precedence.
- **Round-trip**: `parse(tokenize(pretty_print(expr))) == expr` for all well-formed expressions.
- **Desugaring**: list comprehensions, do-notation, where clauses, and multi-param lambdas are desugared during parsing into core `Expr` variants.
- **Builtin resolution**: identifiers matching builtin names (e.g., `map`, `filter`, `fold`) are resolved to `Expr::Builtin` at application sites.

## License

[MIT](../../LICENSE)
