//! Every registered protocol declares a syntactic and a semantic shape, and
//! its registered instance theory is the theory of its syntactic shape.
//!
//! The syntactic shape is the shape a protocol's codec parses to; the
//! semantic shape is the shape its instances have once the references its
//! schema declares are resolved. The two differ exactly when references
//! encode the structure: a tree whose identifiers and cross-references
//! describe a graph (the XML annotation formats, JSON:API, AMR), a tree of
//! items that are rows of a keyed table (`DynamoDB`, Cassandra), or a table
//! of segments that nest into a tree (EDI X12, SWIFT MT).
//!
//! The instance theory a protocol registers is `ThWType` for a tree,
//! `ThFunctor` for a table and `ThGraphInstance` for a graph, possibly
//! extended by a colimit (the `ThWType + ThMeta` instances).

#![allow(clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};

use panproto_gat::Theory;
use panproto_protocols::registry::{descriptor, descriptors};
use panproto_protocols::semantics::shapes;
use panproto_protocols::theories::{th_flat, th_functor, th_graph_instance, th_wtype};
use panproto_schema::InstanceShapeKind::{self, Graph, Table, Tree};

/// The declaration each registered protocol carries: its canonical name,
/// syntactic shape and semantic shape.
const DECLARED: &[(&str, InstanceShapeKind, InstanceShapeKind)] = &[
    ("amr", Tree, Graph),
    ("ansible", Tree, Tree),
    ("arrow", Table, Table),
    ("asn1", Tree, Tree),
    ("asyncapi", Tree, Tree),
    ("atproto", Tree, Graph),
    ("avro", Tree, Tree),
    ("bead", Tree, Graph),
    ("bond", Tree, Tree),
    ("brat", Tree, Graph),
    ("bson", Tree, Graph),
    ("cassandra", Tree, Table),
    ("cddl", Tree, Tree),
    ("cloudformation", Tree, Graph),
    ("concrete", Tree, Graph),
    ("conllu", Table, Graph),
    ("csv", Table, Table),
    ("dataframe", Table, Table),
    ("decomp", Tree, Graph),
    ("docx", Tree, Graph),
    ("dynamodb", Tree, Table),
    ("edi-x12", Table, Tree),
    ("elan", Tree, Graph),
    ("fhir", Tree, Graph),
    ("flatbuffers", Tree, Tree),
    ("folia", Tree, Graph),
    ("fovea", Tree, Graph),
    ("geojson", Tree, Tree),
    ("graphql", Tree, Graph),
    ("iso-space", Tree, Graph),
    ("json-schema", Tree, Tree),
    ("jsonapi", Tree, Graph),
    ("k8s-crd", Tree, Tree),
    ("laf-graf", Tree, Graph),
    ("mongodb", Tree, Graph),
    ("msgpack-schema", Tree, Tree),
    ("naf", Tree, Graph),
    ("neo4j", Tree, Graph),
    ("nif", Tree, Graph),
    ("odf", Tree, Graph),
    ("openapi", Tree, Tree),
    ("parquet", Table, Table),
    ("paula", Tree, Graph),
    ("protobuf", Tree, Tree),
    ("raml", Tree, Tree),
    ("redis", Table, Table),
    ("rss-atom", Tree, Tree),
    ("sql", Table, Table),
    ("swift-mt", Table, Tree),
    ("tei", Tree, Graph),
    ("timeml", Tree, Graph),
    ("toml", Tree, Tree),
    ("ucca", Tree, Graph),
    ("uima", Tree, Graph),
    ("vcard-ical", Tree, Tree),
    ("web-annotation", Tree, Graph),
    ("yaml", Tree, Graph),
];

/// Protocols whose shape row is kept in a table file that another family
/// fills: `sql`'s row belongs to the `sql` family file, which is still
/// empty. Each is checked to have no row yet, so the entry is removed
/// when the row lands.
const ROW_NOT_YET_DECLARED: &[&str] = &["sql"];

/// Protocols whose registered instance theory is not yet the theory of
/// their declared syntactic shape, with the shape the registered theory
/// has (`None` for `ThFlat`, which is no instance shape). Each is checked
/// to still disagree in exactly this way, so the entry is removed when
/// the protocol re-registers its instance theory.
const THEORY_NOT_YET_REDECLARED: &[(&str, Option<InstanceShapeKind>)] = &[
    ("asn1", None),
    ("avro", None),
    ("bond", None),
    ("cassandra", Some(Table)),
    ("dynamodb", Some(Table)),
    ("flatbuffers", None),
    ("naf", Some(Table)),
    ("neo4j", Some(Graph)),
    ("protobuf", None),
    ("redis", None),
];

/// The instance shape whose theory `theory` contains, found by sort and
/// operation names: `ThWType`, `ThFunctor` and `ThGraphInstance` each have
/// sorts the others lack (`Arc`, `Row`, `IEdge`), and a colimit extension
/// keeps every sort and operation of its base. `ThFlat` (which has no
/// `Arc`, `Row` or `IEdge`), and any theory containing none or more than
/// one of the three, has no instance shape.
fn instance_shape(theory: &Theory) -> Option<InstanceShapeKind> {
    let names = |t: &Theory| -> BTreeSet<String> {
        t.sorts
            .iter()
            .map(|s| format!("sort {}", s.name))
            .chain(t.ops.iter().map(|o| format!("op {}", o.name)))
            .collect()
    };
    let have = names(theory);
    let contains = |base: &Theory| names(base).is_subset(&have);
    let matching: Vec<InstanceShapeKind> = [
        (Tree, th_wtype()),
        (Table, th_functor()),
        (Graph, th_graph_instance()),
    ]
    .into_iter()
    .filter(|(_, base)| contains(base))
    .map(|(shape, _)| shape)
    .collect();
    match matching.as_slice() {
        [shape] => Some(*shape),
        _ => None,
    }
}

/// The instance theory the protocol named `name` registers.
fn registered_instance_theory(name: &str) -> Theory {
    let d = descriptor(name).expect("a declared protocol is registered");
    let mut registry = HashMap::new();
    (d.register_theories)(&mut registry).expect("the protocol registers its theories");
    let instance = (d.protocol)().instance_theory;
    registry
        .remove(&instance)
        .unwrap_or_else(|| panic!("{name}: instance theory {instance:?} is not registered"))
}

#[test]
fn the_shape_declarations_list_every_registered_protocol_once() {
    let declared: Vec<&str> = DECLARED.iter().map(|(name, _, _)| *name).collect();
    let registered: Vec<&str> = descriptors().iter().map(|d| d.name).collect();
    assert_eq!(declared, registered);
}

#[test]
fn every_registered_protocol_declares_both_shapes() {
    for &(name, syntactic, semantic) in DECLARED {
        let protocol = (descriptor(name).expect("registered").protocol)();
        if ROW_NOT_YET_DECLARED.contains(&name) {
            assert!(
                shapes::declared(name).is_none(),
                "{name} now declares its shapes; remove it from ROW_NOT_YET_DECLARED"
            );
            continue;
        }
        let row = shapes::declared(name).unwrap_or_else(|| panic!("{name} declares no shape row"));
        assert_eq!(
            (row.syntactic, row.semantic),
            (syntactic, semantic),
            "{name}: the shape row"
        );
        assert_eq!(
            (protocol.syntactic_shape, protocol.semantic_shape),
            (syntactic, semantic),
            "{name}: the shapes its protocol carries"
        );
    }
}

#[test]
fn every_shape_row_names_a_declared_protocol() {
    for row in shapes::rows() {
        assert!(
            DECLARED
                .iter()
                .any(|(name, syntactic, semantic)| *name == row.protocol
                    && *syntactic == row.syntactic
                    && *semantic == row.semantic),
            "shape row {row:?} is not a declaration of this test"
        );
    }
}

#[test]
fn every_instance_theory_is_the_theory_of_the_syntactic_shape() {
    let mut wrong = Vec::new();
    for &(name, syntactic, _) in DECLARED {
        let registered = instance_shape(&registered_instance_theory(name));
        match THEORY_NOT_YET_REDECLARED.iter().find(|(n, _)| *n == name) {
            Some(&(_, today)) if registered != today => wrong.push(format!(
                "{name}: the registered instance theory, of shape {registered:?}, is no \
                 longer the listed {today:?}; if it is now the theory of {syntactic:?}, \
                 remove it from THEORY_NOT_YET_REDECLARED"
            )),
            None if registered != Some(syntactic) => wrong.push(format!(
                "{name}: the instance theory, of shape {registered:?}, is not the theory \
                 of its syntactic shape {syntactic:?}"
            )),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_pending_theory_disagrees_with_its_declared_shape() {
    for &(name, today) in THEORY_NOT_YET_REDECLARED {
        let (_, syntactic, _) = DECLARED
            .iter()
            .find(|(n, _, _)| *n == name)
            .expect("a pending protocol is declared");
        assert_ne!(today, Some(*syntactic), "{name} is not pending");
    }
}

#[test]
fn the_instance_shape_of_each_base_theory_is_its_shape() {
    assert_eq!(instance_shape(&th_wtype()), Some(Tree));
    assert_eq!(instance_shape(&th_functor()), Some(Table));
    assert_eq!(instance_shape(&th_graph_instance()), Some(Graph));
    assert_eq!(instance_shape(&th_flat()), None);
}
