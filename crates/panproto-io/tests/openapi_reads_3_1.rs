//! The `OpenAPI` 3.1 reader keeps what a description says and refuses what
//! it does not read, by name and place.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_io::api::openapi::{
    ApiDocument, DocumentKind, OpenApiErrorKind, read_openapi, read_openapi_bundle, write_openapi,
};
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

const PETS: &str = r##"openapi: 3.1.0
info:
  title: Pets
  version: 1.0.0
paths:
  /pets/{petId}:
    parameters:
      - $ref: "#/components/parameters/petId"
        description: The pet to fetch.
    get:
      operationId: getPet
      responses:
        "200":
          description: The pet.
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/Pet"
          links:
            self:
              operationId: getPet
              parameters:
                petId: $response.body#/id
webhooks:
  newPet:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/Pet"
      responses:
        "200":
          description: Received.
components:
  schemas:
    Pet:
      type: object
      required: [id]
      properties:
        id:
          type: integer
          exclusiveMinimum: 0
          multipleOf: 1
        tag:
          type: ["string", "null"]
          enum: ["a,b", 1, null]
        kind:
          const: pet
      discriminator:
        propertyName: kind
      x-internal: true
  parameters:
    petId:
      name: petId
      in: path
      required: true
      schema:
        type: integer
        minimum: 3
  securitySchemes:
    key:
      type: apiKey
      name: X-Key
      in: header
security:
  - key: []
x-audience: public
"##;

#[test]
fn schema_keywords_are_kept_exactly_and_type_arrays_are_unions() {
    let schema = read_openapi(PETS.as_bytes()).unwrap().schema;
    let tag = "#/components/schemas/Pet/properties/tag";
    assert_eq!(kind(&schema, tag), "union");
    assert_eq!(kept(&schema, tag, "type"), ["[\"string\",\"null\"]"]);
    assert_eq!(kept(&schema, tag, "enum"), ["[\"a,b\",1,null]"]);
    let id = "#/components/schemas/Pet/properties/id";
    assert_eq!(kind(&schema, id), "integer");
    assert_eq!(kept(&schema, id, "exclusiveMinimum"), ["0"]);
    assert_eq!(kept(&schema, id, "multipleOf"), ["1"]);
    assert_eq!(
        kept(&schema, "#/components/schemas/Pet/properties/kind", "const"),
        ["\"pet\""]
    );
    let pet = "#/components/schemas/Pet";
    assert_eq!(
        kept(&schema, pet, "discriminator"),
        ["{\"propertyName\":\"kind\"}"]
    );
    assert_eq!(kept(&schema, pet, "annotation"), ["{\"x-internal\":true}"]);
}

#[test]
fn references_resolve_to_edges_and_keep_their_overrides() {
    let schema = read_openapi(PETS.as_bytes()).unwrap().schema;
    let reference = "#/paths/~1pets~1{petId}/parameters/0";
    assert_eq!(kind(&schema, reference), "reference");
    assert_eq!(
        edges(&schema, reference, "ref"),
        [(
            "#/components/parameters/petId",
            Some("#/components/parameters/petId")
        )]
    );
    assert_eq!(
        kept(&schema, reference, "description"),
        ["\"The pet to fetch.\""]
    );
    let media = "#/paths/~1pets~1{petId}/get/responses/200/content/application~1json";
    let (target, _) = edges(&schema, media, "schema")[0];
    assert_eq!(
        edges(&schema, target, "ref"),
        [("#/components/schemas/Pet", Some("#/components/schemas/Pet"))]
    );
    assert_eq!(
        edges(&schema, "#/security/0", "requirement"),
        [("#/components/securitySchemes/key", Some("key"))]
    );
    assert_eq!(
        edges(
            &schema,
            "#/paths/~1pets~1{petId}/get/responses/200/links/self",
            "operation"
        ),
        [("#/paths/~1pets~1{petId}/get", None)]
    );
}

#[test]
fn webhooks_and_extensions_are_read() {
    let schema = read_openapi(PETS.as_bytes()).unwrap().schema;
    assert_eq!(
        edges(&schema, "#", "webhooks"),
        [("#/webhooks/newPet", Some("newPet"))]
    );
    assert_eq!(kind(&schema, "#/webhooks/newPet/post"), "operation");
    assert_eq!(
        kept(&schema, "#", "extension"),
        ["{\"x-audience\":\"public\"}"]
    );
}

#[test]
fn the_json_and_yaml_forms_read_as_one_schema_and_write_one_text() {
    let yaml = read_openapi(PETS.as_bytes()).unwrap();
    let text = write_openapi(&yaml.schema, &yaml.roots[0]).unwrap();
    let json = read_openapi(text.as_bytes()).unwrap();
    assert_eq!(write_openapi(&json.schema, &json.roots[0]).unwrap(), text);
    assert!(text.starts_with("{\"openapi\":\"3.1.0\",\"info\":{\"title\":\"Pets\""));
    assert!(text.contains("\"minimum\":3"), "{text}");
    assert!(text.contains("\"description\":\"The pet.\""), "{text}");
}

#[test]
fn the_canonical_text_writes_the_version_read_and_the_stated_key_order() {
    let source = b"{\"paths\":{},\"x-a\":1,\"info\":{\"version\":\"2\",\"title\":\"T\"},\"openapi\":\"3.1.2\"}";
    let reading = read_openapi(source).unwrap();
    assert_eq!(
        write_openapi(&reading.schema, &reading.roots[0]).unwrap(),
        "{\"openapi\":\"3.1.2\",\"info\":{\"title\":\"T\",\"version\":\"2\"},\"paths\":{},\"x-a\":1}\n"
    );
}

#[test]
fn a_description_with_components_and_no_paths_reads() {
    let source = b"openapi: 3.1.0\ninfo: {title: T, version: '1'}\ncomponents:\n  schemas:\n    A: {type: string}\n";
    let schema = read_openapi(source).unwrap().schema;
    assert_eq!(kind(&schema, "#/components/schemas/A"), "string");
}

#[test]
fn swagger_and_other_versions_are_refused_by_name_and_place() {
    let swagger = read_openapi(b"{\"swagger\":\"2.0\",\"info\":{},\"paths\":{}}").unwrap_err();
    assert_eq!(swagger.kind, OpenApiErrorKind::Swagger("2.0".into()));
    assert_eq!((swagger.path.as_str(), swagger.offset), ("/swagger", 11));
    for version in ["3.0.3", "3.2.0", "'3.1'"] {
        let source = format!("openapi: {version}\ninfo: {{title: T, version: '1'}}\npaths: {{}}\n");
        let error = read_openapi(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            OpenApiErrorKind::Version(version.trim_matches('\'').into())
        );
        assert_eq!(error.path, "/openapi");
    }
    let number =
        read_openapi(b"openapi: 3.1\ninfo: {title: T, version: '1'}\npaths: {}\n").unwrap_err();
    assert_eq!(number.kind, OpenApiErrorKind::WrongType("a string"));
    let other = read_openapi(b"{\"title\":\"not an API\"}").unwrap_err();
    assert_eq!(other.kind, OpenApiErrorKind::NotOpenApi);
}

#[test]
fn a_dangling_reference_is_refused_with_its_place() {
    let source = b"openapi: 3.1.0\ninfo: {title: T, version: '1'}\ncomponents:\n  responses:\n    A:\n      $ref: '#/components/responses/B'\n";
    let error = read_openapi(source).unwrap_err();
    assert_eq!(
        error.kind,
        OpenApiErrorKind::RefTarget("#/components/responses/B".into())
    );
    assert_eq!(error.path, "/components/responses/A/$ref");
    assert_eq!(error.offset, 90);
}

#[test]
fn a_bundle_resolves_references_between_documents_by_retrieval_uri() {
    let common = b"openapi: 3.1.0\ninfo: {title: Common, version: '1'}\ncomponents:\n  parameters:\n    limit:\n      name: limit\n      in: query\n      schema:\n        $ref: pet.json\n";
    let pet = b"{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\"}}}";
    let api = b"openapi: 3.1.0\ninfo: {title: API, version: '1'}\npaths:\n  /pets:\n    get:\n      parameters:\n        - $ref: common.yaml#/components/parameters/limit\n";
    let documents = [
        ApiDocument {
            retrieval_uri: Some("https://example.com/api.yaml"),
            source: api,
            kind: DocumentKind::OpenApi,
        },
        ApiDocument {
            retrieval_uri: Some("https://example.com/common.yaml"),
            source: common,
            kind: DocumentKind::OpenApi,
        },
        ApiDocument {
            retrieval_uri: Some("https://example.com/pet.json"),
            source: pet,
            kind: DocumentKind::Schema,
        },
    ];
    let reading = read_openapi_bundle(&documents).unwrap();
    assert_eq!(
        reading.roots,
        [
            "https://example.com/api.yaml#",
            "https://example.com/common.yaml#",
            "https://example.com/pet.json#"
        ]
    );
    let schema = &reading.schema;
    assert_eq!(
        edges(
            schema,
            "https://example.com/api.yaml#/paths/~1pets/get/parameters/0",
            "ref"
        ),
        [(
            "https://example.com/common.yaml#/components/parameters/limit",
            Some("common.yaml#/components/parameters/limit")
        )]
    );
    assert_eq!(
        edges(
            schema,
            "https://example.com/common.yaml#/components/parameters/limit/schema",
            "ref"
        ),
        [("https://example.com/pet.json#", Some("pet.json"))]
    );
    let alone = read_openapi(api).unwrap_err();
    assert_eq!(
        alone.kind,
        OpenApiErrorKind::External("common.yaml#/components/parameters/limit".into())
    );
}

#[test]
fn a_parameter_list_is_checked_after_its_references_resolve() {
    let source = b"openapi: 3.1.0\ninfo: {title: T, version: '1'}\npaths:\n  /a:\n    get:\n      parameters:\n        - {$ref: '#/components/parameters/q'}\n        - {name: q, in: query, schema: {}}\ncomponents:\n  parameters:\n    q: {name: q, in: query, schema: {}}\n";
    let error = read_openapi(source).unwrap_err();
    assert_eq!(
        error.kind,
        OpenApiErrorKind::DuplicateParameter {
            name: "q".into(),
            location: "query".into()
        }
    );
    assert_eq!(error.path, "/paths/~1a/get/parameters/1");
}

#[test]
fn a_path_template_takes_its_parameters_through_a_path_item_reference() {
    let source = b"openapi: 3.1.0\ninfo: {title: T, version: '1'}\npaths:\n  /a/{id}:\n    $ref: '#/components/pathItems/item'\ncomponents:\n  pathItems:\n    item:\n      parameters:\n        - {name: id, in: path, required: true, schema: {}}\n      get:\n        responses: {default: {description: d}}\n";
    let schema = read_openapi(source).unwrap().schema;
    assert_eq!(
        edges(&schema, "#/paths/~1a~1{id}", "ref"),
        [(
            "#/components/pathItems/item",
            Some("#/components/pathItems/item")
        )]
    );
}
