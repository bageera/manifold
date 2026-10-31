//! Region-sum query evaluation over the GSE-09 v1.1 wire format.
//!
//! Contract (v1.1 normative):
//!   S(Q) = S_M(Q) + Σ_{p ∈ E∩Q} (v_p − M_θ(p)), corrections in SIGNED
//!   arithmetic after the generator's mod-256 reduction.
//!   Cost target: O(rows + exceptions∩Q), never O(pixels).

use crate::floor_sum::window_sum_linear;
use crate::model::Region;

/// Sum over the decoded byte grid (GQ3 oracle baseline).
pub fn brute_sum(grid: &[u8], side: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> u64 {
    let mut s = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            s += grid[y * side + x] as u64;
        }
    }
    s
}

/// Rectangle in half-open grid coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Rect {
    pub fn new(x0: usize, y0: usize, x1: usize, y1: usize) -> Rect {
        assert!(x0 <= x1 && y0 <= y1, "degenerate rect");
        Rect { x0, y0, x1, y1 }
    }
}

/// Analytical region sum over the region tree, no full decode.
/// Q = half-open rectangle. Signed corrections applied per exception.
pub fn region_sum_rect(
    reg: &Region,
    top: usize,
    left: usize,
    side: usize,
    q: Rect,
) -> Result<u64, String> {
    let (x0, y0, x1, y1) = (q.x0, q.y0, q.x1, q.y1);
    // no intersection
    if x0 >= left + side || x1 <= left || y0 >= top + side || y1 <= top {
        return Ok(0);
    }
    match reg {
        Region::Const { value, .. } => {
            let cx0 = x0.max(left);
            let cx1 = x1.min(left + side);
            let cy0 = y0.max(top);
            let cy1 = y1.min(top + side);
            let n = (cx1 - cx0) * (cy1 - cy0);
            Ok((*value as u64) * (n as u64))
        }
        Region::Planar {
            a,
            b,
            c,
            exceptions,
            ..
        } => {
            let cx0 = x0.max(left).saturating_sub(left);
            let cx1 = (x1.min(left + side)).saturating_sub(left);
            let cy0 = y0.max(top).saturating_sub(top);
            let cy1 = (y1.min(top + side)).saturating_sub(top);
            // per-row linear closed form on LOCAL columns
            let mut total: i64 = 0;
            for rr in cy0..cy1 {
                let ar_loc = ((*a as u32).wrapping_add((*b as u32).wrapping_mul(rr as u32))) as i64;
                // value in row rr, local col cc = (ar + C·cc) mod 256
                //   = ((A + B·rr + C·cc) mod 256)
                // row-local closed form with s = cx0:
                let s_loc = window_sum_linear(ar_loc, *c as i64, cx0 as i64, (cx1 - cx0) as i64)
                    .map_err(|e| e.to_string())?;
                total += s_loc;
            }
            // exception corrections: signed, post-mod
            for (pos, v) in exceptions {
                let (pr, pc) = (*pos as usize / side, *pos as usize % side);
                let gy = top + pr;
                let gx = left + pc;
                if gx >= x0 && gx < x1 && gy >= y0 && gy < y1 {
                    let m = ((*a as usize).wrapping_add((*b as usize).wrapping_mul(pr)))
                        .wrapping_add((*c as usize).wrapping_mul(pc))
                        & 0xFF;
                    // signed correction AFTER mod-256 reduction
                    total += *v as i64 - m as i64;
                }
            }
            if total < 0 {
                return Err(format!("negative region sum {total} — contract violation"));
            }
            Ok(total as u64)
        }
        Region::Raw { body, .. } => {
            let cx0 = x0.max(left);
            let cx1 = x1.min(left + side);
            let cy0 = y0.max(top);
            let cy1 = y1.min(top + side);
            let mut s = 0u64;
            for yy in cy0..cy1 {
                for xx in cx0..cx1 {
                    s += body[(yy - top) * side + (xx - left)] as u64;
                }
            }
            Ok(s)
        }
        Region::Split { children, .. } => {
            let h = side / 2;
            Ok(region_sum_rect(&children[0], top, left, h, q)?
                + region_sum_rect(&children[1], top, left + h, h, q)?
                + region_sum_rect(&children[2], top + h, left, h, q)?
                + region_sum_rect(&children[3], top + h, left + h, h, q)?)
        }
    }
}

/// Convenience wrapper, full-tree offsets.
pub fn region_sum_tree(reg: &Region, q: Rect) -> Result<u64, String> {
    let side = reg.side();
    region_sum_rect(reg, 0, 0, side, q)
}
