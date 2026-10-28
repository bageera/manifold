//! C FFI for Manifold codecs (MNF5/6/7/8).
//!
//! Contract: no panics across the boundary — every fallible call returns a
//! status code; outputs are written through caller-owned buffers.
//! All buffers' lengths are caller-supplied; the codecs never allocate on
//! behalf of the caller.

#![allow(clippy::similar_names)] // API names mirror the format names

// Status codes (stable ABI: values fixed by C header contract)
pub const STATUS_OK: i32 = 0;
pub const STATUS_NULL_ARG: i32 = -1;
pub const STATUS_BUF_TOO_SMALL: i32 = -2;
pub const STATUS_FORMAT_INVALID: i32 = -3;
pub const STATUS_LIMITS: i32 = -4;
pub const STATUS_UNSUPPORTED: i32 = -5;
pub const STATUS_IMAGE_IO: i32 = -6;
pub const STATUS_INTERNAL: i32 = -7;

/// Map a library Error onto a stable status code.
fn status(e: &manifold::error::Error) -> i32 {
    use manifold::error::Error as E;
    match e {
        E::Format { .. } | E::Decompress(_) => STATUS_FORMAT_INVALID,
        E::Limits { .. } => STATUS_LIMITS,
        E::Unsupported { .. } => STATUS_UNSUPPORTED,
        E::Io(_) | E::ImageIo(_) => STATUS_IMAGE_IO,
        E::Internal(_) => STATUS_INTERNAL,
    }
}

// ---- decode ---- ------------------------------------------------------------

/// Decode any MNF container (auto-detects MNF5/6/7/8 by magic).
/// Inputs: `buf`/`len` = container. Outputs: caller buffer `out` sized
/// `*out_len` (pass `out_len` with a zero-length buffer to query; the
/// required size is written back). Returns STATUS_OK on success.
#[no_mangle]
pub unsafe extern "C" fn manifold_decode(
    buf: *const u8,
    len: usize,
    out: *mut u8,
    out_len: *mut usize,
    width: *mut u32,
    height: *mut u32,
) -> i32 {
    if buf.is_null() || out_len.is_null() || width.is_null() || height.is_null() {
        return STATUS_NULL_ARG;
    }
    let data = std::slice::from_raw_parts(buf, len);
    let decoded = decode_any(data);
    let (w, h, pixels) = match decoded {
        Ok(t) => t,
        Err(e) => return status(&e),
    };
    let need = pixels.len();
    if out.is_null() || *out_len < need {
        *out_len = need;
        *width = w;
        *height = h;
        return STATUS_BUF_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), out, need);
    *out_len = need;
    *width = w;
    *height = h;
    STATUS_OK
}

unsafe fn decode_any(buf: &[u8]) -> manifold::error::Result<(u32, u32, Vec<u8>)> {
    if buf.len() >= 4 {
        match &buf[0..4] {
            b"MNF5" => {
                let r = manifold::codec::decode(buf)?;
                return Ok((r.dims.width, r.dims.height, r.pixels));
            }
            b"MNF6" | b"MNF7" => {
                let (d, px) = manifold::mnf6::decode_mnf7(buf)?;
                return Ok((d.width, d.height, px));
            }
            b"MNF8" => {
                let (d, px, _) = manifold::mnf8::decode_mnf8(buf)?;
                return Ok((d.width, d.height, px));
            }
            _ => return Err(manifold::error::Error::msg("bad magic (want MNF5/6/7/8)")),
        }
    }
    Err(manifold::error::Error::msg("bad magic (want MNF5/6/7/8)"))
}

// ---- encode ---- ------------------------------------------------------------

/// Encode MNF6/7 (zstd container) from raw RGB8. Encoded bytes are copied
/// into caller buffer; query by passing out=null (required size written).
#[no_mangle]
pub unsafe extern "C" fn manifold_encode(
    pixels: *const u8,
    width: u32,
    height: u32,
    out: *mut u8,
    out_len: *mut usize,
) -> i32 {
    if pixels.is_null() || out_len.is_null() {
        return STATUS_NULL_ARG;
    }
    let px_len = (width as usize) * (height as usize) * 3;
    let px = std::slice::from_raw_parts(pixels, px_len);
    let d = manifold::codec::Dims { width, height };
    let encoded = manifold::mnf6::encode_mnf7(px, d, false).0;
    copy_out(encoded, out, out_len)
}

/// Encode MNF5 (v0.5 format, zlib container) from raw RGB8.
/// use_geom: 0 = t=0 only, nonzero = D8 geometry search. Same out/out_len
/// convention as the other encoders.
#[no_mangle]
pub unsafe extern "C" fn manifold_encode_mnf5(
    pixels: *const u8,
    width: u32,
    height: u32,
    use_geom: u32,
    out: *mut u8,
    out_len: *mut usize,
) -> i32 {
    if pixels.is_null() || out_len.is_null() {
        return STATUS_NULL_ARG;
    }
    let px_len = (width as usize) * (height as usize) * 3;
    let px = std::slice::from_raw_parts(pixels, px_len);
    let d = manifold::codec::Dims { width, height };
    let encoded = match manifold::codec::encode_mnf5(px, d, use_geom == 0, use_geom != 0) {
        Ok(e) => e,
        Err(e) => return status(&e),
    };
    copy_out(encoded, out, out_len)
}

/// Encode MNF8 (backend auto-select; profile: 0=balanced, 1=maximum).
#[no_mangle]
pub unsafe extern "C" fn manifold_encode_mnf8(
    pixels: *const u8,
    width: u32,
    height: u32,
    profile: u32,
    out: *mut u8,
    out_len: *mut usize,
) -> i32 {
    if pixels.is_null() || out_len.is_null() {
        return STATUS_NULL_ARG;
    }
    let px_len = (width as usize) * (height as usize) * 3;
    let px = std::slice::from_raw_parts(pixels, px_len);
    let d = manifold::codec::Dims { width, height };
    let prof = if profile != 0 {
        manifold::mnf8::Profile::Maximum
    } else {
        manifold::mnf8::Profile::Balanced
    };
    let encoded = manifold::mnf8::encode_mnf8(px, d, false, prof).0;
    copy_out(encoded, out, out_len)
}

unsafe fn copy_out(bytes: Vec<u8>, out: *mut u8, out_len: *mut usize) -> i32 {
    let need = bytes.len();
    if out.is_null() || *out_len < need {
        *out_len = need;
        return STATUS_BUF_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, need);
    *out_len = need;
    STATUS_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_mnf7_via_ffi_shapes() {
        // encode → copy-out → decode → copy-out, exactness — exercises every
        // status branch including the query pass.
        let (w, h) = (32u32, 32u32);
        let px: Vec<u8> = (0..w as usize * h as usize * 3)
            .map(|i| (i % 251) as u8)
            .collect();
        unsafe {
            let px_ptr = px.as_ptr();
            let mut n = 0usize;
            let mut st = manifold_encode(px_ptr, w, h, std::ptr::null_mut(), &mut n);
            assert_eq!(st, STATUS_BUF_TOO_SMALL, "query pass");
            let mut enc = vec![0u8; n];
            st = manifold_encode(px_ptr, w, h, enc.as_mut_ptr(), &mut n);
            assert_eq!(st, STATUS_OK);
            let mut wn = 0usize;
            let (mut ow, mut oh) = (0u32, 0u32);
            let mut st = manifold_decode(
                enc.as_ptr(),
                n,
                std::ptr::null_mut(),
                &mut wn,
                &mut ow,
                &mut oh,
            );
            assert_eq!(st, STATUS_BUF_TOO_SMALL);
            assert_eq!((ow, oh), (w, h));
            let mut out = vec![0u8; wn];
            st = manifold_decode(enc.as_ptr(), n, out.as_mut_ptr(), &mut wn, &mut ow, &mut oh);
            assert_eq!(st, STATUS_OK);
            assert_eq!(out[..wn], px[..]);
            // nulls
            assert_eq!(
                manifold_encode(std::ptr::null(), 1, 1, std::ptr::null_mut(), &mut n),
                STATUS_NULL_ARG
            );
        }
    }

    #[test]
    fn roundtrip_mnf8_and_magic() {
        let (w, h) = (16u32, 16u32);
        let px: Vec<u8> = (0..w as usize * h as usize * 3)
            .map(|i| (i * 7 % 256) as u8)
            .collect();
        unsafe {
            let mut n = 0usize;
            let mut st = manifold_encode_mnf8(px.as_ptr(), w, h, 0, std::ptr::null_mut(), &mut n);
            assert_eq!(st, STATUS_BUF_TOO_SMALL);
            let mut enc = vec![0u8; n];
            let mut st = manifold_encode_mnf8(px.as_ptr(), w, h, 0, enc.as_mut_ptr(), &mut n);
            assert_eq!(st, STATUS_OK);
            let mut wn = 0usize;
            let (mut ow, mut oh) = (0u32, 0u32);
            let mut st = manifold_decode(
                enc.as_ptr(),
                n,
                std::ptr::null_mut(),
                &mut wn,
                &mut ow,
                &mut oh,
            );
            assert_eq!(st, STATUS_BUF_TOO_SMALL);
            let mut out = vec![0u8; wn];
            let mut st =
                manifold_decode(enc.as_ptr(), n, out.as_mut_ptr(), &mut wn, &mut ow, &mut oh);
            assert_eq!(st, STATUS_OK);
            assert_eq!(out[..wn], px[..]);
            // invalid container → FORMAT_INVALID, not a panic
            let junk = vec![0u8; 40];
            let st = manifold_decode(
                junk.as_ptr(),
                junk.len(),
                std::ptr::null_mut(),
                &mut wn,
                &mut ow,
                &mut oh,
            );
            assert_eq!(st, STATUS_FORMAT_INVALID);
        }
    }
    #[test]
    fn roundtrip_mnf8_token_file_via_ffi() {
        // ADR-003: an MNF8 file carrying the token payload decodes through
        // manifold_decode (magic routing, no new entry point). The encoder
        // picks tokens on this ramp body; the decode must be exact.
        let (w, h) = (64u32, 64u32);
        let mut px: Vec<u8> = vec![0; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                px[i] = (y * 3) as u8;
                px[i + 1] = (y / 4) as u8;
                px[i + 2] = 200;
            }
        }
        unsafe {
            let mut n = 0usize;
            let st = manifold_encode_mnf8(px.as_ptr(), w, h, 0, std::ptr::null_mut(), &mut n);
            assert_eq!(st, STATUS_BUF_TOO_SMALL);
            let mut enc = vec![0u8; n];
            let st = manifold_encode_mnf8(px.as_ptr(), w, h, 0, enc.as_mut_ptr(), &mut n);
            assert_eq!(st, STATUS_OK);
            let mut wn = 0usize;
            let (mut ow, mut oh) = (0u32, 0u32);
            let st = manifold_decode(
                enc.as_ptr(),
                n,
                std::ptr::null_mut(),
                &mut wn,
                &mut ow,
                &mut oh,
            );
            assert_eq!(st, STATUS_BUF_TOO_SMALL);
            assert_eq!(wn, px.len());
            let mut out = vec![0u8; wn];
            let st = manifold_decode(enc.as_ptr(), n, out.as_mut_ptr(), &mut wn, &mut ow, &mut oh);
            assert_eq!(st, STATUS_OK);
            assert_eq!(out[..wn], px[..], "token-file roundtrip through the C ABI");
            assert_eq!((ow, oh), (w, h));
        }
    }
    #[test]
    fn roundtrip_mnf5_via_ffi() {
        let (w, h) = (32u32, 32u32);
        let px: Vec<u8> = (0..w as usize * h as usize * 3)
            .map(|i| (i % 251) as u8)
            .collect();
        unsafe {
            let mut n = 0usize;
            let st = manifold_encode_mnf5(px.as_ptr(), w, h, 1, std::ptr::null_mut(), &mut n);
            assert_eq!(st, STATUS_BUF_TOO_SMALL, "query pass");
            let mut enc = vec![0u8; n];
            let st = manifold_encode_mnf5(px.as_ptr(), w, h, 1, enc.as_mut_ptr(), &mut n);
            assert_eq!(st, STATUS_OK);
            let mut wn = 0usize;
            let (mut ow, mut oh) = (0u32, 0u32);
            let st = manifold_decode(
                enc.as_ptr(),
                n,
                std::ptr::null_mut(),
                &mut wn,
                &mut ow,
                &mut oh,
            );
            assert_eq!(st, STATUS_BUF_TOO_SMALL);
            let mut out = vec![0u8; wn];
            let st = manifold_decode(enc.as_ptr(), n, out.as_mut_ptr(), &mut wn, &mut ow, &mut oh);
            assert_eq!(st, STATUS_OK);
            assert_eq!(out[..wn], px[..]);
        }
    }
}
