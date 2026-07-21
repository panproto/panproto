//! Chumsky parser producing `panproto_expr::Expr` from the token stream.
//!
//! Implements the grammar defined in `notes/POLY_IMPLEMENTATION_PLAN.md`.
//! Uses Pratt parsing for operator precedence and recursive descent for
//! the rest. Layout tokens (`Indent`/`Dedent`/`Newline`) from the lexer
//! are consumed directly as delimiters for layout-sensitive blocks.

use std::sync::Arc;

use chumsky::input::{Input as _, Stream, ValueInput};
use chumsky::pratt::{infix, left, prefix, right};
use chumsky::prelude::*;
use chumsky::span::SimpleSpan;

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};

use crate::token::Token;

/// A parse error.
pub type ParseError = Rich<'static, Token, SimpleSpan>;

/// Parse a token stream into an `Expr`.
///
/// The input should come from [`crate::tokenize`].
///
/// # Errors
///
/// Returns parse errors with source spans on failure.
pub fn parse(tokens: &[crate::Spanned]) -> Result<Expr, Vec<ParseError>> {
    let mapped: Vec<(Token, SimpleSpan)> = tokens
        .iter()
        .filter(|s| s.token != Token::Eof)
        .map(|s| (s.token.clone(), SimpleSpan::new(s.span.start, s.span.end)))
        .collect();
    let eoi = tokens.last().map_or_else(
        || SimpleSpan::new(0, 0),
        |s| SimpleSpan::new(s.span.start, s.span.end),
    );
    let stream = Stream::from_iter(mapped).map(eoi, |(tok, span)| (tok, span));
    expr_parser().parse(stream).into_result().map_err(|errs| {
        errs.into_iter()
            .map(chumsky::error::Rich::into_owned)
            .collect()
    })
}

// ── Token matchers ──────────────────────────────────────────────────

/// Match an identifier and return its name.
fn ident<'t, 'src: 't, I>()
-> impl Parser<'t, I, Arc<str>, extra::Err<Rich<'t, Token, SimpleSpan>>> + Clone
where
    I: ValueInput<'t, Token = Token, Span = SimpleSpan>,
{
    select! { Token::Ident(s) => Arc::from(s.as_str()) }.labelled("identifier")
}

/// Match an upper-case identifier.
fn upper_ident<'t, 'src: 't, I>()
-> impl Parser<'t, I, Arc<str>, extra::Err<Rich<'t, Token, SimpleSpan>>> + Clone
where
    I: ValueInput<'t, Token = Token, Span = SimpleSpan>,
{
    select! { Token::UpperIdent(s) => Arc::from(s.as_str()) }.labelled("constructor")
}

// ── Layout blocks ───────────────────────────────────────────────────

/// Parse a layout block: either `{ item ; item ; ... }` or
/// `INDENT item NEWLINE item ... DEDENT`.
fn layout_block<'t, 'src: 't, I, T: 't>(
    item: impl Parser<'t, I, T, extra::Err<Rich<'t, Token, SimpleSpan>>> + Clone,
) -> impl Parser<'t, I, Vec<T>, extra::Err<Rich<'t, Token, SimpleSpan>>> + Clone
where
    I: ValueInput<'t, Token = Token, Span = SimpleSpan>,
{
    let explicit = item
        .clone()
        .separated_by(just(Token::Newline).or(just(Token::Comma)))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    let implicit = item
        .separated_by(just(Token::Newline))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::Indent), just(Token::Dedent));

    explicit.or(implicit)
}

// ── Pattern parser ──────────────────────────────────────────────────

/// Parse a pattern.
fn pattern_parser<'t, 'src: 't, I>()
-> impl Parser<'t, I, Pattern, extra::Err<Rich<'t, Token, SimpleSpan>>> + Clone
where
    I: ValueInput<'t, Token = Token, Span = SimpleSpan>,
{
    recursive(|pat| {
        let wildcard = select! { Token::Ident(s) if s == "_" => Pattern::Wildcard };

        let var = ident().map(Pattern::Var);

        let literal_pat = literal_parser().map(Pattern::Lit);

        let paren = pat
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let list_pat = pat
            .clone()
            .separated_by(just(Token::Comma))
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(Pattern::List);

        let field_pat = ident()
            .then(just(Token::Eq).ignore_then(pat.clone()).or_not())
            .map(|(name, maybe_pat): (Arc<str>, Option<Pattern>)| {
                let p = maybe_pat.unwrap_or_else(|| Pattern::Var(name.clone()));
                (name, p)
            });

        let record_pat = field_pat
            .separated_by(just(Token::Comma))
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(Pattern::Record);

        let constructor = upper_ident()
            .then(pat.clone().repeated().collect::<Vec<_>>())
            .map(|(name, args): (Arc<str>, Vec<Pattern>)| Pattern::Constructor(name, args));

        choice((
            wildcard,
            literal_pat,
            paren,
            list_pat,
            record_pat,
            constructor,
            var,
        ))
    })
}

// ── Literal parser ──────────────────────────────────────────────────

/// Parse a literal value.
fn literal_parser<'t, 'src: 't, I>()
-> impl Parser<'t, I, Literal, extra::Err<Rich<'t, Token, SimpleSpan>>> + Clone
where
    I: ValueInput<'t, Token = Token, Span = SimpleSpan>,
{
    select! {
        Token::Int(n) => Literal::Int(n),
        Token::Float(f) => Literal::Float(f),
        Token::Str(s) => Literal::Str(s),
        Token::True => Literal::Bool(true),
        Token::False => Literal::Bool(false),
        Token::Nothing => Literal::Null,
    }
    .labelled("literal")
}

// ── Builtin name → op mapping ───────────────────────────────────────

/// Resolve a lowercase identifier to a builtin op, if any.
fn resolve_builtin(name: &str) -> Option<BuiltinOp> {
    match name {
        "add" => Some(BuiltinOp::Add),
        "sub" => Some(BuiltinOp::Sub),
        "mul" => Some(BuiltinOp::Mul),
        "abs" => Some(BuiltinOp::Abs),
        "floor" => Some(BuiltinOp::Floor),
        "ceil" => Some(BuiltinOp::Ceil),
        "round" => Some(BuiltinOp::Round),
        "concat" => Some(BuiltinOp::Concat),
        "len" => Some(BuiltinOp::Len),
        "slice" => Some(BuiltinOp::Slice),
        "upper" => Some(BuiltinOp::Upper),
        "lower" => Some(BuiltinOp::Lower),
        "trim" => Some(BuiltinOp::Trim),
        "split" => Some(BuiltinOp::Split),
        "join" => Some(BuiltinOp::Join),
        "replace" => Some(BuiltinOp::Replace),
        "contains" => Some(BuiltinOp::Contains),
        "map" => Some(BuiltinOp::Map),
        "filter" => Some(BuiltinOp::Filter),
        "fold" => Some(BuiltinOp::Fold),
        "append" => Some(BuiltinOp::Append),
        "head" => Some(BuiltinOp::Head),
        "tail" => Some(BuiltinOp::Tail),
        "reverse" => Some(BuiltinOp::Reverse),
        "flat_map" | "flatMap" => Some(BuiltinOp::FlatMap),
        "length" => Some(BuiltinOp::Length),
        "range" => Some(BuiltinOp::Range),
        "merge" | "merge_records" => Some(BuiltinOp::MergeRecords),
        "keys" => Some(BuiltinOp::Keys),
        "values" => Some(BuiltinOp::Values),
        "has_field" | "hasField" => Some(BuiltinOp::HasField),
        "default" | "default_val" | "defaultVal" => Some(BuiltinOp::DefaultVal),
        "clamp" => Some(BuiltinOp::Clamp),
        "truncate_str" | "truncateStr" => Some(BuiltinOp::TruncateStr),
        "int_to_float" | "intToFloat" => Some(BuiltinOp::IntToFloat),
        "float_to_int" | "floatToInt" => Some(BuiltinOp::FloatToInt),
        "int_to_str" | "intToStr" => Some(BuiltinOp::IntToStr),
        "float_to_str" | "floatToStr" => Some(BuiltinOp::FloatToStr),
        "str_to_int" | "strToInt" => Some(BuiltinOp::StrToInt),
        "str_to_float" | "strToFloat" => Some(BuiltinOp::StrToFloat),
        "type_of" | "typeOf" => Some(BuiltinOp::TypeOf),
        "is_null" | "isNull" => Some(BuiltinOp::IsNull),
        "is_list" | "isList" => Some(BuiltinOp::IsList),
        "edge" => Some(BuiltinOp::Edge),
        "children" => Some(BuiltinOp::Children),
        "has_edge" | "hasEdge" => Some(BuiltinOp::HasEdge),
        "edge_count" | "edgeCount" => Some(BuiltinOp::EdgeCount),
        "anchor" => Some(BuiltinOp::Anchor),
        _ => None,
    }
}

// ── Expression parser ───────────────────────────────────────────────

/// Top-level expression parser.
///
/// This is a single `chumsky::recursive` combinator defining the entire
/// expression grammar: literals, variables, list literals/comprehensions,
/// function application, the pratt operator table, lambdas, let, if/case,
/// do-notation, and where-clauses. Extracting individual sub-parsers
/// would require forwarding the recursive `expr` parser by reference
/// with a verbose `impl Parser<…> + Clone` bound at each call site, which
/// would hurt rather than help readability. The grammar is kept inline
/// and the length lint is silenced locally.
#[allow(clippy::too_many_lines)]
fn expr_parser<'t, 'src: 't, I>()
-> impl Parser<'t, I, Expr, extra::Err<Rich<'t, Token, SimpleSpan>>> + Clone
where
    I: ValueInput<'t, Token = Token, Span = SimpleSpan>,
{
    recursive(|expr| {
        let pattern = pattern_parser();

        // ── Atoms ───────────────────────────────────────────

        let lit = literal_parser().map(Expr::Lit);

        let var_or_builtin = ident().map(Expr::Var);

        let constructor = upper_ident().map(Expr::Var);

        let paren_expr = expr
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // List literal or comprehension
        let list_expr = {
            let plain_list = expr
                .clone()
                .separated_by(just(Token::Comma))
                .collect::<Vec<_>>()
                .map(Expr::List);

            // List comprehension: [e | x <- xs, pred]
            let comprehension = expr
                .clone()
                .then_ignore(just(Token::Pipe))
                .then(
                    ident()
                        .then_ignore(just(Token::LeftArrow))
                        .then(expr.clone())
                        .map(|(n, e): (Arc<str>, Expr)| Qual::Generator(n, e))
                        .or(expr.clone().map(Qual::Guard))
                        .separated_by(just(Token::Comma))
                        .at_least(1)
                        .collect::<Vec<Qual>>(),
                )
                .map(|(body, quals): (Expr, Vec<Qual>)| desugar_comprehension(body, &quals));

            // Range: [1..10]. Lowers to the `range` builtin, which
            // constructs the list and charges its length against the
            // evaluator's list budget. An open-ended `[1..]` is rejected:
            // the language has no lazy lists, so there is nothing correct
            // to lower it to, and the alternative of quietly yielding the
            // one-element list `[1]` is a wrong answer rather than a
            // missing feature.
            let range = expr
                .clone()
                .then_ignore(just(Token::DotDot))
                .then(expr.clone().or_not())
                .validate(|(start, end): (Expr, Option<Expr>), extra, emitter| {
                    if let Some(stop) = end {
                        return Expr::Builtin(BuiltinOp::Range, vec![start, stop]);
                    }
                    emitter.emit(Rich::custom(
                        extra.span(),
                        "open-ended range `[a..]` is not supported: the expression \
                         language has no lazy lists. Give an upper bound, as in `[a..b]`.",
                    ));
                    // Emitting rather than failing keeps this branch the winning
                    // alternative, so the message above survives instead of being
                    // masked by a backtrack into the plain-list parser. The value
                    // is never evaluated: parsing fails on the emitted error.
                    Expr::List(vec![start])
                });

            choice((comprehension, range, plain_list))
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
        };

        // Record literal
        let record_expr = {
            let field_bind = ident()
                .then(just(Token::Eq).ignore_then(expr.clone()).or_not())
                .map(|(name, val): (Arc<str>, Option<Expr>)| {
                    let v = val.unwrap_or_else(|| Expr::Var(name.clone()));
                    (name, v)
                });

            field_bind
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .map(Expr::Record)
        };

        let atom = choice((
            lit,
            paren_expr,
            list_expr,
            record_expr,
            constructor,
            var_or_builtin,
        ));

        // ── Postfix: field access (.field) and edge traversal (->edge) ──

        let postfix_chain = atom.foldl(
            choice((
                just(Token::Dot).ignore_then(ident()).map(PostfixOp::Field),
                just(Token::Arrow).ignore_then(ident()).map(PostfixOp::Edge),
            ))
            .repeated(),
            |expr, postfix| match postfix {
                PostfixOp::Field(name) => Expr::Field(Box::new(expr), name),
                PostfixOp::Edge(edge) => Expr::Builtin(
                    BuiltinOp::Edge,
                    vec![expr, Expr::Lit(Literal::Str(edge.to_string()))],
                ),
            },
        );

        // ── Application (juxtaposition) ─────────────────────

        let app = postfix_chain
            .clone()
            .foldl(postfix_chain.repeated(), resolve_application);

        // ── Pratt parser for infix/prefix operators ─────────

        let pratt = app.pratt((
            // Precedence 1: pipe (&)
            infix(left(1), just(Token::Ampersand), |l, _, r, _| {
                Expr::App(Box::new(r), Box::new(l))
            }),
            // Precedence 3: logical or
            infix(left(3), just(Token::OrOr), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Or, vec![l, r])
            }),
            // Precedence 4: logical and
            infix(left(4), just(Token::AndAnd), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::And, vec![l, r])
            }),
            // Precedence 5: comparison
            infix(right(5), just(Token::EqEq), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Eq, vec![l, r])
            }),
            infix(right(5), just(Token::Neq), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Neq, vec![l, r])
            }),
            infix(right(5), just(Token::Lt), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Lt, vec![l, r])
            }),
            infix(right(5), just(Token::Lte), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Lte, vec![l, r])
            }),
            infix(right(5), just(Token::Gt), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Gt, vec![l, r])
            }),
            infix(right(5), just(Token::Gte), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Gte, vec![l, r])
            }),
            // Precedence 6: string concat
            infix(right(6), just(Token::PlusPlus), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Concat, vec![l, r])
            }),
            // Precedence 7: addition/subtraction
            infix(left(7), just(Token::Plus), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Add, vec![l, r])
            }),
            infix(left(7), just(Token::Minus), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Sub, vec![l, r])
            }),
            // Precedence 8: multiplication/division
            infix(left(8), just(Token::Star), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Mul, vec![l, r])
            }),
            infix(left(8), just(Token::Slash), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Div, vec![l, r])
            }),
            infix(left(8), just(Token::Percent), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Mod, vec![l, r])
            }),
            infix(left(8), just(Token::ModKw), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Mod, vec![l, r])
            }),
            infix(left(8), just(Token::DivKw), |l, _, r, _| {
                Expr::Builtin(BuiltinOp::Div, vec![l, r])
            }),
            // Precedence 9: unary prefix
            prefix(9, just(Token::Minus), |_, rhs, _| {
                Expr::Builtin(BuiltinOp::Neg, vec![rhs])
            }),
            prefix(9, just(Token::Not), |_, rhs, _| {
                Expr::Builtin(BuiltinOp::Not, vec![rhs])
            }),
        ));

        // ── Compound expressions ────────────────────────────

        // Lambda: \x y -> body
        let lambda = just(Token::Backslash)
            .ignore_then(
                pattern
                    .clone()
                    .repeated()
                    .at_least(1)
                    .collect::<Vec<Pattern>>(),
            )
            .then_ignore(just(Token::Arrow))
            .then(expr.clone())
            .map(|(params, body): (Vec<Pattern>, Expr)| desugar_lambda(&params, body));

        // Let binding
        let let_bind = ident()
            .then(pattern.clone().repeated().collect::<Vec<Pattern>>())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|((name, params), val): ((Arc<str>, Vec<Pattern>), Expr)| {
                if params.is_empty() {
                    (name, val)
                } else {
                    (name, desugar_lambda(&params, val))
                }
            });

        let let_expr = just(Token::Let)
            .ignore_then(layout_block(let_bind.clone()).or(let_bind.clone().map(|b| vec![b])))
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .map(|(binds, body)| desugar_let_binds(binds, body));

        // If-then-else
        let if_expr = just(Token::If)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::Then))
            .then(expr.clone())
            .then_ignore(just(Token::Else))
            .then(expr.clone())
            .map(|((cond, then_branch), else_branch)| Expr::Match {
                scrutinee: Box::new(cond),
                arms: vec![
                    (Pattern::Lit(Literal::Bool(true)), then_branch),
                    (Pattern::Wildcard, else_branch),
                ],
            });

        // Case-of
        let case_arm = pattern
            .clone()
            .then_ignore(just(Token::Arrow))
            .then(expr.clone());

        let case_expr = just(Token::Case)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::Of))
            .then(layout_block(case_arm))
            .map(|(scrutinee, arms)| Expr::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            });

        // Do-notation
        let do_stmt = choice((
            ident()
                .then_ignore(just(Token::LeftArrow))
                .then(expr.clone())
                .map(|(name, e): (Arc<str>, Expr)| DoStmt::Bind(name, e)),
            just(Token::Let)
                .ignore_then(let_bind.clone())
                .map(|(name, val)| DoStmt::Let(name, val)),
            expr.clone().map(DoStmt::Expr),
        ));

        let do_expr = just(Token::Do)
            .ignore_then(layout_block(do_stmt))
            .map(desugar_do);

        // ── Combine all expression forms ────────────────────

        let full_expr = choice((do_expr, let_expr, if_expr, case_expr, lambda, pratt));

        // Where clause as postfix
        let where_bind = ident()
            .then(pattern.repeated().collect::<Vec<Pattern>>())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|((name, params), val): ((Arc<str>, Vec<Pattern>), Expr)| {
                if params.is_empty() {
                    (name, val)
                } else {
                    (name, desugar_lambda(&params, val))
                }
            });

        let where_clause = just(Token::Where)
            .ignore_then(layout_block(where_bind.clone()).or(where_bind.map(|b| vec![b])));

        full_expr
            .then(where_clause.or_not())
            .map(|(body, where_binds)| match where_binds {
                Some(binds) => desugar_let_binds(binds, body),
                None => body,
            })
    })
}

// ── Helper types ────────────────────────────────────────────────────

/// Postfix operation.
#[derive(Debug, Clone)]
enum PostfixOp {
    /// `.field`
    Field(Arc<str>),
    /// `->edge`
    Edge(Arc<str>),
}

/// List comprehension qualifier.
#[derive(Debug, Clone)]
enum Qual {
    /// `x <- xs`
    Generator(Arc<str>, Expr),
    /// Predicate.
    Guard(Expr),
}

/// Do-notation statement.
#[derive(Debug, Clone)]
enum DoStmt {
    /// `x <- e`
    Bind(Arc<str>, Expr),
    /// `let x = e`
    Let(Arc<str>, Expr),
    /// Bare expression.
    Expr(Expr),
}

// ── Desugaring helpers ──────────────────────────────────────────────

/// Desugar `\p1 p2 ... -> body` into nested lambdas.
fn desugar_lambda(params: &[Pattern], body: Expr) -> Expr {
    params.iter().rev().fold(body, |acc, pat| match pat {
        Pattern::Var(name) => Expr::Lam(name.clone(), Box::new(acc)),
        Pattern::Wildcard => Expr::Lam(Arc::from("_"), Box::new(acc)),
        other => {
            let fresh: Arc<str> = Arc::from("_arg");
            Expr::Lam(
                fresh.clone(),
                Box::new(Expr::Match {
                    scrutinee: Box::new(Expr::Var(fresh)),
                    arms: vec![(other.clone(), acc)],
                }),
            )
        }
    })
}

/// Desugar `let a = e1; b = e2 in body` into nested `Let`.
fn desugar_let_binds(binds: Vec<(Arc<str>, Expr)>, body: Expr) -> Expr {
    binds
        .into_iter()
        .rev()
        .fold(body, |acc, (name, val)| Expr::Let {
            name,
            value: Box::new(val),
            body: Box::new(acc),
        })
}

/// Desugar list comprehension `[e | quals]` into `flatMap`/guard.
fn desugar_comprehension(body: Expr, quals: &[Qual]) -> Expr {
    quals
        .iter()
        .rev()
        .fold(Expr::List(vec![body]), |acc, qual| match qual {
            Qual::Generator(name, source) => Expr::Builtin(
                BuiltinOp::FlatMap,
                vec![source.clone(), Expr::Lam(name.clone(), Box::new(acc))],
            ),
            Qual::Guard(pred) => Expr::Match {
                scrutinee: Box::new(pred.clone()),
                arms: vec![
                    (Pattern::Lit(Literal::Bool(true)), acc),
                    (Pattern::Wildcard, Expr::List(vec![])),
                ],
            },
        })
}

/// Desugar do-notation into nested `flatMap`/`let`.
fn desugar_do(stmts: Vec<DoStmt>) -> Expr {
    if stmts.is_empty() {
        return Expr::List(vec![]);
    }
    let mut iter = stmts.into_iter().rev();
    // Safety: we checked `is_empty()` above, so `next()` always returns `Some`.
    let Some(last) = iter.next() else {
        return Expr::List(vec![]);
    };
    let init = match last {
        DoStmt::Expr(e) | DoStmt::Bind(_, e) => e,
        DoStmt::Let(name, val) => Expr::Let {
            name,
            value: Box::new(val),
            body: Box::new(Expr::List(vec![])),
        },
    };
    iter.fold(init, |acc, stmt| match stmt {
        DoStmt::Bind(name, source) => Expr::Builtin(
            BuiltinOp::FlatMap,
            vec![source, Expr::Lam(name, Box::new(acc))],
        ),
        DoStmt::Let(name, val) => Expr::Let {
            name,
            value: Box::new(val),
            body: Box::new(acc),
        },
        DoStmt::Expr(e) => Expr::Builtin(
            BuiltinOp::FlatMap,
            vec![e, Expr::Lam(Arc::from("_"), Box::new(acc))],
        ),
    })
}

/// Permute a higher-order list builtin's arguments from surface order
/// into evaluator order.
///
/// The surface syntax follows the usual functional convention of naming
/// the function first (`map f xs`, `fold f z xs`), while [`Expr::Builtin`]
/// takes the list first and the function last. The two orders are
/// deliberately distinct: `Expr` is serialized into stored lens
/// documents, so its argument order is the compatibility-bearing one and
/// the surface syntax lowers into it.
///
/// Applied only once the builtin is saturated, since a partial
/// application has no complete order to permute. Builtins outside this
/// set take their arguments in the same order at both layers and pass
/// through untouched.
fn lower_list_builtin_args(op: BuiltinOp, args: Vec<Expr>) -> Vec<Expr> {
    match (op, args.len()) {
        // `map f xs` / `filter p xs` / `flat_map f xs` -> [xs, f]
        (BuiltinOp::Map | BuiltinOp::Filter | BuiltinOp::FlatMap, 2) => {
            let mut args = args;
            args.swap(0, 1);
            args
        }
        // `fold f z xs` -> [xs, z, f]
        (BuiltinOp::Fold, 3) => {
            let mut args = args;
            args.swap(0, 2);
            args
        }
        _ => args,
    }
}

/// Resolve function application, detecting builtin names.
fn resolve_application(func: Expr, arg: Expr) -> Expr {
    match &func {
        Expr::Var(name) => {
            if let Some(op) = resolve_builtin(name) {
                let args = lower_list_builtin_args(op, vec![arg]);
                Expr::Builtin(op, args)
            } else {
                Expr::App(Box::new(func), Box::new(arg))
            }
        }
        Expr::Builtin(op, args) if args.len() < op.arity() => {
            let mut new_args = args.clone();
            new_args.push(arg);
            Expr::Builtin(*op, lower_list_builtin_args(*op, new_args))
        }
        _ => Expr::App(Box::new(func), Box::new(arg)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenize;

    fn parse_ok(input: &str) -> Expr {
        let tokens = tokenize(input).unwrap_or_else(|e| panic!("lex failed: {e}"));
        parse(&tokens).unwrap_or_else(|e| panic!("parse failed: {e:?}"))
    }

    #[test]
    fn parse_literal_int() {
        assert_eq!(parse_ok("42"), Expr::Lit(Literal::Int(42)));
    }

    #[test]
    fn parse_literal_string() {
        assert_eq!(
            parse_ok(r#""hello""#),
            Expr::Lit(Literal::Str("hello".into()))
        );
    }

    #[test]
    fn parse_literal_bool() {
        assert_eq!(parse_ok("True"), Expr::Lit(Literal::Bool(true)));
        assert_eq!(parse_ok("False"), Expr::Lit(Literal::Bool(false)));
    }

    #[test]
    fn parse_nothing() {
        assert_eq!(parse_ok("Nothing"), Expr::Lit(Literal::Null));
    }

    #[test]
    fn parse_variable() {
        assert_eq!(parse_ok("x"), Expr::Var(Arc::from("x")));
    }

    #[test]
    fn parse_arithmetic() {
        assert_eq!(
            parse_ok("1 + 2"),
            Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(2))]
            )
        );
    }

    #[test]
    fn parse_precedence() {
        assert_eq!(
            parse_ok("1 + 2 * 3"),
            Expr::Builtin(
                BuiltinOp::Add,
                vec![
                    Expr::Lit(Literal::Int(1)),
                    Expr::Builtin(
                        BuiltinOp::Mul,
                        vec![Expr::Lit(Literal::Int(2)), Expr::Lit(Literal::Int(3))]
                    ),
                ]
            )
        );
    }

    #[test]
    fn parse_comparison() {
        assert_eq!(
            parse_ok("x == 1"),
            Expr::Builtin(
                BuiltinOp::Eq,
                vec![Expr::Var(Arc::from("x")), Expr::Lit(Literal::Int(1))]
            )
        );
    }

    #[test]
    fn parse_logical() {
        assert_eq!(
            parse_ok("a && b || c"),
            Expr::Builtin(
                BuiltinOp::Or,
                vec![
                    Expr::Builtin(
                        BuiltinOp::And,
                        vec![Expr::Var(Arc::from("a")), Expr::Var(Arc::from("b"))]
                    ),
                    Expr::Var(Arc::from("c")),
                ]
            )
        );
    }

    #[test]
    fn parse_negation() {
        assert_eq!(
            parse_ok("-x"),
            Expr::Builtin(BuiltinOp::Neg, vec![Expr::Var(Arc::from("x"))])
        );
    }

    #[test]
    fn parse_not() {
        assert_eq!(
            parse_ok("not True"),
            Expr::Builtin(BuiltinOp::Not, vec![Expr::Lit(Literal::Bool(true))])
        );
    }

    #[test]
    fn parse_field_access() {
        assert_eq!(
            parse_ok("x.name"),
            Expr::Field(Box::new(Expr::Var(Arc::from("x"))), Arc::from("name"))
        );
    }

    #[test]
    fn parse_edge_traversal() {
        assert_eq!(
            parse_ok("doc -> layers"),
            Expr::Builtin(
                BuiltinOp::Edge,
                vec![
                    Expr::Var(Arc::from("doc")),
                    Expr::Lit(Literal::Str("layers".into())),
                ]
            )
        );
    }

    #[test]
    fn parse_lambda() {
        assert_eq!(
            parse_ok("\\x -> x + 1"),
            Expr::Lam(
                Arc::from("x"),
                Box::new(Expr::Builtin(
                    BuiltinOp::Add,
                    vec![Expr::Var(Arc::from("x")), Expr::Lit(Literal::Int(1))]
                ))
            )
        );
    }

    #[test]
    fn parse_multi_param_lambda() {
        let e = parse_ok("\\x y -> x + y");
        match &e {
            Expr::Lam(x, inner) => {
                assert_eq!(&**x, "x");
                assert!(matches!(&**inner, Expr::Lam(y, _) if &**y == "y"));
            }
            _ => panic!("expected nested Lam, got {e:?}"),
        }
    }

    #[test]
    fn parse_let_in() {
        assert_eq!(
            parse_ok("let x = 1 in x + 1"),
            Expr::Let {
                name: Arc::from("x"),
                value: Box::new(Expr::Lit(Literal::Int(1))),
                body: Box::new(Expr::Builtin(
                    BuiltinOp::Add,
                    vec![Expr::Var(Arc::from("x")), Expr::Lit(Literal::Int(1))]
                )),
            }
        );
    }

    #[test]
    fn parse_if_then_else() {
        let e = parse_ok("if True then 1 else 0");
        assert!(matches!(e, Expr::Match { .. }));
    }

    #[test]
    fn parse_case_of() {
        let e = parse_ok("case x of\n  True -> 1\n  False -> 0");
        match e {
            Expr::Match { arms, .. } => assert_eq!(arms.len(), 2),
            _ => panic!("expected Match"),
        }
    }

    #[test]
    fn parse_list() {
        assert_eq!(
            parse_ok("[1, 2, 3]"),
            Expr::List(vec![
                Expr::Lit(Literal::Int(1)),
                Expr::Lit(Literal::Int(2)),
                Expr::Lit(Literal::Int(3)),
            ])
        );
    }

    #[test]
    fn parse_empty_list() {
        assert_eq!(parse_ok("[]"), Expr::List(vec![]));
    }

    #[test]
    fn parse_record() {
        assert_eq!(
            parse_ok("{ name = x, age = 30 }"),
            Expr::Record(vec![
                (Arc::from("name"), Expr::Var(Arc::from("x"))),
                (Arc::from("age"), Expr::Lit(Literal::Int(30))),
            ])
        );
    }

    #[test]
    fn parse_record_punning() {
        assert_eq!(
            parse_ok("{ name, age }"),
            Expr::Record(vec![
                (Arc::from("name"), Expr::Var(Arc::from("name"))),
                (Arc::from("age"), Expr::Var(Arc::from("age"))),
            ])
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn ranges_lower_to_the_range_builtin_and_evaluate() {
        let config = panproto_expr::EvalConfig::default();
        let eval =
            |src: &str| panproto_expr::eval(&parse_ok(src), &panproto_expr::Env::new(), &config);

        assert_eq!(
            parse_ok("[1..3]"),
            Expr::Builtin(
                BuiltinOp::Range,
                vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(3)),]
            )
        );
        assert_eq!(
            eval("[1..3]").expect("range should evaluate"),
            panproto_expr::Literal::List(vec![
                panproto_expr::Literal::Int(1),
                panproto_expr::Literal::Int(2),
                panproto_expr::Literal::Int(3),
            ]),
            "both bounds are inclusive"
        );
        assert_eq!(
            eval("[0..0]").expect("singleton range should evaluate"),
            panproto_expr::Literal::List(vec![panproto_expr::Literal::Int(0)])
        );
        assert_eq!(
            eval("[3..1]").expect("descending range should evaluate"),
            panproto_expr::Literal::List(vec![]),
            "a descending range is empty, not an error"
        );
        // `range a b` names the same builtin as the bracket syntax.
        assert_eq!(parse_ok("range 1 3"), parse_ok("[1..3]"));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn ranges_compose_with_the_list_builtins() {
        let config = panproto_expr::EvalConfig::default();
        let eval =
            |src: &str| panproto_expr::eval(&parse_ok(src), &panproto_expr::Env::new(), &config);

        assert_eq!(
            eval("map (\\x -> x * x) [1..4]").expect("map over a range should evaluate"),
            panproto_expr::Literal::List(vec![
                panproto_expr::Literal::Int(1),
                panproto_expr::Literal::Int(4),
                panproto_expr::Literal::Int(9),
                panproto_expr::Literal::Int(16),
            ])
        );
        assert_eq!(
            eval("fold (\\a -> \\b -> a + b) 0 [1..100]").expect("fold over a range"),
            panproto_expr::Literal::Int(5050)
        );
    }

    #[test]
    fn an_oversized_range_is_rejected_before_it_allocates() {
        // Range is the one builtin that turns a constant-size expression
        // into an arbitrarily long list, so its length is checked against
        // the list budget rather than discovered after allocating.
        let config = panproto_expr::EvalConfig::default();
        let result = panproto_expr::eval(
            &parse_ok("[0..99999999]"),
            &panproto_expr::Env::new(),
            &config,
        );
        assert!(
            matches!(result, Err(panproto_expr::ExprError::ListLengthExceeded(_))),
            "expected ListLengthExceeded, got {result:?}"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn an_open_ended_range_is_rejected() {
        // There are no lazy lists to lower `[1..]` to. It previously
        // parsed to the one-element list `[1]`, which is a wrong answer
        // rather than a missing feature.
        let tokens = tokenize("[1..]").unwrap_or_else(|e| panic!("lex failed: {e}"));
        let errs = parse(&tokens).expect_err("an open-ended range must not parse");
        let rendered = errs
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            rendered.contains("open-ended range"),
            "the error should name the construct, got: {rendered}"
        );
    }

    #[test]
    fn parse_builtin_application() {
        // Surface order is `map f xs`; the stored form is list-first,
        // which is the order `eval_map` reads.
        assert_eq!(
            parse_ok("map f xs"),
            Expr::Builtin(
                BuiltinOp::Map,
                vec![Expr::Var(Arc::from("xs")), Expr::Var(Arc::from("f"))]
            )
        );
    }

    #[test]
    fn parse_fold_lowers_list_first() {
        assert_eq!(
            parse_ok("fold f z xs"),
            Expr::Builtin(
                BuiltinOp::Fold,
                vec![
                    Expr::Var(Arc::from("xs")),
                    Expr::Var(Arc::from("z")),
                    Expr::Var(Arc::from("f")),
                ]
            )
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn parsed_list_builtins_evaluate() {
        // The lowering exists so that surface-authored list expressions
        // actually run: before it, every `map` / `filter` / `fold` failed
        // with `expected list, got function`.
        let env = panproto_expr::Env::new().extend(
            Arc::from("xs"),
            panproto_expr::Literal::List(vec![
                panproto_expr::Literal::Int(1),
                panproto_expr::Literal::Int(2),
                panproto_expr::Literal::Int(3),
            ]),
        );
        let config = panproto_expr::EvalConfig::default();
        let eval = |src: &str| panproto_expr::eval(&parse_ok(src), &env, &config);

        assert_eq!(
            eval("map (\\x -> x * 2) xs").expect("map should evaluate"),
            panproto_expr::Literal::List(vec![
                panproto_expr::Literal::Int(2),
                panproto_expr::Literal::Int(4),
                panproto_expr::Literal::Int(6),
            ])
        );
        assert_eq!(
            eval("filter (\\x -> x > 1) xs").expect("filter should evaluate"),
            panproto_expr::Literal::List(vec![
                panproto_expr::Literal::Int(2),
                panproto_expr::Literal::Int(3),
            ])
        );
        assert_eq!(
            eval("fold (\\a -> \\b -> a + b) 0 xs").expect("fold should evaluate"),
            panproto_expr::Literal::Int(6)
        );
    }

    #[test]
    fn parse_string_concat() {
        assert_eq!(
            parse_ok(r#""hello" ++ " world""#),
            Expr::Builtin(
                BuiltinOp::Concat,
                vec![
                    Expr::Lit(Literal::Str("hello".into())),
                    Expr::Lit(Literal::Str(" world".into())),
                ]
            )
        );
    }

    #[test]
    fn parse_pipe() {
        assert_eq!(
            parse_ok("x & f"),
            Expr::App(
                Box::new(Expr::Var(Arc::from("f"))),
                Box::new(Expr::Var(Arc::from("x"))),
            )
        );
    }

    #[test]
    fn parse_chained_field_access() {
        assert_eq!(
            parse_ok("x.a.b"),
            Expr::Field(
                Box::new(Expr::Field(
                    Box::new(Expr::Var(Arc::from("x"))),
                    Arc::from("a"),
                )),
                Arc::from("b"),
            )
        );
    }

    #[test]
    fn parse_comprehension() {
        let e = parse_ok("[ x + 1 | x <- xs ]");
        assert!(matches!(e, Expr::Builtin(BuiltinOp::FlatMap, _)));
    }
}
