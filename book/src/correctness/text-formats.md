# Text formats

The text formats carry most configuration and tabular data, and each one fixes, beyond its scalar syntax, how its documents are put together: which key a line of TOML defines, which version of YAML a document is written in. The scalar laws for these routes are on the [Value I/O](value-io.md) page. This page states the laws of document structure that a route must get right before any scalar is read, and names the standard each route reads.

| Format | Standard read | Syntax read as | Data read as |
|---|---|---|---|
| YAML | YAML 1.2.2, one document, core schema | tree | graph |
| TOML | TOML 1.0.0 | tree | tree |
| CSV | RFC 4180, with a header record | table | table |
| TSV | IANA `text/tab-separated-values`, with a header record | table | table |
| Node-link JSON | the node-link form of [`GInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/ginstance/struct.GInstance.html) | graph | graph |

The shapes in the table are the ones each protocol declares in `panproto_protocols::semantics`; a YAML document is a tree as written and a graph once its anchors and aliases are resolved, and the route refuses an alias today. The CSV and TSV dialects are stated in full in the documentation of [`UnifiedCodec`](https://docs.rs/panproto-io/latest/panproto_io/unified_codec/struct.UnifiedCodec.html).

The executable reference for both laws is [`panproto_reference::pages::text_formats`](https://docs.rs/panproto-reference/latest/panproto_reference/pages/text_formats/). It models a document as the sequence of statements or directives that decides its structure, with scalars abstracted away, and the production differentials render each modelled document as text and read it through the preserving route.

## Conformance corpora

Each route is run against a corpus drawn from its standard, vendored under `crates/panproto-io/tests/fixtures/conformance` with its licence and the upstream commit it was taken from. A valid case passes when the route reads it, the tree it reads equals the corpus's expected data, and an unedited emission reproduces its bytes; an invalid case passes when the route refuses it. A case that does not pass is a declared limit: it is listed, with the limit's name, in a file beside the corpus, the test fails if the list and the set of failing cases differ in either direction, and the limit is stated below with a test of its refusal.

The TOML route passes 714 of the 717 cases of the toml-test TOML 1.0.0 list (`conformance_toml_test.rs`). The three others carry one limit, `line-ending-backslash-before-whitespace`: TOML 1.0.0 lets a backslash end a line of a multi-line basic string with spaces or tabs after it, and the tree-sitter TOML grammar admits the backslash only directly before the line break, so the route refuses such a string with a parse error rather than read it.

## TOML keys define each table once {#L-text-formats-1}

### Statement

Model a TOML document as a sequence of statements $s_0, \ldots, s_{n-1}$, each a pair `k1.….km = v`, a header `[k1.….km]` or an array-of-tables header `[[k1.….km]]`, where a pair's value is its statement index. Write $R(s)$ for `resolve_keys(s)`. The preserving TOML route accepts the rendered document exactly when $R(s)$ resolves; it then reads the root table $R(s)$ returns, with each key in first-definition order, each table as a map, each array of tables as a list of maps whatever its length, and each value at its key. When $R(s)$ refuses statement $i$ for reason $r$, the route refuses the document with a parse error that names the key, the class $r$ and the byte offset at which $s_i$ starts. An unedited emission of an accepted document reproduces its bytes.

### Outside the domain

The model has no inline tables, static arrays or scalars other than the pair values, so their interaction with headers (a header that extends an inline table or a static array is refused) is checked by the fixed regression tests and not by the reference. A key path that would nest tables deeper than `MAX_EXTRACT_DEPTH` is refused with `NestingTooDeep` before any table is built, and the model has no depth bound.

### Definitions used

$R$ follows the Keys and Table sections of TOML 1.0.0. Pairs before the first header belong to the root table, and pairs after a header belong to the table it opens. A pair held by table $C$ walks its leading keys from $C$: an absent key becomes a table created by the dotted keys of $C$, a table that the dotted keys of $C$ created is entered, and any other key refuses with `DottedExtension`; the last key must be absent, or the pair refuses with `DuplicateKey`. A header walks its leading keys from the root: an absent key becomes an implicit table, an array of tables is entered at its last element, a table is entered, and a value refuses with `HeaderExtendsValue`. Then `[k]` creates $k$ as a defined table or defines an implicit table $k$, and otherwise refuses with `TableRedefined`; `[[k]]` creates a one-element array of tables or appends an element to an existing one, and otherwise refuses with `ArrayConflict`.

### Proof

The production resolver keeps a nested tree of tables, each tagged with how it was created (root, header, implicit, dotted, inline or array element). We show that its decisions coincide with those of $R$, which keeps the same tables at their absolute paths with, for a dotted table, the table that held the creating pair. Headers agree directly: both walks create implicit tables for absent keys, enter the last element of an array of tables, refuse on a value, and treat the last key by the same three cases. For a pair held by $C$, production enters a leading key exactly when it holds a table tagged dotted, and $R$ exactly when it holds a dotted table whose owner is $C$. A dotted table below $C$ is created only by a pair whose holding table is the table that the pair's walk started from, and the walk starts at $C$ or at a dotted table below it; since no header can open a dotted table (the last key of a header refuses on one), no pair is ever held by a dotted table, so every dotted table reachable from $C$ by a pair's walk was created by a pair held by $C$. The two conditions therefore select the same keys, the refusal reasons follow the same case analysis, and both process statements in source order and stop at the first refusal. Key order agrees because both append a key to its table when the key is created and never move it. The differential checks this equality, the refusal index and class, and the byte-exact unedited emission, on every document of the bounded scope and on generated longer ones.

### Executable counterpart

- Law ID: `text-formats.toml-key-resolution`
- Reference: `panproto_reference::pages::text_formats::toml::resolve_keys`
- Enumerator: `panproto_reference::enumerate::text_formats::toml_documents; bound: statements=3, keys=2, path length=2`
- Production differential test: `toml_key_resolution_matches_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_text_formats --no-tests=fail -E 'test(=toml_key_resolution_matches_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_toml_key_resolution_matches_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_text_formats --no-tests=fail -E 'test(=randomized_toml_key_resolution_matches_the_reference)'`
- Reference test: `toml_key_resolution_follows_the_toml_examples; command: cargo nextest run -p panproto-reference --test text_formats --no-tests=fail -E 'test(=toml_key_resolution_follows_the_toml_examples)'`
- Randomized input scope: up to eight statements, each a pair, header or array-of-tables header over key paths of length one to three on the keys `a`, `b` and `c`.
- File: `crates/panproto-reference/src/pages/text_formats.rs`
- File: `crates/panproto-io/tests/reference_text_formats.rs`
- File: `crates/panproto-io/tests/toml_keys_define_nested_tables.rs`

## YAML directives select the version read {#L-text-formats-2}

### Statement

Model the directives that open a YAML document as a sequence $d_0, \ldots, d_{n-1}$ of `%YAML major.minor` directives, `%TAG` directives and reserved directives, and write $D(d)$ for `read_directives(d)`. The YAML route reads the document under the YAML 1.2 core schema exactly when $D(d)$ accepts, and an unedited emission then reproduces every directive byte. When $D(d)$ refuses directive $i$ for reason $r$, the route refuses the document with a parse error that names the directive, the class $r$ and the byte offset at which $d_i$ starts.

### Outside the domain

The law concerns one document. A stream of several documents is refused before its directives are read, and a document with no content node is refused as well.

### Definitions used

$D$ takes the directives in source order. A `%TAG` directive is refused (`Tag`), since it redefines how tag shorthands expand (YAML 1.2.2 Sec 6.8.2) and the route does not model that. A `%YAML` directive after an earlier one is refused as `Repeated`, which Sec 6.8.1 makes an error; any other `%YAML` directive naming a version other than 1.2 is refused as `Version`. Sec 6.8.1 asks a processor to reject a higher major version and to read version 1.1 or a higher minor version with warnings where the interpretation differs; the route has no warning channel, and reading `yes` or `0777` under the 1.2 core schema would change what a 1.1 author wrote, so it refuses them. A reserved directive carries no meaning (Sec 6.8) and is read past.

### Proof

Production walks the document's children in source order and, for each directive, applies the same three cases in the same order: a second `%YAML` is refused before its version is examined, a first one must name `1.2`, and a `%TAG` is refused. The content node is chosen by its kind, so directives and comments beside it never stand in for content. The differential checks the outcome, the refused index and its class on every sequence of at most three directives over the six-directive alphabet, together with byte-exact unedited emission of every accepted document.

### Executable counterpart

- Law ID: `text-formats.yaml-directive-acceptance`
- Reference: `panproto_reference::pages::text_formats::yaml::read_directives`
- Enumerator: `panproto_reference::enumerate::text_formats::yaml_directive_sequences; bound: length=3, directives=6`
- Production differential test: `yaml_directive_acceptance_matches_the_reference_on_every_bounded_sequence; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_text_formats --no-tests=fail -E 'test(=yaml_directive_acceptance_matches_the_reference_on_every_bounded_sequence)'`
- Reference test: `yaml_directives_are_refused_in_source_order; command: cargo nextest run -p panproto-reference --test text_formats --no-tests=fail -E 'test(=yaml_directives_are_refused_in_source_order)'`
- File: `crates/panproto-reference/src/pages/text_formats.rs`
- File: `crates/panproto-io/tests/reference_text_formats.rs`
- File: `crates/panproto-io/tests/yaml_directives_select_the_version_read.rs`
