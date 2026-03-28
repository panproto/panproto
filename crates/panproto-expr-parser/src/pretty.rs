//! Pretty printer for panproto expressions.
//!
//! Converts `panproto_expr::Expr` back into Haskell-style surface syntax.
//! The output is designed to round-trip through the parser:
//! `parse(tokenize(pretty_print(&e))) == e` for well-formed expressions.
//!
//! Parentheses are minimized using precedence awareness, and operators
//! are printed in infix notation where the parser supports it.

use std::fmt::Write;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};

/// Precedence levels (higher binds tighter).
///
/// These mirror the Pratt parser precedences in `parser.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    /// Top level: no parens needed.
    Top = 0,
    /// Pipe operator (`&`).
    Pipe = 1,
    /// Logical or (`||`).
    Or = 3,
    /// Logical and (`&&`).
    And = 4,
    /// Comparison (`==`, `/=`, `<`, `<=`, `>`, `>=`).
    Cmp = 5,
    /// Concatenation (`++`).
    Concat = 6,
    /// Addition and subtraction (`+`, `-`).
    AddSub = 7,
    /// Multiplication, division, modulo (`*`, `/`, `%`, `mod`, `div`).
    MulDiv = 8,
    /// Unary prefix (`-`, `not`).
    Unary = 9,
    /// Function application.
    App = 10,
    /// Postfix (`.field`, `->edge`), atoms.
    Atom = 11,
}

/// Associativity of a binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Assoc {
    Left,
    Right,
}

/// Pretty print an expression to a string.
///
/// The output uses Haskell-style surface syntax with minimal parentheses.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use panproto_expr::{Expr, Literal, BuiltinOp};
/// use panproto_expr_parser::pretty_print;
///
/// let e = Expr::Builtin(BuiltinOp::Add, vec![
///     Expr::Var(Arc::from("x")),
///     Expr::Lit(Literal::Int(1)),
/// ]);
/// assert_eq!(pretty_print(&e), "x + 1");
/// ```
#[must_use]
pub fn pretty_print(expr: &Expr) -> String {
    let mut buf = String::new();
    write_expr(&mut buf, expr, Prec::Top);
    buf
}

/// Write an expression at the given precedence context.
///
/// If the expression's own precedence is lower than `ctx`, wraps in parens.
fn write_expr(buf: &mut String, expr: &Expr, ctx: Prec) {
    match expr {
        Expr::Var(name) => buf.push_str(name),

        Expr::Lit(lit) => write_literal(buf, lit),

        Expr::Lam(param, body) => {
            let needs_parens = ctx > Prec::Top;
            if needs_parens {
                buf.push('(');
            }
            write_lambda_chain(buf, param, body);
            if needs_parens {
                buf.push(')');
            }
        }

        Expr::App(func, arg) => {
            write_app(buf, expr, ctx);
            let _ = (func, arg); // used inside write_app
        }

        Expr::Record(fields) => {
            write_record_expr(buf, fields);
        }

        Expr::List(items) => {
            buf.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                write_expr(buf, item, Prec::Top);
            }
            buf.push(']');
        }

        Expr::Field(inner, name) => {
            write_expr(buf, inner, Prec::Atom);
            buf.push('.');
            buf.push_str(name);
        }

        Expr::Index(inner, idx) => {
            write_expr(buf, inner, Prec::Atom);
            buf.push('[');
            write_expr(buf, idx, Prec::Top);
            buf.push(']');
        }

        Expr::Match { scrutinee, arms } => {
            write_match(buf, scrutinee, arms, ctx);
        }

        Expr::Let { name, value, body } => {
            write_let(buf, name, value, body, ctx);
        }

        Expr::Builtin(op, args) => {
            write_builtin(buf, *op, args, ctx);
        }
    }
}

/// Write a chain of nested lambdas as `\x y z -> body`.
fn write_lambda_chain(buf: &mut String, first_param: &Arc<str>, first_body: &Expr) {
    buf.push('\\');
    buf.push_str(first_param);
    let mut body = first_body;
    while let Expr::Lam(param, inner) = body {
        buf.push(' ');
        buf.push_str(param);
        body = inner;
    }
    buf.push_str(" -> ");
    write_expr(buf, body, Prec::Top);
}

/// Write function application, collecting curried args: `f x y z`.
fn write_app(buf: &mut String, expr: &Expr, ctx: Prec) {
    let needs_parens = ctx > Prec::App;
    if needs_parens {
        buf.push('(');
    }

    // Collect the application spine.
    let mut spine: Vec<&Expr> = Vec::new();
    let mut head = expr;
    while let Expr::App(func, arg) = head {
        spine.push(arg);
        head = func;
    }
    spine.reverse();

    write_expr(buf, head, Prec::App);
    for arg in &spine {
        buf.push(' ');
        write_expr(buf, arg, Prec::Atom);
    }

    if needs_parens {
        buf.push(')');
    }
}

/// Write a record expression with punning where appropriate.
fn write_record_expr(buf: &mut String, fields: &[(Arc<str>, Expr)]) {
    buf.push_str("{ ");
    for (i, (name, val)) in fields.iter().enumerate() {
        if i > 0 {
            buf.push_str(", ");
        }
        // Record punning: `{ x }` when field name equals variable name.
        if let Expr::Var(v) = val {
            if v == name {
                buf.push_str(name);
                continue;
            }
        }
        buf.push_str(name);
        buf.push_str(" = ");
        write_expr(buf, val, Prec::Top);
    }
    buf.push_str(" }");
}

/// Write a match expression.
///
/// Detects `if/then/else` patterns (two arms with True and Wildcard)
/// and emits those in the shorter form.
fn write_match(buf: &mut String, scrutinee: &Expr, arms: &[(Pattern, Expr)], ctx: Prec) {
    // Detect if/then/else: Match with [Lit(Bool(true)) -> then, Wildcard -> else]
    if arms.len() == 2 {
        if let (Pattern::Lit(Literal::Bool(true)), then_branch) = &arms[0] {
            if let (Pattern::Wildcard, else_branch) = &arms[1] {
                let needs_parens = ctx > Prec::Top;
                if needs_parens {
                    buf.push('(');
                }
                buf.push_str("if ");
                write_expr(buf, scrutinee, Prec::Top);
                buf.push_str(" then ");
                write_expr(buf, then_branch, Prec::Top);
                buf.push_str(" else ");
                write_expr(buf, else_branch, Prec::Top);
                if needs_parens {
                    buf.push(')');
                }
                return;
            }
        }
    }

    let needs_parens = ctx > Prec::Top;
    if needs_parens {
        buf.push('(');
    }
    buf.push_str("case ");
    write_expr(buf, scrutinee, Prec::Top);
    buf.push_str(" of\n");
    for (i, (pat, body)) in arms.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }
        buf.push_str("  ");
        write_pattern(buf, pat);
        buf.push_str(" -> ");
        write_expr(buf, body, Prec::Top);
    }
    if needs_parens {
        buf.push(')');
    }
}

/// Write a let binding, collapsing nested lets into a layout block.
fn write_let(buf: &mut String, name: &Arc<str>, value: &Expr, body: &Expr, ctx: Prec) {
    let needs_parens = ctx > Prec::Top;
    if needs_parens {
        buf.push('(');
    }

    // Collect chained lets.
    let mut bindings: Vec<(&Arc<str>, &Expr)> = vec![(name, value)];
    let mut final_body = body;
    while let Expr::Let {
        name: n,
        value: v,
        body: b,
    } = final_body
    {
        bindings.push((n, v));
        final_body = b;
    }

    if bindings.len() == 1 {
        buf.push_str("let ");
        buf.push_str(name);
        buf.push_str(" = ");
        write_expr(buf, value, Prec::Top);
        buf.push_str(" in ");
    } else {
        buf.push_str("let\n");
        for (n, v) in &bindings {
            buf.push_str("  ");
            buf.push_str(n);
            buf.push_str(" = ");
            write_expr(buf, v, Prec::Top);
            buf.push('\n');
        }
        buf.push_str("in ");
    }
    write_expr(buf, final_body, Prec::Top);

    if needs_parens {
        buf.push(')');
    }
}

/// Write a builtin operation, using infix/prefix syntax where possible.
fn write_builtin(buf: &mut String, op: BuiltinOp, args: &[Expr], ctx: Prec) {
    // Try infix binary operators.
    if let Some((sym, prec, assoc)) = infix_info(op) {
        if args.len() == 2 {
            let needs_parens = ctx > prec;
            if needs_parens {
                buf.push('(');
            }
            // For left-associative operators, the left child is fine at the
            // same precedence but the right child needs to be tighter (to
            // avoid ambiguity). Vice versa for right-associative.
            let (left_ctx, right_ctx) = match assoc {
                Assoc::Left => (prec, next_prec(prec)),
                Assoc::Right => (next_prec(prec), prec),
            };
            write_expr(buf, &args[0], left_ctx);
            buf.push(' ');
            buf.push_str(sym);
            buf.push(' ');
            write_expr(buf, &args[1], right_ctx);
            if needs_parens {
                buf.push(')');
            }
            return;
        }
    }

    // Edge traversal: `expr -> edge`
    if op == BuiltinOp::Edge && args.len() == 2 {
        if let Expr::Lit(Literal::Str(edge_name)) = &args[1] {
            let needs_parens = ctx > Prec::Atom;
            if needs_parens {
                buf.push('(');
            }
            write_expr(buf, &args[0], Prec::Atom);
            buf.push_str(" -> ");
            buf.push_str(edge_name);
            if needs_parens {
                buf.push(')');
            }
            return;
        }
    }

    // Unary prefix: negation and logical not.
    if op == BuiltinOp::Neg && args.len() == 1 {
        let needs_parens = ctx > Prec::Unary;
        if needs_parens {
            buf.push('(');
        }
        buf.push('-');
        write_expr(buf, &args[0], Prec::Atom);
        if needs_parens {
            buf.push(')');
        }
        return;
    }

    if op == BuiltinOp::Not && args.len() == 1 {
        let needs_parens = ctx > Prec::Unary;
        if needs_parens {
            buf.push('(');
        }
        buf.push_str("not ");
        write_expr(buf, &args[0], Prec::Atom);
        if needs_parens {
            buf.push(')');
        }
        return;
    }

    // Fallback: function call syntax `name arg1 arg2 ...`
    let needs_parens = ctx > Prec::App && !args.is_empty();
    if needs_parens {
        buf.push('(');
    }
    buf.push_str(builtin_name(op));
    for arg in args {
        buf.push(' ');
        write_expr(buf, arg, Prec::Atom);
    }
    if needs_parens {
        buf.push(')');
    }
}

/// Map a builtin op to its infix operator symbol, precedence, and associativity.
///
/// Returns `None` for builtins that should use function call syntax.
const fn infix_info(op: BuiltinOp) -> Option<(&'static str, Prec, Assoc)> {
    match op {
        BuiltinOp::Or => Some(("||", Prec::Or, Assoc::Left)),
        BuiltinOp::And => Some(("&&", Prec::And, Assoc::Left)),
        BuiltinOp::Eq => Some(("==", Prec::Cmp, Assoc::Right)),
        BuiltinOp::Neq => Some(("/=", Prec::Cmp, Assoc::Right)),
        BuiltinOp::Lt => Some(("<", Prec::Cmp, Assoc::Right)),
        BuiltinOp::Lte => Some(("<=", Prec::Cmp, Assoc::Right)),
        BuiltinOp::Gt => Some((">", Prec::Cmp, Assoc::Right)),
        BuiltinOp::Gte => Some((">=", Prec::Cmp, Assoc::Right)),
        BuiltinOp::Concat => Some(("++", Prec::Concat, Assoc::Right)),
        BuiltinOp::Add => Some(("+", Prec::AddSub, Assoc::Left)),
        BuiltinOp::Sub => Some(("-", Prec::AddSub, Assoc::Left)),
        BuiltinOp::Mul => Some(("*", Prec::MulDiv, Assoc::Left)),
        BuiltinOp::Div => Some(("/", Prec::MulDiv, Assoc::Left)),
        BuiltinOp::Mod => Some(("%", Prec::MulDiv, Assoc::Left)),
        _ => None,
    }
}

/// Get the next higher precedence level.
const fn next_prec(p: Prec) -> Prec {
    match p {
        Prec::Top => Prec::Pipe,
        Prec::Pipe => Prec::Or,
        Prec::Or => Prec::And,
        Prec::And => Prec::Cmp,
        Prec::Cmp => Prec::Concat,
        Prec::Concat => Prec::AddSub,
        Prec::AddSub => Prec::MulDiv,
        Prec::MulDiv => Prec::Unary,
        Prec::Unary => Prec::App,
        Prec::App | Prec::Atom => Prec::Atom,
    }
}

/// Map a builtin op to its canonical function name for call syntax.
const fn builtin_name(op: BuiltinOp) -> &'static str {
    match op {
        BuiltinOp::Add => "add",
        BuiltinOp::Sub => "sub",
        BuiltinOp::Mul => "mul",
        BuiltinOp::Div => "div",
        BuiltinOp::Mod => "mod",
        BuiltinOp::Neg => "neg",
        BuiltinOp::Abs => "abs",
        BuiltinOp::Floor => "floor",
        BuiltinOp::Ceil => "ceil",
        BuiltinOp::Round => "round",
        BuiltinOp::Eq => "eq",
        BuiltinOp::Neq => "neq",
        BuiltinOp::Lt => "lt",
        BuiltinOp::Lte => "lte",
        BuiltinOp::Gt => "gt",
        BuiltinOp::Gte => "gte",
        BuiltinOp::And => "and",
        BuiltinOp::Or => "or",
        BuiltinOp::Not => "not",
        BuiltinOp::Concat => "concat",
        BuiltinOp::Len => "len",
        BuiltinOp::Slice => "slice",
        BuiltinOp::Upper => "upper",
        BuiltinOp::Lower => "lower",
        BuiltinOp::Trim => "trim",
        BuiltinOp::Split => "split",
        BuiltinOp::Join => "join",
        BuiltinOp::Replace => "replace",
        BuiltinOp::Contains => "contains",
        BuiltinOp::Map => "map",
        BuiltinOp::Filter => "filter",
        BuiltinOp::Fold => "fold",
        BuiltinOp::Append => "append",
        BuiltinOp::Head => "head",
        BuiltinOp::Tail => "tail",
        BuiltinOp::Reverse => "reverse",
        BuiltinOp::FlatMap => "flat_map",
        BuiltinOp::Length => "length",
        BuiltinOp::MergeRecords => "merge",
        BuiltinOp::Keys => "keys",
        BuiltinOp::Values => "values",
        BuiltinOp::HasField => "has_field",
        BuiltinOp::DefaultVal => "default",
        BuiltinOp::Clamp => "clamp",
        BuiltinOp::TruncateStr => "truncate_str",
        BuiltinOp::IntToFloat => "int_to_float",
        BuiltinOp::FloatToInt => "float_to_int",
        BuiltinOp::IntToStr => "int_to_str",
        BuiltinOp::FloatToStr => "float_to_str",
        BuiltinOp::StrToInt => "str_to_int",
        BuiltinOp::StrToFloat => "str_to_float",
        BuiltinOp::TypeOf => "type_of",
        BuiltinOp::IsNull => "is_null",
        BuiltinOp::IsList => "is_list",
        BuiltinOp::Edge => "edge",
        BuiltinOp::Children => "children",
        BuiltinOp::HasEdge => "has_edge",
        BuiltinOp::EdgeCount => "edge_count",
        BuiltinOp::Anchor => "anchor",
    }
}

/// Write a literal value.
fn write_literal(buf: &mut String, lit: &Literal) {
    match lit {
        Literal::Bool(true) => buf.push_str("True"),
        Literal::Bool(false) => buf.push_str("False"),
        Literal::Int(n) => {
            let _ = write!(buf, "{n}");
        }
        Literal::Float(f) => {
            // Ensure there is always a decimal point so the parser
            // recognizes this as a float, not an int.
            let s = format!("{f}");
            if s.contains('.') {
                buf.push_str(&s);
            } else {
                let _ = write!(buf, "{f}.0");
            }
        }
        Literal::Str(s) => {
            buf.push('"');
            // Escape backslashes and double quotes.
            for ch in s.chars() {
                match ch {
                    '\\' => buf.push_str("\\\\"),
                    '"' => buf.push_str("\\\""),
                    '\n' => buf.push_str("\\n"),
                    '\r' => buf.push_str("\\r"),
                    '\t' => buf.push_str("\\t"),
                    c => buf.push(c),
                }
            }
            buf.push('"');
        }
        Literal::Bytes(bytes) => {
            // No native bytes syntax; emit as a list of ints.
            buf.push('[');
            for (i, b) in bytes.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                let _ = write!(buf, "{b}");
            }
            buf.push(']');
        }
        Literal::Null => buf.push_str("Nothing"),
        Literal::Record(fields) => {
            buf.push_str("{ ");
            for (i, (name, val)) in fields.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                buf.push_str(name);
                buf.push_str(" = ");
                write_literal(buf, val);
            }
            buf.push_str(" }");
        }
        Literal::List(items) => {
            buf.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                write_literal(buf, item);
            }
            buf.push(']');
        }
        Literal::Closure { param, body, .. } => {
            // Print as a lambda; the captured env is lost but the
            // expression form is preserved for round-tripping.
            buf.push('\\');
            buf.push_str(param);
            buf.push_str(" -> ");
            write_expr(buf, body, Prec::Top);
        }
    }
}

/// Write a pattern.
fn write_pattern(buf: &mut String, pat: &Pattern) {
    match pat {
        Pattern::Wildcard => buf.push('_'),
        Pattern::Var(name) => buf.push_str(name),
        Pattern::Lit(lit) => write_literal(buf, lit),
        Pattern::Record(fields) => {
            buf.push_str("{ ");
            for (i, (name, p)) in fields.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                // Record pattern punning: `{ x }` when field pattern is Var(x).
                if let Pattern::Var(v) = p {
                    if v == name {
                        buf.push_str(name);
                        continue;
                    }
                }
                buf.push_str(name);
                buf.push_str(" = ");
                write_pattern(buf, p);
            }
            buf.push_str(" }");
        }
        Pattern::List(pats) => {
            buf.push('[');
            for (i, p) in pats.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                write_pattern(buf, p);
            }
            buf.push(']');
        }
        Pattern::Constructor(name, args) => {
            buf.push_str(name);
            for arg in args {
                buf.push(' ');
                // Wrap constructor args in parens if they are themselves
                // constructors with args (to avoid ambiguity).
                let needs_parens = matches!(arg, Pattern::Constructor(_, a) if !a.is_empty());
                if needs_parens {
                    buf.push('(');
                }
                write_pattern(buf, arg);
                if needs_parens {
                    buf.push(')');
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, tokenize};

    /// Parse a string, pretty print it, re-parse, and verify equality.
    fn round_trip(input: &str) {
        let tokens1 = tokenize(input).unwrap_or_else(|e| panic!("first lex failed: {e}"));
        let expr1 = parse(&tokens1).unwrap_or_else(|e| panic!("first parse failed: {e:?}"));
        let printed = pretty_print(&expr1);
        let tokens2 = tokenize(&printed).unwrap_or_else(|e| {
            panic!("re-lex failed for {printed:?}: {e}");
        });
        let expr2 = parse(&tokens2).unwrap_or_else(|e| {
            panic!("re-parse failed for {printed:?}: {e:?}");
        });
        assert_eq!(
            expr1, expr2,
            "round trip failed.\n  input:   {input:?}\n  printed: {printed:?}"
        );
    }

    /// Pretty print an expression built programmatically and check output.
    fn prints_as(expr: &Expr, expected: &str) {
        let actual = pretty_print(expr);
        assert_eq!(actual, expected, "pretty_print mismatch");
    }

    // ── Literals ──────────────────────────────────────────────────

    #[test]
    fn lit_int() {
        prints_as(&Expr::Lit(Literal::Int(42)), "42");
    }

    #[test]
    fn lit_negative_int() {
        prints_as(&Expr::Lit(Literal::Int(-5)), "-5");
    }

    #[test]
    fn lit_float() {
        prints_as(&Expr::Lit(Literal::Float(3.125)), "3.125");
    }

    #[test]
    fn lit_string() {
        prints_as(&Expr::Lit(Literal::Str("hello".into())), r#""hello""#);
    }

    #[test]
    fn lit_string_escapes() {
        prints_as(
            &Expr::Lit(Literal::Str("say \"hi\"".into())),
            r#""say \"hi\"""#,
        );
    }

    #[test]
    fn lit_bool() {
        prints_as(&Expr::Lit(Literal::Bool(true)), "True");
        prints_as(&Expr::Lit(Literal::Bool(false)), "False");
    }

    #[test]
    fn lit_null() {
        prints_as(&Expr::Lit(Literal::Null), "Nothing");
    }

    #[test]
    fn lit_bytes() {
        prints_as(&Expr::Lit(Literal::Bytes(vec![1, 2, 3])), "[1, 2, 3]");
    }

    // ── Variables ─────────────────────────────────────────────────

    #[test]
    fn variable() {
        prints_as(&Expr::Var(Arc::from("x")), "x");
    }

    // ── Lambda ────────────────────────────────────────────────────

    #[test]
    fn lambda_simple() {
        prints_as(
            &Expr::Lam(Arc::from("x"), Box::new(Expr::Var(Arc::from("x")))),
            "\\x -> x",
        );
    }

    #[test]
    fn lambda_multi_param() {
        prints_as(
            &Expr::Lam(
                Arc::from("x"),
                Box::new(Expr::Lam(
                    Arc::from("y"),
                    Box::new(Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Var(Arc::from("x")), Expr::Var(Arc::from("y"))],
                    )),
                )),
            ),
            "\\x y -> x + y",
        );
    }

    #[test]
    fn lambda_round_trip() {
        round_trip("\\x -> x + 1");
        round_trip("\\x y -> x + y");
    }

    // ── Application ───────────────────────────────────────────────

    #[test]
    fn app_simple() {
        prints_as(
            &Expr::App(
                Box::new(Expr::Var(Arc::from("f"))),
                Box::new(Expr::Var(Arc::from("x"))),
            ),
            "f x",
        );
    }

    #[test]
    fn app_chain() {
        prints_as(
            &Expr::App(
                Box::new(Expr::App(
                    Box::new(Expr::Var(Arc::from("f"))),
                    Box::new(Expr::Var(Arc::from("x"))),
                )),
                Box::new(Expr::Var(Arc::from("y"))),
            ),
            "f x y",
        );
    }

    #[test]
    fn app_complex_arg() {
        // f (g x) should parenthesize the argument
        prints_as(
            &Expr::App(
                Box::new(Expr::Var(Arc::from("f"))),
                Box::new(Expr::App(
                    Box::new(Expr::Var(Arc::from("g"))),
                    Box::new(Expr::Var(Arc::from("x"))),
                )),
            ),
            "f (g x)",
        );
    }

    // ── Record ────────────────────────────────────────────────────

    #[test]
    fn record_simple() {
        prints_as(
            &Expr::Record(vec![
                (Arc::from("x"), Expr::Lit(Literal::Int(1))),
                (Arc::from("y"), Expr::Lit(Literal::Int(2))),
            ]),
            "{ x = 1, y = 2 }",
        );
    }

    #[test]
    fn record_punning() {
        prints_as(
            &Expr::Record(vec![
                (Arc::from("x"), Expr::Var(Arc::from("x"))),
                (Arc::from("y"), Expr::Var(Arc::from("y"))),
            ]),
            "{ x, y }",
        );
    }

    #[test]
    fn record_mixed_punning() {
        prints_as(
            &Expr::Record(vec![
                (Arc::from("x"), Expr::Var(Arc::from("x"))),
                (Arc::from("y"), Expr::Lit(Literal::Int(42))),
            ]),
            "{ x, y = 42 }",
        );
    }

    #[test]
    fn record_round_trip() {
        round_trip("{ name = x, age = 30 }");
        round_trip("{ x, y }");
    }

    // ── List ──────────────────────────────────────────────────────

    #[test]
    fn list_simple() {
        prints_as(
            &Expr::List(vec![
                Expr::Lit(Literal::Int(1)),
                Expr::Lit(Literal::Int(2)),
                Expr::Lit(Literal::Int(3)),
            ]),
            "[1, 2, 3]",
        );
    }

    #[test]
    fn list_empty() {
        prints_as(&Expr::List(vec![]), "[]");
    }

    #[test]
    fn list_round_trip() {
        round_trip("[1, 2, 3]");
        round_trip("[]");
    }

    // ── Field access ──────────────────────────────────────────────

    #[test]
    fn field_access() {
        prints_as(
            &Expr::Field(Box::new(Expr::Var(Arc::from("x"))), Arc::from("name")),
            "x.name",
        );
    }

    #[test]
    fn field_chain() {
        prints_as(
            &Expr::Field(
                Box::new(Expr::Field(
                    Box::new(Expr::Var(Arc::from("x"))),
                    Arc::from("a"),
                )),
                Arc::from("b"),
            ),
            "x.a.b",
        );
    }

    #[test]
    fn field_round_trip() {
        round_trip("x.name");
        round_trip("x.a.b");
    }

    // ── Edge traversal ────────────────────────────────────────────

    #[test]
    fn edge_traversal() {
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Edge,
                vec![
                    Expr::Var(Arc::from("doc")),
                    Expr::Lit(Literal::Str("layers".into())),
                ],
            ),
            "doc -> layers",
        );
    }

    #[test]
    fn edge_chain() {
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Edge,
                vec![
                    Expr::Builtin(
                        BuiltinOp::Edge,
                        vec![
                            Expr::Var(Arc::from("doc")),
                            Expr::Lit(Literal::Str("layers".into())),
                        ],
                    ),
                    Expr::Lit(Literal::Str("annotations".into())),
                ],
            ),
            "doc -> layers -> annotations",
        );
    }

    #[test]
    fn edge_round_trip() {
        round_trip("doc -> layers");
        round_trip("doc -> layers -> annotations");
    }

    // ── Infix operators ───────────────────────────────────────────

    #[test]
    fn infix_add() {
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var(Arc::from("x")), Expr::Lit(Literal::Int(1))],
            ),
            "x + 1",
        );
    }

    #[test]
    fn infix_precedence_no_parens() {
        // 1 + 2 * 3 should not need parens because * binds tighter.
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Add,
                vec![
                    Expr::Lit(Literal::Int(1)),
                    Expr::Builtin(
                        BuiltinOp::Mul,
                        vec![Expr::Lit(Literal::Int(2)), Expr::Lit(Literal::Int(3))],
                    ),
                ],
            ),
            "1 + 2 * 3",
        );
    }

    #[test]
    fn infix_precedence_needs_parens() {
        // (1 + 2) * 3 needs parens because + is lower than *.
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Mul,
                vec![
                    Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(2))],
                    ),
                    Expr::Lit(Literal::Int(3)),
                ],
            ),
            "(1 + 2) * 3",
        );
    }

    #[test]
    fn infix_left_assoc_no_parens() {
        // 1 + 2 + 3 is left-associative, so (1+2)+3 needs no parens.
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Add,
                vec![
                    Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(2))],
                    ),
                    Expr::Lit(Literal::Int(3)),
                ],
            ),
            "1 + 2 + 3",
        );
    }

    #[test]
    fn infix_right_assoc_needs_parens() {
        // For left-assoc +, 1 + (2 + 3) needs parens on the right.
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Add,
                vec![
                    Expr::Lit(Literal::Int(1)),
                    Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Lit(Literal::Int(2)), Expr::Lit(Literal::Int(3))],
                    ),
                ],
            ),
            "1 + (2 + 3)",
        );
    }

    #[test]
    fn infix_concat_right_assoc() {
        // ++ is right-associative, so a ++ (b ++ c) needs no parens.
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Concat,
                vec![
                    Expr::Var(Arc::from("a")),
                    Expr::Builtin(
                        BuiltinOp::Concat,
                        vec![Expr::Var(Arc::from("b")), Expr::Var(Arc::from("c"))],
                    ),
                ],
            ),
            "a ++ b ++ c",
        );
    }

    #[test]
    fn infix_comparison() {
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Eq,
                vec![Expr::Var(Arc::from("x")), Expr::Lit(Literal::Int(1))],
            ),
            "x == 1",
        );
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Neq,
                vec![Expr::Var(Arc::from("x")), Expr::Lit(Literal::Int(1))],
            ),
            "x /= 1",
        );
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Lt,
                vec![Expr::Var(Arc::from("x")), Expr::Var(Arc::from("y"))],
            ),
            "x < y",
        );
    }

    #[test]
    fn infix_logical() {
        prints_as(
            &Expr::Builtin(
                BuiltinOp::And,
                vec![Expr::Var(Arc::from("a")), Expr::Var(Arc::from("b"))],
            ),
            "a && b",
        );
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Or,
                vec![Expr::Var(Arc::from("a")), Expr::Var(Arc::from("b"))],
            ),
            "a || b",
        );
    }

    #[test]
    fn infix_round_trips() {
        round_trip("1 + 2");
        round_trip("1 + 2 * 3");
        round_trip("(1 + 2) * 3");
        round_trip("a && b || c");
        round_trip("x == 1");
        round_trip("x /= 1");
    }

    // ── Prefix operators ──────────────────────────────────────────

    #[test]
    fn prefix_neg() {
        prints_as(
            &Expr::Builtin(BuiltinOp::Neg, vec![Expr::Var(Arc::from("x"))]),
            "-x",
        );
    }

    #[test]
    fn prefix_not() {
        prints_as(
            &Expr::Builtin(BuiltinOp::Not, vec![Expr::Lit(Literal::Bool(true))]),
            "not True",
        );
    }

    #[test]
    fn prefix_round_trip() {
        round_trip("-x");
        round_trip("not True");
    }

    // ── Builtin function call syntax ──────────────────────────────

    #[test]
    fn builtin_function_call() {
        prints_as(
            &Expr::Builtin(
                BuiltinOp::Map,
                vec![Expr::Var(Arc::from("f")), Expr::Var(Arc::from("xs"))],
            ),
            "map f xs",
        );
    }

    #[test]
    fn builtin_unary() {
        prints_as(
            &Expr::Builtin(BuiltinOp::Head, vec![Expr::Var(Arc::from("xs"))]),
            "head xs",
        );
    }

    #[test]
    fn builtin_round_trip() {
        round_trip("map f xs");
        round_trip("head xs");
        round_trip("filter f xs");
    }

    // ── Let ───────────────────────────────────────────────────────

    #[test]
    fn let_simple() {
        prints_as(
            &Expr::Let {
                name: Arc::from("x"),
                value: Box::new(Expr::Lit(Literal::Int(1))),
                body: Box::new(Expr::Builtin(
                    BuiltinOp::Add,
                    vec![Expr::Var(Arc::from("x")), Expr::Lit(Literal::Int(1))],
                )),
            },
            "let x = 1 in x + 1",
        );
    }

    #[test]
    fn let_round_trip() {
        round_trip("let x = 1 in x + 1");
    }

    // ── If/then/else ──────────────────────────────────────────────

    #[test]
    fn if_then_else() {
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Lit(Literal::Bool(true))),
            arms: vec![
                (
                    Pattern::Lit(Literal::Bool(true)),
                    Expr::Lit(Literal::Int(1)),
                ),
                (Pattern::Wildcard, Expr::Lit(Literal::Int(0))),
            ],
        };
        prints_as(&expr, "if True then 1 else 0");
    }

    #[test]
    fn if_round_trip() {
        round_trip("if True then 1 else 0");
    }

    // ── Case/of ───────────────────────────────────────────────────

    #[test]
    fn case_of() {
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var(Arc::from("x"))),
            arms: vec![
                (
                    Pattern::Lit(Literal::Bool(true)),
                    Expr::Lit(Literal::Int(1)),
                ),
                (
                    Pattern::Lit(Literal::Bool(false)),
                    Expr::Lit(Literal::Int(0)),
                ),
            ],
        };
        prints_as(&expr, "case x of\n  True -> 1\n  False -> 0");
    }

    #[test]
    fn case_round_trip() {
        round_trip("case x of\n  True -> 1\n  False -> 0");
    }

    // ── Nested expressions ────────────────────────────────────────

    #[test]
    fn nested_let_in_lambda() {
        round_trip("\\x -> let y = x + 1 in y * 2");
    }

    #[test]
    fn nested_if_in_let() {
        round_trip("let x = if True then 1 else 0 in x + 1");
    }

    #[test]
    fn lambda_as_arg() {
        // f (\x -> x) should parenthesize the lambda argument
        prints_as(
            &Expr::App(
                Box::new(Expr::Var(Arc::from("f"))),
                Box::new(Expr::Lam(
                    Arc::from("x"),
                    Box::new(Expr::Var(Arc::from("x"))),
                )),
            ),
            "f (\\x -> x)",
        );
    }

    #[test]
    fn complex_expression_round_trip() {
        round_trip("\\f xs -> map (\\x -> f x + 1) xs");
    }

    // ── Pattern printing ──────────────────────────────────────────

    #[test]
    fn pattern_wildcard() {
        let mut buf = String::new();
        write_pattern(&mut buf, &Pattern::Wildcard);
        assert_eq!(buf, "_");
    }

    #[test]
    fn pattern_var() {
        let mut buf = String::new();
        write_pattern(&mut buf, &Pattern::Var(Arc::from("x")));
        assert_eq!(buf, "x");
    }

    #[test]
    fn pattern_lit() {
        let mut buf = String::new();
        write_pattern(&mut buf, &Pattern::Lit(Literal::Int(42)));
        assert_eq!(buf, "42");
    }

    #[test]
    fn pattern_list() {
        let mut buf = String::new();
        write_pattern(
            &mut buf,
            &Pattern::List(vec![
                Pattern::Var(Arc::from("x")),
                Pattern::Var(Arc::from("y")),
            ]),
        );
        assert_eq!(buf, "[x, y]");
    }

    #[test]
    fn pattern_record_punning() {
        let mut buf = String::new();
        write_pattern(
            &mut buf,
            &Pattern::Record(vec![
                (Arc::from("x"), Pattern::Var(Arc::from("x"))),
                (Arc::from("y"), Pattern::Var(Arc::from("y"))),
            ]),
        );
        assert_eq!(buf, "{ x, y }");
    }

    #[test]
    fn pattern_constructor() {
        let mut buf = String::new();
        write_pattern(
            &mut buf,
            &Pattern::Constructor(Arc::from("Just"), vec![Pattern::Var(Arc::from("x"))]),
        );
        assert_eq!(buf, "Just x");
    }

    // ── Index ─────────────────────────────────────────────────────

    #[test]
    fn index_access() {
        prints_as(
            &Expr::Index(
                Box::new(Expr::Var(Arc::from("xs"))),
                Box::new(Expr::Lit(Literal::Int(0))),
            ),
            "xs[0]",
        );
    }

    // ── Literal record and list ───────────────────────────────────

    #[test]
    fn literal_record() {
        prints_as(
            &Expr::Lit(Literal::Record(vec![
                (Arc::from("x"), Literal::Int(1)),
                (Arc::from("y"), Literal::Int(2)),
            ])),
            "{ x = 1, y = 2 }",
        );
    }

    #[test]
    fn literal_list() {
        prints_as(
            &Expr::Lit(Literal::List(vec![Literal::Int(1), Literal::Int(2)])),
            "[1, 2]",
        );
    }

    // ── Mixed precedence round trips ──────────────────────────────

    #[test]
    fn precedence_logical_and_comparison() {
        round_trip("x == 1 && y == 2");
    }

    #[test]
    fn precedence_arithmetic_in_comparison() {
        round_trip("x + 1 == y * 2");
    }

    #[test]
    fn concat_round_trip() {
        round_trip(r#""hello" ++ " world""#);
    }
}
