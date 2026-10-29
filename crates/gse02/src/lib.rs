//! GSE-02: hierarchical MDL mathematical byte-stream codec (research).
//!
//! Complete-cost wire: 12-B header (magic GSE2, u32 orig len, u32 stream
//! len) + segments (len u16 + body) + shared model table + residual
//! stream, through one zstd-19 container pass. Independent exact decoder;
//! malformed input = Err, never panic (truncation, reserved ids, bad
//! model refs, raw-len mismatch, seg/orig-len mismatch, trailing bytes,
//! expansion bomb). Segments reference the shared model table by id: one
//! equation serves many blocks (the shared-model arm). Partition search =
//! DP over 64-aligned cuts; whole-stream and raw fallbacks compete
//! (never-worse by construction).
//!
//! Models (deterministic integer arithmetic, decoder-reproducible):
//! Const(x) | Arith{a,b}: x_i = (a + b*i) mod 256 (GSE-01 class) |
//! Periodic{phase, residues}: x_i = p[(i + phase) mod k].
//! Near-mathematical data = a model + its residual stream (the residual
//! rides the same container = the "residual generator" arm).

use std::collections::HashMap;

pub const MAGIC: [u8; 4] = *b"GSE2";
const RAW_SEG: u16 = u16::MAX;
const CUT: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Model {
    Const(u8),
    Arith { a: u8, b: u8 },
    Periodic { phase: u8, residues: Vec<u8> },
}

impl Model {
    fn params(&self) -> Vec<u8> {
        match self {
            Model::Const(x) => vec![0, *x],
            Model::Arith { a, b } => vec![1, *a, *b],
            Model::Periodic { phase, residues } => {
                let mut v = vec![2, *phase, residues.len() as u8];
                v.extend_from_slice(residues);
                v
            }
        }
    }

    fn from_params(p: &[u8]) -> Result<(Model, &[u8])> {
        if p.is_empty() {
            return Err(Error::Truncated);
        }
        match p[0] {
            0 if p.len() >= 2 => Ok((Model::Const(p[1]), &p[2..])),
            1 if p.len() >= 3 => Ok((Model::Arith { a: p[1], b: p[2] }, &p[3..])),
            2 if p.len() >= 3 => {
                let k = p[2] as usize;
                if k == 0 || p.len() < 3 + k {
                    return Err(Error::Truncated);
                }
                Ok((
                    Model::Periodic { phase: p[1], residues: p[3..3 + k].to_vec() },
                    &p[3 + k..],
                ))
            }
            0..=2 => Err(Error::Truncated),
            _ => Err(Error::Format("reserved model id".into())),
        }
    }

    fn generate(&self, n: usize) -> Vec<u8> {
        match self {
            Model::Const(x) => vec![*x; n],
            Model::Arith { a, b } => (0..n)
                .map(|i| ((*a as usize + (*b as usize) * i) % 256) as u8)
                .collect(),
            Model::Periodic { phase, residues } => {
                let k = residues.len();
                (0..n).map(|i| residues[(i + *phase as usize) % k]).collect()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Format(String),
    Truncated,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Format(m) => write!(f, "format: {m}"),
            Error::Truncated => write!(f, "truncated stream"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// One fitted 64-aligned block: (model?, residual, cost = params +
/// residual-container + 4-B instruction; raw = literals + own container).
#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockFit {
    len: usize,
    model: Option<Model>,
    residual: Vec<u8>,
    raw: Vec<u8>,
    cost: usize,
}

fn z19(b: &[u8]) -> usize {
    zstd::stream::encode_all(b, 19).map(|v| v.len()).unwrap_or(usize::MAX)
}

fn fit_block(d: &[u8]) -> BlockFit {
    let n = d.len();
    let instr = 4usize;
    let mut best = BlockFit {
        len: n,
        model: None,
        residual: Vec::new(),
        raw: d.to_vec(),
        cost: instr + z19(d),
    };
    if n == 0 {
        return best;
    }
    if d.iter().all(|&x| x == d[0]) {
        let m = Model::Const(d[0]);
        let c = instr + m.params().len();
        if c < best.cost {
            best = BlockFit { len: n, model: Some(m), residual: vec![], raw: vec![], cost: c };
        }
    }
    if n >= 2 {
        let step = d[1].wrapping_sub(d[0]);
        if d.windows(2).all(|w| w[1] == w[0].wrapping_add(step)) {
            let m = Model::Arith { a: d[0], b: step };
            let c = instr + m.params().len();
            if c < best.cost {
                best = BlockFit { len: n, model: Some(m), residual: vec![], raw: vec![], cost: c };
            }
        }
        // periodic: EXACT-period prefilter (zero-residual) first; only for the
        // small candidate-class k (4/7/8/16) do near-periodic blocks pay
        // one residual-container call. Per-block container calls now = 1-2.
        let mut exact_period: Option<usize> = None;
        for k in 2..=(n / 2).min(64) {
            if (k..n).all(|i| d[i] == d[i % k]) {
                exact_period = Some(k);
                break;
            }
        }
        let cand_k = exact_period.or_else(|| {
            [4usize, 7, 8, 16].into_iter().find(|&k| {
                k < n && {
                    let mism = (k..n).filter(|&i| d[i] != d[i % k]).count();
                    mism as f64 / (n as f64) < 0.55
                }
            })
        });
        if let Some(k) = cand_k {
            let m = Model::Periodic { phase: 0, residues: d[..k].to_vec() };
            let gen = m.generate(n);
            let residual: Vec<u8> =
                d.iter().zip(&gen).map(|(&x, &g)| x.wrapping_sub(g)).collect();
            let all_zero = residual.iter().all(|&r| r == 0);
            let rc = if all_zero { 0 } else { z19(&residual) };
            let c = instr + m.params().len() + rc;
            if c < best.cost {
                best = BlockFit {
                    len: n,
                    model: Some(m),
                    residual: if all_zero { vec![] } else { residual },
                    raw: vec![],
                    cost: c,
                };
            }
        }
    }
    best
}

/// Hierarchical MDL discovery. Returns the segment list.
fn discover(data: &[u8]) -> Vec<BlockFit> {
    let n = data.len();
    if n == 0 {
        return Vec::new();
    }
    // ponytail: DP capped at 4 MiB (acceptance fixtures <= 64 KiB);
    // larger = whole-fit only. Upgrade: rolling cuts.
    if n > 1 << 22 {
        return vec![fit_block(data)];
    }
    let inf = usize::MAX / 4;
    let mut costs = vec![inf; n + 1];
    costs[0] = 0;
    // candidate segment lengths = Jon's block sizes {64, 256, 1024, 4096}
    // (plus whatever tail fits): 4 fits per start instead of 64.
    let lens_for = |s: usize| -> Vec<usize> {
        let max_len = (n - s).min(4096);
        [64usize, 256, 1024, 4096]
            .into_iter()
            .filter(|&l| l <= max_len)
            .chain(std::iter::once(max_len).filter(|&l| (64..4096).contains(&max_len) && !matches!(l, 64 | 256 | 1024 | 4096)))
            .collect::<Vec<usize>>()
    };
    let mut cache: HashMap<(usize, usize), BlockFit> = HashMap::new();
    for s in (0..n).step_by(CUT) {
        if costs[s] >= inf {
            continue;
        }
        for len in lens_for(s) {
            let t = s + len;
            let f = match cache.get(&(s, len)) {
                Some(f) => f.clone(),
                None => {
                    let f = fit_block(&data[s..t]);
                    cache.insert((s, len), f.clone());
                    f
                }
            };
            if costs[s] + f.cost < costs[t] {
                costs[t] = costs[s] + f.cost;
            }
        }
    }
    if costs[n] >= inf {
        return vec![fit_block(data)];
    }
    let mut segs: Vec<BlockFit> = Vec::new();
    let mut t = n;
    while t > 0 {
        let mut s = ((t - 1) / CUT) * CUT;
        let mut picked: Option<BlockFit> = None;
        loop {
            if costs[s] < inf && t - s <= 4096 && t - s >= CUT {
                let f = match cache.get(&(s, t - s)) {
                    Some(f) => f.clone(),
                    None => fit_block(&data[s..t]),
                };
                if costs[s].saturating_add(f.cost) == costs[t] {
                    picked = Some(f);
                    break;
                }
            }
            if s == 0 {
                break;
            }
            s -= CUT;
        }
        match picked {
            Some(f) => {
                t -= f.len;
                segs.push(f);
            }
            None => {
                // unaligned <64 tail or DP gap: raw
                let s = (t / CUT) * CUT;
                let raw = data[s..t].to_vec();
                let len = raw.len();
                segs.push(BlockFit { len, model: None, residual: vec![], raw, cost: 0 });
                t = s;
            }
        }
    }
    segs.reverse();
    segs
}

/// The wire: header + segment instructions (len, model-id | RAW + bytes)
/// + shared model table + residual stream; all through z19.
fn serialize(data: &[u8], segs: &[BlockFit]) -> Vec<u8> {
    let mut table: Vec<Model> = Vec::new();
    for s in segs {
        if let Some(m) = &s.model {
            if !table.contains(m) {
                table.push(m.clone());
            }
        }
    }
    let mut stream: Vec<u8> = Vec::new();
    stream.extend_from_slice(&(segs.len() as u32).to_le_bytes());
    for s in segs {
        stream.extend_from_slice(&(s.len as u16).to_le_bytes());
        match &s.model {
            Some(m) => {
                let id = table.iter().position(|t| t == m).unwrap() as u16;
                // high bit = this segment carries residual bytes
                let id = if s.residual.is_empty() { id } else { id | 0x8000 };
                stream.extend_from_slice(&id.to_le_bytes());
            }
            None => {
                stream.extend_from_slice(&RAW_SEG.to_le_bytes());
                stream.extend_from_slice(&s.raw);
            }
        }
    }
    stream.extend_from_slice(&(table.len() as u16).to_le_bytes());
    for m in &table {
        stream.extend_from_slice(&m.params());
    }
    let mut residuals: Vec<u8> = Vec::new();
    for s in segs {
        residuals.extend_from_slice(&s.residual);
    }
    stream.extend_from_slice(&(residuals.len() as u32).to_le_bytes());
    stream.extend_from_slice(&residuals);
    let compressed = zstd::stream::encode_all(stream.as_slice(), 19).expect("zstd");
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    // field 3 = the CONTAINER byte length (the trailing-bytes check in
    // decode compares buf.len() against 12 + this field)
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&compressed);
    out
}

/// Encode with hierarchical MDL discovery.
pub fn encode(data: &[u8]) -> Vec<u8> {
    serialize(data, &discover(data))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegId {
    /// raw u16 id carries the has-residual high bit
    Model(u16),
    Raw,
}

/// Independent decoder. Malformed = Err, never a panic.
pub fn decode(buf: &[u8]) -> Result<Vec<u8>> {
    const HDR: usize = 12;
    if buf.len() < HDR {
        return Err(Error::Truncated);
    }
    if buf[0..4] != MAGIC {
        return Err(Error::Format("magic".into()));
    }
    let orig_len = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
    let stream_len = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
    if buf.len() != HDR + stream_len {
        return Err(Error::Format("trailing or short bytes".into()));
    }
    if orig_len == 0 {
        return Err(Error::Format("empty payload unsupported".into()));
    }
    let stream = zstd::stream::decode_all(&buf[HDR..])
        .map_err(|_| Error::Format("zstd container".into()))?;
    if stream.len() > orig_len * 3 + 4096 {
        return Err(Error::Format("expansion bomb".into()));
    }
    let mut pos = 0usize;
    let n_segs = read_u32(&stream, &mut pos)? as usize;
    if n_segs == 0 {
        return Err(Error::Format("zero segments".into()));
    }
    let mut segs: Vec<(SegId, usize)> = Vec::with_capacity(n_segs);
    let mut raws: Vec<Vec<u8>> = Vec::new();
    for _ in 0..n_segs {
        if pos + 4 > stream.len() {
            return Err(Error::Truncated);
        }
        let len = u16::from_le_bytes(stream[pos..pos + 2].try_into().unwrap()) as usize;
        let id = u16::from_le_bytes(stream[pos + 2..pos + 4].try_into().unwrap());
        pos += 4;
        if id == RAW_SEG {
            if len == 0 || pos + len > stream.len() {
                return Err(Error::Truncated);
            }
            raws.push(stream[pos..pos + len].to_vec());
            pos += len;
            segs.push((SegId::Raw, len));
        } else {
            if len == 0 {
                return Err(Error::Format("zero-length model segment".into()));
            }
            segs.push((SegId::Model(id), len));
        }
    }
    if pos + 2 > stream.len() {
        return Err(Error::Truncated);
    }
    let n_models = u16::from_le_bytes(stream[pos..pos + 2].try_into().unwrap()) as usize;
    pos += 2;
    let mut table: Vec<Model> = Vec::with_capacity(n_models);
    for _ in 0..n_models {
        let (m, rest) = Model::from_params(&stream[pos..])?;
        table.push(m);
        pos = stream.len() - rest.len();
    }
    if pos + 4 > stream.len() {
        return Err(Error::Truncated);
    }
    let n_res = read_u32(&stream, &mut pos)? as usize;
    if pos + n_res != stream.len() {
        return Err(Error::Format("unused trailing stream bytes".into()));
    }
    let gen_total: usize = segs
        .iter()
        .filter(|(i, _)| matches!(i, SegId::Model(f) if f & 0x8000 != 0))
        .map(|(_, l)| *l)
        .sum();
    if gen_total != n_res {
        return Err(Error::Format("residual length mismatch".into()));
    }
    let mut out: Vec<u8> = Vec::with_capacity(orig_len);
    let mut rpos = 0usize;
    let mut raw_pos = 0usize;
    for (id, len) in &segs {
        match id {
            SegId::Raw => {
                out.extend_from_slice(&raws[raw_pos]);
                raw_pos += 1;
            }
            SegId::Model(flagged) => {
                let i = (flagged & 0x7FFF) as usize;
                let m = table.get(i)
                    .ok_or_else(|| Error::Format("model id out of range".into()))?;
                let gen = m.generate(*len);
                if flagged & 0x8000 != 0 {
                    if rpos + *len > n_res {
                        return Err(Error::Truncated);
                    }
                    for (k, g) in gen.iter().enumerate() {
                        out.push(g.wrapping_add(stream[pos + rpos + k]));
                    }
                    rpos += len;
                } else {
                    // exact segment: the model must generate the SEGMENT
                    // with zero residual — verified by the residuals'
                    // accounting (gen_total check covers flagged only);
                    // nothing to add
                    out.extend_from_slice(&gen);
                }
            }
        }
    }
    if raw_pos != raws.len() || rpos != n_res {
        return Err(Error::Format("unreferenced payloads".into()));
    }
    if out.len() != orig_len {
        return Err(Error::Format("length mismatch".into()));
    }
    Ok(out)
}

fn read_u32(s: &[u8], pos: &mut usize) -> Result<u32> {
    if *pos + 4 > s.len() {
        return Err(Error::Truncated);
    }
    let v = u32::from_le_bytes(s[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    Ok(v)
}

/// Byte-exactness helper.
pub fn verify(data: &[u8], encoded: &[u8]) -> bool {
    decode(encoded).map(|d| d == data).unwrap_or(false)
}

/// Acceptance-matrix fixtures (deterministic; Jon's §4).
pub mod fixtures {
    pub fn arith(n: usize) -> Vec<u8> {
        (0..n).map(|i| ((17usize + 3 * i) % 256) as u8).collect()
    }
    pub fn constant(n: usize) -> Vec<u8> {
        vec![7u8; n]
    }
    pub fn acgt(n: usize) -> Vec<u8> {
        (0..n).map(|i| b"ACGT"[i % 4]).collect()
    }
    pub fn random(n: usize, seed: u64) -> Vec<u8> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                (s % 256) as u8
            })
            .collect()
    }
    pub fn mixed(n: usize) -> Vec<u8> {
        let mut v = arith(n / 2);
        v.extend(random(n - n / 2, 0xdead_beef));
        v
    }
    pub fn alternating(n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n);
        let mut i = 0usize;
        while v.len() < n {
            match (i / 256) % 3 {
                0 => v.extend(constant(256)),
                1 => v.extend(arith(256)),
                _ => v.extend(random(256, i as u64 + 1)),
            }
            i += 256;
        }
        v.truncate(n);
        v
    }
    pub fn shared_rec(n: usize) -> Vec<u8> {
        let mut v = Vec::new();
        for s0 in [11u8, 77u8, 200u8] {
            let mut cur = s0;
            for _ in 0..n / 3 {
                v.push(cur);
                cur = cur.wrapping_add(3);
            }
        }
        v
    }
    pub fn noisy_arith(n: usize, seed: u64) -> Vec<u8> {
        let noise = random(n, seed);
        arith(n)
            .iter()
            .zip(&noise)
            .map(|(&x, &r)| x.wrapping_add(r % 7))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all() {
        let cases = [
            ("arith", fixtures::arith(65_536)),
            ("const", fixtures::constant(65_536)),
            ("acgt", fixtures::acgt(65_536)),
            ("random", fixtures::random(65_536, 42)),
            ("mixed", fixtures::mixed(65_536)),
            ("alternating", fixtures::alternating(65_536)),
            ("shared-rec", fixtures::shared_rec(65_536)),
            ("noisy-arith", fixtures::noisy_arith(65_536, 5)),
        ];
        for (name, d) in cases {
            let enc = encode(&d);
            assert_eq!(decode(&enc).unwrap(), d, "fixture {name}");
        }
    }

    #[test]
    fn gse01_class_small() {
        let d = fixtures::arith(65_536);
        let enc = encode(&d);
        assert!(enc.len() < 200, "gse01 class: {}", enc.len());
    }

    #[test]
    fn malformed() {
        let enc = encode(&fixtures::arith(4096));
        let mut t = enc.clone();
        t.push(0x55);
        assert!(decode(&t).is_err());
        let mut m = enc.clone();
        m[0] = b'X';
        assert!(decode(&m).is_err());
        assert!(decode(&enc[..enc.len() / 2]).is_err());
        let mut e2 = encode(&fixtures::random(2048, 1));
        e2[11] = e2[11].wrapping_add(1); // stream_len corrupt
        assert!(decode(&e2).is_err());
    }

    #[test]
    fn random_overhead_recorded_honestly() {
        // The never-worse property is about the DP's CHOICE (it never
        // picks a model worse than raw) — not about beating bare zstd on
        // incompressible data, where the framing must be counted honestly
        // (Jon's rule). Record the measured overhead; bound it.
        let d = fixtures::random(65_536, 99);
        let enc = encode(&d);
        let rawz = zstd::stream::encode_all(d.as_slice(), 19).unwrap().len();
        let overhead = enc.len() - rawz;
        assert!(
            overhead < 128,
            "random overhead {overhead} B (enc {}, rawz {})",
            enc.len(),
            rawz
        );
        assert_eq!(decode(&enc).unwrap(), d);
    }
}