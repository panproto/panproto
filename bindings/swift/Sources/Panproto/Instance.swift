import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Instances against a schema

extension SchemaHandle {
    /// Read a JSON document as a W-type instance of this schema.
    ///
    /// The bytes are decoded as JSON, not as CBOR, so this is the entry
    /// point for a record that arrived over the wire in its own format.
    ///
    /// `rootVertex` names the schema vertex the document's top level
    /// sits over. The engine takes the first of three candidates that
    /// resolves: `rootVertex` when the schema declares a vertex by that
    /// name, then the schema's protocol name, then the schema's declared
    /// primary entry. Passing `nil` skips straight to the second
    /// candidate, which is what a schema built by a lexicon parser
    /// usually wants.
    ///
    /// ```swift
    /// let record = try Data(contentsOf: postURL)
    /// let post = try await schema.instance(fromJSON: record)
    /// ```
    ///
    /// - Parameters:
    ///   - json: the raw JSON bytes of one document.
    ///   - rootVertex: the vertex to anchor the document's top level to,
    ///     or `nil` to let the schema decide.
    /// - Returns: the instance the engine built.
    /// - Throws: ``PanprotoError/io(_:)`` when the bytes are not JSON,
    ///   when no root vertex resolves, or when the document does not fit
    ///   the schema.
    @PanprotoEngine
    public func instance(
        fromJSON json: Data,
        rootVertex: Name? = nil
    ) throws(PanprotoError) -> Instance {
        let operation = "SchemaHandle.instance(fromJSON:rootVertex:)"
        let parsed = Raw.instJsonToInstance(
            schemaHandle: rawValue,
            json: json,
            rootVertex: rootVertex ?? ""
        )
        try parsed.status.orThrow(.io, operation)
        return try Payload.decode(Instance.self, from: parsed.bytes, .io, operation)
    }

    /// Render an instance of this schema as JSON.
    ///
    /// The result is JSON bytes rather than a structural value, because
    /// JSON is the format being produced: what the caller does with it
    /// is write it to a file or put it in a request body.
    ///
    /// This is the inverse of ``instance(fromJSON:rootVertex:)`` up to
    /// what the schema describes. Fields the schema does not describe
    /// survive the round trip in `Node.extraFields`, so a document that
    /// carried them comes back carrying them.
    ///
    /// - Parameter instance: the instance to render.
    /// - Returns: the JSON bytes.
    /// - Throws: ``PanprotoError/io(_:)`` when the instance does not
    ///   encode, or when the engine refuses it against this schema.
    @PanprotoEngine
    public func json(for instance: Instance) throws(PanprotoError) -> Data {
        let operation = "SchemaHandle.json(for:)"
        let payload = try Payload.encode(instance, .io, operation)
        let rendered = Raw.instToJson(schemaHandle: rawValue, instance: payload)
        try rendered.status.orThrow(.io, operation)
        return rendered.bytes
    }

    /// Check an instance against this schema, answering every violation.
    ///
    /// An empty result means the instance is valid. A non-empty one is
    /// not an error: the pass reports violations as messages and
    /// reserves throwing for inputs that stop it running at all, such as
    /// a payload that does not decode. A caller that wants a violation
    /// to be fatal writes that policy itself.
    ///
    /// ```swift
    /// let found = try await schema.violations(in: post)
    /// guard found.isEmpty else { throw MyError.rejected(found) }
    /// ```
    ///
    /// - Parameter instance: the instance to check.
    /// - Returns: one message per violation, empty when the instance is
    ///   valid.
    /// - Throws: ``PanprotoError/io(_:)`` when validation could not run.
    @PanprotoEngine
    public func violations(in instance: Instance) throws(PanprotoError) -> [String] {
        let operation = "SchemaHandle.violations(in:)"
        let payload = try Payload.encode(instance, .io, operation)
        let checked = Raw.instValidate(schemaHandle: rawValue, instance: payload)
        try checked.status.orThrow(.io, operation)
        return try Payload.decode([String].self, from: checked.bytes, .io, operation)
    }
}

// MARK: - Counting through the engine

extension Instance {
    /// The number of nodes the engine counts in this instance.
    ///
    /// `nodeCount` counts the nodes this value holds and needs no
    /// engine. This encodes the instance, hands the bytes across the
    /// boundary, and answers what the engine counted after decoding
    /// them, which makes it a check on the payload rather than on the
    /// value: the two disagreeing means the bytes Swift wrote are not
    /// the instance Swift holds.
    ///
    /// - Returns: the node count the engine read out of the payload.
    /// - Throws: ``PanprotoError/io(_:)`` when the instance does not
    ///   encode, or when the engine cannot decode what it was handed.
    @PanprotoEngine
    public func elementCount() throws(PanprotoError) -> Int {
        let operation = "Instance.elementCount()"
        let payload = try Payload.encode(self, .io, operation)
        let counted = Raw.instElementCount(instance: payload)
        try counted.status.orThrow(.io, operation)
        return Int(counted.count)
    }
}
