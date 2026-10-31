//! Cached-totals analytical evaluator (GSE-10 spec §2/§3):
//! whole-region sums cached at every quadtree node at INDEX time;
//! per-query cost = intersected-leaf closed forms (T_model) + index
//! traversal (T_index, only for leaves with exceptions) + signed
//! corrections (T_correction, only actual hits). No full-frame decode.
//! Reports (sum, visited_leaves, hits) so the gate can attribute cost.

use crate::floor_sum::window_sum_linear;
use crate::model::Region;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LeafKind {
    Const,
    Planar,
    Raw,
}

/// One cached node: cached total + tree bookkeeping.
pub struct CachedNode {
    pub top: usize,
    pub left: usize,
    pub side: usize,
    /// whole-node byte sum AFTER exceptions apply
    pub total: i64,
    /// direct-child node indices (None for leaves)
    pub children: Option<[usize; 4]>,
    pub is_leaf: bool,
    /// leaf-only: resolved kind
    kind: LeafKind,
    /// leaf-only params (const value in .0; planar A,B,C; raw zero)
    pub params: (u8, u8, u8),
    /// leaf-only: raw body (raw kind)
    raw: Vec<u8>,
    /// leaf-only: exceptions as whole-frame (row, col, v), wire order
    exceptions: Vec<(usize, usize, u8)>,
}

/// Cached-total index over a decoded region tree.
pub struct TotalIndex {
    pub nodes: Vec<CachedNode>,
    pub root_total: i64,
    pub side: usize,
    pub total_exceptions: usize,
}

impl TotalIndex {
    /// Build from encoded wire bytes (independent decode → index).
    /// All arithmetic checked; malformed wire = Err (never panic).
    pub fn build(wire: &[u8], side: usize) -> Result<TotalIndex, String> {
        let reg = crate::model::decode_wire(wire, side).map_err(|e| e.to_string())?;
        let mut nodes = Vec::new();
        insert(&reg, 0, 0, side, &mut nodes)?;
        let root_total = nodes[0].total;
        let total_exceptions: usize = nodes.iter().map(|n| n.exceptions.len()).sum();
        Ok(TotalIndex {
            nodes,
            root_total,
            side,
            total_exceptions,
        })
    }

    /// Region sum. Returns (sum, visited_leaves, hits) for cost
    /// attribution per spec §3: T_model ∝ visited, T_correction ∝ hits.
    pub fn region_sum(
        &self,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    ) -> Result<(u64, usize, usize), String> {
        let mut visited = 0usize;
        let mut hits = 0usize;
        let s = self.query(0, 0, 0, self.side, x0, y0, x1, y1, &mut visited, &mut hits)?;
        if s < 0 {
            return Err(format!("negative region sum {s} — contract violation"));
        }
        Ok((s as u64, visited, hits))
    }

    #[allow(clippy::too_many_arguments)]
    fn query(
        &self,
        ni: usize,
        top: usize,
        left: usize,
        side: usize,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        visited: &mut usize,
        hits: &mut usize,
    ) -> Result<i64, String> {
        let node = &self.nodes[ni];
        if x0 >= left + side || x1 <= left || y0 >= top + side || y1 <= top {
            return Ok(0); // no intersection
        }
        if x0 <= left && x1 >= left + side && y0 <= top && y1 >= top + side {
            *visited += 1;
            return Ok(node.total); // full coverage → cached total
        }
        if node.is_leaf {
            *visited += 1;
            return self.leaf_partial(ni, top, left, side, x0, y0, x1, y1, hits);
        }
        let ch = node.children.expect("internal children");
        let h = side / 2;
        let mut acc: i64 = 0;
        // recurse-all (each no-intersection child returns 0 in O(1))
        acc += self.query(ch[0], top, left, h, x0, y0, x1, y1, visited, hits)?;
        acc += self.query(ch[1], top, left + h, h, x0, y0, x1, y1, visited, hits)?;
        acc += self.query(ch[2], top + h, left, h, x0, y0, x1, y1, visited, hits)?;
        acc += self.query(ch[3], top + h, left + h, h, x0, y0, x1, y1, visited, hits)?;
        Ok(acc)
    }

    #[allow(clippy::too_many_arguments)]
    fn leaf_partial(
        &self,
        ni: usize,
        top: usize,
        left: usize,
        side: usize,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        hits: &mut usize,
    ) -> Result<i64, String> {
        let node = &self.nodes[ni];
        let cx0 = x0.max(left);
        let cx1 = x1.min(left + side);
        let cy0 = y0.max(top);
        let cy1 = y1.min(top + side);
        let mut t: i64 = 0;
        match node.kind {
            LeafKind::Const => {
                t = node.params.0 as i64 * ((cx1 - cx0) * (cy1 - cy0)) as i64;
            }
            LeafKind::Planar => {
                let (a, b, c) = node.params;
                for gy in cy0..cy1 {
                    let lr = gy - top;
                    let lc0 = cx0 - left;
                    let lc1 = cx1 - left;
                    let ar = (a as i64 + b as i64 * lr as i64).rem_euclid(256);
                    t += window_sum_linear(ar, c as i64, lc0 as i64, (lc1 - lc0) as i64)
                        .map_err(|e| e.to_string())?;
                }
                for &(gr, gc, v) in &node.exceptions {
                    if gc >= cx0 && gc < cx1 && gr >= cy0 && gr < cy1 {
                        *hits += 1;
                        let m = (a as i64
                            + b as i64 * (gr - top) as i64
                            + c as i64 * (gc - left) as i64)
                            .rem_euclid(256);
                        t += v as i64 - m; // signed correction, post-mod (v1.1)
                    }
                }
            }
            LeafKind::Raw => {
                // honest fallback: sum the intersected literals
                for gy in cy0..cy1 {
                    for gx in cx0..cx1 {
                        t += node.raw[(gy - top) * side + (gx - left)] as i64;
                    }
                }
            }
        }
        Ok(t)
    }
}

fn insert(
    reg: &Region,
    top: usize,
    left: usize,
    side: usize,
    nodes: &mut Vec<CachedNode>,
) -> Result<usize, String> {
    let ni = nodes.len();
    match reg {
        Region::Const { value, .. } => {
            nodes.push(CachedNode {
                top,
                left,
                side,
                total: *value as i64 * (side * side) as i64,
                children: None,
                is_leaf: true,
                kind: LeafKind::Const,
                params: (*value, 0, 0),
                raw: Vec::new(),
                exceptions: Vec::new(),
            });
            Ok(ni)
        }
        Region::Planar {
            a,
            b,
            c,
            exceptions,
            ..
        } => {
            // cached total = closed form + signed exception corrections
            let mut total: i64 = 0;
            for lr in 0..side {
                let ar = (*a as i64 + *b as i64 * lr as i64).rem_euclid(256);
                total +=
                    window_sum_linear(ar, *c as i64, 0, side as i64).map_err(|e| e.to_string())?;
            }
            let mut excs = Vec::with_capacity(exceptions.len());
            for (pos, v) in exceptions {
                let pr = *pos as usize / side;
                let pc = *pos as usize % side;
                let m = (*a as usize)
                    .wrapping_add((*b as usize).wrapping_mul(pr))
                    .wrapping_add((*c as usize).wrapping_mul(pc))
                    & 0xFF;
                total += *v as i64 - m as i64;
                excs.push((top + pr, left + pc, *v));
            }
            nodes.push(CachedNode {
                top,
                left,
                side,
                total,
                children: None,
                is_leaf: true,
                kind: LeafKind::Planar,
                params: (*a, *b, *c),
                raw: Vec::new(),
                exceptions: excs,
            });
            Ok(ni)
        }
        Region::Raw { body, .. } => {
            let total: i64 = body.iter().map(|&v| v as i64).sum();
            nodes.push(CachedNode {
                top,
                left,
                side,
                total,
                children: None,
                is_leaf: true,
                kind: LeafKind::Raw,
                params: (0, 0, 0),
                raw: body.clone(),
                exceptions: Vec::new(),
            });
            Ok(ni)
        }
        Region::Split { children, .. } => {
            // reserve slot (pre-order index stable), recurse, fill back
            nodes.push(CachedNode {
                top,
                left,
                side,
                total: 0,
                children: None,
                is_leaf: false,
                kind: LeafKind::Const, // placeholder for internal nodes
                params: (0, 0, 0),
                raw: Vec::new(),
                exceptions: Vec::new(),
            });
            let h = side / 2;
            let c0 = insert(&children[0], top, left, h, nodes)?;
            let c1 = insert(&children[1], top, left + h, h, nodes)?;
            let c2 = insert(&children[2], top + h, left, h, nodes)?;
            let c3 = insert(&children[3], top + h, left + h, h, nodes)?;
            let total = nodes[c0].total + nodes[c1].total + nodes[c2].total + nodes[c3].total;
            let n = &mut nodes[ni];
            n.children = Some([c0, c1, c2, c3]);
            n.total = total;
            Ok(ni)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{pack_quadtree, pack_single_raw};
    use crate::fixtures::{make, Kind};

    fn oracle(wire: &[u8], side: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> u64 {
        // GQ3: brute over INDEPENDENTLY DECODED bytes
        let g = crate::model::grid_from_wire(wire, side).unwrap();
        let mut s = 0u64;
        for y in y0..y1 {
            for x in x0..x1 {
                s += g[y * side + x] as u64;
            }
        }
        s
    }

    #[test]
    fn cached_totals_exact_random_windows_mixed() {
        let g = make(Kind::Mixed, 128, 9);
        let (wire, _) = pack_quadtree(&g, 128, 8);
        let idx = TotalIndex::build(&wire, 128).unwrap();
        let mut l = crate::fixtures::PythonRng::new(77);
        for _ in 0..300 {
            let x0 = l.randbelow(128).expect("in-range");
            let y0 = l.randbelow(128).expect("in-range");
            let w = 1 + l.randbelow(128 - x0).expect("in-range");
            let h = 1 + l.randbelow(128 - y0).expect("in-range");
            let (got, _v, _h) = idx.region_sum(x0, y0, x0 + w, y0 + h).unwrap();
            let want = oracle(&wire, 128, x0, y0, x0 + w, y0 + h);
            assert_eq!(got, want, "rect ({x0},{y0})+{w}x{h}");
        }
    }

    #[test]
    fn cached_totals_exact_wraparound_corrections() {
        // near-255 plane + small replacements: deep NEGATIVE corrections.
        // NOTE: pack_single_planar fits EXACTLY (fails on exceptions by
        // policy — exceptions ride the QUADTREE in the committed
        // reproduction), so this test packs via the quadtree.
        let mut g = make(Kind::Mathematical, 32, 5);
        for y in 0..32 {
            for x in 0..32 {
                g[y * 32 + x] = (250u32.wrapping_add((7 * x) as u32) & 0xFF) as u8;
            }
        }
        g[10 * 32 + 10] = 0; // M=250 → correction −250
        g[11 * 32 + 11] = 3; // M=4 → correction −1
        let (wire, _) = pack_quadtree(&g, 32, 8);
        let idx = TotalIndex::build(&wire, 32).unwrap();
        for (x0, y0) in [(5usize, 5usize), (8, 8), (10, 10), (0, 0), (28, 28)] {
            for s2 in [1usize, 2, 4, 8] {
                let x1 = (x0 + s2).min(32);
                let y1 = (y0 + s2).min(32);
                let (got, _, _) = idx.region_sum(x0, y0, x1, y1).unwrap();
                let want = oracle(&wire, 32, x0, y0, x1, y1);
                assert_eq!(got, want, "rect ({x0},{y0})..({x1},{y1})");
            }
        }
        // exact-hit accounting: [10..11)² sees exactly 1 visited, 1 hit
        let (_, v, h) = idx.region_sum(10, 10, 11, 11).unwrap();
        assert_eq!(v, 1, "visited");
        assert_eq!(h, 1, "exactly one correction applied");
        // and the sum equals the corrected value
        assert_eq!(idx.region_sum(10, 10, 11, 11).unwrap().0, 0);
    }

    #[test]
    fn raw_fallback_costs_scan_of_intersected_part() {
        let g = make(Kind::Random, 64, 8);
        let (wire, _) = pack_single_raw(&g, 64);
        let idx = TotalIndex::build(&wire, 64).unwrap();
        let (got, v, h) = idx.region_sum(8, 8, 12, 12).unwrap();
        assert_eq!(v, 1);
        assert_eq!(h, 0);
        assert_eq!(got, oracle(&wire, 64, 8, 8, 12, 12));
        assert_eq!(
            idx.root_total,
            wire[1..].iter().map(|&b| b as i64).sum::<i64>()
        );
    }

    #[test]
    fn visited_leaves_bounded_by_tree_depth() {
        // full-frame query visits exactly ONE node (root cached total)
        let g = make(Kind::Mixed, 128, 9);
        let (wire, _) = pack_quadtree(&g, 128, 8);
        let idx = TotalIndex::build(&wire, 128).unwrap();
        let (sum, v, _) = idx.region_sum(0, 0, 128, 128).unwrap();
        assert_eq!(v, 1, "full-frame must read only the root cached total");
        assert_eq!(sum, idx.root_total as u64);
    }
}
