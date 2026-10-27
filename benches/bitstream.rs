//! Bitstream-cost benches — the reproducible size numbers behind
//! docs/benchmarks.md (losses included, per the honesty contract).
//!
//! Byte costs are DETERMINISTIC (variant search is data-driven, no timing
//! dependence), so each cost row is: N deterministic encode repeats (sample_size(10),
//! identical byte counts confirm determinism), then the decode asserts
//! exactness. Decode THROUGHPUT is genuinely stochastic → true criterion
//! timing (20 samples, pixel throughput).
//!
//! Run: cargo bench -p manifold-codec

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use manifold::codec::Dims;
use manifold::mnf8::{decode_mnf8, encode_mnf8, Profile};
use std::hint::black_box;

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

fn bench_class(c: &mut Criterion, name: &str) {
    let (dims, px) = load(name);
    let mut g = c.benchmark_group(format!("size/{name}"));
    g.finish();

    // correctness inside the bench: decode of the shipped bytes is exact
    let (buf, _, _) = encode_mnf8(&px, dims, true, Profile::Balanced);
    let (_, out, _) = decode_mnf8(&buf).expect("decode");
    assert_eq!(out, px, "{name}: bench encode roundtrip exact");
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
    for name in [
        "solid_256x256.png",
        "gradient_320x240.png",
        "screenshot_640x400.png",
    ] {
        bench_class(c, name);
        bench_decode_throughput(c, name);
    }
    for name in [
        "photos/ocean_192x144_s900007.png",
        "heldout-photos/autumn_160x120_s3100006.png",
    ] {
        bench_class(c, name);
    }
}

criterion_group!(benches, corpora);
criterion_main!(benches);
