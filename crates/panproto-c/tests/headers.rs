//! Header generation under `--features headers`.
//!
//! Run via:
//!
//! ```text
//! cargo test -p panproto-c --features headers -- generate_headers --ignored
//! ```
//!
//! The generated header is written to `crates/panproto-c/include/panproto.h`.
//! That file is committed; CI rejects diffs.

#[cfg(feature = "headers")]
#[test]
#[ignore = "run only when regenerating the C header"]
fn generate_headers() -> std::io::Result<()> {
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("include");
    std::fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("panproto.h");
    panproto_c::generate_headers_to(&out_path)
}
