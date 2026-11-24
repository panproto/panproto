# CloudFormation

An AWS CloudFormation template describes a stack: the parameters a user supplies, the resources AWS creates, the conditions under which it creates them, and the values it exports. Its fields refer to one another by name: a resource property reads a parameter with `Ref`, an output reads a resource's attribute with `Fn::GetAtt`, and a resource waits on another through `DependsOn`. Reading a template into a panproto schema makes those references edges of a graph, so a migration or a diff can see that renaming a parameter breaks the three resources that read it. This page states which templates panproto reads, how a template becomes a schema, what the reader refuses, what the writer prints, and the laws the two satisfy.

## Standard and shapes

The reader follows the [template anatomy](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/template-anatomy.html) of AWS CloudFormation, template format version `2010-09-09` (the only version the format defines), with its [intrinsic functions](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/intrinsic-function-reference.html) and [pseudo parameters](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/pseudo-parameter-reference.html). A template's syntax is read as a tree and its meaning as a graph: the `cloudformation` protocol declares a tree syntactic and a graph semantic shape. The reader is [`read_template`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/fn.read_template.html) in `panproto-io` (feature `tree-sitter`). It reads no JSON or YAML itself: a template whose first byte other than whitespace is `{` goes through the JSON route of the [text formats](text-formats.md) (RFC 8259), any other through the YAML route (YAML 1.2.2, one document, the core schema), and every refusal of the route (a repeated key, several documents, an alias, text that is not UTF-8) is the reader's refusal too. `panproto-protocols` does not parse templates, and its schema dispatches refuse `cloudformation` and name this reader.

| Supported | Read as |
|---|---|
| `AWSTemplateFormatVersion: 2010-09-09`, `Description`, `Metadata` | Constraints of the template vertex |
| `Parameters` with their eleven fields (`Type` one of `String`, `Number`, `List<Number>`, `CommaDelimitedList`, an `AWS::` type or a list of one) | A `parameter` vertex each, its fields kept as constraints |
| `Mappings` (two levels of keys above scalars and sequences) | A `mapping` vertex each, the map kept as its `Map` constraint |
| `Conditions` (`Fn::And`, `Fn::Equals`, `Fn::If`, `Fn::Not`, `Fn::Or`, `Condition`) | A `condition` vertex each, the definition kept as its `Definition` constraint |
| `Resources` with `Type`, `Properties`, `Metadata`, `CreationPolicy`, `DeletionPolicy`, `UpdatePolicy`, `UpdateReplacePolicy`, `Condition`, `DependsOn` | A `resource` vertex each; `Condition` and `DependsOn` as edges, the other fields as constraints |
| `Outputs` with `Value`, `Description`, `Export`, `Condition` | An `output` vertex each; `Condition` as an edge, the other fields as constraints |
| `Ref`, `Fn::GetAtt`, `Fn::Sub`, `Fn::FindInMap`, `Fn::If`, `Fn::ImportValue`, `Fn::Base64`, `Fn::Cidr`, `Fn::GetAZs`, `Fn::Join`, `Fn::Select`, `Fn::Split` and the condition functions, in full form or as a scalar short form (`!Ref x`, `!GetAtt a.b`, `!Sub text`) | Kept in the constraint text, in full form; their references resolved to edges |

| Not supported | Refusal | Reason |
|---|---|---|
| Format versions other than `2010-09-09` | `FormatVersion` | The format defines no other |
| `Transform` (the `AWS::Serverless` and `AWS::LanguageExtensions` transforms among them) and `Fn::Transform` | `Unsupported` | A transform or macro is code AWS runs on the template before reading it |
| `Fn::Length`, `Fn::ToJsonString`, `Fn::ForEach` | `Unsupported` | They exist only under the `AWS::LanguageExtensions` transform |
| Module resource types (`Org::Service::Resource::MODULE`) | `Unsupported` | A module expands into resources outside the template |
| The `Rules` section | `Unsupported` | Its functions (`Fn::ValueOf` and the rest) read parameter attributes AWS resolves when a stack is created |
| A short-form tag on a sequence or mapping (`!Join [..]`, `!If [..]`, `!Base64 {..}`) | `TaggedCollection` | The shared YAML route carries a tag on a scalar only; the full form (`Fn::Join:`) reads |
| A computed attribute name in `Fn::GetAtt` | `Arguments` | The attribute is an edge's name, which must be known when the template is read |
| A scalar JSON cannot write (a `!!binary` scalar, a non-finite float) | `Unsupported` | No constraint text carries it exactly |

## The template schema

A template reads as one `template` vertex `Template` joined by a `declares` edge, named by the logical ID, to a vertex `<Section>.<name>` for every declaration: a `parameter` (`Parameters.Env`), `mapping`, `condition`, `resource` or `output`. Every field is kept, never interpreted away: the template's three header fields, each parameter field, a mapping's map, a condition's definition, a resource's type, properties, metadata and policies, and an output's value, description and export are constraints of their vertex whose value is the field's value as canonical JSON text (no whitespace, mapping entries in source order, integers and exact decimals as written, a float in its shortest round-tripping form). A scalar short form is kept in full form: `!Ref Env` is the text `{"Ref":"Env"}`, `!GetAtt Bucket.Arn` is `{"Fn::GetAtt":"Bucket.Arn"}`. Validation keeps every one of these constraints as an annotation: it does not evaluate a condition or check a property against the resource type's specification, which the template does not carry.

The references are resolved, depth first in source order, into edges from the declaration that holds them, each added once:

- `Ref` names a pseudo parameter (an edge `ref` to a `pseudo-parameter` vertex `Pseudo.AWS::Region`, made on first use), a parameter or a resource;
- `Fn::GetAtt` (a sequence of two strings, or one string split at its first dot) names a resource, and the edge `get-att` is named by the attribute;
- a variable `${x}` of an `Fn::Sub` string is a `ref` edge as `Ref` is, `${a.b}` is a `get-att` edge to `a` named `b`, `${!x}` is a literal, and a variable the substitution's own mapping defines resolves to that mapping;
- `Fn::FindInMap` whose first item is a string names a mapping (`find-in-map`);
- `Fn::If`, and a `Condition` function inside a condition, name a condition (`uses-condition`);
- a resource's or output's `Condition` names a condition (`condition`), and a resource's `DependsOn`, a string or a sequence of them, names resources (`depends-on`).

A name that resolves to nothing is refused (`Unresolved`), as CloudFormation refuses it when a stack is created; inside a condition a `Ref` may name only a parameter or pseudo parameter, and `Fn::GetAtt` is refused. `Fn::ImportValue` names an export of another stack, which no template resolves alone: the reader walks its argument and lists it, with its path and the export name when the name is a string, in the reading's [`imports`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/struct.CfnReading.html), in document order.

## What the reader refuses

Every refusal is a [`CfnError`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/struct.CfnError.html) carrying the path of the field at fault (keys joined by `.`, sequence indices as `[i]`), the byte offset at which its value starts, and a [`CfnErrorKind`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/enum.CfnErrorKind.html). The reader returns the first fault in this order. First the route (`Syntax`, or `TaggedCollection` for a tag on a sequence or mapping), then every locally tagged scalar that is not a short form (`UnknownTag`). Then the top level: its keys against the ten sections (`UnknownField`), `AWSTemplateFormatVersion` (`FormatVersion`), `Transform`, `Rules`, `Description` (a string), `Metadata` (a mapping), and `Resources` (`Missing`). Then the declaring sections in the order Parameters, Mappings, Conditions, Resources, Outputs: each a mapping whose names are alphanumeric logical IDs (condition names excepted, which the format does not restrict); then a resource whose logical ID is also a parameter's (`DuplicateName`); then at least one resource. Then each declaration, section by section in source order:

- a parameter: its keys, its `Type` (`Missing`, or `Invalid` for a type the format does not define), then each field in source order (`AllowedValues` a sequence; `Description`, `AllowedPattern` and `ConstraintDescription` strings);
- a mapping: each value two keys deep must not be a mapping;
- a condition: a mapping with one key, a condition function (`Invalid` otherwise), then its references;
- a resource: its keys, its `Type` (`Custom::<name>`, or three non-empty segments joined by `::`; a fourth segment `MODULE` is `Unsupported`), its `Condition`, its `DependsOn`, then each other field in source order (a `DeletionPolicy` of `Delete`, `Retain`, `RetainExceptOnCreate` or `Snapshot`, an `UpdateReplacePolicy` of `Delete`, `Retain` or `Snapshot`, and `Properties`, `Metadata`, `CreationPolicy` and `UpdatePolicy` mappings whose references are then resolved);
- an output: its keys, its `Value` (`Missing`), its `Condition`, then each field in source order (`Description` a string, `Export` a mapping with exactly `Name`; the references of `Value` and `Export`).

Within a value, a one-key mapping is an intrinsic function when its key is `Ref`, `Condition` (inside a condition only) or begins with `Fn::`. `Fn::Transform` and the language extension functions are `Unsupported`, any other `Fn::` key not in the supported table is `UnknownFunction`, and a function whose argument does not have its form (the item counts of `Fn::And` and `Fn::Or` two to ten, `Fn::Not` one, `Fn::Equals`, `Fn::Join`, `Fn::Select` and `Fn::Split` two, `Fn::Cidr`, `Fn::FindInMap` and `Fn::If` three; `Fn::If` naming its condition by a string; `Fn::Sub` a string or a string and a mapping) is `Arguments`. A field of the wrong kind is `WrongType`; any other broken rule is `Invalid`.

The reader does not check what CloudFormation checks only against AWS: a resource's properties against its type's specification, an attribute name against the attributes the type has, the quotas on the number of declarations, and a circular dependency among resources, which CloudFormation refuses when it orders a stack's creation.

## Canonical output

[`write_template`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/fn.write_template.html) writes a `cloudformation` schema as the canonical template: one line of JSON with no whitespace between tokens, ended by a line feed. It holds `AWSTemplateFormatVersion`, `Description` and `Metadata` when the schema keeps them, in that order, then the sections `Parameters`, `Mappings`, `Conditions`, `Resources` and `Outputs` that declare anything, each declaration in the order its `declares` edge was added. A parameter, resource or output writes its kept fields in source order, then a resource or output its `Condition`, then a resource its `DependsOn` as a sequence without repeats; a mapping writes its map and a condition its definition. The writer refuses, with a [`CfnWriteError`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/enum.CfnWriteError.html), a schema without a template vertex, a declaration without its kept map or definition, and a kept constraint that is not JSON. The reader claims no format preservation: the canonical text keeps every field and every reference but not the layout, comments, short forms or the order of the sections.

## Conformance corpus

The corpus under `crates/panproto-io/tests/fixtures/conformance/cloudformation` holds 17 templates vendored unchanged from the [aws-cloudformation-templates](https://github.com/aws-cloudformation/aws-cloudformation-templates) repository at commit `a0f43bc6` (Apache License 2.0; `UPSTREAM.md` names each file): the JSON and YAML forms of eight sample templates (SNS, RDS, DynamoDB, EC2 and CloudWatch) and one template that uses the `AWS::LanguageExtensions` transform, which must be refused at `Transform`. AWS ships no suite of invalid templates, so 20 refusal cases are built from the SNS topic template by breaking one rule each, and `MANIFEST` lists each case's rule, path and refusal (`conformance_cloudformation.rs`). Every other upstream template must read, and its canonical text must read back as the same schema and write as the same text. All cases pass except four YAML templates that use a short-form tag on a sequence or mapping, which `cloudformation-limits.txt` declares as the `TaggedCollection` limit; their JSON forms read.

## A template reads as the page defines it {#L-cloudformation-1}

### Statement

For a document $d$ of the bounded scope, write $R(d)$ for `read(d)`, $Y(d)$ for its block YAML text `render_yaml(d)` and $J(d)$ for its compact JSON text `json(d)`. When $R(d)$ is a reading, `read_template` reads both $Y(d)$ and $J(d)$ as a schema with exactly the vertices and kinds of $R(d)$, the out-edges of each vertex in the same order with the same kinds, targets and names, the same kept constraints in the same order with the same JSON texts, and the same imports in the same order. When $R(d)$ refuses at path $p$ with class $k$, `read_template` refuses $Y(d)$ and $J(d)$ at $p$ with class $k$.

### Outside the domain

The reference reads data trees of nulls, booleans, integers and strings with every function in full form, so short forms, floats and the scalars JSON cannot write are checked by the fixed tests and the corpus. A refusal's byte offset and the message of an `Invalid` refusal are not compared; the path and class are. Refusals of the routes are the subject of [Text formats](text-formats.md).

### Definitions used

$R$ applies the rules of the two sections above in their order. The scope is a base template (a parameter `P`, a mapping `M`, a condition `C` reading `P`, a resource `R` and a resource `S`) whose `S` holds no properties, one of the properties `a` and `b`, or both in either order, each one of 17 values (a string; a `Ref` to a parameter, a resource, a pseudo parameter and to nothing; `Fn::GetAtt` in both forms and to nothing; two substitutions; a map lookup; a conditional value; an import; a join with one argument; `Fn::Length`; an unknown function; and a conditional value naming no condition), together with 26 envelopes, the base with one property changed in one place outside the properties: $1 + 2 \cdot 17 + 2 \cdot 17^2 + 26 = 639$ documents.

### Proof

1. Production reads $Y(d)$ through the YAML route and $J(d)$ through the JSON route, each of which reads the tree $d$ with mapping entries in source order (block YAML reading is [L-text-formats-4](text-formats.md#L-text-formats-4); the differential checks these spellings).
2. Production checks the same conditions as $R$ in the same order: the top-level keys and header fields, the sections' names, then each declaration's fields in the order the section above states, and within a value the functions depth first in source order. The first condition that fails is therefore the same, at the same path, with the same class.
3. On a document both accept, both create the same vertices with the same ids, add each edge once when its reference is resolved, in the same order, and keep each field's JSON text in the order the fields are read, so the readings agree.
4. The exhaustive differential checks the agreement on all 639 documents through both routes, of which 269 are accepted (11 of the 17 values are accepted, so $1 + 2 \cdot 11 + 2 \cdot 11^2 = 265$ property documents, and 4 envelopes), and the property differential on generated property values nested three deep.

### Executable counterpart

- Law ID: `cloudformation.reading`
- Reference: `panproto_reference::pages::cloudformation::read`
- Reference: `panproto_reference::pages::cloudformation::render_yaml`
- Reference: `panproto_reference::pages::cloudformation::json`
- Enumerator: `panproto_reference::enumerate::cloudformation::documents; bound: documents=639`
- Exhaustive differential test: `template_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_cloudformation --no-tests=fail -E 'test(=template_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_templates_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_cloudformation --no-tests=fail -E 'test(=randomized_templates_read_and_write_as_the_reference)'`
- Reference test: `a_template_reads_as_its_declarations_and_their_references; command: cargo nextest run -p panproto-reference --test cloudformation --no-tests=fail -E 'test(=a_template_reads_as_its_declarations_and_their_references)'`
- Reference test: `template_refusals_name_the_field_and_the_rule; command: cargo nextest run -p panproto-reference --test cloudformation --no-tests=fail -E 'test(=template_refusals_name_the_field_and_the_rule)'`
- Reference test: `sub_variables_skip_literals_and_stop_at_an_unclosed_variable; command: cargo nextest run -p panproto-reference --test cloudformation --no-tests=fail -E 'test(=sub_variables_skip_literals_and_stop_at_an_unclosed_variable)'`
- Randomized input scope: two properties `a` (depth at most three) and `b` (depth at most two) built from strings with and without substitution variables, `Ref`, `Fn::GetAtt`, `Fn::Sub` and `Fn::FindInMap` naming any declaration or nothing, sequences and mappings of up to two entries, `Fn::If` and `Fn::Join`.
- File: `crates/panproto-io/src/config/cloudformation.rs`
- File: `crates/panproto-io/src/config/tree.rs`
- File: `crates/panproto-reference/src/pages/cloudformation.rs`

## The writer prints the canonical template, which reads back as the same schema {#L-cloudformation-2}

### Statement

For a document $d$ of the bounded scope that $R$ accepts, write $C(d)$ for `canonical_text(d)`. `write_template` writes the schema `read_template` reads from $Y(d)$, and from $J(d)$, as exactly $C(d)$, and `read_template` reads $C(d)$ as a schema with the same reading.

### Outside the domain

Schemas that no template reads as (no template vertex, a declaration without its kept map or definition, a constraint that is not JSON) are refused by the writer; the reference, which writes from documents, does not model them.

### Definitions used

$C(d)$ is the compact JSON of $d$ with its sections and attributes reordered as the canonical-output section states and repeated `DependsOn` entries removed, every string escaped as RFC 8259 requires with the short escapes for backspace, form feed, line feed, carriage return and tab and `\u00xx` in lowercase hexadecimal for the other control characters, followed by a line feed.

### Proof

1. Production writes each field from its kept JSON text, which by [L-cloudformation-1](#L-cloudformation-1) is the compact JSON of the field's value in $d$, writes `Condition` and `DependsOn` from the edges, which hold their names once each in source order, and orders the sections and declarations as $C$ does.
2. $C(d)$ begins with `{`, so the JSON route reads it as the tree of $d$ with its sections and attributes reordered. The reader adds a declaration's `condition` and `depends-on` edges before the references of its other fields whatever the order of its attributes, and reads the sections in a fixed order, so it reads $C(d)$ as the same reading.
3. The exhaustive differential checks both equalities on all 269 accepted documents through both routes, and the property differential on generated ones.

### Executable counterpart

- Law ID: `cloudformation.canonical-writing`
- Reference: `panproto_reference::pages::cloudformation::canonical_text`
- Enumerator: `panproto_reference::enumerate::cloudformation::documents; bound: documents=639`
- Exhaustive differential test: `template_reading_and_canonical_writing_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_cloudformation --no-tests=fail -E 'test(=template_reading_and_canonical_writing_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_templates_read_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_cloudformation --no-tests=fail -E 'test(=randomized_templates_read_and_write_as_the_reference)'`
- Reference test: `the_canonical_template_orders_sections_and_attributes_as_the_page_states; command: cargo nextest run -p panproto-reference --test cloudformation --no-tests=fail -E 'test(=the_canonical_template_orders_sections_and_attributes_as_the_page_states)'`
- Reference test: `the_template_document_scope_has_its_closed_form_count; command: cargo nextest run -p panproto-reference --test cloudformation --no-tests=fail -E 'test(=the_template_document_scope_has_its_closed_form_count)'`
- File: `crates/panproto-io/src/config/cloudformation.rs`
- File: `crates/panproto-reference/src/pages/cloudformation.rs`
- File: `crates/panproto-reference/src/enumerate/cloudformation.rs`
