//! Clean fixture: no protocol names, no programming-language identifiers.
//!
//! This file is consumed via `include_str!` by the genericity test harness.
//! It intentionally avoids any denylisted terms.

/// A perfectly generic function.
///
/// # Examples
///
/// ```
/// // Doc examples may mention protocol names like atproto without tripping
/// // the denylist: the scanner skips content inside `# Examples` sections.
/// let _ = 1;
/// ```
pub fn generic_function() -> u32 {
    42
}

pub struct GenericRecord {
    pub value: u32,
}

pub mod generic_submodule {
    pub const GENERIC_CONST: u32 = 7;
}
