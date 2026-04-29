/*
 * Pointer-based glue around panproto-c.
 *
 * The base panproto-c API uses safer-ffi's `c_slice::Ref<u8>` and
 * `repr_c::Vec<u8>` types, which are passed by value in the C ABI.
 * GHC's `foreign import capi` cannot reliably pass structs by value
 * across all platforms, so this glue exposes pointer-based wrappers
 * with the same semantics that Haskell's FFI consumes naturally.
 *
 * No allocations happen here: the slices are constructed on the
 * stack and the by-value structs are unpacked from caller-provided
 * pointers.
 */

#include "panproto_glue.h"
#include <string.h>

/* ---------- protocol ---------- */

int32_t pp_protocol_define_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t spec = { .ptr = spec_ptr, .len = spec_len };
    return pp_protocol_define(spec, out_handle);
}

int32_t pp_schema_from_cbor_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
) {
    slice_ref_uint8_t spec = { .ptr = spec_ptr, .len = spec_len };
    return pp_schema_from_cbor(spec, out_handle);
}

/* ---------- buffer release ---------- */

/*
 * Move the contents of *buf into pp_buf_free, then zero the storage
 * so a future Haskell-side double-free can't pass a stale pointer.
 */
void pp_buf_free_at(Vec_uint8_t *buf) {
    Vec_uint8_t taken = *buf;
    memset(buf, 0, sizeof *buf);
    pp_buf_free(taken);
}
