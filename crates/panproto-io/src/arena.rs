//! Arena allocation helpers for zero-copy hot paths.
//!
//! Provides thin wrappers over [`bumpalo::Bump`] for use in parse/emit
//! operations where allocation throughput matters. The arena is used to
//! avoid per-node heap allocation during parsing; nodes are bump-allocated
//! and then materialized into the final `WInstance`/`FInstance` in a
//! single pass.

use bumpalo::Bump;

/// An arena-backed string slice.
///
/// Borrows from the arena's memory, avoiding `String` allocation.
/// Used in zero-copy parsing pathways where the input buffer outlives
/// the parse operation.
#[derive(Debug, Clone, Copy)]
pub struct ArenaStr<'arena> {
    /// The borrowed string data.
    pub data: &'arena str,
}

impl<'arena> ArenaStr<'arena> {
    /// Allocate a copy of `s` in the arena.
    #[must_use]
    pub fn new(arena: &'arena Bump, s: &str) -> Self {
        let data = arena.alloc_str(s);
        Self { data }
    }

    /// Borrow the underlying string.
    #[must_use]
    pub const fn as_str(&self) -> &'arena str {
        self.data
    }
}

/// Pre-allocate an arena with estimated capacity for parsing.
///
/// The estimate is based on input size: roughly 1 node per 64 bytes of
/// input for structured formats, with each node averaging ~128 bytes of
/// arena storage (anchor string + value + metadata).
#[must_use]
pub fn parsing_arena(input_size: usize) -> Bump {
    let estimated_nodes = (input_size / 64).max(16);
    let arena_capacity = estimated_nodes * 128;
    Bump::with_capacity(arena_capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_str_roundtrip() {
        let arena = Bump::new();
        let s = ArenaStr::new(&arena, "hello world");
        assert_eq!(s.as_str(), "hello world");
    }

    #[test]
    fn parsing_arena_nonzero() {
        let arena = parsing_arena(0);
        // Even with 0-byte input, arena should have some capacity.
        let _ = arena.alloc_str("test");
    }
}
