//! A [`CanonicalSink`] writes the bytes [`CanonicalWriter`] writes, whether
//! it collects them or hashes them as they arrive.
//!
//! The trait's encoders are default methods over one `write`, and
//! `CanonicalWriter` keeps its own inherent encoders for `panproto-schema`.
//! Both must agree byte for byte, or a record written through the trait
//! would digest differently from the same record written by the schema
//! crate. And a streaming [`DigestHasher`] must give exactly the digest
//! [`Digest::derive`] gives over the collected bytes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};
use panproto_gat::Name;
use panproto_inst::Value;
use panproto_inst::identity::{
    CanonicalSink, Digest, DigestHasher, GRAPH_INSTANCE_CONTEXT, LENS_CONTEXT,
    TREE_INSTANCE_CONTEXT, write_value, write_value_map,
};
use panproto_schema::{CanonicalWriter, Edge};

/// A sink that uses every default encoder of the trait, since it implements
/// only `write`.
#[derive(Default)]
struct Collect(Vec<u8>);

impl CanonicalSink for Collect {
    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

fn edge() -> Edge {
    Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("b".into()),
    }
}

fn expr() -> Expr {
    Expr::Let {
        name: "x".into(),
        value: Box::new(Expr::Lit(Literal::Float(-0.0))),
        body: Box::new(Expr::Match {
            scrutinee: Box::new(Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var("x".into()), Expr::Lit(Literal::Int(1))],
            )),
            arms: vec![
                (Pattern::Lit(Literal::Int(1)), Expr::Lit(Literal::Null)),
                (Pattern::Wildcard, Expr::Var("x".into())),
            ],
        }),
    }
}

fn values() -> Vec<Value> {
    let fields: HashMap<String, Value> =
        (0..20).map(|i| (format!("f{i}"), Value::Int(i))).collect();
    vec![
        Value::Bool(false),
        Value::Int(i64::MIN),
        Value::Float(f64::NAN),
        Value::Str(String::new()),
        Value::Bytes(vec![0; 3]),
        Value::CidLink("c".into()),
        Value::Blob {
            ref_: "r".into(),
            mime: "m".into(),
            size: u64::MAX,
        },
        Value::Token("t".into()),
        Value::Null,
        Value::Opaque {
            type_: "o".into(),
            fields: fields.clone(),
        },
        Value::Unknown(fields),
        Value::List(vec![Value::List(vec![Value::Null]), Value::Int(3)]),
        Value::LabeledNull(0),
    ]
}

/// Each encoder called through the trait's default method writes the bytes
/// the inherent method of the same name writes on a `CanonicalWriter`.
#[test]
fn sink_defaults_write_the_bytes_of_the_inherent_encoders() {
    type Case = (&'static str, fn(&mut CanonicalWriter), fn(&mut Collect));
    let name: Name = "n".into();
    let cases: [Case; 15] = [
        ("len", |w| w.len(300), |s| s.len(300)),
        ("tag", |w| w.tag(9), |s| s.tag(9)),
        ("bool", |w| w.bool(true), |s| s.bool(true)),
        ("u32", |w| w.u32(0xdead_beef), |s| s.u32(0xdead_beef)),
        ("u64", |w| w.u64(u64::MAX - 1), |s| s.u64(u64::MAX - 1)),
        ("i64", |w| w.i64(-5), |s| s.i64(-5)),
        ("f64", |w| w.f64(-0.0), |s| s.f64(-0.0)),
        ("bytes", |w| w.bytes(&[1, 2, 3]), |s| s.bytes(&[1, 2, 3])),
        ("str", |w| w.str("héllo"), |s| s.str("héllo")),
        ("name", |w| w.name(&"v".into()), |s| s.name(&"v".into())),
        ("opt_name", |w| w.opt_name(None), |s| s.opt_name(None)),
        ("edge", |w| w.edge(&edge()), |s| s.edge(&edge())),
        ("expr", |w| w.expr(&expr()), |s| s.expr(&expr())),
        (
            "literal",
            |w| w.literal(&Literal::List(vec![Literal::Str("s".into())])),
            |s| s.literal(&Literal::List(vec![Literal::Str("s".into())])),
        ),
        (
            "pattern",
            |w| {
                w.pattern(&Pattern::Constructor(
                    "C".into(),
                    vec![Pattern::Var("v".into())],
                ));
            },
            |s| {
                s.pattern(&Pattern::Constructor(
                    "C".into(),
                    vec![Pattern::Var("v".into())],
                ));
            },
        ),
    ];
    for (what, inherent, default) in cases {
        let mut w = CanonicalWriter::new();
        inherent(&mut w);
        let mut s = Collect::default();
        default(&mut s);
        assert_eq!(s.0, w.into_bytes(), "{what}");
    }

    // A present optional name, through both paths.
    let mut w = CanonicalWriter::new();
    w.opt_name(Some(&name));
    let mut s = Collect::default();
    s.opt_name(Some(&name));
    assert_eq!(s.0, w.into_bytes(), "opt_name(Some)");

    // `CanonicalWriter`'s own trait impl agrees with its inherent methods
    // when a default method is called on it explicitly through the trait.
    let mut inherent = CanonicalWriter::new();
    inherent.str("x");
    inherent.edge(&edge());
    let mut through_trait = CanonicalWriter::new();
    CanonicalSink::str(&mut through_trait, "x");
    CanonicalSink::edge(&mut through_trait, &edge());
    assert_eq!(through_trait, inherent);
}

/// `CanonicalSink::value` is the encoder `write_value` wrote before it became
/// a wrapper: every value writes the same bytes through a plain sink, through
/// `CanonicalWriter` and through `write_value`.
#[test]
fn sink_value_writes_the_bytes_write_value_wrote() {
    for value in values() {
        let mut w = CanonicalWriter::new();
        write_value(&mut w, &value);
        let expected = w.into_bytes();

        let mut s = Collect::default();
        s.value(&value);
        assert_eq!(s.0, expected, "{value:?} through the default method");

        let mut direct = CanonicalWriter::new();
        CanonicalSink::value(&mut direct, &value);
        assert_eq!(direct.into_bytes(), expected, "{value:?} on the writer");

        let mut h = Digest::hasher(TREE_INSTANCE_CONTEXT);
        h.value(&value);
        assert_eq!(
            h.finalize(),
            Digest::derive(TREE_INSTANCE_CONTEXT, &expected),
            "{value:?} hashed as it is written"
        );
    }

    let map: HashMap<String, Value> = (0..10).map(|i| (format!("k{i}"), Value::Int(i))).collect();
    let mut w = CanonicalWriter::new();
    write_value_map(&mut w, &map);
    let mut s = Collect::default();
    write_value_map(&mut s, &map);
    assert_eq!(s.0, w.into_bytes(), "write_value_map through any sink");
}

/// `Digest::derive(c, b)` equals `Digest::hasher(c)` fed `b`, whole or in
/// any split.
#[test]
fn derive_equals_a_hasher_fed_the_same_bytes() {
    let bytes: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    let whole = Digest::derive(GRAPH_INSTANCE_CONTEXT, &bytes);
    for split in [0, 1, 63, 64, 65, 1024, 1025, 4999, 5000] {
        let mut h = Digest::hasher(GRAPH_INSTANCE_CONTEXT);
        h.update(&bytes[..split]);
        h.update(&bytes[split..]);
        assert_eq!(h.finalize(), whole, "split at {split}");
    }
    let empty: DigestHasher = Digest::hasher(LENS_CONTEXT);
    assert_eq!(empty.finalize(), Digest::derive(LENS_CONTEXT, &[]));
    // Different contexts over the same bytes give different digests.
    assert_ne!(Digest::derive(LENS_CONTEXT, &bytes), whole);
}

/// A record stream written into a `DigestHasher` digests to what the same
/// stream collected in a `CanonicalWriter` derives to, and finalizing does
/// not end the stream.
#[test]
fn derive_equals_streaming_hasher() {
    let mut w = CanonicalWriter::new();
    let mut h = Digest::hasher(GRAPH_INSTANCE_CONTEXT);
    for (i, value) in values().into_iter().enumerate() {
        let id = u32::try_from(i).unwrap();
        CanonicalSink::u32(&mut w, id);
        CanonicalSink::value(&mut w, &value);
        h.u32(id);
        h.value(&value);
        assert_eq!(
            h.finalize(),
            Digest::derive(GRAPH_INSTANCE_CONTEXT, w.as_bytes()),
            "after record {i}"
        );
    }
}
