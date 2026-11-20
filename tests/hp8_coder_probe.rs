// HP8 differential probe: constriction's own coder vs the Python port.
//   MNF_HP8_CODER_PROBE=1 cargo test --release hp8_coder_probe -- --ignored --nocapture
use constriction::stream::queue::DefaultRangeEncoder;
use constriction::stream::Encode;

macro_rules! probe_precision {
    ($precision:expr, $n:expr) => {{
        let precision: u32 = $precision;
        let total = 1u32 << $precision;
        let mut counts = [0u32; 256];
        for (i, c) in counts.iter_mut().enumerate() {
            *c = 1 + ((i * 7 + 13) % 31) as u32;
        }
        let sum: u32 = counts.iter().sum();
        let mut probs = [0u32; 256];
        let mut rem: Vec<u64> = Vec::with_capacity(256);
        let mut acc: u64 = 0;
        for (i, &c) in counts.iter().enumerate() {
            let v = (c as u64 * total as u64) / sum as u64;
            probs[i] = v as u32;
            rem.push(c as u64 * total as u64 - v * sum as u64);
            acc += v as u64;
        }
        let mut grants = total as u64 - acc;
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
        assert_eq!(probs.iter().sum::<u32>(), total);
        let model = build_model::<$precision>(&probs);
        let symbols: Vec<usize> = (0..$n)
            .map(|i| ((i * 97 + 41) * (i < 300) as usize + (i * 131 + 7)) % 256)
            .collect();
        let mut enc = DefaultRangeEncoder::new();
        enc.encode_symbols(symbols.iter().copied().zip(std::iter::repeat(&model)))
            .unwrap();
        let words = enc.into_compressed().unwrap();
        let mut bytes = Vec::new();
        for w in words.iter() {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        println!(
            "HP8PROBE precision={precision} total_bytelen={}",
            bytes.len()
        );
        println!(
            "HP8PROBE precision={precision} hex={}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        println!("HP8PROBE probs={probs:?}");
    }};
}

#[test]
#[ignore]
fn hp8_coder_probe() {
    let n_usize: usize = std::env::var("MNF_HP8_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    if std::env::var("MNF_HP8_CODER_PROBE").is_err() {
        return;
    }
    probe_precision!(24, n_usize);
    probe_precision!(16, n_usize);
    probe_precision!(8, n_usize);
}

fn build_model<const P: usize>(
    probs: &[u32; 256],
) -> constriction::stream::model::ContiguousCategoricalEntropyModel<u32, Vec<u32>, P> {
    constriction::stream::model::ContiguousCategoricalEntropyModel::<
        u32,
        Vec<u32>,
        P,
    >::from_nonzero_fixed_point_probabilities(probs.to_vec(), false)
    .unwrap()
}
