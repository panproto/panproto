# W3C JSON-LD 1.1 API tests

Source: <https://github.com/w3c/json-ld-api>, commit
`55514738d854e56bb8bad4c53cda2f0e23be4a50` (2026-10-07), licensed under the
W3C Software and Document License (`LICENSE.md`).

Vendored, unchanged:

- `tests/toRdf-manifest.jsonld` as `toRdf-manifest.jsonld`;
- every file of `tests/toRdf` as `toRdf`;
- `tests/expand/er56-in.jsonld` as `expand/er56-in.jsonld`, the input the
  `toRdf` entry `#ter56` names.

`conformance_jsonld_api.rs` runs every entry of the manifest; the entries
the JSON-LD route does not pass are listed with their class in
`../json-ld-api-limits.txt`.
