//! The identity status, policy and composition rules follow the tables in
//! `panproto_inst::identity`, row for row.
//!
//! Each property draws a row (a way a complement can be bound, or fail to
//! be bound, to a lens) together with the data the complement carries, and
//! checks the outcome against an oracle written directly from the table.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::identity::{
    ComplementShape, Digest, IdentityError, IdentityField, IdentityPolicy, IdentityStatus,
    LegacyRejectReason, LegacyWidth, legacy_schema_fingerprint,
};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{Complement, Node, WInstance};
use panproto_lens::{ComplementCompose, ComplementIdentity, Lens, LensError, get, put_checked};
use panproto_mig::Migration;
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// A lens dropping one of `width` string fields of a `post`.
fn lens(width: usize) -> (Lens, WInstance) {
    let protocol = Protocol::default();
    let mut src = SchemaBuilder::new(&protocol)
        .vertex("post", "object", None)
        .unwrap();
    let mut tgt = SchemaBuilder::new(&protocol)
        .vertex("post", "object", None)
        .unwrap();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::from([(Name::from("post"), Name::from("post"))]);
    let mut nodes = vec![Node::new(0, "post")];
    let mut arcs = Vec::new();
    for i in 0..width {
        let v = format!("post.f{i}");
        let e = prop("post", &v, &format!("f{i}"));
        src = src
            .vertex(&v, "string", None)
            .unwrap()
            .edge("post", &v, "prop", Some(&format!("f{i}")))
            .unwrap();
        if i > 0 {
            tgt = tgt
                .vertex(&v, "string", None)
                .unwrap()
                .edge("post", &v, "prop", Some(&format!("f{i}")))
                .unwrap();
            vertex_map.insert(Name::from(v.as_str()), Name::from(v.as_str()));
            edge_map.insert(e.clone(), e.clone());
        }
        let id = u32::try_from(i).unwrap() + 1;
        nodes.push(
            Node::new(id, v.as_str())
                .with_value(FieldPresence::Present(Value::Str(format!("v{i}")))),
        );
        arcs.push((0, id, e));
    }
    let (src, tgt): (Schema, Schema) = (src.build().unwrap(), tgt.build().unwrap());
    let migration = Migration {
        vertex_map,
        edge_map,
        ..Migration::empty()
    };
    let compiled = panproto_mig::compile(&src, &tgt, &migration).unwrap();
    let instance = WInstance::new(
        nodes.into_iter().map(|n| (n.id, n)).collect(),
        arcs,
        Vec::new(),
        0,
        "post".into(),
    );
    (
        Lens {
            compiled,
            src_schema: src,
            tgt_schema: tgt,
        },
        instance,
    )
}

/// A way a complement can be bound to the lens it is checked against.
#[derive(Clone, Copy, Debug)]
enum Row {
    Verified,
    Mismatch(IdentityField),
    Legacy(LegacyWidth),
    LegacyOfAnotherSchema,
    UnboundEmpty,
    UnboundWithData,
}

fn arb_row() -> impl Strategy<Value = Row> {
    prop_oneof![
        Just(Row::Verified),
        Just(Row::Mismatch(IdentityField::SourceSchema)),
        Just(Row::Mismatch(IdentityField::TargetSchema)),
        Just(Row::Mismatch(IdentityField::Lens)),
        Just(Row::Mismatch(IdentityField::Format)),
        Just(Row::Legacy(LegacyWidth::Bits64)),
        Just(Row::Legacy(LegacyWidth::Bits32)),
        Just(Row::LegacyOfAnotherSchema),
        Just(Row::UnboundEmpty),
        Just(Row::UnboundWithData),
    ]
}

fn arb_policy() -> impl Strategy<Value = IdentityPolicy> {
    prop_oneof![
        Just(IdentityPolicy::Strict),
        Just(IdentityPolicy::AllowLegacy)
    ]
}

/// The complement `get` produced, rebound as `row` says.
fn complement_for(row: Row, lens: &Lens, from_get: &Complement) -> Complement {
    let mut c = from_get.clone();
    match row {
        Row::Verified => {}
        Row::Mismatch(field) => {
            let e = c.identity.as_mut().unwrap();
            match field {
                IdentityField::SourceSchema => e.source_schema = Digest([1; 32]),
                IdentityField::TargetSchema => e.target_schema = Digest([2; 32]),
                IdentityField::Lens => e.lens = Digest([3; 32]),
                IdentityField::Format => {
                    e.complement_format = ComplementShape::Graph.current_format();
                }
            }
        }
        Row::Legacy(width) => {
            c.identity = None;
            c.source_fingerprint = legacy_schema_fingerprint(&lens.src_schema, width);
        }
        Row::LegacyOfAnotherSchema => {
            c.identity = None;
            c.source_fingerprint = 0x5eed;
        }
        Row::UnboundEmpty => c = Complement::empty(),
        Row::UnboundWithData => {
            c.identity = None;
            c.source_fingerprint = 0;
        }
    }
    c
}

/// The table.
fn oracle(row: Row, policy: IdentityPolicy) -> Result<IdentityStatus, IdentityError> {
    let fingerprint_of = |width| {
        let (lens, _) = lens(4);
        legacy_schema_fingerprint(&lens.src_schema, width)
    };
    match (row, policy) {
        (Row::Verified, _) => Ok(IdentityStatus::Verified),
        (Row::Mismatch(field), _) => Err(IdentityError::Mismatch {
            field,
            expected: String::new(),
            found: String::new(),
        }),
        (Row::Legacy(_), IdentityPolicy::Strict) => Err(IdentityError::LegacyComplementRejected {
            reason: LegacyRejectReason::PolicyStrict,
        }),
        (Row::Legacy(width), IdentityPolicy::AllowLegacy) => Ok(IdentityStatus::Legacy {
            fingerprint: fingerprint_of(width),
            width,
        }),
        (Row::LegacyOfAnotherSchema, _) => Err(IdentityError::LegacyComplementRejected {
            reason: LegacyRejectReason::FingerprintMismatch,
        }),
        (Row::UnboundEmpty, _) => Ok(IdentityStatus::Unbound),
        (Row::UnboundWithData, _) => Err(IdentityError::UnboundComplement),
    }
}

/// Equal up to the rendered values of a mismatch, which the oracle does
/// not spell out.
fn same_outcome(
    got: &Result<IdentityStatus, IdentityError>,
    want: &Result<IdentityStatus, IdentityError>,
) -> bool {
    match (got, want) {
        (
            Err(IdentityError::Mismatch { field: a, .. }),
            Err(IdentityError::Mismatch { field: b, .. }),
        ) => a == b,
        _ => got == want,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `verify` (and so `put_checked`) admits exactly the rows the policy
    /// table admits, with the status the status table gives.
    #[test]
    fn verification_follows_the_policy_table(
        row in arb_row(),
        policy in arb_policy(),
    ) {
        let (lens, source) = lens(4);
        let (view, from_get) = get(&lens, &source).unwrap();
        let complement = complement_for(row, &lens, &from_get);

        let got = complement.verify(&lens, policy).map_err(|e| match e {
            LensError::Identity(e) => e,
            other => panic!("not an identity error: {other}"),
        });
        let want = oracle(row, policy);
        prop_assert!(same_outcome(&got, &want), "{row:?} {policy:?}: {got:?} vs {want:?}");

        // And put does what verification decided. The empty unbound row is
        // admitted but carries none of the data a reassembly needs, so it
        // is checked through verify alone.
        if !matches!(row, Row::UnboundEmpty) {
            let put = put_checked(&lens, &view, &complement, policy);
            match (&put, &want) {
                (Ok((restored, status)), Ok(expected)) => {
                    prop_assert_eq!(status, expected);
                    prop_assert!(panproto_lens::instances_equivalent(restored, &source));
                }
                (Err(LensError::Identity(e)), Err(_)) => {
                    prop_assert!(same_outcome(&Err(e.clone()), &want));
                }
                _ => prop_assert!(false, "{row:?} {policy:?}: put gave {put:?}"),
            }
        }
    }
}

/// A way a complement can be bound, for composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Binding {
    EmptyUnbound,
    UnboundWithData,
    Stamped { lens: u8, empty: bool },
    Legacy { fingerprint: u64, empty: bool },
}

fn arb_binding() -> impl Strategy<Value = Binding> {
    prop_oneof![
        Just(Binding::EmptyUnbound),
        Just(Binding::UnboundWithData),
        (0u8..2, any::<bool>()).prop_map(|(lens, empty)| Binding::Stamped { lens, empty }),
        (1u64..3, any::<bool>())
            .prop_map(|(fingerprint, empty)| Binding::Legacy { fingerprint, empty }),
    ]
}

/// A complement bound as `binding` says, carrying data under `key` unless
/// the binding is empty. Keys are distinct per operand, so composition
/// never meets a data conflict and only the identity rules decide it.
fn bound(binding: Binding, key: u32) -> Complement {
    let mut c = Complement::empty();
    let with_data = |c: &mut Complement| {
        c.dropped_nodes.insert(key, Node::new(key, "post.f0"));
    };
    match binding {
        Binding::EmptyUnbound => {}
        Binding::UnboundWithData => with_data(&mut c),
        Binding::Stamped { lens: which, empty } => {
            let (lens, _) = lens(3 + usize::from(which));
            c.stamp_from(&lens);
            if !empty {
                with_data(&mut c);
            }
        }
        Binding::Legacy { fingerprint, empty } => {
            c.source_fingerprint = fingerprint;
            if !empty {
                with_data(&mut c);
            }
        }
    }
    c
}

trait StampFrom {
    fn stamp_from(&mut self, lens: &Lens);
}

impl StampFrom for Complement {
    fn stamp_from(&mut self, lens: &Lens) {
        use panproto_inst::identity::IdentityCarrier;
        self.stamp(lens.identity(ComplementShape::WType), &lens.src_schema);
    }
}

/// The composition table. `Ok(Some(i))` names which operand's identity the
/// composite carries; `Ok(None)` is an unbound composite.
const fn compose_oracle(a: Binding, b: Binding) -> Result<Option<usize>, &'static str> {
    use Binding::{EmptyUnbound, Legacy, Stamped, UnboundWithData};
    match (a, b) {
        (UnboundWithData, _) | (_, UnboundWithData) => Err("unbound"),
        (EmptyUnbound, EmptyUnbound) => Ok(None),
        (EmptyUnbound, _) => Ok(Some(1)),
        (_, EmptyUnbound) => Ok(Some(0)),
        (Stamped { lens: x, .. }, Stamped { lens: y, .. }) => {
            if x == y {
                Ok(Some(0))
            } else {
                Err("mismatch")
            }
        }
        (Stamped { .. }, Legacy { .. }) | (Legacy { .. }, Stamped { .. }) => Err("mixed"),
        (Legacy { fingerprint: f, .. }, Legacy { fingerprint: g, .. }) => {
            if f == g {
                Ok(Some(0))
            } else {
                Err("fingerprint")
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Composition follows the table row for row, in either order.
    #[test]
    fn composition_follows_the_identity_table(a in arb_binding(), b in arb_binding()) {
        let (ca, cb) = (bound(a, 10), bound(b, 20));
        let got = ca.compose(&cb);
        match (compose_oracle(a, b), got) {
            (Ok(which), Ok(composite)) => {
                let (identity, fingerprint) = match which {
                    None => (None, 0),
                    Some(0) => (ca.identity.clone(), ca.source_fingerprint),
                    Some(_) => (cb.identity.clone(), cb.source_fingerprint),
                };
                prop_assert_eq!(composite.identity, identity);
                prop_assert_eq!(composite.source_fingerprint, fingerprint);
            }
            (Err("unbound"), Err(LensError::Identity(IdentityError::UnboundComplement)))
            | (Err("mismatch"), Err(LensError::Identity(IdentityError::Mismatch { .. })))
            | (
                Err("mixed"),
                Err(LensError::Identity(IdentityError::LegacyComplementRejected {
                    reason: LegacyRejectReason::MixedWithVerified,
                })),
            )
            | (
                Err("fingerprint"),
                Err(LensError::Identity(IdentityError::LegacyComplementRejected {
                    reason: LegacyRejectReason::FingerprintMismatch,
                })),
            ) => {}
            (want, got) => prop_assert!(false, "{a:?} . {b:?}: expected {want:?}, got {got:?}"),
        }
    }

    /// The empty unbound complement is a two-sided unit for every
    /// complement composition is defined on.
    #[test]
    fn the_empty_unbound_complement_is_the_unit(a in arb_binding()) {
        prop_assume!(a != Binding::UnboundWithData);
        let c = bound(a, 10);
        for composite in [
            Complement::empty().compose(&c).unwrap(),
            c.compose(&Complement::empty()).unwrap(),
        ] {
            prop_assert_eq!(&composite.identity, &c.identity);
            prop_assert_eq!(composite.source_fingerprint, c.source_fingerprint);
            prop_assert_eq!(composite.dropped_nodes.len(), c.dropped_nodes.len());
        }
    }
}
