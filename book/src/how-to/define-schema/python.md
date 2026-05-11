# Define a schema from Python

## Prerequisites

`panproto` installed ([Install the Python SDK](../install/python.md)).

## The task

```python
import panproto

proto = panproto.get_builtin_protocol("json-schema")

b = proto.schema()
b.vertex("user", "object")
b.vertex("user.name", "string")
b.vertex("user.age", "integer")
b.edge("user", "user.name", "prop", "name")
b.edge("user", "user.age", "prop", "age")
schema = b.build()
```

`panproto.get_builtin_protocol(name)` returns the named protocol; `.vertex(id, kind)` and `.edge(src, tgt, kind, name=None)` each mutate the `SchemaBuilder` in place (returning `None`), and `.build()` validates and returns a `Schema`. The TypeScript SDK exposes the same operations as a chainable surface; the Python binding does not.

## Verification

```python
schema.validate(proto)
```

Raises `panproto.SchemaValidationError` on failure. The error carries the offending equation and location.

## Common mistakes

- Chaining the builder calls. The Python `SchemaBuilder.vertex(...)` / `edge(...)` / `constraint(...)` methods mutate in place and return `None`; hold the builder in a variable and mutate it statement-by-statement, then call `.build()`.
- Using a Python dict where the SDK expects a `Schema` handle. Conversion is deliberate; to materialise an `Instance` from bytes against a built `Schema`, use `panproto.IoRegistry().parse(protocol, schema, data)`.

## See also

- [Reference: Python SDK](../../reference/sdk-python.md).
- [Build a migration](../build-migration.md).
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
