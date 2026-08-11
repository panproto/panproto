# The CBOR codec

Why this package ships an encoder and a decoder of its own, what they
guarantee, and why conformance to the engine is checked over decoded
values rather than over bytes.

## Overview

Every payload crossing panproto's C ABI is CBOR. The engine writes it
with [`ciborium`](https://docs.rs/ciborium) driven by
[`serde`](https://serde.rs/), and reads it back the same way. That is a
narrower target than "CBOR" in general: what has to line up is not the
byte format but `serde`'s data model, which decides how a struct, an
`Option`, an enum, and a `Vec<u8>` are spelled.

``CBOREncoder`` and ``CBORDecoder`` are written against that model. Both
conform to Swift's `Encoder` and `Decoder`, so an ordinary `Codable`
conformance works and the wire types in this module are mostly
synthesized. The two are asymmetric on purpose: writing is strict, and
reading is tolerant.

## What the encoder writes

The mapping from Swift to `serde`'s model is fixed, and worth stating
because a few of the pairings are not the obvious ones.

- A `Codable` struct becomes a definite-length map with text keys in
  declaration order, matching a Rust struct.
- A `nil` written with `encode(_:forKey:)` becomes CBOR null, and a
  non-`nil` optional becomes the wrapped value itself, matching `Option`.
  A synthesized conformance reaches for `encodeIfPresent` instead, which
  leaves the key out; the engine reads an absent field for an `Option` as
  `None`, so both spellings arrive.
- A `Dictionary` with `String` keys becomes a map with text keys, and one
  with `Int` keys becomes a map with integer keys.
- `Data` becomes a byte string, which the engine accepts both for a
  `serde_bytes` field and for a plain `Vec<u8>`. An `[UInt8]` becomes an
  array of integers, which the engine also accepts for both.
- A ``CBORValue`` is written through unchanged, so a payload can carry a
  fragment no Swift type describes.

A Rust enum reaches the wire externally tagged: a unit variant is a text
string, and every other variant is a one-entry map keyed by the variant
name whose value is the payload. A Swift enum reproduces that by encoding
a string into a single-value container for a unit case, and by opening a
one-key keyed container otherwise.

The output is deterministic. Lengths are always definite, every integer
head is the shortest that fits, a float takes the narrowest of half,
single, and double precision that reproduces it exactly, and collections
carrying no order of their own (`Dictionary`, `Set`) are sorted by the
encoded bytes of their keys or elements, which is the ordering RFC 8949
calls canonical. Two encodes of one value therefore agree byte for byte.

```swift
func catalogEncode(_ schema: Schema) throws -> Data {
    try CBOREncoder().encode(schema)
}
```

## What the decoder accepts

Reading is deliberately looser, because a host is expected to keep
working against an engine that has learned new fields.

- Definite and indefinite lengths both parse, for maps, arrays, byte
  strings, and text strings.
- Unrecognized map keys are skipped rather than refused.
- Half, single, and double precision floats all parse.
- Semantic tags are read through: a tagged item decodes as the item it
  tags, and the bignum tags 2 and 3 additionally decode as integers. A
  field typed as ``CBORValue`` keeps its tags.
- A `UInt64` above `Int64.max` decodes without loss.

Tolerance stops at the payload boundary. A payload is exactly one CBOR
item, and trailing bytes are an error, which is the rule the engine's own
decoder applies to the same bytes.

```swift
func catalogDecode(_ bytes: Data) throws -> Schema {
    try CBORDecoder().decode(Schema.self, from: bytes)
}
```

Failures split by where they arise. ``CBORError`` covers the bytes: the
input ran out, it is not well-formed CBOR, or it is well-formed and this
host declines it (a length past what the platform can address, nesting
too deep, trailing bytes). Every case reports the byte offset the trouble
starts at. Once the bytes have parsed, a field of the wrong type or a key
that is not there surfaces as Swift's own `DecodingError`, and a value
that declines to encode surfaces as `EncodingError`.

## Reading a payload with no Swift type

``CBORValue`` holds an item in the shape RFC 8949 gives it, with one case
per major type. It decodes anything well-formed, which is how a host
inspects an answer this package does not model: a field a newer engine
grew, a fragment of a protocol-specific record, or a payload being
diagnosed.

```swift
func catalogTextField(named key: String, in payload: Data) throws -> String? {
    guard case .map(let entries) = try CBORDecoder().decode(CBORValue.self, from: payload) else {
        return nil
    }
    return entries.first { $0.key == .textString(key) }?.value.stringValue
}
```

Two representational choices in ``CBORValue`` follow from wanting a
decoded item to re-encode to the bytes it came from. Negative integers
are stored the way the wire stores them: ``CBORValue/negative(_:)``
carries `n` and denotes `-1 - n`, so the range down to `-2^64` survives
even though it does not fit in `Int64`. And a map is an ordered list of
``CBORValue/Entry`` rather than a dictionary, because key order is part
of the payload, keys need not be strings, and duplicate keys are
representable; a dictionary would lose all three.

## Conformance is structural, not byte-level

The obvious test for a wire type would be that encoding a value
reproduces the engine's bytes. That test is wrong here, and it is worth
being explicit about why.

Most of the engine's schema and instance fields are Rust `HashMap`s.
`ciborium` writes a map in the order the map iterates, and a `HashMap`'s
iteration order depends on a hash seed rather than on the data, so the
engine does not emit one schema as the same bytes twice. It is not a bug
to fix on the Swift side: a byte comparison would fail against a correct
engine.

What does hold is that the *value* survives the trip. Decoding what the
engine wrote, encoding it again with this encoder, handing the result
back to the engine, and reading it out once more yields a value equal to
the first one. That is the round trip the fixture tests assert, and it is
the claim to reach for when a wire type is added or changed.

```swift
func catalogDescribeTheSameSchema(_ left: Data, _ right: Data) throws -> Bool {
    try CBORDecoder().decode(Schema.self, from: left)
        == CBORDecoder().decode(Schema.self, from: right)
}
```

Determinism therefore lives on one side of the boundary only. This
encoder is reproducible, which is what makes a Swift-built payload
hashable, cacheable, and comparable against itself. The engine's writer
is not, and nothing in this package pretends otherwise.

The one place where field order does carry meaning is the pair arrays.
A Rust map keyed by something other than a string cannot be a CBOR map
with text keys, so the engine writes it as an array of two-element
arrays. Those decode into Swift dictionaries, and ``WireMap/pairs(of:)``
writes them back out ordered by the encoded key, so a value assembled in
Swift always produces one byte string even though the map it came from
has no order.

## Topics

### Coding

- ``CBOREncoder``
- ``CBORDecoder``
- ``CBORError``

### Inspecting a payload

- ``CBORValue``

### Maps the wire spells as pairs

- ``WirePair``
- ``WireTriple``
- ``WireMap``
- ``UInt32KeyedMap``
