//! Build a tiny git repository in a tempdir, populate it with a real
//! OpenTelemetry `.proto` file, and import it into a panproto-vcs `MemStore`.

use std::fs;

use panproto_git::import_git_repo;
use panproto_vcs::MemStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let repo = git2::Repository::init(tmp.path())?;

    fs::write(
        tmp.path().join("trace.proto"),
        include_bytes!("../../../fixtures/protobuf/trace.proto"),
    )?;

    let mut index = repo.index()?;
    index.add_path(std::path::Path::new("trace.proto"))?;
    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let sig = git2::Signature::now("example", "example@panproto.dev")?;
    repo.commit(Some("HEAD"), &sig, &sig, "vendor trace.proto", &tree, &[])?;

    let mut store = MemStore::new();
    let result = import_git_repo(&repo, &mut store, "HEAD")?;
    println!(
        "imported {} commit(s); panproto HEAD = {}",
        result.commit_count, result.head_id
    );
    Ok(())
}
