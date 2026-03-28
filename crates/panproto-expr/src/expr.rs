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

/// Simple type classification for expressions.
///
/// This is a lightweight type system for the expression language,
/// independent of the GAT type system in `panproto-gat`. Used for
/// type inference and coercion validation within expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExprType {
    /// 64-bit signed integer.
    Int,
    /// 64-bit IEEE 754 float.
    Float,
    /// UTF-8 string.
    Str,
    /// Boolean.
    Bool,
    /// Homogeneous list.
    List,
    /// Record (ordered map of fields to values).
    Record,
    /// Unknown or polymorphic type.
    Any,
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

    // --- Rounding (3) ---
    /// `floor(a: float) → int`
    Floor,
    /// `ceil(a: float) → int`
    Ceil,
    /// `round(a: float) → int` (rounds to nearest, ties to even)
    Round,

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

    // --- Utility (3) ---
    /// `default(x, fallback)`: returns fallback if x is null, else x.
    DefaultVal,
    /// `clamp(x, min, max)`: clamp a numeric value to the range [min, max].
    Clamp,
    /// `truncate_str(s, max_len)`: truncate a string to at most `max_len` bytes
    /// (char-boundary safe).
    TruncateStr,

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

    // --- Graph traversal (5) ---
    // These builtins require an instance context (`InstanceEnv` in
    // panproto-inst) and are evaluated by `eval_with_instance`, not
    // the standard `eval`. In the standard evaluator they return Null.
    /// `edge(node_ref: string, edge_kind: string) → value`
    /// Follow a named edge from a node in the instance tree.
    Edge,
    /// `children(node_ref: string) → [value]`
    /// Get all children of a node in the instance tree.
    Children,
    /// `has_edge(node_ref: string, edge_kind: string) → bool`
    /// Check if a node has a specific outgoing edge.
    HasEdge,
    /// `edge_count(node_ref: string) → int`
    /// Count outgoing edges from a node.
    EdgeCount,
    /// `anchor(node_ref: string) → string`
    /// Get the schema anchor (sort/kind) of a node.
    Anchor,
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
            | Self::Round
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
            | Self::Length
            | Self::Children
            | Self::EdgeCount
            | Self::Anchor => 1,
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
            | Self::FlatMap
            | Self::Edge
            | Self::HasEdge
            | Self::DefaultVal
            | Self::TruncateStr => 2,
            // Ternary
            Self::Slice | Self::Replace | Self::Fold | Self::Clamp => 3,
        }
    }

    /// Returns the type signature `(input_types, output_type)` for builtins
    /// with a known, monomorphic signature. Polymorphic builtins (e.g., `Add`
    /// works on both int and float) return `None`.
    #[must_use]
    pub const fn signature(self) -> Option<(&'static [ExprType], ExprType)> {
        match self {
            // Coercions: precise source→target signatures.
            Self::IntToFloat => Some((&[ExprType::Int], ExprType::Float)),
            Self::FloatToInt | Self::Floor | Self::Ceil | Self::Round => {
                Some((&[ExprType::Float], ExprType::Int))
            }
            Self::IntToStr => Some((&[ExprType::Int], ExprType::Str)),
            Self::FloatToStr => Some((&[ExprType::Float], ExprType::Str)),
            Self::StrToInt | Self::Len => Some((&[ExprType::Str], ExprType::Int)),
            Self::StrToFloat => Some((&[ExprType::Str], ExprType::Float)),

            // Boolean operations.
            Self::And | Self::Or => Some((&[ExprType::Bool, ExprType::Bool], ExprType::Bool)),
            Self::Not => Some((&[ExprType::Bool], ExprType::Bool)),

            // Comparison: polymorphic inputs, bool output.
            Self::Eq | Self::Neq | Self::Lt | Self::Lte | Self::Gt | Self::Gte => {
                Some((&[ExprType::Any, ExprType::Any], ExprType::Bool))
            }

            // String operations.
            Self::Concat => Some((&[ExprType::Str, ExprType::Str], ExprType::Str)),
            Self::Slice => Some((
                &[ExprType::Str, ExprType::Int, ExprType::Int],
                ExprType::Str,
            )),
            Self::Upper | Self::Lower | Self::Trim => Some((&[ExprType::Str], ExprType::Str)),
            Self::Split => Some((&[ExprType::Str, ExprType::Str], ExprType::List)),
            Self::Join => Some((&[ExprType::List, ExprType::Str], ExprType::Str)),
            Self::Replace => Some((
                &[ExprType::Str, ExprType::Str, ExprType::Str],
                ExprType::Str,
            )),
            Self::Contains => Some((&[ExprType::Str, ExprType::Str], ExprType::Bool)),
            Self::TruncateStr => Some((&[ExprType::Str, ExprType::Int], ExprType::Str)),

            // List operations.
            Self::Length => Some((&[ExprType::List], ExprType::Int)),
            Self::Reverse => Some((&[ExprType::List], ExprType::List)),

            // Record operations.
            Self::MergeRecords => Some((&[ExprType::Record, ExprType::Record], ExprType::Record)),
            Self::Keys | Self::Values => Some((&[ExprType::Record], ExprType::List)),
            Self::HasField => Some((&[ExprType::Record, ExprType::Str], ExprType::Bool)),

            // Type inspection.
            Self::TypeOf => Some((&[ExprType::Any], ExprType::Str)),
            Self::IsNull | Self::IsList => Some((&[ExprType::Any], ExprType::Bool)),

            // Polymorphic builtins: return None.
            Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Mod
            | Self::Neg
            | Self::Abs
            | Self::Map
            | Self::Filter
            | Self::Fold
            | Self::FlatMap
            | Self::Append
            | Self::Head
            | Self::Tail
            | Self::DefaultVal
            | Self::Clamp
            | Self::Edge
            | Self::Children
            | Self::HasEdge
            | Self::EdgeCount
            | Self::Anchor => None,
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

    /// Coerce an integer to a float.
    #[must_use]
    pub fn int_to_float(arg: Self) -> Self {
        Self::Builtin(BuiltinOp::IntToFloat, vec![arg])
    }

    /// Coerce a float to an integer (truncates toward zero).
    #[must_use]
    pub fn float_to_int(arg: Self) -> Self {
        Self::Builtin(BuiltinOp::FloatToInt, vec![arg])
    }

    /// Coerce an integer to a string.
    #[must_use]
    pub fn int_to_str(arg: Self) -> Self {
        Self::Builtin(BuiltinOp::IntToStr, vec![arg])
    }

    /// Coerce a float to a string.
    #[must_use]
    pub fn float_to_str(arg: Self) -> Self {
        Self::Builtin(BuiltinOp::FloatToStr, vec![arg])
    }

    /// Parse a string as an integer.
    #[must_use]
    pub fn str_to_int(arg: Self) -> Self {
        Self::Builtin(BuiltinOp::StrToInt, vec![arg])
    }

    /// Parse a string as a float.
    #[must_use]
    pub fn str_to_float(arg: Self) -> Self {
        Self::Builtin(BuiltinOp::StrToFloat, vec![arg])
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
