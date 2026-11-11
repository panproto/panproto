# RDF formats

RDF data is a graph from the start: a set of triples whose subjects and objects are shared terms. Reading it into a panproto graph instance, rather than through a tree that encodes the graph, lets lenses and validation work on the triples themselves. This page states which RDF syntaxes panproto reads, how a document becomes a graph instance, what each reader refuses, what each writer prints, and the laws the readers and writers satisfy.

## Standards and shapes

The `ntriples` protocol (alias `n-triples`) reads and writes N-Triples as the W3C Recommendation *RDF 1.1 N-Triples* of 25 February 2014 defines it, over the abstract syntax of *RDF 1.1 Concepts and Abstract Syntax* of the same date. Its syntax and its data are both read as a graph: the protocol declares a graph syntactic shape and a graph semantic shape, and its graph codec, [`NTriplesCodec`](https://docs.rs/panproto-io/latest/panproto_io/rdf/ntriples/struct.NTriplesCodec.html), is registered by `default_registry` under the `tree-sitter` feature of `panproto-io`.

| Format | Standard | Read as | Status |
|---|---|---|---|
| N-Triples | RDF 1.1 N-Triples (W3C Recommendation, 2014-02-25) | graph | Read and written; the limits below |
| N-Quads | RDF 1.1 N-Quads (W3C Recommendation, 2014-02-25) | | Not supported: no bundled tree-sitter grammar reads N-Quads, and the Turtle grammar refuses each of the 12 positive syntax tests of the W3C N-Quads suite that carries a graph label |
| TriG, RDF/XML | | | Not supported |
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
