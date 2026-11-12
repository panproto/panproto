# Kubernetes CRDs

A Kubernetes cluster learns a new kind of resource from a `CustomResourceDefinition` (CRD): a YAML document that names the resource, lists its versions, and gives each version a schema that the API server checks every object of that kind against. Reading a CRD into a panproto schema is what lets those objects be validated and migrated between versions. This page states which CRDs panproto reads, how a CRD becomes a schema, what the reader refuses, what the writer prints, and the laws the two satisfy.

## Standard and shapes

The reader follows the [`apiextensions.k8s.io/v1` CustomResourceDefinition](https://kubernetes.io/docs/reference/kubernetes-api/extend-resources/custom-resource-definition-v1/) of Kubernetes 1.37, whose version schemas are the [structural schemas](https://kubernetes.io/docs/tasks/extend-kubernetes/custom-resources/custom-resource-definitions/#specifying-a-structural-schema) Kubernetes builds on the OpenAPI 3.0 schema object (its `JSONSchemaProps`). A CRD is read as a tree: the `k8s-crd` protocol declares a tree syntactic and a tree semantic shape. The reader is [`read_crd`](https://docs.rs/panproto-io/latest/panproto_io/config/k8s_crd/fn.read_crd.html) in `panproto-io` (feature `tree-sitter`). It reads no YAML itself: the document goes through the YAML route of the [text formats](text-formats.md) (YAML 1.2.2, one document, the core schema), so a JSON CRD, which YAML 1.2 reads as the same document, is read the same way, and every refusal of that route (several documents, an alias, a `%TAG` directive, text that is not UTF-8) is the reader's refusal too. `panproto-protocols` does not parse CRDs, and its schema dispatches refuse `k8s-crd` and name this reader.

| Supported | Read as |
|---|---|
| `apiVersion: apiextensions.k8s.io/v1`, `kind: CustomResourceDefinition` | The document; anything else is refused |
| `metadata` with `name` and the other `ObjectMeta` fields | The resource vertex, named by `metadata.name`; the other fields kept as one constraint |
| `spec.group`, `spec.names` (`plural`, `singular`, `kind`, `listKind`, `shortNames`, `categories`), `spec.scope` | Constraints of the resource vertex |
| `spec.versions[]` with `name`, `served`, `storage`, `deprecated`, `deprecationWarning`, `subresources`, `additionalPrinterColumns`, `selectableFields` | A version vertex per version, its fields kept as constraints |
| `schema.openAPIV3Schema` | The version's structural schema, below a `schema` edge |
| `spec.conversion` with strategy `None`, `spec.preserveUnknownFields: false`, top-level `status` | Constraints of the resource vertex |

| Not supported | Refusal | Reason |
|---|---|---|
| `apiextensions.k8s.io/v1beta1` and other API versions | `ApiVersion` | Kubernetes stopped serving `v1beta1` in 1.22 |
| Conversion webhooks (`strategy: Webhook`, a `webhook` field) | `Unsupported` | The conversion is code outside the document |
| `$ref`, `$schema`, `id`, `definitions`, `dependencies`, `additionalItems`, `patternProperties`, `uniqueItems: true` | `Forbidden` | Kubernetes refuses them in a CRD |
| A boolean `additionalProperties`, a scalar JSON cannot write (a `!!binary` or locally tagged scalar, a non-finite float) | `Unsupported` | No constraint text carries them exactly |
| Several CRDs in one stream | the YAML route's refusal | One document is one CRD |

## The CRD schema

A CRD named `<crd>` reads as one `custom-resource` vertex `<crd>`, a `version` vertex `<crd>/<v>` for each version `v` joined to it by a `version` edge named `v`, and, for each version, its schema nodes below a `schema` edge from the version to the root node `<crd>/<v>#`. A schema node is a vertex whose kind is its `type` (`object`, `array`, `string`, `integer`, `number` or `boolean`), or `int-or-string` for an untyped node with `x-kubernetes-int-or-string: true`, or `any` for an untyped node with `x-kubernetes-preserve-unknown-fields: true`. Below a node `n`, the property `p` is the vertex `n.p` reached by a `prop` edge named `p`, the items schema is `n[]` reached by an `items` edge, and an `additionalProperties` schema is `n{}` reached by a `values` edge; in `p`, `~` is written `~0`, `.` `~1`, `[` `~2` and `{` `~3`, so no two nodes share an id.

Every other field is kept, never interpreted away: each keyword of a schema node (its description, format, default, enum, bounds, pattern, `required`, `allOf`, `anyOf`, `oneOf`, `not`, CEL rules under `x-kubernetes-validations`, and the `x-kubernetes-*` list and map types) is a constraint of the node's vertex whose value is the keyword's value as canonical JSON text: no whitespace, mapping entries in source order, integers and exact decimals as written, a float in its shortest round-tripping form. The resource's and the versions' fields are kept the same way. Validation reads a node's `format` as an OpenAPI value format and keeps the other kept constraints as annotations; it does not evaluate value validations or CEL rules, as it does not evaluate a PostgreSQL `CHECK`.

Two kinds of field name other fields, and the reader resolves them. A `required` entry that names a property of its node becomes a required edge of the node; one that names no property is kept in the node's `required` constraint and reported, with its path, in the reading's `unresolved` list, in document order, since Kubernetes accepts it (a node that keeps unknown fields may require one it does not declare). An `x-kubernetes-list-map-keys` entry must name a property of the list's items whose type is a scalar, and the keys become a [`Key`](https://docs.rs/panproto-schema/latest/panproto_schema/keys/struct.Key.html) of the items vertex with role `Unique` and scope `Within` the list, so that the shared reference resolution and validation check the uniqueness Kubernetes enforces on a list map.

## What the reader refuses

Every refusal is a [`CrdError`](https://docs.rs/panproto-io/latest/panproto_io/config/k8s_crd/struct.CrdError.html) carrying the path of the field at fault (keys joined by `.`, sequence indices as `[i]`), the byte offset at which its value starts, and a [`CrdErrorKind`](https://docs.rs/panproto-io/latest/panproto_io/config/k8s_crd/enum.CrdErrorKind.html). The reader returns the first fault in this order. At every mapping it first checks each key, in source order, against the fields the definition declares there (`UnknownField`, or `Forbidden` for a forbidden schema field). It then reads `apiVersion` (`ApiVersion`), `kind` (`Kind`), `metadata` and its `name`, then `spec`: `group` (a domain name with a dot), `names` (`plural` and `singular` lowercase RFC 1035 labels, `kind` not empty), `scope` (`Namespaced` or `Cluster`), `versions` (at least one; for each, its `name`, a label not given before (`DuplicateVersion`), `served`, `storage`, `deprecated`, `deprecationWarning`, `schema`, `subresources`, `additionalPrinterColumns`, `selectableFields`; then exactly one storage version (`StorageVersions`)), `conversion` and `preserveUnknownFields`; then `metadata.name` against `<plural>.<group>`; and last `status`. A field of the wrong kind is `WrongType` and an absent required field `Missing`; any other broken rule is `Invalid`.

A schema node is read in this order: its keys; `x-kubernetes-int-or-string` and `x-kubernetes-preserve-unknown-fields`, which must be booleans; its `type` (`Missing` when a node has none and neither flag); each kept keyword in source order, checked against its kind (strings for `description`, `title`, `format` and `pattern`; booleans for the flags, `nullable` and the exclusive bounds; numbers for `maximum`, `minimum` and `multipleOf`; non-negative integers for the length, item and property counts; sequences for `enum`, `allOf`, `anyOf`, `oneOf`, `required`, `x-kubernetes-list-map-keys` and `x-kubernetes-validations`; mappings for `not` and `externalDocs`; a list type `atomic`, `set` or `map` on an array only, and a map type `atomic` or `granular` on an object only); then `properties` (an object only), `additionalProperties` (an object without properties), `items` (an array only, one schema, and required on an array); then the list-map keys (`ListMapKey` for a key that names no scalar property of the items); then the `required` entries. The root schema of a version must be an object.

The reader checks the shapes of `subresources`, `additionalPrinterColumns` and `selectableFields` (a mapping, and sequences of mappings) and keeps them as written; it does not check the fields inside them, nor the JSONPath expressions they hold, nor the rules Kubernetes places on the root's `metadata` property, all of which the API server refuses when broken. A node that sets both a `type` and `x-kubernetes-int-or-string: true`, as the volume snapshot CRDs Kubernetes ships do, is read with the kind of its type and keeps the flag.

## Canonical output

[`write_crd`](https://docs.rs/panproto-io/latest/panproto_io/config/k8s_crd/fn.write_crd.html) writes a `k8s-crd` schema as the canonical CRD: one line of JSON with no whitespace between tokens, ended by a line feed, which YAML 1.2 reads as the same document. The fields come in the order the definition lists them: `apiVersion`, `kind`, `metadata` (with `name` first, then the kept fields), `spec` with `group`, `names` (`plural`, `singular`, `kind`, `listKind`, `shortNames`, `categories`), `scope`, `versions`, `conversion` and `preserveUnknownFields`, then `status`; a version writes `name`, `served`, `storage`, `deprecated`, `deprecationWarning`, `schema`, `subresources`, `additionalPrinterColumns` and `selectableFields`; a schema node writes `type`, then its kept keywords in source order, then `properties`, `additionalProperties` and `items`. Each kept field is written as its JSON text. The writer refuses, with a [`CrdWriteError`](https://docs.rs/panproto-io/latest/panproto_io/config/k8s_crd/enum.CrdWriteError.html), a schema without exactly one resource vertex, a version without a schema edge, and a kept constraint that is not JSON. The reader claims no format preservation: the canonical text keeps every field but not the layout, comments or key order of the source outside the order above.

## Conformance corpus

The corpus under `crates/panproto-io/tests/fixtures/conformance/k8s-crd` holds 11 CRDs vendored unchanged from the Kubernetes repository at v1.37.2 (Apache License 2.0; `UPSTREAM.md` names each file and the commit): the sample-controller CRDs, the volume snapshot and volume populator CRDs, the Calico IP pool CRD, a kcp `APIExport` CRD and the `kubectl` test CRD. Each must read, and its canonical text must read back as the same schema and write as the same text. Kubernetes ships no suite of invalid CRDs, so 18 refusal cases are built from the test CRD by breaking one rule each, and `MANIFEST` lists each case's rule, path and refusal (`conformance_k8s_crd.rs`). All 29 cases pass, and the corpus declares no limit.

## A CRD reads as the page defines it {#L-kubernetes-crds-1}

### Statement

For a document $d$ of the bounded scope, write $R(d)$ for `read(d)` and $Y(d)$ for its block YAML text `render_yaml(d)`. When $R(d)$ is a reading, `read_crd` reads $Y(d)$ as a schema with exactly the vertices and kinds of $R(d)$, the out-edges of each vertex in the same order with the same kinds, targets and names, the same kept constraints in the same order with the same JSON texts, the same required edges and list-map keys, and the same unresolved entries in the same order. When $R(d)$ refuses at path $p$ with class $k$, `read_crd` refuses $Y(d)$ at $p$ with class $k$.

### Outside the domain

The reference reads data trees of nulls, booleans, integers and strings, so floats, exact decimals and the scalars JSON cannot write are checked by the fixed tests and the corpus. A refusal's byte offset and the message of an `Invalid` refusal are not compared; the path and class are. Refusals of the YAML route are the subject of [Text formats](text-formats.md).

### Definitions used

$R$ applies the rules of the two sections above in their order. The scope is the base CRD whose root schema holds no property, one of the properties `a` and `b`, or both in either order, each one of 18 schema nodes (a string, a bounded integer, an enum with a default, a set list, a list map whose key resolves and one whose key does not, a map, an object with a resolved and an unresolved required entry, an int-or-string node, a node that keeps unknown fields, a node without a type, a `$ref`, an unknown keyword, an array without items, properties beside `additionalProperties`, a negative length, `uniqueItems: true`, and a typed int-or-string node), together with 24 envelopes, the base changed in one field outside the schema: $1 + 2 \cdot 18 + 2 \cdot 18^2 + 24 = 709$ documents.

### Proof

1. Production reads $Y(d)$ through the YAML route, which reads a block document whose keys and strings are double-quoted and whose other scalars are integers, booleans and nulls as the tree $d$, mapping entries in source order (block reading is [L-text-formats-4](text-formats.md#L-text-formats-4); the differential checks these spellings).
2. Production checks the same conditions as $R$ in the same order: the known keys of each mapping first, then the fields in the order the section above states, and within a schema node its keys, its flags, its type, its keywords in source order, its children, its list-map keys and its required entries. The first condition that fails is therefore the same, at the same path, with the same class.
3. On a document both accept, both create the same vertices with the same ids (the same escaping), add each out-edge when its target is read, in source order, and keep each field's JSON text in the order the fields are read, so the readings agree.
4. The exhaustive differential checks the agreement on all 709 documents, of which 227 are accepted (10 of the 18 nodes are accepted, so $1 + 2 \cdot 10 + 2 \cdot 10^2 = 221$ property documents, and 6 envelopes), and the property differential on generated schemas nested three deep with property names that need escaping.

### Executable counterpart

- Law ID: `kubernetes-crds.reading`
- Reference: `panproto_reference::pages::kubernetes_crds::read`
- Reference: `panproto_reference::pages::kubernetes_crds::render_yaml`
- Enumerator: `panproto_reference::enumerate::kubernetes_crds::documents; bound: documents=709`
- Exhaustive differential test: `crd_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_k8s_crd --no-tests=fail -E 'test(=crd_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_crds_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_k8s_crd --no-tests=fail -E 'test(=randomized_crds_read_and_write_as_the_reference)'`
- Reference test: `a_crd_reads_as_its_resource_versions_and_schema_nodes; command: cargo nextest run -p panproto-reference --test kubernetes_crds --no-tests=fail -E 'test(=a_crd_reads_as_its_resource_versions_and_schema_nodes)'`
- Reference test: `crd_refusals_name_the_field_and_the_rule; command: cargo nextest run -p panproto-reference --test kubernetes_crds --no-tests=fail -E 'test(=crd_refusals_name_the_field_and_the_rule)'`
- Reference test: `unresolved_required_entries_are_reported_and_list_maps_declare_keys; command: cargo nextest run -p panproto-reference --test kubernetes_crds --no-tests=fail -E 'test(=unresolved_required_entries_are_reported_and_list_maps_declare_keys)'`
- Randomized input scope: one property `spec` holding a schema of depth at most three built from scalar leaves, objects of up to two properties named from `a`, `b.c`, `~x`, `[y`, `{z` and `k` with a resolved and an unresolved required entry, arrays and maps.
- File: `crates/panproto-io/src/config/k8s_crd.rs`
- File: `crates/panproto-io/src/config/tree.rs`
- File: `crates/panproto-reference/src/pages/kubernetes_crds.rs`

## The writer prints the canonical CRD, which reads back as the same schema {#L-kubernetes-crds-2}

### Statement

For a document $d$ of the bounded scope that $R$ accepts, write $C(d)$ for `canonical_text(d)`. `write_crd` writes the schema `read_crd` reads from $Y(d)$ as exactly $C(d)$, and `read_crd` reads $C(d)$ as a schema with the same reading.

### Outside the domain

Schemas that no CRD reads as (several resource vertices, a version without a schema, a constraint that is not JSON) are refused by the writer; the reference, which writes from documents, does not model them.

### Definitions used

$C(d)$ is the compact JSON of $d$ with its fields reordered as the canonical-output section states, every string escaped as RFC 8259 requires with the short escapes for backspace, form feed, line feed, carriage return and tab and `\u00xx` in lowercase hexadecimal for the other control characters, followed by a line feed.

### Proof

1. Production writes each field from its kept JSON text, which by [L-kubernetes-crds-1](#L-kubernetes-crds-1) is the compact JSON of the field's value in $d$, and orders the fields and schema nodes as $C$ does, walking each vertex's out-edges in the order they were added, which is source order.
2. $C(d)$ is a JSON document, so the YAML route reads it as the tree of $d$ with its fields reordered; the reader's outcome does not depend on the order of a mapping's fields except through the order of kept keywords and properties, which $C$ keeps, so it reads $C(d)$ as the same reading.
3. The exhaustive differential checks both equalities on all 227 accepted documents, and the property differential on generated ones.

### Executable counterpart

- Law ID: `kubernetes-crds.canonical-writing`
- Reference: `panproto_reference::pages::kubernetes_crds::canonical_text`
- Enumerator: `panproto_reference::enumerate::kubernetes_crds::documents; bound: documents=709`
- Exhaustive differential test: `crd_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_k8s_crd --no-tests=fail -E 'test(=crd_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_crds_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_k8s_crd --no-tests=fail -E 'test(=randomized_crds_read_and_write_as_the_reference)'`
- Reference test: `the_canonical_crd_orders_fields_as_the_page_states; command: cargo nextest run -p panproto-reference --test kubernetes_crds --no-tests=fail -E 'test(=the_canonical_crd_orders_fields_as_the_page_states)'`
- Reference test: `the_crd_document_scope_has_its_closed_form_count; command: cargo nextest run -p panproto-reference --test kubernetes_crds --no-tests=fail -E 'test(=the_crd_document_scope_has_its_closed_form_count)'`
- File: `crates/panproto-io/src/config/k8s_crd.rs`
- File: `crates/panproto-reference/src/pages/kubernetes_crds.rs`
- File: `crates/panproto-reference/src/enumerate/kubernetes_crds.rs`
