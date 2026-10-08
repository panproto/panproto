//! Operations assigned to the value-law page.

pub use crate::bytes::{
    latin1_decode_encode, latin1_encode_decode, optional, padded_bytes_decode_encode,
    padded_text_encode_decode,
};
pub use crate::canonical::{decimal_bytes, decode_value};
pub use crate::coerce::{
    coerce, coerce_dialect, coerce_discriminated, coerce_tagged, coerce_value,
};
pub use crate::exact::{Decimal, DecimalValue};
pub use crate::float::{
    binary16, computed_nan, exact_reading, narrow_bits, nearest_reading, round_to, shortest,
    widen_bits,
};
pub use crate::temporal::{
    canonical_bytes, cmp_chronological, display_parse, from_epoch_count, parse_display,
    to_epoch_count, unwritten_zulu,
};
pub use crate::value::{Value, eval, representation};
