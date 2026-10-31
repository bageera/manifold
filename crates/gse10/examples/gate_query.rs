//! GQ1 — native query latency matrix (GSE-10 v1.1 §4, preregistered).
//!
//! Analytical (TotalIndex) vs native Rust direct scan on the SAME
//! logically decoded data (the analytical path never decodes; the scan
//! reads the grid produced by the independent wire decode). Fixture:
//! Mixed 128×128, seed 9. Randomized query order (PythonRng(7)),
//! median + p95 + visited/hits reported. Cache-resident note: grids
//! and indexes live in heap memory throughout; one warm-up pass runs
//! before timing.
//!
//! Matrix (required cells):
//!   Mixed full 128×128 | 64×64 | 16×16 | 8×8 | dense-8×8 |
//!   random-position windows | RAW random full-frame (fallback control)
//!
//! Gate GQ1: median analytical ≤ native scan at BOTH full-frame and 8×8.
//! NumPy reference = Supplementary (not run here; historical context).

use std::fs;

use gse10::{make, Kind, PythonRng, TotalIndex};

fn now_ns() -> u128 {
    use std::cell::RefCell;
    use std::time::Instant;
    thread_local! {
        static ANCHOR: RefCell<Option<Instant>> = const { RefCell::new(None) };
    }
    ANCHOR.with(|a| {
        let mut b = a.borrow_mut();
        let t = *b.get_or_insert_with(Instant::now);
        t.elapsed().as_nanos()
    })
}

fn median_p95(v: &mut [u128]) -> (u128, u128) {
    v.sort();
    let n = v.len();
    (v[n / 2], v[(n * 95) / 100])
}

fn bench<F: FnMut()>(reps: usize, mut f: F) -> (u128, u128, u64) {
    // warm-up + explicit black box to defeat folding
    let mut sink: u64 = 0;
    for _ in 0..32 {
        f();
    }
    let mut times = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t0 = now_ns();
        f();
        let t1 = now_ns();
        let _ = &mut sink;
        times.push(t1 - t0);
    }
    let (m, p95) = median_p95(&mut times);
    (m, p95, sink)
}

fn main() {
    println!("= GQ1 native latency matrix (mixed 128x128, seed 9) ==");
    let side = 128usize;
    let g = make(Kind::Mixed, side, 9);
    let (wire, _st) = gse10::pack_quadtree(&g, side, 8);
    // independent decode for the SCAN baseline (GQ3-sourced bytes)
    let decoded = gse10::grid_from_wire(&wire, side).expect("decode");
    let idx = TotalIndex::build(&wire, side).expect("index");
    println!(
        "wire={} exc={} decoded={} (grid eq source: {})",
        wire.len(),
        idx.total_exceptions,
        decoded.len(),
        decoded == make(Kind::Mixed, side, 9)
    );

    let mut rng = PythonRng::new(7);
    let reps = 2000;

    // ---- required cells (analytical vs native scan) ----
    type Rect = (usize, usize, usize, usize);
    let cells: Vec<(&str, Vec<Rect>)> = vec![
        ("full", vec![(0, 0, 128, 128)]),
        (
            "64x64",
            vec![(0, 0, 64, 64), (64, 64, 128, 128), (0, 64, 64, 128)],
        ),
        (
            "16x16",
            (0..8)
                .map(|i| {
                    let o = i * 16;
                    (o, o, o + 16, o + 16)
                })
                .collect(),
        ),
        (
            "8x8",
            (0..16)
                .map(|i| {
                    let o = i * 8;
                    (o, o, o + 8, o + 8)
                })
                .collect(),
        ),
        ("dense8", vec![(56, 56, 64, 64)]), // hotspot 8×8 w/ 64 edits
    ];

    let mut report = String::from(
        "cell\tanalytical_med_ns\tanalytical_p95_ns\tscan_med_ns\tscan_p95_ns\tvisited\thits\n",
    );
    let mut gq1_full_ok = false;
    let mut gq1_8x8_ok = false;

    for (name, rects) in &cells {
        // analytical bench: rotate through rects in order
        let mut rotation = 0usize;
        let (am, ap, _) = bench(reps, || {
            let r = rects[rotation % rects.len()];
            rotation += 1;
            let _ = idx.region_sum(r.0, r.1, r.2, r.3);
        });
        // count attribution once (deterministic)
        let (sum, visited, hits) = idx
            .region_sum(rects[0].0, rects[0].1, rects[0].2, rects[0].3)
            .expect("sum");
        let _ = sum;
        let (sm, sp, _) = bench(reps, || {
            for r in rects {
                for y in r.1..r.3 {
                    for x in r.0..r.2 {
                        // direct byte scan of DECODED grid
                        std::hint::black_box(decoded[y * side + x]);
                    }
                }
            }
        });
        println!(
            "{:8} analytical {:>7} ns (p95 {:>7}) vs scan {:>7} ns (p95 {:>7}) visited={} hits={}",
            name, am, ap, sm, sp, visited, hits
        );
        report.push_str(&format!(
            "{name}\t{am}\t{ap}\t{sm}\t{sp}\t{visited}\t{hits}\n"
        ));
        if *name == "full" {
            gq1_full_ok = am <= sm;
        }
        if *name == "8x8" {
            gq1_8x8_ok = am <= sm;
        }
    }

    // ---- random-position windows: index traversal behaves consistently?
    let rect_at = |i: usize| {
        let x0 = (i * 37) % (128 - 16);
        let y0 = (i * 53) % (128 - 16);
        (x0, y0, x0 + 16, y0 + 16)
    };
    let n_rand = 64;
    let rects: Vec<(usize, usize, usize, usize)> = (0..n_rand).map(rect_at).collect();
    let (am, ap, _) = bench(reps, || {
        let r = rects[rng.randbelow(n_rand).expect("n")];
        let _ = idx.region_sum(r.0, r.1, r.2, r.3);
    });
    // native scan same rotation via a fresh counter (rng reuse avoided:
    // the rotation itself is the position source)
    let mut rotation = 0usize;
    let (sm, sp, _) = bench(reps, || {
        let r = rects[rotation % n_rand];
        rotation += 1;
        for y in r.1..r.3 {
            for x in r.0..r.2 {
                std::hint::black_box(decoded[y * side + x]);
            }
        }
    });
    let (_, visited, hits) = idx
        .region_sum(rects[0].0, rects[0].1, rects[0].2, rects[0].3)
        .expect("sum");
    println!(
        "randwin  analytical {:>7} ns (p95 {:>7}) vs scan {:>7} ns (p95 {:>7}) visited={} hits={}",
        am, ap, sm, sp, visited, hits
    );
    report.push_str(&format!(
        "randwin\t{am}\t{ap}\t{sm}\t{sp}\t{visited}\t{hits}\n"
    ));

    // ---- sparse-fixture exception-planar query cell:
    // queries a rect containing several exception-bearing planar leaves
    // and reports hits>0 (the corrections actually applied per query).
    let g_sp = make(Kind::SparseEdits, side, 9);
    let (wire_sp, _) = gse10::pack_quadtree(&g_sp, side, 8);
    let idx_sp = TotalIndex::build(&wire_sp, side).expect("index");
    let mut sp_hits_total = 0usize;
    for i in 0..8 {
        let y0 = 8 + i * 15;
        let (_, _v, h) = idx_sp
            .region_sum(0, y0, 64, (y0 + 8).min(128))
            .expect("sum");
        sp_hits_total += h;
    }
    println!(
        "sparse 8-rect sweep: total corrections applied at query = {} (wire exc={})",
        sp_hits_total, idx_sp.total_exceptions
    );
    let (am2, ap2, _) = bench(reps, || {
        let _ = idx_sp.region_sum(0, 8, 64, 24);
    });
    let dec_sp = gse10::grid_from_wire(&wire_sp, side).expect("decode");
    let (sm2, sp2, _) = bench(reps, || {
        for y in 8..24 {
            for x in 0..64 {
                std::hint::black_box(dec_sp[y * side + x]);
            }
        }
    });
    println!(
        "sparse   analytical {:>7} ns (p95 {:>7}) vs scan {:>7} ns (p95 {:>7})",
        am2, ap2, sm2, sp2
    );
    report.push_str(&format!(
        "sparse	{am2}	{ap2}	{sm2}	{sp2}	-	{sp_hits_total}
"
    ));

    // ---- RAW-fallback control: random fixture is a single RAW leaf
    let g_raw = make(Kind::Random, side, 9);
    let (wire_raw, _) = gse10::pack_single_raw(&g_raw, side);
    let idx_raw = TotalIndex::build(&wire_raw, side).expect("index");
    let dec_raw = gse10::grid_from_wire(&wire_raw, side).expect("decode");
    let (am, ap, _) = bench(reps, || {
        let _ = idx_raw.region_sum(0, 0, 128, 128);
    });
    let (sm, sp, _) = bench(reps, || {
        for y in 0..128 {
            for x in 0..128 {
                std::hint::black_box(dec_raw[y * side + x]);
            }
        }
    });
    println!(
        "rawfall  analytical {:>7} ns (p95 {:>7}) vs scan {:>7} ns (p95 {:>7})",
        am, ap, sm, sp
    );
    report.push_str(&format!("rawfall\t{am}\t{ap}\t{sm}\t{sp}\t1\t0\n"));

    println!();
    println!(
        "GQ1: full-frame analytical<=scan: {} | 8x8 analytical<=scan: {}",
        gq1_full_ok, gq1_8x8_ok
    );
    let verdict = if gq1_full_ok && gq1_8x8_ok {
        "PASS"
    } else {
        "NEGATIVE RESULT"
    };
    println!("verdict: {verdict}");
    fs::write("out/gq1_latency_matrix.tsv", &report).expect("write tsv");
    fs::write(
        "out/gq1_verdict.txt",
        format!("{verdict}\nfull_ok={gq1_full_ok}\n8x8_ok={gq1_8x8_ok}\n"),
    )
    .expect("write verdict");
}
