/*
 * manifold.h — C ABI for the Manifold lossless RGB8 codecs (MNF5/6/7/8).
 *
 * Contract: never panics on caller data; all fallibility via status codes.
 * Buffers are caller-owned; lengths are caller-supplied. Query pattern:
 * call once with out=NULL to learn the required buffer size.
 *
 * MNF8 files may carry the FLAG_TOKENS payload (ADR-003): manifold_decode
 * handles both record and token payloads transparently by magic; the MNF8
 * encoders measure the token arm and emit it only when it is smaller.
 */
#ifndef MANIFOLD_H
#define MANIFOLD_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* status codes */
#define MANIFOLD_STATUS_OK             0
#define MANIFOLD_STATUS_NULL_ARG      -1
#define MANIFOLD_STATUS_BUF_TOO_SMALL -2
#define MANIFOLD_STATUS_FORMAT_INVALID -3
#define MANIFOLD_STATUS_LIMITS        -4
#define MANIFOLD_STATUS_UNSUPPORTED   -5
#define MANIFOLD_STATUS_IMAGE_IO      -6
#define MANIFOLD_STATUS_INTERNAL      -7

/*
 * Decode any MNF container (MNF5/6/7/8 auto-detected by magic).
 * pixels_len: buffer size on input; required size written when
 * MANIFOLD_STATUS_BUF_TOO_SMALL. width/height always written on
 * OK/BUFFER_TOO_SMALL. Returns a MANIFOLD_STATUS_* code.
 */
int32_t manifold_decode(const uint8_t *buf, size_t len,
                        uint8_t *pixels, size_t *pixels_len,
                        uint32_t *width, uint32_t *height);

/*
 * Encode MNF5 (v0.5 format, zlib container) from raw RGB8.
 * use_geom: 0 = identity transform only, nonzero = D8 search.
 */
int32_t manifold_encode_mnf5(const uint8_t *pixels, uint32_t width, uint32_t height,
                             uint32_t use_geom, uint8_t *out, size_t *out_len);

/*
 * Encode MNF7 (zstd container) from raw RGB8 (w*elen*3 bytes).
 * out_len: buffer size on input; required size when TOO_SMALL.
 */
int32_t manifold_encode(const uint8_t *pixels, uint32_t width, uint32_t height,
                        uint8_t *out, size_t *out_len);

/*
 * Encode MNF8. profile: 0 = balanced (zstd-19), 1 = maximum
 * (all backends measured). Same out/out_len convention.
 */
int32_t manifold_encode_mnf8(const uint8_t *pixels, uint32_t width, uint32_t height,
                             uint32_t profile, uint8_t *out, size_t *out_len);

#ifdef __cplusplus
}
#endif
#endif /* MANIFOLD_H */
