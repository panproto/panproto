//! GraphQL type system protocol definition.
//!
//! Schema theory: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
//! Instance theory: `ThWType`.
//!
//! A GraphQL type system document (the September 2025 edition of the
//! specification, [`EDITION`](crate::api::graphql::EDITION)) is text in GraphQL's own syntax, and this
//! crate links no grammar for it: `panproto_io::api::graphql::read_graphql`
//! reads it through the bundled tree-sitter GraphQL grammar
//! (feature `tree-sitter` of `panproto-io`) into a schema of this protocol,
//! and both schema dispatches of this crate refuse the protocol with an
//! error naming that reader.
//!
//! Every vertex is a schema element whose id is its schema coordinate (the
//! specification's section 2.14): a named type is a vertex whose kind names
//! its kind ([`TYPE_KINDS`](crate::api::graphql::TYPE_KINDS)) and whose id is the type's name, a field of an
//! object or interface type a [`FIELD`](crate::api::graphql::FIELD) vertex `Type.field`, an argument an
//! [`ARGUMENT`](crate::api::graphql::ARGUMENT) vertex `Type.field(arg:)` or `@directive(arg:)`, an input
//! field an [`INPUT_FIELD`](crate::api::graphql::INPUT_FIELD) vertex `Input.field`, an enum value an
//! [`ENUM_VALUE`](crate::api::graphql::ENUM_VALUE) vertex `Enum.VALUE`, and a directive definition a
//! [`DIRECTIVE`](crate::api::graphql::DIRECTIVE) vertex `@directive`. The schema itself is the [`SCHEMA`](crate::api::graphql::SCHEMA)
//! vertex [`SCHEMA_VERTEX`](crate::api::graphql::SCHEMA_VERTEX), which no coordinate names. The edges ([`EDGES`](crate::api::graphql::EDGES)) join the schema to its root
//! operation types, a type to its fields, values, members and the
//! interfaces it implements, and a field, argument or input field to the
//! named type under its wrappers. Everything else a definition states is a
//! constraint ([`SORTS`](crate::api::graphql::SORTS)): its description, its type reference as written
//! with its list and non-null wrappers, a default value, each directive
//! applied to it, and the meaning of the built-in directives
//! `@deprecated`, `@specifiedBy` and `@oneOf`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

use crate::theories;

/// The edition of the GraphQL specification read.
pub const EDITION: &str = "September2025";

/// The kind of the schema vertex.
pub const SCHEMA: &str = "schema";

/// The id of the schema vertex: braces never occur in a GraphQL name.
pub const SCHEMA_VERTEX: &str = "{schema}";

/// The kind of a scalar type.
pub const SCALAR: &str = "scalar";
/// The kind of an object type.
pub const OBJECT: &str = "object";
/// The kind of an interface type.
pub const INTERFACE: &str = "interface";
/// The kind of a union type.
pub const UNION: &str = "union";
/// The kind of an enum type.
pub const ENUM: &str = "enum";
/// The kind of an input object type.
pub const INPUT_OBJECT: &str = "input-object";
/// The kind of a field of an object or interface type.
pub const FIELD: &str = "field";
/// The kind of an argument of a field or a directive.
pub const ARGUMENT: &str = "argument";
/// The kind of an input field of an input object type.
pub const INPUT_FIELD: &str = "input-field";
/// The kind of a value of an enum type.
pub const ENUM_VALUE: &str = "enum-value";
/// The kind of a directive definition.
pub const DIRECTIVE: &str = "directive";

/// The kinds of the named types, in the order of the specification's
/// `TypeDefinition` production.
pub const TYPE_KINDS: [&str; 6] = [SCALAR, OBJECT, INTERFACE, UNION, ENUM, INPUT_OBJECT];

/// Every vertex kind.
pub const KINDS: [&str; 12] = [
    SCHEMA,
    SCALAR,
    OBJECT,
    INTERFACE,
    UNION,
    ENUM,
    INPUT_OBJECT,
    FIELD,
    ARGUMENT,
    INPUT_FIELD,
    ENUM_VALUE,
    DIRECTIVE,
];

/// The edge from the schema to a root operation type, named `query`,
/// `mutation` or `subscription`.
pub const ROOT: &str = "root";
/// The edge from an object or interface type to a field, named by the
/// field.
pub const FIELD_EDGE: &str = "field";
/// The edge from a field or directive to an argument, named by the
/// argument.
pub const ARGUMENT_EDGE: &str = "argument";
/// The edge from an input object type to an input field, named by the
/// input field.
pub const INPUT_FIELD_EDGE: &str = "input-field";
/// The edge from an enum type to a value, named by the value.
pub const VALUE_EDGE: &str = "value";
/// The edge from a field, argument or input field to the named type of its
/// type reference, unnamed.
pub const TYPE_EDGE: &str = "type";
/// The edge from an object or interface type to an interface it declares
/// it implements, named by the interface.
pub const IMPLEMENTS: &str = "implements";
/// The edge from a union type to a member type, named by the member.
pub const MEMBER: &str = "member";

/// The kinds of types an output position (a field's type) may name.
const OUTPUT_TYPES: &[&str] = &[SCALAR, OBJECT, INTERFACE, UNION, ENUM];
/// The kinds of types an input position (an argument's or input field's
/// type) may name.
const INPUT_TYPES: &[&str] = &[SCALAR, ENUM, INPUT_OBJECT];

/// Every edge kind, with the vertex kinds it leaves and the vertex kinds it
/// reaches.
pub const EDGES: &[(&str, &[&str], &[&str])] = &[
    (ROOT, &[SCHEMA], &[OBJECT]),
    (FIELD_EDGE, &[OBJECT, INTERFACE], &[FIELD]),
    (ARGUMENT_EDGE, &[FIELD, DIRECTIVE], &[ARGUMENT]),
    (INPUT_FIELD_EDGE, &[INPUT_OBJECT], &[INPUT_FIELD]),
    (VALUE_EDGE, &[ENUM], &[ENUM_VALUE]),
    (TYPE_EDGE, &[FIELD], OUTPUT_TYPES),
    (TYPE_EDGE, &[ARGUMENT, INPUT_FIELD], INPUT_TYPES),
    (IMPLEMENTS, &[OBJECT, INTERFACE], &[INTERFACE]),
    (MEMBER, &[UNION], &[OBJECT]),
];

/// The constraint sort of a description: the string's value.
pub const DESCRIPTION: &str = "description";
/// The constraint sort of a type reference: the reference as GraphQL text,
/// the named type with its list (`[ ]`) and non-null (`!`) wrappers.
pub const TYPE: &str = "type";
/// The constraint sort of a default value: the value as canonical GraphQL
/// text.
pub const DEFAULT: &str = "default";
/// The constraint sort of a directive applied to a definition.
///
/// Its value is the application as canonical GraphQL text. A definition
/// has one per application, in the order applied; the built-in directives
/// are read into their own sorts instead.
pub const APPLIED: &str = "directive";
/// The constraint sort of `@deprecated`: the deprecation reason.
pub const DEPRECATED: &str = "deprecated";
/// The constraint sort of `@specifiedBy`: the scalar specification URL.
pub const SPECIFIED_BY: &str = "specified-by";
/// The constraint sort of `@oneOf` on an input object type: `true`.
pub const ONE_OF: &str = "one-of";
/// The constraint sort of a repeatable directive definition: `true`.
pub const REPEATABLE: &str = "repeatable";
/// The constraint sort of a location of a directive definition, one per
/// location in the order listed.
pub const LOCATION: &str = "location";
/// The constraint sort of a built-in scalar (`Int`, `Float`, `String`,
/// `Boolean`, `ID`) the document references and does not define: `true`.
pub const BUILT_IN: &str = "built-in";

/// Every constraint sort.
pub const SORTS: [&str; 10] = [
    DESCRIPTION,
    TYPE,
    DEFAULT,
    APPLIED,
    DEPRECATED,
    SPECIFIED_BY,
    ONE_OF,
    REPEATABLE,
    LOCATION,
    BUILT_IN,
];

/// The built-in scalars, in the order the specification defines them.
pub const BUILT_IN_SCALARS: [&str; 5] = ["Int", "Float", "String", "Boolean", "ID"];

/// The built-in directives, in the order the specification defines them.
pub const BUILT_IN_DIRECTIVES: [&str; 5] =
    ["skip", "include", "deprecated", "specifiedBy", "oneOf"];

/// The deprecation reason `@deprecated` gives when it states none.
pub const DEFAULT_DEPRECATION_REASON: &str = "No longer supported";

/// The operation types, in the order of the specification.
pub const OPERATION_TYPES: [&str; 3] = ["query", "mutation", "subscription"];

/// The default root type name of each operation type.
pub const DEFAULT_ROOT_NAMES: [(&str, &str); 3] = [
    ("query", "Query"),
    ("mutation", "Mutation"),
    ("subscription", "Subscription"),
];

/// The type system directive locations, in the order of the
/// specification's `TypeSystemDirectiveLocation` production.
pub const TYPE_SYSTEM_LOCATIONS: [&str; 11] = [
    "SCHEMA",
    "SCALAR",
    "OBJECT",
    "FIELD_DEFINITION",
    "ARGUMENT_DEFINITION",
    "INTERFACE",
    "UNION",
    "ENUM",
    "ENUM_VALUE",
    "INPUT_OBJECT",
    "INPUT_FIELD_DEFINITION",
];

/// The executable directive locations, in the order of the
/// specification's `ExecutableDirectiveLocation` production.
pub const EXECUTABLE_LOCATIONS: [&str; 8] = [
    "QUERY",
    "MUTATION",
    "SUBSCRIPTION",
    "FIELD",
    "FRAGMENT_DEFINITION",
    "FRAGMENT_SPREAD",
    "INLINE_FRAGMENT",
    "VARIABLE_DEFINITION",
];

/// Returns the GraphQL protocol definition.
///
/// Schema theory: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
/// Instance theory: `ThWType`.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "graphql".into(),
        schema_theory: "ThGraphQLSchema".into(),
        instance_theory: "ThGraphQLInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: KINDS.iter().map(|k| (*k).to_owned()).collect(),
        constraint_sorts: SORTS.iter().map(|s| (*s).to_owned()).collect(),
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for GraphQL with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThGraphQLSchema", "ThGraphQLInstance");
}

/// The edge rules, one per edge kind: [`EDGES`] with the rows of one kind
/// joined.
fn edge_rules() -> Vec<EdgeRule> {
    let mut rules: Vec<EdgeRule> = Vec::new();
    for (kind, sources, targets) in EDGES {
        let index = if let Some(i) = rules.iter().position(|r| r.edge_kind == *kind) {
            i
        } else {
            rules.push(EdgeRule {
                edge_kind: (*kind).into(),
                src_kinds: Vec::new(),
                tgt_kinds: Vec::new(),
            });
            rules.len() - 1
        };
        let rule = &mut rules[index];
        for source in *sources {
            if !rule.src_kinds.iter().any(|k| k == source) {
                rule.src_kinds.push((*source).into());
            }
        }
        for target in *targets {
            if !rule.tgt_kinds.iter().any(|k| k == target) {
                rule.tgt_kinds.push((*target).into());
            }
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_are_the_type_kinds_with_the_parts_of_a_definition() {
        let p = protocol();
        assert_eq!(p.name, "graphql");
        assert_eq!(p.obj_kinds, KINDS);
        for kind in TYPE_KINDS {
            assert!(KINDS.contains(&kind));
        }
    }

    #[test]
    fn edge_kinds_are_declared_once_each() {
        let p = protocol();
        let kinds: Vec<&str> = p.edge_rules.iter().map(|r| r.edge_kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                ROOT,
                FIELD_EDGE,
                ARGUMENT_EDGE,
                INPUT_FIELD_EDGE,
                VALUE_EDGE,
                TYPE_EDGE,
                IMPLEMENTS,
                MEMBER
            ]
        );
        let type_rule = p
            .edge_rules
            .iter()
            .find(|r| r.edge_kind == TYPE_EDGE)
            .map(|r| r.tgt_kinds.clone())
            .unwrap_or_default();
        assert_eq!(
            type_rule,
            [SCALAR, OBJECT, INTERFACE, UNION, ENUM, INPUT_OBJECT]
        );
    }

    #[test]
    fn constraint_sorts_are_declared_once_each() {
        let mut sorts = SORTS.to_vec();
        sorts.sort_unstable();
        sorts.dedup();
        assert_eq!(sorts.len(), SORTS.len());
        assert_eq!(protocol().constraint_sorts, SORTS);
    }

    #[test]
    fn the_schema_vertex_id_is_no_graphql_name() {
        assert!(
            !SCHEMA_VERTEX
                .chars()
                .all(|c| c == '_' || c.is_ascii_alphanumeric())
        );
    }

    #[test]
    fn register_theories_registers_the_schema_and_instance_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThGraphQLSchema"));
        assert!(registry.contains_key("ThGraphQLInstance"));
    }
}
