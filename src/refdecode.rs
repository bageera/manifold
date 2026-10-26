// Independent reference decoder for MNF5 (README §6: at least one decoder
// implementation driven by the format specification rather than reused
// encoder helpers).
//
// Deliberately different from codec::decode at every decision point:
// - validates in spec order Section 1 (magic -> dims -> tile_size ->
//   tile_count -> payload_len) with its own arithmetic;
// - parses records with manual bounds checks (no shared cursor pattern);
// - implements D8 as push maps (source pixel scattered to its output
//   position) instead of codec's pull maps;
// - reconstructs references by building the transformed base then adding
//   residuals via iterator zip.

use crate::codec::Dims;
use std::io::Read;

const HEADER: usize = 24;
const TS: u32 = 16;
const MAX_PIXELS: u64 = 1 << 26;
const MAX_PAYLOAD: u32 = (1 << 31) - 1;

pub struct RefDecodeResult {
    pub dims: Dims,
    pub pixels: Vec<u8>,
}

// Spec-quote helper (docs/format.md uses `ceil`); std div_ceil is the same math.
fn div_ceil(a: u32, b: u32) -> u32 {
    a.div_ceil(b)
}

/// D8 push map: where does transform t place source pixel (sx, sy)?
/// Returns (out_w, out_h, out_x, out_y). Inverse viewpoint of codec's
/// pull maps; same labeling, pinned by the differential tests.
fn push_target(t: u8, w: u32, h: u32, sx: u32, sy: u32) -> (u32, u32, u32, u32) {
    match t {
        0 => (w, h, sx, sy),
        1 => (w, h, w - 1 - sx, sy),
        2 => (w, h, sx, h - 1 - sy),
        3 => (w, h, w - 1 - sx, h - 1 - sy),
        4 => (h, w, h - 1 - sy, sx),
        5 => {
            let (ow, oh, ox, oy) = push_target(4, w, h, sx, sy);
            push_target(1, ow, oh, ox, oy)
        }
        6 => {
            let (ow, oh, ox, oy) = push_target(4, w, h, sx, sy);
            push_target(3, ow, oh, ox, oy)
        }
        7 => {
            let (ow, oh, ox, oy) = push_target(4, w, h, sx, sy);
            let (w2, h2, ox2, oy2) = push_target(3, ow, oh, ox, oy);
            push_target(1, w2, h2, ox2, oy2)
        }
        _ => unreachable!("range validated by caller"),
    }
}

fn push_transform(t: u8, w: u32, h: u32, data: &[u8]) -> Vec<u8> {
    let (ow, oh) = if t >= 4 { (h, w) } else { (w, h) };
    let mut out = vec![0u8; (ow * oh * 3) as usize];
    let mut s = 0usize;
    for sy in 0..h {
        for sx in 0..w {
            let (_ow, _oh, ox, oy) = push_target(t, w, h, sx, sy);
            let ow_ = if t >= 4 { h } else { w };
            let d = ((oy * ow_ + ox) * 3) as usize;
            out[d] = data[s];
            out[d + 1] = data[s + 1];
            out[d + 2] = data[s + 2];
            s += 3;
        }
    }
    out
}

pub fn ref_decode(buf: &[u8]) -> Result<RefDecodeResult, String> {
    if buf.len() < HEADER {
        return Err("short".into());
    }
    if buf[0..4] != *b"MNF5" {
        return Err("magic".into());
    }
    let u32le = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let width = u32le(4);
    let height = u32le(8);
    if width == 0 || height == 0 {
        return Err("dims".into());
    }
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err("pixels".into());
    }
    let ts = u32le(12);
    if ts != TS {
        return Err("tilesize".into());
    }
    let cols = div_ceil(width, TS);
    let rows = div_ceil(height, TS);
    let tile_count = u32le(16);
    if tile_count != cols * rows {
        return Err("tilecount".into());
    }
    let payload_len = u32le(20);
    if payload_len > MAX_PAYLOAD {
        return Err("payloadlen".into());
    }
    if buf.len() != HEADER + payload_len as usize {
        return Err("filelen".into());
    }
    let zlib = &buf[HEADER..];

    let limit = tile_count as u64 * (1 + 5 + (TS * TS * 3) as u64);
    let mut raw: Vec<u8> = Vec::new();
    {
        let mut d = flate2::read::ZlibDecoder::new(zlib).take(limit + 1);
        d.read_to_end(&mut raw).map_err(|_| "zlib".to_string())?;
    }
    if raw.len() as u64 > limit {
        return Err("bomb".into());
    }

    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    let mut decoded: Vec<(u32, u32, Vec<u8>)> = Vec::with_capacity(tile_count as usize);
    let mut ip = 0usize;
    for i in 0..tile_count {
        let row = i / cols;
        let col = i % cols;
        let tw = TS.min(width - col * TS);
        let th = TS.min(height - row * TS);
        let n = (tw * th * 3) as usize;

        if ip >= raw.len() {
            return Err("eof".into());
        }
        let mode = raw[ip];
        ip += 1;

        let refs: Option<(usize, u8)> = match mode {
            0 => None,
            1 => {
                if ip + 4 > raw.len() {
                    return Err("refmeta".into());
                }
                let idx = u32::from_le_bytes(raw[ip..ip + 4].try_into().unwrap()) as usize;
                ip += 4;
                Some((idx, 0))
            }
            2 => {
                if ip + 5 > raw.len() {
                    return Err("refmeta".into());
                }
                let idx = u32::from_le_bytes(raw[ip..ip + 4].try_into().unwrap()) as usize;
                let tr = raw[ip + 4];
                ip += 5;
                Some((idx, tr))
            }
            _ => return Err("mode".into()),
        };

        if ip + n > raw.len() {
            return Err("body".into());
        }
        let body = raw[ip..ip + n].to_vec();
        ip += n;

        let tile = match refs {
            None => body,
            Some((src, tr)) => {
                if src >= i as usize {
                    return Err("reforder".into());
                }
                if tr >= 8 {
                    return Err("tranform-range".into());
                }
                let (rw, rh, ref_rgb) = &decoded[src];
                if *rw != tw || *rh != th {
                    return Err("refshape".into());
                }
                if tw != th && tr >= 4 {
                    return Err("xform-shape".into());
                }
                let base = if tr == 0 {
                    ref_rgb.clone()
                } else {
                    push_transform(tr, tw, th, ref_rgb)
                };
                base.iter()
                    .zip(body.iter())
                    .map(|(a, r)| a.wrapping_add(*r))
                    .collect()
            }
        };
        decoded.push((tw, th, tile.clone()));

        for y in 0..th as usize {
            let py = row as usize * TS as usize + y;
            for x in 0..tw as usize {
                let pxx = col as usize * TS as usize + x;
                let d = (py * width as usize + pxx) * 3;
                let s = (y * tw as usize + x) * 3;
                pixels[d] = tile[s];
                pixels[d + 1] = tile[s + 1];
                pixels[d + 2] = tile[s + 2];
            }
        }
    }
    if ip != raw.len() {
        return Err("extra".into());
    }
    Ok(RefDecodeResult {
        dims: Dims { width, height },
        pixels,
    })
}
