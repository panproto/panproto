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

## Get (restrict)

`get` applies the restrict functor $M^*$ and returns both the projected view and the complement $C$:

```python
view, complement = compiled.get(instance)
```

The complement is a dict summarizing dropped nodes and arcs. `CompiledMigration` does not expose a `put` method directly; for the reverse direction either compile the inverse migration (`invert_migration` below) or generate a full bidirectional `Lens` (`auto_generate_lens` produces a `Lens` whose `.put(view, complement)` reconstructs the source instance).

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

## Finding a morphism, and finding a span

`find_best_morphism` searches for a *total* morphism $S \to T$: every source
vertex must have an image. It returns `None` when there is none, which on real
schema pairs is the common case rather than the exceptional one, since dropping
a single field is enough to make one impossible.

```python
found = panproto.find_best_morphism(src, tgt)
if found is not None:
    print(found.quality, found.vertex_map, found.edge_map)
```

`find_span` asks the weaker question that always has an answer. It returns a
span $S \leftarrow A \rightarrow T$ whose apex $A$ is the sub-schema of $S$ the
search could place in $T$, so "these two schemas share nothing" comes back as an
empty apex rather than as `None`:

```python
span = panproto.find_span(src, tgt, protocol)

print(span.apex.vertex_count())   # how much of the source was covered
print(span.apex_coverage)         # the same number as a fraction of |src|
print(span.quality)               # how well the covered part matches
print(span.quality_bounds)        # (lower, upper) bracketing `quality`
print(span.proven_optimal)        # whether the search ruled out anything better
```

The protocol is a parameter because the apex is a schema, and a schema is only
well formed against a protocol: inducing the apex re-validates it rather than
assuming it.

A total morphism is the degenerate span, the one whose apex is the whole
source. `is_total` tests for it and `as_total_morphism()` returns the
`FoundMorphism` shape:

```python
if span.is_total:
    found = span.as_total_morphism()
```

Read `quality` and `apex_coverage` together. `quality` measures how well the
covered part matches and says nothing about how much was covered, so a span
covering one vertex perfectly scores higher than one covering nine vertices
well. `quality` is also comparable only among spans out of the *same* source
schema: every denominator of the objective is fixed by $S$, so two spans over
different sources are measured on different scales.

`quality_bounds` collapses to a point exactly when `proven_optimal` holds. When
it does not, the interval is what separates "0.4, and nothing better exists"
from "0.4, and the search ran out of budget before it could rule out 0.9".
