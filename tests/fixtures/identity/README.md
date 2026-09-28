# Identity golden file

`golden.json` records the identity of every schema and lens in the identity
corpus, as `panproto_inst::identity` computes it:

- `contexts`: the blake3 key-derivation context of each kind of digest;
- `envelope_version`: the `IdentityEnvelope` layout version;
- `schemas`: each protocol fixture schema the protocol parsers read, with its
  `schema_digest` as hex;
- `lenses`: each corpus lens with the `IdentityEnvelope` it stamps on the
  complements it produces.

A lens entry's `kind` says how the lens is built:

| kind        | parameters                  | the lens                                                  |
|-------------|-----------------------------|-----------------------------------------------------------|
| `migration` | `case`                      | the migration of a `tests/fixtures/complement_upgrade` case, compiled between its schemas |
| `auto`      | `protocol`, `src`, `tgt`    | what `auto_generate` finds between two corpus schemas under the default configuration |

Every binding computes these identities from the same inputs and compares
them with this file: the Rust tests in `tests/integration`
(`every_identity_matches_the_golden_file.rs` and
`the_identity_is_the_same_in_every_process.rs`), and the bindings' own suites.
A kind a consumer has no builder for is a failure, never a skip. Adding a kind
means adding its builder to `tests/integration/tests/identity_corpus/mod.rs`
and to every other consumer.

## Versioning

A context string names the version of the encoding its digests are over.
Any change to `panproto_schema::canonical_bytes`, to
`panproto_inst::identity::canonical_compiled_bytes`, or to a field of
`IdentityEnvelope` bumps the affected context's version (and
`IDENTITY_ENVELOPE_VERSION` for an envelope change) in the same change, and
this file is regenerated:

```sh
PP_IDENTITY_GOLDEN=bless cargo test -p panproto-integration-tests --test every_identity_matches_the_golden_file
```

A change that moves a digest without bumping its context fails the golden
test, which is the point: stored complements name these digests.

Every schema fixture under `fixtures/` is in the corpus. A change to a
protocol parser that changes what it reads from a fixture moves that
schema's digest (and the envelope of every lens over it) without any
encoding change; that is a change to the schema, so the file is
re-blessed in the same change and no context version moves.

An `auto` entry exists only for a pair on which `auto_generate` finds a
lens under the default configuration, whose balanced tier asks for a
total morphism. Pairs with no total morphism, such as the two
`app.bsky.feed.post` lexicons in either direction and
`sql-tests/users` to `sql-tests/users_posts`, are pinned as typed
failures by `auto_generate_reports_no_morphism_where_none_exists.rs`.
Every other ordered pair of corpus schemas of one protocol on which
`auto_generate` finds a lens is in the corpus, with two exceptions: the
lenses from `sql-tests/users_posts` to the two Sakila schemas. The
total-morphism search behind them (`panproto_mig::find_best_morphism`)
takes about 10 s (PostgreSQL) and 3 s (SQLite) in a debug build, and
every process of the cross-process test rebuilds every lens, so those
two are left out.
