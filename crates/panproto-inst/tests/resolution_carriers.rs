//! Contract tests for the resolution complement carriers.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::identity::{AnyComplement, Encoding, decode_complement};
use panproto_inst::{
    AssemblyPlanId, ComplementShape, DefinitionKey, EdgeId, RefOccurrence, ResolutionComplement,
    ResolutionContextKind, ResolutionKind, ResolutionProviderId, ResolutionSpan, ResolutionSpec,
    SynthKey, Value, ViewRef, resolution_identity, resolution_spec_digest,
};
use panproto_schema::{
    Cardinality, Edge, OnUnresolved, PathSyntax, RefSource, Reference, ResolveMode, Schema,
};

fn edge() -> Edge {
    Edge {
        src: "holder".into(),
        tgt: "target".into(),
        kind: "reference".into(),
        name: Some("to".into()),
    }
}

fn reference() -> Reference {
    Reference {
        id: "r".into(),
        holder: "holder".into(),
        source: RefSource::Path {
            field: Vec::new(),
            syntax: PathSyntax::IdRef,
        },
        target_vertex: "target".into(),
        target_key: None,
        edge: Some(edge()),
        cardinality: Cardinality::One,
        nullable: false,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    }
}

fn spec() -> ResolutionSpec {
    ResolutionSpec {
        kind: ResolutionKind::TreeToGraph,
        provider: ResolutionProviderId {
            name: "test".into(),
            version: 1,
        },
        semantic_schema: Schema::default(),
        protocol_references: BTreeMap::new(),
        structural_edges: BTreeSet::new(),
        reference_writers: BTreeMap::new(),
        context_requirements: BTreeSet::new(),
        assembly: None,
    }
}

#[test]
fn resolution_kinds_have_the_fixed_wire_names() {
    assert_eq!(
        serde_json::to_string(&[
            ResolutionKind::TreeToGraph,
            ResolutionKind::TableToGraph,
            ResolutionKind::TreeToTable,
            ResolutionKind::TableToTree,
        ])
        .unwrap(),
        r#"["tree_to_graph","table_to_graph","tree_to_table","table_to_tree"]"#
    );
}

#[test]
fn resolution_complement_has_the_exact_named_field_order() {
    assert_eq!(
        serde_json::to_string(&ResolutionComplement::default()).unwrap(),
        r#"{"occurrences":[],"definitions":[],"redirected":{},"order":{},"spanning":{},"span_nodes":{},"unresolved":[],"assembly":null,"identity":null}"#
    );
}

#[test]
fn resolution_complement_round_trips_in_all_four_encodings() {
    let mut complement = ResolutionComplement::default();
    complement.occurrences.insert(
        ViewRef::Edge(EdgeId(9)),
        vec![RefOccurrence {
            holder: ViewRef::Node(2),
            reference: "r".into(),
            index: 1,
            literal: Value::Str("x".to_owned()),
            form: Some("X".to_owned()),
            context: None,
        }],
    );
    complement.definitions.insert(
        DefinitionKey {
            reference: "r".into(),
            target: 7,
        },
        ViewRef::Node(2),
    );
    complement.span_nodes.insert(
        7,
        ResolutionSpan {
            text: ViewRef::Node(3),
            start: 1,
            end: 4,
        },
    );
    let expected = serde_json::to_value(&complement).unwrap();

    let json: ResolutionComplement =
        serde_json::from_slice(&serde_json::to_vec(&complement).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(json).unwrap(), expected);
    for bytes in [
        rmp_serde::to_vec(&complement).unwrap(),
        rmp_serde::to_vec_named(&complement).unwrap(),
    ] {
        let decoded: ResolutionComplement = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
    }
    let mut cbor = Vec::new();
    ciborium::into_writer(&complement, &mut cbor).unwrap();
    let decoded: ResolutionComplement = ciborium::from_reader(cbor.as_slice()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
}

#[test]
fn resolution_synth_index_names_occurrence_edges_and_span_nodes() {
    let mut complement = ResolutionComplement::default();
    complement.occurrences.insert(
        ViewRef::Edge(EdgeId(5)),
        vec![RefOccurrence {
            holder: ViewRef::Node(1),
            reference: "r".into(),
            index: 3,
            literal: Value::Int(4),
            form: None,
            context: None,
        }],
    );
    complement.span_nodes.insert(
        6,
        ResolutionSpan {
            text: ViewRef::Node(2),
            start: 7,
            end: 9,
        },
    );
    let index = complement.synth_index();
    assert_eq!(
        index.edges[&EdgeId(5)],
        SynthKey::ResolutionReference {
            holder: ViewRef::Node(1),
            reference: "r".into(),
            index: 3,
        }
    );
    assert_eq!(
        index.nodes[&6],
        SynthKey::ResolutionSpan {
            text: ViewRef::Node(2),
            start: 7,
            end: 9,
        }
    );
}

#[test]
fn resolution_spec_digest_reads_every_field() {
    let base = spec();
    let base_digest = resolution_spec_digest(&base);
    let mut variants = Vec::new();

    let mut changed = base.clone();
    changed.kind = ResolutionKind::TableToGraph;
    variants.push(changed);
    let mut changed = base.clone();
    changed.provider.version = 2;
    variants.push(changed);
    let mut changed = base.clone();
    changed.semantic_schema.protocol = "other".to_owned();
    variants.push(changed);
    let mut changed = base.clone();
    changed.protocol_references.insert("r".into(), reference());
    variants.push(changed);
    let mut changed = base.clone();
    changed.structural_edges.insert(edge());
    variants.push(changed);
    let mut changed = base.clone();
    changed.reference_writers.insert(edge(), "r".into());
    variants.push(changed);
    let mut changed = base.clone();
    changed
        .context_requirements
        .insert(ResolutionContextKind::ExternalText);
    variants.push(changed);
    let mut changed = base.clone();
    changed.assembly = Some(AssemblyPlanId {
        grammar: "g".into(),
        version: 1,
    });
    variants.push(changed);

    for changed in variants {
        assert_ne!(resolution_spec_digest(&changed), base_digest);
    }
}

#[test]
fn resolution_complement_decoder_checks_its_shape_and_identity() {
    let syntactic = Schema::default();
    let plan = spec();
    let mut complement = ResolutionComplement {
        identity: Some(resolution_identity("test", &syntactic, &plan)),
        ..ResolutionComplement::default()
    };
    let bytes = serde_json::to_vec(&complement).unwrap();
    assert!(matches!(
        decode_complement(&bytes, Encoding::Json, ComplementShape::Resolution).unwrap(),
        AnyComplement::Resolution(_)
    ));

    complement.identity.as_mut().unwrap().complement_format =
        ComplementShape::WType.current_format();
    let bytes = serde_json::to_vec(&complement).unwrap();
    assert!(decode_complement(&bytes, Encoding::Json, ComplementShape::Resolution).is_err());
}
