//! AT Protocol content identifiers in string form.
//!
//! The Lexicon `cid` format follows the AT Protocol data model's blessed
//! CID set (<https://atproto.com/specs/data-model>, "Link and CID Formats"),
//! aligned with the DASL CID specification (<https://dasl.ing/cid.html>).
//! Its string form is therefore exactly 59 ASCII bytes: the lowercase
//! multibase prefix `b`, followed by unpadded lowercase RFC 4648 base32 of
//! a 36-byte `CIDv1`. The binary form contains version `0x01`, either the raw
//! (`0x55`) or DRISL/dag-cbor (`0x71`) codec, the SHA-256 multihash code
//! `0x12`, digest length `0x20`, and exactly 32 digest bytes.
//!
//! Checking the exact encoded length before decoding is also the resource
//! bound: the decoder performs 58 fixed-alphabet symbol lookups and writes
//! into a fixed 36-byte array. No input-dependent allocation or arbitrary
//! radix conversion occurs.

/// The lowercase RFC 4648 base32 alphabet used by DASL CID strings.
const BASE32_LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
/// The number of bytes in the blessed binary CID form.
const CID_BINARY_LEN: usize = 36;
/// The number of unpadded base32 symbols encoding [`CID_BINARY_LEN`] bytes.
const CID_BASE32_LEN: usize = (CID_BINARY_LEN * 8).div_ceil(5);
/// The number of bytes in a SHA-256 digest.
const CID_DIGEST_LEN: usize = 32;

/// Decodes the exact unpadded lowercase-base32 body of a blessed CID.
///
/// A 36-byte input uses 58 symbols and leaves two pad bits. RFC 4648
/// section 3.5 requires those bits to be zero for a canonical encoding.
fn decode_cid_base32(body: &str) -> Option<[u8; CID_BINARY_LEN]> {
    if body.len() != CID_BASE32_LEN || !body.is_ascii() {
        return None;
    }

    let mut output = [0_u8; CID_BINARY_LEN];
    let mut output_index = 0;
    let mut buffer: u16 = 0;
    let mut held: u32 = 0;

    for symbol in body.bytes() {
        let value = BASE32_LOWER
            .iter()
            .position(|candidate| *candidate == symbol)?;
        buffer = (buffer << 5) | u16::try_from(value).ok()?;
        held += 5;
        if held >= 8 {
            held -= 8;
            *output.get_mut(output_index)? = u8::try_from(buffer >> held).ok()?;
            output_index += 1;
            buffer &= (1_u16 << held) - 1;
        }
    }

    (output_index == CID_BINARY_LEN && buffer == 0).then_some(output)
}

/// Whether `text` is a blessed AT Protocol CID in canonical string form.
pub(super) fn is_atproto_cid(text: &str) -> bool {
    let Some(body) = text.strip_prefix('b') else {
        return false;
    };
    let Some(bytes) = decode_cid_base32(body) else {
        return false;
    };
    matches!(
        bytes.as_slice(),
        [0x01, 0x55 | 0x71, 0x12, 0x20, digest @ ..] if digest.len() == CID_DIGEST_LEN
    )
}

#[cfg(test)]
mod tests {
    use super::{CID_BASE32_LEN, decode_cid_base32, is_atproto_cid};

    const DRISL_CID: &str = "bafyreidfayvfuwqa7qlnopdjiqrxzs6blmoeu4rujcjtnci5beludirz2a";
    const RAW_CID: &str = "bafkreichzyovokfzmymz36p5jibbjrhsur6n7hjnzxrpbt5jaydp2szvna";

    #[test]
    fn canonical_blessed_cids_are_accepted() {
        assert!(is_atproto_cid(DRISL_CID));
        assert!(is_atproto_cid(RAW_CID));
    }

    #[test]
    fn decoder_has_exact_fixed_boundaries() {
        let body = &DRISL_CID[1..];
        assert_eq!(body.len(), CID_BASE32_LEN);
        assert!(decode_cid_base32(&body[..body.len() - 1]).is_none());
        assert!(decode_cid_base32(body).is_some());
        assert!(decode_cid_base32(&format!("{body}a")).is_none());
    }

    #[test]
    fn long_malformed_input_is_refused_before_decoding() {
        let input = format!("b{}", "a".repeat(1_000_000));
        assert!(!is_atproto_cid(&input));
    }
}
