// HP8-P3 — actual entropy-coding wire accounting via production coder.
//   MNF_HP8_WIRE=1 MNF_HP8_VEC=<file> MNF_HP8_SYMS=<file> MNF_HP8_P=16 \
//     cargo test --release hp8_wire_probe -- --ignored --nocapture
// Reads u32 fixed-point rows (n,256) + symbol bytes; LR-normalizes each row
// to 2^P; encodes every symbol with a fresh sum-to-2^P model; verifies exact
// decode with the mirrored decoder; prints the accounting block.
use constriction::stream::model::ContiguousCategoricalEntropyModel;
use constriction::stream::queue::{DefaultRangeDecoder, DefaultRangeEncoder};
use constriction::stream::{Decode, Encode};

fn lr_to_total(counts: &[u64; 256], total: u64) -> [u32; 256] {
    let sum: u64 = counts.iter().sum();
    let mut probs = [0u32; 256];
    let mut rem = [0u64; 256];
    let mut acc: u64 = 0;
    for i in 0..256 {
        let v = counts[i] * total / sum;
        probs[i] = v as u32;
        rem[i] = counts[i] * total - v * sum;
        acc += v;
    }
    let mut grants = total - acc;
    let mut order: Vec<usize> = (0..256).collect();
    order.sort_by(|&a, &b| rem[b].cmp(&rem[a]).then(a.cmp(&b)));
    let mut k = 0usize;
    while grants > 0 {
        let i = order[k % 256];
        if probs[i] == 0 {
            k += 1;
            continue;
        }
        probs[i] += 1;
        grants -= 1;
        k += 1;
    }
    for i in 0..256 {
        if probs[i] == 0 {
            let src = (0..256).max_by_key(|&j| probs[j]).unwrap();
            probs[src] -= 1;
            probs[i] = 1;
        }
    }
    probs
}

fn build_model<const P: usize>(
    probs: &[u32; 256],
) -> ContiguousCategoricalEntropyModel<u32, Vec<u32>, P> {
    ContiguousCategoricalEntropyModel::<u32, Vec<u32>, P>::from_nonzero_fixed_point_probabilities(
        probs.to_vec(),
        false,
    )
    .unwrap()
}

macro_rules! run_at_precision {
    ($fn_name:ident, $precision:literal) => {
        fn $fn_name(rows: &[&[u8]], syms: &[u8]) {
            let p: usize = $precision;
            let total = 1u64 << p;
            let mut ideal_bits = 0f64;
            let mut enc = DefaultRangeEncoder::new();
            for (ri, row) in rows.iter().enumerate() {
                let mut counts = [0u64; 256];
                for (i, c) in counts.iter_mut().enumerate() {
                    *c = u32::from_le_bytes(row[i * 4..i * 4 + 4].try_into().unwrap()) as u64;
                }
                let probs = lr_to_total(&counts, total);
                let sym = syms[ri] as usize;
                ideal_bits += p as f64 - (probs[sym] as f64).log2();
                let model = build_model::<$precision>(&probs);
                enc.encode_symbols(std::iter::once((sym, &model))).unwrap();
            }
            let words = enc.into_compressed().unwrap();
            let mut bytes = Vec::with_capacity(words.len() * 4);
            for w in words.iter() {
                bytes.extend_from_slice(&w.to_le_bytes());
            }
            // decode round-trip with the mirrored decoder (word backend)
            let mut dec = DefaultRangeDecoder::from_compressed(words.clone()).unwrap();
            let mut ok = true;
            for (ri, row) in rows.iter().enumerate() {
                let mut counts = [0u64; 256];
                for (i, c) in counts.iter_mut().enumerate() {
                    *c = u32::from_le_bytes(row[i * 4..i * 4 + 4].try_into().unwrap()) as u64;
                }
                let probs = lr_to_total(&counts, total);
                let model = build_model::<$precision>(&probs);
                match dec.decode_symbol(&model) {
                    Ok(s) if s == syms[ri] as usize => {}
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            println!(
                "HP8WIRE precision={} n={} ideal_bits={:.1} finalized_bytes={} \
                 payload_bytes={} framing_bytes={} decode_exact={} (per-row fresh models; \
                 model_bytes=0 in-stream)",
                p,
                syms.len(),
                ideal_bits,
                bytes.len(),
                bytes.len(),
                0,
                ok
            );
        }
    };
}

run_at_precision!(run_p24, 24);
run_at_precision!(run_p20, 20);
run_at_precision!(run_p16, 16);
run_at_precision!(run_p14, 14);
run_at_precision!(run_p12, 12);
run_at_precision!(run_p10, 10);
run_at_precision!(run_p8, 8);

#[test]
#[ignore]
fn hp8_wire_probe() {
    if std::env::var("MNF_HP8_WIRE").is_err() {
        return;
    }
    let vec_path = std::env::var("MNF_HP8_VEC").expect("MNF_HP8_VEC");
    let syms_path = std::env::var("MNF_HP8_SYMS").expect("MNF_HP8_SYMS");
    let precision: usize = std::env::var("MNF_HP8_P")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16);
    // optional row stride via MNF_HP8_STRIDE (default 8: ~5k rows of a 40k prefix)
    let stride: usize = std::env::var("MNF_HP8_STRIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let raw = std::fs::read(&vec_path).expect("vec read");
    let n = u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize;
    let all_rows: Vec<&[u8]> = raw[4..].chunks(256 * 4).take(n).collect();
    let syms_all = std::fs::read(&syms_path).expect("syms read");
    assert_eq!(all_rows.len(), syms_all.len(), "vec/sym count mismatch");
    let rows: Vec<&[u8]> = all_rows.iter().step_by(stride).copied().collect();
    let syms: Vec<u8> = syms_all.iter().step_by(stride).copied().collect();
    match precision {
        24 => run_p24(&rows, &syms),
        20 => run_p20(&rows, &syms),
        16 => run_p16(&rows, &syms),
        14 => run_p14(&rows, &syms),
        12 => run_p12(&rows, &syms),
        10 => run_p10(&rows, &syms),
        8 => run_p8(&rows, &syms),
        other => panic!("unsupported precision {other}"),
    }
}
