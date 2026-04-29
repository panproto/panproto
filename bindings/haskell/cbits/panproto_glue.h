/*
 * Pointer-based glue around the panproto-c by-value FFI.
 *
 * GHC's `foreign import capi` cannot reliably pass structs by value
 * across all platforms. The functions declared here are tiny
 * forwarding shims (defined in `panproto_glue.c`) that accept
 * pointers and forward to the by-value Rust API. Haskell imports
 * these instead of `pp_buf_free` and `pp_protocol_define` directly.
 */

#ifndef PANPROTO_GLUE_H
#define PANPROTO_GLUE_H

#include "panproto.h"
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int32_t pp_protocol_define_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

int32_t pp_schema_from_cbor_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

void pp_buf_free_at(Vec_uint8_t *buf);

#ifdef __cplusplus
}
#endif

#endif /* PANPROTO_GLUE_H */
