//! Literal values in the expression language.
//!
//! [`Literal`] is the expression language's own value type, independent of
//! `panproto_inst::Value` to avoid dependency cycles. Downstream crates
//! provide conversions between the two.

use std::sync::Arc;

/// A literal value in the expression language.
///
/// This is the result type of expression evaluation and the leaf node
/// type for literal expressions. Kept minimal — just the primitives
/// needed for schema transforms.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Literal {
    /// Boolean value.
    Bool(bool),
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit IEEE 754 float.
    Float(f64),
    /// UTF-8 string.
    Str(String),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// Null / absent value.
    Null,
    /// A record (ordered map of fields to values).
    Record(Vec<(Arc<str>, Self)>),
    /// A list of values.
    List(Vec<Self>),
    /// A closure: a lambda expression captured with its environment.
    ///
    /// Closures are first-class values produced by evaluating a `Lam` expression.
    /// They capture the parameter name, body, and the environment at the point
    /// of creation, enabling proper lexical scoping.
    Closure {
        /// The parameter name bound by this lambda.
        param: Arc<str>,
        /// The body expression (serialized as the AST).
        body: Box<crate::Expr>,
        /// Captured environment bindings at the point of closure creation.
        env: Vec<(Arc<str>, Self)>,
    },
}

impl Literal {
    /// Returns a human-readable type name for error messages.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "string",
            Self::Bytes(_) => "bytes",
            Self::Null => "null",
            Self::Record(_) => "record",
            Self::List(_) => "list",
            Self::Closure { .. } => "function",
        }
    }

    /// Returns `true` if this is a [`Literal::Null`].
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Attempts to extract a boolean value.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Attempts to extract an integer value.
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Attempts to extract a float value.
    #[must_use]
    pub const fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Attempts to extract a string reference.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to extract a record reference.
    #[must_use]
    pub fn as_record(&self) -> Option<&[(Arc<str>, Self)]> {
        match self {
            Self::Record(fields) => Some(fields),
            _ => None,
        }
    }

    /// Attempts to extract a list reference.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    /// Look up a field in a record by name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Self> {
        match self {
            Self::Record(fields) => fields.iter().find(|(k, _)| &**k == name).map(|(_, v)| v),
            _ => None,
        }
    }
}

// Custom PartialEq that uses f64::to_bits for float comparison,
// making it consistent with Eq and Hash.
impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            (Self::Null, Self::Null) => true,
            (Self::Record(a), Self::Record(b)) => a == b,
            (Self::List(a), Self::List(b)) => a == b,
            (
                Self::Closure {
                    param: p1,
                    body: b1,
                    env: e1,
                },
                Self::Closure {
                    param: p2,
                    body: b2,
                    env: e2,
                },
            ) => p1 == p2 && b1 == b2 && e1 == e2,
            _ => false,
        }
    }
}

impl Eq for Literal {}

impl std::hash::Hash for Literal {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Bool(b) => b.hash(state),
            Self::Int(n) => n.hash(state),
            Self::Float(f) => f.to_bits().hash(state),
            Self::Str(s) => s.hash(state),
            Self::Bytes(b) => b.hash(state),
            Self::Null => {}
            Self::Record(fields) => fields.hash(state),
            Self::List(items) => items.hash(state),
            Self::Closure { param, body, env } => {
                param.hash(state);
                body.hash(state);
                env.hash(state);
            }
        }
    }
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Str(s) => write!(f, "\"{s}\""),
            Self::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            Self::Null => write!(f, "null"),
            Self::Record(fields) => {
                write!(f, "{{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, " }}")
            }
            Self::List(items) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Self::Closure { param, .. } => write!(f, "<closure λ{param}>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_equality_uses_bits() {
        // NaN == NaN when using to_bits comparison
        let a = Literal::Float(f64::NAN);
        let b = Literal::Float(f64::NAN);
        assert_eq!(a, b);
    }

    #[test]
    fn type_names() {
        assert_eq!(Literal::Bool(true).type_name(), "bool");
        assert_eq!(Literal::Int(42).type_name(), "int");
        assert_eq!(Literal::Null.type_name(), "null");
        assert_eq!(Literal::Record(vec![]).type_name(), "record");
        assert_eq!(Literal::List(vec![]).type_name(), "list");
    }

    #[test]
    fn record_field_lookup() {
        let rec = Literal::Record(vec![
            (Arc::from("name"), Literal::Str("alice".into())),
            (Arc::from("age"), Literal::Int(30)),
        ]);
        assert_eq!(rec.field("name"), Some(&Literal::Str("alice".into())));
        assert_eq!(rec.field("age"), Some(&Literal::Int(30)));
        assert_eq!(rec.field("missing"), None);
    }
}
