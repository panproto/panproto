# Migrations

A migration $M: S \to T$ is a schema morphism mapping vertices, edges, and hyper-edges from a source schema $S$ to a target schema $T$. Compilation precomputes the restrict functor $M^*: \mathbf{Set}^T \to \mathbf{Set}^S$, which transforms instance data by:

1. Computing the surviving vertex set (image of the vertex map)
2. Computing the surviving edge set (image of the edge map)
3. Building vertex and edge remap tables
4. Copying resolver entries for ancestor contraction

## Building a migration

```python
import panproto

mb = panproto.MigrationBuilder()
mb.map_vertex("users", "users")
mb.map_vertex("users.id", "users.id")
mb.map_vertex("users.name", "users.name")
migration = mb.build()
```

For contraction ambiguity (when intermediate vertices are dropped and the resulting edge is ambiguous), add resolvers:

```python
mb.resolve("users", "users.id", "users", "users.id", "prop", "id")
```

## Compilation

```python
compiled = panproto.compile_migration(migration, src_schema, tgt_schema)
```

Compilation calls `check_existence` internally. If the migration references sorts or edges not present in either schema, it raises `MigrationError`.

## Lifting instances

The `lift` operation applies $M_!(X)$ (left Kan extension):

```python
lifted = compiled.lift(instance)
```

## Get/Put (lens interface)

`get` applies the restrict functor $M^*$ and returns both the projected view and the complement $C$:

```python
view, complement = compiled.get(instance)
```

The complement is a dict summarizing dropped nodes and arcs. It is needed by `put` to reconstruct the original.

## Existence checking

`check_existence` verifies that the migration is well-defined: all referenced sorts exist in both schemas, edge maps are consistent with vertex maps, and protocol-specific constraints (hyper-edge coherence, reachability) are satisfied.

```python
report = panproto.check_existence(migration, protocol, src_schema, tgt_schema)
```

The report is a dict with `errors` (list of structured error objects) and `valid` (bool).

## Composition and inversion

```python
composed = panproto.compose_migrations(m1, m2)
inverted = panproto.invert_migration(migration, src_schema, tgt_schema)
```

`compose` concatenates vertex maps: if $m_1$ maps $A \to B$ and $m_2$ maps $B \to C$, the result maps $A \to C$. `invert` requires bijectivity; it raises `MigrationError` if the vertex map is not injective or surjective.

## Coverage checking

```python
report = panproto.check_coverage(compiled, instances, src_schema, tgt_schema)
```

Runs each instance through `lift` and counts successes/failures. The report contains `total_records`, `successful`, `failed` (with per-record failure reasons), and `coverage_ratio`.
