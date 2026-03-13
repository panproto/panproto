# panproto

Universal schema migration engine built on generalized algebraic theories.

## Features

- **77 protocols** supported with bidirectional parse/emit
- **Functorial data migration** with mathematical compositionality guarantees
- **Bidirectional lenses** with complement tracking for lossless round-trips

### Performance

panproto uses SIMD-accelerated parsing throughout:

| Pathway | Library | Speedup |
|---------|---------|---------|
| JSON | `simd-json` | 2-4x |
| HTML | `tl` | SIMD tag scanning |
| Tabular | `memchr` | SIMD byte search |

### Quick Start

```rust
let registry = panproto_io::default_registry();
let instance = registry.parse_wtype("html", &schema, &bytes)?;
```

> The commutativity guarantee from Spivak 2012 ensures that
> parse, restrict, and emit compose correctly.

For more details, see the [tutorial](https://panproto.github.io/tutorial/).
