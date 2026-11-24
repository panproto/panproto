# Edge lists

An edge list is the plainest way to write a graph down: a table with one row per edge, naming the node the edge leaves and the node it enters. Graph tools exchange graphs this way because every spreadsheet and every CSV library can read the result. This page states which edge-list dialects panproto reads, how a document becomes a graph instance, what the reader refuses, what the writer prints, and the laws the reader and the writer satisfy.

## Dialects and shapes

No standard defines the edge list. The dialect panproto reads follows the edges table of [Gephi's spreadsheet import](https://docs.gephi.org/desktop/User_Manual/Import/CSV_Format/), which requires, in that documentation's words, "columns labeled 'Source' and 'Target' (which can be case insensitive)", and whose edges are directed unless a `Type` column says otherwise. panproto keeps the `Source` and `Target` columns and an optional `Label` column, and refuses every other column by name, Gephi's `Type`, `Id`, `Weight` and `Interval` among them. Two delimited texts carry the table, one per protocol:

| Protocol | Text | Read as | Status |
|---|---|---|---|
| `edgelist-csv` | [RFC 4180](https://www.rfc-editor.org/rfc/rfc4180) CSV with a header record, read by the CSV route of the [text formats](text-formats.md) | graph | Read and written canonically; the limits below |
| `edgelist-tsv` | IANA [`text/tab-separated-values`](https://www.iana.org/assignments/media-types/text/tab-separated-values) with a header record, read by the TSV route | graph | Read and written canonically; the limits below |
| Whitespace-separated lists without a header (SNAP, NetworkX `read_edgelist`) | | | Not supported: the first line is read as a header and its fields refused with `UnsupportedColumn` |
| Undirected edges, weights, edge ids, node tables | | | Not supported: the columns that carry them are refused with `UnsupportedColumn` |

Each protocol declares a graph syntactic shape and a graph semantic shape, and its graph codec, [`EdgeListCodec`](https://docs.rs/panproto-io/latest/panproto_io/edgelist/struct.EdgeListCodec.html), is registered by `default_registry` under the `tree-sitter` feature of `panproto-io`. The codec parses the text with the bundled tree-sitter CSV or TSV grammar and reads its records with the table extraction of that route, so an edge list has exactly the route's quoting, record and field-count rules: in CSV a field may be quoted, with `""` for one quote, and a quoted field may hold a delimiter or a line break; in TSV a quote is ordinary text and no field holds a tab or a line break. A leading byte-order mark is skipped.

## The edge-list graph encoding

Every edge list reads into one graph encoding, which the schema [`edgelist::csv::graph_schema`](https://docs.rs/panproto-protocols/latest/panproto_protocols/edgelist/csv/fn.graph_schema.html) declares: two vertices, each named for its kind, and two `prop` edges.

| Vertex | One node per | Value |
|---|---|---|
| `node` | distinct node name | the name |
| `edge` | record | the label, when the document has a `label` column; none otherwise |

An `edge` node has a `source` edge and a `target` edge to `node` nodes. An edge is a node rather than an instance edge so that it carries its label, and so that two records naming the same pair of nodes stay two edges, each with its own label and its own place in the order of the records.

A node is its name, the field's text exactly: nothing is trimmed, folded or coerced, so ` a ` and `a` are two nodes and `01` is a name, never a number. An edge list holds no node table, so a node has no attributes and a node that no edge names cannot be written. A record naming one node twice is a self-loop, and two records naming the same pair are parallel edges; both are kept as written. The reader never merges anything: the parallel-edge policy is that every record is its own edge. Nodes are numbered from 0 in record order: for each record, its `edge` node, then its source's `node` node and its target's when the name is new.

## What the reader reads and refuses

The header names the columns in any order, each compared to `source`, `target` and `label` without regard to ASCII case. Every later record is one edge, except a blank line, which is not a record. Each refusal is a [`ParseInstanceError::EdgeList`](https://docs.rs/panproto-io/latest/panproto_io/error/enum.ParseInstanceError.html) carrying an [`EdgeListReadError`](https://docs.rs/panproto-io/latest/panproto_io/edgelist/enum.EdgeListReadError.html), and the reader reports the first fault in this order.

| Input | Refusal |
|---|---|
| Bytes that are not UTF-8 | `NotUtf8`, at the first invalid byte |
| A header record the route cannot read | `Table`, with the route's message |
| No record at all | `NoHeader` |
| A header field other than `source`, `target` or `label` (an empty one included) | `UnsupportedColumn`, with its position and text |
| A header field naming a column an earlier field names | `DuplicateColumn`, with its position and text |
| No `source` column, then no `target` column | `MissingColumn` |
| A later record the route cannot read, or one whose field count differs from the header's | `Table` |
| An empty `source`, then `target` field, record by record | `EmptyName`, with the record (counting data records from 1) and the column |

The route reads RFC 4180 and the IANA registration with two choices of its own, which the reader inherits and the reference states. A TSV line ends at a line feed, a carriage return, or both. A record of several fields whose last field is empty and unquoted cannot end the text without a line ending, since the grammar cannot read it there (`a,` as the last bytes is refused with `Table`, while `a,""` and `a,` followed by a line feed are read); the canonical writer quotes every empty CSV field and ends every record, so canonical output never meets this limit. A CSV field that opens a quote and continues after the closing quote, or holds a quote without being quoted, is refused with `Table`, as RFC 4180 requires.

## Canonical output

[`emit_graph`](https://docs.rs/panproto-io/latest/panproto_io/registry/struct.ProtocolRegistry.html#method.emit_graph) and [`edgelist::write`](https://docs.rs/panproto-io/latest/panproto_io/edgelist/fn.write.html) write a graph as the canonical edge list: the header `source` and `target`, followed by `label` when the edges have labels; one record per `edge` node in ascending id order; fields separated by the dialect's delimiter and every record, the header included, ended by a line feed. A CSV field is quoted, each `"` doubled, exactly when it is empty or holds `,`, `"`, a carriage return or a line feed; a TSV field is written as itself.

The writer refuses, with an [`EmitInstanceError::EdgeList`](https://docs.rs/panproto-io/latest/panproto_io/error/enum.EmitInstanceError.html) carrying an [`EdgeListWriteError`](https://docs.rs/panproto-io/latest/panproto_io/edgelist/struct.EdgeListWriteError.html) that names the node at fault, every graph no edge list reads back as. It checks the nodes in ascending id order: an anchor other than `node` and `edge` (`UnknownAnchor`); data the encoding has no place for (`Presentation`); a `node` node without a string or an `edge` node with a value that is not one (`Value`); an empty name (`EmptyName`); a tab, line feed or carriage return in a TSV field (`UnwritableCharacter`); an out-edge that is not a `source` or `target` edge of the encoding (`UnexpectedEdge`); an `edge` node without exactly one of each (`EdgeCount`). Then, in id order, a `node` node whose name an earlier one holds (`DuplicateNode`), which a document would merge; an `edge` node that disagrees with the first on having a label (`MixedLabels`), since a document either has a `label` column or has none; and a `node` node no edge reaches (`Isolated`), which a document would drop. Every lossy emission is thus reported rather than done: nothing is merged, dropped or relabelled. The route claims no format preservation; an unedited round trip writes the canonical text.

## Conformance set

No standard test suite exists for edge lists. The set under `crates/panproto-io/tests/fixtures/conformance/edgelist` is built from the rules this page cites, and its `MANIFEST` records, for each file, the rule it exercises and its source: the Gephi documentation's required columns, case-insensitive names and directed default, the record and quoting examples of RFC 4180 Sec 2 rewritten under an edge-list header, and the IANA registration's tab-separated records. Each positive case lists the canonical text it reads as, and each negative case the refusal it draws (`conformance_edgelist.rs`). All 18 cases pass, and the set declares no limit.

## Edge lists read as the page defines them {#L-edge-lists-1}

### Statement

For a dialect $D$ and a document $d$ of the bounded scope, write $R_D(d)$ for `read_document(d, D)`. When $R_D(d)$ is a list of edges, the `edgelist` route of $D$ reads $d$ as exactly that list, in record order. When $R_D(d)$ is a refusal, the route refuses $d$ with an `EdgeListReadError` of the same kind and, where the kind carries them, the same column position, text, record and column name.

### Outside the domain

A document that is not UTF-8 is refused with `NotUtf8` before any reading, which the reference, reading characters, does not model. The `Table` kind is compared by kind only: its message is the route's.

### Definitions used

$R_D$ splits the text into records by RFC 4180 (CSV) or by lines and tabs (TSV), with the two choices the section above states, then applies the header rules and the record rules of the refusal table in its order. The scope is, for CSV, each of 3 prefixes (none, a byte-order mark, a blank line), 10 header spellings and 2 line endings, with a body of zero, one or two of 14 record spellings, each document with and without its final line ending, and the empty document: $1 + 3 \cdot 10 \cdot 2 \cdot 211 \cdot 2 = 25{,}321$ documents. For TSV it is the same with 9 headers, 11 records and 3 line endings: $1 + 3 \cdot 9 \cdot 3 \cdot 133 \cdot 2 = 21{,}547$ documents. The spellings include every refusal of the table.

### Proof

1. The route refuses a header record the grammar cannot read, and only then reads the header: by recovery markers that start before the header row ends, and by the RFC 4180 quoting check on each header field. $R_D$ refuses exactly a first record that is not a record of the dialect. For a TSV header, both split the first line at its tabs.
2. Both check the header fields left to right with the same tests (a case-insensitive match of the three names, then a repeat), then `source`, then `target`, so the first header refusal agrees.
3. The route's table extraction then refuses any later recovery marker, malformed quoted field or field-count mismatch, skipping blank lines as $R_D$ does; $R_D$ refuses any later record that is not a record of the dialect or has another field count. The two agree on which texts hold such a record, and the differential checks it on the scope.
4. On a text both read, the records are the same, with quotes read the same way, so the empty-name checks in record order and the edges agree.
5. The exhaustive differential checks the agreement on all 46,868 documents, and the property differential on generated documents of up to six records with any line endings.

### Executable counterpart

- Law ID: `edge-lists.reading`
- Reference: `panproto_reference::pages::edge_lists::read_document`
- Enumerator: `panproto_reference::enumerate::edge_lists::documents; bound: documents=46868`
- Exhaustive differential test: `edge_list_documents_read_as_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_edge_lists --no-tests=fail -E 'test(=edge_list_documents_read_as_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_edge_list_documents_read_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_edge_lists --no-tests=fail -E 'test(=randomized_edge_list_documents_read_as_the_reference)'`
- Reference test: `edge_lists_read_records_as_directed_edges_and_keep_parallel_edges_and_loops; command: cargo nextest run -p panproto-reference --test edge_lists --no-tests=fail -E 'test(=edge_lists_read_records_as_directed_edges_and_keep_parallel_edges_and_loops)'`
- Reference test: `edge_list_refusals_follow_the_order_the_page_states; command: cargo nextest run -p panproto-reference --test edge_lists --no-tests=fail -E 'test(=edge_list_refusals_follow_the_order_the_page_states)'`
- File: `crates/panproto-io/src/edgelist.rs`
- File: `crates/panproto-io/src/edgelist/reader.rs`
- File: `crates/panproto-reference/src/pages/edge_lists.rs`

## Edges encode as one node per record and per name {#L-edge-lists-2}

### Statement

For a list $e$ of edges of the bounded scope, write $E(e)$ for `encode(e)`. The production encoding `edgelist::to_graph(e)` has exactly the nodes of $E(e)$, with the same ids, anchors and values, and exactly its edges, in the same id order with the same endpoints and names.

### Outside the domain

A list too long for `u32` node or edge ids is refused with `InstError::NodeIdsExhausted` or `InstError::EdgeIdsExhausted`, which no list of the scope reaches.

### Definitions used

$E$ walks $e$ in order and, for each edge, adds an `edge` node holding its label, then the `node` node of its source and of its target when the name has none, then the `source` and `target` edges. The scope is every list of at most two of 8 edges (unlabelled, labelled, a self-loop, a pair that is parallel to another, names and labels the canonical form quotes, and fields holding a tab or a line break): $1 + 8 + 8^2 = 73$ lists.

### Proof

1. Production keeps one table from name to node and allocates from one counter starting at 0, in the order $E$ does: the `edge` node, then each endpoint's node when new; so each name has one node and the ids agree.
2. Every edge gets a fresh `edge` node in both, so a repeated edge (a parallel edge) adds a second node in both.
3. Both add the `source` edge, then the `target` edge, as soon as the endpoint exists, so the edge ids agree. The differential checks equality on all 73 lists.

### Executable counterpart

- Law ID: `edge-lists.graph-encoding`
- Reference: `panproto_reference::pages::edge_lists::encode`
- Enumerator: `panproto_reference::enumerate::edge_lists::edge_lists; bound: lists=73`
- Exhaustive differential test: `the_edge_list_encoding_matches_the_reference_on_every_bounded_list; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_edge_lists --no-tests=fail -E 'test(=the_edge_list_encoding_matches_the_reference_on_every_bounded_list)'`
- Reference test: `the_encoding_numbers_nodes_by_record_and_keeps_every_record; command: cargo nextest run -p panproto-reference --test edge_lists --no-tests=fail -E 'test(=the_encoding_numbers_nodes_by_record_and_keeps_every_record)'`
- File: `crates/panproto-io/src/edgelist.rs`
- File: `crates/panproto-reference/src/pages/edge_lists.rs`

## The writer prints the canonical edge list or names the node no document holds {#L-edge-lists-3}

### Statement

For a dialect $D$ and a graph $g$ of the bounded scope, write $W_D(g)$ for `write_graph(g, D)`. When $W_D(g)$ is a text, the writer of $D$ writes $g$ as exactly that text, and reading the text gives a graph $g'$ with $W_D(g') = W_D(g)$. When $W_D(g)$ refuses node $n$ for reason $r$, the writer refuses $g$ with an `EdgeListWriteError` naming node $n$ for the same reason.

### Outside the domain

A node carrying a discriminator, extra fields, annotations, a position or a non-plain shape, an edge with a position, and a value other than a string are refused with `Presentation`, `UnexpectedEdge` and `Value`; the reference's graphs, which hold string values only, cannot express them, and the fixed tests in `edge_list_route.rs` check each of them.

### Definitions used

$W_D$ checks the nodes in id order (the anchor; a `node` node's value present and not empty; the characters $D$ cannot write; each out-edge against the encoding; an `edge` node's one `source` and one `target`), then in id order each `node` node whose name an earlier one holds, then each `edge` node that disagrees with the first on having a label, then each `node` node no edge reaches, and otherwise prints `canonical_text` of the edges in `edge` node order. The scope is the encoding $E(e)$ of each of the 73 lists of [L-edge-lists-2](#L-edge-lists-2) together with its single-fault variants: each node duplicated, an isolated node and a node of an unknown anchor added, each edge dropped, each `node` node's value removed and emptied, each `edge` node's label removed or added, and a `source` edge from each `node` node to itself; 1,387 graphs, each written in both dialects.

### Proof

1. Production checks the same conditions in the same order: each node in ascending id order, its anchor, value and characters before its out-edges in id order and the counts after them, then the duplicate pass, the label pass and the reachability pass, each in id order; the first condition that fails falls on the same node with the same reason.
2. On a graph both accept, the edges are those of $g$ in `edge` node order; production writes each as the record the canonical form defines, with the same quoting.
3. Every canonical record is one the route reads (every empty CSV field is quoted, every record ends in a line feed, no TSV field holds a tab or a line break), so reading the text gives the same edges, by [L-edge-lists-1](#L-edge-lists-1), and their graph, by [L-edge-lists-2](#L-edge-lists-2), has the same canonical text.
4. The differential checks the text, the read-back text and every refusal on all 1,387 graphs in both dialects.

### Executable counterpart

- Law ID: `edge-lists.canonical-writing`
- Reference: `panproto_reference::pages::edge_lists::canonical_text`
- Reference: `panproto_reference::pages::edge_lists::write_graph`
- Enumerator: `panproto_reference::enumerate::edge_lists::mutations; bound: graphs=1387`
- Exhaustive differential test: `canonical_edge_lists_match_the_reference_on_every_bounded_graph; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_edge_lists --no-tests=fail -E 'test(=canonical_edge_lists_match_the_reference_on_every_bounded_graph)'`
- Reference test: `canonical_edge_lists_quote_and_refuse_as_the_page_states; command: cargo nextest run -p panproto-reference --test edge_lists --no-tests=fail -E 'test(=canonical_edge_lists_quote_and_refuse_as_the_page_states)'`
- Reference test: `the_writer_refuses_what_no_edge_list_reads_back_as; command: cargo nextest run -p panproto-reference --test edge_lists --no-tests=fail -E 'test(=the_writer_refuses_what_no_edge_list_reads_back_as)'`
- Reference test: `the_edge_list_scopes_have_their_closed_form_counts; command: cargo nextest run -p panproto-reference --test edge_lists --no-tests=fail -E 'test(=the_edge_list_scopes_have_their_closed_form_counts)'`
- File: `crates/panproto-io/src/edgelist.rs`
- File: `crates/panproto-reference/src/pages/edge_lists.rs`
- File: `crates/panproto-reference/src/enumerate/edge_lists.rs`
