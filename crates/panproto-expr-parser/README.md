# panproto-expr-parser

[![crates.io](https://img.shields.io/crates/v/panproto-expr-parser.svg)](https://crates.io/crates/panproto-expr-parser)
[![docs.rs](https://docs.rs/panproto-expr-parser/badge.svg)](https://docs.rs/panproto-expr-parser)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Lexer, parser, and pretty-printer for `panproto-expr`.

## Syntax and implementation

The surface language includes lambdas, application, `let`, conditionals,
pattern matching, records, lists, list comprehensions, literals, and infix
operators. The lexer uses `logos`. It inserts indentation tokens for layout-sensitive
constructs. The parser uses `chumsky` and a Pratt parser for precedence.

`pretty_print` emits a canonical surface form and inserts parentheses according to
the parser's precedence table. Property tests check pretty-print and reparse on
generated well-formed expressions. This is a tested invariant over those generators,
not a claim that arbitrary malformed source is preserved.

## Example

```rust,ignore
use panproto_expr_parser::{parse, pretty_print, tokenize};

let tokens = tokenize(r#"\x -> x * 2 + 1"#)?;
let expr = parse(&tokens)?;
let rendered = pretty_print(&expr);
```

## Public API

| Item | Purpose |
|------|---------|
| `tokenize` | Produce a spanned token stream |
| `parse` | Parse spanned tokens into `panproto_expr::Expr` |
| `pretty_print` | Render an expression in canonical surface syntax |
| `Token`, `Span`, `Spanned` | Token and source-location types |
| `LexError`, `parser::ParseError` | Lexer and chumsky parser errors |

## License

[MIT](../../LICENSE)
