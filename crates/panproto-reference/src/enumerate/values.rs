//! Complete finite scopes for the value-law page.

use crate::float::Format;

/// Numerals with at most two significand digits and exponents from -1 to 1.
#[must_use]
pub fn numerals() -> Vec<String> {
    super::numerals(2, -1, 1)
}

/// All binary16 bit patterns.
#[must_use]
pub fn binary16_patterns() -> Vec<u64> {
    super::boundary_bits(Format::B16, None, &(0..1024).collect::<Vec<_>>())
}

/// The fixed temporal lexical product used by the page.
#[must_use]
pub fn temporal_lexemes() -> Vec<String> {
    super::temporal_lexemes()
}

/// Every byte string of length at most two.
#[must_use]
pub fn byte_strings() -> Option<super::ByteStrings> {
    super::byte_strings(2)
}
