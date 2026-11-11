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

The executable reference for the page's four laws is [`panproto_reference::pages::text_formats`](https://docs.rs/panproto-reference/latest/panproto_reference/pages/text_formats/). It models a document as the statements, directives or block tree that decide its structure, reads scalars with the route scalar functions of the [Value I/O](value-io.md) page, and the production differentials render each modelled document as text and read it through the preserving route.

## Conformance corpora

Each route is run against a corpus drawn from its standard, vendored under `crates/panproto-io/tests/fixtures/conformance` with its licence and the upstream commit it was taken from. A valid case passes when the route reads it, the tree it reads equals the corpus's expected data, and an unedited emission reproduces its bytes; an invalid case passes when the route refuses it. A case that does not pass is a declared limit: it is listed, with the limit's name, in a file beside the corpus, the test fails if the list and the set of failing cases differ in either direction, and the limit is stated below with a test of its refusal.

The TOML route passes 714 of the 717 cases of the toml-test TOML 1.0.0 list (`conformance_toml_test.rs`). The three others carry one limit, `line-ending-backslash-before-whitespace`: TOML 1.0.0 lets a backslash end a line of a multi-line basic string with spaces or tabs after it, and the tree-sitter TOML grammar admits the backslash only directly before the line break, so the route refuses such a string with a parse error rather than read it.

The YAML route passes 296 of the 402 cases of the yaml-test-suite data release (`conformance_yaml_test_suite.rs`). It reads scalars through a whole-document decoder, libyaml as `yaml_serde` packages it, walked beside the CST so that each scalar takes its meaning in place: a block scalar its indentation and final line break, a scalar its anchor and tag. A document whose structure the decoder and the CST read differently is refused, never read with either structure. The 106 other cases fall under these limits.

| Limit | Cases | What the route does |
|---|---|---|
| `no-json-form` | 29 | The case is valid YAML with no JSON form, so the suite gives no expected data. The route refuses 28 of them under the limits below and reads one, a lone `!`. |
| `multiple-documents` | 20 | Refuses a stream of more than one document: one document is one instance. |
| `alias` | 14 | Refuses an alias, which would make the tree a graph. |
| `container-tag` | 9 | Refuses a tag on a mapping or sequence with `UnsupportedTaggedContainer`. |
| `decoder-refuses` | 7 | Refuses a document libyaml does not decode and whose scalars do not all read the same alone (no block scalar, anchor, tag or multi-line flow scalar); when they do, it reads the document scalar by scalar. |
| `decoder-disagrees` | 6 | Refuses a scalar where libyaml departs from YAML 1.2.2: an anchor name outside `[A-Za-z0-9_-]`, a `?` inside a flow collection, a single-line scalar decoded differently in place and alone, and a block scalar ending the stream on a line of spaces. |
| `tag-directive` | 5 | Refuses a `%TAG` directive ([L-text-formats-2](#L-text-formats-2)). |
| `no-document` | 5 | Refuses a stream with no document. |
| `version-directive` | 4 | Refuses `%YAML 1.1` ([L-text-formats-2](#L-text-formats-2)). |
| `local-tag-kept` | 2 | Reads a scalar with a local tag such as `!foo` as an opaque value that keeps the tag, which the suite's JSON drops. |
| `accepts-invalid` | 2 | Reads two invalid streams that both the tree-sitter grammar and libyaml accept: a block scalar whose leading empty lines hold more spaces than its indentation allows, and a directive inside a document, after `---` with no `...` before it. |
| `binary-tag-read-as-bytes` | 1 | Reads a `!!binary` scalar as the bytes it encodes, where the suite's JSON keeps the base64 text. |
| `structure-not-read` | 1 | Refuses `[?x]`, which the tree-sitter grammar reads as an explicit key. |
| `complex-key` | 1 | Refuses a mapping key that is a collection. |

The CSV route reads every one of the 12 csv-spectrum cases (`conformance_csv_spectrum.rs`) as its records, each field as text, and writes each back byte for byte. The case `location_coordinates` gives in its JSON a contact number its CSV file does not hold; the test expects the file's number. The TSV route is checked on the same records written tab-separated, for the 9 cases whose fields hold no tab or line break. The TSV route reads its lines and tabs from the source rather than from the grammar's nodes, since the tree-sitter TSV grammar counts a tab among its extras and so gives an empty field no node.

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

## A TOML document reads as its tables and scalars {#L-text-formats-3}

### Statement

For a TOML statement sequence $s$ with value tokens $t$, write $D(s,t)$ for `read_document(s, t)`. When $D(s,t)$ reads, the preserving TOML route reads the rendered document as exactly the tables $D$ gives, keys in first-definition order and each value the TOML scalar function's reading of its token, and an unedited emission reproduces the document's bytes. When $D(s,t)$ refuses a key, the route refuses the document naming the same statement and class. When $D(s,t)$ refuses only a scalar, the route refuses the document.

### Outside the domain

When a document holds both a refused key and a refused scalar, the law states only that the route refuses it; which of the two refusals each side names first is not compared. Values other than integers, strings and booleans lie outside the bounded scope, and their reading is the subject of [L-value-io-4](value-io.md#L-value-io-4).

### Definitions used

$D$ composes [L-text-formats-1](#L-text-formats-1) with the TOML scalar function $P_{\mathrm{toml}}$ of [L-value-io-4](value-io.md#L-value-io-4): it resolves the statements with `resolve_keys`, and reads each pair's token with $P_{\mathrm{toml}}$.

### Proof

Production resolves the keys of the whole document before reading any value, and reads each value from its own token, so its tables are those of [L-text-formats-1](#L-text-formats-1) and its values those of the scalar route; the composition is therefore $D$ wherever both laws hold. Neither law covers the interaction of the two (a scalar read under a dotted key, in a sub-table, or as an element of an array of tables), so the differential checks the composition directly: on every document of at most two statements over the key paths of length one or two on `a` and `b`, each pair carrying one of six tokens (an integer, a basic and a literal string, a boolean, a hexadecimal integer and the refused `01`), it compares the tables read, the refusal and its class, and the bytes of an unedited emission.

### Executable counterpart

- Law ID: `text-formats.toml-document-reading`
- Reference: `panproto_reference::pages::text_formats::toml::read_document`
- Enumerator: `panproto_reference::enumerate::text_formats::toml_token_documents; bound: statements=2, keys=2, path length=2, tokens=6`
- Production differential test: `toml_documents_read_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_text_formats --no-tests=fail -E 'test(=toml_documents_read_as_the_reference_on_every_bounded_document)'`
- Reference test: `toml_documents_read_their_tables_and_scalars; command: cargo nextest run -p panproto-reference --test text_formats --no-tests=fail -E 'test(=toml_documents_read_their_tables_and_scalars)'`
- File: `crates/panproto-reference/src/pages/text_formats.rs`
- File: `crates/panproto-io/tests/reference_text_formats.rs`

## A block YAML document reads as written {#L-text-formats-4}

### Statement

For a document tree $n$ of scalar tokens, mappings and sequences, write $R(n)$ for `render_block(n)` and $Y(n)$ for `read_block_document(n)`. When $Y(n)$ reads, the YAML route reads $R(n)$ as exactly $Y(n)$: each mapping its entries in order, each sequence its items in order, and each scalar the YAML scalar function's reading of its token. An unedited emission reproduces $R(n)$.

### Outside the domain

The top-level empty scalar renders as an empty stream, which has no document, and lies outside the scope. Flow collections, multi-line scalars, tags, anchors and aliases are not rendered by $R$; the conformance corpus above covers them.

### Definitions used

$R$ writes block style: a `key:` line per mapping entry and a `-` line per sequence item, a scalar after one space on its indicator's line, an empty collection as `{}` or `[]`, and any other collection on the following lines two columns further in. $Y$ reads scalars with $P_{\mathrm{yaml}}$ of [L-value-io-4](value-io.md#L-value-io-4).

### Proof

The YAML route walks the CST of $R(n)$, whose block mappings and sequences are those $R$ wrote, and reads each scalar from the whole document's decoding, which for a one-line scalar of the scope is the YAML 1.2 core reading that $P_{\mathrm{yaml}}$ defines. The differential compares the two readings and the unedited emission on every document of depth at most two over the tokens `-1`, `yes`, `'q'` and the empty scalar, with mappings of one or two of the keys `a` and `b` and sequences of at most two items: 13,271 documents.

### Executable counterpart

- Law ID: `text-formats.yaml-block-document-reading`
- Reference: `panproto_reference::pages::text_formats::yaml::render_block`
- Reference: `panproto_reference::pages::text_formats::yaml::read_block_document`
- Enumerator: `panproto_reference::enumerate::text_formats::yaml_block_documents; bound: depth=2, keys=2, items=2, tokens=4`
- Production differential test: `yaml_block_documents_read_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_text_formats --no-tests=fail -E 'test(=yaml_block_documents_read_as_the_reference_on_every_bounded_document)'`
- Reference test: `yaml_block_documents_render_and_read_as_written; command: cargo nextest run -p panproto-reference --test text_formats --no-tests=fail -E 'test(=yaml_block_documents_render_and_read_as_written)'`
- File: `crates/panproto-reference/src/pages/text_formats.rs`
- File: `crates/panproto-io/tests/reference_text_formats.rs`
