//! Kubernetes `CustomResourceDefinition` protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! A CRD document (`apiextensions.k8s.io/v1`, Kubernetes 1.37) is YAML, and
//! this crate links no YAML grammar: `panproto_io::config::k8s_crd::read_crd`
//! reads it through the shared YAML route of `panproto-io` (feature
//! `tree-sitter`) into a schema of this protocol, and both schema dispatches
//! refuse the protocol with an error naming that reader.
//!
//! The schema has one `custom-resource` vertex named by the CRD's
//! `metadata.name`, a `version` vertex `<crd>/<version>` per served version
//! joined to it by a `version` edge, and the structural schema of each version
//! below a `schema` edge: an `object`, `array`, `string`, `integer`, `number`,
//! `boolean`, `int-or-string` or `any` vertex per schema node, joined by `prop`
//! edges (named by the property), `items` edges and `values` edges (an
//! `additionalProperties` schema).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::theories;

/// The keywords of a `JSONSchemaProps` node that the reader keeps as
/// constraints of the node's vertex, each holding the keyword's value as
/// canonical JSON text.
pub const SCHEMA_CONSTRAINTS: &[&str] = &[
    "description",
    "title",
    "format",
    "default",
    "example",
    "enum",
    "nullable",
    "maximum",
    "exclusiveMaximum",
    "minimum",
    "exclusiveMinimum",
    "multipleOf",
    "maxLength",
    "minLength",
    "pattern",
    "maxItems",
    "minItems",
    "uniqueItems",
    "maxProperties",
    "minProperties",
    "required",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "externalDocs",
    "x-kubernetes-preserve-unknown-fields",
    "x-kubernetes-embedded-resource",
    "x-kubernetes-int-or-string",
    "x-kubernetes-list-type",
    "x-kubernetes-list-map-keys",
    "x-kubernetes-map-type",
    "x-kubernetes-validations",
];

/// The fields of the CRD kept as constraints of its `custom-resource`
/// vertex, each holding the field's value as canonical JSON text.
pub const RESOURCE_CONSTRAINTS: &[&str] = &[
    "group",
    "scope",
    "kind",
    "plural",
    "singular",
    "listKind",
    "shortNames",
    "categories",
    "metadata",
    "status",
];

/// The fields of a version kept as constraints of its `version` vertex,
/// each holding the field's value as canonical JSON text.
pub const VERSION_CONSTRAINTS: &[&str] = &[
    "served",
    "storage",
    "deprecated",
    "deprecationWarning",
    "subresources",
    "additionalPrinterColumns",
    "selectableFields",
];

/// Returns the K8s CRD protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    let constraint_sorts = SCHEMA_CONSTRAINTS
        .iter()
        .chain(RESOURCE_CONSTRAINTS)
        .chain(VERSION_CONSTRAINTS)
        .map(|sort| (*sort).to_owned())
        .collect();
    let mut protocol = Protocol {
        name: "k8s-crd".into(),
        schema_theory: "ThK8sCrdSchema".into(),
        instance_theory: "ThK8sCrdInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "custom-resource".into(),
            "version".into(),
            "object".into(),
            "array".into(),
            "string".into(),
            "integer".into(),
            "number".into(),
            "boolean".into(),
            "int-or-string".into(),
            "any".into(),
        ],
        constraint_sorts,
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for K8s CRD.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThK8sCrdSchema", "ThK8sCrdInstance");
}

/// The vertex kinds of a structural schema node.
const SCHEMA_KINDS: &[&str] = &[
    "object",
    "array",
    "string",
    "integer",
    "number",
    "boolean",
    "int-or-string",
    "any",
];

fn edge_rules() -> Vec<EdgeRule> {
    let schema_kinds = || SCHEMA_KINDS.iter().map(|kind| (*kind).into()).collect();
    vec![
        EdgeRule {
            edge_kind: "version".into(),
            src_kinds: vec!["custom-resource".into()],
            tgt_kinds: vec!["version".into()],
        },
        EdgeRule {
            edge_kind: "schema".into(),
            src_kinds: vec!["version".into()],
            tgt_kinds: vec!["object".into()],
        },
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: schema_kinds(),
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
            tgt_kinds: schema_kinds(),
        },
        EdgeRule {
            edge_kind: "values".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: schema_kinds(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_declares_every_kept_field_as_a_constraint_sort() {
        let p = protocol();
        assert_eq!(p.name, "k8s-crd");
        for sort in SCHEMA_CONSTRAINTS
            .iter()
            .chain(RESOURCE_CONSTRAINTS)
            .chain(VERSION_CONSTRAINTS)
        {
            assert!(
                p.constraint_sorts.iter().any(|s| s == sort),
                "{sort} is not declared"
            );
        }
    }

    #[test]
    fn register_theories_registers_the_schema_theory() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThK8sCrdSchema"));
    }
}
