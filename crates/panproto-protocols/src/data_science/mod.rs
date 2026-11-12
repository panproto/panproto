//! Data science and analytics protocol definitions.

/// Apache Arrow schema protocol definition and parser/emitter.
pub mod arrow;
/// DataFrame schema (pandera-style) protocol definition and parser/emitter.
pub mod dataframe;
/// Frictionless Table Schema descriptors: reading, checking and writing.
pub mod frictionless;
/// Apache Parquet schema protocol definition and parser/emitter.
pub mod parquet;
