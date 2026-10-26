# Query instances

The query engine selects nodes by schema anchor, follows edge kinds, filters with an expression, and returns selected fields. Rust and TypeScript expose the same operation.

## Prerequisites

A `Schema` and a `WInstance` loaded against it. Predicates use the [expression language](../reference/expression-language.md).

## Filter and project

`panproto_inst::InstanceQuery` uses the field names `anchor`, `predicate`, `group_by`, `project`, `limit`, and `path`. `execute_query` takes the query first, the instance second, and the schema third.

```rust,no_run
use panproto_core::gat::Name;
use panproto_core::inst::{InstanceQuery, QueryError, QueryMatch, WInstance, execute_query};
use panproto_core::schema::Schema;

fn titles(schema: &Schema, instance: &WInstance) -> Result<Vec<QueryMatch>, QueryError> {
    let query = InstanceQuery {
        anchor: Name::from("post"),
        project: Some(vec!["id".into(), "title".into()]),
        limit: Some(50),
        ..InstanceQuery::default()
    };

    execute_query(&query, instance, schema)
}
```

The anchor selects matching instance nodes. `project` limits each result's `fields` map. It does not change the matched node's `value`, identifier, or anchor.

## Follow edges

`path` contains edge kinds, not target vertex names or property labels. The executor first selects nodes matching `anchor`, then follows each edge kind in order:

```rust,no_run
# use panproto_core::gat::Name;
# use panproto_core::inst::{InstanceQuery, QueryError, WInstance, execute_query};
# use panproto_core::schema::Schema;
# fn run(schema: &Schema, instance: &WInstance) -> Result<(), QueryError> {
let query = InstanceQuery {
    anchor: Name::from("user"),
    path: vec![Name::from("authored")],
    project: Some(vec!["title".into()]),
    ..InstanceQuery::default()
};
let posts = execute_query(&query, instance, schema)?;
# let _ = posts;
# Ok(())
# }
```

Predicates are `panproto_expr::Expr` values. The evaluator binds a node's extra fields, scalar child values reached by labeled edges, and the metadata variables `_anchor`, `_id`, `_value`, and `_children_count`. Instance-aware builtins such as `Edge`, `Edges`, `Children`, `HasEdge`, and `EdgeCount` receive the current instance and node. `Edge` follows the first matching edge, the least edge id on a graph instance, and `Edges` lists every matching edge in order, so parallel edges each contribute their target. Candidates are taken in ascending node id before the predicate and `limit` apply, so a limited query returns the same nodes in every process.

## TypeScript boundary

`@panproto/core` exports `executeQuery(query, instance, wasm)`. The wrapper converts the public `groupBy`, `projection`, and `nodeId` names to and from Rust's `group_by`, `project`, and `node_id` wire fields. It also supplies the schema handle retained by the `Instance`, so callers do not encode the schema separately.

The WASM module keeps `execute_query(queryBytes, instanceBytes, schemaBytes)` for direct byte-oriented callers. SDK code uses `execute_query_with_schema_handle` to avoid serializing a schema that is already in the WASM resource table.

## Verification and limits

`execute_query` returns `Result<Vec<QueryMatch>, QueryError>`. Before the predicate is evaluated at a node, the executor refuses it, in ascending node id, when it reads a child name that two or more of the node's children carry (`QueryError::AmbiguousChildField`), a structural child whose subgraph is not a tree (`QueryError::StructuralChildNotATree`), or a value that no expression literal can hold (`QueryError::ValueNotExpressible`). Any other evaluation that fails or does not produce `true` excludes that node. The executor accepts a schema argument but does not use it to reject an anchor absent from the schema. The byte-oriented WASM entry point rejects missing or malformed schema bytes, while the handle entry point rejects an invalid or non-schema handle. Neither entry point type-checks the query against the schema after resolving it.

Expression evaluation uses `EvalConfig::default()`. `InstanceQuery` exposes no budget field, and the query functions expose no configuration argument, so a caller cannot raise that budget at the query call site.

## See also

- [Reference: expression language](../reference/expression-language.md).
- [Convert data between formats](./convert-data.md).
