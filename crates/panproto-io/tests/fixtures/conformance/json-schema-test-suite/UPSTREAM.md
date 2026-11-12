# JSON-Schema-Test-Suite

Source: <https://github.com/json-schema-org/JSON-Schema-Test-Suite>, branch
`main`, commit `38dea932b02986f496345bf007108277777630bf` (2026-11-11),
licensed under the MIT License (`LICENSE`).

Vendored, unchanged, as `tests/<file name>`: every file of
`validation/tests` that holds a test case compatible with the 2020-12
dialect, as the suite's `compatibility` property and the matching rule of
its `validation/README.md` decide. `MANIFEST` lists the 59 files with the
number of such cases (430 in all) and of the instance tests they hold (2082
in all). The subdirectories `validation/tests/optional` and
`validation/tests/proposals` and the `annotations` suite are not vendored.

`conformance_json_schema.rs` reads the schema of every 2020-12 case, with
the case's `externalSchemas` as further documents of the bundle at their
retrieval URIs, writes each document canonically and reads the canonical
text back. The cases the reader does not pass are listed with their limit
in `../json-schema-test-suite-limits.txt`, which also declares the instance
tests, which test a validator and not a reader.
