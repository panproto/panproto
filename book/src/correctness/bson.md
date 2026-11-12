# BSON

[BSON](https://bsonspec.org/) is the binary document format MongoDB stores and sends, and [MongoDB Extended JSON](https://github.com/mongodb/specifications/blob/master/source/extended-json/extended-json.md) is its text form. A document in either form carries values JSON cannot tell apart (a 32-bit and a 64-bit integer, a double and a decimal, an object id and a string), and a reader that blurs them changes the data a migration then moves. This page states which documents panproto reads, how each BSON type is held, what the decoder and the Extended JSON reader refuse, what the writers print, and the laws the readers and writers satisfy.

## Standard and shapes

The binary form is [BSON 1.1](https://bsonspec.org/spec.html), the grammar of the specification's page, and a decimal128 value is the BID encoding of IEEE 754-2008 decimal128 as the [BSON Decimal128 specification](https://github.com/mongodb/specifications/blob/master/source/bson-decimal128/decimal128.md) states, with that specification's strings. The text form is version 2 of Extended JSON, as `source/extended-json/extended-json.md` of [mongodb/specifications](https://github.com/mongodb/specifications) states it at commit `b8e0ee19`, in its two formats: *canonical*, which wraps every value whose type JSON cannot carry in a type wrapper object (`{"$numberInt": "1"}`, `{"$date": {"$numberLong": "0"}}`), and *relaxed*, which writes an int32 or int64 as a JSON integer, a finite double as a JSON number with a fraction or an exponent, and a datetime from 1970 to 9999 as an RFC 3339 string. The reader reads either format, as the specification asks of a parser by default; each writer writes the one it is asked for. Legacy Extended JSON is not a format of version 2 and is not read.

A document is read as a tree: the `bson` protocol declares a tree syntactic and a tree semantic shape. BSON has no reference from one part of a document to another. A `DBPointer` and a `DBRef` (a document of `$ref`, `$id` and an optional `$db`) name a document of another collection, outside the document, so nothing is resolved: the [`Resolution`](https://docs.rs/panproto-io/latest/panproto_io/resolution/struct.Resolution.html) lens, which resolves references within an instance, has nothing to do, and a `DBRef` is read as the ordinary document it is. The decoder [`decode`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.decode.html) and the writers [`encode`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.encode.html) and [`write_extended_json`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.write_extended_json.html) are in `panproto-io` in every build. The Extended JSON reader [`read_extended_json`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.read_extended_json.html) is under the `tree-sitter` feature: it reads no JSON itself but goes through the shared JSON route ([`read_json`](https://docs.rs/panproto-io/latest/panproto_io/config/tree/fn.read_json.html), RFC 8259 through the tree-sitter JSON grammar), so a repeated member name, a comment, a second value and text that is not UTF-8 are refused before the reader sees the document. The binary form is a fixed byte layout, so it is decoded by its grammar directly; no tree-sitter grammar is involved.

| BSON type | Read as | Held in the tree encoding as |
|---|---|---|
| double (`0x01`) | its 64 bits, a NaN's payload included | a float |
| string (`0x02`), JavaScript code (`0x0D`), symbol (`0x0E`, deprecated) | its UTF-8 text, U+0000 allowed | a string |
| document (`0x03`) | its elements in order, two elements of one name kept | a `document` node with `element` edges |
| array (`0x04`) | its values in order | an `array` node with `item` edges |
| binary (`0x05`) | its subtype and bytes; for subtype 2 the payload after the repeated inner length | an opaque `binary` value of `subtype` and `bytes` |
| undefined (`0x06`, deprecated), min key (`0xFF`), max key (`0x7F`) | the value | a node holding nothing |
| object id (`0x07`) | its twelve bytes | bytes |
| boolean (`0x08`) | the flag | a boolean |
| UTC datetime (`0x09`) | its millisecond count | the UTC date-time `Temporal::from_epoch_count` gives it in milliseconds |
| null (`0x0A`) | null | null |
| regular expression (`0x0B`) | pattern and options | an opaque `regex` value of `pattern` and `options` |
| DBPointer (`0x0C`, deprecated) | namespace and object id | an opaque `db-pointer` value of `namespace` and `id` |
| code with scope (`0x0F`, deprecated) | code and scope document | a `code-with-scope` node holding the code, with a `scope` edge |
| int32 (`0x10`), int64 (`0x12`) | the integer, the two types kept apart | an integer under its own vertex |
| timestamp (`0x11`) | time and increment | an opaque `timestamp` value of `time` and `increment` |
| decimal128 (`0x13`) | sign, coefficient and exponent, an infinity, or a NaN with its sign, signal and payload | an exact decimal |

Every deprecated type is carried exactly, not refused and not converted: the corpus's `converted_bson` and `converted_extjson` forms, which turn a symbol into a string or a DBPointer into a `DBRef`, are an optional conversion the codec does not make.

| Not supported | Refusal | Reason |
|---|---|---|
| Legacy Extended JSON: `{"$binary": "…", "$type": "…"}`, `{"$date": <number>}` | `WrapperKeys`, `WrapperValue` | Not version 2; a `$regex` or `$type` key without a wrapper key is an ordinary key, as version 2 requires |
| Degenerate BSON: array keys other than `"0"`, `"1"`, … in order; regular expression options out of alphabetical order | `ArrayKey`, `RegexOptions` | Reading them would rename the keys or reorder the options the document holds |
| Decimal128 bits that are not a canonical encoding (a coefficient above $10^{34} - 1$, which IEEE 754 reads as zero; bits an infinity or a NaN ignores) | `NonCanonicalDecimal128` | Reading them as zero would make two byte strings one value, and the writer could not give back the bytes read |
| A `$numberDecimal` string decimal128 holds only by clamping its exponent or by rounding (`1E+6112`, 35 significant digits with trailing zeros) | `DecimalRepresentation` | Clamping and rounding change the coefficient and exponent written |
| A relaxed JSON integer outside the int64 range | `IntegerRange` | The specification reads it as a double, which loses digits |
| A JSON number or `$numberDouble` string with more digits than its double keeps, or outside the double range | `InexactNumber` | Reading it would round it |
| Extended JSON of a NaN other than the one `NaN` reads as, of a document with two elements of one name, or of an embedded document with a wrapper key | `NaN`, `DuplicateKey`, `WrapperKey` (writer) | No Extended JSON text reads back as the document |
| A `$uuid` without hyphens or with a `urn:uuid:` prefix | `WrapperValue` | The specification lets a parser read them; this one reads the hyphenated form only |
| Validation of a subtype 9 vector's layout | none (the bytes are carried) | The vector rules are a separate specification; the bytes are kept as written |
| Nesting deeper than 128 documents, arrays and scopes; in Extended JSON text, deeper than the JSON route reads (64 documents) | `Depth`; the JSON route's refusal for text | Bounds the recursion of reading and writing |

## The BSON tree encoding

Every BSON document is read under one schema, [`tree_schema`](https://docs.rs/panproto-protocols/latest/panproto_protocols/data_schema/bson/fn.tree_schema.html) of `panproto-protocols`: a vertex `element`, and a vertex for each BSON type named for it (`double`, `string`, `document`, `array`, `binary`, `undefined`, `object-id`, `boolean`, `datetime`, `null`, `regex`, `db-pointer`, `code`, `symbol`, `code-with-scope`, `int32`, `timestamp`, `int64`, `decimal128`, `min-key`, `max-key`), with an `element` edge from `document` to `element`, a `value` edge from `element` and an `item` edge from `array` to every value vertex, and a `scope` edge from `code-with-scope` to `document`. [`to_instance`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.to_instance.html) numbers nodes from 0 in document order: a document's node, then for each element its `element` node, holding the element's name as a string, and its value's nodes; an array's node, then its items' nodes; a code with scope's node, then its scope's. An element is a node rather than an edge name so that two elements of one name stay two, in their order. [`from_instance`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.from_instance.html) reads an instance back, refusing with a `BsonInstanceError` naming the node any instance no document reads back as: a root that is not a `document`, a node with presentation data the encoding has no place for, an edge that is not an edge of the encoding between its ends' anchors, a node reached twice or not at all, an element or code with scope without exactly one `value` or `scope` edge, and a value its anchor does not hold (an int32 outside 32 bits, a key holding U+0000, unsorted options, a decimal decimal128 does not hold as written). The `bson` codec of the registry decodes and encodes the binary form through this encoding.

## What the decoder refuses

[`decode`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.decode.html) reads the bytes left to right and returns the first rule broken as a [`BsonDecodeError`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/struct.BsonDecodeError.html) with the byte offset at fault. A document's length field (at its offset) must be readable and the length at least 5 and within the bytes its container leaves it: the whole input for the top-level document, the bytes before its container's last byte for an embedded one (`Truncated`, `DocumentLength`). Then element by element: a type byte at the document's last byte must be `0x00` (`Terminator`) and a `0x00` before it is refused (`EarlyTerminator`); a type byte must be a BSON 1.1 type (`UnknownType`); the key must end in `0x00` before the document's last byte (`UnterminatedCString`) and be UTF-8 (`InvalidUtf8`, at the first byte of the first invalid sequence); in an array the key must be the element's index (`ArrayKey`, at the key). The value then follows its type's rule: a fixed-size value must fit (`Truncated`); a string's length is at least 1, fits (`StringLength`, at its length field) and ends in `0x00` (`StringTerminator`, at that byte), and its text is UTF-8; a boolean is `0x00` or `0x01` (`Boolean`); a binary's length is not negative and fits (`BinaryLength`), and a subtype 2 binary repeats its length less 4 (`OldBinaryLength`, at the inner length); a regular expression's two C strings follow the key's rules and the options are in alphabetical order (`RegexOptions`, at the options); a code with scope's length is at least 14, fits, and is the length of its code and scope (`CodeWithScopeLength`); a decimal128 is a canonical encoding (`NonCanonicalDecimal128`). An embedded document, array or scope deeper than 128 is refused at its length field (`Depth`), and bytes after the top-level document at the first of them (`TrailingBytes`).

## Reading Extended JSON

The top-level value must be an object (`NotADocument`) and is a document whatever its keys, as the specification's parser rules apply to objects other than the top-level one. Any other object holding a key of a type wrapper (`$oid`, `$symbol`, `$numberInt`, `$numberLong`, `$numberDouble`, `$numberDecimal`, `$binary`, `$code`, `$scope`, `$timestamp`, `$regularExpression`, `$dbPointer`, `$date`, `$minKey`, `$maxKey`, `$undefined`, `$uuid`) must have exactly the keys of one wrapper, in any order (`WrapperKeys`, at the object, naming its first wrapper key), and that wrapper's value must have its form (`WrapperValue`, at the value): 24 hexadecimal digits of either case for `$oid`; a string `-?(0|[1-9][0-9]*)` in range for `$numberInt` and `$numberLong`; `Infinity`, `-Infinity`, `NaN` or a decimal string for `$numberDouble`; a numeric string of the Decimal128 specification (a sign, digits with a point anywhere, an exponent; or `Inf`, `Infinity` or `NaN` in any case) for `$numberDecimal`; an object of exactly padded `base64` and a one- or two-digit hexadecimal `subType` for `$binary`; an object of exactly `t` and `i`, integers from 0 to $2^{32} - 1$, for `$timestamp`; an object of exactly `pattern` and `options` for `$regularExpression`, with no U+0000 (`NulInRegex`) and the options in alphabetical order (`RegexOptions`); an object of exactly `$ref` and an `$oid` wrapper `$id` for `$dbPointer`; a `$numberLong` wrapper or an RFC 3339 date-time (four-digit year, at most three fraction digits, no leap second, an offset `Z`, `z` or `±HH:MM`) for `$date`; the integer 1 for `$minKey` and `$maxKey`; `true` for `$undefined`; and the hyphenated UUID form, read as a subtype 4 binary, for `$uuid`. Every other object is a document. A string, boolean, null and array are themselves. A JSON integer is an int32 when it fits and an int64 otherwise, and one outside int64 is refused (`IntegerRange`). A JSON number with a fraction or an exponent is a double, and so is a `$numberDouble` decimal string, when the shortest decimal form of the double nearest it has the same value: `0.1` and `1.50` read, `0.10000000000000000001` and `1e400` are refused (`InexactNumber`). A `$numberDecimal` string is the decimal it writes, its exponent and trailing zeros as written; one that needs clamping or rounding is refused (`DecimalRepresentation`). A key holding U+0000 is refused (`NulInKey`). Every refusal is an [`ExtJsonError`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/struct.ExtJsonError.html) with the JSON pointer of the value at fault, its byte offset and its kind, and the reader returns the first in document order.

## Canonical output

[`encode`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.encode.html) writes the BSON 1.1 layout, little-endian, arrays keyed `"0"`, `"1"`, … in order, a subtype 2 binary's inner length repeated and each decimal128 in its canonical BID encoding. It refuses, with a [`BsonWriteError`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/struct.BsonWriteError.html) naming the JSON pointer of the value, a key or regular expression holding U+0000, unsorted options, a decimal decimal128 does not hold as written, a length an `int32` cannot state and nesting deeper than 128.

[`write_extended_json`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/bson/fn.write_extended_json.html) writes one JSON value with no whitespace and no line ending: members in element order, strings escaped as `serde_json` escapes them, hexadecimal in lower case, base64 with padding, and the wrappers' members in the order of the specification's conversion table. A finite double is written as its shortest digits that read back as the double, positional with at least one fraction digit (`1.0`, `-0.0`, `0.0001`) when the decimal exponent of its first digit is from -4 to 15 and `dE±n` otherwise (`1.2345678921232E+18`), the form of the corpus; a decimal128 by the Decimal128 specification's string. In relaxed mode a datetime from 1970-01-01 to the end of 9999 is `YYYY-MM-DDTHH:MM:SS`, three millisecond digits when they are not zero, and `Z`. Beside `encode`'s refusals the writer refuses a document with two elements of one name (the JSON route refuses a repeated member), an embedded document holding a wrapper key (it would read back as that wrapper), and a NaN other than the one `NaN` reads as (a double with other bits than `0x7FF8000000000000`, a decimal128 NaN with a sign, a signal or a payload). Relaxed text keeps values but not every type, by the specification's design: an int64 within the int32 range reads back as an int32.

## Conformance corpus

Two sets of cases check the codec against the specifications. The first is the BSON corpus, `source/bson-corpus/tests` of [mongodb/specifications](https://github.com/mongodb/specifications) at commit `b8e0ee19`: 31 JSON files holding 731 `valid`, 75 `decodeErrors` and 180 `parseErrors` cases. The corpus is licensed under [Creative Commons Attribution-NonCommercial-ShareAlike 3.0 United States](https://creativecommons.org/licenses/by-nc-sa/3.0/us/), which is neither permissive nor compatible with panproto's MIT licence, so it is not kept in the repository. `crates/panproto-io/tests/fixtures/conformance/bson-corpus/MANIFEST` records the commit and each file's name, case counts and SHA-256 digest, and `python3 tools/fetch-bson-corpus.py` downloads the files at that commit into the directory's `tests/`, which git ignores, failing on any digest that differs. `conformance_bson.rs` runs every representation with the corpus document's assertions for a codec with a native representation: canonical BSON decodes, encodes back to itself directly and through the tree encoding, and writes the case's canonical and relaxed Extended JSON; canonical Extended JSON reads, writes itself back and, unless the case is lossy, encodes to the canonical BSON; relaxed Extended JSON writes itself back; degenerate forms read as the canonical ones. Texts are compared as JSON values. Every `decodeErrors` case is refused by the decoder, every `parseErrors` case by the Extended JSON reader (and read by the JSON route, so the refusal is Extended JSON's) or the decimal128 string parser. Of the 2,073 representations, 2,001 pass, and the 72 others are declared in `bson-corpus-limits.txt` with the class the test checks: 57 `decimal-representation` (`$numberDecimal` strings that need clamping or rounding), 7 `write-nan` (NaNs with a sign, signal or payload, which canonical Extended JSON writes as its one `NaN`), 3 `non-canonical-decimal128`, 3 `array-key` and 2 `regex-options` (the degenerate forms). The corpus gate workflow fetches the corpus and runs the test with `PANPROTO_BSON_CORPUS=required`, under which a missing corpus fails the test; without the variable and without a fetched corpus, the test reports itself skipped.

The second set runs in every build of the tests. `fixtures/conformance/bson-spec/MANIFEST` holds 106 cases written for panproto from the text of the [BSON 1.1 specification](https://bsonspec.org/spec.html), the [Decimal128 specification](https://github.com/mongodb/specifications/blob/master/source/bson-decimal128/decimal128.md) and the [Extended JSON v2 specification](https://github.com/mongodb/specifications/blob/master/source/extended-json/extended-json.md), none taken from the corpus, each line naming the section it exercises: 31 `valid` cases (canonical BSON that decodes, encodes back directly and through the tree encoding, and writes its canonical Extended JSON, which reads back as the bytes), 5 `relaxed`, 6 `read` (Extended JSON in an alternative form, such as a `$uuid` or a date with an offset, that reads as given bytes), 27 decode errors, 34 parse errors and 3 write errors, each refusal pinned to its class and its byte offset or JSON pointer. `conformance_bson_spec.rs` checks that every one of the 21 BSON types has a valid case, a decode error and a parse error, and that every refusal class of the decoder, of the Extended JSON reader and of the writer's Extended JSON refusals has a case. For the types JSON itself carries (string, array, boolean, null), the parse error is the JSON route's.

The set also pins a bound that comes from the JSON route and not from this page's rules: the route's tree walker stops at 128 levels of the JSON grammar's tree, in which each nested object with a member takes two (the object and its pair), so Extended JSON text nests at most 64 documents when the innermost is empty and 63 when it has a member, while binary BSON nests 128. The Extended JSON specification asks a parser to support at least 200 levels with a SHOULD; the reader supports fewer, and refuses deeper text with the route's error at the root rather than with `Depth`.

## Decoding reads as the page defines it {#L-bson-1}

### Statement

For every byte string $b$ of the bounded scope, `decode(b)` is the document the reference `decode` reads from $b$, with the same elements, types and values (a double by its bits, a decimal128 by its sign, coefficient and exponent or its special value), or a refusal at the same byte offset with the same kind as the reference's.

### Outside the domain

Nesting deeper than the scope's (three levels) is checked by the fixed tests, the 128-level bound among them. Messages are not compared; offsets and kinds are.

### Definitions used

The reference reads the bytes by the rules and the order of the section on refusals, with decimal128 from the BID bit layout (sign bit 127; combination bits 126 to 122 `11111` for a NaN with bit 121 its signal, bits 120 to 110 zero and a payload below $10^{33}$ in bits 109 to 0; `11110` for an infinity with every other bit zero; otherwise, unless bits 126 and 125 are both set, a biased exponent in bits 126 to 113 and a coefficient below $10^{34}$ in bits 112 to 0). The scope is built from 31 values, one or more of every BSON type: the encodings of the empty document, of one element `a` of each value, of two elements `a` and `b` of each ordered pair, and of two elements both named `a` ($1 + 31 + 31^2 + 1 = 994$ documents), and, for the empty and the 31 one-element documents, each byte replaced by each of `0x00`, `0x01`, `0x7F`, `0xFF` that differs from it, each proper prefix, and the encoding followed by `0x00` (2,297 more): 3,291 byte strings.

### Proof

1. Production and the reference read a document's length, then its elements, with the same container bound: the top-level document's is the input's end, an embedded document's or value's is its document's last byte, and a code with scope's code and scope are bounded by its own length. Both check the conditions of each element in the same order (type byte, key, the array index, then the value's rule), so the first fault met is the same, at the offset each condition names.
2. Each value's bytes are read the same way: integers, doubles and datetimes little-endian, a timestamp's high four bytes its time, a subtype 2 binary's payload after its inner length; production decodes decimal128 bits into an exact decimal by the same conditions as the reference, so the same bits are refused and the same values read.
3. The exhaustive differential checks the agreement on all 3,291 byte strings, of which 1,632 decode (the 994 encodings and 638 mutations that remain documents), and the property differential on generated documents nested three deep.

### Executable counterpart

- Law ID: `bson.decoding`
- Reference: `panproto_reference::pages::bson::decode`
- Enumerator: `panproto_reference::enumerate::bson::byte_strings; bound: byte-strings=3291`
- Exhaustive differential test: `bson_decoding_matches_the_reference_on_every_bounded_byte_string; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_bson --no-tests=fail -E 'test(=bson_decoding_matches_the_reference_on_every_bounded_byte_string)'`
- Property differential test: `randomized_documents_encode_decode_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_bson --no-tests=fail -E 'test(=randomized_documents_encode_decode_and_write_as_the_reference)'`
- Reference test: `a_binary_document_reads_by_the_bson_grammar_and_encodes_back; command: cargo nextest run -p panproto-reference --test bson --no-tests=fail -E 'test(=a_binary_document_reads_by_the_bson_grammar_and_encodes_back)'`
- Reference test: `binary_refusals_follow_the_order_the_page_states; command: cargo nextest run -p panproto-reference --test bson --no-tests=fail -E 'test(=binary_refusals_follow_the_order_the_page_states)'`
- Randomized input scope: documents of up to three elements named `a`, `b` or `c`, whose values are the scope's 31 values or documents and arrays of up to two members, nested up to three deep, with keys from `a`, `b`, `$c`, `x.y` and `é`.
- File: `crates/panproto-io/src/data_schema/bson/binary.rs`
- File: `crates/panproto-io/src/data_schema/bson/decimal128.rs`
- File: `crates/panproto-reference/src/pages/bson.rs`

## The binary writer and the tree encoding give back what was read {#L-bson-2}

### Statement

For every document $d$ of the bounded scope, `encode(d)` is the reference's `encode(d)`, `decode` reads it back as $d$, `to_instance(d)` has exactly the nodes of the reference's `tree(d)` in the same order (each node's parent edge, anchor and held value), and `from_instance(to_instance(d))` is $d$. So binary to instance to binary is the identity on every byte string the decoder accepts, which by [L-bson-1](#L-bson-1) and the corpus includes every canonical BSON of the corpus that is not a declared limit.

### Outside the domain

Instances that no document reads as are refused by `from_instance` with the node at fault; the fixed tests check those refusals, and the reference, which writes from documents, does not model them.

### Definitions used

The reference's `encode` writes the layout of the section on canonical output, and `tree` lists the nodes of the tree encoding in document order with each value written as its kind and its parts (a double by its bits, a datetime by the ISO text of its millisecond count with three fraction digits, a decimal by sign, coefficient and exponent).

### Proof

1. `encode` writes each element's type byte, key and value in the layout `decode` reads, with every length the length of what follows it, so `decode(encode(d)) = d` for every document whose values hold the rules `encode` checks; every decoded document holds them, since the decoder refuses what breaks them.
2. Conversely, for bytes $b$ that `decode` accepts, every length in $b$ equals the length of what it covers (the decoder checks each against the bytes read), array keys are the indices, the options sorted and each decimal128 canonical, so `encode(decode(b))` writes the same bytes.
3. `to_instance` creates one node per document, element and value in document order and `from_instance` reads them back by the same traversal, holding each value through the same pairing of BSON type and value kind; `Temporal::from_epoch_count` in milliseconds and `to_epoch_count` are inverse on the datetimes.
4. The exhaustive differential checks all four equalities on the 994 documents, and the property differential on generated documents.

### Executable counterpart

- Law ID: `bson.binary-writing`
- Reference: `panproto_reference::pages::bson::encode`
- Reference: `panproto_reference::pages::bson::tree`
- Enumerator: `panproto_reference::enumerate::bson::documents; bound: documents=994`
- Exhaustive differential test: `bson_writing_and_the_tree_encoding_match_the_reference_on_every_bounded_document; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_bson --no-tests=fail -E 'test(=bson_writing_and_the_tree_encoding_match_the_reference_on_every_bounded_document)'`
- Property differential test: `randomized_documents_encode_decode_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_bson --no-tests=fail -E 'test(=randomized_documents_encode_decode_and_write_as_the_reference)'`
- Reference test: `the_tree_encoding_numbers_nodes_in_document_order; command: cargo nextest run -p panproto-reference --test bson --no-tests=fail -E 'test(=the_tree_encoding_numbers_nodes_in_document_order)'`
- Reference test: `the_bson_scopes_have_their_closed_form_counts; command: cargo nextest run -p panproto-reference --test bson --no-tests=fail -E 'test(=the_bson_scopes_have_their_closed_form_counts)'`
- File: `crates/panproto-io/src/data_schema/bson/binary.rs`
- File: `crates/panproto-io/src/data_schema/bson/instance.rs`
- File: `crates/panproto-protocols/src/data_schema/bson.rs`
- File: `crates/panproto-reference/src/enumerate/bson.rs`

## Canonical Extended JSON reads back as the same document {#L-bson-3}

### Statement

For every document $d$ of the bounded scope, `write_extended_json(d, Canonical)` and `write_extended_json(d, Relaxed)` are the reference's `canonical_text(d)` and `relaxed_text(d)`, or refusals at the same JSON pointer with the same kind; when the canonical text is written, `read_extended_json` reads it back as $d$. For every text $t$ of the bounded text scope, `read_extended_json(t)` is the reference's `read_text(t)`: the same document, or a refusal at the same JSON pointer with the same kind.

### Outside the domain

The reference reads JSON with its own RFC 8259 reader, so refusals of the shared JSON route other than a repeated member name (a comment, a control character in a string) are the route's subject and are not compared. Byte offsets and messages are not compared; pointers and kinds are. RFC 3339 offsets beyond those of the scope are checked by the fixed tests.

### Definitions used

The reference writes the texts of the section on canonical output, doubles from Rust's shortest float formatting and decimal128 from the Decimal128 specification's rules, and reads texts by the rules of the section on reading. The text scope is the canonical and relaxed text of every document of the scope that has them ($1 + 28 + 28^2 = 813$ each, since three of the 31 values, the double NaN with a payload, the negative signalling decimal NaN with a payload and the embedded document with a `$oid` key, and the document with two elements `a` have none), and 62 texts that exercise each refusal and alternative form: 1,688 texts.

### Proof

1. Both writers visit elements in order and check, at each, the key, the earlier names and the wrapper keys before writing the value, and both refuse the same values; the writers' texts are then the same characters, since both escape strings the same way and write the same wrappers in the same member order, with the same double and decimal128 strings.
2. The canonical text of $d$ wraps every value whose type JSON cannot carry, writes each double, decimal and datetime by a string that reads back as the same value, and has no repeated member name and no wrapper key outside a wrapper, so the reader reads each wrapper back as the value it wrote and each other object as the document it wrote: the reading is $d$.
3. Both readers apply the rules of the section on reading in the same order (the top-level object, then each member's key and value in source order, a wrapper's keys before its value), so they agree on every text, and the exhaustive differential checks it: all 1,626 written texts read, and 24 of the 62 others (an integer at the int64 bound, `1.50`, a negative zero, wrapper keys in other orders, a `$uuid`, a date with an offset, a `DBRef` and `$regex` documents among them).

### Executable counterpart

- Law ID: `bson.extended-json`
- Reference: `panproto_reference::pages::bson::canonical_text`
- Reference: `panproto_reference::pages::bson::relaxed_text`
- Reference: `panproto_reference::pages::bson::read_text`
- Enumerator: `panproto_reference::enumerate::bson::texts; bound: texts=1688`
- Exhaustive differential test: `extended_json_matches_the_reference_on_every_bounded_document_and_text; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_bson --no-tests=fail -E 'test(=extended_json_matches_the_reference_on_every_bounded_document_and_text)'`
- Property differential test: `randomized_documents_encode_decode_and_write_as_the_reference; command: cargo nextest run -p panproto-io --features tree-sitter --test reference_bson --no-tests=fail -E 'test(=randomized_documents_encode_decode_and_write_as_the_reference)'`
- Reference test: `extended_json_writes_both_formats_and_refuses_what_does_not_read_back; command: cargo nextest run -p panproto-reference --test bson --no-tests=fail -E 'test(=extended_json_writes_both_formats_and_refuses_what_does_not_read_back)'`
- Reference test: `extended_json_reads_by_the_conversion_table_and_names_the_place; command: cargo nextest run -p panproto-reference --test bson --no-tests=fail -E 'test(=extended_json_reads_by_the_conversion_table_and_names_the_place)'`
- Reference test: `decimal128_and_double_strings_follow_the_page; command: cargo nextest run -p panproto-reference --test bson --no-tests=fail -E 'test(=decimal128_and_double_strings_follow_the_page)'`
- File: `crates/panproto-io/src/data_schema/bson/extjson_read.rs`
- File: `crates/panproto-io/src/data_schema/bson/extjson_write.rs`
- File: `crates/panproto-io/src/config/tree.rs`
- File: `crates/panproto-reference/src/pages/bson.rs`
