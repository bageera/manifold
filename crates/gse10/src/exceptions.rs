//! Exception indexes (GSE-10 spec §3): two strategies + cost
//! attribution. T_query = T_model + T_index + T_correction; instrument
//! counts index traversal separately from actual corrections applied.

/// Prebuilt exception index over one decoded region tree (leaf-level
/// planar exceptions hoisted to whole-frame coordinates).
pub struct FlatExceptions {
    /// sorted by whole-frame row then column
    pub entries: Vec<(usize, usize, u8)>, // (row, col, v)
    /// row_offsets[r] = first entry index with row >= r
    pub row_offsets: Vec<usize>,
    side: usize,
}

impl FlatExceptions {
    /// Build the sorted-positions index from a decoded region tree.
    pub fn sorted(reg: &crate::model::Region, side: usize) -> FlatExceptions {
        let mut entries: Vec<(usize, usize, u8)> = Vec::new();
        collect_planar(reg, 0, 0, side, &mut entries);
        entries.sort_by_key(|e| (e.0, e.1));
        let rows = side + 1;
        let mut row_offsets = vec![0usize; rows];
        let mut r = 0usize;
        for (i, e) in entries.iter().enumerate() {
            while r <= e.0 && r < rows {
                row_offsets[r] = i;
                r += 1;
            }
        }
        while r < rows {
            row_offsets[r] = entries.len();
            r += 1;
        }
        FlatExceptions {
            entries,
            row_offsets,
            side,
        }
    }

    /// Visit exceptions inside [x0,x1)×[y0,y1); visitor receives
    /// (row, col, v). Returns (visited, hits) for cost attribution.
    pub fn for_each_in_rect(
        &self,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        mut f: impl FnMut(usize, usize, u8),
    ) -> (usize, usize) {
        let mut hits = 0;
        let mut visited = 0;
        for y in y0..y1 {
            let start = self.row_offsets[y];
            let end = self.row_offsets[y + 1];
            // binary search cols within [start, end)
            let lo = start + self.entries[start..end].partition_point(|e| e.1 < x0);
            let hi = start + self.entries[start..end].partition_point(|e| e.1 < x1);
            for e in &self.entries[lo..hi] {
                f(e.0, e.1, e.2);
                hits += 1;
            }
            visited += hi - lo;
        }
        (visited, hits)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn side(&self) -> usize {
        self.side
    }
}

fn collect_planar(
    reg: &crate::model::Region,
    top: usize,
    left: usize,
    side: usize,
    out: &mut Vec<(usize, usize, u8)>,
) {
    match reg {
        crate::model::Region::Planar { exceptions, .. } => {
            for (pos, v) in exceptions {
                let (pr, pc) = (*pos as usize / side, *pos as usize % side);
                out.push((top + pr, left + pc, *v));
            }
        }
        crate::model::Region::Split { children, .. } => {
            let h = side / 2;
            collect_planar(&children[0], top, left, h, out);
            collect_planar(&children[1], top, left + h, h, out);
            collect_planar(&children[2], top + h, left, h, out);
            collect_planar(&children[3], top + h, left + h, h, out);
        }
        _ => {}
    }
}

/// Strategy 2: row buckets with cumulative corrections (per row prefix
/// sums of corrections for repeated narrow row-range queries).
pub struct RowBucketIndex {
    pub rows: Vec<Vec<(usize, u8)>>, // row -> [(col, v)]
    side: usize,
}

impl RowBucketIndex {
    pub fn build(reg: &crate::model::Region, side: usize) -> RowBucketIndex {
        let mut entries: Vec<(usize, usize, u8)> = Vec::new();
        collect_planar(reg, 0, 0, side, &mut entries);
        let mut rows = vec![Vec::new(); side];
        for (r, c, v) in entries {
            rows[r].push((c, v));
        }
        for row in &mut rows {
            row.sort_by_key(|e| e.0);
        }
        RowBucketIndex { rows, side }
    }

    pub fn for_each_in_rect(
        &self,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        mut f: impl FnMut(usize, usize, u8),
    ) -> (usize, usize) {
        let mut visited = 0;
        let mut hits = 0;
        for y in y0..y1 {
            for (c, v) in &self.rows[y] {
                visited += 1;
                if *c >= x0 && *c < x1 {
                    f(y, *c, *v);
                    hits += 1;
                }
            }
        }
        (visited, hits)
    }

    pub fn side(&self) -> usize {
        self.side
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{decode_wire, regions_to_grid};

    fn build_planar_with_exceptions(side: usize) -> (Vec<u8>, crate::model::Region) {
        // hand-wire: whole-frame planar with a few exceptions
        let mut w = Vec::new();
        w.push(0x01u8);
        w.extend_from_slice(&[3, 7, 11]);
        // varint k
        let k = 3usize;
        assert!(k < 0x80);
        w.push(k as u8);
        // positions row-major; include a wraparound-correction case
        let pos0 = 0usize;
        w.push(pos0 as u8); // (0,0), M=3 → v=0 → correction −3
        w.push(0);
        let pos1 = side + 1; // (1,1), M=3+7+11=21 → v=250 → +229
        w.push(pos1 as u8);
        w.push(250);
        let pos2 = 2 * side + 2; // (2,2), M=(3+14+22)=39 → v=38 → −1
        w.push(pos2 as u8);
        w.push(38);
        let reg = decode_wire(&w, side).unwrap();
        (w, reg)
    }

    #[test]
    fn sorted_index_hits_and_costs() {
        let (_w, reg) = build_planar_with_exceptions(32);
        let grid = regions_to_grid(&reg, 32).unwrap();
        let idx = FlatExceptions::sorted(&reg, 32);
        assert_eq!(idx.len(), 3);
        // rect covering (1,1) only
        let mut seen = Vec::new();
        let (visited, hits) = idx.for_each_in_rect(1, 1, 2, 2, |r, c, v| seen.push((r, c, v)));
        assert_eq!(seen, vec![(1, 1, 250)]);
        assert_eq!(hits, 1);
        // cost attribution: traversal visited 1 row's slice, 1 hit
        assert_eq!(visited, 1, "visited must not exceed scanned span");
        // full-frame
        let (visited2, hits2) = idx.for_each_in_rect(0, 0, 32, 32, |_, _, _| {});
        assert_eq!(hits2, 3);
        assert!(visited2 >= hits2);
        let _ = grid;
    }

    #[test]
    fn row_bucket_hits() {
        let (_w, reg) = build_planar_with_exceptions(32);
        let idx = RowBucketIndex::build(&reg, 32);
        let mut hits = 0;
        let (_v, h) = idx.for_each_in_rect(0, 0, 32, 32, |_, _, _| {
            hits += 1;
        });
        assert_eq!(h, 3);
        assert_eq!(hits, 3);
    }
}
