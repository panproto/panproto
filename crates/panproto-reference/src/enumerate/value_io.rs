//! Complete finite scopes for the value-I/O page.

/// Every byte string of length at most two.
#[must_use]
pub fn byte_strings() -> Option<super::ByteStrings> {
    super::byte_strings(2)
}

/// Numerals with at most two digits and exponents from -1 to 1.
#[must_use]
pub fn numerals() -> Vec<String> {
    super::numerals(2, -1, 1)
}

/// All words of length at most two over a supplied labelled alphabet.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T]) -> Vec<Vec<T>> {
    super::sequences(alphabet, 2)
}

/// The fixed temporal lexical product used by the page.
#[must_use]
pub fn temporal_lexemes() -> Vec<String> {
    super::temporal_lexemes()
}
