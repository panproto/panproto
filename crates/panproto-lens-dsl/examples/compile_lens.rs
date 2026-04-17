//! Compile a lens DSL file to a `ProtolensChain` JSON on stdout.
//!
//! ```shell
//! cargo run -p panproto-lens-dsl --example compile_lens -- path/to/lens.yaml BODY_VERTEX
//! ```

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path: PathBuf = args
        .next()
        .ok_or("usage: compile_lens <lens-file> <body-vertex>")?
        .into();
    let body_vertex = args
        .next()
        .ok_or("usage: compile_lens <lens-file> <body-vertex>")?;

    let compiled = panproto_lens_dsl::load_and_compile(&path, &body_vertex)?;
    println!("{}", compiled.chain.to_json()?);
    Ok(())
}
