//! CoNLL-U instance codec via SIMD-accelerated tab splitting.
//!
//! Parses CoNLL-U formatted text into an `FInstance` with `sentence`
//! and `token` tables. Uses `memchr` for SIMD line/field scanning.

use panproto_inst::{FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::tabular_pathway;
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// CoNLL-U instance codec.
pub struct ConlluCodec;

impl Default for ConlluCodec {
    fn default() -> Self {
        Self
    }
}

impl ConlluCodec {
    /// Create a new CoNLL-U codec.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl InstanceParser for ConlluCodec {
    fn protocol_name(&self) -> &str {
        "conllu"
    }

    fn native_repr(&self) -> NativeRepr {
        NativeRepr::Functor
    }

    fn parse_wtype(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<WInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: "conllu".into(),
            requested: NativeRepr::WType,
            native: NativeRepr::Functor,
        })
    }

    fn parse_functor(
        &self,
        schema: &Schema,
        input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        tabular_pathway::parse_conllu(schema, input, "conllu")
    }
}

impl InstanceEmitter for ConlluCodec {
    fn protocol_name(&self) -> &str {
        "conllu"
    }

    fn emit_wtype(
        &self,
        _schema: &Schema,
        _instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: "conllu".into(),
            requested: NativeRepr::WType,
            native: NativeRepr::Functor,
        })
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        tabular_pathway::emit_conllu(instance, "conllu")
    }
}
