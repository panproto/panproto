# W3C JSON-LD 1.1 API tests

Source: <https://github.com/w3c/json-ld-api>, commit
`55514738d854e56bb8bad4c53cda2f0e23be4a50` (2026-10-07), licensed under the
W3C Software and Document License and the dual licensing of W3C test
suites, as the repository's `LICENSE.md` (kept here unchanged) states. That
file only names the licence, which requires its full text and a copyright
notice to accompany copies, so `LICENSE-W3C-SOFTWARE-AND-DOCUMENT.md` holds
the notice and the text of the 2023 version the licence link resolves to.

Vendored, unchanged:

- `tests/toRdf-manifest.jsonld` as `toRdf-manifest.jsonld`;
- every file of `tests/toRdf` as `toRdf`;
- `tests/expand/er56-in.jsonld` as `expand/er56-in.jsonld`, the input the
  `toRdf` entry `#ter56` names.

`conformance_jsonld_api.rs` runs every entry of the manifest; the entries
the JSON-LD route does not pass are listed with their class in
`../json-ld-api-limits.txt`.
