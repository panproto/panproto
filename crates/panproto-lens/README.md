# panproto-lens

[Bidirectional lens](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29) combinators for panproto.

Every schema migration is a lens with a `get` direction (restrict, projecting data forward) and a `put` direction (restore from [complement](https://en.wikipedia.org/wiki/View_(database)#Updating_views), bringing modifications back). The lens laws -- GetPut and PutGet -- guarantee round-trip fidelity (see [Diskin et al., 2011](https://doi.org/10.1016/j.tcs.2010.12.039)). This crate also provides [Cambria](https://www.inkandswitch.com/cambria/)-style atomic combinators for building lenses declaratively.

## API

| Item | Description |
|------|-------------|
| `Lens` | Asymmetric lens backed by a compiled migration, source schema, and target schema |
| `get` | Forward direction: project an instance to a view, producing a complement |
| `put` | Backward direction: restore source from a modified view and complement |
| `Complement` | Data discarded by `get`, needed by `put` to reconstruct the source |
| `Combinator` | [Cambria](https://www.inkandswitch.com/cambria/)-style atomic schema transformation (rename, add, remove, etc.) |
| `from_combinators` | Build a lens from a chain of combinators |
| `compose` | Compose two lenses sequentially |
| `SymmetricLens` | Symmetric (bidirectional) lens variant |
| `check_laws` / `check_get_put` / `check_put_get` | Verify lens laws on a test instance |
| `LensError` / `LawViolation` | Error types |

## Example

```rust,ignore
use panproto_lens::{Lens, get, put, check_laws};

let (view, complement) = get(&lens, &instance)?;
// Modify the view...
let restored = put(&lens, &view, &complement)?;

// Verify round-trip laws
check_laws(&lens, &instance)?;
```

## License

[MIT](../../LICENSE)
