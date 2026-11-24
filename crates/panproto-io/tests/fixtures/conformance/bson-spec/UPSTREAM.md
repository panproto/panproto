# Original BSON conformance set

Not vendored: every case of `MANIFEST` was written for panproto from the
text of the BSON 1.1 specification (<https://bsonspec.org/spec.html>), the
BSON Decimal128 specification and the Extended JSON v2 specification
(`source/bson-decimal128/decimal128.md` and
`source/extended-json/extended-json.md` of
<https://github.com/mongodb/specifications>), and each line names the
section it exercises. No case is copied from the BSON corpus of those
specifications, which is fetched rather than kept here (see
`../bson-corpus/UPSTREAM.md`). The set is under the repository's MIT
licence. `conformance_bson_spec.rs` runs every case in every build of the
tests and pins the counts: 31 valid, 5 relaxed, 6 read, 27 decode errors,
34 parse errors and 3 write errors.
