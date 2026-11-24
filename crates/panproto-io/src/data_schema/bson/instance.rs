//! The BSON tree encoding: documents to instances and back, and the `bson`
//! instance codec.

use std::collections::{HashMap, HashSet};
use std::fmt;

use panproto_expr::exact::{Temporal, TimeUnit};
use panproto_inst::{FInstance, FieldPresence, Node, NodeShape, Value, WInstance};
use panproto_protocols::data_schema::bson::{
    ARRAY, CODE_WITH_SCOPE, DOCUMENT, ELEMENT, ELEMENT_EDGE, ITEM, SCOPE, VALUE, VALUE_VERTICES,
    edges,
};
use panproto_schema::{Edge, Schema};

use super::decimal128::representable;
use super::{BsonDocument, BsonValue, MAX_DEPTH, decode, encode, options_in_order};
use crate::error::{EmitInstanceError, ParseInstanceError};
use crate::traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// The protocol name.
const PROTOCOL: &str = "bson";

/// An instance the BSON tree encoding refuses, or a document too large to
/// encode: the node at fault and why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("node {node}: {kind}")]
pub struct BsonInstanceError {
    /// The node at fault (for an edge, the node it leaves; for the
    /// exhaustion of node ids, the last id given).
    pub node: u32,
    /// Why.
    pub kind: BsonInstanceErrorKind,
}

/// Why an instance has no BSON document.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BsonInstanceErrorKind {
    /// The document needs more nodes than an instance can number.
    NodeIdsExhausted,
    /// The instance's root is not one of its nodes.
    NoRoot,
    /// An edge leaves or enters a node the instance does not hold.
    MissingNode,
    /// The root is not anchored at `document`.
    RootAnchor,
    /// The node carries data the encoding has no place for: a
    /// discriminator, extra fields, annotations, a position or a shape.
    Presentation,
    /// The node's value is not a value of its anchor, as the BSON module
    /// states (for instance an int32 outside the 32-bit range, a key holding
    /// U+0000, unsorted regular expression options, or a decimal decimal128
    /// does not hold as written).
    Value,
    /// The edge is not an edge of the encoding from the node's anchor to
    /// its target's anchor (which is how a node anchored at a vertex the
    /// encoding does not have is found, since only the root is reached
    /// without an edge).
    UnexpectedEdge {
        /// The edge's name.
        name: String,
    },
    /// An element without exactly one `value` edge, or a code with scope
    /// without exactly one `scope` edge.
    EdgeCount {
        /// The edge's name.
        name: String,
        /// How many the node has.
        count: usize,
    },
    /// The node is the target of a second edge, or of an edge on a path
    /// from itself.
    SharedNode,
    /// No path of edges leads from the root to the node.
    Unreachable,
    /// The node is nested deeper than [`MAX_DEPTH`] documents, arrays and
    /// scopes.
    Depth,
}

impl fmt::Display for BsonInstanceErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeIdsExhausted => f.write_str("the document needs more nodes than ids exist"),
            Self::NoRoot => f.write_str("the root is not a node of the instance"),
            Self::MissingNode => f.write_str("an edge names a node the instance does not hold"),
            Self::RootAnchor => f.write_str("the root is not a document"),
            Self::Presentation => {
                f.write_str("the node carries data the BSON tree encoding has no place for")
            }
            Self::Value => f.write_str("the node's value is not a value of its anchor"),
            Self::UnexpectedEdge { name } => write!(
                f,
                "edge {name:?} is not an edge of the BSON tree encoding here"
            ),
            Self::EdgeCount { name, count } => write!(f, "the node has {count} {name:?} edges"),
            Self::SharedNode => f.write_str("the node is reached by more than one edge"),
            Self::Unreachable => f.write_str("the node is not reached from the root"),
            Self::Depth => write!(f, "the node is nested deeper than {MAX_DEPTH}"),
        }
    }
}

const fn refusal(node: u32, kind: BsonInstanceErrorKind) -> BsonInstanceError {
    BsonInstanceError { node, kind }
}

/// The encoding's edge `name` from `src` to `tgt`.
fn encoding_edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: name.into(),
        name: Some(name.into()),
    }
}

/// The vertex a value's node is anchored at.
fn anchor(value: &BsonValue) -> &'static str {
    let index = match value {
        BsonValue::MinKey => 19,
        BsonValue::MaxKey => 20,
        other => usize::from(other.type_byte()) - 1,
    };
    VALUE_VERTICES[index]
}

/// An opaque value of type `type_` with `fields`.
fn opaque(type_: &str, fields: [(&str, Value); 2]) -> Value {
    Value::Opaque {
        type_: type_.to_owned(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    }
}

/// The scalar a value's node holds, as the BSON module states: a double
/// as a float, a string, code, symbol or code with scope's code as a
/// string, an object id as its twelve bytes, a boolean, a datetime as the
/// UTC date-time of its millisecond count (`Temporal::from_epoch_count` in
/// milliseconds), null, an int32 and an int64 as an integer, a decimal128 as
/// an exact decimal, and a binary (`subtype`, `bytes`), regular expression
/// (`pattern`, `options`), DB pointer (`namespace`, `id`) and timestamp
/// (`time`, `increment`) as an opaque value of its anchor's name with those
/// fields. Documents, arrays, undefined and the keys hold none.
fn scalar(value: &BsonValue) -> Option<Value> {
    Some(match value {
        BsonValue::Double(x) => Value::Float(*x),
        BsonValue::String(text)
        | BsonValue::Code(text)
        | BsonValue::Symbol(text)
        | BsonValue::CodeWithScope { code: text, .. } => Value::Str(text.clone()),
        BsonValue::Binary { subtype, bytes } => opaque(
            "binary",
            [
                ("subtype", Value::Int(i64::from(*subtype))),
                ("bytes", Value::Bytes(bytes.clone())),
            ],
        ),
        BsonValue::ObjectId(id) => Value::Bytes(id.to_vec()),
        BsonValue::Boolean(flag) => Value::Bool(*flag),
        BsonValue::DateTime(ms) => Value::Temporal(
            Temporal::from_epoch_count(i128::from(*ms), TimeUnit::Millis, true).ok()?,
        ),
        BsonValue::Null => Value::Null,
        BsonValue::Regex { pattern, options } => opaque(
            "regex",
            [
                ("pattern", Value::Str(pattern.clone())),
                ("options", Value::Str(options.clone())),
            ],
        ),
        BsonValue::DbPointer { namespace, id } => opaque(
            "db-pointer",
            [
                ("namespace", Value::Str(namespace.clone())),
                ("id", Value::Bytes(id.to_vec())),
            ],
        ),
        BsonValue::Int32(n) => Value::Int(i64::from(*n)),
        BsonValue::Timestamp { time, increment } => opaque(
            "timestamp",
            [
                ("time", Value::Int(i64::from(*time))),
                ("increment", Value::Int(i64::from(*increment))),
            ],
        ),
        BsonValue::Int64(n) => Value::Int(*n),
        BsonValue::Decimal128(decimal) => Value::Decimal(decimal.clone()),
        BsonValue::Document(_)
        | BsonValue::Array(_)
        | BsonValue::Undefined
        | BsonValue::MinKey
        | BsonValue::MaxKey => return None,
    })
}

/// The builder of one instance.
#[derive(Default)]
struct Builder {
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
    next: u32,
}

impl Builder {
    fn node(&mut self, anchor: &str, value: Option<Value>) -> Result<u32, BsonInstanceError> {
        let id = self.next;
        self.next = id
            .checked_add(1)
            .ok_or_else(|| refusal(id, BsonInstanceErrorKind::NodeIdsExhausted))?;
        let mut node = Node::new(id, anchor);
        if let Some(value) = value {
            node = node.with_value(FieldPresence::Present(value));
        }
        self.nodes.insert(id, node);
        Ok(id)
    }

    fn arc(&mut self, parent: u32, child: u32, name: &str) {
        let edge = encoding_edge(
            &self.nodes[&parent].anchor,
            &self.nodes[&child].anchor,
            name,
        );
        self.arcs.push((parent, child, edge));
    }

    fn document(&mut self, document: &BsonDocument) -> Result<u32, BsonInstanceError> {
        let id = self.node(DOCUMENT, None)?;
        for (key, value) in &document.0 {
            let element = self.node(ELEMENT, Some(Value::Str(key.clone())))?;
            self.arc(id, element, ELEMENT_EDGE);
            let child = self.value(value)?;
            self.arc(element, child, VALUE);
        }
        Ok(id)
    }

    fn value(&mut self, value: &BsonValue) -> Result<u32, BsonInstanceError> {
        match value {
            BsonValue::Document(document) => self.document(document),
            BsonValue::Array(items) => {
                let id = self.node(ARRAY, None)?;
                for item in items {
                    let child = self.value(item)?;
                    self.arc(id, child, ITEM);
                }
                Ok(id)
            }
            BsonValue::CodeWithScope { scope, .. } => {
                let id = self.node(CODE_WITH_SCOPE, scalar(value))?;
                let child = self.document(scope)?;
                self.arc(id, child, SCOPE);
                Ok(id)
            }
            other => self.node(anchor(other), scalar(other)),
        }
    }
}

/// The instance of a document in the BSON tree encoding.
///
/// Nodes are numbered from 0 in document order: a document's node, then
/// for each element its element node and its value's nodes; an array's
/// node, then its items' nodes; a code with scope's node, then its scope's.
/// Each node's edges are added in that order.
///
/// # Errors
///
/// [`BsonInstanceErrorKind::NodeIdsExhausted`] when the document needs more
/// nodes than an instance can number.
pub fn to_instance(document: &BsonDocument) -> Result<WInstance, BsonInstanceError> {
    let mut builder = Builder::default();
    let root = builder.document(document)?;
    Ok(WInstance::new(
        builder.nodes,
        builder.arcs,
        Vec::new(),
        root,
        DOCUMENT.into(),
    ))
}

/// Whether `schema` holds the BSON tree encoding: each of its vertices
/// with its kind and each of its edges.
#[must_use]
pub fn schema_holds_encoding(schema: &Schema) -> bool {
    std::iter::once(ELEMENT)
        .chain(VALUE_VERTICES.iter().copied())
        .all(|kind| {
            schema
                .vertices
                .get(kind)
                .is_some_and(|vertex| vertex.kind.as_ref() == kind)
        })
        && edges()
            .iter()
            .all(|(src, tgt, name)| schema.edges.contains_key(&encoding_edge(src, tgt, name)))
}

/// The reader of one instance.
struct Reader<'a> {
    instance: &'a WInstance,
    children: HashMap<u32, Vec<(u32, &'a Edge)>>,
    reached: HashSet<u32>,
}

impl Reader<'_> {
    /// Check the node `id` and its out-edges, mark its children reached,
    /// and return them in order.
    fn check(&mut self, id: u32) -> Result<Vec<u32>, BsonInstanceError> {
        let instance = self.instance;
        let node = &instance.nodes[&id];
        let fail = |kind| Err(refusal(id, kind));
        let anchor = node.anchor.as_ref();
        if node.discriminator.is_some()
            || !node.extra_fields.is_empty()
            || !node.annotations.is_empty()
            || node.position.is_some()
            || node.shape != NodeShape::Plain
        {
            return fail(BsonInstanceErrorKind::Presentation);
        }
        let mut out = Vec::new();
        for (child, edge) in self.children.get(&id).into_iter().flatten() {
            let target = instance.nodes[child].anchor.as_ref();
            let name = edge.name.as_deref().unwrap_or_default();
            let allowed = edges().iter().any(|(s, t, n)| {
                *s == anchor && *t == target && *n == name && **edge == encoding_edge(s, t, n)
            });
            if !allowed {
                return fail(BsonInstanceErrorKind::UnexpectedEdge {
                    name: name.to_owned(),
                });
            }
            if !self.reached.insert(*child) || *child == instance.root {
                return Err(refusal(*child, BsonInstanceErrorKind::SharedNode));
            }
            out.push(*child);
        }
        for (holder, name) in [(ELEMENT, VALUE), (CODE_WITH_SCOPE, SCOPE)] {
            if anchor == holder && out.len() != 1 {
                return fail(BsonInstanceErrorKind::EdgeCount {
                    name: name.to_owned(),
                    count: out.len(),
                });
            }
        }
        Ok(out)
    }

    /// The document whose node is `id`, at nesting `depth`.
    fn document(&mut self, id: u32, depth: usize) -> Result<BsonDocument, BsonInstanceError> {
        if depth > MAX_DEPTH {
            return Err(refusal(id, BsonInstanceErrorKind::Depth));
        }
        let elements_of = self.check(id)?;
        self.none(id)?;
        let mut elements = Vec::new();
        for element in elements_of {
            let children = self.check(element)?;
            let key = match self.held(element) {
                Some(Value::Str(key)) if !key.contains('\0') => key.clone(),
                _ => return Err(refusal(element, BsonInstanceErrorKind::Value)),
            };
            elements.push((key, self.value(children[0], depth)?));
        }
        Ok(BsonDocument(elements))
    }

    /// The scalar node `id` holds, if any.
    fn held(&self, id: u32) -> Option<&Value> {
        match &self.instance.nodes[&id].value {
            Some(FieldPresence::Present(value)) => Some(value),
            _ => None,
        }
    }

    /// The value whose node is `id`, inside a container at `depth`.
    fn value(&mut self, id: u32, depth: usize) -> Result<BsonValue, BsonInstanceError> {
        let anchor = self.instance.nodes[&id].anchor.to_string();
        match anchor.as_str() {
            DOCUMENT => self.document(id, depth + 1).map(BsonValue::Document),
            ARRAY => {
                if depth + 1 > MAX_DEPTH {
                    return Err(refusal(id, BsonInstanceErrorKind::Depth));
                }
                let children = self.check(id)?;
                self.none(id)?;
                let mut items = Vec::new();
                for child in children {
                    items.push(self.value(child, depth + 1)?);
                }
                Ok(BsonValue::Array(items))
            }
            CODE_WITH_SCOPE => {
                let children = self.check(id)?;
                let Some(Value::Str(code)) = self.held(id).cloned() else {
                    return Err(refusal(id, BsonInstanceErrorKind::Value));
                };
                let scope = self.document(children[0], depth + 1)?;
                Ok(BsonValue::CodeWithScope { code, scope })
            }
            _ => {
                self.check(id)?;
                leaf(
                    &anchor,
                    self.held(id),
                    self.instance.nodes[&id].value.is_some(),
                )
                .ok_or_else(|| refusal(id, BsonInstanceErrorKind::Value))
            }
        }
    }

    /// Refuse a value on a node that holds none.
    fn none(&self, id: u32) -> Result<(), BsonInstanceError> {
        if self.instance.nodes[&id].value.is_some() {
            return Err(refusal(id, BsonInstanceErrorKind::Value));
        }
        Ok(())
    }
}

/// The two fields of an opaque value of type `type_`, by name.
fn fields<'v>(value: &'v Value, type_: &str, names: [&str; 2]) -> Option<[&'v Value; 2]> {
    match value {
        Value::Opaque { type_: t, fields } if t == type_ && fields.len() == 2 => {
            Some([fields.get(names[0])?, fields.get(names[1])?])
        }
        _ => None,
    }
}

fn twelve(value: &Value) -> Option<[u8; 12]> {
    match value {
        Value::Bytes(bytes) => <[u8; 12]>::try_from(bytes.as_slice()).ok(),
        _ => None,
    }
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::Str(text) => Some(text.clone()),
        _ => None,
    }
}

fn integer<T: TryFrom<i64>>(value: &Value) -> Option<T> {
    match value {
        Value::Int(n) => T::try_from(*n).ok(),
        _ => None,
    }
}

/// The value of a leaf anchored at `anchor` holding `held`; `valued` says
/// whether the node has any value, present or not.
fn leaf(anchor: &str, held: Option<&Value>, valued: bool) -> Option<BsonValue> {
    let Some(value) = held else {
        return match (anchor, valued) {
            ("undefined", false) => Some(BsonValue::Undefined),
            ("min-key", false) => Some(BsonValue::MinKey),
            ("max-key", false) => Some(BsonValue::MaxKey),
            _ => None,
        };
    };
    Some(match (anchor, value) {
        ("double", Value::Float(x)) => BsonValue::Double(*x),
        ("string", _) => BsonValue::String(text(value)?),
        ("code", _) => BsonValue::Code(text(value)?),
        ("symbol", _) => BsonValue::Symbol(text(value)?),
        ("binary", _) => {
            let [subtype, bytes] = fields(value, "binary", ["subtype", "bytes"])?;
            let Value::Bytes(bytes) = bytes else {
                return None;
            };
            BsonValue::Binary {
                subtype: integer(subtype)?,
                bytes: bytes.clone(),
            }
        }
        ("object-id", _) => BsonValue::ObjectId(twelve(value)?),
        ("boolean", Value::Bool(flag)) => BsonValue::Boolean(*flag),
        ("datetime", Value::Temporal(temporal)) => BsonValue::DateTime(
            temporal
                .to_epoch_count(TimeUnit::Millis, true)
                .and_then(|ms| i64::try_from(ms).ok())?,
        ),
        ("null", Value::Null) => BsonValue::Null,
        ("regex", _) => {
            let [pattern, options] = fields(value, "regex", ["pattern", "options"])?;
            let (pattern, options) = (text(pattern)?, text(options)?);
            if pattern.contains('\0') || options.contains('\0') || !options_in_order(&options) {
                return None;
            }
            BsonValue::Regex { pattern, options }
        }
        ("db-pointer", _) => {
            let [namespace, id] = fields(value, "db-pointer", ["namespace", "id"])?;
            BsonValue::DbPointer {
                namespace: text(namespace)?,
                id: twelve(id)?,
            }
        }
        ("int32", _) => BsonValue::Int32(integer(value)?),
        ("timestamp", _) => {
            let [time, increment] = fields(value, "timestamp", ["time", "increment"])?;
            BsonValue::Timestamp {
                time: integer(time)?,
                increment: integer(increment)?,
            }
        }
        ("int64", _) => BsonValue::Int64(integer(value)?),
        ("decimal128", Value::Decimal(decimal)) if representable(decimal) => {
            BsonValue::Decimal128(decimal.clone())
        }
        _ => return None,
    })
}

/// The document of an instance in the BSON tree encoding.
///
/// The nodes are checked from the root, depth first in the order of their
/// edges: the root must be a node anchored at `document`; each node must
/// carry no discriminator, extra fields, annotations, position or shape,
/// and its out-edges must be edges of the encoding between the anchors of
/// their ends, to nodes no other edge reaches;
/// an element must hold a string with no U+0000 and have one `value` edge,
/// a code with scope one `scope` edge, and every value node the scalar its
/// anchor holds (or none for documents, arrays, undefined and the keys);
/// nesting must not exceed [`MAX_DEPTH`]. Then, in ascending id order, a
/// node no edge from the root reaches is refused.
///
/// # Errors
///
/// The [`BsonInstanceError`] of the first fault.
pub fn from_instance(instance: &WInstance) -> Result<BsonDocument, BsonInstanceError> {
    let root = instance.root;
    let Some(node) = instance.nodes.get(&root) else {
        return Err(refusal(root, BsonInstanceErrorKind::NoRoot));
    };
    if node.anchor.as_ref() != DOCUMENT {
        return Err(refusal(root, BsonInstanceErrorKind::RootAnchor));
    }
    let mut children: HashMap<u32, Vec<(u32, &Edge)>> = HashMap::new();
    for (parent, child, edge) in &instance.arcs {
        for id in [parent, child] {
            if !instance.nodes.contains_key(id) {
                return Err(refusal(*id, BsonInstanceErrorKind::MissingNode));
            }
        }
        children.entry(*parent).or_default().push((*child, edge));
    }
    let mut reader = Reader {
        instance,
        children,
        reached: HashSet::new(),
    };
    let document = reader.document(root, 1)?;
    let mut ids: Vec<u32> = instance.nodes.keys().copied().collect();
    ids.sort_unstable();
    if let Some(id) = ids
        .into_iter()
        .find(|id| *id != root && !reader.reached.contains(id))
    {
        return Err(refusal(id, BsonInstanceErrorKind::Unreachable));
    }
    Ok(document)
}

/// The `bson` instance codec: reads a binary BSON document into the BSON
/// tree encoding and writes an instance in that encoding as its canonical
/// binary document.
///
/// Both directions require the schema to hold the encoding
/// ([`schema_holds_encoding`]), as
/// [`tree_schema`](panproto_protocols::data_schema::bson::tree_schema)
/// builds it.
#[derive(Clone, Copy, Debug, Default)]
pub struct BsonCodec;

impl BsonCodec {
    fn schema_check(schema: &Schema) -> Result<(), String> {
        if schema_holds_encoding(schema) {
            Ok(())
        } else {
            Err("the schema does not hold the BSON tree encoding".to_owned())
        }
    }
}

impl InstanceParser for BsonCodec {
    fn protocol_name(&self) -> &str {
        PROTOCOL
    }

    fn native_repr(&self) -> NativeRepr {
        NativeRepr::Tree
    }

    fn parse_wtype(&self, schema: &Schema, input: &[u8]) -> Result<WInstance, ParseInstanceError> {
        Self::schema_check(schema).map_err(|message| ParseInstanceError::SchemaMismatch {
            protocol: PROTOCOL.into(),
            message,
        })?;
        let document = decode(input).map_err(|error| ParseInstanceError::Bson {
            protocol: PROTOCOL.into(),
            error,
        })?;
        to_instance(&document).map_err(|error| ParseInstanceError::SchemaMismatch {
            protocol: PROTOCOL.into(),
            message: error.to_string(),
        })
    }

    fn parse_functor(
        &self,
        _schema: &Schema,
        _input: &[u8],
    ) -> Result<FInstance, ParseInstanceError> {
        Err(ParseInstanceError::UnsupportedRepresentation {
            protocol: PROTOCOL.into(),
            requested: NativeRepr::Table,
            native: NativeRepr::Tree,
        })
    }
}

impl InstanceEmitter for BsonCodec {
    fn protocol_name(&self) -> &str {
        PROTOCOL
    }

    fn emit_wtype(
        &self,
        schema: &Schema,
        instance: &WInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Self::schema_check(schema).map_err(|message| EmitInstanceError::Emit {
            protocol: PROTOCOL.into(),
            message,
        })?;
        let document =
            from_instance(instance).map_err(|error| EmitInstanceError::BsonInstance {
                protocol: PROTOCOL.into(),
                error,
            })?;
        encode(&document).map_err(|error| EmitInstanceError::BsonWrite {
            protocol: PROTOCOL.into(),
            error,
        })
    }

    fn emit_functor(
        &self,
        _schema: &Schema,
        _instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        Err(EmitInstanceError::UnsupportedRepresentation {
            protocol: PROTOCOL.into(),
            requested: NativeRepr::Table,
            native: NativeRepr::Tree,
        })
    }
}
