//! GSE-02 acceptance gate: the eight-dataset acceptance matrix +
//! three-source separation (generator/partition/shared advantage),
//! complete serialized costs, bare zstd-19 comparator with the SAME
//! framing counted.
use gse02::*;

fn z19(b: &[u8]) -> usize {
    zstd::stream::encode_all(b, 19).unwrap().len()
}

fn gse_framed_zstd(d: &[u8]) -> usize {
    // A-arm: the SAME 12-B header framing + zstd
    12 + z19(d)
}

fn main() {
    let sets: Vec<(&str, Vec<u8>, &str)> = vec![
        ("arithmetic", fixtures::arith(65_536), "generator: whole-stream arith"),
        ("constant", fixtures::constant(65_536), "generator: whole-stream const"),
        ("acgt", fixtures::acgt(65_536), "generator: periodic k=4"),
        ("random", fixtures::random(65_536, 42), "fallback: raw"),
        ("arith+random", fixtures::mixed(65_536), "partition: 2 regions"),
        ("alternating", fixtures::alternating(65_536), "adaptive segmentation"),
        ("shared-rec", fixtures::shared_rec(65_536), "shared model table"),
        ("near-arith", fixtures::noisy_arith(65_536, 5), "residual generator"),
    ];
    println!("{:14} {:>10} {:>10} {:>8}  expected behavior",
             "dataset", "A zstd", "GSE-02", "delta");
    let mut gse_wins = 0;
    for (name, d, expect) in &sets {
        let enc = encode(d);
        let a = gse_framed_zstd(d);
        let delta = enc.len() as i64 - a as i64;
        if delta < 0 {
            gse_wins += 1;
        }
        println!(
            "{:14} {:>10} {:>10} {:>8}  {}",
            name,
            a,
            enc.len(),
            format!("{delta:+}"),
            expect
        );
    }
    println!("\nGSE beats complete-framed zstd on {gse_wins}/8");
}