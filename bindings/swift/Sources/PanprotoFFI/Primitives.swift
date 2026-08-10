import CPanproto
import Foundation
import PanprotoStructural

// MARK: - Borrowed input slices

/// A one-byte allocation that stands in for the base address of an
/// empty buffer.
///
/// Both ABI byte types spell their pointer non-null on the Rust side:
/// `slice_ref_uint8_t.ptr` is a `NonNull<u8>` and `Vec_uint8_t.ptr` is a
/// `NonNullOwned<u8>`. The engine checks that on entry and rejects a
/// null in either, `len` and `cap` of zero notwithstanding. Swift
/// produces a null on both sides of that requirement:
/// `Data.withUnsafeBytes` hands back a null base address for empty
/// storage, and an owned output buffer has no address at all until the
/// engine writes one. Pointing both at a live byte with a zero length
/// satisfies the check: the engine reads nothing through it, and the
/// address is valid and aligned for `UInt8`. A `Vec_uint8_t` built on it
/// also frees cleanly, since Rust deallocates a `Vec` only when its
/// capacity is non-zero.
///
/// The allocation lives for the process; it is one byte, written once
/// before any concurrent read, which is what `nonisolated(unsafe)`
/// asserts here.
@usableFromInline
nonisolated(unsafe) internal let emptyBufferSentinel: UnsafeMutablePointer<UInt8> = {
    let pointer = UnsafeMutablePointer<UInt8>.allocate(capacity: 1)
    pointer.initialize(to: 0)
    return pointer
}()

/// Call `body` with a borrowed slice over `bytes`.
///
/// The slice is valid only for the duration of `body`; the engine
/// copies anything it needs before returning, so nothing escapes.
@inlinable
public func withPpSlice<R>(
    _ bytes: Data,
    _ body: (slice_ref_uint8_t) throws -> R
) rethrows -> R {
    try bytes.withUnsafeBytes { raw in
        try body(makePpSlice(raw))
    }
}

/// Call `body` with a borrowed slice over the UTF-8 encoding of `text`.
@inlinable
public func withPpSlice<R>(
    _ text: String,
    _ body: (slice_ref_uint8_t) throws -> R
) rethrows -> R {
    var text = text
    return try text.withUTF8 { raw in
        try body(makePpSlice(UnsafeRawBufferPointer(raw)))
    }
}

/// Build a `slice_ref_uint8_t` over a raw buffer, substituting the
/// sentinel pointer when the buffer is empty.
@inlinable
public func makePpSlice(_ raw: UnsafeRawBufferPointer) -> slice_ref_uint8_t {
    guard let base = raw.baseAddress, raw.count > 0 else {
        return slice_ref_uint8_t(ptr: UnsafePointer(emptyBufferSentinel), len: 0)
    }
    return slice_ref_uint8_t(
        ptr: base.assumingMemoryBound(to: UInt8.self),
        len: raw.count
    )
}

/// Call `body` with two borrowed slices held live simultaneously.
@inlinable
public func withPpSlices<A: PpSliceConvertible, B: PpSliceConvertible, R>(
    _ a: A,
    _ b: B,
    _ body: (slice_ref_uint8_t, slice_ref_uint8_t) throws -> R
) rethrows -> R {
    try a.withPpSlice { sa in
        try b.withPpSlice { sb in
            try body(sa, sb)
        }
    }
}

/// Call `body` with three borrowed slices held live simultaneously.
@inlinable
public func withPpSlices<A: PpSliceConvertible, B: PpSliceConvertible, C: PpSliceConvertible, R>(
    _ a: A,
    _ b: B,
    _ c: C,
    _ body: (slice_ref_uint8_t, slice_ref_uint8_t, slice_ref_uint8_t) throws -> R
) rethrows -> R {
    try a.withPpSlice { sa in
        try b.withPpSlice { sb in
            try c.withPpSlice { sc in
                try body(sa, sb, sc)
            }
        }
    }
}

/// Call `body` with four borrowed slices held live simultaneously.
@inlinable
public func withPpSlices<
    A: PpSliceConvertible, B: PpSliceConvertible, C: PpSliceConvertible, D: PpSliceConvertible, R
>(
    _ a: A,
    _ b: B,
    _ c: C,
    _ d: D,
    _ body: (slice_ref_uint8_t, slice_ref_uint8_t, slice_ref_uint8_t, slice_ref_uint8_t) throws -> R
) rethrows -> R {
    try a.withPpSlice { sa in
        try b.withPpSlice { sb in
            try c.withPpSlice { sc in
                try d.withPpSlice { sd in
                    try body(sa, sb, sc, sd)
                }
            }
        }
    }
}

/// A value that can lend its bytes to the engine as a borrowed slice.
///
/// The two conformances are ``Foundation/Data`` (CBOR, JSON, and raw
/// source payloads) and ``Swift/String`` (the UTF-8 name, path, and
/// format arguments the ABI takes as byte slices).
public protocol PpSliceConvertible {
    /// Call `body` with a slice borrowing this value's bytes.
    func withPpSlice<R>(_ body: (slice_ref_uint8_t) throws -> R) rethrows -> R
}

extension Data: PpSliceConvertible {
    /// Lends this buffer's bytes to the engine for the call's duration.
    @inlinable
    public func withPpSlice<R>(_ body: (slice_ref_uint8_t) throws -> R) rethrows -> R {
        try withUnsafeBytes { try body(makePpSlice($0)) }
    }
}

extension String: PpSliceConvertible {
    /// Lends this string's UTF-8 bytes to the engine for the call's duration.
    @inlinable
    public func withPpSlice<R>(_ body: (slice_ref_uint8_t) throws -> R) rethrows -> R {
        var copy = self
        return try copy.withUTF8 { try body(makePpSlice(UnsafeRawBufferPointer($0))) }
    }
}

// MARK: - Owned output buffers

/// An empty `Vec_uint8_t` the engine accepts as an out-parameter.
///
/// This is the value every owned output buffer starts at. The engine
/// takes `Vec_uint8_t *out` as a Rust `&mut Vec<u8>`, so the record has
/// to be a valid empty vector before the call, not zeroed storage: the
/// assignment that writes the result drops whatever was there first. A
/// non-null pointer with a zero capacity is the vector that drop leaves
/// alone.
@inlinable
public func makeEmptyPpBuffer() -> Vec_uint8_t {
    Vec_uint8_t(ptr: emptyBufferSentinel, len: 0, cap: 0)
}

/// Copy an owned `Vec_uint8_t` into `Data` and free it.
///
/// Every `Vec_uint8_t` the engine writes is owned by the caller and
/// must go back through `pp_buf_free` exactly once. Copying first and
/// freeing immediately means no `Data` ever borrows engine-owned
/// storage, so a buffer cannot outlive its allocation. The record is
/// spent once this returns: passing the same one again is the
/// double-free the contract forbids.
@inlinable
public func drainPpBuffer(_ buffer: consuming Vec_uint8_t) -> Data {
    let out: Data
    if let pointer = buffer.ptr, buffer.len > 0 {
        out = Data(bytes: pointer, count: buffer.len)
    } else {
        out = Data()
    }
    pp_buf_free(buffer)
    return out
}

/// Run `call` against an empty output buffer, then drain it.
///
/// The buffer is drained on every path, success or failure: a failing
/// entry point may still have written a partial buffer, and the
/// contract makes freeing it the host's job either way. An entry point
/// that fails before writing anything leaves the empty vector from
/// ``makeEmptyPpBuffer()``, which frees as a no-op.
@inlinable
public func withPpOutBuffer(
    _ call: (UnsafeMutablePointer<Vec_uint8_t>) -> Int32
) -> (status: RawStatus, bytes: Data) {
    var buffer = makeEmptyPpBuffer()
    let code = withUnsafeMutablePointer(to: &buffer) { call($0) }
    return (RawStatus(code: code), drainPpBuffer(buffer))
}
