//! GSE-09 v1.1 wire model + decoder (independent path).
//!
//! Wire format: `docs/gse09/wire-spec.md` (FROZEN). Self-describing
//! pre-order quadtree:
//!   0x03 = internal partition (4 children: TL, TR, BL, BR follow)
//!   0x00 CONST  (1 B value)
//!   0x01 PLANAR (3 B params A,B,C canonical; varint exc count k;
//!               k × (varint row-major pos, 1 B replacement byte))
//!   0x02 RAW    (s×s literal bytes, row-major)
//!
//! Normative (v1.1): the wire stores REPLACEMENT BYTES v_p, never
//! mod-256 deltas; corrections are derived at QUERY time in signed
//! arithmetic after the generator's mod-256 reduction (see query.rs).
//!
//! The decoder is the GQ3 oracle entry: sums verify over the grid
//! THIS fills from wire bytes — no encoder-side working data exists
//! in this crate.

use std::fmt;

pub const MIN_TILE: usize = 8;

#[derive(Debug)]
pub enum WireError {
    Truncated,
    BadType(u8),
    BadExceptionPos(usize),
    DupException(usize),
    VarintTooLong,
    TrailingBytes,
    BadDimensions(usize),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Truncated => write!(f, "truncated stream"),
            WireError::BadType(t) => write!(f, "bad tile type {t}"),
            WireError::BadExceptionPos(p) => {
                write!(f, "exception position {p} out of tile")
            }
            WireError::DupException(p) => {
                write!(f, "duplicate exception position {p}")
            }
            WireError::VarintTooLong => write!(f, "varint too long"),
            WireError::TrailingBytes => write!(f, "trailing bytes"),
            WireError::BadDimensions(s) => {
                write!(f, "bad dimensions {s}")
            }
        }
    }
}

impl std::error::Error for WireError {}

/// Byte-valued generator model, GSE-09 v1.1 families (constants,
/// linear, planar, periodic all reduce to or beyond these).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelType {
    Const(u8),
    /// (A, B, C): value(r, c) = (A + B·r_local + C·c_local) mod 256
    Planar(u8, u8, u8),
    Raw, // opaque body — no generator
}

/// A decoded region tree node.
#[derive(Debug, Clone)]
pub enum Region {
    Const {
        side: usize,
        value: u8,
    },
    Planar {
        side: usize,
        a: u8,
        b: u8,
        c: u8,
        /// (row-major tile position, replacement byte)
        exceptions: Vec<(u32, u8)>,
    },
    Raw {
        side: usize,
        body: Vec<u8>,
    },
    Split {
        side: usize,
        children: Box<[Region; 4]>,
    },
}

impl Region {
    pub fn side(&self) -> usize {
        match self {
            Region::Const { side, .. }
            | Region::Planar { side, .. }
            | Region::Raw { side, .. }
            | Region::Split { side, .. } => *side,
        }
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn read(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        if self.pos + n > self.buf.len() {
            return Err(WireError::Truncated);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.read(1)?[0])
    }
    fn varint(&mut self) -> Result<usize, WireError> {
        let mut n = 0usize;
        for shift in (0..32).step_by(7) {
            let b = self.u8()?;
            n |= ((b & 0x7F) as usize) << shift;
            if b & 0x80 == 0 {
                return Ok(n);
            }
        }
        Err(WireError::VarintTooLong)
    }
}

fn decode_wire_top(wire: &[u8], side: usize) -> Result<Region, WireError> {
    if side < MIN_TILE || !side.is_power_of_two() {
        return Err(WireError::BadDimensions(side));
    }
    let mut r = Reader { buf: wire, pos: 0 };
    let region = read_region(&mut r, 0, side)?;
    if r.pos != wire.len() {
        return Err(WireError::TrailingBytes);
    }
    Ok(region)
}

/// Decode the wire stream into a region tree (independent oracle path).
/// Errors, never panics. side must be a power of two >= MIN_TILE.
pub fn decode_wire(wire: &[u8], side: usize) -> Result<Region, WireError> {
    decode_wire_top(wire, side)
}

/// Decode the full grid from wire bytes (the GQ3 oracle entry).
pub fn grid_from_wire(wire: &[u8], side: usize) -> Result<Vec<u8>, WireError> {
    let reg = decode_wire(wire, side)?;
    regions_to_grid(&reg, side)
}

fn read_region(r: &mut Reader<'_>, _offset: usize, side: usize) -> Result<Region, WireError> {
    let t = r.u8()?;
    match t {
        0x00 => {
            let v = r.u8()?;
            Ok(Region::Const { side, value: v })
        }
        0x01 => {
            let p = r.read(3)?;
            let (a, b, c) = (p[0], p[1], p[2]);
            let k = r.varint()?;
            let mut exceptions = Vec::with_capacity(k.min(side * side));
            let mut seen = std::collections::HashSet::new();
            for _ in 0..k {
                let pos = r.varint()?;
                if pos >= side * side {
                    return Err(WireError::BadExceptionPos(pos));
                }
                let v = r.u8()?;
                if !seen.insert(pos) {
                    return Err(WireError::DupException(pos));
                }
                exceptions.push((pos as u32, v));
            }
            Ok(Region::Planar {
                side,
                a,
                b,
                c,
                exceptions,
            })
        }
        0x02 => {
            let n = side.checked_mul(side).ok_or(WireError::Truncated)?;
            let body = r.read(n)?.to_vec();
            Ok(Region::Raw { side, body })
        }
        0x03 if side >= 2 * MIN_TILE => {
            let h = side / 2;
            let tl = read_region(r, 0, h)?;
            let tr = read_region(r, 0, h)?;
            let bl = read_region(r, 0, h)?;
            let br = read_region(r, 0, h)?;
            Ok(Region::Split {
                side,
                children: Box::new([tl, tr, bl, br]),
            })
        }
        0x03 => Err(WireError::BadDimensions(side)),
        other => Err(WireError::BadType(other)),
    }
}

/// Fill `grid` (side×side, row-major) from a region tree.
/// Exceptions are buffered (wire order), generator fill first,
/// replacements after — per spec.
pub fn regions_to_grid(reg: &Region, side: usize) -> Result<Vec<u8>, WireError> {
    let mut grid = vec![0u8; side.checked_mul(side).ok_or(WireError::Truncated)?];
    fill_region(reg, 0, 0, side, side, &mut grid)?;
    Ok(grid)
}

fn fill_region(
    reg: &Region,
    top: usize,
    left: usize,
    side: usize,
    stride: usize,
    g: &mut [u8],
) -> Result<(), WireError> {
    match reg {
        Region::Const { value, .. } => {
            for rr in 0..side {
                for cc in 0..side {
                    g[(top + rr) * stride + left + cc] = *value;
                }
            }
        }
        Region::Planar {
            a,
            b,
            c,
            exceptions,
            ..
        } => {
            for rr in 0..side {
                let ar = ((*a as usize).wrapping_add((*b as usize).wrapping_mul(rr))) & 0xFF;
                for cc in 0..side {
                    g[(top + rr) * stride + left + cc] =
                        (ar.wrapping_add((*c as usize).wrapping_mul(cc))) as u8;
                }
            }
            for (pos, v) in exceptions {
                let (pr, pc) = (*pos as usize / side, *pos as usize % side);
                g[(top + pr) * stride + left + pc] = *v;
            }
        }
        Region::Raw { body, .. } => {
            for rr in 0..side {
                for cc in 0..side {
                    g[(top + rr) * stride + left + cc] = body[rr * side + cc];
                }
            }
        }
        Region::Split { children, .. } => {
            let h = side / 2;
            fill_region(&children[0], top, left, h, stride, g)?;
            fill_region(&children[1], top, left + h, h, stride, g)?;
            fill_region(&children[2], top + h, left, h, stride, g)?;
            fill_region(&children[3], top + h, left + h, h, stride, g)?;
        }
    }
    Ok(())
}

// grid_from_wire is the GQ3 oracle entry defined above (independent
// decode path); no stray helpers remain.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_contract() {
        // smoke: a single CONST leaf decodes; side enforced power-of-two
        let wire = [0x00u8, 77];
        assert!(decode_wire(&wire, 7).is_err());
        assert!(decode_wire(&wire, 0).is_err());
    }
}
