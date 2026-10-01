//! Regenerate the frozen validation corpus.
//!
//! Each fixture is one validation case: the schema, the protocol it is read
//! under (its name, and its definition when the name is not a registered
//! protocol's), the validation options, budget and external verdicts, the
//! instance, the edit trace applied to it after it is validated, the report
//! validation gives, and what each edit of the trace does: the report
//! validating the instance after the edit gives, or the error applying it
//! gives, which leaves the instance as it was. It is written three times, as named `MessagePack`
//! (`<case>.named.msgpack`), CBOR (`<case>.cbor`) and JSON
//! (`<case>.json`), and a graph instance is also written as node-link JSON
//! (`<case>.nodelink.json`). A report's diagnostics carry their failure
//! witnesses, which `Diagnostic::verify` confirms against the instance. The
//! cases are grouped by what they exercise: `anchors`, `endpoints`,
//! `values`, `cardinality`, `ordering`, `cycles`, `fans`, `keys` and
//! `incomplete`, each case in every instance shape that can express it.
//!
//! Instances that construction or decoding rejects are written as
//! rejections (`<case>.rejection.*`): the instance as its wire form, and
//! the error decoding it gives.
//!
//! Run with the `fixtures` feature, which brings the `MessagePack` and CBOR
//! writers: `cargo run -p panproto-protocols --features fixtures --bin
//! validation-fixtures [OUT_DIR]`, where `OUT_DIR` defaults to the
//! repository's `tests/fixtures/validation`. Built without it, the binary
//! writes nothing and exits with an error. The validation fixture tests run
//! this binary into a temporary directory and fail when its output differs
//! from the committed corpus.

#[cfg(feature = "fixtures")]
mod corpus;

#[cfg(feature = "fixtures")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    corpus::run()
}

#[cfg(not(feature = "fixtures"))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "validation-fixtures writes MessagePack and CBOR, which need the `fixtures` feature: \
         cargo run -p panproto-protocols --features fixtures --bin validation-fixtures"
    );
    std::process::ExitCode::FAILURE
}
