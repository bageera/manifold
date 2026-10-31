//! GQ2 + GQ4 — storage accounting matrix (GSE-10 v1.1, preregistered).
//!
//! Portfolio = { quadtree-greedy (with sparse planar arm), single-planar,
//! single-const, single-raw } — min() at PRE-CONTAINER wire bytes, then
//! one zstd-19 container pass over the winning wire (complete serialized
//! cost). Standalone per-fixture rows include LOSSES (a rejected arm's
//! regression is reported, never masked by the portfolio).
//!
//! Gate GQ2: every per-fixture standalone row reports the portfolio
//! delta vs the shipping comparator (zstd-19 on raw bytes, same CLI);
//! portfolio min() must never lose outright.
//! Gate GQ4: PackStats decomposition must equal wire length (asserted
//! in pack_quadtree tests) — charged here as the L_parts row.
//!
//! Container: `zstd --ultra -19` CLI (parity with the Python harness).

use std::fs;
use std::process::Command;

use gse10::{make, Kind, KIND_NAMES};

const KINDS: [Kind; 7] = [
    Kind::Constant,
    Kind::Mathematical,
    Kind::SparseEdits,
    Kind::Mixed,
    Kind::SmoothNoisy,
    Kind::Random,
    Kind::DenseHotspot(64),
];

fn zstd19(data: &[u8]) -> Vec<u8> {
    let tmp = std::env::temp_dir().join(format!("gse10z{}", now_ns() as u64));
    fs::write(&tmp, data).expect("write tmp");
    let out = tmp.with_extension("zst");
    let st = Command::new("/opt/homebrew/bin/zstd")
        .args([
            "--ultra",
            "-19",
            "-f",
            tmp.to_str().expect("p"),
            "-o",
            out.to_str().expect("p"),
        ])
        .output()
        .expect("zstd run");
    assert!(st.status.success(), "zstd failed: {:?}", st.stderr);
    let z = fs::read(&out).expect("read zst");
    let _ = fs::remove_file(&tmp);
    let _ = fs::remove_file(&out);
    z
}

fn now_ns() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

fn main() {
    println!("= GQ2/GQ4 storage matrix (seed 9, both sizes) ==");
    let mut report = String::from(
        "kind\tside\traw\tzstd19\twire_best\tarm\tcontainer\tstandalone_delta_vs_zstd\tquad_wire\tquad_container\tloss_vs_zstd\n",
    );
    let mut failures = 0usize;

    for side in [128usize, 256usize] {
        for (ki, kind) in KINDS.iter().enumerate() {
            let g = make(*kind, side, 9);
            let raw = g.len();
            let zstd = zstd19(&g).len();

            // arms (complete pre-container wire)
            let (qwire, qst) = gse10::pack_quadtree(&g, side, 8);
            debug_assert_eq!(qwire.len(), qst.accounted(), "GQ4 drift");
            let singles: Vec<(&str, Vec<u8>)> = {
                let mut v = Vec::new();
                if let Some((w, _)) = gse10::pack_single_planar(&g, side) {
                    v.push(("planar", w));
                }
                if let Some((w, _)) = gse10::pack_single_const(&g, side) {
                    v.push(("const", w));
                }
                v.push(("raw", gse10::pack_single_raw(&g, side).0));
                v.push(("quad", qwire.clone()));
                v
            };
            let (arm, wwire) = singles
                .iter()
                .min_by_key(|(_, w)| w.len())
                .map(|(a, w)| (*a, w.clone()))
                .expect("arms");
            let container = zstd19(&wwire).len();
            let quad_container = zstd19(&qwire).len();
            let delta = container as i64 - zstd as i64;
            let quad_delta = quad_container as i64 - zstd as i64;
            let loss = delta > 0;
            if loss {
                failures += 1;
            }
            let name = KIND_NAMES[ki];
            println!(
                "{:14} {:>3}  raw={:>6} zstd={:>7} best={}({:>6}->{} {}) quad={:>6}->{} {}",
                name,
                side,
                raw,
                zstd,
                arm,
                wwire.len(),
                container,
                fmt_pct(delta, zstd),
                qwire.len(),
                quad_container,
                fmt_pct(quad_delta, zstd),
            );
            report.push_str(&format!(
                "{name}\t{side}\t{raw}\t{zstd}\t{}\t{arm}\t{container}\t{delta}\t{}\t{quad_container}\t{quad_delta}\n",
                wwire.len(),
                qwire.len(),
            ));
        }
    }

    println!();
    println!("portfolio outright losses: {failures} / 14 cells");
    let verdict = if failures == 0 {
        "PASS"
    } else {
        "NEGATIVE RESULT"
    };
    println!("verdict: {verdict}");
    fs::create_dir_all("out").expect("out dir");
    fs::write("out/gq2_storage_matrix.tsv", &report).expect("write tsv");
    fs::write(
        "out/gq2_verdict.txt",
        format!("{verdict}\nfailures={failures}\n"),
    )
    .expect("write verdict");
}

fn fmt_pct(delta: i64, base: usize) -> String {
    format!("{:+.1}%", delta as f64 / base as f64 * 100.0)
}
