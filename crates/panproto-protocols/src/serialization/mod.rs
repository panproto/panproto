//! Serialization and IDL protocol definitions.

/// ASN.1 protocol definition and parser/emitter.
pub mod asn1;

/// Apache Avro protocol definition and `.avsc` parser/emitter.
pub mod avro;

/// Microsoft Bond protocol definition and parser/emitter.
pub mod bond;

/// Cap'n Proto protocol definition and `.capnp` parser/emitter.
pub mod capnproto;

/// FlatBuffers protocol definition and `.fbs` parser/emitter.
pub mod flatbuffers;

/// MessagePack Schema protocol definition and JSON parser/emitter.
pub mod msgpack_schema;

/// Protobuf protocol definition and `.proto` parser/emitter.
pub mod protobuf;

/// Apache Thrift protocol definition and `.thrift` parser/emitter.
pub mod thrift;
