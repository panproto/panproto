# panproto-expr-parser

[![crates.io](https://img.shields.io/crates/v/panproto-expr-parser.svg)](https://crates.io/crates/panproto-expr-parser)
[![docs.rs](https://docs.rs/panproto-expr-parser/badge.svg)](https://docs.rs/panproto-expr-parser)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Parses human-written expressions into the `panproto-expr` AST, and formats them back to text.

## What it does

`panproto-expr` represents expressions as an AST (`Expr` enum values). This crate is the bridge between that AST and a string you can actually type. It takes a string like `\x -> if x > 0 then x * 2 else 0` and produces the corresponding `Expr` tree, or takes an `Expr` tree and formats it back to a readable string.

The surface syntax follows Haskell conventions: backslash-lambdas, `let`/`in`, `case`/`of` with pattern matching, `if`/`then`/`else`, list comprehensions, and record literals. A Pratt precedence climbing algorithm handles operator precedence correctly, so `1 + 2 * 3` parses as `1 + (2 * 3)` without requiring explicit parentheses. The pretty-printer is the inverse: it adds parentheses only where needed to preserve the original meaning, so `parse(tokenize(pretty_print(expr))) == expr` for all well-formed expressions.

The lexer uses `logos` for fast tokenization and inserts GHC-style layout tokens (`Indent`/`Dedent`) so indented blocks work without explicit braces. The parser uses `chumsky` 1.0.

## Quick example

```rust,ignore
use panproto_expr_parser::{tokenize, parse, pretty_print};

let tokens = tokenize(r#"\x -> x * 2 + 1"#).unwrap();
let expr = parse(&tokens).unwrap();

// Serialize back to canonical surface syntax.
let source = pretty_print(&expr);
assert_eq!(source, r#"\x -> x * 2 + 1"#);
```

## API overview

| Item | What it does |
|------|-------------|
| `tokenize` | Lex a source string into a token stream; inserts layout tokens for indentation |
| `parse` | Parse a token stream into a `panproto_expr::Expr` AST |
| `pretty_print` | Format an `Expr` back to canonical surface syntax with minimal parentheses |
| `Token` | 50+ token kinds: keywords, literals, operators, delimiters, and layout tokens |
| `Span` / `Spanned` | Source location attached to tokens and parse errors |
| `LexError` / `ParseError` | Structured error types with source spans for editor integration |

## License

[MIT](../../LICENSE)
