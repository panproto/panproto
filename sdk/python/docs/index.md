# panproto Python SDK

Native Python bindings for panproto via PyO3. All computation runs in compiled Rust; there is no WASM layer, no MessagePack serialization overhead, and no wasmtime dependency.

## Install

```bash
pip install panproto
```

For development builds from source:

```bash
cd panproto/
maturin develop --manifest-path crates/panproto-py/Cargo.toml
```

## Minimal example

```python
import panproto

sql = panproto.get_builtin_protocol("sql")

# Build source schema
src = sql.schema()
src.vertex("users", "table")
src.vertex("users.id", "integer")
src.edge("users", "users.id", "prop", "id")
old = src.build()

# Build target schema (added a column)
tgt = sql.schema()
tgt.vertex("users", "table")
tgt.vertex("users.id", "integer")
tgt.vertex("users.email", "string")
tgt.edge("users", "users.id", "prop", "id")
tgt.edge("users", "users.email", "prop", "email")
new = tgt.build()

# Diff and classify
diff = panproto.diff_schemas(old, new)
report = diff.classify(sql)
print(report.compatible)       # True
print(report.report_text())    # human-readable summary
```

## What panproto does

panproto treats schemas as models of generalized algebraic theories (GATs). A protocol (SQL, Protobuf, GraphQL, ATProto, etc.) defines a GAT signature $\text{Th}_P = (\mathcal{S}, \mathcal{O}, \mathcal{E})$, and a schema is a model of that theory: a finite $\mathbf{C}$-set with vertices, edges, hyper-edges, and constraints.

Migrations between schemas are compiled into restrict functors $M^*: \mathbf{Set}^T \to \mathbf{Set}^S$ that transform instance data. The get/put interface forms an asymmetric lens with a complement $C$ capturing the data lost in the forward direction.

76 protocols are built in, covering annotation, API, config, data schema, data science, database, domain, serialization, type system, and web document formats.

## Modules

| Module | Purpose |
|--------|---------|
| [Schemas](schemas.md) | Protocol-aware schema construction |
| [Migrations](migrations.md) | Compiled migrations, lift, get/put |
| [Lenses](lenses.md) | Bidirectional transformations, law checking |
| [I/O](io.md) | Parse and emit instances across 76 protocols |
| [GAT](gat.md) | Theory operations: colimit, morphism, model |
| [VCS](vcs.md) | Schematic version control |
| [Expressions](expressions.md) | Pure functional expression language |
| [API Reference](api.md) | Auto-generated from docstrings |
