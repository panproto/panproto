# Define a schema from Python

## Prerequisites

`panproto` installed ([Install the Python SDK](../install/python.md)).

## The task

```python
import panproto

p = panproto.Panproto()
proto = p.protocol("json-schema")

schema = (proto.schema()
    .vertex("user", "object")
    .vertex("user.name", "string")
    .vertex("user.age", "integer")
    .edge("user", "user.name", "prop", name="name", required=True)
    .edge("user", "user.age", "prop", name="age", required=False)
    .build())
```

The fluent surface mirrors the TypeScript SDK. `.vertex()` and `.edge()` build up a `SchemaBuilder`; `.build()` validates and returns a `Schema`.

## Verification

```python
schema.validate()
```

Raises `panproto.ValidationError` on failure. Catch it to inspect `.equation` and `.location`.

## Common mistakes

- Passing edge metadata as positional arguments instead of keyword arguments. The Python binding requires keyword arguments for everything beyond the source, target, and edge kind.
- Using a Python dict where the SDK expects a `Schema` handle. Conversion is deliberate; `panproto.parse(json_dict, protocol="json-schema")` is the explicit bridge.

## See also

- [Reference: Python SDK](../../reference/sdk-python.md).
- [Build a migration](../build-migration.md).
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
