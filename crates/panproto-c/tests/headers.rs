//! Header generation under `--features headers`.
//!
//! Run via:
//!
//! ```text
//! PP_REGEN_HEADERS=1 cargo test -p panproto-c --features headers -- generate_headers
//! ```
//!
//! The generated header is written to `crates/panproto-c/include/panproto.h`.
//! That file is committed; CI rejects diffs.
//!
//! Without `PP_REGEN_HEADERS` set the test is an early-return no-op, so the
//! default `cargo test` run never rewrites the committed header.

#[cfg(feature = "headers")]
#[test]
fn generate_headers() -> std::io::Result<()> {
    let Ok(_) = std::env::var("PP_REGEN_HEADERS") else {
        return Ok(());
    };
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("include");
    std::fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("panproto.h");
    panproto_c::generate_headers_to(&out_path)
}
