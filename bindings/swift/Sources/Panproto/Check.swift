import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Diffing two schemas

extension SchemaHandle {
    /// Compare this schema against `other` across every change category
    /// the engine recognizes.
    ///
    /// This is the diff to reach for. It reports vertices, edges,
    /// constraints, hyper-edges, required edges, namespace ids,
    /// variants, orderings, recursion points, usage modes, spans, the
    /// nominal flag, the four enrichment maps, and the renames the
    /// engine detected from a removal paired with an addition. It is
    /// also the only shape ``ProtocolHandle/classify(_:)`` reads, so a
    /// compatibility verdict starts here rather than at
    /// ``structuralDiff(to:)``.
    ///
    /// The receiver is the older schema and `other` the newer one:
    /// `SchemaDiff.addedVertices` names what `other` carries and this
    /// schema does not, and `SchemaDiff.removedVertices` the reverse.
    ///
    /// ```swift
    /// let diff = try await post.diff(to: profile)
    /// let report = try await atproto.classify(diff)
    /// ```
    ///
    /// - Parameter other: The newer schema.
    /// - Returns: Every change leading from this schema to `other`.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/check(_:)``
    ///   domain when either handle names something other than a live
    ///   schema, or when the payload the engine wrote does not decode.
    @PanprotoEngine
    public func diff(to other: SchemaHandle) throws(PanprotoError) -> SchemaDiff {
        let result = Raw.checkDiffFull(s1: rawValue, s2: other.rawValue)
        try result.status.orThrow(.check, "SchemaHandle.diff")
        return try Payload.decode(
            SchemaDiff.self,
            from: result.bytes,
            .check, "SchemaHandle.diff"
        )
    }

    /// Compare this schema against `other` at the vertex and edge level
    /// alone.
    ///
    /// Reach for this when the question is which vertices and edges
    /// moved and nothing else: the engine walks the two vertex maps and
    /// the two edge maps and stops there, so constraints, hyper-edges,
    /// variants, orderings, recursion points, usage modes, spans, and
    /// the enrichment maps are all outside what it reports.
    /// ``diff(to:)`` covers those, and it is the diff a compatibility
    /// verdict needs; the classifier reads a `SchemaDiff` and does not
    /// accept a `StructuralDiff`, which spells its edge and kind-change
    /// entries differently as well.
    ///
    /// The receiver is the older schema and `other` the newer one, as in
    /// ``diff(to:)``.
    ///
    /// - Parameter other: The newer schema.
    /// - Returns: The vertices and edges that appeared or disappeared,
    ///   and the vertices that changed kind.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/check(_:)``
    ///   domain when either handle names something other than a live
    ///   schema, or when the payload the engine wrote does not decode.
    @PanprotoEngine
    public func structuralDiff(to other: SchemaHandle) throws(PanprotoError) -> StructuralDiff {
        let result = Raw.checkDiffSimple(s1: rawValue, s2: other.rawValue)
        try result.status.orThrow(.check, "SchemaHandle.structuralDiff")
        return try Payload.decode(
            StructuralDiff.self,
            from: result.bytes,
            .check, "SchemaHandle.structuralDiff"
        )
    }
}

// MARK: - Classifying a diff

extension ProtocolHandle {
    /// Judge a diff against this protocol, sorting every change into the
    /// breaking and the non-breaking list.
    ///
    /// The protocol supplies the rules the verdict rests on: which edge
    /// kinds it governs, which constraint sorts it declares, and so on.
    /// The same diff therefore classifies differently under two
    /// protocols, which is why the entry point takes one rather than
    /// deriving it from the schemas.
    ///
    /// `diff` must come from ``SchemaHandle/diff(to:)``. The engine
    /// refuses the lightweight shape ``SchemaHandle/structuralDiff(to:)``
    /// answers with, and reshaping it would not recover the categories
    /// the classifier reads.
    ///
    /// - Parameter diff: The full diff to classify.
    /// - Returns: The breaking and non-breaking changes, together with
    ///   the tier they put the revision in.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/check(_:)``
    ///   domain when this handle names something other than a live
    ///   protocol, when the engine refuses the encoded diff, or when
    ///   either payload does not survive its codec.
    @PanprotoEngine
    public func classify(_ diff: SchemaDiff) throws(PanprotoError) -> CompatReport {
        let encoded = try Payload.encode(diff, .check, "ProtocolHandle.classify")
        let result = Raw.checkClassify(proto: rawValue, diff: encoded)
        try result.status.orThrow(.check, "ProtocolHandle.classify")
        return try Payload.decode(
            CompatReport.self,
            from: result.bytes,
            .check, "ProtocolHandle.classify"
        )
    }
}

// MARK: - Rendering a report

extension CompatReport {
    /// Render this report as the engine's own human-readable summary.
    ///
    /// The text is the engine's, not this package's: it groups the
    /// changes by list, names each one in the engine's vocabulary, and
    /// leads with the verdict. Rendering here rather than in Swift is
    /// what keeps a report printed by the CLI and a report printed by a
    /// Swift host reading alike.
    ///
    /// - Returns: The rendered summary, decoded from the engine's UTF-8
    ///   bytes.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/check(_:)``
    ///   domain when the engine refuses the encoded report.
    @PanprotoEngine
    public func renderedText() throws(PanprotoError) -> String {
        let encoded = try Payload.encode(self, .check, "CompatReport.renderedText")
        let result = Raw.checkReportText(report: encoded)
        try result.status.orThrow(.check, "CompatReport.renderedText")
        return String(decoding: result.bytes, as: UTF8.self)
    }

    /// Render this report as a JSON document.
    ///
    /// The bytes are UTF-8 JSON rather than CBOR, and this package
    /// declares no wire type for the shape the engine builds, so they
    /// arrive opaque: hand them to `JSONSerialization`, to a
    /// `JSONDecoder` against a type of your own, or straight to whatever
    /// consumes the report. Everything the document holds is also on the
    /// report itself, so decoding it back into Swift buys nothing that
    /// ``CompatReport`` does not already give you.
    ///
    /// - Returns: The JSON document, as the engine serialized it.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/check(_:)``
    ///   domain when the engine refuses the encoded report or cannot
    ///   serialize the document it built.
    @PanprotoEngine
    public func renderedJSON() throws(PanprotoError) -> Data {
        let encoded = try Payload.encode(self, .check, "CompatReport.renderedJSON")
        let result = Raw.checkReportJson(report: encoded)
        try result.status.orThrow(.check, "CompatReport.renderedJSON")
        return result.bytes
    }
}
