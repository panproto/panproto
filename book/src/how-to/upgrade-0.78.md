# Upgrade from 0.77 to 0.78

Release 0.78 brings formats under the [format contract](../reference/formats.md): a format is read against a pinned version of its standard, every value it carries maps exactly, and what the model cannot carry is refused by name. Several schema readers that did not meet that bar were removed from `panproto-protocols`, and their replacements live in `panproto-io`. A few lens operations that left data untouched now change it as their names say, and some public types gained or lost fields. This page lists every change that can break code written against 0.77, what to write instead, and what changes for the bindings and the `schema` CLI.

Code that matches on an error enum keeps compiling when the enum gains a variant, since the error enums are `#[non_exhaustive]`; such additions are not listed here. The [changelog](https://github.com/panproto/panproto/blob/main/CHANGELOG.md) lists every change.

## Schema readers that moved to `panproto-io`

In 0.77, `panproto-protocols` read OpenAPI, AsyncAPI, JSON Schema, Kubernetes CRDs, CloudFormation templates, Ansible files and BSON schemas with readers that dropped or coerced what they did not understand: an OpenAPI `type: ["string", "null"]` read as `object`, a JSON Schema `enum` was joined into one string, and every real CRD, CloudFormation template and Ansible playbook was refused for want of a private wrapper shape. Those readers are removed. Each format is now read in `panproto-io`, under its `tree-sitter` feature, through the shared JSON and YAML routes, and refuses what it cannot carry with a typed error. Both schema dispatches, [`parse_schema_document`](https://docs.rs/panproto-protocols/latest/panproto_protocols/fn.parse_schema_document.html) and [`parse_schema_source`](https://docs.rs/panproto-protocols/latest/panproto_protocols/fn.parse_schema_source.html), refuse these protocols with an error that names the new reader.

| Protocol | Removed from `panproto-protocols` | Read and written in `panproto-io` with |
|---|---|---|
| `openapi` | `api::openapi::parse_openapi`, `parse_openapi_bundle`, `emit_openapi` | [`api::openapi::read_openapi`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/fn.read_openapi.html), [`read_openapi_bundle`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/fn.read_openapi_bundle.html), [`write_openapi`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/fn.write_openapi.html) |
| `asyncapi` | `api::asyncapi::parse_asyncapi`, `emit_asyncapi` | [`api::asyncapi::read_asyncapi`](https://docs.rs/panproto-io/latest/panproto_io/api/asyncapi/fn.read_asyncapi.html), [`read_asyncapi_bundle`](https://docs.rs/panproto-io/latest/panproto_io/api/asyncapi/fn.read_asyncapi_bundle.html), [`write_asyncapi`](https://docs.rs/panproto-io/latest/panproto_io/api/asyncapi/fn.write_asyncapi.html) |
| `json-schema` | `data_schema::json_schema::parse_json_schema`, `parse_json_schema_bundle`, `emit_json_schema` | [`data_schema::json_schema::read_schema`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/fn.read_schema.html), [`read_bundle`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/fn.read_bundle.html), [`write_document`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/fn.write_document.html) |
| `k8s-crd` | `config::k8s_crd::parse_k8s_crd_schema`, `emit_k8s_crd_schema` | [`config::k8s_crd::read_crd`](https://docs.rs/panproto-io/latest/panproto_io/config/k8s_crd/fn.read_crd.html), [`write_crd`](https://docs.rs/panproto-io/latest/panproto_io/config/k8s_crd/fn.write_crd.html) |
| `cloudformation` | `config::cloudformation::parse_cfn_schema`, `emit_cfn_schema` | [`config::cloudformation::read_template`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/fn.read_template.html), [`write_template`](https://docs.rs/panproto-io/latest/panproto_io/config/cloudformation/fn.write_template.html) |
| `ansible` | `config::ansible::parse_ansible_schema`, `emit_ansible_schema` | [`config::ansible::read_ansible`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/fn.read_ansible.html), which takes the file's [`FileClass`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/enum.FileClass.html), and [`write_ansible`](https://docs.rs/panproto-io/latest/panproto_io/config/ansible/fn.write_ansible.html) |
| `bson` | `data_schema::bson::parse_bson_schema`, `emit_bson_schema` | The `bson` protocol is now the tree encoding of BSON documents; MongoDB `$jsonSchema` documents are read by the `mongodb` protocol |

Before, in 0.77:

```text
let schema = panproto_protocols::api::openapi::parse_openapi(&json_value)?;
```

After, the reader takes the document's bytes, JSON or YAML, and returns the schema with the vertices it is rooted at:

```rust
use panproto_core::io::api::openapi::read_openapi;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let description = br#"{"openapi": "3.1.0", "info": {"title": "Pets", "version": "1"}, "paths": {}}"#;
    let reading = read_openapi(description)?;
    assert_eq!(reading.schema.protocol, "openapi");
    Ok(())
}
```

A document of a version the reader does not read is refused rather than read as if it were 3.1, so code that fed Swagger 2.0 or OpenAPI 3.0 documents to `parse_openapi` now receives an [`OpenApiError`](https://docs.rs/panproto-io/latest/panproto_io/api/openapi/struct.OpenApiError.html) whose kind is `Swagger` or `Version`. The same holds for JSON Schema documents of drafts 04 to 2019-09 and for AsyncAPI 2 documents. The [supported formats](../reference/formats.md) page lists each refused variant with the refusal it draws.

The `bson` instance codec read JSON text in 0.77 and no BSON at all. It now decodes and encodes binary BSON 1.1, so bytes that were JSON must be read as [MongoDB Extended JSON](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.read_extended_json.html) instead. The `k8s-crd` and `ansible` instance codecs read through the YAML route instead of the JSON one under the `tree-sitter` feature.

## The `sql` protocol is the `postgresql` protocol

The protocol is renamed from `sql` to `postgresql`, with no alias, and its module from `database::sql` to [`database::postgresql`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/postgresql/index.html). Every lookup of `sql`, in Rust, in a binding or in a `panproto.toml`, now finds no protocol. The reader, [`postgresql::read_ddl`](https://docs.rs/panproto-io/latest/panproto_io/postgresql/fn.read_ddl.html) in `panproto-io`, reads PostgreSQL 17 through the PostgreSQL grammar and refuses what it does not read by name, where `parse_ddl` of 0.77 mapped an unknown column type to `string`, dropped `CHECK` constraints and referential actions, and skipped every statement it did not recognise. Unquoted names fold to lowercase as PostgreSQL folds them, an unresolved foreign key is refused, and the `PRIMARY KEY`, `UNIQUE` and `FOREIGN KEY` constraint sorts give way to the schema's key and reference declarations.

```rust
use panproto_core::io::postgresql::read_ddl;
use panproto_core::protocols::database::postgresql::OtherStatements;
use panproto_core::protocols::registry::descriptor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert!(descriptor("sql").is_none());
    let ddl = "CREATE TABLE Pets (id integer PRIMARY KEY, name text NOT NULL);";
    let reading = read_ddl(ddl, OtherStatements::Refuse)?;
    assert_eq!(reading.schema.protocol, "postgresql");
    assert!(reading.not_read.is_empty());
    Ok(())
}
```

A script that also holds views, functions or grants is refused statement by statement with [`OtherStatements::Refuse`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/postgresql/enum.OtherStatements.html); with `OtherStatements::List` those statements are listed in `not_read` and the tables are read.

## A stopped morphism search is an error

[`find_morphisms`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_morphisms.html) and [`find_best_morphism`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_best_morphism.html) promise that every morphism they return is optimal. In 0.77 a search that a node or operation budget stopped after it had reached one total morphism returned that morphism as `Ok`, although nothing proved it optimal. Every stopped search now returns [`SpanError::Stopped`](https://docs.rs/panproto-mig/latest/panproto_mig/error/enum.SpanError.html#variant.Stopped), and the variant gained the field `incumbent`, the total morphism the search held when it stopped, if any. A pattern that named only `limit` no longer compiles; name `incumbent` or end the pattern with `..`. A caller that wants the best morphism found so far, proved or not, takes it from the refusal:

```rust
use panproto_core::mig::SpanError;
use panproto_core::mig::hom_search::{FoundMorphism, SearchOptions, find_morphisms};
use panproto_core::schema::Schema;

/// The first optimal morphism, or the one a stopped search held.
fn best_found(src: &Schema, tgt: &Schema) -> Result<Option<FoundMorphism>, SpanError> {
    match find_morphisms(src, tgt, &SearchOptions::default()) {
        Ok(list) => Ok(list.morphisms.into_iter().next()),
        Err(SpanError::Stopped { incumbent, .. }) => Ok(incumbent.map(|found| *found)),
        Err(other) => Err(other),
    }
}

fn main() {
    let _ = best_found;
}
```

Every binding reports the stop as an error of the search: Python raises `MigrationError`, the C entry points return `PpStatus::Operation`, and the TypeScript, Swift and Haskell searches throw or return their migration error.

## Lens operations that now change the data

Three elementary protolenses recorded a change in the schema and left the data as it was. They now do what the schema change says, so a lens built from one of them gives a different view than in 0.77.

- **`merge_sorts`** ([`elementary::merge_sorts`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/elementary/fn.merge_sorts.html), and the lens DSL step) removed every vertex of both sorts and added an empty vertex, so get moved all their data into the complement. It now renames both sorts to the merged one: vertex ids, edges and data are kept, and the complement is empty.
- **`coerce_sort`** ([`elementary::sort_coerce`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/elementary/fn.sort_coerce.html), and the lens DSL step) left every value of the coerced kind as it was. Get now applies the forward expression to each value of that kind, put takes an edited value back by the inverse, and an edit under a coercion with no inverse is refused with `TransformNotInvertible`.
- **Directed equations** ([`elementary::directed_eq`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/elementary/fn.directed_eq.html), which the lens DSL's `directed_equations` compile to) left the data as it was. An equation whose left side is an application `o(...)` now rewrites the field `o` by its implementation, with the field's value bound to `x`, and takes an edit back by its inverse.

Other lens changes that can surface as new refusals: renaming an operation onto a name that would send two distinct edges to one edge is refused with `LensError::ProtolensError`, and tree put refuses a view node at a target vertex that no surviving source vertex reaches with `AmbiguousReverseAnchor`, as graph put already did. [`Protolens::is_lossless`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/struct.Protolens.html#method.is_lossless) is no longer a `const fn`, so a `const` that called it must become a `let` or a `static` initialised at run time.

## Lens DSL and theory DSL documents

Each lens DSL member now has one spelling. A document written for 0.77 needs these renames:

| Where | 0.77 | 0.78 |
|---|---|---|
| `add_field`, `add_sort` | `fallback` | `default` (in Nickel, the quoted `"default"`) |
| a rule | `match` | `pattern`, which requires `name` |
| a directed equation | `impl_term` | `impl` |
| a rule pattern | `type_id` | removed; no compiler read it |
| `from_diff` | `rename_edges` | removed; it did nothing |

Before:

```text
{ "id": "post.v1-to-v2", "source": "post.v1", "target": "post.v2", "steps": [{ "add_field": { "name": "status", "kind": "string", "fallback": "draft" } }] }
```

After:

```text
{ "id": "post.v1-to-v2", "source": "post.v1", "target": "post.v2", "steps": [{ "add_field": { "name": "status", "kind": "string", "default": "draft" } }] }
```

Every document is checked against a closed grammar before it compiles, so a member the grammar does not name, such as a leftover `fallback`, is refused with `LensDslError::Shape` at its path rather than ignored. The lens DSL error variants `ExprParse`, `TermParse`, `DefaultValue`, `RuleCompile` and `CoercionNotHonest` lost their `step_desc` or `index` field, since a refusal is located by its path; `NoBody`, `MultipleBodies` and `UnknownStep` became `Shape`. In the theory DSL, `TheoryDslError::TypeCheck` and `MorphismCheck` carry the [`GatError`](https://docs.rs/panproto-gat/latest/panproto_gat/enum.GatError.html) instead of its message, and `Spanned` replaces `TypeCheckSpanned`.

## Schema version control

- [`Repository::init`](https://docs.rs/panproto-vcs/latest/panproto_vcs/repo/struct.Repository.html#method.init) refuses a directory that already holds a repository with [`VcsError::AlreadyARepository`](https://docs.rs/panproto-vcs/latest/panproto_vcs/error/enum.VcsError.html#variant.AlreadyARepository); in 0.77 it reset `HEAD` to `main`. The Python binding's `Repository.init` raises accordingly, and `schema init` reports the error.
- A repository records its format in `.panproto/format`. A repository written by 0.77 has no marker; 0.78 reads it as format 1 and writes the marker on its first write, so no step is needed. A marker naming another format is refused with `VcsError::UnsupportedRepositoryFormat`, which is what an older release would need to say about a newer repository.

## The bindings and the `schema` CLI

The bindings call the same Rust functions, so each change above reaches them. The ones a binding user meets are these.

- **Protocol names.** `sql` resolves to nothing in every binding (`get_builtin_protocol("sql")` in Python, `protocol('sql')` in TypeScript, `ProtocolHandle.builtin("sql")` in Swift); use `postgresql`. The TypeScript `PROTOCOL_CATEGORIES` map now lists canonical hyphenated names (`k8s-crd`, `iso-space`, `msgpack-schema`) and the `edgelist`, `rdf` and `text` categories, matching what `IoRegistry.protocols` reports.
- **Schema documents.** Python's `parse_schema_document` and `parse_schema_source`, TypeScript's `parseSchemaDocument` and `parseSchemaSource`, and the `schema` CLI with `--protocol` refuse `openapi`, `asyncapi`, `json-schema`, `k8s-crd`, `cloudformation`, `ansible`, `postgresql` and `bson`, with an error that names the `panproto-io` reader. These readers are reachable from Rust in 0.78; the bindings and the CLI gain routes to them in a later release. A schema written by one of them can be passed to any binding as panproto schema JSON.
- **Morphism search.** A search a budget stopped raises or returns an error, as described above, instead of returning an unproved morphism.
- **Lens DSL documents** loaded through a binding need the member renames above.
- **Repositories.** Python's `Repository.init` raises `VcsError` on an existing repository.

The C ABI, and so the Swift and Haskell packages, keeps every entry point of 0.77 with the same signature.
