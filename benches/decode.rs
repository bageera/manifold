//! Decode-throughput benches (criterion). The bitstream SIZE rows live in
//! scripts/report_sizes.py — byte costs are deterministic, so they belong in
//! a report script, not a statistical harness (documented there).
//!
//! Run: cargo bench -p manifold-codec

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use manifold::codec::Dims;
use manifold::mnf8::{decode_mnf8, encode_mnf8, Profile};

fn load(name: &str) -> (Dims, Vec<u8>) {
    let path = format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"));
    let img = image::open(&path).expect(name).to_rgb8();
    (
        Dims {
            width: img.width(),
            height: img.height(),
        },
        img.into_raw(),
    )
}

fn bench_decode_throughput(c: &mut Criterion, name: &str) {
    let (dims, px) = load(name);
    let (buf, _, _) = encode_mnf8(&px, dims, true, Profile::Balanced);
    let mut g = c.benchmark_group(format!("decode/{name}"));
    g.sample_size(20);
    g.throughput(Throughput::Elements(
        (dims.width as u64) * (dims.height as u64),
    ));
    g.bench_function("mnf8/balanced", |b| {
        b.iter(|| decode_mnf8(black_box(&buf)).expect("decode"))
    });
    g.finish();
}

fn corpora(c: &mut Criterion) {
    for name in ["solid_256x256.png", "gradient_320x240.png", "screenshot_640x400.png"] {
        bench_decode_throughput(c, name);
    }
}

criterion_group!(benches, corpora);
criterion_main!(benches);