//! Database schema protocol definitions.

/// Cassandra CQL protocol definition and parser/emitter.
pub mod cassandra;
/// DynamoDB protocol definition and parser/emitter.
pub mod dynamodb;
mod keyword;
/// MongoDB Schema Validation protocol definition and parser/emitter.
pub mod mongodb;
/// Neo4j graph database protocol definition and parser/emitter.
pub mod neo4j;
/// SQL DDL protocol definition and parser/emitter.
pub mod postgresql;
/// Redis RediSearch protocol definition and parser/emitter.
pub mod redis;
