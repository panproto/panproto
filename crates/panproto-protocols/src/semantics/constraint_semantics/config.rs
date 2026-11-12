//! Constraint-semantics rows for the configuration protocols
//! (`cloudformation`, `ansible`, `k8s-crd`).
//!
//! `required` is a presence flag of a parameter or property and `default` a
//! default value; both describe. The flag is about the element that holds
//! the parameter or property, which must have it (the schema's required
//! edge), not about its own value. Every field the `cloudformation` reader
//! keeps (a parameter's type, default, allowed values and bounds, a
//! resource's properties and policies, a mapping, a condition's
//! definition, an output's value) is kept as its JSON text and not
//! evaluated by validation, so it describes. A CRD schema node's `format`
//! is a value format of `OpenAPI` v3; every other keyword the CRD reader
//! keeps (its value validations, CEL rules, list and map types, and the
//! resource's and versions' fields) is kept as written and not evaluated by
//! validation, so it describes.

use panproto_gat::Name;
use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;

/// The row giving sort `sort` of protocol `protocol` the meaning `semantics`
/// builds.
const fn row(
    protocol: &'static str,
    sort: &'static str,
    semantics: fn() -> ConstraintSemantics,
) -> ConstraintRow {
    ConstraintRow {
        protocol,
        sort,
        semantics,
    }
}

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("cloudformation", "AWSTemplateFormatVersion", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "Description", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "Metadata", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "AllowedPattern", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "AllowedValues", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "ConstraintDescription", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "Default", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "MaxLength", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "MaxValue", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "MinLength", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "MinValue", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "NoEcho", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "Type", || ConstraintSemantics::Annotation),
    row("cloudformation", "Map", || ConstraintSemantics::Annotation),
    row("cloudformation", "Definition", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "Properties", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "CreationPolicy", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "DeletionPolicy", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "UpdatePolicy", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "UpdateReplacePolicy", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "Value", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "Export", || {
        ConstraintSemantics::Annotation
    }),
    row("ansible", "required", || ConstraintSemantics::Annotation),
    row("ansible", "default", || ConstraintSemantics::Annotation),
    row("k8s-crd", "description", || ConstraintSemantics::Annotation),
    row("k8s-crd", "title", || ConstraintSemantics::Annotation),
    row("k8s-crd", "format", || {
        ConstraintSemantics::Format(Name::from("k8s-crd"))
    }),
    row("k8s-crd", "default", || ConstraintSemantics::Annotation),
    row("k8s-crd", "example", || ConstraintSemantics::Annotation),
    row("k8s-crd", "enum", || ConstraintSemantics::Annotation),
    row("k8s-crd", "nullable", || ConstraintSemantics::Annotation),
    row("k8s-crd", "maximum", || ConstraintSemantics::Annotation),
    row("k8s-crd", "exclusiveMaximum", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "minimum", || ConstraintSemantics::Annotation),
    row("k8s-crd", "exclusiveMinimum", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "multipleOf", || ConstraintSemantics::Annotation),
    row("k8s-crd", "maxLength", || ConstraintSemantics::Annotation),
    row("k8s-crd", "minLength", || ConstraintSemantics::Annotation),
    row("k8s-crd", "pattern", || ConstraintSemantics::Annotation),
    row("k8s-crd", "maxItems", || ConstraintSemantics::Annotation),
    row("k8s-crd", "minItems", || ConstraintSemantics::Annotation),
    row("k8s-crd", "uniqueItems", || ConstraintSemantics::Annotation),
    row("k8s-crd", "maxProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "minProperties", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "required", || ConstraintSemantics::Annotation),
    row("k8s-crd", "allOf", || ConstraintSemantics::Annotation),
    row("k8s-crd", "anyOf", || ConstraintSemantics::Annotation),
    row("k8s-crd", "oneOf", || ConstraintSemantics::Annotation),
    row("k8s-crd", "not", || ConstraintSemantics::Annotation),
    row("k8s-crd", "externalDocs", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "x-kubernetes-preserve-unknown-fields", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "x-kubernetes-embedded-resource", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "x-kubernetes-int-or-string", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "x-kubernetes-list-type", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "x-kubernetes-list-map-keys", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "x-kubernetes-map-type", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "x-kubernetes-validations", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "group", || ConstraintSemantics::Annotation),
    row("k8s-crd", "scope", || ConstraintSemantics::Annotation),
    row("k8s-crd", "kind", || ConstraintSemantics::Annotation),
    row("k8s-crd", "plural", || ConstraintSemantics::Annotation),
    row("k8s-crd", "singular", || ConstraintSemantics::Annotation),
    row("k8s-crd", "listKind", || ConstraintSemantics::Annotation),
    row("k8s-crd", "shortNames", || ConstraintSemantics::Annotation),
    row("k8s-crd", "categories", || ConstraintSemantics::Annotation),
    row("k8s-crd", "metadata", || ConstraintSemantics::Annotation),
    row("k8s-crd", "status", || ConstraintSemantics::Annotation),
    row("k8s-crd", "served", || ConstraintSemantics::Annotation),
    row("k8s-crd", "storage", || ConstraintSemantics::Annotation),
    row("k8s-crd", "deprecated", || ConstraintSemantics::Annotation),
    row("k8s-crd", "deprecationWarning", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "subresources", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "additionalPrinterColumns", || {
        ConstraintSemantics::Annotation
    }),
    row("k8s-crd", "selectableFields", || {
        ConstraintSemantics::Annotation
    }),
];
