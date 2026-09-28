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

Fixtures the protocol parsers do not yet read are left out of the corpus:
`fixtures/jsonschema/{github-workflow,package,tsconfig}.json`,
`fixtures/protobuf/descriptor.proto` and `fixtures/sql/sakila-postgres.sql`.
