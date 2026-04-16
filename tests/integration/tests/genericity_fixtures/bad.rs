//! Dirty fixture: contains protocol names and language identifiers.
//!
//! Consumed via `include_str!` by the genericity test harness to verify
//! that the scanners actually detect violations.

/// Documentation that names a protocol directly: bsky is the AT Proto
/// application cluster, and this reference should trip the denylist.
pub fn handle_bsky() -> u32 {
    0
}

// A Rust-specific identifier should trip the language-name denylist.
pub struct RustBridge {
    pub kind: u32,
}
