//! Instance codecs for data science protocols.
//!
//! All data science protocols use JSON as their instance representation.
//! `DataFrame` interchange is natively JSON. `Parquet` and `Arrow` use
//! JSON-encoded record batches; dedicated binary parsers (`arrow-rs`)
//! can be registered as drop-in replacements for higher throughput.

use crate::json_codec::JsonCodec;
use crate::registry::ProtocolRegistry;

/// Register all data science protocol codecs with the registry.
pub fn register_all(registry: &mut ProtocolRegistry) {
    registry.register(JsonCodec::new("dataframe"));
    registry.register(JsonCodec::new("parquet"));
    registry.register(JsonCodec::new("arrow"));
}
