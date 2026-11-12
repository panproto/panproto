# BSON corpus

Source: <https://github.com/mongodb/specifications>, branch `master`,
commit `b8e0ee19b4622fe48c9337969da56b24037b9335` (2026-11-11), directory
`source/bson-corpus/tests`, licensed under the Creative Commons
Attribution-NonCommercial-ShareAlike 3.0 United States License
(`LICENSE.md`, the repository's licence file). The corpus is test data,
vendored unchanged for conformance testing only; it is not part of any
published crate.

Vendored, unchanged, as `tests/<file name>`: every JSON file of the
directory, 31 files, all of it (about 330 KB). The Perl script `bsonview`
of the directory is not vendored. `MANIFEST` lists the files with their
BSON type byte and their numbers of `valid` (731), `decodeErrors` (75)
and `parseErrors` (180) cases. The corpus's format is described in
`source/bson-corpus/bson-corpus.md` of the same commit, and the Extended
JSON it uses in `source/extended-json/extended-json.md` (version 2).

`conformance_bson.rs` runs every case: each representation of a `valid`
case (canonical BSON, canonical and relaxed Extended JSON, degenerate BSON
and Extended JSON) through the assertions the corpus document states for a
codec with a native representation, each `decodeErrors` case through the
decoder and each `parseErrors` case through the Extended JSON reader or,
for decimal128, the decimal128 string parser. The representations the
codec does not pass are listed with their refusal class in
`../bson-corpus-limits.txt`.
