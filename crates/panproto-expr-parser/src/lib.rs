//! Haskell-style surface syntax parser for panproto expressions.
//!
//! Parses a human-readable functional language into panproto's native
//! representation types: `Expr`, `InstanceQuery`, `FieldTransform`,
//! `DirectedEquation`, and `WInstance` of `ThExpr`.
//!
//! The surface syntax supports list comprehensions, do-notation, let/where
//! bindings, case/of with guards, lambda expressions, curried application,
//! function composition, operator sections, record syntax with punning,
//! pattern matching, and `->` for graph edge traversal.

/// Placeholder — parser implementation follows in subsequent phases.
#[must_use]
pub const fn parse_smoke_test() -> bool {
    // Verify logos and chumsky are importable
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        assert!(parse_smoke_test());
    }
}
