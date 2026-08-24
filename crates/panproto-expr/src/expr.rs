//! Expression AST, pattern, and builtin operation types.
//!
//! The expression language is a pure functional language: lambda calculus
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
/// All operations are pure: no IO, no mutation, deterministic.
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

    // --- List (10) ---
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
    /// `range(start: int, stop: int) → [int]` (inclusive of both bounds;
    /// empty when `stop < start`)
    Range,

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
    /// Resolve a surface identifier to the builtin it names.
    ///
    /// Both the `snake_case` and `camelCase` spellings are accepted where the
    /// surface syntax offers both.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "add" => Some(Self::Add),
            "sub" => Some(Self::Sub),
            "mul" => Some(Self::Mul),
            "abs" => Some(Self::Abs),
            "floor" => Some(Self::Floor),
            "ceil" => Some(Self::Ceil),
            "round" => Some(Self::Round),
            "concat" => Some(Self::Concat),
            "len" => Some(Self::Len),
            "slice" => Some(Self::Slice),
            "upper" => Some(Self::Upper),
            "lower" => Some(Self::Lower),
            "trim" => Some(Self::Trim),
            "split" => Some(Self::Split),
            "join" => Some(Self::Join),
            "replace" => Some(Self::Replace),
            "contains" => Some(Self::Contains),
            "map" => Some(Self::Map),
            "filter" => Some(Self::Filter),
            "fold" => Some(Self::Fold),
            "append" => Some(Self::Append),
            "head" => Some(Self::Head),
            "tail" => Some(Self::Tail),
            "reverse" => Some(Self::Reverse),
            "flat_map" | "flatMap" => Some(Self::FlatMap),
            "length" => Some(Self::Length),
            "range" => Some(Self::Range),
            "merge" | "merge_records" => Some(Self::MergeRecords),
            "keys" => Some(Self::Keys),
            "values" => Some(Self::Values),
            "has_field" | "hasField" => Some(Self::HasField),
            "default" | "default_val" | "defaultVal" => Some(Self::DefaultVal),
            "clamp" => Some(Self::Clamp),
            "truncate_str" | "truncateStr" => Some(Self::TruncateStr),
            "int_to_float" | "intToFloat" => Some(Self::IntToFloat),
            "float_to_int" | "floatToInt" => Some(Self::FloatToInt),
            "int_to_str" | "intToStr" => Some(Self::IntToStr),
            "float_to_str" | "floatToStr" => Some(Self::FloatToStr),
            "str_to_int" | "strToInt" => Some(Self::StrToInt),
            "str_to_float" | "strToFloat" => Some(Self::StrToFloat),
            "type_of" | "typeOf" => Some(Self::TypeOf),
            "is_null" | "isNull" => Some(Self::IsNull),
            "is_list" | "isList" => Some(Self::IsList),
            "edge" => Some(Self::Edge),
            "children" => Some(Self::Children),
            "has_edge" | "hasEdge" => Some(Self::HasEdge),
            "edge_count" | "edgeCount" => Some(Self::EdgeCount),
            "anchor" => Some(Self::Anchor),
            _ => None,
        }
    }

    /// Permute surface-syntax arguments into the order [`Expr::Builtin`] holds.
    ///
    /// The surface syntax follows the usual functional convention of naming the
    /// function first (`map f xs`, `fold f z xs`), while [`Expr::Builtin`] takes
    /// the list first and the function last. The two orders are deliberately
    /// distinct: `Expr` is serialized into stored lens documents, so its
    /// argument order is the compatibility-bearing one and the surface syntax
    /// lowers into it.
    ///
    /// The permutation applies only to a saturated call, since a partial
    /// application has no complete order to permute. Builtins outside this set
    /// take their arguments in the same order at both layers and pass through
    /// untouched.
    #[must_use]
    pub fn surface_args_to_expr_args(self, mut args: Vec<Expr>) -> Vec<Expr> {
        match (self, args.len()) {
            // `map f xs` / `filter p xs` / `flat_map f xs` -> [xs, f]
            (Self::Map | Self::Filter | Self::FlatMap, 2) => {
                args.swap(0, 1);
                args
            }
            // `fold f z xs` -> [xs, z, f]
            (Self::Fold, 3) => {
                args.swap(0, 2);
                args
            }
            _ => args,
        }
    }

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
            | Self::Range
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
            // Overloaded on the first argument: substring containment on a
            // string, element membership on a list. Inputs are `Any`; only
            // the `Bool` result is fixed.
            Self::Contains => Some((&[ExprType::Any, ExprType::Any], ExprType::Bool)),
            Self::TruncateStr => Some((&[ExprType::Str, ExprType::Int], ExprType::Str)),

            // List operations.
            Self::Length => Some((&[ExprType::List], ExprType::Int)),
            Self::Range => Some((&[ExprType::Int, ExprType::Int], ExprType::List)),
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
