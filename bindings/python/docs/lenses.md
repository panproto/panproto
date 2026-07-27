# Lenses

An asymmetric lens is a pair $(get, put)$ where:

$$get: S \to V$$
$$put: V \times C \to S$$

$S$ is the source instance, $V$ is the view (projected instance), and $C$ is the complement (data discarded by $get$, needed by $put$ to reconstruct $S$).

## Lens laws

Two round-trip properties must hold:

**GetPut**: $put(get(s), c(s)) = s$ for all source instances $s$, where $c(s)$ is the complement produced by $get(s)$.

**PutGet**: $get(put(v, c)) = v$ for all views $v$ and complements $c$.

GetPut says: if you project and immediately reconstruct without modifying the view, you get back the original. PutGet says: if you reconstruct and then project again, you get back the same view.

## Auto-generating a lens

Given source and target schemas, panproto finds the best morphism alignment, factorizes it into elementary protolens steps, and instantiates the chain into a concrete lens:

```python
import panproto

lens, quality, coerce_proposals = panproto.auto_generate_lens(src_schema, tgt_schema, protocol)
print(quality)            # alignment quality score, 0.0 to 1.0
print(coerce_proposals)   # empty below the `exploratory` stringency tier
```

Pass `stringency="exploratory"` to surface sort-coercion proposals (each a dict with `src`, `tgt`, `witness_name`, `witness_class`, `confidence`, `explanation`). The default tier is `balanced`, which returns an empty `coerce_proposals` list.

## Get and put

```python
view, complement = lens.get(instance)
restored = lens.put(view, complement)
```

The `Complement` object stores:

- `dropped_nodes`: nodes from the source that do not appear in the view
- `dropped_arcs`: arcs from the source that do not appear in the view
- Contraction choices made during ancestor contraction
- Original parent mappings before contraction

## Law checking

```python
lens.check_laws(instance)       # raises LensError if either law fails
lens.check_get_put(instance)    # check only GetPut
lens.check_put_get(instance)    # check only PutGet
```

Each method raises `LensError` with a description of the violation if the law does not hold.

## Composition

```python
composed = lens1.compose(lens2)
```

The composed lens applies `lens1` first, then `lens2`. The target schema of `lens1` must match the source schema of `lens2`.

## Loading a chain from a lens-DSL document

`ProtolensChain` accepts lens-DSL documents (Nickel, JSON, or YAML) and compiles them to a chain anchored at a named body vertex of the source schema:

```python
import panproto

# From a file (dispatches on extension):
chain = panproto.ProtolensChain.from_dsl_path(
    "lenses/user-v1-to-v2.ncl",
    body_vertex="record:body",
)

# From source text:
chain = panproto.ProtolensChain.from_dsl_json(json_source, "record:body")
chain = panproto.ProtolensChain.from_dsl_yaml(yaml_source, "record:body")
chain = panproto.ProtolensChain.from_dsl_nickel(nickel_source, "record:body")
```

`from_dsl_nickel` also accepts `import_paths=[...]` to extend Nickel's import-resolution lookup so user-defined modules can be referenced. Instantiate the chain against source and target schemas with `chain.instantiate(src, tgt)` to get a `Lens`.

## Complement

The complement $C$ is the kernel of $get$: it captures exactly the information lost in the forward direction. For an isomorphism (bijective migration), the complement is empty. For a projection (dropping columns), the complement stores the dropped values.

The `Complement` object can be serialized to a dict:

```python
d = complement.to_dict()
print(complement.dropped_node_count)
print(complement.dropped_arc_count)
```

## Lenses from a compiled migration

`auto_generate_lens` finds a morphism between two schemas, which is a search
and does not scale to schemas of many hundreds of vertices. A migration you
already have (from `compile_migration`, or one the version-control layer
derived from a name-keyed diff) needs no search at all: a lens is a compiled
migration together with the two schemas it runs between, which is exactly
what a `CompiledMigration` holds.

So the round-trip laws are reachable directly from it:

```python
compiled = panproto.compile_migration(migration, src, tgt)

view, complement = compiled.get(instance)
source = compiled.put(view, complement)

compiled.check_laws(instance)      # raises MigrationError on violation
compiled.check_get_put(instance)
compiled.check_put_get(instance)

lens = compiled.to_lens()          # or take the Lens and use it directly
```

`get` and `put` are the two halves of the same lens, so the complement one
produces is the one the other consumes. `to_lens()` cannot fail and involves
no alignment search.
