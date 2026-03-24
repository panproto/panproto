# Schemas

A schema is a finite presentation of a $\mathbf{C}$-set: vertices are generators of sorts, edges are morphisms between sorts, and constraints are equations in the theory. Every schema belongs to a protocol, which specifies the GAT signature $\text{Th}_P = (\mathcal{S}, \mathcal{O}, \mathcal{E})$ the schema must conform to.

## Protocols

A protocol defines:

- **Sorts** $\mathcal{S}$: the vertex kinds (e.g., `table`, `integer`, `string` for SQL)
- **Operations** $\mathcal{O}$: the edge kinds (e.g., `prop`, `foreign-key`) with source/target kind constraints
- **Equations** $\mathcal{E}$: structural laws (e.g., constraint sorts like `primary_key`, `maxLength`)

```python
import panproto

sql = panproto.get_builtin_protocol("sql")
print(sql.name)           # "sql"
print(sql.obj_kinds)      # ["table", "integer", "string", ...]
print(sql.edge_rules)     # [{edge_kind: "prop", src_kinds: ["table"], ...}, ...]
```

76 protocols are built in. List them with:

```python
panproto.list_builtin_protocols()
```

## Building a schema

`Protocol.schema()` returns a mutable `SchemaBuilder`. Call `vertex()`, `edge()`, `hyper_edge()`, and `constraint()` to add elements, then `build()` to produce a validated `Schema`.

```python
builder = sql.schema()
builder.vertex("users", "table")
builder.vertex("users.id", "integer")
builder.vertex("users.name", "string")
builder.edge("users", "users.id", "prop", "id")
builder.edge("users", "users.name", "prop", "name")
builder.constraint("users.id", "primary_key", "")
schema = builder.build()
```

Each call validates against the protocol's edge rules. Adding a `prop` edge from `integer` to `table` when the protocol restricts `prop` sources to `table` raises `SchemaValidationError`.

## Schema properties

```python
schema.protocol       # "sql"
schema.vertex_count   # 3
schema.edge_count     # 2
schema.vertices       # [Vertex(id="users", kind="table"), ...]
schema.edges          # [Edge("users" -> "users.id", kind="prop", name="id"), ...]
```

Look up individual elements:

```python
v = schema.vertex("users.id")    # Vertex or None
out = schema.outgoing_edges("users")
inc = schema.incoming_edges("users.id")
cs = schema.constraints_for("users.id")
```

## Normalization and validation

```python
normalized = schema.normalize()         # collapse ref-chains
issues = schema.validate(sql)           # list[str], empty if valid
```

## Serialization

```python
json_str = schema.to_json()             # JSON string
schema2 = panproto.Schema.from_json(json_str)   # round-trip
d = schema.to_dict()                    # Python dict (via serde)
```

## Custom protocols

```python
custom = panproto.define_protocol({
    "name": "custom",
    "schema_theory": "ThGraph",
    "instance_theory": "ThWType",
    "edge_rules": [
        {"edge_kind": "link", "src_kinds": ["node"], "tgt_kinds": ["node"]},
    ],
    "obj_kinds": ["node"],
    "constraint_sorts": [],
})
```
