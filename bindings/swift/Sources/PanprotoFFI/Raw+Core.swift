import CPanproto
import Foundation
import PanprotoStructural

// MARK: - Namespace

/// The raw shim layer over the panproto-c ABI.
///
/// Each static method forwards to exactly one `pp_*` entry point,
/// renamed by dropping the `pp_` prefix and converting the remainder
/// from snake_case to lowerCamelCase. The methods bridge Swift values
/// to the two ABI payload shapes and back: borrowed input slices
/// (`slice_ref_uint8_t`, produced by ``withPpSlice(_:_:)``) and owned
/// output buffers (`Vec_uint8_t`, drained and freed by
/// ``withPpOutBuffer(_:)``). Nothing here interprets a payload; CBOR
/// and JSON decoding belongs to the layers above.
///
/// Handles index a process-global slab held behind a mutex, so a handle
/// is valid from whichever thread reaches for it, and every handle goes
/// back through ``handleFree(_:)`` when the host is done with it. The
/// thread-affine part of the ABI is the last-error slot: a failing entry
/// point stashes its envelope where only the calling thread can drain
/// it, so ``lastErrorTake()`` has to run on the thread that failed.
///
/// Call ``initialize()`` once per process before anything else so that
/// caught panics stop printing to stderr.
public enum Raw {}

// MARK: - Lifecycle

// `pp_buf_free` has no shim of its own: it is bound in
// `Primitives.swift`, where `drainPpBuffer(_:)` calls it on every owned
// buffer the engine hands back. Exposing a second entry point would
// invite the double-free the contract forbids.

extension Raw {
    /// Initialize the panproto-c runtime.
    ///
    /// Installs the process-global Rust panic hook that suppresses the
    /// default stderr output; panics stay observable because every
    /// entry point catches them and stashes the message in the
    /// thread-local last-error slot, which ``lastErrorTake()`` drains.
    /// Idempotent, and always answers ``RawStatus/ok``. No payload
    /// crosses the boundary.
    @inlinable
    public static func initialize() -> RawStatus {
        RawStatus(code: pp_init())
    }

    /// Free a slab handle, marking its slot reusable.
    ///
    /// `handle` may name any slab variant. Double-free is safe: a freed
    /// slot stays freed. No payload crosses the boundary.
    @inlinable
    public static func handleFree(_ handle: UInt32) -> RawStatus {
        RawStatus(code: pp_handle_free(handle))
    }

    /// Take the pending error envelope, clearing the last-error slot.
    ///
    /// The returned bytes are a CBOR-encoded `ErrorEnvelope`. An empty
    /// buffer means no error was pending, which is reported alongside
    /// ``RawStatus/ok``: the absence of an error is not itself a
    /// failure. No handles are involved.
    @inlinable
    public static func lastErrorTake() -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_last_error_take(out) }
    }
}

// MARK: - Protocol
//
// Every method that yields a handle reports it alongside a status. On
// any status other than `.ok` the handle is meaningless: the engine
// either left the out-parameter untouched or never reached the
// allocation, so the caller must check the status before using it. That
// holds for every out-handle in this file.

extension Raw {
    /// Ingest a protocol specification and register it in the slab.
    ///
    /// `spec` is a CBOR-encoded `Protocol`. On success the returned
    /// handle names a fresh `Protocol` slab entry. A CBOR decode
    /// failure answers ``RawStatus/serialization``.
    @inlinable
    public static func protocolDefine(spec: Data) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(spec) { spec in pp_protocol_define(spec, &handle) }
        return (RawStatus(code: code), handle)
    }

    /// Serialize a registered protocol to CBOR.
    ///
    /// `proto` must be a `Protocol` handle. The returned bytes are a
    /// CBOR-encoded `Protocol`. A handle that is stale or points at
    /// another slab variant answers ``RawStatus/invalidHandle`` or
    /// ``RawStatus/typeMismatch``.
    @inlinable
    public static func protocolSerialize(proto: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_protocol_serialize(proto, out) }
    }
}

// MARK: - Schema

extension Raw {
    /// Build a schema by replaying a list of builder operations against
    /// a protocol.
    ///
    /// `proto` must be a `Protocol` handle. `ops` is a CBOR-encoded
    /// `Vec<BuildOp>`. On success the returned handle names a fresh
    /// `Schema` slab entry.
    @inlinable
    public static func schemaBuild(
        proto: UInt32,
        ops: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(ops) { ops in pp_schema_build(proto, ops, &handle) }
        return (RawStatus(code: code), handle)
    }

    /// Deserialize a schema into the slab.
    ///
    /// `spec` is a CBOR-encoded `Schema`. On success the returned handle
    /// names a fresh `Schema` slab entry. A CBOR decode failure answers
    /// ``RawStatus/serialization`` and leaves the handle untouched.
    @inlinable
    public static func schemaFromCbor(spec: Data) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(spec) { spec in pp_schema_from_cbor(spec, &handle) }
        return (RawStatus(code: code), handle)
    }

    /// Extract a schema's protocol name, vertices, and edges.
    ///
    /// `schemaHandle` must be a `Schema` handle. The returned bytes are
    /// a CBOR-encoded record with `protocol`, `vertices`, and `edges`
    /// fields.
    @inlinable
    public static func schemaMetadata(schemaHandle: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_schema_metadata(schemaHandle, out) }
    }

    /// Normalize a schema by collapsing its reference chains.
    ///
    /// `schemaHandle` must be a `Schema` handle. On success the returned
    /// handle names a fresh `Schema` slab entry holding the normalized
    /// schema; the input handle stays valid and unchanged.
    @inlinable
    public static func schemaNormalize(
        schemaHandle: UInt32
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_schema_normalize(schemaHandle, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Parse an ATProto lexicon document into a schema.
    ///
    /// `json` is raw JSON bytes, decoded with `serde_json` rather than
    /// as CBOR. On success the returned handle names a fresh `Schema`
    /// slab entry.
    @inlinable
    public static func schemaParseAtprotoLexicon(
        json: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(json) { json in
            pp_schema_parse_atproto_lexicon(json, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Serialize a registered schema to CBOR.
    ///
    /// `schemaHandle` must be a `Schema` handle. The returned bytes are
    /// a CBOR-encoded `Schema`.
    @inlinable
    public static func schemaToCbor(schemaHandle: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_schema_to_cbor(schemaHandle, out) }
    }

    /// Validate a schema against a protocol.
    ///
    /// `schemaHandle` must be a `Schema` handle and `protoHandle` a
    /// `Protocol` handle. The returned bytes are a CBOR-encoded
    /// `Vec<String>` of human-readable messages; an empty list means the
    /// schema is valid. A completed validation pass answers
    /// ``RawStatus/ok`` whether or not the schema passed, so violations
    /// arrive as messages rather than as a failing status; a non-ok
    /// status means validation could not run at all.
    @inlinable
    public static func schemaValidate(
        schemaHandle: UInt32,
        protoHandle: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_schema_validate(schemaHandle, protoHandle, out) }
    }
}

// MARK: - Check

extension Raw {
    /// Classify a schema diff against a protocol, producing a
    /// compatibility report.
    ///
    /// `proto` must be a `Protocol` handle. `diff` is a CBOR-encoded
    /// `check::SchemaDiff`, as produced by ``checkDiffFull(s1:s2:)``.
    /// The returned bytes are a CBOR-encoded `check::CompatReport`. A
    /// `diff` that is not a valid CBOR `check::SchemaDiff` answers
    /// ``RawStatus/serialization``.
    @inlinable
    public static func checkClassify(
        proto: UInt32,
        diff: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(diff) { diff in
            withPpOutBuffer { out in pp_check_classify(proto, diff, out) }
        }
    }

    /// Diff two schemas across the full set of change categories.
    ///
    /// `s1` and `s2` must both be `Schema` handles. The returned bytes
    /// are a CBOR-encoded `check::SchemaDiff` covering constraints,
    /// hyper-edges, variants, recursion points, usage modes, spans, and
    /// nominal-identity changes.
    @inlinable
    public static func checkDiffFull(s1: UInt32, s2: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_check_diff_full(s1, s2, out) }
    }

    /// Diff two schemas at the vertex and edge level.
    ///
    /// `s1` and `s2` must both be `Schema` handles. The returned bytes
    /// are a CBOR-encoded structural `SchemaDiff`, the lightweight shape
    /// the helper diff produces rather than the full report from
    /// ``checkDiffFull(s1:s2:)``.
    @inlinable
    public static func checkDiffSimple(s1: UInt32, s2: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_check_diff_simple(s1, s2, out) }
    }

    /// Render a compatibility report as a JSON document.
    ///
    /// `report` is a CBOR-encoded `check::CompatReport`. The returned
    /// bytes are UTF-8 JSON, not CBOR. A `report` that does not decode
    /// answers ``RawStatus/serialization``. No handles are involved.
    @inlinable
    public static func checkReportJson(report: Data) -> (status: RawStatus, bytes: Data) {
        withPpSlice(report) { report in
            withPpOutBuffer { out in pp_check_report_json(report, out) }
        }
    }

    /// Render a compatibility report as human-readable text.
    ///
    /// `report` is a CBOR-encoded `check::CompatReport`. The returned
    /// bytes are UTF-8 text, not CBOR. A `report` that does not decode
    /// answers ``RawStatus/serialization``. No handles are involved.
    @inlinable
    public static func checkReportText(report: Data) -> (status: RawStatus, bytes: Data) {
        withPpSlice(report) { report in
            withPpOutBuffer { out in pp_check_report_text(report, out) }
        }
    }
}

// MARK: - Instance

extension Raw {
    /// Count the nodes in a W-type instance.
    ///
    /// `instance` is a CBOR-encoded `Instance`. The returned value is
    /// the node count. No handles are involved.
    @inlinable
    public static func instElementCount(instance: Data) -> (status: RawStatus, count: UInt32) {
        var count: UInt32 = 0
        let code = withPpSlice(instance) { instance in pp_inst_element_count(instance, &count) }
        return (RawStatus(code: code), count)
    }

    /// Parse JSON bytes into a W-type instance anchored at a schema.
    ///
    /// `schemaHandle` must be a `Schema` handle. `json` is raw JSON
    /// bytes, decoded with `serde_json` rather than as CBOR.
    /// `rootVertex` is the UTF-8 name of the root vertex; passing the
    /// empty string infers it. The returned bytes are a CBOR-encoded
    /// `Instance`.
    ///
    /// Root selection takes the first of these that resolves: the
    /// explicit `rootVertex` when the schema declares it, then the
    /// schema's protocol name, then the schema's declared primary entry.
    @inlinable
    public static func instJsonToInstance(
        schemaHandle: UInt32,
        json: Data,
        rootVertex: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(json, rootVertex) { json, rootVertex in
            withPpOutBuffer { out in
                pp_inst_json_to_instance(schemaHandle, json, rootVertex, out)
            }
        }
    }

    /// Render a W-type instance as JSON.
    ///
    /// `schemaHandle` must be a `Schema` handle. `instance` is a
    /// CBOR-encoded `Instance`. The returned bytes are JSON, not CBOR.
    @inlinable
    public static func instToJson(
        schemaHandle: UInt32,
        instance: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(instance) { instance in
            withPpOutBuffer { out in pp_inst_to_json(schemaHandle, instance, out) }
        }
    }

    /// Validate a W-type instance against a schema.
    ///
    /// `schemaHandle` must be a `Schema` handle. `instance` is a
    /// CBOR-encoded `Instance`. The returned bytes are a CBOR-encoded
    /// `Vec<String>` of validation messages; an empty list means the
    /// instance is valid. A completed pass answers ``RawStatus/ok``
    /// whether or not the instance passed, so violations arrive as
    /// messages; a non-ok status is reserved for inputs that stop
    /// validation from running, such as a bad handle or undecodable
    /// instance bytes.
    @inlinable
    public static func instValidate(
        schemaHandle: UInt32,
        instance: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(instance) { instance in
            withPpOutBuffer { out in pp_inst_validate(schemaHandle, instance, out) }
        }
    }
}

// MARK: - Registry

extension Raw {
    /// Emit an instance to a protocol's native format bytes.
    ///
    /// `registry` must be an `IoRegistry` handle and `schemaHandle` a
    /// `Schema` handle. `protoName` is the UTF-8 name of a codec
    /// registered in `registry`. `instance` is the CBOR-encoded instance
    /// (a `Instance` or an `FInstance`, per the protocol's native
    /// representation). The returned bytes are the raw format bytes, not
    /// CBOR. Instance bytes that do not decode answer
    /// ``RawStatus/serialization``; an unknown protocol or a failing
    /// emit answers ``RawStatus/operation``.
    @inlinable
    public static func ioEmitInstance(
        registry: UInt32,
        protoName: String,
        schemaHandle: UInt32,
        instance: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(protoName, instance) { protoName, instance in
            withPpOutBuffer { out in
                pp_io_emit_instance(registry, protoName, schemaHandle, instance, out)
            }
        }
    }

    /// List the protocol names a registry carries codecs for.
    ///
    /// `registry` must be an `IoRegistry` handle. The returned bytes are
    /// a CBOR-encoded `Vec<String>`.
    @inlinable
    public static func ioListProtocols(registry: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_io_list_protocols(registry, out) }
    }

    /// Parse a protocol's native format bytes into an instance.
    ///
    /// `registry` must be an `IoRegistry` handle and `schemaHandle` a
    /// `Schema` handle. `protoName` is the UTF-8 name of a codec
    /// registered in `registry`. `input` is the raw format bytes. The
    /// returned bytes are the CBOR-encoded instance, a `Instance` or an
    /// `FInstance` according to the protocol's native representation. An
    /// unknown protocol or a failing parse answers
    /// ``RawStatus/operation``.
    @inlinable
    public static func ioParseInstance(
        registry: UInt32,
        protoName: String,
        schemaHandle: UInt32,
        input: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(protoName, input) { protoName, input in
            withPpOutBuffer { out in
                pp_io_parse_instance(registry, protoName, schemaHandle, input, out)
            }
        }
    }

    /// Create an I/O registry holding every built-in protocol codec.
    ///
    /// On success the returned handle names a fresh `IoRegistry` slab
    /// entry. No payload crosses the boundary.
    @inlinable
    public static func ioRegisterProtocols() -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_io_register_protocols(&handle)
        return (RawStatus(code: code), handle)
    }

    /// Look up a built-in protocol specification by name.
    ///
    /// `name` is the UTF-8 protocol name. The returned bytes are a
    /// CBOR-encoded `Protocol`, ready for
    /// ``protocolDefine(spec:)``. A name outside the built-in catalogue
    /// answers ``RawStatus/operation``. No handles are involved.
    @inlinable
    public static func registryGetBuiltin(name: String) -> (status: RawStatus, bytes: Data) {
        withPpSlice(name) { name in
            withPpOutBuffer { out in pp_registry_get_builtin(name, out) }
        }
    }

    /// List the names of every built-in semantic protocol.
    ///
    /// The returned bytes are a CBOR-encoded `Vec<String>`, each name
    /// accepted by ``registryGetBuiltin(name:)``. No handles are
    /// involved.
    @inlinable
    public static func registryListBuiltin() -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in pp_registry_list_builtin(out) }
    }
}
