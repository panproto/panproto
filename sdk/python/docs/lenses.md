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

lens, quality = panproto.auto_generate_lens(src_schema, tgt_schema, protocol)
print(quality)   # alignment quality score, 0.0 to 1.0
```

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

## Complement

The complement $C$ is the kernel of $get$: it captures exactly the information lost in the forward direction. For an isomorphism (bijective migration), the complement is empty. For a projection (dropping columns), the complement stores the dropped values.

The `Complement` object can be serialized to a dict:

```python
d = complement.to_dict()
print(complement.dropped_node_count)
print(complement.dropped_arc_count)
```
