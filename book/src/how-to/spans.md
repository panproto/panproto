# Find a span between two schemas

Use a span search when a source schema may have only a partial correspondence in a target schema. The result contains an apex, which is the matched part of the source, and a migration from that apex into the target.

## Prerequisites

The `schema` CLI or the Rust [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) crate. Both schemas must name a registered protocol.

## Search from the CLI

```sh
schema auto-migrate schemas/v1.json schemas/v2.json
```

The report includes the apex size, vertex coverage, quality, quality bounds, and the right-leg vertex and edge maps. The default command fails when the apex is empty. Add `--span` when an empty overlap is a useful answer:

```sh
schema auto-migrate schemas/v1.json schemas/v2.json --span
```

Use `--total` when every source vertex and every mappable source edge must be covered:

```sh
schema auto-migrate schemas/v1.json schemas/v2.json --total
```

`--total` and `--span` conflict. A partial optimal span does not show that no total morphism exists, so `--total` runs the total-morphism search when the first result is partial. It exits non-zero only when that search finds no total morphism or cannot run.

Add `--monic` when distinct apex vertices must map to distinct target vertices. This constrains vertex injectivity only; it does not promise an injective edge map.

### Save the mapping

```sh
schema auto-migrate schemas/v1.json schemas/v2.json --json \
  > migrations/v1-to-v2.json
```

JSON output is the span's right leg, serialized as a `Migration`. Its source is the apex. Since apex vertices reuse source identifiers, the map keys are still names from `v1`; unmatched source vertices are absent. The JSON does not include coverage, quality, or the certificate, so review the human report before saving the mapping.

If the mapping will be lifted directly, combine `--json` with `--monic` to avoid a vertex contraction that has no built-in rule for combining two source values:

```sh
schema auto-migrate schemas/v1.json schemas/v2.json --monic --json \
  > migrations/v1-to-v2.json
```

## Search from Rust

[`find_span`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_span.html) always returns a span when the search runs. Schemas with no common vertex produce an empty apex.

```rust,no_run
use panproto_mig::{SearchOptions, find_span};
use panproto_schema::{Protocol, Schema};

fn search(
    source: &Schema,
    target: &Schema,
    protocol: &Protocol,
) -> Result<(), panproto_mig::SpanError> {
    let options = SearchOptions {
        monic: true,
        ..SearchOptions::default()
    };
    let span = find_span(source, target, protocol, &options)?;

    println!("coverage: {:.1}%", span.apex_coverage * 100.0);
    println!("quality bounds: {:?}", span.quality_bounds);
    println!("total: {}", span.is_total());
    println!("vertex map: {:?}", span.right.vertex_map);
    Ok(())
}
```

Set `SearchOptions::hard_pins` only for correspondences the search may not reconsider. An incompatible pin can force that source vertex out of the apex. Soft evidence belongs in `SpanSearch::with_evidence` instead.

## Read the result

`apex_coverage` counts matched source vertices. `SchemaSpan::is_total()` also checks the relevant source edges, so coverage of `1.0` does not by itself establish totality.

`quality` ranks alternatives for one fixed source schema. Do not compare it across different source schemas. Equal lower and upper quality bounds mean that the search proved the returned score optimal; otherwise, the interval records the unresolved range.

Before accepting a result, inspect the vertex map and confirm that the coverage matches the intended task. A saved mapping can pass `schema check` while remaining partial because the existence check validates the entries present in the mapping rather than requiring source-wide coverage.

## Limitations

- A schema morphism maps one source vertex to at most one target vertex. Splits, joins, and other value computations require a [field transform](./field-transforms.md).
- `--monic` prevents vertex collisions but does not establish edge injectivity. The overlap discovery used by `schema integrate --auto-overlap` performs the stronger search required for a pushout.
- `--json` omits the apex and certificate. Keep the human report if later review needs the coverage or proof status.

## See also

- [Build a migration](./build-migration.md) for checking and applying the saved mapping.
- [Apply field transforms](./field-transforms.md) for value-level computations.
- [Migrations as morphisms](../explanation/migrations-as-morphisms.md) for the underlying model.
- [CLI reference](../reference/cli.md) for the complete option list.
