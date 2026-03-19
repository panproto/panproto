//! Expression AST, pattern, and builtin operation types.
//!
//! The expression language is a pure functional language — lambda calculus
//! with pattern matching, algebraic data types, and built-in operations on
//! strings, numbers, records, and lists. Comparable to a pure subset of ML.

use std::sync::Arc;

use crate::Literal;

/// An expression in the pure functional language.
///
/// All variants are serializable, content-addressable, and evaluate
/// deterministically on any platform (including WASM).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    /// Variable reference.
    Var(Arc<str>),
    /// Lambda abstraction: `λparam. body`.
    Lam(Arc<str>, Box<Self>),
    /// Function application: `func(arg)`.
    App(Box<Self>, Box<Self>),
    /// Literal value.
    Lit(Literal),
    /// Record construction: `{ name: expr, ... }`.
    Record(Vec<(Arc<str>, Self)>),
    /// List construction: `[expr, ...]`.
    List(Vec<Self>),
    /// Field access: `expr.field`.
    Field(Box<Self>, Arc<str>),
    /// Index access: `expr[index]`.
    Index(Box<Self>, Box<Self>),
    /// Pattern matching: `match scrutinee { pat => body, ... }`.
    Match {
        /// The value being matched against.
        scrutinee: Box<Self>,
        /// Arms: (pattern, body) pairs tried in order.
        arms: Vec<(Pattern, Self)>,
    },
    /// Let binding: `let name = value in body`.
    Let {
        /// The bound variable name.
        name: Arc<str>,
        /// The value to bind.
        value: Box<Self>,
        /// The body where the binding is visible.
        body: Box<Self>,
    },
    /// Built-in operation applied to arguments.
    Builtin(BuiltinOp, Vec<Self>),
}

/// A destructuring pattern for match expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Pattern {
    /// Matches anything, binds nothing.
    Wildcard,
    /// Matches anything, binds the value to a name.
    Var(Arc<str>),
    /// Matches a specific literal value.
    Lit(Literal),
    /// Matches a record with specific field patterns.
    Record(Vec<(Arc<str>, Self)>),
    /// Matches a list with element patterns.
    List(Vec<Self>),
    /// Matches a tagged constructor with argument patterns.
    Constructor(Arc<str>, Vec<Self>),
}

/// Built-in operations, grouped by domain.
///
/// Each operation has a fixed arity enforced at evaluation time.
/// All operations are pure — no IO, no mutation, deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BuiltinOp {
    // --- Arithmetic (7) ---
    /// `add(a: int|float, b: int|float) → int|float`
    Add,
    /// `sub(a: int|float, b: int|float) → int|float`
    Sub,
    /// `mul(a: int|float, b: int|float) → int|float`
    Mul,
    /// `div(a: int|float, b: int|float) → int|float` (truncating for ints)
    Div,
    /// `mod_(a: int, b: int) → int`
    Mod,
    /// `neg(a: int|float) → int|float`
    Neg,
    /// `abs(a: int|float) → int|float`
    Abs,

    // --- Rounding (2) ---
    /// `floor(a: float) → int`
    Floor,
    /// `ceil(a: float) → int`
    Ceil,

    // --- Comparison (6) ---
    /// `eq(a, b) → bool`
    Eq,
    /// `neq(a, b) → bool`
    Neq,
    /// `lt(a, b) → bool`
    Lt,
    /// `lte(a, b) → bool`
    Lte,
    /// `gt(a, b) → bool`
    Gt,
    /// `gte(a, b) → bool`
    Gte,

    // --- Boolean (3) ---
    /// `and(a: bool, b: bool) → bool`
    And,
    /// `or(a: bool, b: bool) → bool`
    Or,
    /// `not(a: bool) → bool`
    Not,

    // --- String (10) ---
    /// `concat(a: string, b: string) → string`
    Concat,
    /// `len(s: string) → int` (byte length)
    Len,
    /// `slice(s: string, start: int, end: int) → string`
    Slice,
    /// `upper(s: string) → string`
    Upper,
    /// `lower(s: string) → string`
    Lower,
    /// `trim(s: string) → string`
    Trim,
    /// `split(s: string, delim: string) → [string]`
    Split,
    /// `join(parts: [string], delim: string) → string`
    Join,
    /// `replace(s: string, from: string, to: string) → string`
    Replace,
    /// `contains(s: string, substr: string) → bool`
    Contains,

    // --- List (9) ---
    /// `map(list: [a], f: a → b) → [b]`
    Map,
    /// `filter(list: [a], pred: a → bool) → [a]`
    Filter,
    /// `fold(list: [a], init: b, f: (b, a) → b) → b`
    Fold,
    /// `append(list: [a], item: a) → [a]`
    Append,
    /// `head(list: [a]) → a`
    Head,
    /// `tail(list: [a]) → [a]`
    Tail,
    /// `reverse(list: [a]) → [a]`
    Reverse,
    /// `flat_map(list: [a], f: a → [b]) → [b]`
    FlatMap,
    /// `length(list: [a]) → int` (list length, distinct from string Len)
    Length,

    // --- Record (4) ---
    /// `merge(a: record, b: record) → record` (b fields override a)
    MergeRecords,
    /// `keys(r: record) → [string]`
    Keys,
    /// `values(r: record) → [any]`
    Values,
    /// `has_field(r: record, name: string) → bool`
    HasField,

    // --- Type coercions (6) ---
    /// `int_to_float(n: int) → float`
    IntToFloat,
    /// `float_to_int(f: float) → int` (truncates)
    FloatToInt,
    /// `int_to_str(n: int) → string`
    IntToStr,
    /// `float_to_str(f: float) → string`
    FloatToStr,
    /// `str_to_int(s: string) → int` (fails on non-numeric)
    StrToInt,
    /// `str_to_float(s: string) → float` (fails on non-numeric)
    StrToFloat,

    // --- Type inspection (3) ---
    /// `type_of(v) → string` (returns type name)
    TypeOf,
    /// `is_null(v) → bool`
    IsNull,
    /// `is_list(v) → bool`
    IsList,
}

impl BuiltinOp {
    /// Returns the expected number of arguments for this builtin.
    #[must_use]
    pub const fn arity(self) -> usize {
        match self {
            // Unary
            Self::Neg
            | Self::Abs
            | Self::Floor
            | Self::Ceil
            | Self::Not
            | Self::Upper
            | Self::Lower
            | Self::Trim
            | Self::Head
            | Self::Tail
            | Self::Reverse
            | Self::Keys
            | Self::Values
            | Self::IntToFloat
            | Self::FloatToInt
            | Self::IntToStr
            | Self::FloatToStr
            | Self::StrToInt
            | Self::StrToFloat
            | Self::TypeOf
            | Self::IsNull
            | Self::IsList
            | Self::Len
            | Self::Length => 1,
            // Binary
            Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Mod
            | Self::Eq
            | Self::Neq
            | Self::Lt
            | Self::Lte
            | Self::Gt
            | Self::Gte
            | Self::And
            | Self::Or
            | Self::Concat
            | Self::Split
            | Self::Join
            | Self::Append
            | Self::Map
            | Self::Filter
            | Self::HasField
            | Self::MergeRecords
            | Self::Contains
            | Self::FlatMap => 2,
            // Ternary
            Self::Slice | Self::Replace | Self::Fold => 3,
        }
    }
}

impl Expr {
    /// Create a variable expression.
    #[must_use]
    pub fn var(name: impl Into<Arc<str>>) -> Self {
        Self::Var(name.into())
    }

    /// Create a lambda expression.
    #[must_use]
    pub fn lam(param: impl Into<Arc<str>>, body: Self) -> Self {
        Self::Lam(param.into(), Box::new(body))
    }

    /// Create an application expression.
    #[must_use]
    pub fn app(func: Self, arg: Self) -> Self {
        Self::App(Box::new(func), Box::new(arg))
    }

    /// Create a let-binding expression.
    #[must_use]
    pub fn let_in(name: impl Into<Arc<str>>, value: Self, body: Self) -> Self {
        Self::Let {
            name: name.into(),
            value: Box::new(value),
            body: Box::new(body),
        }
    }

    /// Create a field access expression.
    #[must_use]
    pub fn field(expr: Self, name: impl Into<Arc<str>>) -> Self {
        Self::Field(Box::new(expr), name.into())
    }

    /// Create a builtin operation applied to arguments.
    #[must_use]
    pub const fn builtin(op: BuiltinOp, args: Vec<Self>) -> Self {
        Self::Builtin(op, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_arities() {
        assert_eq!(BuiltinOp::Add.arity(), 2);
        assert_eq!(BuiltinOp::Not.arity(), 1);
        assert_eq!(BuiltinOp::Fold.arity(), 3);
        assert_eq!(BuiltinOp::Slice.arity(), 3);
    }

    #[test]
    fn expr_constructors() {
        let e = Expr::let_in(
            "x",
            Expr::Lit(Literal::Int(42)),
            Expr::builtin(
                BuiltinOp::Add,
                vec![Expr::var("x"), Expr::Lit(Literal::Int(1))],
            ),
        );
        assert!(matches!(e, Expr::Let { .. }));
    }
}
