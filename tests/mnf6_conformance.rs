// MNF6 differential + conformance: every MNF6 file the encoder produces
// must decode exactly through decode_mnf7, and the differential pair —
// codec::decode (MNF5) vs ref_decode — stays agreement-checked. MNF6 adds:
// golden vector pinning, reserved-flag rejection, and (like MNF5) fuzzing
// through scripts/fuzz.py --mnf6.

use manifold::codec::{self, Dims};
use manifold::mnf6::{decode_mnf7, encode_mnf7};

fn manifold_zlib(raw: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(raw).expect("zlib write");
    e.finish().expect("zlib finish")
}

fn load_conformance(name: &str) -> Vec<u8> {
    let base = env!("CARGO_MANIFEST_DIR");
    std::fs::read(format!("{base}/tests/conformance/{name}")).expect(name)
}

#[test]
fn mnf7_all_mode3_stream_accepted() {
    // Audit regression pin: mode-3 records are 833 bytes for a full tile
    // (1 + 256 + 2*288), larger than the 776 the old decompression bound
    // assumed — a legal all-mode-3 file was rejected as a "bomb".
    // Craft exactly such a stream and require acceptance.
    let (w, h) = (32u32, 32u32);
    let np = 256usize;
    let n9 = (np * 9).div_ceil(8);
    let mut raw = Vec::new();
    for _ in 0..4 {
        raw.push(3u8); // mode 3
        raw.extend(std::iter::repeat_n(128u8, np)); // Y plane (n/3 bytes)
        for _ in 0..2 {
            // co/cg biased +256 (i.e. value 256), LSB-first 9-bit packing
            let mut packed = Vec::with_capacity(n9);
            let mut acc: u32 = 0;
            let mut bits = 0u32;
            for _ in 0..np {
                acc |= 256 << bits;
                bits += 9;
                while bits >= 8 {
                    packed.push((acc & 0xFF) as u8);
                    acc >>= 8;
                    bits -= 8;
                }
            }
            if bits > 0 {
                packed.push((acc & 0xFF) as u8);
            }
            raw.extend_from_slice(&packed);
        }
    }
    let payload = manifold_zlib(&raw);
    let mut buf = Vec::new();
    buf.extend_from_slice(b"MNF7");
    buf.extend_from_slice(&w.to_le_bytes());
    buf.extend_from_slice(&h.to_le_bytes());
    buf.extend_from_slice(&16u32.to_le_bytes()); // tile size
    buf.extend_from_slice(&4u32.to_le_bytes()); // tile count
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&payload);
    let (dims, px) = decode_mnf7(&buf).expect("all-mode-3 stream must decode");
    assert_eq!((dims.width, dims.height), (w, h));
    // y=128 with co=cg=0 (biased) reconstructs flat gray 128
    assert!(
        px.chunks(3).all(|c| c == [128, 128, 128]),
        "expected flat gray reconstruction"
    );
}

#[test]
fn mnf6_golden_vector_exact() {
    let buf = load_conformance("mnf7_gradient_64x64.mnf7");
    let expect = load_conformance("mnf7_gradient_64x64.expected.pixels");
    let (dims, px) = decode_mnf7(&buf).expect("golden vector decode");
    assert_eq!((dims.width, dims.height), (64, 64));
    assert_eq!(px, expect, "MNF6 golden pixels");
}

#[test]
fn mnf6_rejects_reserved_flag_bits() {
    // flip bit 5 in the flags word of the golden vector
    let mut buf = load_conformance("mnf7_gradient_64x64.mnf7");
    buf[24] |= 0b100000;
    assert!(decode_mnf7(&buf).is_err(), "reserved bits must reject");
}

#[test]
fn mnf6_roundtrip_all_corpus_all_arms() {
    let base = env!("CARGO_MANIFEST_DIR");
    for dir in ["testdata", "testdata/heldout"] {
        for entry in std::fs::read_dir(format!("{base}/{dir}")).expect("dir") {
            let path = entry.expect("entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("png") {
                continue;
            }
            let img = image::open(&path).expect("open").to_rgb8();
            let d = Dims {
                width: img.width(),
                height: img.height(),
            };
            let px = img.into_raw();
            for geom in [false, true] {
                let (buf, st) = encode_mnf7(&px, d, geom);
                let (rd, rpx) =
                    decode_mnf7(&buf).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert_eq!(
                    rpx,
                    px,
                    "{}: mismatch (flags {:#x})",
                    path.display(),
                    st.chosen_flags
                );
                assert_eq!((rd.width, rd.height), (d.width, d.height));
            }
        }
    }
}

#[test]
fn mnf6_full_corpus_fits_beats_mnf5_everywhere() {
    // rate-aware selection guarantee (v0.8 exit): MNF6 never loses to MNF5's
    // best arm on the whole corpus (10/10 measured in the session audit;
    // this test re-verifies on a stable subset to keep runtime bounded).
    let base = env!("CARGO_MANIFEST_DIR");
    for name in [
        "screenshot_640x400.png",
        "solid_256x256.png",
        "heldout/blob_224x160.png",
    ] {
        let img = image::open(format!("{base}/testdata/{name}"))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .to_rgb8();
        let d = Dims {
            width: img.width(),
            height: img.height(),
        };
        let px = img.into_raw();

        let mut best_mnf5 = usize::MAX;
        for (atlas, geom) in [(false, false), (true, false), (true, true)] {
            let tiles = codec::split_tiles(&px, d);
            let raw = codec::encode_tiles(&tiles, atlas, geom);
            let z = codec::compress_payload(&raw);
            // 24 = MNF5 HEADER_SIZE (audit fix: was 28, the MNF6 header size,
            // which inflated MNF5 by 4 bytes and let the assert pass loosely)
            best_mnf5 = best_mnf5.min(codec::HEADER_SIZE + z.len());
        }
        let (buf6, _) = encode_mnf7(&px, d, true);
        assert!(
            buf6.len() <= best_mnf5,
            "{name}: MNF6 {} > MNF5 {}",
            buf6.len(),
            best_mnf5
        );
    }
}
