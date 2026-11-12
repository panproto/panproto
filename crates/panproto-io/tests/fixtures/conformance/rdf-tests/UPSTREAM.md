# W3C RDF test suites

Source: <https://github.com/w3c/rdf-tests>, commit
`4158bca195a29fe1fd90f92698170bc7cc385273` (2026-11-10), dual-licensed
under the W3C Test Suite License and the W3C 3-clause BSD License, as the
repository's `LICENSE.md` (kept here unchanged) states; used under the
3-clause BSD form. `LICENSE` is `rdf/rdf11/rdf-turtle/LICENSE` of the same
commit, unchanged, which holds the text of both licences.

Vendored, unchanged:

- `rdf/rdf11/rdf-n-triples` as `rdf11/rdf-n-triples`: its `manifest.ttl`
  and every `.nt` case.
- `rdf/rdf11/rdf-turtle` as `rdf11/rdf-turtle`: its `manifest.ttl` and
  every `.ttl` and `.nt` file; the `TESTS.zip` and `TESTS.tar.gz`
  archives, the `reports` directory of implementation reports and
  `README` are left out; the directory's `LICENSE` is kept at the top of
  this directory.

`conformance_rdf_tests.rs` runs every entry of each manifest.
