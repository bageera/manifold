//! GSE-09 v1.1 wire packer (Rust port of the committed reproduction
//! encoder, `docs/gse09/prototype_reproduction.py::pack_regions`).
//!
//! Wire contract (FROZEN): see docs/gse09/wire-spec.md — replacement
//! bytes on the wire; the v1.1 stream structure: 0x03 split (TL,TR,BL,BR
//! pre-order), 0x01 planar (3 params + varint k + k×(varint pos, u8 v)),
//! 0x00 const, 0x02 raw (s² literals row-major).
//!
//! The packer mirrors the Python encoder's POLICY exactly (greedy:
//! whole-tile planar when wire ≤ raw; const when uniform; raw at
//! min_tile; else split). Byte-cost decomposition is tracked for GQ4.

#[derive(Debug, Default, Clone)]
pub struct PackStats {
    pub planar_tiles: u32,
    pub raw_tiles: u32,
    pub const_tiles: u32,
    pub split_nodes: u32,
    /// type byte per node
    pub type_bytes: usize,
    /// planar params (3×) + const params (1×)
    pub param_bytes: usize,
    /// planar count varints + position varints + replacement bytes
    pub exception_bytes: usize,
    /// raw literal bytes
    pub literal_bytes: usize,
}

impl PackStats {
    /// Σ of the decomposition — must equal the wire length (asserted).
    pub fn accounted(&self) -> usize {
        self.type_bytes + self.param_bytes + self.exception_bytes + self.literal_bytes
    }
}

pub(crate) fn varint_len(n: usize) -> usize {
    let mut n = n;
    let mut l = 1;
    while n >= 0x80 {
        n >>= 7;
        l += 1;
    }
    l
}

fn push_varint(buf: &mut Vec<u8>, mut n: usize) {
    loop {
        let b = (n & 0x7F) as u8;
        n >>= 7;
        if n == 0 {
            buf.push(b);
            return;
        }
        buf.push(b | 0x80);
    }
}

/// Exact planar fit at the tile view g[top..top+s, left..left+s] with
/// row stride `stride`. Value(r, c) = (A + B·r + C·c) mod 256.
/// Errors, never panics; returns None when no exact fit.
fn fit_planar(g: &[u8], stride: usize, top: usize, left: usize, s: usize) -> Option<(u8, u8, u8)> {
    if s < 2 || top + s > stride_len(g, stride) || left + s > stride {
        return None;
    }
    let at = |r: usize, c: usize| g[(top + r) * stride + left + c];
    let c0 = at(0, 1).wrapping_sub(at(0, 0));
    for r in 0..s {
        if at(r, 1).wrapping_sub(at(r, 0)) != c0 {
            return None;
        }
    }
    let b = at(1, 0).wrapping_sub(at(0, 0));
    let a = at(0, 0);
    // full verify, masked (v1.1: compare AFTER mod-256 reduction)
    for r in 0..s {
        let ar = a.wrapping_add(b.wrapping_mul(r as u8));
        for c in 0..s {
            if ar.wrapping_add(c0.wrapping_mul(c as u8)) != at(r, c) {
                return None;
            }
        }
    }
    Some((a, b, c0))
}

fn stride_len(g: &[u8], stride: usize) -> usize {
    g.len() / stride.max(1)
}

/// Exception-tolerant planar candidate fit (GSE-10 sparse arm).
///
/// The committed reproduction's `fit_planar` demands FULL-tile exactness,
/// so exceptions never survive into planar leaves there (its sparse_edits
/// fixture went raw per-tile — the measured +86% loss row). The v1.1 wire
/// already supports planar-with-exceptions; this fit recovers the
/// original GSE-09 report's arm: derive (A, B, C) from the MAJORITY
/// column step and one clean column pair, tolerating scattered edits,
/// then the mismatch set = exceptions.
///
/// Returns (A, B, C, exceptions) when the tile is planar-plus-sparse,
/// None when no consistent candidate emerges. Never panics.
/// (A, B, C) plus the row-major exception set from the sparse fit.
pub type PlanarSparse = (u8, u8, u8, Vec<(u32, u8)>);

#[allow(clippy::type_complexity)]
fn fit_planar_sparse(
    g: &[u8],
    stride: usize,
    top: usize,
    left: usize,
    s: usize,
) -> Option<PlanarSparse> {
    if s < 2 || top + s > stride_len(g, stride) || left + s > stride {
        return None;
    }
    let at = |r: usize, c: usize| g[(top + r) * stride + left + c];
    // C candidate = majority of (v(r,1) - v(r,0)) over rows
    let mut counts = [0u32; 256];
    for r in 0..s {
        counts[at(r, 1).wrapping_sub(at(r, 0)) as usize] += 1;
    }
    let mut best = 0u8;
    let mut best_n = 0u32;
    for (v, n) in counts.iter().enumerate() {
        if *n > best_n {
            best_n = *n;
            best = v as u8;
        }
    }
    // require an actual majority beyond trivial noise (≥ half + 1)
    if best_n < s.div_ceil(2) as u32 {
        return None;
    }
    let c = best;
    let consistent_cols: Vec<usize> = (0..s)
        .filter(|&r| at(r, 1).wrapping_sub(at(r, 0)) == c)
        .collect();
    // B/A candidate from the first consistent column, requiring a pair
    // 2 rows apart in the SAME column: (v(r2,c) - v(r1,c)) / (r2-r1) = B·(r2-r1)
    // need exact per-row delta from TWO consistent rows in one column:
    if consistent_cols.len() < 2 {
        return None;
    }
    let col = consistent_cols[0];
    // find two rows in this column consistent with a single step `b`:
    // for candidate b, majority across rows with (v(r+1,col)-v(r,col))
    let mut bcounts = [0u32; 256];
    for w in 0..s.saturating_sub(1) {
        let d = at(w + 1, col).wrapping_sub(at(w, col));
        bcounts[d as usize] += 1;
    }
    let mut bb = 0u8;
    let mut bn = 0u32;
    for (v, n) in bcounts.iter().enumerate() {
        if *n > bn {
            bn = *n;
            bb = v as u8;
        }
    }
    if bn < (s as u32) / 2 {
        return None;
    }
    let b = bb;
    // anchor: cleanest column (fewest mismatches vs (A + B·r) trend);
    // A = the column's value at its clean rows' start — derive from the
    // top-most consistent row of the FIRST consistent column:
    let row0 = consistent_cols[0];
    // re-base the anchor to the LOCAL origin (0,0): the plane through
    // at(row0,col) is A + B·(r-row0) + C·(c-col); local origin A becomes
    // at(row0,col) - B*row0 - C*col (mod 256).
    let a = at(row0, col)
        .wrapping_sub(b.wrapping_mul(row0 as u8))
        .wrapping_sub(c.wrapping_mul(col as u8));
    // Full verify: exceptions = the mismatch set; require exceptions
    // sparse enough that a planar WIRE beats raw (checked by caller).
    let exceptions = exceptions_of(g, stride, top, left, s, a, b, c);
    Some((a, b, c, exceptions))
}

#[allow(clippy::too_many_arguments)]
fn exceptions_of(
    g: &[u8],
    stride: usize,
    top: usize,
    left: usize,
    s: usize,
    a: u8,
    b: u8,
    c: u8,
) -> Vec<(u32, u8)> {
    let mut out = Vec::new();
    for r in 0..s {
        let ar = a.wrapping_add(b.wrapping_mul(r as u8));
        for cc in 0..s {
            let v = g[(top + r) * stride + left + cc];
            if ar.wrapping_add(c.wrapping_mul(cc as u8)) != v {
                out.push(((r * s + cc) as u32, v));
            }
        }
    }
    out
}

/// Greedy quadtree pack — byte-policy identical to the committed
/// Python reproduction (see module doc).
pub fn pack_quadtree(g: &[u8], side: usize, min_tile: usize) -> (Vec<u8>, PackStats) {
    let mut st = PackStats::default();
    let mut wire = Vec::with_capacity(g.len() / 2 + 64);
    emit(g, side, 0, 0, side, min_tile, &mut wire, &mut st);
    (wire, st)
}

#[allow(clippy::too_many_arguments)]
fn emit(
    g: &[u8],
    stride: usize,
    top: usize,
    left: usize,
    s: usize,
    min_tile: usize,
    wire: &mut Vec<u8>,
    st: &mut PackStats,
) {
    st.type_bytes += 1;
    let raw_wire = 1 + s * s;
    // Two planar fits compete with raw: exact (no exceptions) and the
    // exception-tolerant sparse arm (the GSE-09 report's economics; the
    // committed reproduction's exact-only fit never fires with edits —
    // measured +86% loss row). Wire stays frozen (v1.1 supports both).
    let candidate = if let Some((a, b, c)) = fit_planar(g, stride, top, left, s) {
        Some((a, b, c, exceptions_of(g, stride, top, left, s, a, b, c)))
    } else if let Some((a, b, c, exc)) = fit_planar_sparse(g, stride, top, left, s) {
        Some((a, b, c, exc))
    } else {
        None
    };
    if let Some((a, b, c, exceptions)) = candidate {
        let planar_wire = 4
            + 3
            + varint_len(exceptions.len())
            + exceptions
                .iter()
                .map(|&(p, _)| varint_len(p as usize) + 1)
                .sum::<usize>();
        if planar_wire <= raw_wire {
            wire.push(0x01);
            wire.extend_from_slice(&[a, b, c]);
            st.param_bytes += 3;
            push_varint(wire, exceptions.len());
            st.exception_bytes += varint_len(exceptions.len());
            for (pos, v) in exceptions {
                st.exception_bytes += varint_len(pos as usize) + 1;
                push_varint(wire, pos as usize);
                wire.push(v);
            }
            st.planar_tiles += 1;
            return;
        }
        // planar fits but costs more than raw → fall through (policy)
    }
    let first = g[top * stride + left];
    let mut uniform = true;
    'outer: for r in top..top + s {
        for c in left..left + s {
            if g[r * stride + c] != first {
                uniform = false;
                break 'outer;
            }
        }
    }
    if uniform && 2 < 1 + s * s {
        wire.push(0x00);
        wire.push(first);
        st.param_bytes += 1;
        st.const_tiles += 1;
        return;
    }
    if s <= min_tile {
        wire.push(0x02);
        for r in top..top + s {
            wire.extend_from_slice(&g[r * stride + left..r * stride + left + s]);
        }
        st.literal_bytes += s * s;
        st.raw_tiles += 1;
        return;
    }
    wire.push(0x03);
    st.split_nodes += 1;
    let h = s / 2;
    emit(g, stride, top, left, h, min_tile, wire, st);
    emit(g, stride, top, left + h, h, min_tile, wire, st);
    emit(g, stride, top + h, left, h, min_tile, wire, st);
    emit(g, stride, top + h, left + h, h, min_tile, wire, st);
}

/// Whole-frame single planar leaf arm (portfolio competitor).
pub fn pack_single_planar(g: &[u8], side: usize) -> Option<(Vec<u8>, PackStats)> {
    let (a, b, c) = fit_planar(g, side, 0, 0, side)?;
    let exceptions = exceptions_of(g, side, 0, 0, side, a, b, c);
    let mut wire = Vec::new();
    let mut st = PackStats {
        type_bytes: 1,
        ..Default::default()
    };
    wire.push(0x01);
    wire.extend_from_slice(&[a, b, c]);
    st.param_bytes += 3;
    push_varint(&mut wire, exceptions.len());
    st.exception_bytes += varint_len(exceptions.len());
    for (pos, v) in exceptions {
        st.exception_bytes += varint_len(pos as usize) + 1;
        push_varint(&mut wire, pos as usize);
        wire.push(v);
    }
    st.planar_tiles = 1;
    Some((wire, st))
}

/// Whole-frame raw leaf arm (portfolio competitor, never-worse form).
pub fn pack_single_raw(g: &[u8], _side: usize) -> (Vec<u8>, PackStats) {
    let st = PackStats {
        type_bytes: 1,
        literal_bytes: g.len(),
        ..Default::default()
    };
    let mut wire = vec![0x02u8];
    wire.extend_from_slice(g);
    (wire, st)
}

/// Whole-frame const leaf arm (portfolio competitor).
pub fn pack_single_const(g: &[u8], _side: usize) -> Option<(Vec<u8>, PackStats)> {
    let v = g[0];
    if !g.iter().all(|&x| x == v) {
        return None;
    }
    let st = PackStats {
        type_bytes: 1,
        param_bytes: 1,
        const_tiles: 1,
        ..Default::default()
    };
    Some((vec![0x00u8, v], st))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{make, Kind};
    use crate::model::{decode_wire, grid_from_wire, regions_to_grid};

    fn roundtrip(kind: Kind, side: usize, seed: u64) {
        let g = make(kind, side, seed);
        let (wire, st) = pack_quadtree(&g, side, 8);
        assert_eq!(wire.len(), st.accounted(), "accounting drift {kind:?}");
        let reg =
            decode_wire(&wire, side).unwrap_or_else(|e| panic!("decode failed {kind:?}: {e}"));
        let back = regions_to_grid(&reg, side).unwrap();
        assert_eq!(back, g, "roundtrip {kind:?} side {side}");
    }

    #[test]
    fn quadtree_roundtrip_all_kinds() {
        for k in [
            Kind::Constant,
            Kind::Mathematical,
            Kind::SparseEdits,
            Kind::Mixed,
            Kind::SmoothNoisy,
            Kind::Random,
            Kind::DenseHotspot(40),
        ] {
            roundtrip(k, 64, 11);
        }
    }

    #[test]
    fn quadtree_mixed_128_wire_shape() {
        let g = make(Kind::Mixed, 128, 9);
        let (w, st) = pack_quadtree(&g, 128, 8);
        assert_eq!(st.planar_tiles, 128);
        assert_eq!(st.raw_tiles, 128);
        assert!(w.len() < 10_000, "mixed wire too fat: {}", w.len());
        let reg = decode_wire(&w, 128).unwrap();
        assert_eq!(regions_to_grid(&reg, 128).unwrap(), g);
    }

    #[test]
    fn single_arms_roundtrip() {
        let g = make(Kind::Mathematical, 64, 4);
        let (w, _) = pack_single_planar(&g, 64).expect("exact planar frame fits");
        let reg = decode_wire(&w, 64).unwrap();
        assert_eq!(regions_to_grid(&reg, 64).unwrap(), g);
        let g = make(Kind::Random, 64, 4);
        let (w, _) = pack_single_raw(&g, 64);
        let reg = decode_wire(&w, 64).unwrap();
        assert_eq!(regions_to_grid(&reg, 64).unwrap(), g);
        let g = make(Kind::Constant, 64, 4);
        let (w, _) = pack_single_const(&g, 64).unwrap();
        let reg = decode_wire(&w, 64).unwrap();
        assert_eq!(regions_to_grid(&reg, 64).unwrap(), g);
    }

    #[test]
    fn grid_from_wire_helper() {
        // keep the grid-path honest too (decoded bytes as GQ3 source)
        let g = make(Kind::Mixed, 32, 6);
        let (w, _) = pack_quadtree(&g, 32, 8);
        assert_eq!(grid_from_wire(&w, 32).unwrap(), g);
    }
}
