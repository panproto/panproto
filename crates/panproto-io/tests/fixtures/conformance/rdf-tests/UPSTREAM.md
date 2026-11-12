# W3C RDF test suites

Source: <https://github.com/w3c/rdf-tests>, commit
`4158bca195a29fe1fd90f92698170bc7cc385273` (2026-11-10), W3C test suite
licence, used under its 3-clause BSD form (`LICENSE`).

Vendored, unchanged:

- `rdf/rdf11/rdf-n-triples` as `rdf11/rdf-n-triples`: its `manifest.ttl`
  and every `.nt` case.
- `rdf/rdf11/rdf-turtle` as `rdf11/rdf-turtle`: its `manifest.ttl` and
  every `.ttl` and `.nt` file; the `TESTS.zip` and `TESTS.tar.gz`
  archives, the `reports` directory of implementation reports, `README`
  and the directory's own copy of `LICENSE` are left out.

`conformance_rdf_tests.rs` runs every entry of each manifest.
