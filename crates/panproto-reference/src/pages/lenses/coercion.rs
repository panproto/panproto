//! The sort coercion protolens at the instance level.
//!
//! A sort coercion leaves the schema's vertices and edges as they are and
//! rewrites the value of every node of a vertex of the coerced kind by its
//! forward map; put takes a view value back by its inverse map. The maps
//! are parameters: the reference takes them as functions on its own values,
//! so the expression language that production evaluates them with is the
//! trusted boundary, checked on its own correctness pages.

use std::collections::{BTreeMap, BTreeSet};

use crate::migration::Migration;
use crate::model::{Graph, Name, Node};
use crate::restrict::{
    LayerRefusal, PredicateRefusal, PutFieldRefusal, ValueLayer, ValueLayerSemantics,
};
use crate::schema::Schema;
use crate::value::{Presence, Value};

/// A value map: the image of a value, or `None` where the map is undefined.
pub type ValueMap = fn(&Value) -> Option<Value>;

/// The value layer of a sort coercion.
#[derive(Clone, Debug)]
pub struct SortCoercion {
    /// Name of the coercion, recorded in the layer's canonical descriptor.
    pub name: &'static str,
    /// The coerced vertex kind.
    pub sort: &'static str,
    /// The vertex ids of the coerced kind in the schemas this layer is used
    /// with: restoration, which sees a view node's anchor and not its kind,
    /// applies the inverse at these vertices only.
    pub vertices: &'static [&'static str],
    /// The forward map applied at get.
    pub forward: ValueMap,
    /// The inverse map applied at put to an edited value, when the
    /// coercion declares one.
    pub inverse: Option<ValueMap>,
}

/// The field name under which a value write is reported.
const VALUE_FIELD: &str = "__value__";

impl SortCoercion {
    /// Definition: The target and migration of a sort coercion at a schema:
    /// the schema itself, every vertex and edge surviving under its own
    /// name, and every vertex of the coerced kind transformed.
    ///
    /// Book anchors: `L-lenses-25`.
    #[must_use]
    pub fn target_and_migration(&self, schema: &Schema) -> (Schema, Migration) {
        let transformed: BTreeSet<Name> = schema
            .vertices
            .iter()
            .filter(|(id, kind)| kind.as_str() == self.sort && self.coerces(id))
            .map(|(id, _)| id.clone())
            .collect();
        let migration = Migration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.clone(),
            output_fields: transformed
                .iter()
                .map(|vertex| (vertex.clone(), Vec::new()))
                .collect(),
            transformed,
            ..Migration::default()
        };
        (schema.clone(), migration)
    }

    fn coerces(&self, anchor: &str) -> bool {
        self.vertices.contains(&anchor)
    }

    /// The forward image of a node value: a present value's image under the
    /// forward map (`Err` where the map is undefined), and an absent or null
    /// value as it is.
    fn forward_presence(&self, value: Option<&Presence>) -> Result<Option<Presence>, ()> {
        match value {
            Some(Presence::Present(value)) => (self.forward)(value)
                .map(|image| Some(Presence::Present(image)))
                .ok_or(()),
            other => Ok(other.cloned()),
        }
    }
}

impl ValueLayer for SortCoercion {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/sort-coercion",
            version: 1,
            canonical_payload: self.name.as_bytes().to_vec(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Ok(true)
    }

    /// Definition: Get rewrites a present value by the forward map and
    /// leaves an absent or null value as it is; a value outside the map's
    /// domain is refused, naming the node's own value.
    ///
    /// Book anchors: `L-lenses-25`.
    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        if !self.coerces(&node.anchor) {
            return Ok((node.fields.clone(), node.value.clone()));
        }
        let value = self
            .forward_presence(node.value.as_ref())
            .map_err(|()| LayerRefusal {
                kind: "field_transform_failed".to_owned(),
                detail: VALUE_FIELD.to_owned(),
            })?;
        Ok((node.fields.clone(), value))
    }

    /// Definition: Put restores a coerced node's value.
    ///
    /// It keeps the recorded source value when the view still shows its
    /// forward image, and keeps a view value that the forward map fixes when
    /// get recorded none. Any other view value is an edit: a coercion with
    /// no inverse refuses it with `TransformNotInvertible`; one with an
    /// inverse takes a present value back by the inverse (refusing a value
    /// outside the inverse's domain the same way) and keeps an absent or
    /// null one. A node at a vertex the coercion does not cover keeps its
    /// view value.
    ///
    /// Book anchors: `L-lenses-25`.
    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        let fields = original_fields
            .cloned()
            .unwrap_or_else(|| view.fields.clone());
        if !self.coerces(&view.anchor) {
            return Ok((fields, view.value.clone()));
        }
        let refused = || PutFieldRefusal::TransformNotInvertible {
            field: VALUE_FIELD.to_owned(),
        };
        if let Some(original) = original_value
            && self.forward_presence(original.as_ref()).as_ref() == Ok(&view.value)
        {
            return Ok((fields, original.clone()));
        }
        if original_value.is_none()
            && self.forward_presence(view.value.as_ref()).as_ref() == Ok(&view.value)
        {
            return Ok((fields, view.value.clone()));
        }
        let inverse = self.inverse.ok_or_else(refused)?;
        match &view.value {
            Some(Presence::Present(value)) => {
                let source = inverse(value).ok_or_else(refused)?;
                Ok((fields, Some(Presence::Present(source))))
            }
            other => Ok((fields, other.clone())),
        }
    }
}
