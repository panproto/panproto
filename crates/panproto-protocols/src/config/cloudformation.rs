//! AWS `CloudFormation` template protocol definition.
//!
//! Uses Group A theory: constrained multigraph + W-type.
//!
//! A template (format version `2010-09-09`) is JSON or YAML, and this crate
//! links no grammar for either: `panproto_io::config::cloudformation::read_template`
//! reads it through the shared JSON or YAML route of `panproto-io` (feature
//! `tree-sitter`) into a schema of this protocol, and both schema dispatches
//! refuse the protocol with an error naming that reader.
//!
//! The schema has one `template` vertex joined by a `declares` edge, named
//! by the logical ID, to a `parameter`, `mapping`, `condition`, `resource` or
//! `output` vertex per declaration, and a `pseudo-parameter` vertex per
//! pseudo parameter a reference names. The references a template declares
//! are edges: `ref` (a `Ref`, or an `Fn::Sub` variable without a dot),
//! `get-att` (an `Fn::GetAtt`, or an `Fn::Sub` variable with one, named by
//! the attribute), `find-in-map`, `uses-condition` (an `Fn::If` or a
//! `Condition` function), and the resource attributes `condition` and
//! `depends-on`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::theories;

/// The fields the reader keeps as constraints.
///
/// Each holds the field's value as canonical JSON text: the template's `AWSTemplateFormatVersion`,
/// `Description` and `Metadata`; a parameter's fields; a mapping's `Map`
/// and a condition's `Definition`; a resource's `Type`, `Properties`,
/// `Metadata` and policies; an output's `Value` and `Export`.
pub const CONSTRAINT_SORTS: &[&str] = &[
    "AWSTemplateFormatVersion",
    "Description",
    "Metadata",
    "AllowedPattern",
    "AllowedValues",
    "ConstraintDescription",
    "Default",
    "MaxLength",
    "MaxValue",
    "MinLength",
    "MinValue",
    "NoEcho",
    "Type",
    "Map",
    "Definition",
    "Properties",
    "CreationPolicy",
    "DeletionPolicy",
    "UpdatePolicy",
    "UpdateReplacePolicy",
    "Value",
    "Export",
];

/// The vertex kinds of a template schema.
const KINDS: &[&str] = &[
    "template",
    "parameter",
    "mapping",
    "condition",
    "resource",
    "output",
    "pseudo-parameter",
];

/// Returns the `CloudFormation` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "cloudformation".into(),
        schema_theory: "ThCfnSchema".into(),
        instance_theory: "ThCfnInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: KINDS.iter().map(|kind| (*kind).into()).collect(),
        constraint_sorts: CONSTRAINT_SORTS
            .iter()
            .map(|sort| (*sort).to_owned())
            .collect(),
        has_order: true,
        nominal_identity: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for `CloudFormation`.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThCfnSchema", "ThCfnInstance");
}

fn rule(kind: &str, src: &[&str], tgt: &[&str]) -> EdgeRule {
    let names = |kinds: &[&str]| kinds.iter().map(|k| (*k).to_owned()).collect();
    EdgeRule {
        edge_kind: kind.into(),
        src_kinds: names(src),
        tgt_kinds: names(tgt),
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    let holders = &["condition", "resource", "output"][..];
    vec![
        rule(
            "declares",
            &["template"],
            &["parameter", "mapping", "condition", "resource", "output"],
        ),
        rule(
            "ref",
            holders,
            &["parameter", "resource", "pseudo-parameter"],
        ),
        rule("get-att", &["resource", "output"], &["resource"]),
        rule("find-in-map", holders, &["mapping"]),
        rule("uses-condition", holders, &["condition"]),
        rule("condition", &["resource", "output"], &["condition"]),
        rule("depends-on", &["resource"], &["resource"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_declares_every_kept_field_as_a_constraint_sort() {
        let p = protocol();
        assert_eq!(p.name, "cloudformation");
        for sort in CONSTRAINT_SORTS {
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
        assert!(registry.contains_key("ThCfnSchema"));
    }
}
