//! Exact byte/text reference mappings.
//!
//! See the byte-text laws on `correctness/values.md`.

/// Why text cannot denote bytes in the selected form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteTextError {
    /// A character lies above U+00FF.
    NonLatin1,
    /// Required base64 padding is absent or malformed.
    InvalidPadding,
    /// A character is outside the selected base64 alphabet or has
    /// non-zero unused bits.
    InvalidBase64,
}

/// Map each byte `b` to U+0000 + `b`.
#[must_use]
pub fn latin1_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

/// Map each code point U+0000 through U+00FF to its byte.
///
/// # Errors
///
/// [`ByteTextError::NonLatin1`] when any code point is above U+00FF.
pub fn latin1_encode(text: &str) -> Result<Vec<u8>, ByteTextError> {
    text.chars()
        .map(|character| u8::try_from(u32::from(character)).map_err(|_| ByteTextError::NonLatin1))
        .collect()
}

/// The Latin-1 decode-then-encode law.
#[must_use]
pub fn latin1_decode_encode(bytes: &[u8]) -> bool {
    latin1_encode(&latin1_decode(bytes)).as_deref() == Ok(bytes)
}

/// The Latin-1 encode-then-decode law on its exact domain.
#[must_use]
pub fn latin1_encode_decode(text: &str) -> bool {
    latin1_encode(text).is_ok_and(|bytes| latin1_decode(&bytes) == text)
}

/// Normalize optional base64 padding to the unique padded length.
///
/// This operation checks only the padding shape. Alphabet and quantum
/// decoding remain the caller's format-specific operation.
///
/// # Errors
///
/// [`ByteTextError::InvalidPadding`] when padding is misplaced, has the
/// wrong length, or cannot complete one four-character quantum.
pub fn optional(text: &str) -> Result<String, ByteTextError> {
    let (body, padding) = text
        .find('=')
        .map_or((text, ""), |at| (&text[..at], &text[at..]));
    if !padding.bytes().all(|byte| byte == b'=') || padding.len() > 2 {
        return Err(ByteTextError::InvalidPadding);
    }
    if !body
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
    {
        return Err(ByteTextError::InvalidBase64);
    }
    let needed = (4 - body.len() % 4) % 4;
    if needed > 2 || (!padding.is_empty() && padding.len() != needed) {
        return Err(ByteTextError::InvalidPadding);
    }
    Ok(format!("{body}{}", "=".repeat(needed)))
}

const fn sextet(byte: u8) -> Option<u8> {
    Some(match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    })
}

/// Encode bytes with the standard base64 alphabet and required padding.
#[must_use]
pub fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let word = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(char::from(ALPHABET[((word >> 18) & 63) as usize]));
        output.push(char::from(ALPHABET[((word >> 12) & 63) as usize]));
        if chunk.len() > 1 {
            output.push(char::from(ALPHABET[((word >> 6) & 63) as usize]));
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(char::from(ALPHABET[(word & 63) as usize]));
        } else {
            output.push('=');
        }
    }
    output
}

/// Decode standard base64, optionally requiring its canonical padding.
///
/// # Errors
///
/// [`ByteTextError::InvalidPadding`] or
/// [`ByteTextError::InvalidBase64`] at the first malformed quantum.
pub fn decode_base64(text: &str, require_padding: bool) -> Result<Vec<u8>, ByteTextError> {
    let normalized = optional(text)?;
    if require_padding && normalized != text {
        return Err(ByteTextError::InvalidPadding);
    }
    let mut output = Vec::new();
    for quantum in normalized.as_bytes().chunks_exact(4) {
        let padding = quantum
            .iter()
            .rev()
            .take_while(|byte| **byte == b'=')
            .count();
        let mut values = [0u8; 4];
        for (index, byte) in quantum.iter().enumerate() {
            if *byte != b'=' {
                values[index] = sextet(*byte).ok_or(ByteTextError::InvalidBase64)?;
            }
        }
        if (padding == 2 && values[1] & 0x0f != 0) || (padding == 1 && values[2] & 0x03 != 0) {
            return Err(ByteTextError::InvalidBase64);
        }
        let word = u32::from(values[0]) << 18
            | u32::from(values[1]) << 12
            | u32::from(values[2]) << 6
            | u32::from(values[3]);
        output.push(((word >> 16) & 0xff) as u8);
        if padding < 2 {
            output.push(((word >> 8) & 0xff) as u8);
        }
        if padding == 0 {
            output.push((word & 0xff) as u8);
        }
    }
    Ok(output)
}

/// The padded accepted-text encode/decode law: an accepted padded spelling
/// is already the optional-padding normal form.
#[must_use]
pub fn padded_text_encode_decode(text: &str) -> bool {
    decode_base64(text, true).is_ok_and(|bytes| encode_base64(&bytes) == text)
}

/// The padded byte decode/encode law.
#[must_use]
pub fn padded_bytes_decode_encode(bytes: &[u8]) -> bool {
    decode_base64(&encode_base64(bytes), true).as_deref() == Ok(bytes)
}
