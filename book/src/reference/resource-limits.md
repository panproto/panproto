# Resource limits

Reading a schema costs time and memory proportional to what the input asks for, and a public entry point reads input it did not author. Every such surface therefore parses within a **budget**: an allowance that is checked before work begins and drawn down as it proceeds.

## What is bounded

| Resource | Default | What it counts |
|---|---|---|
| `input_bytes` | 64 MiB | Bytes of input accepted in one operation |
| `bundle_entries` | 4096 | Documents in one bundle |
| `graph_elements` | 1,000,000 | Vertices, edges and nodes in a decoded schema or instance |
| `metadata_bytes` | 16 MiB | Metadata attached to a schema or instance |
| `depth` | 128 | Levels of nesting descended |
| `steps` | 10,000,000 | Steps of evaluation or search |
| `output_bytes` | 64 MiB | Bytes of output produced |

The depth bound matches the walk and extraction depths that already existed, so it governs nothing it did not govern before.

## Two properties that make this a policy

Several subsystems already had sound local bounds: parser walk depth, CST extraction depth, expression evaluation steps, morphism search budget, model-check assignment counts. What they were not was a policy. What an input was allowed to cost depended on which door it came through, so the same document could be refused through one surface and accepted through another.

**A budget is shared, not per-call.** A nested operation draws from the same allowance as the operation containing it. Cloning a `Budget` shares its counters rather than copying them, so a caller is not charged once for a walk and again for each subwalk, and an oversized input cannot slip past a bound by being processed in pieces. A budget that reset per subsystem would bound each step and nothing overall, which is what several unrelated local limits already achieved.

**A failure names the resource and the bound.** `LimitExceeded` carries which allowance ran out and what it was set to. "Too deep" without a number tells a caller nothing about what to pass instead, and with seven separate allowances it does not even say which setting to change.

## Defaults, configuration, and opting out

`parse_schema_document`, `parse_schema_source` and `parse_schema_bundle` apply `ResourceLimits::defaults()`. Every binding calls those, so the CLI, Python, C and WASM all inherit the same policy; none of them selects its own, and none can inherit an unbounded one by accident.

A Rust caller that needs different bounds uses the `_within` variants:

```rust,ignore
use panproto_expr::limits::{Budget, ResourceLimits};

let mut limits = ResourceLimits::defaults();
limits.bundle_entries = 32_768;
let budget = Budget::new(limits);

let schema = panproto_protocols::parse_schema_bundle_within("atproto", &docs, &budget)?;
```

Passing the same budget to several calls has them share one allowance, which is how a caller bounds a whole pipeline rather than each stage of it.

`ResourceLimits::unbounded()` removes every bound, and a field set to `0` removes that one. This is a legitimate choice for a Rust caller processing input it produced itself. It is deliberately something to ask for: no FFI or command-line entry point selects it, because unbounded behaviour should never be inherited at a boundary that reads input from elsewhere.

## See also

- [What panproto verifies](../explanation/what-is-verified.md) for the bounded model check, whose assignment budget is configured separately per invocation.
