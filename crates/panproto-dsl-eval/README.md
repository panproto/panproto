# panproto-dsl-eval

[![crates.io](https://img.shields.io/crates/v/panproto-dsl-eval.svg)](https://crates.io/crates/panproto-dsl-eval)
[![docs.rs](https://docs.rs/panproto-dsl-eval/badge.svg)](https://docs.rs/panproto-dsl-eval)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Shared Nickel, JSON, and YAML evaluation for panproto's declarative DSLs.

## Installation

Add the crate to a Rust 1.85 or newer project:

```sh
cargo add panproto-dsl-eval
```

## Usage

```rust
use panproto_dsl_eval::{eval_nickel, BundledContract};

let contract = BundledContract {
    file_name: "lens.ncl",
    source: include_str!("../contracts/lens.ncl"),
};
let document: LensDocument = eval_nickel(source, &[], &contract)?;
```

## Source formats

[`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl) and
[`panproto-theory-dsl`](https://docs.rs/panproto-theory-dsl) each load declarative
documents from three formats, and the evaluation machinery is the same for both.

| Extension | Evaluated by |
|-----------|--------------|
| `.ncl` | [`nickel-lang`](https://docs.rs/nickel-lang), with a bundled contract library staged on the import path, then deserialized via `to_serde` |
| `.json` | [`serde_json`](https://docs.rs/serde_json) |
| `.yaml`, `.yml` | [`yaml_serde`](https://docs.rs/yaml_serde) |

This crate owns the `nickel-lang` and `yaml_serde` dependencies and the
temp-directory contract staging, so each DSL crate depends on it once instead of
compiling the Nickel evaluator itself. The functions are generic over the target
document type: a DSL crate supplies its own document type and contract library,
and maps `DslEvalError` into its own error type.

## Nickel contracts

A caller embeds its contract library with `include_str!` and passes it as a
`BundledContract`. The library is written to a temp directory placed first on the
Nickel import path, so `import "panproto/<file_name>"` resolves during evaluation;
`file_name` doubles as the source name Nickel reports in diagnostics. Additional
import paths are appended after it, which lets a document import its own
neighbours.

Where the Nickel diagnostic carries a source location, it surfaces as
`DslEvalError::NickelEvalSpanned`, which keeps the evaluated source alongside the
byte span of the offending token so a caller can render the failure against the
text it came from.

## API reference

| Item | Purpose |
|------|---------|
| `eval_nickel` | Evaluate a Nickel source against a bundled contract, then deserialize |
| `eval_json` | Deserialize a JSON source |
| `eval_yaml` | Deserialize a YAML source |
| `BundledContract` | A contract library and the file name it is imported under |
| `DslEvalError` | Nickel evaluation (with and without a span), JSON, YAML, and contract-staging errors |

The underlying configuration language is [Nickel](https://nickel-lang.org).

## License

[MIT](../../LICENSE)
