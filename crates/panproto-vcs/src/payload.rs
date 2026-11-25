//! Encodings of the payloads of data sets, complements and edit logs.
//!
//! A [`DataSetObject`], a [`ComplementObject`] and an [`EditLogObject`] each
//! carry their payload as bytes together with a [`PayloadEncoding`]: the
//! shape of instance the payload belongs to and the version of that shape's
//! encoding. This module is the one place those bytes are written and read,
//! for all three shapes. Version 1 of each encoding is `MessagePack` with
//! structs as positional arrays (`rmp_serde::to_vec`) of the value below:
//!
//! | object      | tree                  | table               | graph               |
//! |-------------|-----------------------|---------------------|---------------------|
//! | data set    | `Vec<WInstance>`      | `FInstance`         | `GInstance`         |
//! | complement  | `Vec<Complement>`     | `TableComplement`   | `GraphComplement`   |
//! | edit log    | `Vec<TreeEdit>`       | `Vec<TableDelta>`   | `Vec<GraphDelta>`   |
//!
//! A tree data set holds one tree per record and a tree complement one
//! complement per record, in record order; a table or graph data set holds
//! one instance. The table and graph values serialize their maps in
//! ascending key order, so each has exactly one encoding, and decoding
//! refuses bytes that are not that encoding with
//! [`VcsError::NonCanonicalPayload`]: an object's identifier is then a
//! function of the value it holds. The tree encodings are the ones tree data
//! has always been stored in; their identity is the bytes as written, which
//! decoding does not re-derive, so that every tree object a repository
//! already holds keeps decoding.
//!
//! Decoding refuses, before reading the bytes, an encoding version other than
//! 1 with [`VcsError::UnsupportedPayloadEncoding`]; it refuses bytes that do
//! not decode whole as the stated value, or a value that contradicts the
//! object's record or edit count, with [`VcsError::UndecodablePayload`].

use std::io::Cursor;

use panproto_inst::identity::{AnyComplement, ComplementShape, Encoding, decode_complement};
use panproto_inst::{
    Complement, FInstance, GInstance, GraphComplement, GraphDelta, TableComplement, TableDelta,
    TreeEdit, WInstance,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{
    ComplementObject, DataSetObject, EditLogObject, InstanceShape, PayloadEncoding,
};

/// The object kind names the refusals carry.
const DATASET: &str = "dataset";
const COMPLEMENT: &str = "complement";
const EDIT_LOG: &str = "editlog";

/// An instance of one of the three shapes, as a data set holds it.
#[derive(Clone, Debug)]
pub enum Instance {
    /// A sequence of trees, one per record.
    Tree(Vec<WInstance>),
    /// A table instance.
    Table(FInstance),
    /// A graph instance.
    Graph(GInstance),
}

impl Instance {
    /// The instance's shape.
    #[must_use]
    pub const fn shape(&self) -> InstanceShape {
        match self {
            Self::Tree(_) => InstanceShape::Tree,
            Self::Table(_) => InstanceShape::Table,
            Self::Graph(_) => InstanceShape::Graph,
        }
    }

    /// The number of records: trees, rows or nodes.
    #[must_use]
    pub fn record_count(&self) -> u64 {
        let count = match self {
            Self::Tree(trees) => trees.len(),
            Self::Table(table) => table.total_row_count(),
            Self::Graph(graph) => graph.node_count(),
        };
        u64::try_from(count).unwrap_or(u64::MAX)
    }

    /// The instance's payload under the current encoding of its shape.
    ///
    /// # Errors
    ///
    /// [`VcsError::Serialization`] when `MessagePack` encoding fails.
    pub fn encode(&self) -> Result<(PayloadEncoding, Vec<u8>), VcsError> {
        let bytes = match self {
            Self::Tree(trees) => rmp_serde::to_vec(trees)?,
            Self::Table(table) => rmp_serde::to_vec(table)?,
            Self::Graph(graph) => rmp_serde::to_vec(graph)?,
        };
        Ok((PayloadEncoding::current(self.shape()), bytes))
    }
}

/// A complement of one of the three shapes, as a complement object holds it.
#[derive(Clone, Debug)]
pub enum ComplementPayload {
    /// One tree complement per record of the tree data set.
    Tree(Vec<Complement>),
    /// A table complement.
    Table(Box<TableComplement>),
    /// A graph complement.
    Graph(Box<GraphComplement>),
}

impl ComplementPayload {
    /// The complement's shape.
    #[must_use]
    pub const fn shape(&self) -> InstanceShape {
        match self {
            Self::Tree(_) => InstanceShape::Tree,
            Self::Table(_) => InstanceShape::Table,
            Self::Graph(_) => InstanceShape::Graph,
        }
    }

    /// The complement's payload under the current encoding of its shape.
    ///
    /// # Errors
    ///
    /// [`VcsError::Serialization`] when `MessagePack` encoding fails.
    pub fn encode(&self) -> Result<(PayloadEncoding, Vec<u8>), VcsError> {
        let bytes = match self {
            Self::Tree(complements) => rmp_serde::to_vec(complements)?,
            Self::Table(complement) => rmp_serde::to_vec(complement.as_ref())?,
            Self::Graph(complement) => rmp_serde::to_vec(complement.as_ref())?,
        };
        Ok((PayloadEncoding::current(self.shape()), bytes))
    }
}

/// An edit sequence of one of the three shapes, as an edit log holds it.
#[derive(Clone, Debug)]
pub enum EditLogPayload {
    /// Tree edits.
    Tree(Vec<TreeEdit>),
    /// Table deltas.
    Table(Vec<TableDelta>),
    /// Graph deltas.
    Graph(Vec<GraphDelta>),
}

impl EditLogPayload {
    /// The edits' shape.
    #[must_use]
    pub const fn shape(&self) -> InstanceShape {
        match self {
            Self::Tree(_) => InstanceShape::Tree,
            Self::Table(_) => InstanceShape::Table,
            Self::Graph(_) => InstanceShape::Graph,
        }
    }

    /// The number of edits.
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::Tree(edits) => edits.len(),
            Self::Table(deltas) => deltas.len(),
            Self::Graph(deltas) => deltas.len(),
        }
    }

    /// Whether the sequence holds no edit.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The edits' payload under the current encoding of their shape.
    ///
    /// # Errors
    ///
    /// [`VcsError::Serialization`] when `MessagePack` encoding fails.
    pub fn encode(&self) -> Result<(PayloadEncoding, Vec<u8>), VcsError> {
        let bytes = match self {
            Self::Tree(edits) => rmp_serde::to_vec(edits)?,
            Self::Table(deltas) => rmp_serde::to_vec(deltas)?,
            Self::Graph(deltas) => rmp_serde::to_vec(deltas)?,
        };
        Ok((PayloadEncoding::current(self.shape()), bytes))
    }
}

impl DataSetObject {
    /// The data set holding `instance` under the current encoding of its
    /// shape, for the schema `schema_id`.
    ///
    /// # Errors
    ///
    /// [`VcsError::Serialization`] when `MessagePack` encoding fails.
    pub fn from_instance(
        schema_id: ObjectId,
        instance: &Instance,
        key: Option<String>,
    ) -> Result<Self, VcsError> {
        let (encoding, data) = instance.encode()?;
        Ok(Self {
            schema_id,
            data,
            record_count: instance.record_count(),
            key,
            encoding,
        })
    }

    /// Decode the instance this data set holds.
    ///
    /// A tree payload holding source JSON, the form tree data was first
    /// stored in, is read through its schema by
    /// [`Repository::decoded_data_at`](crate::Repository::decoded_data_at)
    /// and is refused here.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnsupportedPayloadEncoding`] for a version other than 1,
    /// [`VcsError::UndecodablePayload`] for bytes that do not decode whole
    /// or a record count other than the instance's, and
    /// [`VcsError::NonCanonicalPayload`] for a table or graph payload that
    /// is not the canonical encoding of its instance.
    pub fn decode(&self) -> Result<Instance, VcsError> {
        let encoding = supported(DATASET, self.encoding)?;
        let instance = match encoding.shape {
            InstanceShape::Tree => {
                if serde_json::from_slice::<serde_json::Value>(&self.data).is_ok() {
                    return Err(undecodable(
                        DATASET,
                        encoding,
                        "the payload is source JSON, which is read through its schema".to_owned(),
                    ));
                }
                Instance::Tree(whole(DATASET, encoding, &self.data)?)
            }
            InstanceShape::Table => {
                let table: FInstance = whole(DATASET, encoding, &self.data)?;
                canonical(DATASET, encoding, &table, &self.data)?;
                Instance::Table(table)
            }
            InstanceShape::Graph => {
                let graph: GInstance = whole(DATASET, encoding, &self.data)?;
                canonical(DATASET, encoding, &graph, &self.data)?;
                Instance::Graph(graph)
            }
        };
        let actual = instance.record_count();
        if actual != self.record_count {
            return Err(undecodable(
                DATASET,
                encoding,
                format!(
                    "the object records {} records, but the payload holds {actual}",
                    self.record_count
                ),
            ));
        }
        Ok(instance)
    }
}

impl ComplementObject {
    /// The complement object holding `complement`, produced by the
    /// migration `migration_id` from the data set `data_id`.
    ///
    /// # Errors
    ///
    /// [`VcsError::Serialization`] when `MessagePack` encoding fails.
    pub fn from_payload(
        migration_id: ObjectId,
        data_id: ObjectId,
        complement: &ComplementPayload,
    ) -> Result<Self, VcsError> {
        let (encoding, bytes) = complement.encode()?;
        Ok(Self {
            migration_id,
            data_id,
            complement: bytes,
            encoding,
        })
    }

    /// Decode the complement this object holds.
    ///
    /// A tree payload is read as a sequence of tree complements, each of
    /// whose identity envelopes, when present, must name the current tree
    /// complement format; a table payload as one table complement, under the
    /// same check. A graph payload is one graph complement.
    ///
    /// # Errors
    ///
    /// As [`DataSetObject::decode`].
    pub fn decode(&self) -> Result<ComplementPayload, VcsError> {
        let encoding = supported(COMPLEMENT, self.encoding)?;
        let bytes = &self.complement;
        match encoding.shape {
            InstanceShape::Tree => {
                whole::<Vec<Complement>>(COMPLEMENT, encoding, bytes)?;
                let complements = panproto_inst::identity::decode_complements(
                    bytes,
                    Encoding::MsgpackPositional,
                    ComplementShape::WType,
                )
                .map_err(|error| undecodable(COMPLEMENT, encoding, error.to_string()))?
                .into_iter()
                .map(|decoded| match decoded {
                    AnyComplement::WType(complement) => Ok(*complement),
                    other => Err(undecodable(
                        COMPLEMENT,
                        encoding,
                        format!("decoded a {:?} complement", other.shape()),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
                Ok(ComplementPayload::Tree(complements))
            }
            InstanceShape::Table => {
                whole::<TableComplement>(COMPLEMENT, encoding, bytes)?;
                let decoded =
                    decode_complement(bytes, Encoding::MsgpackPositional, ComplementShape::Functor)
                        .map_err(|error| undecodable(COMPLEMENT, encoding, error.to_string()))?;
                let AnyComplement::Functor(table) = decoded else {
                    return Err(undecodable(
                        COMPLEMENT,
                        encoding,
                        format!("decoded a {:?} complement", decoded.shape()),
                    ));
                };
                canonical(COMPLEMENT, encoding, table.as_ref(), bytes)?;
                Ok(ComplementPayload::Table(table))
            }
            InstanceShape::Graph => {
                let graph: GraphComplement = whole(COMPLEMENT, encoding, bytes)?;
                canonical(COMPLEMENT, encoding, &graph, bytes)?;
                Ok(ComplementPayload::Graph(Box::new(graph)))
            }
        }
    }
}

impl EditLogObject {
    /// The edit log holding `edits`, made against the data set `data_id`
    /// of the schema `schema_id`, with `final_complement` the complement
    /// after the last edit.
    ///
    /// # Errors
    ///
    /// [`VcsError::Serialization`] when `MessagePack` encoding fails.
    pub fn from_payload(
        schema_id: ObjectId,
        data_id: ObjectId,
        edits: &EditLogPayload,
        final_complement: ObjectId,
    ) -> Result<Self, VcsError> {
        let (encoding, bytes) = edits.encode()?;
        Ok(Self {
            schema_id,
            data_id,
            edits: bytes,
            edit_count: u64::try_from(edits.len()).unwrap_or(u64::MAX),
            final_complement,
            encoding,
        })
    }

    /// Decode the edits this log holds.
    ///
    /// # Errors
    ///
    /// As [`DataSetObject::decode`], with the edit count in place of the
    /// record count.
    pub fn decode(&self) -> Result<EditLogPayload, VcsError> {
        let encoding = supported(EDIT_LOG, self.encoding)?;
        let bytes = &self.edits;
        let edits = match encoding.shape {
            InstanceShape::Tree => EditLogPayload::Tree(whole(EDIT_LOG, encoding, bytes)?),
            InstanceShape::Table => {
                let deltas: Vec<TableDelta> = whole(EDIT_LOG, encoding, bytes)?;
                canonical(EDIT_LOG, encoding, &deltas, bytes)?;
                EditLogPayload::Table(deltas)
            }
            InstanceShape::Graph => {
                let deltas: Vec<GraphDelta> = whole(EDIT_LOG, encoding, bytes)?;
                canonical(EDIT_LOG, encoding, &deltas, bytes)?;
                EditLogPayload::Graph(deltas)
            }
        };
        let actual = u64::try_from(edits.len()).unwrap_or(u64::MAX);
        if actual != self.edit_count {
            return Err(undecodable(
                EDIT_LOG,
                encoding,
                format!(
                    "the object records {} edits, but the payload holds {actual}",
                    self.edit_count
                ),
            ));
        }
        Ok(edits)
    }
}

/// The encoding, when this build reads it.
const fn supported(
    kind: &'static str,
    encoding: PayloadEncoding,
) -> Result<PayloadEncoding, VcsError> {
    if encoding.version == 1 {
        Ok(encoding)
    } else {
        Err(VcsError::UnsupportedPayloadEncoding { kind, encoding })
    }
}

const fn undecodable(kind: &'static str, encoding: PayloadEncoding, reason: String) -> VcsError {
    VcsError::UndecodablePayload {
        kind,
        encoding,
        reason,
    }
}

/// Decode `bytes` as one `MessagePack` value of `T` that uses every byte.
fn whole<T: DeserializeOwned>(
    kind: &'static str,
    encoding: PayloadEncoding,
    bytes: &[u8],
) -> Result<T, VcsError> {
    let mut cursor = Cursor::new(bytes);
    let value: T = rmp_serde::from_read(&mut cursor)
        .map_err(|error| undecodable(kind, encoding, error.to_string()))?;
    let used = cursor.position();
    if u64::try_from(bytes.len()).ok() != Some(used) {
        return Err(undecodable(
            kind,
            encoding,
            format!(
                "{} bytes follow the value",
                bytes
                    .len()
                    .saturating_sub(usize::try_from(used).unwrap_or(0))
            ),
        ));
    }
    Ok(value)
}

/// Refuse `bytes` unless they are the encoding of `value`.
fn canonical<T: Serialize + ?Sized>(
    kind: &'static str,
    encoding: PayloadEncoding,
    value: &T,
    bytes: &[u8],
) -> Result<(), VcsError> {
    if rmp_serde::to_vec(value)? == bytes {
        Ok(())
    } else {
        Err(VcsError::NonCanonicalPayload { kind, encoding })
    }
}
