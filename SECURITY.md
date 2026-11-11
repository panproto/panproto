# Security policy

## Supported versions

panproto is pre-1.0. Security fixes are released for the latest published minor version only.

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open the repository's
[Security tab](https://github.com/panproto/panproto/security) and choose "Report a vulnerability". Do not open a public
issue for a suspected vulnerability.

A report is most useful when it includes the affected crate or package and version, a minimal input or program that
reproduces the problem, and the impact you observed (for instance, a panic on untrusted input, memory unsafety across
the C or WASM boundary, or a repository operation that reads or writes outside its directory).

You can expect an acknowledgement within seven days. Once a fix is released, the advisory is published with credit to
the reporter unless the reporter asks otherwise.

## Scope

In scope: the Rust crates under `crates/`, the bindings under `bindings/`, the `schema` CLI, and the release artifacts
published from this repository. Parsers and codecs are expected to return errors, never to panic or exhaust memory, on
arbitrary input; a counterexample is a valid report.
