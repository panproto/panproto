# BSON corpus

Source: <https://github.com/mongodb/specifications>, branch `master`,
commit `b8e0ee19b4622fe48c9337969da56b24037b9335` (2026-11-11), directory
`source/bson-corpus/tests`, licensed under the Creative Commons
Attribution-NonCommercial-ShareAlike 3.0 United States License (the
repository's `LICENSE.md` at that commit). That licence is neither
permissive nor compatible with this repository's MIT licence, so the corpus
is not kept here.

`MANIFEST` records the repository, the commit and the directory on its
`source` line, and lists the 31 JSON files of the directory with their BSON
type byte, their numbers of `valid` (731), `decodeErrors` (75) and
`parseErrors` (180) cases and their SHA-256 digests. The Perl script
`bsonview` of the directory is not fetched. The corpus's format is described
in `source/bson-corpus/bson-corpus.md` of the same commit, and the Extended
JSON it uses in `source/extended-json/extended-json.md` (version 2).

`tools/fetch-bson-corpus.py` downloads each file at the recorded commit,
checks its digest and puts it in `tests/`, which git ignores, with the
commit in `tests/REVISION`; `--check` verifies a fetched copy without the
network. `conformance_bson.rs` runs every case of the fetched corpus: each
representation of a `valid` case (canonical BSON, canonical and relaxed
Extended JSON, degenerate BSON and Extended JSON) through the assertions the
corpus document states for a codec with a native representation, each
`decodeErrors` case through the decoder and each `parseErrors` case through
the Extended JSON reader or, for decimal128, the decimal128 string parser.
The representations the codec does not pass are named, with their refusal
class, in `../bson-corpus-limits.txt`. Without a fetched corpus the test
reports itself skipped, or fails naming the fetch command when
`PANPROTO_BSON_CORPUS=required` is set, as the corpus gate workflow sets it
after fetching. Per-PR CI runs the original set of `../bson-spec` instead.
