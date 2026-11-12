# Third-party material

panproto is under the [MIT licence](LICENSE). The repository also carries material from other projects, each under its own licence, which is kept beside it. Nothing in this list is compiled into a published crate unless the section says so.

## Tree-sitter grammars

The vendored grammars under `grammars/` keep their upstream licences, copied unchanged into `grammars/<name>/LICENSE`. A binary that compiles a grammar in carries that grammar's terms with it; [`crates/panproto-grammars/README.md`](crates/panproto-grammars/README.md#license) lists the grammars whose licences ask for their notice in a binary distribution's documentation. `tools/fetch-grammars.py` refuses a grammar whose licence file is not permissive.

## Conformance corpora

The conformance tests of `panproto-io` read corpora under [`crates/panproto-io/tests/fixtures/conformance`](crates/panproto-io/tests/fixtures/conformance). They are test data only: the crate's `exclude` keeps the directory out of the published package. Each directory's `UPSTREAM.md` records what was taken, from which commit, and how the tests use it. The licence of each was established by fetching the upstream's licence file at the recorded commit, and every licence is permissive; each directory holds the licence text the licence asks to accompany a copy.

| Directory | Upstream | Commit | Licence (as fetched) | Licence text here |
|---|---|---|---|---|
| `ansible` | [geerlingguy/ansible-for-devops](https://github.com/geerlingguy/ansible-for-devops) | `86e1bf80` | MIT | `LICENSE` (upstream `LICENSE`, unchanged) |
| `cloudformation` | [aws-cloudformation/aws-cloudformation-templates](https://github.com/aws-cloudformation/aws-cloudformation-templates) | `a0f43bc6` | Apache-2.0 | `LICENSE` and `NOTICE` (upstream `LICENSE.txt` and `NOTICE.txt`, unchanged) |
| `csv-spectrum` | [max-mapper/csv-spectrum](https://github.com/max-mapper/csv-spectrum) | `d30e80f8` | BSD-2-Clause, declared in `package.json`; no licence file upstream | `LICENSE` (the BSD 2-Clause text with the `package.json` author as copyright holder) |
| `json-ld-api` | [w3c/json-ld-api](https://github.com/w3c/json-ld-api) | `55514738` | W3C Software and Document License, with the W3C test suite dual licensing | `LICENSE.md` (upstream, unchanged) and `LICENSE-W3C-SOFTWARE-AND-DOCUMENT.md` (notice and licence text) |
| `json-schema-test-suite` | [json-schema-org/JSON-Schema-Test-Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite) | `38dea932` | MIT | `LICENSE` (upstream, unchanged) |
| `k8s-crd` | [kubernetes/kubernetes](https://github.com/kubernetes/kubernetes) (tag `v1.37.2`) | `c50e5395` | Apache-2.0 (no `NOTICE` upstream) | `LICENSE` (upstream, unchanged) |
| `postgresql-docs` | [postgres/postgres](https://github.com/postgres/postgres) (tag `REL_17_12`) | `702b3dc8` | PostgreSQL Licence | `COPYRIGHT` (upstream, unchanged) |
| `rdf-tests` | [w3c/rdf-tests](https://github.com/w3c/rdf-tests) | `4158bca1` | W3C Test Suite License and W3C 3-clause BSD License (dual), used under the BSD form | `LICENSE.md` (upstream, unchanged) and `LICENSE` (upstream `rdf/rdf11/rdf-turtle/LICENSE`, unchanged) |
| `sakila` | [jOOQ/sakila](https://github.com/jOOQ/sakila) | `aed53ce6` | BSD-2-Clause | `LICENSE` (upstream, unchanged) |
| `toml-test` | [toml-lang/toml-test](https://github.com/toml-lang/toml-test) | `96fd0884` | MIT | `LICENSE` (upstream, unchanged) |
| `yaml-test-suite` | [yaml/yaml-test-suite](https://github.com/yaml/yaml-test-suite) (`data` branch) | `6ad3d2c6` | MIT (the `main` branch's `License` at `da267a5c`; the `data` branch has no licence file) | `License` (upstream, unchanged) |
| `edgelist`, `bson-spec` | written for panproto from the specifications each case names | none | MIT, as the repository | none needed |

One corpus is not kept in the repository because its licence is not permissive:

| Directory | Upstream | Commit | Licence (as fetched) | How it is used |
|---|---|---|---|---|
| `bson-corpus` | [mongodb/specifications](https://github.com/mongodb/specifications), `source/bson-corpus/tests` | `b8e0ee19` | Creative Commons Attribution-NonCommercial-ShareAlike 3.0 United States | `MANIFEST` records the commit and each file's SHA-256 digest; `python3 tools/fetch-bson-corpus.py` downloads the files into an ignored directory and checks them, and the [corpus gate workflow](.github/workflows/corpus-gate.yml) runs the conformance test on them. Per-PR CI runs the original `bson-spec` set. |

A new corpus is added only under a permissive licence (MIT, BSD, Apache-2.0, ISC, the PostgreSQL Licence, the W3C software, document and test suite licences, CC0 or the public domain), with its licence text, an `UPSTREAM.md` and a row in this table; a corpus under any other licence is fetched at a pinned commit and checked against recorded digests, as the BSON corpus is.
