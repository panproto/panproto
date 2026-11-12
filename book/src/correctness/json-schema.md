# JSON Schema

A [JSON Schema](https://json-schema.org/) document describes the JSON values an application accepts: which types a value may take, which members an object must have, which items an array holds, and which other schemas, in the same document or another, a subschema borrows its rules from through `$ref`. Reading a schema into panproto is what lets a migration or a lens be written against it, and every keyword the reader keeps or drops changes what that migration may assume. This page states which documents panproto reads, how a document becomes a schema graph, how its references resolve, what the reader refuses, what the writer prints, and the laws the two satisfy.

## Standard and shapes

The reader follows the 2020-12 dialect of JSON Schema: [JSON Schema Core](https://json-schema.org/draft/2020-12/json-schema-core) and [JSON Schema Validation](https://json-schema.org/draft/2020-12/json-schema-validation), with the vocabularies of the dialect's [meta-schema](https://json-schema.org/draft/2020-12/schema) `https://json-schema.org/draft/2020-12/schema`. A document is read as a tree: the `json-schema` protocol declares a tree syntactic and a tree semantic shape. The reader is [`read_schema`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/fn.read_schema.html) for one document, and [`read_bundle`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/fn.read_bundle.html) for a set of documents each with an optional retrieval URI, in `panproto-io` (feature `tree-sitter`). It reads no JSON itself: every document goes through the shared JSON route ([`read_json`](https://docs.rs/panproto-io/latest/panproto_io/config/tree/fn.read_json.html), [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259) through the tree-sitter JSON grammar), so a repeated member name, a comment, a second value after the first and text that is not UTF-8 are refused before the reader sees the document. `panproto-protocols` does not parse JSON Schema, and its schema dispatches refuse `json-schema` and name this reader.

A document that names no dialect in `$schema` is read as 2020-12, which is what the specification asks of a document without one when the reader is configured for that dialect. A document, or an embedded resource, whose `$schema` names any other URI is refused: drafts 04 to 2019-09 give `items`, `$ref` beside other keywords, `dependencies` and the identifier keywords different meanings, and reading them under 2020-12 would change what the document says. Every reference resolves within the bundle, and every resource of the bundle is 2020-12, so no reference crosses from one dialect to another.

| Supported | Read as |
|---|---|
| A schema object, `true` and `false` | A vertex per subschema |
| `type` | The vertex kind (`null`, `boolean`, `object`, `array`, `number`, `string`, `integer`; `union` for an array of types; `any` without `type`), and a constraint |
| `$defs`, `properties`, `patternProperties`, `dependentSchemas` | An edge per member (`def`, `prop`, `pattern-prop`, `dependent-schema`) named by its key; an empty object is a constraint |
| `prefixItems`, `allOf`, `anyOf`, `oneOf` | An edge per item (`prefix-item`, `all-of`, `any-of`, `one-of`) named by its index |
| `additionalProperties`, `propertyNames`, `items`, `contains`, `not`, `if`, `then`, `else`, `unevaluatedItems`, `unevaluatedProperties`, `contentSchema` | One edge each (`additional-props`, `property-names`, `items`, `contains`, `not`, `if`, `then`, `else`, `unevaluated-items`, `unevaluated-props`, `content-schema`) |
| `$ref` | A `ref` edge to the subschema it resolves to, named by the reference as written |
| `$dynamicRef` whose target its static resolution fixes | A `dynamic-ref` edge to that subschema |
| `$schema` naming 2020-12, `$id`, `$anchor`, `$dynamicAnchor`, `$vocabulary`, `$comment` | Constraints; `$id` and the anchors also identify the subschema for references |
| The validation keywords (`enum`, `const`, `multipleOf`, the bounds, the length, item, contains and property counts, `pattern`, `uniqueItems`, `required`, `dependentRequired`), `format`, `contentEncoding`, `contentMediaType`, and the meta-data keywords (`title`, `description`, `default`, `deprecated`, `readOnly`, `writeOnly`, `examples`) | Constraints; `required` names that `properties` declares are also required edges |
| Any other member of a schema object | An `annotation` constraint holding the member, as 2020-12 collects an unknown keyword |

| Not supported | Refusal | Reason |
|---|---|---|
| `$schema` naming another dialect, among them drafts 04, 06, 07 and 2019-09 and a custom meta-schema | `UnsupportedDialect` | Their keywords mean other things than 2020-12's |
| `definitions`, `dependencies`, `additionalItems`, `$recursiveRef`, `$recursiveAnchor` | `Superseded` | Keywords of earlier drafts that 2020-12 replaced; a schema using them was written for another dialect |
| `items` holding an array (the earlier drafts' tuple form) | `WrongType` | 2020-12 writes a tuple with `prefixItems` |
| A `$dynamicRef` whose target depends on the dynamic scope | `DynamicScope` | Its target is fixed only by an evaluation, which a schema graph has no notion of |
| A reference to a resource outside the bundle | `RemoteRef` | The reader performs no network access; the referenced document must be given in the bundle |
| A relative `$id` or reference with no base URI in scope | `NoBase` | RFC 3986 resolves a relative reference only against a base |
| [CDDL](https://www.rfc-editor.org/rfc/rfc8610) (RFC 8610) documents | none (not read under the contract) | No tree-sitter CDDL grammar is vendored; see the paragraph below |
| [XML Schema](https://www.w3.org/TR/xmlschema11-1/) documents | none | Not a JSON Schema format |

The `cddl` protocol keeps the reader `parse_cddl` of `panproto-protocols`, a line-based reader that predates the format contract and stays outside it until a CDDL grammar is chosen. Run on the RFC 8610 examples (the two forms of the reputation object of Appendix H, the control operator examples of Section 3.8 and the prelude of Appendix D), it reads each rule as a vertex but drops what the contract requires it to keep: a map built from group rules reads with no members and the `reputon` rule is lost; occurrence indicators (`?`, `*`) and the `* text => any` entry are dropped; `float16` reads as `float`; every control operator (`.size`, `.bits`, `.regexp`, `.ge`, `.default`, `.within`) is dropped; a trailing comment containing `/` is read as two choice variants; socket assignments (`$message /= [...]`) read as items of the rule before them; and tags (`#6.0(tstr)`) and the simple-value prelude rules read as bare rules.

## The JSON Schema graph

Each subschema of a document, the root included, is a vertex whose id is the document's prefix, then `#`, then the subschema's JSON pointer from the document root (RFC 6901, `~` written `~0` and `/` written `~1`): the prefix is the document's retrieval URI when it has one, else the URI its root `$id` resolves to, else empty, so the root of a lone document is `#` and its property `a` is `#/properties/a`. A `true` or `false` subschema is a vertex of that kind. Every edge goes from a subschema to a subschema. A subschema's edges come keyword by keyword in the order the canonical writer uses (below), each keyword's edges in the order of its members or items, and its reference edges, `$ref` before `$dynamicRef`, come last.

Every keyword that is not an applicator or a reference is kept, never interpreted away, as a constraint of its subschema's vertex whose value is the keyword's value as canonical JSON text: no whitespace, members in source order, integers and exact decimals as written in their canonical form, a float in its shortest round-tripping form. A member 2020-12 does not define is kept as an `annotation` constraint whose value is the JSON text of an object holding that one member, and a subschema inside it is not read as a schema, as 2020-12 specifies. A `required` name that the same subschema's `properties` declares also becomes a required edge of the vertex. Each `$defs` member and each document root is an entry of the schema. Validation reads `minLength` and `maxLength` as code-point lengths and `minimum` and `maximum` as inclusive bounds; it keeps the other constraints, `format` among them (an annotation in the 2020-12 dialect), as annotations, and it does not evaluate the applicators.

## How references resolve

The reader resolves `$ref` and `$dynamicRef` while it builds the schema, with the RFC 3986 functions of the shared reference resolution ([`resolve_uri_reference`](https://docs.rs/panproto-io/latest/panproto_io/resolution/fn.resolve_uri_reference.html) and [`resolve_uri_against`](https://docs.rs/panproto-io/latest/panproto_io/resolution/fn.resolve_uri_against.html)). It does not go through the [`Resolution`](https://docs.rs/panproto-io/latest/panproto_io/resolution/struct.Resolution.html) lens, which resolves the references an instance makes to other parts of the instance; a schema reference names a subschema, and the edge it becomes is part of the schema the lens would be indexed by.

The base URI of a document's root is its retrieval URI, or none. A subschema with `$id` starts a resource whose URI is the `$id` resolved against the base in scope (a relative `$id` with no base is refused with `NoBase`), and that URI is the base of everything inside it; the root of a document without a base is a resource with the empty key, which `#`-only references reach. `$anchor` and `$dynamicAnchor` name the subschema that holds them within its resource. A reference is resolved against the base in scope at the subschema that holds it (a reference that is only a fragment, against the subschema's resource) and split at `#`: the part before names a resource of the bundle, else the reference is refused with `RemoteRef`; an empty fragment names the resource's root, a fragment starting with `/` is a JSON pointer, percent-decoded, from the resource's root, which must name a subschema (`RefTarget`), and any other fragment an anchor of the resource (`UnknownAnchor`). A `$dynamicRef` resolves the same way. 2020-12 Core, Section 8.2.3.2, replaces that static target by a subschema of the dynamic scope only when the target was named by a `$dynamicAnchor`, and then by the outermost resource of the dynamic scope that declares a dynamic anchor of the same name. Every resource an evaluation enters is a resource of the bundle, so when no other resource of the bundle declares that name every evaluation reaches the static target, and the reader keeps it as a `dynamic-ref` edge; when another does, the subschema reached depends on the evaluation, and the reference is refused with `DynamicScope`.

## What the reader refuses

Every refusal is a [`JsonSchemaError`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/struct.JsonSchemaError.html) carrying the index of the document, the JSON pointer of the value at fault, its byte offset, and a [`JsonSchemaErrorKind`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/enum.JsonSchemaErrorKind.html). The reader makes two passes and returns the first fault. The first pass takes the documents in order. A document's retrieval URI must be absolute with no fragment (`Invalid`) and given once (`DuplicateDocument`). Then the document's subschemas are visited depth first, the subschemas of a subschema keyword by keyword in the canonical order and each keyword's in source order, and at each subschema the reader checks, in this order: that it is an object or a boolean (`WrongType`, "a schema"); each keyword in source order (`Superseded` for an earlier draft's keyword, then the keyword's value against the 2020-12 meta-schema: strings for the identifiers, references, `pattern`, `format` and the text keywords, with `$id` holding no non-empty fragment and an anchor matching `^[A-Za-z_][-A-Za-z0-9._]*$`; booleans for `uniqueItems`, `deprecated`, `readOnly`, `writeOnly` and the values of `$vocabulary`; numbers for the bounds and `multipleOf`, which must be greater than 0; non-negative integers, by value, for the counts; arrays for `enum` and `examples`; a type name or a non-empty array of distinct type names for `type`; arrays of distinct strings for `required` and the values of `dependentRequired`; objects for the applicators that hold named subschemas and non-empty arrays for those that hold a list); then `$schema` (`Invalid` outside the root of a resource, `UnsupportedDialect` for another dialect); then `$id` (`NoBase`, `DuplicateId`); then its anchors in source order (`DuplicateAnchor`). The root of a second document without a base is refused with `DuplicateDocument`. The second pass visits the same subschemas in the same order and resolves each subschema's `$ref` and then its `$dynamicRef`, as the previous section states, before its subschemas.

The reader checks the JSON kinds of `pattern` and of the `patternProperties` keys and keeps them as written; it does not check that they are ECMA-262 regular expressions, nor that `format`, `contentEncoding` and `contentMediaType` name registered values, nor the URIs `$schema` and `$vocabulary` hold beyond comparing the dialect. A count written as a float with no fractional part (`2.0`) is read, as 2020-12 counts it an integer, and kept as written.

## Canonical output

[`write_document`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/fn.write_document.html) writes the document of a `json-schema` schema rooted at a given vertex as its canonical text: one line of JSON with no whitespace between tokens, ended by a line feed. A `true` or `false` vertex is written as that boolean. Any other vertex is an object whose members are its kept constraints in source order (an annotation spliced in as the member it holds, an empty object of subschemas under its keyword), then `$ref` and `$dynamicRef` as written, then the applicators in the order `$defs`, `properties`, `patternProperties`, `additionalProperties`, `propertyNames`, `dependentSchemas`, `prefixItems`, `items`, `contains`, `allOf`, `anyOf`, `oneOf`, `not`, `if`, `then`, `else`, `unevaluatedItems`, `unevaluatedProperties`, `contentSchema`, each holding its subschemas in the order of their edges. The writer refuses, with a [`JsonSchemaWriteError`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/enum.JsonSchemaWriteError.html), a root or target that is not a vertex, a constraint that is not JSON text of its form, and a boolean-schema vertex with constraints or edges. The reader claims no format preservation: the canonical text keeps every keyword and every reference as written, but not the layout of the source or its order of keywords outside the order above.

## Conformance corpus

The corpus under `crates/panproto-io/tests/fixtures/conformance/json-schema-test-suite` holds the 59 files of `validation/tests` of the [JSON-Schema-Test-Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite) at commit `38dea932` (MIT License; `UPSTREAM.md` names the commit) that hold a test case compatible with 2020-12 by the suite's `compatibility` rule: 430 cases. The suite tests a validator: each case is a schema with instances and the verdict a validator gives each. What this protocol can be checked against is the schema half: every case's schema, read as the first document of a bundle whose other documents are the case's `externalSchemas` at their retrieval URIs, must read, validate against the `json-schema` protocol, write its canonical text, and read back from that text as the same schema (`conformance_json_schema.rs`). 415 cases pass. The 15 others are declared in `json-schema-test-suite-limits.txt` with the class of their refusal, which the test checks: 13 are `dynamic-scope` (the `$dynamicRef` cases whose target the dynamic scope chooses) and 2 `custom-metaschema` (`vocabulary.json` cases whose `$schema` names a meta-schema of the suite's). The 2,082 instance verdicts of the 430 cases are declared as one class, `instance-validation`: panproto reads the schema and does not evaluate JSON Schema validation, so no reading gives a verdict to compare. The suite's `optional` and `proposals` directories are not vendored.

## A bundle reads as the page defines it {#L-json-schema-1}

### Statement

For a bundle $b$ of the bounded scope, write $R(b)$ for `read(b)` and $T(d)$ for the JSON text `render(d)` of each document $d$ of $b$. When $R(b)$ is a reading, `read_bundle` reads the texts $T(d)$, with $b$'s retrieval URIs, as a schema with exactly the vertices and kinds of $R(b)$, the out-edges of each vertex in the same order with the same kinds, targets and names, the same kept constraints in the same order with the same JSON texts, the same required edges, the same entries in the same order and the same document roots. When $R(b)$ refuses document $i$ at pointer $p$ with class $k$, `read_bundle` refuses document $i$ at $p$ with class $k$.

### Outside the domain

The reference reads data trees of nulls, booleans, integers and strings, so floats, exact decimals and wide integers are checked by the fixed tests and the corpus. A refusal's byte offset and its message are not compared; the document, pointer and class are. Refusals of the JSON route are the route's subject.

### Definitions used

$R$ applies the rules of the two sections above in their order, and resolves references with `resolve`, an RFC 3986 Section 5.2 resolver written for the reference. The scope is the base document, identified as `https://e.test/r`, whose `$defs` holds `d` (an integer anchored `A`) and whose `properties` holds no property, one of the properties `a` and `b`, or both in either order, each one of 24 subschemas (a bounded string, a type union, `true`, `false`, `{}`, an enumeration, a tuple whose keywords are out of canonical order, an object with a resolved and an unresolved required name, a pointer to `$defs/d`, the anchor `A`, a pointer to `/properties/a`, a resource `n` referring to `r#A`, the conditional and logical applicators, an unknown keyword with an empty `patternProperties`, a remote reference, a `$dynamicRef` to its own dynamic anchor `D`, a resource `m` declaring `D`, an unknown type, a negative count, `definitions`, a second anchor `A`, an `$id` with a fragment, a tuple `items` and a `$schema` below the root), together with 22 envelopes, bundles that change the base in one respect outside its properties or add a second document: $1 + 2 \cdot 24 + 2 \cdot 24^2 + 22 = 1223$ bundles.

### Proof

1. Production reads each text $T(d)$ through the JSON route, which reads a JSON text of objects, arrays, strings, integers, booleans and nulls as the tree $d$, members in source order (the route's grammar and its refusals are the shared JSON route's; the differential checks these spellings).
2. Production checks the same conditions as $R$ in the same order: in the first pass, the retrieval URI, then for each subschema, in the depth-first order the section on refusals states, its kind, its keywords in source order, `$schema`, `$id` and its anchors; in the second pass, each subschema's `$ref` and `$dynamicRef` before its subschemas. Both resolve a reference against the same base by RFC 3986 and look its resource, pointer and anchor up in what the first pass registered, so the first condition that fails is the same, in the same document, at the same pointer, with the same class.
3. On a bundle both accept, both create the same vertices with the same ids (the same prefix and pointer escaping), add each subschema edge when its target is built, keyword by keyword in the canonical order, and each reference edge after every vertex exists, in the order the references were met, and keep each keyword's JSON text in source order, so the readings agree.
4. The exhaustive differential checks the agreement on all 1223 bundles, of which 544 are accepted: 16 of the 24 subschemas read under `a` and 15 under `b`, since a pointer to `/properties/a` names nothing without `a`; two properties both of which read alone read together unless they give the same `$id` (twice `n` or twice `m`), declare `D` twice in the root resource, or declare `D` in two resources, which makes the `$dynamicRef` depend on the dynamic scope, so $16^2 - 5 = 251$ pairs read in each order; with the base and 10 of the 22 envelopes, $1 + 31 + 502 + 10 = 544$. The property differential checks generated subschemas nested three deep with property names that need escaping.

### Executable counterpart

- Law ID: `json-schema.reading`
- Reference: `panproto_reference::pages::json_schema::read`
- Reference: `panproto_reference::pages::json_schema::render`
- Reference: `panproto_reference::pages::json_schema::resolve`
- Enumerator: `panproto_reference::enumerate::json_schema::documents; bound: bundles=1223`
- Exhaustive differential test: `json_schema_reading_and_canonical_writing_match_the_reference_on_every_bounded_bundle; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_json_schema --no-tests=fail -E 'test(=json_schema_reading_and_canonical_writing_match_the_reference_on_every_bounded_bundle)'`
- Property differential test: `randomized_json_schemas_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_json_schema --no-tests=fail -E 'test(=randomized_json_schemas_read_and_write_as_the_reference)'`
- Reference test: `a_document_reads_as_its_subschema_vertices_edges_and_constraints; command: cargo nextest run -p panproto-reference --test json_schema --no-tests=fail -E 'test(=a_document_reads_as_its_subschema_vertices_edges_and_constraints)'`
- Reference test: `json_schema_refusals_name_the_place_and_the_rule; command: cargo nextest run -p panproto-reference --test json_schema --no-tests=fail -E 'test(=json_schema_refusals_name_the_place_and_the_rule)'`
- Reference test: `references_resolve_through_ids_anchors_pointers_and_rfc_3986; command: cargo nextest run -p panproto-reference --test json_schema --no-tests=fail -E 'test(=references_resolve_through_ids_anchors_pointers_and_rfc_3986)'`
- Randomized input scope: one property `spec` of the base holding a subschema of depth at most three built from typed leaves, booleans and references to `$defs/d` and the anchor `A`, objects of up to two properties named from `a`, `b/c`, `~x`, `p q` and `k` with a resolved and an unresolved required name, arrays and `anyOf`.
- File: `crates/panproto-io/src/data_schema/json_schema.rs`
- File: `crates/panproto-io/src/config/tree.rs`
- File: `crates/panproto-reference/src/pages/json_schema.rs`

## The writer prints the canonical document, which reads back as the same schema {#L-json-schema-2}

### Statement

For a bundle $b$ of the bounded scope that $R$ accepts, write $C(d)$ for `canonical_text(d)`. For each document $d$ of $b$, `write_document` writes the schema `read_bundle` reads from the texts $T$, at $d$'s root, as exactly $C(d)$, and `read_bundle` reads the texts $C(d)$, with $b$'s retrieval URIs, as a schema with the same reading.

### Outside the domain

Schemas that no document reads as (a missing root, a constraint that is not JSON, a boolean schema with keywords) are refused by the writer; the reference, which writes from documents, does not model them.

### Definitions used

$C(d)$ is the compact JSON of $d$ in which each schema object's members are reordered as the canonical-output section states, every string escaped as RFC 8259 requires with the short escapes for backspace, form feed, line feed, carriage return and tab and `\u00xx` in lowercase hexadecimal for the other control characters, followed by a line feed.

### Proof

1. Production writes each kept member from its constraint's JSON text, which by [L-json-schema-1](#L-json-schema-1) is the compact JSON of the keyword's value in $d$, in the order the constraints were kept, which is source order; it writes each reference from its edge's name, which is the reference as written, and each applicator from its edges, which are in the order of the keyword's members in the source; and it orders the groups as $C$ does.
2. $C(d)$ is a JSON text, so the JSON route reads it as the tree of $d$ with each schema object's members reordered. An accepted reading depends on a schema object's member order only through the order of its kept keywords and of each applicator's members, which $C$ keeps: the reader visits subschemas keyword by keyword in a fixed order and resolves `$ref` before `$dynamicRef` whatever their order in the source. The reading of $C(d)$ is therefore the reading of $d$.
3. The exhaustive differential checks both equalities on all 544 accepted bundles, and the property differential on generated ones.

### Executable counterpart

- Law ID: `json-schema.canonical-writing`
- Reference: `panproto_reference::pages::json_schema::canonical_text`
- Enumerator: `panproto_reference::enumerate::json_schema::documents; bound: bundles=1223`
- Exhaustive differential test: `json_schema_reading_and_canonical_writing_match_the_reference_on_every_bounded_bundle; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_json_schema --no-tests=fail -E 'test(=json_schema_reading_and_canonical_writing_match_the_reference_on_every_bounded_bundle)'`
- Property differential test: `randomized_json_schemas_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_json_schema --no-tests=fail -E 'test(=randomized_json_schemas_read_and_write_as_the_reference)'`
- Reference test: `the_canonical_document_orders_keywords_as_the_page_states; command: cargo nextest run -p panproto-reference --test json_schema --no-tests=fail -E 'test(=the_canonical_document_orders_keywords_as_the_page_states)'`
- Reference test: `the_json_schema_bundle_scope_has_its_closed_form_count; command: cargo nextest run -p panproto-reference --test json_schema --no-tests=fail -E 'test(=the_json_schema_bundle_scope_has_its_closed_form_count)'`
- File: `crates/panproto-io/src/data_schema/json_schema.rs`
- File: `crates/panproto-reference/src/pages/json_schema.rs`
- File: `crates/panproto-reference/src/enumerate/json_schema.rs`
