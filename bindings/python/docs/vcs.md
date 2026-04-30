# Schematic Version Control

panproto provides git-like version control for schemas. Commits store schemas as content-addressed objects (blake3 hashes). Merge is computed via schema colimit: given branches $S_1$ and $S_2$ from common ancestor $S_0$, the merge schema is the pushout $S_1 +_{S_0} S_2$.

## In-memory repository

```python
import panproto

repo = panproto.VcsRepository()
```

## Adding schemas

```python
object_id = repo.add(schema)
print(object_id)   # blake3 hash string
```

## Listing refs

```python
refs = repo.list_refs()
```

## Architecture

Objects in the store are content-addressed. The `ObjectId` is a blake3 hash of the serialized object. Object types include:

- `Schema`: a validated schema
- `Commit`: points to a schema, parent commits, and an optional migration
- `Migration`: the migration specification between two schemas
- `Tag`: annotated tag pointing to any object
- `DataSet`: instance data bound to a schema
- `Complement`: complement data from a data migration
- `Protocol`: protocol specification
- `Expr`: expression used in a migration resolver

The VCS module is currently in-memory only. Filesystem-backed repositories (via `FsStore`) are available in the Rust API and will be exposed in a future version.
