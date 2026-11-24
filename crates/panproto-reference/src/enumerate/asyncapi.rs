//! Complete finite scopes for the `AsyncAPI` page.
//!
//! The document scope is the cartesian product of five choices made on one
//! base document: its `asyncapi` version ([`versions`]), a second channel
//! `d` ([`channels`]), the operation `o` on the channel `c`
//! ([`operations`]), the component message `M` that `c` holds
//! ([`messages`]), and one member added to the document ([`additions`]).
//! The first choices of each list are read; the rest each break one rule.

use crate::pages::json_schema::{Doc, m, s};

fn r(target: &str) -> Doc {
    m(&[("$ref", s(target))])
}

const fn seq(items: Vec<Doc>) -> Doc {
    Doc::Seq(items)
}

/// The versions the scope writes in `asyncapi`; the first two are read.
#[must_use]
pub fn versions() -> Vec<Doc> {
    vec![s("3.1.0"), s("3.0.0"), s("2.6.0"), s("3.2.0")]
}

/// The number of read versions.
pub const READ_VERSIONS: usize = 2;

/// The forms of the channel `d`; the first three are read.
#[must_use]
pub fn channels() -> Vec<Doc> {
    vec![
        m(&[
            ("address", s("d")),
            ("servers", seq(vec![r("#/servers/s")])),
            ("bindings", m(&[("mqtt", m(&[("qos", Doc::Int(1))]))])),
            ("x-d", seq(vec![Doc::Int(1), Doc::Null])),
        ]),
        m(&[
            ("$ref", s("#/components/channels/C")),
            ("description", s("ignored")),
        ]),
        m(&[
            ("address", Doc::Null),
            (
                "messages",
                m(&[(
                    "e",
                    m(&[(
                        "payload",
                        m(&[("type", s("array")), ("items", r("#/components/schemas/S"))]),
                    )]),
                )]),
            ),
            ("tags", seq(vec![])),
        ]),
        m(&[("address", s("d?x=1"))]),
        m(&[("address", s("d/{x}")), ("parameters", m(&[("y", m(&[]))]))]),
        m(&[("servers", seq(vec![r("#/components/servers/z")]))]),
    ]
}

/// The number of read channel forms.
pub const READ_CHANNELS: usize = 3;

/// The forms of the operation `o`; the first three are read.
#[must_use]
pub fn operations() -> Vec<Doc> {
    vec![
        m(&[
            ("action", s("receive")),
            ("channel", r("#/channels/c")),
            ("messages", seq(vec![r("#/channels/c/messages/m")])),
            (
                "reply",
                m(&[
                    ("address", m(&[("location", s("$message.header#/replyTo"))])),
                    ("channel", r("#/channels/r")),
                ]),
            ),
        ]),
        m(&[
            ("action", s("send")),
            ("channel", r("#/channels/c")),
            ("traits", seq(vec![r("#/components/operationTraits/t")])),
            (
                "bindings",
                m(&[("kafka", m(&[("groupId", m(&[("type", s("string"))]))]))]),
            ),
            ("messages", seq(vec![])),
        ]),
        r("#/components/operations/P"),
        m(&[("action", s("publish")), ("channel", r("#/channels/c"))]),
        m(&[
            ("action", s("send")),
            ("channel", r("#/components/channels/C")),
        ]),
        m(&[
            ("action", s("send")),
            ("channel", r("#/channels/c")),
            ("messages", seq(vec![r("#/components/messages/M")])),
        ]),
    ]
}

/// The number of read operation forms.
pub const READ_OPERATIONS: usize = 3;

/// The forms of the component message `M`; the first four are read.
#[must_use]
pub fn messages() -> Vec<Doc> {
    vec![
        m(&[
            ("payload", r("#/components/schemas/S")),
            ("contentType", s("application/json")),
            ("correlationId", m(&[("location", s("$message.header#/c"))])),
        ]),
        m(&[
            (
                "headers",
                m(&[
                    ("type", s("object")),
                    (
                        "properties",
                        m(&[("h", m(&[("type", s("integer")), ("minimum", Doc::Int(1))]))]),
                    ),
                ]),
            ),
            (
                "payload",
                m(&[
                    ("type", s("object")),
                    ("definitions", m(&[("D", m(&[("type", s("string"))]))])),
                    (
                        "properties",
                        m(&[("a", r("#/components/messages/M/payload/definitions/D"))]),
                    ),
                ]),
            ),
            (
                "examples",
                seq(vec![m(&[("payload", m(&[("a", s("x"))]))])]),
            ),
        ]),
        m(&[
            (
                "payload",
                m(&[
                    (
                        "schemaFormat",
                        s("application/vnd.aai.asyncapi+json;version=3.0.0"),
                    ),
                    (
                        "schema",
                        m(&[("type", s("string")), ("deprecated", Doc::Bool(true))]),
                    ),
                ]),
            ),
            ("traits", seq(vec![r("#/components/messageTraits/MT")])),
        ]),
        m(&[
            ("payload", Doc::Bool(true)),
            ("bindings", r("#/components/messageBindings/B")),
        ]),
        m(&[(
            "payload",
            m(&[
                (
                    "schemaFormat",
                    s("application/vnd.apache.avro;version=1.9.0"),
                ),
                ("schema", m(&[("type", s("record"))])),
            ]),
        )]),
        m(&[("payload", r("#/components/schemas/F"))]),
        m(&[("payload", m(&[("$id", s("https://e.test/p"))]))]),
    ]
}

/// The number of read message forms.
pub const READ_MESSAGES: usize = 4;

/// The members the document may gain, each a name and a value; the first
/// three are read, and the first is no member at all.
#[must_use]
pub fn additions() -> Vec<Option<(String, Doc)>> {
    let member = |name: &str, value: Doc| Some((name.to_owned(), value));
    vec![
        None,
        member("x-a", m(&[("b", seq(vec![Doc::Int(1), Doc::Null]))])),
        member("defaultContentType", s("application/json")),
        member("paths", m(&[])),
        member("id", s("app")),
    ]
}

/// The number of read additions.
pub const READ_ADDITIONS: usize = 3;

/// The base document's components.
fn components(message: &Doc) -> Doc {
    m(&[
        (
            "schemas",
            m(&[
                (
                    "S",
                    m(&[
                        ("type", s("object")),
                        ("required", seq(vec![s("n/m")])),
                        ("discriminator", s("n/m")),
                        ("properties", m(&[("n/m", m(&[("type", s("string"))]))])),
                    ]),
                ),
                (
                    "F",
                    m(&[
                        (
                            "schemaFormat",
                            s("application/schema+json;version=draft-07"),
                        ),
                        ("schema", m(&[("type", s("string"))])),
                    ]),
                ),
            ]),
        ),
        (
            "servers",
            m(&[("z", m(&[("host", s("z")), ("protocol", s("ws"))]))]),
        ),
        (
            "channels",
            m(&[(
                "C",
                m(&[
                    ("address", Doc::Null),
                    ("messages", m(&[("k", m(&[("payload", Doc::Bool(true))]))])),
                ]),
            )]),
        ),
        (
            "operations",
            m(&[(
                "P",
                m(&[
                    ("action", s("send")),
                    ("channel", r("#/components/channels/C")),
                    (
                        "messages",
                        seq(vec![r("#/components/channels/C/messages/k")]),
                    ),
                ]),
            )]),
        ),
        ("messages", m(&[("M", message.clone())])),
        (
            "securitySchemes",
            m(&[(
                "key",
                m(&[
                    ("type", s("httpApiKey")),
                    ("name", s("k")),
                    ("in", s("header")),
                ]),
            )]),
        ),
        (
            "operationTraits",
            m(&[(
                "t",
                m(&[
                    ("summary", s("s")),
                    ("security", seq(vec![r("#/components/securitySchemes/key")])),
                ]),
            )]),
        ),
        ("messageTraits", m(&[("MT", m(&[("name", s("mt"))]))])),
        (
            "messageBindings",
            m(&[("B", m(&[("amqp", m(&[("bindingVersion", s("0.3.0"))]))]))]),
        ),
    ])
}

/// The base document with the given version, channel `d`, operation `o`,
/// message `M` and added member.
#[must_use]
pub fn document(
    version: &Doc,
    channel: &Doc,
    operation: &Doc,
    message: &Doc,
    addition: Option<&(String, Doc)>,
) -> Doc {
    let c = m(&[
        ("address", s("c/{id}")),
        ("messages", m(&[("m", r("#/components/messages/M"))])),
        (
            "parameters",
            m(&[("id", m(&[("location", s("$message.payload#/id"))]))]),
        ),
    ]);
    let mut members = vec![
        ("asyncapi".to_owned(), version.clone()),
        (
            "info".to_owned(),
            m(&[("title", s("T")), ("version", s("1"))]),
        ),
        (
            "servers".to_owned(),
            m(&[("s", m(&[("host", s("h")), ("protocol", s("mqtt"))]))]),
        ),
        (
            "channels".to_owned(),
            m(&[
                ("c", c),
                ("r", m(&[("address", Doc::Null)])),
                ("d", channel.clone()),
            ]),
        ),
        ("operations".to_owned(), m(&[("o", operation.clone())])),
        ("components".to_owned(), components(message)),
    ];
    if let Some((name, value)) = addition {
        members.push((name.clone(), value.clone()));
    }
    Doc::Map(members)
}

/// Every document of the scope, in the order version, channel, operation,
/// message, addition.
#[must_use]
pub fn documents() -> Vec<Doc> {
    let mut out = Vec::new();
    for version in &versions() {
        for channel in &channels() {
            for operation in &operations() {
                for message in &messages() {
                    for addition in &additions() {
                        out.push(document(
                            version,
                            channel,
                            operation,
                            message,
                            addition.as_ref(),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// The number of documents of the scope, the product of the five choices'
/// sizes: $4 \cdot 6 \cdot 6 \cdot 7 \cdot 5 = 5040$.
#[must_use]
pub const fn document_count() -> usize {
    4 * 6 * 6 * 7 * 5
}

/// The number of documents the page reads: those whose five choices are
/// each read, $2 \cdot 3 \cdot 3 \cdot 4 \cdot 3 = 216$.
#[must_use]
pub const fn accepted_count() -> usize {
    READ_VERSIONS * READ_CHANNELS * READ_OPERATIONS * READ_MESSAGES * READ_ADDITIONS
}
