//! Standoff offsets: conversion between a declared offset unit and the
//! half-open byte interval a span is keyed by.
//!
//! A span's interval counts the exact bytes of its text: the UTF-8 bytes of
//! a local string value, the bytes of a local byte-string value, or the
//! bytes loaded for an external key. Nothing is normalized and line endings
//! are not folded. An offset in a declared unit converts to a byte offset
//! when it lies at a unit boundary of that text; one past the last unit is
//! the text's length. Every other offset is refused, never clamped.

use panproto_schema::OffsetUnit;

use super::error::OffsetUnitReason;

/// Why an offset in a declared unit has no byte offset in a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitFailure {
    /// The offset counts past the end of the text.
    OutOfRange,
    /// The offset is inside a unit, or the text cannot be counted in the
    /// unit.
    Violation(OffsetUnitReason),
}

/// The byte offsets at which each unit of `text` begins, followed by the
/// text's length: the boundaries an offset in `unit` may name, the offset
/// `k` naming the `k`-th entry.
///
/// Bytes are their own units. Characters and UTF-16 code units need UTF-8
/// text. A character outside the basic multilingual plane is two UTF-16
/// code units, and the offset between them names no byte boundary: its
/// entry is `None`. Tokens take the supplied boundaries.
///
/// # Errors
///
/// [`OffsetUnitReason::TextNotUtf8`] for a character unit over bytes that
/// are not UTF-8.
pub fn unit_boundaries(
    text: &[u8],
    unit: &OffsetUnit,
    tokens: Option<&[u64]>,
) -> Result<Vec<Option<u64>>, OffsetUnitReason> {
    let len = text.len() as u64;
    match unit {
        OffsetUnit::Byte => Ok((0..=len).map(Some).collect()),
        OffsetUnit::Utf8Char => {
            let text = std::str::from_utf8(text).map_err(|_| OffsetUnitReason::TextNotUtf8)?;
            Ok(text
                .char_indices()
                .map(|(index, _)| Some(index as u64))
                .chain(std::iter::once(Some(len)))
                .collect())
        }
        OffsetUnit::Utf16 => {
            let text = std::str::from_utf8(text).map_err(|_| OffsetUnitReason::TextNotUtf8)?;
            let mut boundaries = Vec::with_capacity(text.len() + 1);
            for (index, character) in text.char_indices() {
                boundaries.push(Some(index as u64));
                if character.len_utf16() == 2 {
                    boundaries.push(None);
                }
            }
            boundaries.push(Some(len));
            Ok(boundaries)
        }
        OffsetUnit::Token => Ok(tokens.unwrap_or(&[]).iter().copied().map(Some).collect()),
    }
}

/// Whether `boundaries` are valid token boundaries of a text of `len`
/// bytes: they start at zero, increase strictly and end at `len`.
#[must_use]
pub fn valid_token_boundaries(boundaries: &[u64], len: u64) -> bool {
    boundaries.first() == Some(&0)
        && boundaries.last() == Some(&len)
        && boundaries.windows(2).all(|pair| pair[0] < pair[1])
}

/// The byte offset an offset in a unit names.
///
/// # Errors
///
/// [`UnitFailure::OutOfRange`] past the last boundary and
/// [`UnitFailure::Violation`] inside a unit.
pub fn to_byte(boundaries: &[Option<u64>], offset: u64) -> Result<u64, UnitFailure> {
    let index = usize::try_from(offset).map_err(|_| UnitFailure::OutOfRange)?;
    match boundaries.get(index) {
        None => Err(UnitFailure::OutOfRange),
        Some(None) => Err(UnitFailure::Violation(OffsetUnitReason::NotOnBoundary)),
        Some(Some(byte)) => Ok(*byte),
    }
}

/// The offset in a unit that names the byte offset `byte`.
///
/// # Errors
///
/// [`UnitFailure::OutOfRange`] past the text and
/// [`UnitFailure::Violation`] at a byte inside a unit.
pub fn from_byte(boundaries: &[Option<u64>], byte: u64) -> Result<u64, UnitFailure> {
    let last = boundaries
        .iter()
        .rev()
        .find_map(|entry| *entry)
        .unwrap_or(0);
    if byte > last {
        return Err(UnitFailure::OutOfRange);
    }
    boundaries
        .iter()
        .position(|entry| *entry == Some(byte))
        .map(|index| index as u64)
        .ok_or(UnitFailure::Violation(OffsetUnitReason::NotOnBoundary))
}

/// A declared interval converted to its half-open byte interval.
///
/// With `end_inclusive`, `end` names the last unit of the span, so the byte
/// end is the boundary after it, and a span cannot be empty: `end + 1 ==
/// start` is [`OffsetUnitReason::EmptyInclusive`].
///
/// # Errors
///
/// [`UnitFailure::OutOfRange`] when an end lies past the text or the start
/// lies after the end, [`UnitFailure::Violation`] with the offset that is
/// inside a unit.
pub fn declared_to_bytes(
    boundaries: &[Option<u64>],
    start: u64,
    end: u64,
    end_inclusive: bool,
) -> Result<(u64, u64), (UnitFailure, u64)> {
    let exclusive_end = if end_inclusive {
        let next = end.checked_add(1).ok_or((UnitFailure::OutOfRange, end))?;
        if next == start {
            return Err((
                UnitFailure::Violation(OffsetUnitReason::EmptyInclusive),
                end,
            ));
        }
        next
    } else {
        end
    };
    if start > exclusive_end {
        return Err((UnitFailure::OutOfRange, start));
    }
    let byte_start = to_byte(boundaries, start).map_err(|failure| (failure, start))?;
    let byte_end = to_byte(boundaries, exclusive_end).map_err(|failure| (failure, end))?;
    Ok((byte_start, byte_end))
}

/// A half-open byte interval converted to the declared form, the inverse of
/// [`declared_to_bytes`] on its image.
///
/// # Errors
///
/// [`UnitFailure::Violation`] when a byte end is inside a unit or an
/// inclusive declaration would need an empty span, and
/// [`UnitFailure::OutOfRange`] when the interval lies past the text.
pub fn bytes_to_declared(
    boundaries: &[Option<u64>],
    byte_start: u64,
    byte_end: u64,
    end_inclusive: bool,
) -> Result<(u64, u64), UnitFailure> {
    if byte_start > byte_end {
        return Err(UnitFailure::OutOfRange);
    }
    let start = from_byte(boundaries, byte_start)?;
    let exclusive_end = from_byte(boundaries, byte_end)?;
    if end_inclusive {
        if exclusive_end == start {
            return Err(UnitFailure::Violation(OffsetUnitReason::EmptyInclusive));
        }
        Ok((start, exclusive_end - 1))
    } else {
        Ok((start, exclusive_end))
    }
}

/// The number of units of a text in the declared unit: the last boundary's
/// index.
#[must_use]
pub const fn unit_len(boundaries: &[Option<u64>]) -> u64 {
    boundaries.len().saturating_sub(1) as u64
}
