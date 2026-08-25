# Engine fixtures

Each file contains bytes returned by a panproto-c entry point.
`Scripts/GenerateFixtures` captures the payloads through the `Raw` shim layer.

Regenerate the whole directory with:

```
cd bindings/swift
swift run generate-fixtures Tests/PanprotoTests/Fixtures
```

The generator removes the existing `.cbor` and `.json` fixtures before writing
the new set. Payloads are CBOR produced by `ciborium` unless the file name ends
in `.json`.

| Fixture | Entry point | Bytes | Captured from |
| --- | --- | --- | --- |
| `builtin-protocols.cbor` | `Raw.registryListBuiltin()` | 401 | the whole built-in catalogue |
| `protocol-brat.cbor` | `Raw.registryGetBuiltin(name:)` | 969 | name `brat` |
| `protocol-conllu.cbor` | `Raw.registryGetBuiltin(name:)` | 827 | name `conllu` |
| `protocol-naf.cbor` | `Raw.registryGetBuiltin(name:)` | 1495 | name `naf` |
| `protocol-uima.cbor` | `Raw.registryGetBuiltin(name:)` | 974 | name `uima` |
| `protocol-folia.cbor` | `Raw.registryGetBuiltin(name:)` | 1544 | name `folia` |
| `protocol-tei.cbor` | `Raw.registryGetBuiltin(name:)` | 1021 | name `tei` |
| `protocol-timeml.cbor` | `Raw.registryGetBuiltin(name:)` | 844 | name `timeml` |
| `protocol-elan.cbor` | `Raw.registryGetBuiltin(name:)` | 1006 | name `elan` |
| `protocol-iso_space.cbor` | `Raw.registryGetBuiltin(name:)` | 1246 | name `iso_space` |
| `protocol-paula.cbor` | `Raw.registryGetBuiltin(name:)` | 1607 | name `paula` |
| `protocol-laf_graf.cbor` | `Raw.registryGetBuiltin(name:)` | 782 | name `laf_graf` |
| `protocol-decomp.cbor` | `Raw.registryGetBuiltin(name:)` | 1312 | name `decomp` |
| `protocol-ucca.cbor` | `Raw.registryGetBuiltin(name:)` | 557 | name `ucca` |
| `protocol-fovea.cbor` | `Raw.registryGetBuiltin(name:)` | 1650 | name `fovea` |
| `protocol-bead.cbor` | `Raw.registryGetBuiltin(name:)` | 1629 | name `bead` |
| `protocol-web_annotation.cbor` | `Raw.registryGetBuiltin(name:)` | 2237 | name `web_annotation` |
| `protocol-amr.cbor` | `Raw.registryGetBuiltin(name:)` | 4277 | name `amr` |
| `protocol-concrete.cbor` | `Raw.registryGetBuiltin(name:)` | 1040 | name `concrete` |
| `protocol-nif.cbor` | `Raw.registryGetBuiltin(name:)` | 1183 | name `nif` |
| `protocol-openapi.cbor` | `Raw.registryGetBuiltin(name:)` | 731 | name `openapi` |
| `protocol-asyncapi.cbor` | `Raw.registryGetBuiltin(name:)` | 619 | name `asyncapi` |
| `protocol-jsonapi.cbor` | `Raw.registryGetBuiltin(name:)` | 494 | name `jsonapi` |
| `protocol-raml.cbor` | `Raw.registryGetBuiltin(name:)` | 541 | name `raml` |
| `protocol-graphql.cbor` | `Raw.registryGetBuiltin(name:)` | 638 | name `graphql` |
| `protocol-cloudformation.cbor` | `Raw.registryGetBuiltin(name:)` | 455 | name `cloudformation` |
| `protocol-ansible.cbor` | `Raw.registryGetBuiltin(name:)` | 449 | name `ansible` |
| `protocol-k8s_crd.cbor` | `Raw.registryGetBuiltin(name:)` | 518 | name `k8s_crd` |
| `protocol-cddl.cbor` | `Raw.registryGetBuiltin(name:)` | 530 | name `cddl` |
| `protocol-bson.cbor` | `Raw.registryGetBuiltin(name:)` | 512 | name `bson` |
| `protocol-json-schema.cbor` | `Raw.registryGetBuiltin(name:)` | 684 | name `json-schema` |
| `protocol-dataframe.cbor` | `Raw.registryGetBuiltin(name:)` | 473 | name `dataframe` |
| `protocol-parquet.cbor` | `Raw.registryGetBuiltin(name:)` | 485 | name `parquet` |
| `protocol-arrow.cbor` | `Raw.registryGetBuiltin(name:)` | 644 | name `arrow` |
| `protocol-mongodb.cbor` | `Raw.registryGetBuiltin(name:)` | 618 | name `mongodb` |
| `protocol-dynamodb.cbor` | `Raw.registryGetBuiltin(name:)` | 480 | name `dynamodb` |
| `protocol-cassandra.cbor` | `Raw.registryGetBuiltin(name:)` | 622 | name `cassandra` |
| `protocol-neo4j.cbor` | `Raw.registryGetBuiltin(name:)` | 545 | name `neo4j` |
| `protocol-redis.cbor` | `Raw.registryGetBuiltin(name:)` | 401 | name `redis` |
| `protocol-sql.cbor` | `Raw.registryGetBuiltin(name:)` | 487 | name `sql` |
| `protocol-geojson.cbor` | `Raw.registryGetBuiltin(name:)` | 505 | name `geojson` |
| `protocol-fhir.cbor` | `Raw.registryGetBuiltin(name:)` | 581 | name `fhir` |
| `protocol-rss_atom.cbor` | `Raw.registryGetBuiltin(name:)` | 497 | name `rss_atom` |
| `protocol-vcard_ical.cbor` | `Raw.registryGetBuiltin(name:)` | 465 | name `vcard_ical` |
| `protocol-swift_mt.cbor` | `Raw.registryGetBuiltin(name:)` | 439 | name `swift_mt` |
| `protocol-edi_x12.cbor` | `Raw.registryGetBuiltin(name:)` | 494 | name `edi_x12` |
| `protocol-avro.cbor` | `Raw.registryGetBuiltin(name:)` | 667 | name `avro` |
| `protocol-flatbuffers.cbor` | `Raw.registryGetBuiltin(name:)` | 781 | name `flatbuffers` |
| `protocol-asn1.cbor` | `Raw.registryGetBuiltin(name:)` | 844 | name `asn1` |
| `protocol-bond.cbor` | `Raw.registryGetBuiltin(name:)` | 794 | name `bond` |
| `protocol-msgpack_schema.cbor` | `Raw.registryGetBuiltin(name:)` | 729 | name `msgpack_schema` |
| `protocol-protobuf.cbor` | `Raw.registryGetBuiltin(name:)` | 679 | name `protobuf` |
| `protocol-atproto.cbor` | `Raw.registryGetBuiltin(name:)` | 809 | name `atproto` |
| `protocol-docx.cbor` | `Raw.registryGetBuiltin(name:)` | 586 | name `docx` |
| `protocol-odf.cbor` | `Raw.registryGetBuiltin(name:)` | 545 | name `odf` |
| `schema-bsky-post.cbor` | `Raw.schemaParseAtprotoLexicon(json:) then Raw.schemaToCbor(schemaHandle:)` | 25066 | `fixtures/atproto/lexicons/app.bsky.feed.post.json` |
| `schema-bsky-profile.cbor` | `Raw.schemaParseAtprotoLexicon(json:) then Raw.schemaToCbor(schemaHandle:)` | 10513 | `fixtures/atproto/lexicons/app.bsky.actor.profile.json` |
| `schema-metadata-post.cbor` | `Raw.schemaMetadata(schemaHandle:)` | 5811 | the `app.bsky.feed.post` schema |
| `instance-post-0.cbor` | `Raw.instJsonToInstance(schemaHandle:json:rootVertex:)` | 1114 | `fixtures/atproto/records/post-0.json` at root vertex `app.bsky.feed.post` |
| `diff-simple-post-profile.cbor` | `Raw.checkDiffSimple(s1:s2:)` | 7006 | post schema against profile schema |
| `diff-full-post-profile.cbor` | `Raw.checkDiffFull(s1:s2:)` | 8395 | post schema against profile schema |
| `compat-report.cbor` | `Raw.checkClassify(proto:diff:)` | 9461 | the full diff against the `atproto` protocol |
| `chain-post-profile.json` | `Raw.protolensChainToJson(chain:)` | 415 | the chain auto-generated at stringency `lenient` |
| `complement-spec.cbor` | `Raw.protolensComplementSpec(chain:schema:)` | 577 | the same chain at the post schema |
| `get-record.cbor` | `Raw.lensGetRecord(migration:record:)` | 1660 | the post instance through the post schema's chain against itself |
| `vcs-add.cbor` | `Raw.vcsAdd(repo:schema:)` | 119 | the post schema staged into a fresh repository |
| `vcs-commit.cbor` | `Raw.vcsCommit(repo:message:author:)` | 163 | the commit that records the staged post schema |
| `vcs-branches.cbor` | `Raw.vcsListBranches(repo:)` | 211 | the listing after `Raw.vcsBranch(repo:name:)` created `post-fixture` |
| `vcs-status.cbor` | `Raw.vcsStatus(repo:)` | 128 | the repository after the commit and the branch |
| `vcs-log.cbor` | `Raw.vcsLog(repo:count:)` | 275 | the ten most recent commits |
| `theory-graph.cbor` | `Raw.gatCreateTheory(spec:) then Raw.gatSerializeTheory(theory:)` | 248 | a two-sort graph theory with `src` and `tgt` |
| `theory-labelled.cbor` | `Raw.gatCreateTheory(spec:) then Raw.gatSerializeTheory(theory:)` | 212 | a theory adding `Label` and `label` over the same `Vertex` sort |
| `theory-graph-labelled-colimit.cbor` | `Raw.gatColimit(t1:t2:shared:) then Raw.gatSerializeTheory(theory:)` | 363 | `ThGraph` and `ThLabelled` amalgamated over `ThVertex` |
| `expr-parsed.cbor` | `Raw.exprParse(source:)` | 120 | source `let base = 1 in map (\x -> x.score + base) records` |

## Notes

The `vcs-*` payloads carry commit ids and timestamps from the run that produced them, so regenerating them changes their bytes even when the engine has not changed.

The post and profile schemas align only at the `lenient` tier.
`Raw.lensAutoGenerateProtolens(schema1:schema2:stringency:)` returns code 7
with "no morphism found between schemas" at `strict`, `balanced`, and
`exploratory`. The `exploratory` strategy is not a superset of `lenient`. It
uses lossy retraction witnesses and a lower similarity threshold.

`Raw.lensGetRecord(migration:record:)` cannot carry the fixture record through
the post-to-profile chain. Chain generation and instantiation succeed, but get
returns code 7 because the target schema has no edge from
`app.bsky.feed.post:body` to `app.bsky.feed.post:body.langs:items`. Every post
fixture has `langs`. `get-record.cbor` thus uses the chain from the post
schema to itself.

Built-in protocol payloads name their theories with strings. The C ABI has no
function that resolves one of those names to a `Theory` handle.
`pp_gat_create_theory` takes a complete CBOR theory, while `pp_gat_colimit`
takes theory handles. The `theory-*` fixtures thus come from theories
created explicitly through `pp_gat_create_theory`.
