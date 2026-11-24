# RDF formats

RDF data is a graph from the start: a set of triples whose subjects and objects are shared terms. Reading it into a panproto graph instance, rather than through a tree that encodes the graph, lets lenses and validation work on the triples themselves. This page states which RDF syntaxes panproto reads, how a document becomes a graph instance, what each reader refuses, what each writer prints, and the laws the readers and writers satisfy.

## Standards and shapes

The `ntriples` protocol (alias `n-triples`) reads and writes N-Triples as the W3C Recommendation *RDF 1.1 N-Triples* of 25 February 2014 defines it, over the abstract syntax of *RDF 1.1 Concepts and Abstract Syntax* of the same date. Its syntax and its data are both read as a graph: the protocol declares a graph syntactic shape and a graph semantic shape, and its graph codec, [`NTriplesCodec`](https://docs.rs/panproto-io/latest/panproto_io/rdf/ntriples/struct.NTriplesCodec.html), is registered by `default_registry` under the `tree-sitter` feature of `panproto-io`.

The `turtle` protocol (alias `ttl`) reads Turtle as the W3C Recommendation [*RDF 1.1 Turtle*](https://www.w3.org/TR/2014/REC-turtle-20140225/) of 25 February 2014 defines it, into the same graph encoding, and resolves relative IRI references as [RFC 3986](https://www.rfc-editor.org/rfc/rfc3986) Sec 5.2 resolves them. Its syntax and its data are likewise both read as a graph, and its graph codec, [`TurtleCodec`](https://docs.rs/panproto-io/latest/panproto_io/rdf/turtle/struct.TurtleCodec.html), is registered under the same feature.

The `jsonld` protocol (alias `json-ld`) reads JSON-LD as the W3C Recommendations [*JSON-LD 1.1*](https://www.w3.org/TR/2020/REC-json-ld11-20200716/) and [*JSON-LD 1.1 Processing Algorithms and API*](https://www.w3.org/TR/2020/REC-json-ld11-api-20200716/) of 16 July 2020 define it, in the processing mode `json-ld-1.1`, into the same graph encoding. A JSON-LD document is a JSON tree, which the reader reads straight into the graph it denotes, so the protocol declares a graph syntactic shape and a graph semantic shape; its graph codec, [`JsonLdCodec`](https://docs.rs/panproto-io/latest/panproto_io/rdf/jsonld/struct.JsonLdCodec.html), is registered under the same feature.

| Format | Standard | Read as | Status |
|---|---|---|---|
| N-Triples | RDF 1.1 N-Triples (W3C Recommendation, 2014-02-25) | graph | Read and written; the limits below |
| Turtle | RDF 1.1 Turtle (W3C Recommendation, 2014-02-25) | graph | Read, written canonically, and read and written through a format-preserving route; the limits below |
| JSON-LD | JSON-LD 1.1 and JSON-LD 1.1 Processing Algorithms and API (W3C Recommendations, 2020-07-16), processing mode `json-ld-1.1` | JSON tree, read as a graph | Read with the subset and refusals below, written canonically, and read and written through a format-preserving route |
| N-Quads | RDF 1.1 N-Quads (W3C Recommendation, 2014-02-25) | | Not supported: no bundled tree-sitter grammar reads N-Quads, and the Turtle grammar refuses each of the 12 positive syntax tests of the W3C N-Quads suite that carries a graph label (see [N-Quads](#n-quads)) |
| TriG, RDF/XML | | | Not supported; the Turtle reader refuses a TriG graph block with `NotInFormat` |
| JSON-LD named graphs, remote contexts, framing, JSON-LD 1.0 processing | | | Not supported: the JSON-LD reader refuses a named graph with `NamedGraph` and a remote context with `Unsupported`, and offers neither framing nor the 1.0 mode |
| RDF-star, RDF 1.2 triple terms | | | Not supported: the reader refuses `<<` with `Syntax` |

## The RDF graph encoding

Every RDF route reads into one graph encoding, which the schema [`rdf::ntriples::graph_schema`](https://docs.rs/panproto-protocols/latest/panproto_protocols/rdf/ntriples/fn.graph_schema.html) declares. The schema has five vertices, each named for its kind, and eight `prop` edges.

| Vertex | One node per | Value |
|---|---|---|
| `iri` | distinct IRI | the IRI, its escapes decoded |
| `blank` | distinct blank node of the document | the label the document writes |
| `literal` | distinct literal | the lexical form, its escapes decoded |
| `language` | distinct language tag | the tag as written |
| `triple` | distinct triple | none |

A `triple` node has a `subject` edge to an `iri` or `blank` node, a `predicate` edge to an `iri` node and an `object` edge to an `iri`, `blank` or `literal` node. A `literal` node has a `datatype` edge to the `iri` node of its datatype and, when it is a language-tagged string, a `language` edge to its tag. A triple is a node rather than an edge so that a predicate is a term like any other: the IRI that is the predicate of one triple is the same node as the IRI that is the subject of another.

Values are kept exactly. A lexical form is never canonicalised (`"01"^^xsd:integer` keeps `01`), and a language tag keeps its case, which RDF 1.1 Concepts permits since it compares tags character by character. A literal written with no datatype has the datatype `xsd:string`, and a language-tagged literal the datatype `rdf:langString`, as RDF 1.1 gives them; `"x"` and `"x"^^xsd:string` are therefore one term. Blank node labels scope to one document: a graph read from one document keeps that document's labels as presentation, and the writer prints them back.

An RDF graph is a set, so a document that states one triple twice denotes the graph that holds it once. The reader reads a repeated triple as the one `triple` node it already has, and a repeated term as its one node. Nodes are numbered from 0 in the order the document first mentions them: for each new triple, its `triple` node, then its subject, its predicate and its object, a new literal followed by its datatype and its tag.

## What the N-Triples reader reads and refuses

The reader parses the document with the bundled tree-sitter Turtle grammar, since N-Triples is a subset of Turtle, and walks the tree. Every refusal is a [`ParseInstanceError::Rdf`](https://docs.rs/panproto-io/latest/panproto_io/error/enum.ParseInstanceError.html) carrying an [`RdfReadError`](https://docs.rs/panproto-io/latest/panproto_io/rdf/struct.RdfReadError.html) with the byte offset of the token at fault, and the reader stops at the first fault in document order.

| Input | Refusal kind |
|---|---|
| Bytes that are not UTF-8 | `NotUtf8`, at the first invalid byte |
| A raw U+0000 or U+007F anywhere | `UnsupportedCharacter`: the grammar cannot read either inside a string |
| A directive, prefixed name, `a`, number, boolean, single-quoted or long string, blank node property list, collection, predicate list or object list | `NotInFormat`, naming the construct |
| A triple spanning a line break, or two triples on one line | `LineStructure` |
| An IRI with no scheme | `RelativeIri` |
| An IRI holding, raw or escaped, a character `IRIREF` excludes (U+0000 to U+0020, `<`, `>`, `"`, `{`, `}`, `|`, `^`, backquote, backslash) | `IriCharacter` |
| An escape the format does not define, or a numeric escape naming a surrogate or a code point above U+10FFFF | `InvalidEscape`, `NotAScalarValue` |
| A malformed language tag or blank node label | `InvalidLanguageTag`, `InvalidBlankLabel` |
| Anything else the grammar does not read | `Syntax` |

Two of these go beyond the grammar of the Recommendation. An escaped IRI character that `IRIREF` excludes raw is refused, because RFC 3987 admits no such character in an IRI and the canonical form could not write it back; the W3C Turtle suite tests the same refusal (`turtle-syntax-bad-uri-escape-02`, an IRI holding `\u003C`). A raw U+0000 or U+007F is refused anywhere, because the grammar cannot read it inside a string; this is the one limit the conformance corpus shows.

## Canonical output

[`emit_graph`](https://docs.rs/panproto-io/latest/panproto_io/registry/struct.ProtocolRegistry.html#method.emit_graph) writes a graph as canonical N-Triples: one line per triple, the lines in ascending code point order, each the subject, the predicate and the object separated by one space and followed by ` .` and a line feed. An IRI is written between angle brackets with no escape. A blank node is written as `_:` and its label: blank nodes are not relabelled, so the output is canonical for a graph with its labels, and two graphs that differ only in their labels are written differently (RDF Dataset Canonicalization is not implemented). A literal of datatype `xsd:string` is written with no datatype, a language-tagged literal with its tag as read, and any other literal with `^^` and its datatype. A string escapes backspace, tab, line feed, form feed, carriage return, `"` and backslash as `\b`, `\t`, `\n`, `\f`, `\r`, `\"` and `\\`, writes U+0000 to U+0007, U+000B, U+000E to U+001F, U+007F, U+FFFE and U+FFFF as `\u` and four uppercase hexadecimal digits, and writes every other character as itself. These string rules are those of the canonical form in the W3C Working Draft *RDF 1.2 N-Triples* of 5 November 2026; unlike that draft, the writer keeps a language tag's case, since the reader keeps it.

The writer refuses, with an [`EmitInstanceError::Rdf`](https://docs.rs/panproto-io/latest/panproto_io/error/enum.EmitInstanceError.html) carrying an [`RdfWriteError`](https://docs.rs/panproto-io/latest/panproto_io/rdf/struct.RdfWriteError.html) that names the node at fault, every graph that no document reads back as: a node anchored outside the encoding, a value that does not fit its anchor, a node carrying data the encoding has no place for (a discriminator, extra fields, annotations, a position or a shape), an edge that is not an edge of the encoding, a missing or repeated required edge, a language tag without `rdf:langString` or the converse, two nodes holding one term or one triple (which a document would merge), a node no triple reaches (which a document would drop), and a malformed IRI, label or tag. Collapsing two nodes is thus reported rather than done silently. The route claims no format preservation: comments, layout, the order of triples and the escapes a document chose are not kept, and an unedited round trip writes the canonical text.

## Conformance corpus

The reader is run against the W3C RDF 1.1 N-Triples test suite, vendored under `crates/panproto-io/tests/fixtures/conformance/rdf-tests` with its licence and upstream commit (`conformance_rdf_tests.rs`). A positive syntax test passes when the route reads the file, its canonical output reads back as a graph of the same size and writing that graph again gives the same bytes; a negative syntax test passes when the route refuses the file. An entry that does not pass is a declared limit in `rdf-tests-limits.txt` with the class it shows, and the test fails if the list and the failing entries differ in either direction. The route passes 69 of the 70 entries: all 29 negative tests and 40 of the 41 positive ones.

| Limit | Entries | What the reader does |
|---|---|---|
| `raw-nul-or-delete` | 1 (`literal_ascii_boundaries`) | Refuses the raw U+0000 and U+007F in its string with `UnsupportedCharacter` |

The tree-sitter Turtle grammar alone, run over the suite, reads 40 of the 41 positive tests and rejects 15 of the 29 negative ones; the walker refuses the 14 it over-accepts.

The Turtle route is run against the W3C RDF 1.1 Turtle test suite, vendored beside it under `rdf11/rdf-turtle`: its manifest and the 430 other `.ttl` and `.nt` files, without the archives and implementation reports of the upstream directory. Each file is read with the base the manifest assumes for it, `https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-turtle/` followed by the file name. A positive syntax test passes when the route reads the file, the format-preserving route reads the same graph and writes the unedited graph back as the file's bytes, and canonical Turtle reads back as the same triples; an evaluation test passes when, in addition, the triples are isomorphic to those the N-Triples route reads from the expected result; a negative syntax test passes when the route refuses the file. Isomorphism is decided exactly: ground triples are compared as sets, blank nodes are coloured by the triples they occur in and the colours refined until the partition stops growing, and a bijection between blank nodes of equal colour is searched for by backtracking, each partial assignment checked against the triples it fixes; a search that reaches its bound of one million assignments counts as a failure. The route passes 307 of the 313 entries: all 94 negative tests, all 74 positive syntax tests and 139 of the 145 evaluation tests.

| Limit | Entries | What the reader does |
|---|---|---|
| `raw-nul-or-delete` | 5 (`LITERAL1_all_controls`, `LITERAL1_ascii_boundaries`, `LITERAL2_ascii_boundaries`, `LITERAL_LONG1_ascii_boundaries`, `LITERAL_LONG2_ascii_boundaries`) | Refuses the raw U+0000 in its string with `UnsupportedCharacter` |
| `grammar-syntax` | 1 (`turtle-subm-02`) | Refuses with `Syntax`: the grammar reads the `a` of the prefixed name `a:a` as the keyword `a`, where the Recommendation's longest-match rule reads the prefixed name |

The tree-sitter grammar alone reads 213 of the 219 positive and evaluation tests and rejects 77 of the 94 negative ones; the walker refuses the 17 it over-accepts: three IRIs whose escapes name an excluded character, two undefined prefixes, a TriG graph block, an N3 `is ... of` path and ten numeric escapes naming a surrogate.

The JSON-LD route is run against the `toRdf` tests of the W3C JSON-LD 1.1 API test suite, vendored under `json-ld-api` with its licence and upstream commit: the manifest, every file of `tests/toRdf` and the one input of `tests/expand` the manifest names (`conformance_jsonld_api.rs`). An entry whose options ask for JSON-LD 1.0 processing, generalized RDF, an `rdfDirection` form or an `expandContext` is not run and counts as a limit of that class. Every other entry is read with the base its options give, or the manifest's `baseIri` followed by the input's path. A positive evaluation test passes when the route reads the input, the triples are isomorphic (by the exact check of the Turtle corpus) to those the N-Triples route reads from the expected N-Quads, the format-preserving route reads the same graph and writes the unedited graph back as the input's bytes, and canonical JSON-LD reads back as the same triples; a negative test passes when the route refuses the input with the API error code the manifest expects; a positive syntax test passes when the route reads the input. The route passes 213 of the 467 entries: 149 of the 345 positive evaluation tests, 48 of the 106 negative tests and all 16 positive syntax tests. Every other entry is a declared limit in `json-ld-api-limits.txt`, and the test fails if the list and the failing entries differ in either direction.

| Limit | Entries | What the reader does |
|---|---|---|
| `unsupported-scoped-context` | 35 | Refuses a term definition's `@context` with `Unsupported` |
| `unsupported-@protected` | 34 | Refuses `@protected` with `Unsupported` |
| `unsupported-@container-@graph`, `-@index`, `-@type`, `-@language`, `-@id` | 30, 12, 10, 7, 4 | Refuses the container mapping with `Unsupported` |
| `unsupported-@json` | 23 | Refuses a JSON literal with `Unsupported` |
| `json-ld-1.0` | 19 | Not run: the entry asks for JSON-LD 1.0 processing |
| `unsupported-@nest`, `-@import`, `-@included`, `-@direction`, `-@index`, `-@propagate` | 13, 9, 8, 7, 3, 2 | Refuses the keyword with `Unsupported` |
| `named-graph` | 12 | Refuses a named graph with `NamedGraph` |
| `unsupported-remote-context` | 6 | Refuses a remote context with `Unsupported` |
| `refused-iri-character`, `refused-relative-iri`, `refused-malformed-iri` | 5, 2, 2 | Refuses the IRI the standard drops |
| `rdf-direction-option`, `expand-context-option` | 4, 1 | Not run: the entry asks for an option the reader does not offer |
| `refused-free-floating` | 3 | Refuses the free-floating value the standard drops |
| `refused-reserved-keyword`, `refused-language-tag`, `refused-blank-predicate` | 1, 1, 1 | Refuses the value, tag or property the standard ignores or drops |

## N-Quads {#n-quads}

No bundled grammar reads N-Quads. N-Quads is N-Triples with an optional graph label (an IRI or a blank node) before the final `.`, and the vendored Turtle grammar has no production for a fourth term, so reading N-Quads through tree-sitter needs one of: an N-Quads grammar; a TriG grammar, which would still not read N-Quads, whose graph label follows the triple; or an extension of the vendored Turtle grammar that admits an optional graph label before the `.` of a triple. A survey of maintained grammars found [`tree-sitter-rdf`](https://github.com/symbolith/tree-sitter-rdf) (commit `a168f6be`, 2026-11-10), by the author of the vendored Turtle grammar, which holds an RDF 1.1 N-Quads grammar among nine RDF grammars. Its package metadata declares the MIT licence, but the repository holds no licence file and declares no licence of its own. Run over the W3C N-Quads suite, it reads 52 of the 53 positive syntax tests (all but `literal_ascii_boundaries`, the raw U+0000 case N-Triples shows) and rejects 29 of the 34 negative ones. The route waits on a decision to vendor it.


## What the Turtle reader reads and refuses

The Turtle reader parses the document with the same bundled tree-sitter Turtle grammar and walks the tree in document order. A codec made with [`TurtleCodec::new`](https://docs.rs/panproto-io/latest/panproto_io/rdf/turtle/struct.TurtleCodec.html#method.new) starts with no base IRI and refuses every relative IRI reference with `RelativeIri`; one made with `with_base` starts from the base it is given, which RFC 3986 Sec 5.1.3 makes the document's retrieval IRI, and `with_base` refuses a base that is not an absolute IRI with `InvalidBaseIri`. Each `@base` or `BASE` directive replaces the base from the point it appears, resolved against the base before it.

| Construct | Reading |
|---|---|
| `@prefix` and `PREFIX` | The prefix maps to its IRI, resolved against the base in force, from this point on; a later directive for the same prefix replaces it |
| IRI reference `<...>` | Its escapes decoded; when it is relative, resolved against the base in force by [`resolve_iri`](https://docs.rs/panproto-io/latest/panproto_io/rdf/turtle/fn.resolve_iri.html) |
| Prefixed name | The prefix's IRI followed by the local name, each `\` escape of `PN_LOCAL_ESC` replaced by its character and each `%` escape kept as written |
| `a` | `rdf:type` |
| `;` and `,` | One triple per predicate and object, with the subject (and the predicate) repeated |
| `[]`, `[ ... ]` | A blank node with a fresh label; a property list's triples have it as subject |
| `( ... )` | `rdf:nil` when empty; otherwise one blank node per element, each with an `rdf:first` triple to its element and an `rdf:rest` triple to the next cell or to `rdf:nil` |
| Strings | All four forms (`"..."`, `'...'`, `"""..."""`, `'''...'''`), escapes decoded |
| Integer, decimal, double, `true`, `false` | A literal whose lexical form is the token exactly as written, of datatype `xsd:integer`, `xsd:decimal`, `xsd:double` or `xsd:boolean` |

Each blank node written without a label gets a fresh label: `b` followed by a decimal counter that starts at 0 and advances on every allocation, skipping every label the document writes, so a fresh label never names a labelled node. A blank node property list's label is allocated at its `[`, and a collection allocates all its cells' labels at its `(`, first to last. Triples are listed in the order the walk completes them: the triples of an object (its property list or its collection) come before the triple that has it as object, a subject's own triples come before its first predicate, and a collection's triples go cell by cell, each element's own triples followed by the cell's `rdf:first` and `rdf:rest` triples.

The reader refuses with the kinds of the N-Triples table above, where they apply, and in addition:

| Input | Refusal kind |
|---|---|
| A prefixed name whose prefix no earlier directive declares | `UndefinedPrefix`, naming the prefix |
| A relative IRI reference with no base in force | `RelativeIri` |
| A TriG graph block `{ ... }`, which the grammar reads | `NotInFormat`, naming "a graph block" |
| Anything else the grammar does not read | `Syntax` |

When the grammar cannot read the document, the reader walks every statement that ends before the earliest region the parser marked as unread, then checks each term token before that region on its own (an IRI's escapes and resolution, a prefixed name's prefix, a string's escapes, a label's form), and only then reports `Syntax` at the start of the region, so the refusal names the first fault in document order.

## Canonical Turtle

[`emit_graph`](https://docs.rs/panproto-io/latest/panproto_io/registry/struct.ProtocolRegistry.html#method.emit_graph) for `turtle`, and [`rdf::turtle::write`](https://docs.rs/panproto-io/latest/panproto_io/rdf/turtle/fn.write.html), write a graph as canonical Turtle: no directives; one statement per subject, the subjects in ascending code point order of their N-Triples text; within a statement the predicates in ascending order of their text `<...>`, separated by ` ;`, a line feed and four spaces; after each predicate its objects in ascending order of their text, separated by `, `; each statement ended by ` .` and a line feed. Every term is written as canonical N-Triples writes it, with the same escapes, so canonical Turtle needs no base IRI and reads back as the same triples. The writer refuses exactly the graphs the N-Triples writer refuses, for the same reasons.

## The format-preserving Turtle route

[`TurtleCodec::parse_graph_preserving`](https://docs.rs/panproto-io/latest/panproto_io/rdf/turtle/struct.TurtleCodec.html#method.parse_graph_preserving) reads a document into the graph the native route reads, together with a [`TurtleComplement`](https://docs.rs/panproto-io/latest/panproto_io/rdf/turtle/struct.TurtleComplement.html) that holds what the triples do not: the CST schema of the document (its directives, prefixed names, abbreviations, comments, white space and the order of its statements), the base IRI, the graph as read, and for each term node the tokens that write it and whether the document also writes it somewhere with no token of its own. [`emit_graph_preserving`](https://docs.rs/panproto-io/latest/panproto_io/rdf/turtle/struct.TurtleCodec.html#method.emit_graph_preserving) writes a graph back into those bytes, following the CST complement of the [text formats](text-formats.md).

An unedited graph is written as the document's bytes. An edit to the value of a term node rewrites each token that writes that term and nothing else:

| Edited node | Token | Written as |
|---|---|---|
| `iri` | IRI reference, prefixed name or `a` | `<` the IRI `>` |
| `blank` | label | `_:` and the label |
| `literal` | quoted string | the canonical N-Triples string; the tag or datatype token is kept |
| `literal` | integer, decimal, double or boolean | the new lexical form when it is a token of the same form, and otherwise the canonical string followed by `^^` and the datatype |
| `language` | tag | `@` and the tag |

The route refuses, with an `RdfWriteError` naming the node, every edit it cannot carry in place. A graph that differs from the one read in more than node values (a node added or removed, an anchor changed, an edge added, removed or moved) is refused with `StructuralEdit`. A graph no document reads back as is refused as the canonical writer refuses it, so an edit that makes two terms equal is refused with `DuplicateTerm` or `DuplicateTriple` rather than merging them. A changed term the document writes in some place without a token is refused with `ImplicitTerm`: the datatype a plain string, a tagged string or a bare number or boolean implies, `rdf:first`, `rdf:rest` and `rdf:nil` of a collection, and a blank node written as `[]`, a property list or a collection cell. A label holding `:`, which Turtle cannot write, is refused with `InvalidBlankLabel`. The native and the preserving route read the same graph from every document.

## What the JSON-LD reader reads and refuses

The JSON-LD reader parses the document with the bundled tree-sitter JSON grammar, the grammar the JSON text route reads with, and builds the JSON value from the tree: each string token is decoded as RFC 8259 decodes it, each number token is checked against the RFC 8259 number grammar, and a comment (which the grammar admits), white space other than space, tab, line feed and carriage return, more than one top-level value and nesting deeper than 128 arrays and objects are refused. It then runs the Expansion algorithm (API Sec 5.1, with Context Processing, Create Term Definition, IRI Expansion and Value Expansion of Sec 4.1, 4.2, 5.2 and 5.3) and reads the triples of the expanded default graph as Deserialize JSON-LD to RDF (Sec 8.1 to 8.3) does. The processing mode is pinned to `json-ld-1.1`; a context's `@version` must be `1.1`.

Contexts are inline. A codec made with [`JsonLdCodec::new`](https://docs.rs/panproto-io/latest/panproto_io/rdf/jsonld/struct.JsonLdCodec.html#method.new) reads with no base IRI, and one made with `with_base` reads with the base it is given, which is the API's `documentUrl`. An `@base` entry changes the base for the rest of its context's scope, and `null` in a context resets the active context to the initial one. The reader performs no network access: a context given as a string (a remote context) is refused.

| Feature | Reading |
|---|---|
| Context entries | `@base`, `@vocab` (expanded against the vocabulary mapping before it, then the base), `@language`, `@version` |
| Term definitions | A string, `null`, or a map with `@id`, `@reverse`, `@type` (an IRI, `@id`, `@vocab` or `@none`), `@container` (`@list` or `@set`), `@language` and `@prefix`; compact IRIs, prefixes and keyword aliases; the `@type` term with `@container: @set` |
| Node objects | `@id`, `@type`, properties, `@reverse`, and `@graph` on a top-level object that holds nothing else |
| Values | Strings, numbers and booleans; value objects with `@value`, `@type` and `@language`; `@list` (lists of lists included) and `@set` |
| Blank node identifiers | `_:` and a label: an N-Triples blank node label is kept, and any other identifier gets a fresh label |

Each blank node without a kept label gets a fresh label, `b` and a decimal counter from 0 that skips every label the document keeps. Triples are listed in the order the walk over the expanded items completes them. A node object's label is allocated where the walk reaches it; its `@type` triples come first, then each property's values in document order, each value's own triples before the triple that has it as object, then the reverse properties likewise. A list allocates all its cells' labels when reached, then gives each element's triples followed by the cell's `rdf:first` and `rdf:rest` triples. The API's node map relabels every blank node, so the triples are those of the API up to a renaming of blank nodes.

A JSON number denotes the binary64 value nearest its decimal text, as the API's JavaScript reading of JSON gives it. Following API Sec 8.6, a value with no fractional part and a magnitude below $10^{21}$, not typed `xsd:double`, is written as the decimal integer it is (`-0` is `0`), with the datatype `xsd:integer` unless a value object or a term gives one. Any other value is written as the API's `toExponential(15)` writes it: rounded to 16 significant digits with a tie rounded away from zero, as one non-zero digit, a point, the remaining digits without trailing zeros (at least one), `E` and the exponent without a plus sign or leading zeros, with the datatype `xsd:double` unless one is given; zero is `0.0E0`. So `1.0` reads as `"1"^^xsd:integer`, `1e21` as `"1.0E21"^^xsd:double` and `1.0000152587890625` as `"1.000015258789063E0"^^xsd:double`. A boolean reads as `true` or `false` of `xsd:boolean`. A language tag keeps its case.

The standard drops some data silently, and the reader follows it in two places. A key that expands to no IRI, blank node identifier or keyword is dropped (Expansion step 13.3): with no `@vocab`, a key such as `name` that no term defines denotes nothing, and so does a key of the form `@` and letters that is no keyword. A `null` value, a value object whose `@value` is `null` and a map holding only `@language` denote no value. A node object with no property denotes no triple, as in the API. Everywhere else the reader refuses what the standard would drop.

Every refusal is a [`ParseInstanceError::Rdf`](https://docs.rs/panproto-io/latest/panproto_io/error/enum.ParseInstanceError.html) carrying an [`RdfReadError`](https://docs.rs/panproto-io/latest/panproto_io/rdf/struct.RdfReadError.html). A JSON-level fault has the kind `NotUtf8`, `Syntax`, `NotInFormat` (a comment) or `NestingLimit`. A JSON-LD refusal has the kind `JsonLd`, which carries a [`JsonLdRefusal`](https://docs.rs/panproto-io/latest/panproto_io/rdf/jsonld/enum.JsonLdRefusal.html) and the JSON Pointer (RFC 6901) of the value at fault; its offset is that value's first byte.

| Input | Refusal |
|---|---|
| A document the API rejects | `Error`, with the API's error code (`invalid IRI mapping`, `cyclic IRI mapping`, `invalid value object` and the other codes of Sec 9.4.2 the subset can meet) |
| A remote context, `@import`, `@protected`, `@propagate`, `@direction`, `@nest`, `@included`, `@index`, `@json`, a scoped context, a container other than `@list` and `@set`, any other keyword in a node object, `@language` in a node object | `Unsupported`, naming the feature |
| An object repeating a key | `DuplicateKey`: JSON leaves its meaning open |
| A scalar, value object or list at the top level or in the top-level `@graph` | `FreeFloating`: the standard drops it |
| A graph object other than the top-level `@graph` | `NamedGraph`: the RDF graph encoding holds one graph |
| A property that is a blank node identifier | `BlankPredicate`: an RDF triple cannot hold it |
| An `@id` or `@type` value of the form `@` and letters that is no keyword | `ReservedKeyword`: the standard ignores it |
| An IRI that is relative after expansion, holds a character `IRIREF` excludes, or holds a second `#` | `RelativeIri`, `IriCharacter`, `MalformedIri`: the standard drops the triple |
| A language tag that is not an RDF `LANGTAG` | `InvalidLanguageTag`: the standard drops the literal |
| A number whose nearest binary64 value is infinite | `NumberOutOfRange` |

Refusals come in processing order: the JSON faults; then the first repeated key in document order; then the expansion's refusals in the order the algorithm meets them (a map's `@context` before its other entries, a `@type` value of `@json` before the entries, the entries in document order); then the refusals of the triples in the walk's order.

## Canonical JSON-LD

[`emit_graph`](https://docs.rs/panproto-io/latest/panproto_io/registry/struct.ProtocolRegistry.html#method.emit_graph) for `jsonld`, and [`rdf::jsonld::write`](https://docs.rs/panproto-io/latest/panproto_io/rdf/jsonld/fn.write.html), write a graph in expanded form with no context: a JSON array holding one node object per subject, each on its own line indented by two spaces, the subjects in ascending code point order of their N-Triples text. A node object is `{"@id": ...}` followed by its predicates in ascending code point order, each with the array of its objects in ascending order of their N-Triples text. An IRI object is `{"@id": iri}`, a blank node `{"@id": "_:label"}`, and a literal `{"@value": lexical}` followed by `"@language"` for a tagged string or `"@type"` for a datatype other than `xsd:string`. A string escapes `"`, backslash, backspace, form feed, line feed, carriage return and tab as two characters, every other character below U+0020 as `\u` and four lowercase hexadecimal digits, and writes every other character as itself. The empty graph is `[]`. Every IRI is absolute, so the output reads back as the same triples with or without a base. The writer refuses what the N-Triples writer refuses, and in addition an IRI holding a second `#` (`IriFragments`), which the reader would refuse.

## The format-preserving JSON-LD route

[`JsonLdCodec::parse_graph_preserving`](https://docs.rs/panproto-io/latest/panproto_io/rdf/jsonld/struct.JsonLdCodec.html#method.parse_graph_preserving) reads a document into the graph the native route reads, with a [`JsonLdComplement`](https://docs.rs/panproto-io/latest/panproto_io/rdf/jsonld/struct.JsonLdComplement.html) holding the CST schema of the document (its contexts, compact IRIs, terms, key order and layout), the base, the graph as read and, for each term node, the JSON tokens that write it and whether the document also writes it with no token of its own. [`emit_graph_preserving`](https://docs.rs/panproto-io/latest/panproto_io/rdf/jsonld/struct.JsonLdCodec.html#method.emit_graph_preserving) writes an unedited graph as the document's bytes, through the CST complement of the [text formats](text-formats.md).

| Edited node | Token | Written as |
|---|---|---|
| `iri` | a key, an `@id` or `@type` value, or a string a term coerces to an IRI | a JSON string holding the absolute IRI |
| `blank` | an `@id` or `@type` value | a JSON string holding `_:` and the label |
| `literal` | a string value or `@value` string | a JSON string holding the lexical form |
| `literal` | a number or boolean | the new lexical form as a bare token, when a number or boolean there converts to it; otherwise refused with `NativeLexical` |
| `language` | an `@language` value | a JSON string holding the tag |

A changed term the document writes in some place with no token of its own is refused with `ImplicitTerm`: a datatype a term coerces to or a native value implies, the default language, `rdf:type`, `rdf:first`, `rdf:rest`, `rdf:nil` and a fresh blank node. Structural edits are refused with `StructuralEdit`, and graphs no document reads back as with the canonical writer's refusals. A context may give the new spelling another meaning (a key rewritten as an absolute IRI no longer carries its term's coercion, for instance), so the route reads the rewritten document again and writes it only when it reads back as the edited graph; otherwise it refuses with `ReadBackDiffers`. The native and the preserving route read the same graph from every document.

## N-Triples documents read as the grammar defines them {#L-rdf-formats-1}

### Statement

For a document $d$ of the bounded scope, write $N(d)$ for `read_document(d)`. When $N(d)$ is a list of triples, the N-Triples route reads $d$ as exactly that list, in document order with repeats. When $N(d)$ is the line $i$, the route refuses $d$ with an `RdfReadError` whose offset lies on line $i$, counting line feeds from zero.

### Outside the domain

Documents outside the scope are read by the same reader, and the conformance corpus checks them on the W3C suite. A document that is not UTF-8 is refused with `NotUtf8` before any reading, which the reference, reading characters, does not model.

### Definitions used

$N$ reads the productions of Sec 7 of RDF 1.1 N-Triples character by character: a document is lines separated by `EOL`, each blank, a comment, or one `triple` with an optional comment; space and tab separate terminals. It decodes `UCHAR` and `ECHAR`, gives a plain literal the datatype `xsd:string` and a tagged one `rdf:langString`, and applies the three refusals stated above beyond the grammar: a raw U+0000 or U+007F, an IRI with no scheme, and an IRI holding an excluded character. The scope is 1,704 documents: each of five subject spellings, three predicate spellings, 26 object spellings and four endings as a one-line document, and each ordered pair of six lines joined by each of four separators. The spellings include every refused construct of the table above.

### Proof

1. A document $N$ reads has, on each line, at most one triple whose terminals are separated by blanks, so the tree-sitter Turtle grammar reads it as one `triple` node per line with one subject, one property and one object. The walker accepts exactly this shape and reads each token from its source bytes: an IRI and a string by decoding the same escapes $N$ decodes, a label and a tag by the same productions, so the triples agree term for term.
2. A document $N$ refuses on line $i$ has a first fault on line $i$, with every earlier line read. Either the grammar reads that line as Turtle, in which case the walker meets the fault there (a construct Turtle has and N-Triples does not, a line structure fault, or a token the productions refuse), or it does not, in which case the parser's innermost recovery marker lies on that line and the walker, which reads every node ending before the marker, reports it there. Earlier lines hold no fault, so neither earlier source of refusal applies.
3. A raw U+0000 or U+007F is refused before parsing by both, at the line of its first occurrence.
4. The exhaustive differential checks the equality of the readings and of the refusal lines on all 1,704 documents, and the property differential on generated documents of up to six lines drawn from the same spellings with any separators.

### Executable counterpart

- Law ID: `rdf-formats.ntriples-reading`
- Reference: `panproto_reference::pages::rdf_formats::ntriples::read_document`
- Enumerator: `panproto_reference::enumerate::rdf_formats::ntriples_documents; bound: documents=1704`
- Exhaustive differential test: `ntriples_documents_read_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_rdf_formats --no-tests=fail -E 'test(=ntriples_documents_read_as_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_ntriples_documents_read_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_rdf_formats --no-tests=fail -E 'test(=randomized_ntriples_documents_read_as_the_reference)'`
- Reference test: `ntriples_reads_the_examples_of_the_recommendation; command: cargo nextest run -p panproto-reference --test rdf_formats --no-tests=fail -E 'test(=ntriples_reads_the_examples_of_the_recommendation)'`
- Reference test: `ntriples_refusals_name_the_line_of_the_first_fault; command: cargo nextest run -p panproto-reference --test rdf_formats --no-tests=fail -E 'test(=ntriples_refusals_name_the_line_of_the_first_fault)'`
- File: `crates/panproto-io/src/rdf/ntriples.rs`
- File: `crates/panproto-io/src/rdf/ntriples/reader.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats.rs`

## Triples encode as one node per term and per triple {#L-rdf-formats-2}

### Statement

For a list $t$ of triples of the bounded scope, write $E(t)$ for `encode(t)`. The production encoding `rdf::to_graph(t)` has exactly the nodes of $E(t)$, with the same ids, anchors and values, and exactly its edges, in the same id order with the same endpoints and names.

### Outside the domain

A list too long for `u32` node or edge ids is refused with `InstError::NodeIdsExhausted` or `InstError::EdgeIdsExhausted`, which no list of the scope reaches.

### Definitions used

$E$ walks $t$ in order and, for each triple not seen before, adds a `triple` node, then the node of its subject, of its predicate and of its object, each reused when a node already holds the same term under the same anchor; a new literal adds its datatype and tag after itself. The scope is every list of at most two of 24 triples (two subjects, two predicates, one of them also a subject, and six objects covering every literal form and a lexical form with characters the canonical form escapes): $1 + 24 + 24^2 = 601$ lists.

### Proof

1. Production keeps one table per anchor from term to node, and $E$ one table keyed by anchor and term; both look a term up before allocating, so each term has one node in both, and a repeated triple adds nothing in both.
2. Both allocate in the same order (triple, subject, predicate, object, and a literal's datatype and tag after it) from the same counter starting at 0, so the ids agree; both add each edge as soon as its target exists, in the same order, so the edge ids agree.
3. The differential checks equality of nodes and edges on all 601 lists.

### Executable counterpart

- Law ID: `rdf-formats.graph-encoding`
- Reference: `panproto_reference::pages::rdf_formats::ntriples::encode`
- Enumerator: `panproto_reference::enumerate::rdf_formats::triple_lists; bound: lists=601`
- Exhaustive differential test: `the_graph_encoding_matches_the_reference_on_every_bounded_triple_list; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_rdf_formats --no-tests=fail -E 'test(=the_graph_encoding_matches_the_reference_on_every_bounded_triple_list)'`
- Reference test: `the_graph_encoding_numbers_nodes_by_first_mention_and_reads_a_set; command: cargo nextest run -p panproto-reference --test rdf_formats --no-tests=fail -E 'test(=the_graph_encoding_numbers_nodes_by_first_mention_and_reads_a_set)'`
- File: `crates/panproto-io/src/rdf/mod.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats.rs`

## The writer prints canonical N-Triples or names the node no document holds {#L-rdf-formats-3}

### Statement

For a graph $g$ of the bounded scope, write $W(g)$ for `write_graph(g)`. When $W(g)$ is a text, the N-Triples writer writes $g$ as exactly that text, and reading the text gives a graph $g'$ with $W(g') = W(g)$. When $W(g)$ refuses node $n$ for reason $r$, the writer refuses $g$ with an `RdfWriteError` naming node $n$ for the same reason.

### Outside the domain

A node carrying a discriminator, extra fields, annotations, a position or a non-plain shape, an edge with a position, and a value other than a string are refused with `Presentation`, `UnexpectedEdge` and `Value`; the reference's graphs, which hold string values only, cannot express them, and the fixed tests in `rdf_graphs_round_trip_through_ntriples.rs` check each of them.

### Definitions used

$W$ checks the nodes in id order (the anchor; the value, absent on a triple and present on a term; the form of an IRI, label and tag; each out-edge against the encoding's edges between the two anchors; the count of each required edge; the literal's datatype rule), then in id order each node whose term or triple an earlier node holds, then the first node no triple reaches, and otherwise prints the canonical text of the section above. The scope is the encoding $E(t)$ of each of the 601 lists of [L-rdf-formats-2](#L-rdf-formats-2) together with its single-fault variants: each node duplicated, an unused IRI added, each edge dropped, each node's value replaced by each faulty value for its anchor, and a `datatype` edge added from each triple to its predicate, 21,043 graphs in all.

### Proof

1. Production checks the same conditions in the same order: each node in ascending id order with its anchor, value and form before its out-edges in id order and the counts after them, then the duplicate pass in id order, then the reachability pass; the first condition that fails therefore falls on the same node with the same reason.
2. On a graph both accept, the triples are those of $g$; production writes each as the line the canonical form defines, with the same escapes, and sorts the lines by code point, which is the byte order of their UTF-8 text and the order of the reference's set of lines.
3. Reading the canonical text yields one triple per line, so by [L-rdf-formats-2](#L-rdf-formats-2) the graph read back holds the same triples, and its canonical text is the same.
4. The differential checks the text, the read-back text and every refusal on all 21,043 graphs.

### Executable counterpart

- Law ID: `rdf-formats.canonical-ntriples`
- Reference: `panproto_reference::pages::rdf_formats::ntriples::canonical_text`
- Reference: `panproto_reference::pages::rdf_formats::ntriples::write_graph`
- Enumerator: `panproto_reference::enumerate::rdf_formats::mutations; bound: graphs=21043`
- Exhaustive differential test: `canonical_ntriples_matches_the_reference_on_every_bounded_graph; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_rdf_formats --no-tests=fail -E 'test(=canonical_ntriples_matches_the_reference_on_every_bounded_graph)'`
- Reference test: `canonical_text_sorts_lines_and_escapes_as_the_canonical_form_does; command: cargo nextest run -p panproto-reference --test rdf_formats --no-tests=fail -E 'test(=canonical_text_sorts_lines_and_escapes_as_the_canonical_form_does)'`
- Reference test: `the_writer_refuses_what_no_document_reads_back_as; command: cargo nextest run -p panproto-reference --test rdf_formats --no-tests=fail -E 'test(=the_writer_refuses_what_no_document_reads_back_as)'`
- Reference test: `the_rdf_scopes_have_their_closed_form_counts; command: cargo nextest run -p panproto-reference --test rdf_formats --no-tests=fail -E 'test(=the_rdf_scopes_have_their_closed_form_counts)'`
- File: `crates/panproto-io/src/rdf/mod.rs`
- File: `crates/panproto-io/src/rdf/ntriples.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats.rs`
- File: `crates/panproto-reference/src/enumerate/rdf_formats.rs`

## Turtle documents read as the grammar defines them {#L-rdf-formats-4}

### Statement

For a document $d$ and a base $b$ of the bounded scope, write $T(d, b)$ for `read_document(d, b)`. When $T(d, b)$ is a list of triples, the Turtle route with base $b$ reads $d$ as exactly that list, in the order the walk completes the triples, with the same fresh labels. When $T(d, b)$ is a range of lines, the route refuses $d$ with an `RdfReadError` whose offset lies on a line of that range, counting line feeds from zero.

### Outside the domain

Documents outside the scope are read by the same reader, and the conformance corpus checks them on the W3C suite. A document that is not UTF-8 is refused with `NotUtf8` before any reading, which the reference, reading characters, does not model. A prefix named `a` is outside the scope, since the grammar reads `a:` as the keyword `a` (the declared limit `turtle-subm-02`).

### Definitions used

$T$ tokenizes by longest match, as Sec 6.5 of RDF 1.1 Turtle requires, reads `@prefix` and `@base` as keywords where they also match `LANGTAG`, and reads the productions of Sec 6.5 character by character, with white space and comments between tokens. It resolves IRI references with the resolution of [L-rdf-formats-5](#L-rdf-formats-5), gives literals the datatypes of the section above, allocates fresh labels and orders triples as that section states, reading a document twice so that fresh labels skip every label it writes, and applies the stated refusals: a raw U+0000 or U+007F, an IRI holding an excluded character, a relative reference with no base, an undefined prefix. A fault is found at a position (the start of a rejected token, or the end of the last token read when the required next token is absent), and $T$ returns the lines from that position to the first token after it, since white space and comments may separate the two. The scope is 30,050 documents: for each of two bases (none and `http://b/d/f`) and four headers, each of 9 subject, 4 predicate, 26 object and 4 ending spellings as one statement; and each ordered pair of 7 statement lines joined by each of 2 separators. 8,101 of them read.

### Proof

1. A document $T$ reads is a sequence of statements the grammar's rules cover: each production of Sec 6.5 has a rule of the tree-sitter grammar that admits the same token sequences, with the exception the scope leaves out. The walker reads directives in order, so the base and prefix table in force at each token agree with $T$'s; it decodes each token by the same lexical rules (the escapes of `IRIREF`, `ECHAR`, `UCHAR` and `PN_LOCAL_ESC`) and resolves it by [L-rdf-formats-5](#L-rdf-formats-5), so each term agrees.
2. Both allocate fresh labels by the same counter, skipping the same set of written labels, at the same points (an `ANON` or `[` when met, a collection's cells all at its `(`), and both complete triples in the same order (an object's triples before its triple, a collection's cell by cell), so the lists agree element for element.
3. A document $T$ refuses has a first fault. When the grammar reads every statement before it, the walker meets the fault in the same statement at the same token, or, for a missing token, the parser's earliest recovery marker lies between the last token read and the next one, on a line of the range. When the fault is a token the stated refusals reject inside a region the parser cannot read, the walker's token check meets it first, since it checks every term token before that region in document order. A raw U+0000 or U+007F is refused before parsing by both.
4. The exhaustive differential checks the agreement on all 30,050 documents, and the property differential on generated documents of up to four statements over the same spellings, headers, bases and separators.

### Executable counterpart

- Law ID: `rdf-formats.turtle-reading`
- Reference: `panproto_reference::pages::rdf_formats::turtle::read_document`
- Enumerator: `panproto_reference::enumerate::rdf_formats::turtle_documents; bound: documents=30050`
- Exhaustive differential test: `turtle_documents_read_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_turtle --no-tests=fail -E 'test(=turtle_documents_read_as_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_turtle_documents_read_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_turtle --no-tests=fail -E 'test(=randomized_turtle_documents_read_as_the_reference)'`
- Reference test: `turtle_reads_each_abbreviation_as_its_triples; command: cargo nextest run -p panproto-reference --test rdf_formats_turtle --no-tests=fail -E 'test(=turtle_reads_each_abbreviation_as_its_triples)'`
- Reference test: `turtle_refusals_name_the_line_of_the_first_fault; command: cargo nextest run -p panproto-reference --test rdf_formats_turtle --no-tests=fail -E 'test(=turtle_refusals_name_the_line_of_the_first_fault)'`
- Reference test: `fresh_labels_skip_the_labels_the_document_writes; command: cargo nextest run -p panproto-reference --test rdf_formats_turtle --no-tests=fail -E 'test(=fresh_labels_skip_the_labels_the_document_writes)'`
- File: `crates/panproto-io/src/rdf/turtle.rs`
- File: `crates/panproto-io/src/rdf/turtle/reader.rs`
- File: `crates/panproto-io/src/rdf/cst.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/turtle.rs`

## Relative references resolve as RFC 3986 resolves them {#L-rdf-formats-5}

### Statement

For an absolute IRI $b$ and a reference $r$ of the bounded scope, write $R(b, r)$ for `resolve(b, r)`. The production resolution `rdf::turtle::resolve_iri(b, r)` equals $R(b, r)$.

### Outside the domain

The reader never resolves against a base that is not absolute: `with_base` refuses one with `InvalidBaseIri`, and a relative reference with no base is refused with `RelativeIri`. An absolute IRI reference is used as written, without dot-segment removal, which the statement does not cover.

### Definitions used

$R$ transcribes RFC 3986: the split of Appendix B, keeping a scheme only when it has the form of Sec 3.1; the algorithm of Sec 5.2.2 with the strict parser; the merge of Sec 5.2.3; `remove_dot_segments` of Sec 5.2.4, applying its steps A to E to a buffer of characters; and the recomposition of Sec 5.3. The scope is each of 5 bases with each reference made of at most three of 9 pieces (`g`, `.`, `..`, `/`, `//h`, `?y`, `#s`, `;x`, `g:`): $5 \cdot (1 + 9 + 9^2 + 9^3) = 4{,}100$ pairs.

### Proof

1. Production splits the fragment at the first `#`, then the query at the first `?`, then a scheme before the first `:` when the part before it is a scheme, then an authority after `//` up to the next `/`. Since a scheme holds no `/`, `?` or `#` and an authority no `/`, `?` or `#`, this is the split of Appendix B.
2. Production chooses among the four cases of Sec 5.2.2 by the same tests, takes the query and fragment as that section does, and merges as Sec 5.2.3 does.
3. Production's dot-segment removal applies the same five rules in the same order to the remaining input, each rule a prefix test on the input and the same change to input and output, so each step leaves both buffers equal to $R$'s.
4. Both recompose by Sec 5.3. The exhaustive differential checks equality on all 4,100 pairs, which include every example of RFC 3986 Sec 5.4, and the property differential on references of up to eight pieces followed by up to six characters drawn from `a-z./?#:;=`.

### Executable counterpart

- Law ID: `rdf-formats.iri-resolution`
- Reference: `panproto_reference::pages::rdf_formats::turtle::resolve`
- Enumerator: `panproto_reference::enumerate::rdf_formats::resolution_pairs; bound: pairs=4100`
- Exhaustive differential test: `iri_resolution_matches_the_reference_on_every_bounded_pair; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_turtle --no-tests=fail -E 'test(=iri_resolution_matches_the_reference_on_every_bounded_pair)'`
- Property differential test: `randomized_references_resolve_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_turtle --no-tests=fail -E 'test(=randomized_references_resolve_as_the_reference)'`
- Reference test: `resolution_follows_the_examples_of_rfc_3986; command: cargo nextest run -p panproto-reference --test rdf_formats_turtle --no-tests=fail -E 'test(=resolution_follows_the_examples_of_rfc_3986)'`
- File: `crates/panproto-io/src/rdf/turtle.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/turtle.rs`

## The Turtle writer prints canonical Turtle {#L-rdf-formats-6}

### Statement

For a list $t$ of triples of the bounded scope, write $C(t)$ for `canonical_text(t)`. The production writer `rdf::turtle::write(t)` prints exactly $C(t)$, and the Turtle route reads $C(t)$, with or without a base, as a list holding exactly the distinct triples of $t$.

### Outside the domain

The writer is given triples: a graph that is not the graph of any list of triples is refused before writing by the refusals of [L-rdf-formats-3](#L-rdf-formats-3), which apply to the `turtle` codec unchanged.

### Definitions used

$C$ sorts the distinct triples by the canonical N-Triples text of their subject, predicate and object, and writes each run of one subject as one statement in the form of the section above. The scope is the 601 lists of [L-rdf-formats-2](#L-rdf-formats-2).

### Proof

1. Production groups the triples by subject text, then by predicate text, in ordered maps, and sorts and deduplicates each object list, which orders the distinct triples as $C$ orders them; it writes the same separators at the same boundaries, and each term by the canonical N-Triples printer, whose text $C$ uses.
2. Canonical Turtle holds only absolute IRIs, labels, and strings with their tags or datatypes, so the reading of [L-rdf-formats-4](#L-rdf-formats-4) reads each predicate-object pair as one triple, with no fresh labels, and needs no base.
3. The differential checks the text and the triples read back on all 601 lists.

### Executable counterpart

- Law ID: `rdf-formats.canonical-turtle`
- Reference: `panproto_reference::pages::rdf_formats::turtle::canonical_text`
- Enumerator: `panproto_reference::enumerate::rdf_formats::triple_lists; bound: lists=601`
- Exhaustive differential test: `canonical_turtle_matches_the_reference_on_every_bounded_triple_list; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_turtle --no-tests=fail -E 'test(=canonical_turtle_matches_the_reference_on_every_bounded_triple_list)'`
- Reference test: `canonical_turtle_groups_subjects_and_predicates; command: cargo nextest run -p panproto-reference --test rdf_formats_turtle --no-tests=fail -E 'test(=canonical_turtle_groups_subjects_and_predicates)'`
- File: `crates/panproto-io/src/rdf/turtle.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/turtle.rs`

## The preserving route writes an edit in place or names why it cannot {#L-rdf-formats-7}

### Statement

For a document $d$ of the bounded scope that reads with base $b$, the format-preserving route writes the unedited graph as $d$ byte for byte. For each term $k$ of its graph and each value $v$ of the scope for $k$'s kind, write $E(d, k, v)$ for `edit(d, b, k, v)`. When $E$ is a text, setting $k$'s node to $v$ and writing through the route gives exactly that text, and reading it gives `edited_triples` of the reading with $k$ set to $v$, up to a renaming of the blank nodes written without a label. When $E$ is a refusal, the route refuses with an `RdfWriteError` of the same kind, a `DuplicateTerm` or `DuplicateTriple` matching a refusal named `Duplicate`.

### Outside the domain

Structural edits (a node added or removed, an anchor changed, an edge added, removed or moved) are refused with `StructuralEdit` before any other check; the scope edits values only, and `turtle_route.rs` checks the refusal on a removed triple. A value that is not a string is refused with `Value`.

### Definitions used

$E$ reads $d$ with the reading of [L-rdf-formats-4](#L-rdf-formats-4), recording each token that writes a term with its byte span and spelling, and each term written somewhere without a token. It checks, in production's order, the new value's form (`RelativeIri`, `IriCharacter`, `InvalidBlankLabel`, `InvalidLanguageTag`), an IRI edit that breaks a literal's tag and datatype rule (`LanguageDatatype`), a term made equal to another (`Duplicate`), a term written without a token (`ImplicitTerm`) and a label holding `:` (`InvalidBlankLabel`), and otherwise replaces each token's span by the text of the table above. The scope is the 3,744 one-statement documents of the reading scope with the base `http://b/d/f` and the header `@prefix : <http://e/> .`, of which 1,449 read; each of their terms is edited to each of two values for its kind (`EDIT_VALUES`), 18,438 edits, of which 10,323 are written.

### Proof

1. The complement holds the CST schema of $d$, and emitting a CST schema with no token text changed writes the document's bytes, as the text formats' CST complement does; the conformance corpus checks this on every readable file of the W3C suite.
2. Production checks structure, then runs the canonical writer's checks on the edited graph, whose node-level checks in id order and duplicate pass meet the first of $E$'s first four conditions that holds (the edited node, or for `LanguageDatatype` the literal whose datatype it is); then the implicit occurrence; then the label form. The walk that records occurrences visits the same tokens as $E$'s reading, by [L-rdf-formats-4](#L-rdf-formats-4), so the two agree on which terms are written without a token.
3. A permitted edit sets the text of each of the term's token vertices, and emission writes that text over the vertex's whole span and every other byte unchanged; $E$ replaces the same spans with the same texts, so the bytes agree.
4. Each replacement is a token denoting the new term in its position (an absolute IRI needs no base, a canonical string or bare token of the same form keeps the datatype and tag tokens beside it), so reading the output gives the edited triples; relabelling a written blank node may move the fresh labels, which the renaming absorbs.
5. The differential checks the unedited bytes, every outcome and every read-back on the scope.

### Executable counterpart

- Law ID: `rdf-formats.turtle-preservation`
- Reference: `panproto_reference::pages::rdf_formats::turtle::edit`
- Reference: `panproto_reference::pages::rdf_formats::turtle::edited_triples`
- Enumerator: `panproto_reference::enumerate::rdf_formats::preserving_documents; bound: documents=3744`
- Exhaustive differential test: `preserving_edits_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_turtle --no-tests=fail -E 'test(=preserving_edits_match_the_reference_on_every_bounded_document)'`
- Reference test: `edits_rewrite_the_terms_tokens_or_name_the_refusal; command: cargo nextest run -p panproto-reference --test rdf_formats_turtle --no-tests=fail -E 'test(=edits_rewrite_the_terms_tokens_or_name_the_refusal)'`
- Reference test: `the_turtle_scopes_have_their_closed_form_counts; command: cargo nextest run -p panproto-reference --test rdf_formats_turtle --no-tests=fail -E 'test(=the_turtle_scopes_have_their_closed_form_counts)'`
- File: `crates/panproto-io/src/rdf/turtle/reader.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/turtle.rs`
- File: `crates/panproto-reference/src/enumerate/rdf_formats.rs`

## JSON-LD documents read as the API's algorithms define them {#L-rdf-formats-8}

### Statement

For a document $d$ and a base $b$ of the bounded scope, write $J(d, b)$ for `read_document(d, b)`. When $J(d, b)$ is a list of triples, the JSON-LD route with base $b$ reads $d$ as exactly that list, in the order the walk completes the triples, with the same fresh labels. When $J(d, b)$ is a JSON-LD refusal of kind $k$ at the JSON Pointer $p$, the route refuses $d$ with an `RdfReadErrorKind::JsonLd` whose refusal has kind $k$ and whose pointer is $p$. When $J(d, b)$ finds no JSON value, the route refuses $d$ with a JSON-level kind.

### Outside the domain

Documents outside the scope are read by the same reader, and the conformance corpus checks them on the W3C suite. A document that is not UTF-8, holds a comment or white space JSON does not have, or nests deeper than 128 levels is refused before the JSON value is built, which the reference, reading a JSON value, does not model. The reader's order of triples and its fresh labels are its own; the API's node map relabels blank nodes, and the corpus compares with the API's expected results up to that renaming.

### Definitions used

$J$ reads $d$ with the reference's own RFC 8259 reader, refuses the first repeated key in document order, and then transcribes the algorithms of the API over the JSON value: Context Processing and Create Term Definition (Sec 4.1, 4.2) with the `defined` map and its cycle check, IRI Expansion (Sec 5.2) with the resolution of [L-rdf-formats-5](#L-rdf-formats-5), Expansion and Value Expansion (Sec 5.1, 5.3) in document order of keys, and the reading of triples of Sec 8.1 to 8.3 over the expanded default graph, with the numbers of [L-rdf-formats-9](#L-rdf-formats-9). It applies the subset and refusals of the section above, names each refusal by its kind and the pointer of the value at fault, and allocates fresh labels and orders triples as that section states. The scope is 1,538 documents: for each of two bases (none and `http://b/d/f`), each of 20 `@context` spellings (none, each feature of the subset and several refused ones, a cycle and an invalid term) followed by each of 38 node object bodies (each value form, keyword and refusal), and 9 whole documents (scalars, arrays, empty objects, free-floating values, a nested graph object, invalid JSON). 628 of them read.

### Proof

1. The JSON value production builds from the tree-sitter tree equals the value the reference reads: the JSON grammar's rules for objects, arrays, strings, numbers and literals are those of RFC 8259 apart from the comments and white space production refuses first; each string token is decoded by an RFC 8259 decoder and each number token checked against the RFC 8259 grammar; members keep their source order, as the reference keeps them. Both refuse the first repeated key in document order, since production checks each key before building its value.
2. Production follows the same steps of the same algorithms on that value: the context entries in the order of Sec 4.1, each term definition's entries in the order of Sec 4.2 (`@protected`, `@type`, `@reverse` or `@id`, then `@container`, `@index`, `@context`, `@language`, `@direction`, `@nest`, `@prefix`, and unknown entries last), and each map's `@context`, then its `@json` check, then its entries in document order. Each step tests the same conditions, so both meet the same first refusal at the same value, and its pointer, which both build by RFC 6901 from the path to that value, agrees.
3. On a document both read, the expanded items agree item for item, since each step produces the same node, value and list objects; both walk them in the same order, allocate fresh labels by the same counter skipping the same kept labels, and convert numbers by [L-rdf-formats-9](#L-rdf-formats-9), so the lists of triples agree element for element.
4. The exhaustive differential checks the agreement on all 1,538 documents, and the property differential on objects of one to three bodies under each context and base, nested as a property value or not.

### Executable counterpart

- Law ID: `rdf-formats.jsonld-reading`
- Reference: `panproto_reference::pages::rdf_formats::jsonld::read_document`
- Enumerator: `panproto_reference::enumerate::rdf_formats::jsonld_documents; bound: documents=1538`
- Exhaustive differential test: `jsonld_documents_read_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_jsonld --no-tests=fail -E 'test(=jsonld_documents_read_as_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_jsonld_documents_read_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_jsonld --no-tests=fail -E 'test(=randomized_jsonld_documents_read_as_the_reference)'`
- Reference test: `jsonld_reads_contexts_values_lists_and_reverse_properties; command: cargo nextest run -p panproto-reference --test rdf_formats_jsonld --no-tests=fail -E 'test(=jsonld_reads_contexts_values_lists_and_reverse_properties)'`
- Reference test: `jsonld_refusals_name_the_kind_and_the_pointer_of_the_value; command: cargo nextest run -p panproto-reference --test rdf_formats_jsonld --no-tests=fail -E 'test(=jsonld_refusals_name_the_kind_and_the_pointer_of_the_value)'`
- Reference test: `keys_that_expand_to_no_iri_are_dropped_as_the_standard_requires; command: cargo nextest run -p panproto-reference --test rdf_formats_jsonld --no-tests=fail -E 'test(=keys_that_expand_to_no_iri_are_dropped_as_the_standard_requires)'`
- File: `crates/panproto-io/src/rdf/jsonld.rs`
- File: `crates/panproto-io/src/rdf/jsonld/expand.rs`
- File: `crates/panproto-io/src/rdf/jsonld/to_rdf.rs`
- File: `crates/panproto-io/src/rdf/jsonld/reader.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/jsonld.rs`

## Native numbers take the API's lexical forms {#L-rdf-formats-9}

### Statement

For a JSON numeral $n$ and a datatype $t$ (none or an IRI) of the bounded scope, write $N(n, t)$ for `number_form(n, t)`. The production conversion `rdf::jsonld::number_literal(n, t)` gives the literal whose lexical form and datatype are $N(n, t)$, and refuses $n$ exactly when $N(n, t)$ does, which is when the binary64 rounding of $n$ overflows.

### Outside the domain

A token that is not an RFC 8259 numeral is refused by the JSON reading before conversion. The API defines the conversion on the implementation's number value; the statement pins that value to the binary64 value nearest the numeral, as the API's JavaScript reading gives it.

### Definitions used

$N$ reads the numeral as an exact rational, rounds it to binary64 by the round-to-nearest-even rounding of the values page, decodes the result exactly, and writes it with exact rational arithmetic: the integer itself when it is an integer of magnitude below $10^{21}$ and $t$ is not `xsd:double`, and otherwise the magnitude scaled to 16 significant digits, rounded up when the remainder is at least one half, in the form of the section above. The scope is 37 numerals (signed zeros, the boundary at $10^{21}$, integers beyond $2^{53}$, the largest finite value and overflow, subnormals and underflow, ties at the sixteenth digit, and carries) under 4 datatypes (none, `xsd:double`, `xsd:integer`, another IRI): 148 pairs.

### Proof

1. Production parses the numeral by the correctly rounded decimal to binary64 conversion of the Rust standard library, which gives the same value as $N$'s exact rounding, and both refuse an infinite result.
2. Production tests the fractional part, the magnitude against $10^{21}$ and the datatype as $N$ does; the integer case prints the value's exact integer digits, as $N$ does, and writes a zero of either sign as `0`.
3. In the double case production prints the exact decimal expansion of the value (a binary64 value has at most 767 significant digits, and the printing precision exceeds it), keeps 16 digits, and rounds up when the seventeenth is at least 5. The expansion is exact, so that digit is at least 5 exactly when the remainder is at least one half, which is $N$'s test; a carry out of the first digit raises the exponent in both. Trailing zeros are removed down to one digit after the point in both.
4. The exhaustive differential checks all 148 pairs, and the property differential checks generated numerals with up to 25 integer digits, up to 20 fraction digits and exponents from $-340$ to $340$ under each datatype.

### Executable counterpart

- Law ID: `rdf-formats.jsonld-numbers`
- Reference: `panproto_reference::pages::rdf_formats::jsonld::number_form`
- Enumerator: `panproto_reference::enumerate::rdf_formats::JSONLD_NUMERALS; bound: pairs=148`
- Exhaustive differential test: `native_numbers_convert_as_the_reference_on_every_bounded_numeral; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_jsonld --no-tests=fail -E 'test(=native_numbers_convert_as_the_reference_on_every_bounded_numeral)'`
- Property differential test: `randomized_numerals_convert_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_jsonld --no-tests=fail -E 'test(=randomized_numerals_convert_as_the_reference)'`
- Reference test: `numbers_take_the_api_forms_at_their_boundaries; command: cargo nextest run -p panproto-reference --test rdf_formats_jsonld --no-tests=fail -E 'test(=numbers_take_the_api_forms_at_their_boundaries)'`
- File: `crates/panproto-io/src/rdf/jsonld.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/jsonld.rs`

## The JSON-LD writer prints canonical JSON-LD {#L-rdf-formats-10}

### Statement

For a list $t$ of triples of the bounded scope, write $C(t)$ for `canonical_text(t)`. The production writer `rdf::jsonld::write(t)` prints exactly $C(t)$, and the JSON-LD route reads $C(t)$, with no base, as a list holding exactly the distinct triples of $t$.

### Outside the domain

The writer is given triples: a graph that is not the graph of any list of triples is refused before writing by the refusals of [L-rdf-formats-3](#L-rdf-formats-3), and a graph with an IRI holding a second `#` by `IriFragments`, which the scope's IRIs do not hold.

### Definitions used

$C$ groups the distinct triples by the N-Triples text of their subject, then by predicate, and writes each group as one node object in the form of the section above. The scope is the 601 lists of [L-rdf-formats-2](#L-rdf-formats-2).

### Proof

1. Production groups the triples in ordered maps keyed by the subject's N-Triples text, the predicate and the object's N-Triples text, which orders and deduplicates them as $C$ does, and writes the same separators and the same string escapes.
2. Canonical JSON-LD has no context, so every key is an absolute IRI, which IRI Expansion returns unchanged; every `@id` is an absolute IRI or a blank node identifier whose label is an N-Triples label, kept by the reading; every value object holds a string `@value` with an absolute `@type` or a `LANGTAG` `@language`. By [L-rdf-formats-8](#L-rdf-formats-8) the reading gives one triple per object of each predicate, the distinct triples of $t$, and needs no base.
3. The differential checks the text and the triples read back on all 601 lists.

### Executable counterpart

- Law ID: `rdf-formats.canonical-jsonld`
- Reference: `panproto_reference::pages::rdf_formats::jsonld::canonical_text`
- Enumerator: `panproto_reference::enumerate::rdf_formats::triple_lists; bound: lists=601`
- Exhaustive differential test: `canonical_jsonld_matches_the_reference_on_every_bounded_triple_list; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_jsonld --no-tests=fail -E 'test(=canonical_jsonld_matches_the_reference_on_every_bounded_triple_list)'`
- Reference test: `canonical_jsonld_writes_one_node_object_per_subject; command: cargo nextest run -p panproto-reference --test rdf_formats_jsonld --no-tests=fail -E 'test(=canonical_jsonld_writes_one_node_object_per_subject)'`
- File: `crates/panproto-io/src/rdf/jsonld.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/jsonld.rs`

## The preserving JSON-LD route writes an edit that reads back or names why it cannot {#L-rdf-formats-11}

### Statement

For a document $d$ of the bounded scope that reads with base $b$, the format-preserving route writes the unedited graph as $d$ byte for byte. For each term node $k$ of its graph and each value $v$ of the scope for $k$'s anchor, setting $k$'s value to $v$ and writing through the route either gives a text that the reference reading $J$ of [L-rdf-formats-8](#L-rdf-formats-8) reads, with base $b$, as exactly the triples of the edited graph, or refuses with an `RdfWriteError` of one of the kinds the section above states for an edit of a value.

### Outside the domain

Structural edits are refused with `StructuralEdit` before any other check; the scope edits values only. A value that is not a string is refused with `Value`. The statement does not say which edits are written: whether a rewritten token keeps its meaning under the document's context is decided by reading the result back, and $J$ checks that reading independently.

### Definitions used

The scope is the 320 documents of the reading scope that read with the base `http://b/d/f`; each of their term nodes is set to each of two values for its anchor (`http://e/new` and `http://e/a#b#c` for an IRI, `z` and `b0` for a blank node, `y` and `7` for a literal, `fr` and `en-GB` for a tag), 2,040 edits, of which 900 are written.

### Proof

1. The complement holds the CST schema of $d$, and emitting it with no token text changed writes the document's bytes, as the text formats' CST complement does; the conformance corpus checks this on every readable input of the W3C suite.
2. An edit that production writes passed its own read-back: the rewritten document, read by the production reading, gives the edited triples as a set. By [L-rdf-formats-8](#L-rdf-formats-8) the production reading and $J$ agree on documents of the forms the rewriting produces (the scope's documents with JSON strings and bare tokens replaced), and the differential checks this agreement on every written edit by reading the output with $J$.
3. Every refusal is raised by a check the section above names: the canonical writer's checks on the edited graph, the implicit occurrence, the native token's form, the label's form, or the read-back. The differential checks each refusal's kind against that list.
4. The differential checks the unedited bytes, every outcome and every read-back on the scope.

### Executable counterpart

- Law ID: `rdf-formats.jsonld-preservation`
- Reference: `panproto_reference::pages::rdf_formats::jsonld::read_document`
- Enumerator: `panproto_reference::enumerate::rdf_formats::jsonld_documents; bound: documents=320`
- Exhaustive differential test: `preserving_edits_read_back_under_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_jsonld --no-tests=fail -E 'test(=preserving_edits_read_back_under_the_reference_on_every_bounded_document)'`
- Reference test: `the_jsonld_scopes_have_their_closed_form_counts; command: cargo nextest run -p panproto-reference --test rdf_formats_jsonld --no-tests=fail -E 'test(=the_jsonld_scopes_have_their_closed_form_counts)'`
- File: `crates/panproto-io/src/rdf/jsonld/reader.rs`
- File: `crates/panproto-reference/src/pages/rdf_formats/jsonld.rs`
- File: `crates/panproto-reference/src/enumerate/rdf_formats.rs`
