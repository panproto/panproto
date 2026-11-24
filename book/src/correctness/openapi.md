# OpenAPI

An [OpenAPI](https://spec.openapis.org/oas/v3.1.2.html) description states the surface of an HTTP API: its servers, its paths and the operations on them, the parameters, bodies and responses of each operation, the security it requires, and the shapes of its data as JSON Schema. Reading a description into panproto is what lets a migration or a lens be written against an API, and every field the reader keeps, resolves or drops changes what that migration may assume. This page states which descriptions panproto reads, how a description becomes a schema graph, how its references resolve, what the reader refuses and what it does not check, what the writer prints, and the laws the two satisfy.

## Standard and shapes

The reader follows version 3.1 of the [OpenAPI Specification](https://spec.openapis.org/oas/v3.1.2.html), read against the text of its latest patch, 3.1.2; it reads a document whose `openapi` field is `3.1.0`, `3.1.1`, `3.1.2` or another `3.1` patch, with an optional `-` suffix, as the 3.1 meta-schema's pattern `^3\.1\.\d+(-.+)?$` allows. [Version 3.2](https://spec.openapis.org/oas/v3.2.0.html) exists and is not read: it adds fields (`$self`, the `query` operation, `additionalOperations`, tag hierarchies, sequential media types) whose meaning a 3.1 reading would drop. A 3.0 document and a Swagger 2.0 document are refused by name: 3.0 Schema Objects are a different dialect from JSON Schema 2020-12 (`nullable`, the boolean `exclusiveMinimum`), and Swagger 2.0 has another document structure (`definitions`, `parameters` at the top level, `basePath`).

The reader is [`read_openapi`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/fn.read_openapi.html) for one document and [`read_openapi_bundle`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/fn.read_openapi_bundle.html) for a description split over several documents, each an OpenAPI document or a JSON Schema document with an optional retrieval URI, the first the entry document; both are in `panproto-io` (feature `tree-sitter`). The reader parses neither JSON nor YAML itself: a document whose first byte other than whitespace is `{` goes through the shared JSON route ([`read_json`](https://docs.rs/panproto-io/latest/panproto_io/config/tree/fn.read_json.html)), any other through the shared YAML route ([`read_yaml`](https://docs.rs/panproto-io/latest/panproto_io/config/tree/fn.read_yaml.html)), so a repeated key, a YAML alias or a second YAML document is refused before the reader sees the document, as the [text formats](text-formats.md) page states. `panproto-protocols` does not parse OpenAPI, and its schema dispatches refuse `openapi` and name this reader.

The `openapi` protocol declares a tree syntactic and a tree semantic shape. The shapes describe an instance of the schema, which is a request or response payload: JSON data, which is a tree. The schema graph itself is not a tree, since references, links and security requirements add edges between its objects.

| Supported | Read as |
|---|---|
| The OpenAPI Object and every object it holds: Info, Contact, License, Server and Server Variable, Paths and Path Item, Operation, External Documentation, Parameter (in `query`, `header`, `path` and `cookie`, with `schema` or `content`), Request Body, Media Type, Encoding, Responses and Response, Callback, Example, Link, Header, Tag, Components (all ten maps, `pathItems` among them), Security Scheme (all five types), OAuth Flows and OAuth Flow, Security Requirement | A vertex per object, whose kind names the object (`openapi`, `info`, `contact`, `license`, `server`, `server-variable`, `paths`, `path-item`, `operation`, `external-docs`, `parameter`, `request-body`, `media-type`, `encoding`, `responses`, `response`, `callback`, `example`, `link`, `header`, `tag`, `components`, `security-scheme`, `oauth-flows`, `oauth-flow`, `security-requirement`) |
| A field holding an object, a list of objects or a map of objects | An edge per object (the field's name in kebab case: `info`, `servers`, `paths`, `webhooks`, `request-body`, `security-schemes` and the rest), named by its index or key |
| The paths of a Paths Object, the status codes of a Responses Object, the expressions of a Callback | An edge per entry (`path`, `response`, `expression`) named by its key |
| A field holding a string, a boolean, any value, a list of strings or a map of strings (`openapi`, `title`, `name`, `in`, `style`, `explode`, `example`, `tags`, `scopes` and the rest) | A constraint of the field's name holding the value as canonical JSON text |
| An empty map or list of objects | A constraint of the field's name holding `{}` or `[]` |
| A Reference Object, with `summary` and `description` | A `reference` vertex with a `ref` edge to the object it resolves to, named by the reference as written; `summary` and `description` are constraints, and any other member, which the specification ignores, an `ignored` constraint holding the member |
| A Path Item's `$ref` | A `ref` edge from the Path Item to the Path Item it resolves to |
| A Link's `operationId` or `operationRef` | Kept as constraints, and an `operation` edge to the operation they name |
| A Security Requirement | A `scopes` constraint per scheme holding the scheme name and its scopes, and a `requirement` edge per scheme to the security scheme of the components |
| A Schema Object | The JSON Schema 2020-12 reading of the [JSON Schema](json-schema.md) page, under the OAS dialect `https://spec.openapis.org/oas/3.1/dialect/base`, whose `discriminator`, `xml`, `externalDocs` and `example` are kept constraints |
| A specification extension (`x-`) | An `extension` constraint holding the member as written |

| Not supported | Refusal | Reason |
|---|---|---|
| A Swagger 2.0 document (a `swagger` field) | `Swagger` | Another document structure and another schema dialect |
| An `openapi` version other than 3.1.x, 3.0.x and 3.2.x among them | `Version` | 3.0 Schema Objects are not JSON Schema 2020-12; 3.2 adds fields a 3.1 reading would drop |
| `jsonSchemaDialect` or a Schema Object's `$schema` naming a dialect other than the OAS dialect and 2020-12 | `Dialect`, or `Schema` with `UnsupportedDialect` | Another dialect's keywords mean other things |
| A reference to a document outside the bundle | `External` | The reader performs no network access; the referenced document must be given in the bundle with its retrieval URI |
| A document of a bundle that is neither an OpenAPI document nor a JSON Schema document (a file holding only parameters, for instance) | none (the caller names each document's kind) | The reader learns a document's kind from the caller, not from the reference that reaches it |

## The OpenAPI graph

Each object of a document is a vertex whose id is the document's prefix, then `#`, then the object's JSON pointer from the document root (RFC 6901, `~` written `~0` and `/` written `~1`): the prefix is the document's retrieval URI when it has one, else empty, so the root of a lone document is `#` and the `get` operation of the path `/pets/{id}` is `#/paths/~1pets~1{id}/get`. A Schema Object is a subschema vertex named the same way, so the subschemas of every Schema Object of a description are vertices of the one schema. The protocol's object table ([`OBJECTS`](https://docs.rs/panproto-protocols/latest/panproto_protocols/api/openapi/constant.OBJECTS.html)) lists each kind's fields in the order of the specification's tables, and the reader visits them in that order, so an object's constraints and edges come in that order whatever the order of the source.

Every value is kept, never interpreted away: a value field is a constraint whose value is canonical JSON text (no whitespace, members in source order, integers and exact decimals as written in their canonical form, a float in its shortest round-tripping form), so `minimum: 3` stays `3` and `enum: ["a,b", 1, null]` stays that array. The `openapi` field is a constraint like any other, so the version read is the version written. The entry document's root is the schema's entry, and every `$defs` member of a Schema Object is an entry, as on the JSON Schema page.

## How references resolve

A Reference Object's `$ref`, a Path Item's `$ref` and a Link's `operationRef` are resolved by the reader against the document's base URI, its retrieval URI or none, with the RFC 3986 functions of the shared reference resolution ([`resolve_uri_reference`](https://docs.rs/panproto-io/latest/panproto_io/resolution/fn.resolve_uri_reference.html) and [`resolve_uri_against`](https://docs.rs/panproto-io/latest/panproto_io/resolution/fn.resolve_uri_against.html)). A reference that is only a fragment names a place of its own document. Any other reference is resolved and split at `#`: the part before must be the retrieval URI of a document of the bundle (`External` otherwise), and the fragment must be empty or a JSON pointer, percent-decoded (`RefTarget` otherwise). The place it names must be an object the reader read (`RefTarget`) of the kind the reference's place takes (`RefKind`): a parameter's reference must name a parameter, a response's a response, a Path Item's a Path Item, a Link's `operationRef` an operation. A Reference Object may name another Reference Object of the same kind, and a chain of references that returns to a place it has visited is refused with `RefCycle`. A Schema Object's `$ref` resolves as JSON Schema resolves it, against the `$id` resources in scope and, without one, against the document, so `#/components/schemas/Pet` names that Schema Object; a pointer to any place that is not a Schema Object is refused with `Schema` and `RefTarget`.

A Link's `operationId` names the operation with that `operationId` among every operation of the description (`UnknownOperationId` otherwise). A Security Requirement's names name the security schemes of its own document's components (`UnknownScheme` otherwise).

## What the reader refuses

Every refusal is an [`OpenApiError`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/struct.OpenApiError.html) carrying the index of the document, the JSON pointer of the value at fault, its byte offset, and an [`OpenApiErrorKind`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/enum.OpenApiErrorKind.html), and the reader returns the first fault in this order. First, each document's route refusal (`Syntax`), and each retrieval URI absolute with no fragment (`Invalid`) and given once (`DuplicateDocument`); then, document by document, the root an object (`WrongType`), `swagger` absent (`Swagger`), `openapi` present (`NotOpenApi`), a string (`WrongType`) and a 3.1 version (`Version`), and `jsonSchemaDialect` naming the OAS dialect or 2020-12 (`Dialect`).

Then the reader walks the objects depth first. At each object it checks, in this order: that it is an object (`WrongType`); each member in source order, which must be a field of the object, an `x-` extension, or a patterned key (`UnknownField`); each REQUIRED field in table order (`Missing`); the object's own rules; and then each field in table order, its value's JSON kind (`WrongType`) before the objects it holds. The objects' own rules are the MUSTs of the 3.1.2 text that the field table does not state:

- the OpenAPI Object holds `paths`, `components` or `webhooks` (`Missing`), and its tags have distinct names (`DuplicateTag`);
- a License's `identifier` and `url`, an Example's `value` and `externalValue`, a Media Type's, Parameter's and Header's `example` and `examples`, and a Link's `operationRef` and `operationId` are exclusive (`Exclusive`), and a Link has one of the last two (`Missing`);
- a Server's URL has no query or fragment, a Server Variable's `enum` is not empty and holds its `default` (`Invalid`);
- a Parameter's `in` is `query`, `header`, `path` or `cookie`; it has `schema` or `content` but not both (`Exclusive`, `Missing`), `content` holds one media type, and `style`, `explode`, `allowReserved`, `example` and `examples` are used only with `schema`; `allowEmptyValue` and `allowReserved` only in `query`; `style` is one its location allows (`matrix`, `label` or `simple` in `path`, `form`, `spaceDelimited`, `pipeDelimited` or `deepObject` in `query`, `simple` in `header`, `form` in `cookie`); a path parameter has `required: true` (`Invalid`);
- a Header follows the same strategy rules, has no `name`, `in`, `allowEmptyValue` or `allowReserved` (`UnknownField`), and its `style` is `simple`; an Encoding's `style` is one of the four `query` styles (`Invalid`);
- a path begins with `/`, a response is keyed by `default`, a status code or `1XX` to `5XX`, a Responses Object holds at least one, and a component name matches `^[a-zA-Z0-9.\-_]+$` (`Invalid`, `ComponentName`);
- a Security Scheme's `type` is one of the five, it has the fields its type requires and none another type uses (`Missing`, `Invalid`), an API key is in `query`, `header` or `cookie`, and `bearerFormat` is used only with the `bearer` scheme; each OAuth flow has its URLs and none another flow uses.

After the walk come, in this order: the references, in the order they were read; the Schema Objects, by the JSON Schema reader, whose refusals are `Schema` carrying the JSON Schema kind; the operation ids, distinct among every operation of the description (`DuplicateOperationId`); each Link's operation; each Security Requirement's schemes; each parameters list, holding a name and location once once its references resolve (`DuplicateParameter`); and the paths of each document: each template brace paired around a name (`Template`), no two paths equal but for their template names (`DuplicatePath`), each path parameter of the Path Item (through its `$ref`) and of each operation a template expression of the path (`UnknownPathParameter`), and each template expression a path parameter of every operation, from the Path Item or the operation (`MissingPathParameter`). A Path Item with no operations needs no path parameters, as the specification allows.

### What the reader does not check

These MUSTs of the specification are not checked, and the description is read as written: that a field described as a URI, a URL or an email address has that form; that an Encoding's key names a property of its Media Type's schema, which depends on how the schema's applicators combine; that runtime expressions (the keys of a Callback, the values of a Link's `parameters`) are well formed, which the reader keeps as text; that the path parameters of Path Items under `webhooks`, `callbacks` and `components/pathItems` match a template, since only the Paths Object gives a path; and that a `discriminator`'s `mapping` names schemas. A header parameter named `Accept`, `Content-Type` or `Authorization`, which the specification says is ignored, is kept. None of the SHOULD rules is checked: that examples match their schemas, that a single response code is a success code, that operation ids follow programming naming conventions, that HTTP authentication schemes are registered, and that a Reference Object's `summary` and `description` override the target's (the reader keeps both and joins them by the `ref` edge).

## Canonical output

[`write_openapi`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/fn.write_openapi.html) writes the document of an `openapi` schema rooted at a given vertex as its canonical text: one line of JSON with no whitespace between tokens, ended by a line feed. An object's members are its fields in the order of the object table, which is the order of the specification's tables (a Path Item's `$ref` first), each value field from its constraint, each field of objects from its edges in the order of the edges, an empty map or list from its constraint; then its patterned entries in the order of their edges, a Security Requirement's entries in the order read; then its extensions in the order read. A Reference Object is its `$ref`, `summary`, `description` and ignored members, in that order. A Schema Object is written as the JSON Schema page's writer writes a subschema. The writer refuses, with an [`OpenApiWriteError`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/enum.OpenApiWriteError.html), a root or target that is not a vertex and a constraint that is not JSON text of its form. The reader claims no format preservation: YAML is written as JSON, and the layout and member order of the source outside the order above are not kept.

## Conformance corpus

The corpus under `crates/panproto-io/tests/fixtures/conformance/openapi` holds, from [OAI/OpenAPI-Specification](https://github.com/OAI/OpenAPI-Specification) (Apache License 2.0; `UPSTREAM.md` names the commits), the 3.1 meta-schema's 35 pass and 11 fail test documents at commit `ef5916de` of the `v3.1-dev` branch and the two 3.1 example documents in JSON at commit `e77481c7`, with 46 refusal cases written for panproto from the 3.1.2 text, each breaking one rule. Every upstream document must read, write its canonical text and read back from it as the same schema, unless `MANIFEST` lists it with the refusal it draws; every case `MANIFEST` lists must be refused at its pointer with its class (`conformance_openapi.rs`). Every fail document and every written case is refused where `MANIFEST` says. Three pass documents are refused for a MUST of the specification text that the meta-schema does not check: two whose Links name an `operationId` no operation of the document has, and one whose content-based path parameter lacks `required: true`. Two pass documents are declared limits in `openapi-limits.txt`, which the test checks: one names the meta-schema's work-in-progress dialect in `jsonSchemaDialect`, and one holds a Reference Object to a document outside the bundle.

## A document reads as the page defines it {#L-openapi-1}

### Statement

For a document $d$ of the bounded scope, write $R(d)$ for `read` of $d$ with no retrieval URI and $T(d)$ for the JSON text `render(d)`. When $R(d)$ is a reading, `read_openapi` reads $T(d)$ as a schema with exactly the vertices and kinds of $R(d)$, the out-edges of each vertex in the same order with the same kinds, targets and names, the same constraints in the same order with the same JSON texts, the same required edges, the same entries in the same order and the same root. When $R(d)$ refuses at pointer $p$ with class $k$, `read_openapi` refuses at $p$ with class $k$.

### Outside the domain

The reference reads data trees of nulls, booleans, integers and strings, so floats, exact decimals, wide integers and YAML are checked by the fixed tests and the corpus, as are bundles of several documents. A refusal's byte offset and its message are not compared; the document, pointer and class are. Refusals of the JSON route are the route's subject.

### Definitions used

$R$ applies the rules of the sections above in their order, with its own field table, and reads Schema Objects with `read_embedded`, the JSON Schema reference's reading of Schema Objects held at JSON pointers of host documents (each document a resource, the OAS vocabulary kept and checked). The scope is a base document holding one path `/p/{id}` with one operation `getP`, and components holding a Schema Object `S` (a property `n/m` and a `discriminator`), a parameter `id`, a response `ok` and an API key scheme `key`; the scope varies four choices on it: the `openapi` version (4: `3.1.0`, `3.1.2`, `3.0.3`, `3.2.0`), the operation's one parameter (13: an inline path parameter, a Reference Object with a `description` and an ignored member, a path parameter whose schema refers to `S`, a content-based path parameter with examples and an extension, and nine that break a rule: no `required`, a dangling reference, a reference to a response, a `matrix` style in `query`, a query parameter that leaves the template without a parameter, `schema` with `content`, a path parameter whose name is no template, `allowReserved` in a header, and a schema reference to nothing), the operation's responses (7: `200`, a `default` Reference Object, `2XX` with a Link to `getP`, and four that break a rule: none, a `600` key, no `description`, a Link to an unknown `operationId`), and a member added to the document (10: none, a Security Requirement for `key`, an extension, a webhook referring to the path, and six that break a rule: a requirement for an undeclared scheme, a repeated tag, a draft-07 `jsonSchemaDialect`, `swagger`, `basePath`, and a Server Variable whose default is not in its enum). That is $4 \cdot 13 \cdot 7 \cdot 10 = 3640$ documents.

### Proof

1. Production reads $T(d)$ through the JSON route, which reads a JSON text of objects, arrays, strings, integers, booleans and nulls as the tree $d$, members in source order (the route's grammar and its refusals are the shared JSON route's).
2. Production checks the same conditions as $R$ in the same order: the version and dialect of the root; then, at each object of the depth-first walk in table order, its kind, its members in source order, its REQUIRED fields in table order, its own rules in the order the section on refusals lists them, and its fields' JSON kinds before the objects they hold; then the references in the order read, the Schema Objects by the JSON Schema rules of the JSON Schema page (which `read_embedded` follows, as [L-json-schema-1](json-schema.md#L-json-schema-1) states for `read`), the operation ids, the Links, the Security Requirements, the parameters lists and the paths. Both locate a reference by the same RFC 3986 resolution and the same JSON pointer decoding, so the first condition that fails is the same, at the same pointer, with the same class.
3. On a document both accept, both create the same vertices with the same ids (the same pointer escaping), add each edge to an object of the description when its target is built, in table order, each reference edge in the order the references were read, the Schema Objects' vertices and edges after the walk with the edges to them in the order they were met, and then the Link and Security Requirement edges, and keep each value in table order with the extensions after it, so the readings agree.
4. The exhaustive differential checks the agreement on all 3640 documents, of which 96 are accepted: each choice is read on its own in 2 versions, 4 parameters, 3 responses and 4 added members, and a read choice of one kind breaks no rule of another, since the base declares every component the read choices refer to, so the accepted documents are the $2 \cdot 4 \cdot 3 \cdot 4 = 96$ whose four choices are each read. The property differential checks documents with up to three further paths, each with an operation holding one of the 13 parameters, which makes the path template rules meet each other and the base path.

### Executable counterpart

- Law ID: `openapi.reading`
- Reference: `panproto_reference::pages::openapi::read`
- Reference: `panproto_reference::pages::json_schema::read_embedded`
- Enumerator: `panproto_reference::enumerate::openapi::documents; bound: documents=3640`
- Exhaustive differential test: `openapi_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_openapi --no-tests=fail -E 'test(=openapi_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_openapi_documents_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_openapi --no-tests=fail -E 'test(=randomized_openapi_documents_read_and_write_as_the_reference)'`
- Reference test: `a_document_reads_as_its_object_vertices_edges_and_constraints; command: cargo nextest run -p panproto-reference --test openapi --no-tests=fail -E 'test(=a_document_reads_as_its_object_vertices_edges_and_constraints)'`
- Reference test: `openapi_references_resolve_to_their_targets; command: cargo nextest run -p panproto-reference --test openapi --no-tests=fail -E 'test(=openapi_references_resolve_to_their_targets)'`
- Reference test: `openapi_refusals_name_the_place_and_the_rule; command: cargo nextest run -p panproto-reference --test openapi --no-tests=fail -E 'test(=openapi_refusals_name_the_place_and_the_rule)'`
- Randomized input scope: the scope's documents with up to three further paths drawn from `/a`, `/a/{x}`, `/b/{id}`, `/c~d/e` and `/a/{y}`, each with an operation holding one of the 13 parameters and a `200` response.
- File: `crates/panproto-io/src/api/openapi.rs`
- File: `crates/panproto-io/src/data_schema/json_schema.rs`
- File: `crates/panproto-reference/src/pages/openapi.rs`

## The writer prints the canonical document, which reads back as the same schema {#L-openapi-2}

### Statement

For a document $d$ of the bounded scope that $R$ accepts, write $C(d)$ for `canonical_text(d)`. `write_openapi` writes the schema `read_openapi` reads from $T(d)$, at its root, as exactly $C(d)$, and `read_openapi` reads $C(d)$ as a schema with the same reading.

### Outside the domain

Schemas that no document reads as (a missing root, a constraint that is not JSON, a vertex of a kind with no place in a document) are refused by the writer; the reference, which writes from documents, does not model them.

### Definitions used

$C(d)$ is the compact JSON of $d$ in which each object's members are reordered as the canonical-output section states and each Schema Object is written as `canonical_text` of the JSON Schema page writes it, followed by a line feed.

### Proof

1. Production writes each value field from its constraint's JSON text, which by [L-openapi-1](#L-openapi-1) is the compact JSON of the field's value in $d$; each reference from its edge's name, which is the reference as written; each field of objects from its edges, which are in the order of the field's entries in the source; each Schema Object by the JSON Schema writer, which by [L-json-schema-2](json-schema.md#L-json-schema-2) writes the canonical subschema; and it orders an object's groups as $C$ does.
2. $C(d)$ is a JSON text, so the JSON route reads it as the tree of $d$ with each object's members reordered. An accepted reading depends on an object's member order only through the order of its extensions, of its patterned entries and of each map's and list's entries, which $C$ keeps: the reader visits fields in table order whatever their order in the source, and refusals do not arise since $d$ is accepted and every rule is a condition on members, not on their order. The reading of $C(d)$ is therefore the reading of $d$.
3. The exhaustive differential checks both equalities on all 96 accepted documents, and the property differential on generated ones.

### Executable counterpart

- Law ID: `openapi.canonical-writing`
- Reference: `panproto_reference::pages::openapi::canonical_text`
- Enumerator: `panproto_reference::enumerate::openapi::documents; bound: documents=3640`
- Exhaustive differential test: `openapi_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_openapi --no-tests=fail -E 'test(=openapi_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_openapi_documents_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_openapi --no-tests=fail -E 'test(=randomized_openapi_documents_read_and_write_as_the_reference)'`
- Reference test: `the_canonical_openapi_document_orders_fields_as_the_page_states; command: cargo nextest run -p panproto-reference --test openapi --no-tests=fail -E 'test(=the_canonical_openapi_document_orders_fields_as_the_page_states)'`
- Reference test: `the_openapi_document_scope_has_its_closed_form_count; command: cargo nextest run -p panproto-reference --test openapi --no-tests=fail -E 'test(=the_openapi_document_scope_has_its_closed_form_count)'`
- File: `crates/panproto-io/src/api/openapi.rs`
- File: `crates/panproto-reference/src/pages/openapi.rs`
- File: `crates/panproto-reference/src/enumerate/openapi.rs`
