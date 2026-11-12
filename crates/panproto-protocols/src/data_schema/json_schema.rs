//! JSON Schema protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! A JSON Schema document (dialect 2020-12) is JSON, and this crate links no
//! JSON grammar: `panproto_io::data_schema::json_schema::read_schema` reads it
//! through the shared JSON route of `panproto-io` (feature `tree-sitter`) into
//! a schema of this protocol, and both schema dispatches refuse the protocol
//! with an error naming that reader.
//!
//! Every subschema is a vertex. A boolean schema is a `true` or `false`
//! vertex; a schema object is a vertex whose kind is its `type` (`null`,
//! `boolean`, `object`, `array`, `number`, `string` or `integer`), `union`
//! for a `type` array, or `any` for a schema without `type`. Each applicator
//! keyword is an edge kind
//! ([`APPLICATOR_EDGES`](crate::data_schema::json_schema::APPLICATOR_EDGES))
//! to the subschemas it holds, `$ref` a `ref` edge and `$dynamicRef` a
//! `dynamic-ref` edge to the subschema they resolve to, and every other
//! keyword a constraint holding the keyword's value as canonical JSON text
//! ([`KEPT`](crate::data_schema::json_schema::KEPT),
//! [`EMPTY_KEPT`](crate::data_schema::json_schema::EMPTY_KEPT) for an empty
//! object of subschemas, or
//! [`ANNOTATION`](crate::data_schema::json_schema::ANNOTATION) for a keyword
//! 2020-12 does not define).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::theories;

/// The dialect the reader reads, as `$schema` names it.
pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// The 2020-12 keywords kept as constraints of their subschema's vertex.
///
/// Each constraint holds the keyword's value as canonical JSON text. They
/// are the core keywords other than the references and `$defs`, and the
/// validation, format, content and meta-data keywords.
pub const KEPT: &[&str] = &[
    "$schema",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$vocabulary",
    "$comment",
    "type",
    "enum",
    "const",
    "multipleOf",
    "maximum",
    "exclusiveMaximum",
    "minimum",
    "exclusiveMinimum",
    "maxLength",
    "minLength",
    "pattern",
    "maxItems",
    "minItems",
    "uniqueItems",
    "maxContains",
    "minContains",
    "maxProperties",
    "minProperties",
    "required",
    "dependentRequired",
    "format",
    "contentEncoding",
    "contentMediaType",
    "title",
    "description",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
    "examples",
];

/// The applicators whose value is an object of subschemas. One whose
/// object is empty (`"properties": {}`) holds no subschema to reach by an
/// edge, so it is kept as a constraint of its name holding `{}`.
pub const EMPTY_KEPT: &[&str] = &[
    "$defs",
    "properties",
    "patternProperties",
    "dependentSchemas",
];

/// The constraint sort of a keyword 2020-12 does not define, which the
/// specification collects as an annotation: its value is the JSON text of
/// an object holding the one member as written.
pub const ANNOTATION: &str = "annotation";

/// The edge kind of each applicator keyword (and of `$defs`).
pub const APPLICATOR_EDGES: &[(&str, &str)] = &[
    ("$defs", "def"),
    ("properties", "prop"),
    ("patternProperties", "pattern-prop"),
    ("additionalProperties", "additional-props"),
    ("propertyNames", "property-names"),
    ("dependentSchemas", "dependent-schema"),
    ("prefixItems", "prefix-item"),
    ("items", "items"),
    ("contains", "contains"),
    ("allOf", "all-of"),
    ("anyOf", "any-of"),
    ("oneOf", "one-of"),
    ("not", "not"),
    ("if", "if"),
    ("then", "then"),
    ("else", "else"),
    ("unevaluatedItems", "unevaluated-items"),
    ("unevaluatedProperties", "unevaluated-props"),
    ("contentSchema", "content-schema"),
];

/// The vertex kinds of a subschema.
pub const KINDS: &[&str] = &[
    "null", "boolean", "object", "array", "number", "string", "integer", "union", "any", "true",
    "false",
];

/// Returns the JSON Schema protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    let mut constraint_sorts: Vec<String> = KEPT
        .iter()
        .chain(EMPTY_KEPT)
        .map(|sort| (*sort).to_owned())
        .collect();
    constraint_sorts.push(ANNOTATION.to_owned());
    let mut protocol = Protocol {
        name: "json-schema".into(),
        schema_theory: "ThJsonSchemaSchema".into(),
        instance_theory: "ThJsonSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: KINDS.iter().map(|kind| (*kind).to_owned()).collect(),
        constraint_sorts,
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for JSON Schema with a theory registry.
///
/// JSON Schema shares the constrained-multigraph schema theory and
/// W-type instance theory with `ATProto`, `bson`, and `cddl`, so it
/// registers through the shared helper rather than composing the
/// colimit by hand.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThJsonSchemaSchema",
        "ThJsonSchemaInstance",
    );
}

/// Well-formedness rules for JSON Schema edges: every edge joins two
/// subschemas, and a keyword applies to a schema object whatever its `type`
/// (`properties` beside `"type": "string"` constrains nothing, but it is
/// part of the document), so no rule narrows the kinds an edge joins.
fn edge_rules() -> Vec<EdgeRule> {
    APPLICATOR_EDGES
        .iter()
        .map(|(_, kind)| *kind)
        .chain(["ref", "dynamic-ref"])
        .map(|kind| EdgeRule {
            edge_kind: kind.into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_declares_every_kept_keyword_and_the_annotation_sort() {
        let p = protocol();
        assert_eq!(p.name, "json-schema");
        assert_eq!(p.constraint_sorts.len(), KEPT.len() + EMPTY_KEPT.len() + 1);
        for sort in KEPT.iter().chain(EMPTY_KEPT).chain([&ANNOTATION]) {
            assert!(
                p.constraint_sorts.iter().any(|s| s == sort),
                "{sort} is not declared"
            );
        }
        assert_eq!(p.edge_rules.len(), APPLICATOR_EDGES.len() + 2);
    }

    #[test]
    fn register_theories_registers_the_schema_and_instance_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThJsonSchemaSchema"));
        assert!(registry.contains_key("ThJsonSchemaInstance"));
    }
}
