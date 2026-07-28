#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Reconstruction from a view alone, and the condition under which it exists.
//!
//! A lens with complement decomposes its source: `S ≅ V × C`, with `get`
//! the isomorphism and `put` its inverse. Reconstructing a source from a
//! view alone asks for `S ≅ V`, which holds exactly when `C ≅ 1`.
//!
//! So there is no design latitude here. When the complement is terminal,
//! `get` is injective and its section exists; when it is not, distinct
//! sources share a view, the fibre over that view has more than one point,
//! and no section exists to be written. The tests below pin both halves:
//! that reconstruction succeeds and round-trips when the complement is
//! trivial, and that it is refused, with the obstruction named, when it is
//! not.
//!
//! The `residue_is_trivial` / `is_empty` distinction matters throughout. A
//! complement's fields split into the residue (information in `S` and not
//! in `V`, which *is* `C`) and the reassembly bookkeeping (`original_parent`,
//! `arc_order`, `arc_edges`, a function of the view's shape). Only the
//! residue bears on whether `C ≅ 1`.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::{
    CompiledMigration, Complement, FieldTransform, WInstance, parse_json, to_json,
};
use panproto_lens::asymmetric::{get, put, put_without_complement};
use panproto_lens::{Lens, LensError};
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
    for (id, kind) in verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in edge_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }
    Schema {
        protocol: "test".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: Vec::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

/// `root { v: integer, w: integer }`.
fn pair_schema() -> Schema {
    let ev = Edge {
        src: "root".into(),
        tgt: "root.v".into(),
        kind: "prop".into(),
        name: Some("v".into()),
    };
    let ew = Edge {
        src: "root".into(),
        tgt: "root.w".into(),
        kind: "prop".into(),
        name: Some("w".into()),
    };
    make_schema(
        &[
            ("root", "object"),
            ("root.v", "integer"),
            ("root.w", "integer"),
        ],
        &[ev, ew],
    )
}

fn lens_with(schema: &Schema, transforms: Vec<FieldTransform>) -> Lens {
    let mut ft = HashMap::new();
    if !transforms.is_empty() {
        ft.insert(Name::from("root"), transforms);
    }
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: ft,
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

/// `v -> v + 1`, inverse `v - 1`: a bijection on the integers, so the
/// complement it produces is terminal.
fn shift_lens(schema: &Schema) -> Lens {
    lens_with(
        schema,
        vec![FieldTransform::ApplyExpr {
            key: "v".into(),
            expr: Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var(Arc::from("v")), Expr::Lit(Literal::Int(1))],
            ),
            inverse: Some(Expr::Builtin(
                BuiltinOp::Sub,
                vec![Expr::Var(Arc::from("v")), Expr::Lit(Literal::Int(1))],
            )),
            coercion_class: CoercionClass::Iso,
        }],
    )
}

fn instance(schema: &Schema, v: i64, w: i64) -> WInstance {
    parse_json(schema, "root", &serde_json::json!({"v": v, "w": w})).expect("parse")
}

// ---------------------------------------------------------------------------
// C ≅ 1: the isomorphism case
// ---------------------------------------------------------------------------

/// The identity lens is an isomorphism and its complement is pure
/// bookkeeping: `residue_is_trivial` holds while `is_empty` does not.
#[test]
fn an_identity_lens_has_a_terminal_complement() {
    let schema = pair_schema();
    let lens = lens_with(&schema, vec![]);
    let source = instance(&schema, 5, 7);

    assert!(lens.is_isomorphism(), "the identity lens is invertible");

    let (_view, complement) = get(&lens, &source).expect("get");
    assert!(
        complement.residue_is_trivial(),
        "an identity projection discards nothing, so C is terminal"
    );
    assert!(
        !complement.is_empty(),
        "but it still records the reassembly bookkeeping, which is why \
         `is_empty` is the wrong question to ask about information loss"
    );
}

/// `put_without_complement ∘ get = id` on the source: the `S ≅ V` half.
#[test]
fn reconstruction_recovers_the_source() {
    let schema = pair_schema();
    let lens = shift_lens(&schema);
    let source = instance(&schema, 5, 7);

    let (view, _) = get(&lens, &source).expect("get");
    assert_eq!(
        to_json(&schema, &view),
        serde_json::json!({"v": 6, "w": 7}),
        "the forward direction shifts v"
    );

    let restored = put_without_complement(&lens, &view).expect("the lens is an isomorphism");
    assert_eq!(
        to_json(&schema, &restored),
        to_json(&schema, &source),
        "and the view alone determines the source it came from"
    );
}

/// `get ∘ put_without_complement = id` on the view: the other half of the
/// isomorphism.
#[test]
fn reconstruction_is_a_two_sided_inverse() {
    let schema = pair_schema();
    let lens = shift_lens(&schema);

    for (v, w) in [(0, 0), (5, 7), (-3, 12), (i64::MAX - 1, 0)] {
        let source = instance(&schema, v, w);
        let (view, _) = get(&lens, &source).expect("get");

        let restored = put_without_complement(&lens, &view).expect("isomorphism");
        let (re_viewed, _) = get(&lens, &restored).expect("re-get");

        assert!(
            panproto_lens::instances_equivalent(&source, &restored),
            "put ∘ get = id at ({v}, {w})"
        );
        assert!(
            panproto_lens::instances_equivalent(&view, &re_viewed),
            "get ∘ put = id at ({v}, {w})"
        );
    }
}

/// Reconstruction agrees with the complement-carrying `put`. The derived
/// bookkeeping has to be the bookkeeping `get` would have recorded, not
/// merely something that happens to work.
#[test]
fn reconstruction_agrees_with_the_complement_carrying_put() {
    let schema = pair_schema();
    let lens = shift_lens(&schema);
    let source = instance(&schema, 5, 7);

    let (view, complement) = get(&lens, &source).expect("get");
    let with = put(&lens, &view, &complement).expect("put");
    let without = put_without_complement(&lens, &view).expect("isomorphism");

    assert!(
        panproto_lens::instances_equivalent(&with, &without),
        "the two agree, so deriving C ≅ 1 is not a different operation"
    );
}

// ---------------------------------------------------------------------------
// C ≇ 1: the cases where no section exists
// ---------------------------------------------------------------------------

/// A dropped vertex puts information in the complement, so `get` is not
/// injective and no section of it exists.
#[test]
fn a_dropped_vertex_defeats_reconstruction() {
    let schema = pair_schema();
    let mut lens = lens_with(&schema, vec![]);
    lens.compiled.surviving_verts.remove(&Name::from("root.w"));

    assert!(!lens.is_isomorphism());
    let source = instance(&schema, 5, 7);
    let (view, _) = get(&lens, &source).expect("get");

    match put_without_complement(&lens, &view) {
        Err(LensError::NotAnIsomorphism { detail }) => assert!(
            detail.contains("root.w"),
            "the refusal names the obstruction: {detail}"
        ),
        other => panic!("expected a refusal naming the dropped vertex, got {other:?}"),
    }
}

/// A non-injective value transform likewise. Integer division collapses
/// two sources to one view value, so the fibre over it is not a singleton.
/// This is the shape of the `confidence -> floor(confidence * 1000 + 0.5)`
/// step from the field report: a quantization, which round-trips only for
/// values already on the grid it rounds to, and therefore not a bijection
/// however it is classified.
#[test]
fn a_non_injective_transform_defeats_reconstruction() {
    let schema = pair_schema();
    let lens = lens_with(
        &schema,
        vec![FieldTransform::ApplyExpr {
            key: "v".into(),
            // Integer division by two: 4 and 5 share an image, so the
            // fibre over a view value is not a point.
            expr: Expr::Builtin(
                BuiltinOp::Div,
                vec![Expr::Var(Arc::from("v")), Expr::Lit(Literal::Int(2))],
            ),
            inverse: None,
            coercion_class: CoercionClass::Retraction,
        }],
    );

    assert!(!lens.is_isomorphism());
    let source = instance(&schema, 5, 7);
    let (view, _) = get(&lens, &source).expect("get");

    match put_without_complement(&lens, &view) {
        Err(LensError::NotAnIsomorphism { detail }) => assert!(
            detail.contains("Retraction"),
            "the refusal names the coercion class that is not injective: {detail}"
        ),
        other => panic!("expected a refusal naming the transform, got {other:?}"),
    }
}

/// The refusal is a static property of the lens, so it does not depend on
/// which source happened to be projected. A lossy lens is refused even for
/// a record on which the round trip would coincidentally succeed.
#[test]
fn refusal_does_not_depend_on_the_particular_record() {
    let schema = pair_schema();
    let lens = lens_with(
        &schema,
        vec![FieldTransform::ApplyExpr {
            key: "v".into(),
            // Integer division by two: 4 and 5 share an image, so the
            // fibre over a view value is not a point.
            expr: Expr::Builtin(
                BuiltinOp::Div,
                vec![Expr::Var(Arc::from("v")), Expr::Lit(Literal::Int(2))],
            ),
            inverse: None,
            coercion_class: CoercionClass::Retraction,
        }],
    );

    // `v` is already an integer, so `floor` is the identity on it and the
    // value would survive a round trip. The lens is still not injective.
    for v in [0, 5, -3] {
        let source = instance(&schema, v, 7);
        let (view, _) = get(&lens, &source).expect("get");
        assert!(
            matches!(
                put_without_complement(&lens, &view),
                Err(LensError::NotAnIsomorphism { .. })
            ),
            "a lens is invertible or not; it is not invertible at some records"
        );
    }
}

// ---------------------------------------------------------------------------
// `put` on inputs outside the image of `get`
// ---------------------------------------------------------------------------

/// `put` inverts `get` on the image of `get`. An empty complement is not in
/// that image for a view with structure, and the reassembly would return an
/// empty record. Refusing beats returning it: the caller cannot otherwise
/// tell a reconstructed empty record from the loss of a populated one.
#[test]
fn put_refuses_a_complement_outside_the_image_of_get() {
    let schema = pair_schema();
    let lens = shift_lens(&schema);
    let source = instance(&schema, 5, 7);
    let (view, _) = get(&lens, &source).expect("get");

    match put(&lens, &view, &Complement::empty()) {
        Err(LensError::ComplementMismatch { detail }) => assert!(
            detail.contains("no parent"),
            "the refusal says why the complement does not fit: {detail}"
        ),
        Ok(restored) => panic!(
            "expected a refusal, got a silent reconstruction: {}",
            to_json(&schema, &restored)
        ),
        other => panic!("expected ComplementMismatch, got {other:?}"),
    }
}

/// A view with no arcs has nothing to reassemble, so an empty complement is
/// in the image of `get` for it and `put` must still accept it.
#[test]
fn put_accepts_an_empty_complement_for_a_structureless_view() {
    let schema = make_schema(&[("root", "object")], &[]);
    let lens = lens_with(&schema, vec![]);
    let source = parse_json(&schema, "root", &serde_json::json!({})).expect("parse");
    let (view, _) = get(&lens, &source).expect("get");

    assert!(
        put(&lens, &view, &Complement::empty()).is_ok(),
        "an empty complement is the right complement for a view with no arcs"
    );
}
