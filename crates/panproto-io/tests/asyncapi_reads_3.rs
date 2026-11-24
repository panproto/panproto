//! The `AsyncAPI` 3 reader keeps what a description says and refuses what
//! it does not read, by name and place.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_io::api::asyncapi::{
    AsyncApiDocument, AsyncApiErrorKind, DocumentKind, read_asyncapi, read_asyncapi_bundle,
    write_asyncapi,
};
use panproto_io::data_schema::json_schema::JsonSchemaErrorKind;
use panproto_schema::Schema;

fn kept<'s>(schema: &'s Schema, vertex: &str, sort: &str) -> Vec<&'s str> {
    schema
        .constraints
        .get(vertex)
        .map(|cs| {
            cs.iter()
                .filter(|c| c.sort.as_ref() == sort)
                .map(|c| c.value.as_str())
                .collect()
        })
        .unwrap_or_default()
}

fn edges<'s>(schema: &'s Schema, vertex: &str, kind: &str) -> Vec<(&'s str, Option<&'s str>)> {
    schema
        .outgoing_edges(vertex)
        .iter()
        .filter(|e| e.kind.as_ref() == kind)
        .map(|e| (e.tgt.as_ref(), e.name.as_deref()))
        .collect()
}

fn kind<'s>(schema: &'s Schema, vertex: &str) -> &'s str {
    schema.vertices.get(vertex).map_or("", |v| v.kind.as_ref())
}

const ACCOUNTS: &str = r##"asyncapi: 3.1.0
id: urn:example:accounts
info:
  title: Accounts
  version: 1.0.0
  x-audience: internal
servers:
  production:
    host: broker.example.com:8883
    protocol: mqtt
    security:
      - $ref: "#/components/securitySchemes/user"
    bindings:
      mqtt:
        clientId: accounts
defaultContentType: application/json
channels:
  userSignedUp:
    address: user/{userId}/signedup
    parameters:
      userId:
        description: The user.
        location: $message.payload#/id
    servers:
      - $ref: "#/servers/production"
    messages:
      UserSignedUp:
        $ref: "#/components/messages/UserSignedUp"
  replies:
    address: null
    messages:
      Ack:
        payload: true
operations:
  onUserSignedUp:
    action: receive
    channel:
      $ref: "#/channels/userSignedUp"
    messages:
      - $ref: "#/channels/userSignedUp/messages/UserSignedUp"
    reply:
      address:
        location: $message.header#/replyTo
      channel:
        $ref: "#/channels/replies"
components:
  schemas:
    User:
      type: object
      required: [id]
      discriminator: id
      properties:
        id:
          type: integer
          minimum: 1
      definitions:
        Name:
          type: string
    Avro:
      schemaFormat: application/schema+json;version=draft-07
      schema:
        type: string
        discriminator: kind
  messages:
    UserSignedUp:
      contentType: application/json
      correlationId:
        location: $message.header#/correlationId
      headers:
        type: object
      payload:
        $ref: "#/components/schemas/User"
      traits:
        - $ref: "#/components/messageTraits/common"
      examples:
        - payload:
            id: 7
  messageTraits:
    common:
      name: common
  securitySchemes:
    user:
      type: userPassword
"##;

#[test]
fn a_document_reads_as_its_objects_fields_and_references() {
    let reading = read_asyncapi(ACCOUNTS.as_bytes()).unwrap();
    let schema = &reading.schema;
    assert_eq!(reading.roots, ["#"]);
    assert_eq!(kind(schema, "#"), "asyncapi");
    assert_eq!(kept(schema, "#", "asyncapi"), ["\"3.1.0\""]);
    assert_eq!(
        kept(schema, "#/info", "extension"),
        ["{\"x-audience\":\"internal\"}"]
    );
    assert_eq!(
        kept(schema, "#/servers/production/bindings", "mqtt"),
        ["{\"clientId\":\"accounts\"}"]
    );
    assert_eq!(kind(schema, "#/channels/userSignedUp"), "channel");
    assert_eq!(kept(schema, "#/channels/replies", "address"), ["null"]);
    assert_eq!(
        edges(
            schema,
            "#/channels/userSignedUp/messages/UserSignedUp",
            "ref"
        ),
        [(
            "#/components/messages/UserSignedUp",
            Some("#/components/messages/UserSignedUp")
        )]
    );
    assert_eq!(
        edges(schema, "#/operations/onUserSignedUp", "channel"),
        [("#/operations/onUserSignedUp/channel", None)]
    );
    assert_eq!(
        kind(schema, "#/operations/onUserSignedUp/channel"),
        "reference"
    );
    assert_eq!(
        edges(schema, "#/components/messages/UserSignedUp", "payload"),
        [("#/components/messages/UserSignedUp/payload", None)]
    );
    assert_eq!(
        edges(schema, "#/components/messages/UserSignedUp/payload", "ref"),
        [(
            "#/components/schemas/User",
            Some("#/components/schemas/User")
        )]
    );
    assert_eq!(kind(schema, "#/components/schemas/User"), "object");
    assert_eq!(
        kept(schema, "#/components/schemas/User", "discriminator"),
        ["\"id\""]
    );
    assert_eq!(
        edges(schema, "#/components/schemas/User", "definition"),
        [("#/components/schemas/User/definitions/Name", Some("Name"))]
    );
    assert_eq!(
        kind(schema, "#/components/schemas/Avro"),
        "multi-format-schema"
    );
    assert_eq!(
        kept(schema, "#/components/schemas/Avro/schema", "annotation"),
        ["{\"discriminator\":\"kind\"}"]
    );
    assert_eq!(
        kind(schema, "#/channels/replies/messages/Ack/payload"),
        "true"
    );
}

#[test]
fn the_canonical_text_reads_back_as_the_same_schema() {
    let reading = read_asyncapi(ACCOUNTS.as_bytes()).unwrap();
    let text = write_asyncapi(&reading.schema, &reading.roots[0]).unwrap();
    assert!(text.starts_with("{\"asyncapi\":\"3.1.0\",\"id\":\"urn:example:accounts\",\"info\":"));
    assert!(text.ends_with("}\n"));
    let again = read_asyncapi(text.as_bytes()).unwrap();
    assert_eq!(again.schema.vertices, reading.schema.vertices);
    assert_eq!(again.schema.edges, reading.schema.edges);
    assert_eq!(again.schema.constraints, reading.schema.constraints);
    assert_eq!(
        write_asyncapi(&again.schema, &again.roots[0]).unwrap(),
        text
    );
}

#[test]
fn a_fragment_document_is_read_where_a_reference_names_it() {
    let main = r#"asyncapi: 3.0.0
info: {title: T, version: "1"}
channels:
  c:
    messages:
      m:
        $ref: "common.yaml#/messages/m"
"#;
    let common = r##"messages:
  m:
    payload:
      $ref: "#/schemas/p"
schemas:
  p:
    type: string
unused:
  not: an object of any kind
"##;
    let reading = read_asyncapi_bundle(&[
        AsyncApiDocument {
            retrieval_uri: Some("https://e.test/api.yaml"),
            source: main.as_bytes(),
            kind: DocumentKind::AsyncApi,
        },
        AsyncApiDocument {
            retrieval_uri: Some("https://e.test/common.yaml"),
            source: common.as_bytes(),
            kind: DocumentKind::Fragment,
        },
    ])
    .unwrap();
    let schema = &reading.schema;
    assert_eq!(
        kind(schema, "https://e.test/common.yaml#/messages/m"),
        "message"
    );
    assert_eq!(
        kind(schema, "https://e.test/common.yaml#/schemas/p"),
        "string"
    );
    assert!(
        !schema
            .vertices
            .contains_key("https://e.test/common.yaml#/unused")
    );
    assert_eq!(reading.roots, ["https://e.test/api.yaml#"]);
}

fn refusal(text: &str) -> (String, AsyncApiErrorKind) {
    let error = read_asyncapi(text.as_bytes()).unwrap_err();
    (error.path, error.kind)
}

#[test]
fn versions_other_than_3_0_and_3_1_are_refused_at_the_version() {
    for version in ["2.6.0", "3.2.0", "3.1", "4.0.0", "3.0.0-"] {
        let text = format!("asyncapi: \"{version}\"\ninfo: {{title: T, version: \"1\"}}\n");
        assert_eq!(
            refusal(&text),
            (
                "/asyncapi".to_owned(),
                AsyncApiErrorKind::Version(version.to_owned())
            ),
            "{version}"
        );
    }
    for version in ["3.0.0", "3.1.0", "3.1.7-rc1"] {
        let text = format!("asyncapi: \"{version}\"\ninfo: {{title: T, version: \"1\"}}\n");
        assert!(read_asyncapi(text.as_bytes()).is_ok(), "{version}");
    }
}

#[test]
fn schema_formats_other_than_asyncapi_and_draft_07_are_refused() {
    let text = r#"asyncapi: 3.1.0
info: {title: T, version: "1"}
components:
  schemas:
    s:
      schemaFormat: application/vnd.google.protobuf;version=3
      schema: "message S {}"
"#;
    assert_eq!(
        refusal(text),
        (
            "/components/schemas/s/schemaFormat".to_owned(),
            AsyncApiErrorKind::SchemaFormat("application/vnd.google.protobuf;version=3".to_owned())
        )
    );
    let draft_2020 = r#"asyncapi: 3.1.0
info: {title: T, version: "1"}
components:
  schemas:
    s:
      $defs: {}
"#;
    assert_eq!(
        refusal(draft_2020),
        (
            "/components/schemas/s/$defs".to_owned(),
            AsyncApiErrorKind::Schema(JsonSchemaErrorKind::Draft07("$defs"))
        )
    );
}

#[test]
fn a_reply_with_an_address_needs_a_channel_without_one() {
    let text = r##"asyncapi: 3.1.0
info: {title: T, version: "1"}
channels:
  c: {address: c}
operations:
  o:
    action: send
    channel: {$ref: "#/channels/c"}
    reply:
      address: {location: "$message.header#/to"}
      channel: {$ref: "#/channels/c"}
"##;
    assert_eq!(refusal(text).0, "/operations/o/reply/address".to_owned());
}
