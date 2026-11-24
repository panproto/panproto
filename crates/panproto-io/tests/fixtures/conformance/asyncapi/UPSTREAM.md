# AsyncAPI 3.1.0 example documents

Source: <https://github.com/asyncapi/spec>, licensed under the Apache
License 2.0 (`LICENSE` and `NOTICE`, the repository's files at the commit
below, unchanged). One commit is used: tag `v3.1.0`, commit
`b3fac5bb522771428ea57b16129b273cd3ea0180` (2026-01-31), whose
`spec/asyncapi.md` is the text the reader follows. Every example document
of its `examples/` folder, 28 files, is vendored unchanged as
`upstream/examples/`, keeping the folder structure of the social media
example (`social-media/{backend,comments-service,frontend,
notification-service,public-api}/asyncapi.yaml` and the four
`social-media/common/*.yaml` fragment documents).

The files under `invalid/` were written for panproto from the AsyncAPI
3.1.0 specification, each breaking one rule; `MANIFEST` lists the rule and
the refusal expected. `conformance_asyncapi.rs` runs every case, reading
each social media service as a bundle with the four `common` files as
fragment documents; the cases the reader does not pass are listed with
their limit in `../asyncapi-limits.txt`.
