# OpenAPI 3.1 test documents

Source: <https://github.com/OAI/OpenAPI-Specification>, licensed under the
Apache License 2.0 (`LICENSE`, the repository's `LICENSE`, unchanged; the
repository has no `NOTICE`). Two commits are used:

- branch `v3.1-dev`, commit `ef5916de74d7f2e946944409f0d942edd04d106f`
  (2026-11-10): the meta-schema's test documents `tests/schema/pass/*.yaml`
  (35 files) and `tests/schema/fail/*.yaml` (11 files), vendored unchanged
  as `upstream/pass/` and `upstream/fail/`;
- branch `main`, commit `e77481c73e1c703c7de2e4249047b42b8e9b56f4`
  (2024-10-29, the last commit before the repository's `examples` folder
  moved into the test documents): the 3.1 example documents in JSON,
  `examples/v3.1/non-oauth-scopes.json` and
  `examples/v3.1/webhook-example.json`, vendored unchanged as
  `upstream/examples/`.

The files under `invalid/` were written for panproto from the OpenAPI
3.1.2 specification, each breaking one rule; `MANIFEST` lists the rule and
the refusal expected, and lists the upstream documents the reader refuses.
`conformance_openapi.rs` runs every case; the cases the reader does not
pass are listed with their limit in `../openapi-limits.txt`.
