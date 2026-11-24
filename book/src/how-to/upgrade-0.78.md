# Upgrade from 0.77 to 0.78

Release 0.78 makes a committed data set record the protocol its records are records under. The protocol was looked up by name in the commit the data was read at, so redefining a name, merging, amending or replaying commits could change what committed data meant or make it unreadable. [Committed data](../explanation/semantics/pushouts-and-merge.md#committed-data) and [Protocol environments](../explanation/semantics/pushouts-and-merge.md#protocol-environments) give the model. The [changelog](https://github.com/panproto/panproto/blob/main/CHANGELOG.md) lists every change.

## Repositories

Do not use a repository 0.78 has written to with an earlier release. Each data set has a further field, and a commit's environment is an object kind that earlier releases do not know, so they cannot read its data or collect its garbage: both fail with a decoding error and change nothing. They still read its commits and schemas, and still commit schemas to it, and such a commit records no environment.

0.78 reads repositories written by earlier releases, with no conversion, and the object IDs in them are unchanged. Data in them records no protocol, so [`Repository::decoded_data_at`](https://docs.rs/panproto-vcs/latest/panproto_vcs/repo/struct.Repository.html#method.decoded_data_at) checks it against its schema alone, and returns attributes the schema does not declare. A commit that recorded one protocol object is read as the environment that defines its name.

A staging index written by an earlier release loses its staged protocol. Stage it again.

## Protocols are staged, not registered

`Repository::set_protocol` is removed, and with it `ProtocolSources`, `resolve_protocol`, `data_mig::protocol_for_schema` and the `registered` parameters. A protocol that no registry defines is staged with [`Repository::add_protocol`](https://docs.rs/panproto-vcs/latest/panproto_vcs/repo/struct.Repository.html#method.add_protocol), and the next commit records it. Every commit keeps the protocols of the commits it is made from.

Before, in 0.77:

```text
repo.set_protocol(protocol);
```

After:

```rust
use panproto_core::schema::{Protocol, SchemaBuilder};
use panproto_core::vcs::Repository;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("upgrade-078-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let protocol = Protocol {
        name: "notes".into(),
        obj_kinds: vec!["object".into()],
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("note", "object", None)?
        .build()?;

    let mut repo = Repository::init(&dir)?;
    repo.add_protocol(&protocol)?;
    repo.add(&schema)?;
    repo.commit("notes", "alice")?;
    assert_eq!(
        repo.environment()?.resolve(repo.store(), "notes")?,
        protocol
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
```

A protocol name that means nothing is now `VcsError::UnknownProtocol` when data is staged, where it was a reason inside `VcsError::DataValidationFailed`.

## Merging, cherry-picking and rebasing

Branches that give a protocol name different protocols do not merge. `Repository::merge`, `cherry_pick` and `rebase` return `VcsError::ProtocolConflicts`, with each name and both protocols. Stage the protocol one branch means on the other and try again. A change only one branch made is kept; before, a merge could discard it and a cherry-pick could revert it.

Data transported to a merged schema must be valid for that schema under the protocol it means there. A merge, cherry-pick or rebase whose data is not is refused with `VcsError::DataMigrationFailed`. Data that a commit holds over another schema than its own is carried unchanged.

## Amending

`Repository::amend` records everything staged. It refuses a stage that `commit` refuses, with `VcsError::ValidationPending` or `VcsError::ValidationFailed`, where it recorded an unverified schema before.

## Reading JSON

[`parse_json`](https://docs.rs/panproto-inst/latest/panproto_inst/parse/fn.parse_json.html) loses nothing it reads. Three kinds of input change:

| Input | 0.77 | 0.78 |
|---|---|---|
| A nonempty array at a vertex with no item edge | Read as an empty list | Held whole as the vertex's value |
| A `$type` member that is not a string | Dropped | Kept as an attribute |
| An empty object at a list vertex | Written back as an empty array | Refused with `ParseError::ExpectedArray` |

## Rust types

| 0.77 | 0.78 |
|---|---|
| `CommitObject::protocol_id` | `CommitObject::environment_id`, an `Object::ProtocolEnvironment` |
| `CommitObjectBuilder::protocol_id(id)` | `CommitObjectBuilder::environment_id(Some(id))` |
| `Index::staged_protocol` | `Index::staged_protocols`, a `ProtocolEnvironment` |
| `DataSetObject` with four fields | a fifth, `protocol_id` |
| `migrate_forward(store, data, src, tgt, protocol)` | `migrate_forward(store, data, src, tgt)`, each end a `Fibre` |
| `migrate_backward(store, data, complement, src, tgt, protocol)` | `migrate_backward(store, data, complement, src, tgt)`, each end a `Fibre` |
| `lift_commit_data`, returning a pair | takes a `LiftTarget` and returns `LiftedData` |
| `verify_square` on five arguments | takes a `TransportedData` and two `Fibre`s |
| `cherry_pick`, `cherry_pick_with_options`, `rebase` with `registered` | without it |

`Index::has_staged` is no longer `const`.

## Python

`Repository.add` stages the protocol a schema was built under, so nothing needs registering. `index()` reports `staged_protocols`, a mapping from names to object IDs, in place of `staged_protocol`. Each mapping `data_at` returns has a `protocol_id`.

A schema read from JSON has only its protocol's name; stage the protocol with `Repository.add_protocol`. A constraint whose sort has no meaning makes a record's validation undecided, and staging refuses it. Give `Protocol.from_theories` the meanings in `constraint_semantics`.
