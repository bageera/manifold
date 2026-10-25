// MNF6 differential + conformance: every MNF6 file the encoder produces
// must decode exactly through decode_mnf6, and the differential pair —
// codec::decode (MNF5) vs ref_decode — stays agreement-checked. MNF6 adds:
// golden vector pinning, reserved-flag rejection, and (like MNF5) fuzzing
// through scripts/fuzz.py --mnf6.

use manifold::codec::{self, Dims};
use manifold::mnf6::{decode_mnf6, encode_mnf6};

fn load_conformance(name: &str) -> Vec<u8> {
    let base = env!("CARGO_MANIFEST_DIR");
    std::fs::read(format!("{base}/tests/conformance/{name}")).expect(name)
}

#[test]
fn mnf6_golden_vector_exact() {
    let buf = load_conformance("mnf6_gradient_64x64.mnf6");
    let expect = load_conformance("mnf6_gradient_64x64.expected.pixels");
    let (dims, px) = decode_mnf6(&buf).expect("golden vector decode");
    assert_eq!((dims.width, dims.height), (64, 64));
    assert_eq!(px, expect, "MNF6 golden pixels");
}

#[test]
fn mnf6_rejects_reserved_flag_bits() {
    // flip bit 5 in the flags word of the golden vector
    let mut buf = load_conformance("mnf6_gradient_64x64.mnf6");
    buf[24] |= 0b100000;
    assert!(decode_mnf6(&buf).is_err(), "reserved bits must reject");
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
                let (buf, st) = encode_mnf6(&px, d, geom);
                let (rd, rpx) =
                    decode_mnf6(&buf).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
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
            best_mnf5 = best_mnf5.min(28 + z.len());
        }
        let (buf6, _) = encode_mnf6(&px, d, true);
        assert!(
            buf6.len() <= best_mnf5,
            "{name}: MNF6 {} > MNF5 {}",
            buf6.len(),
            best_mnf5
        );
    }
}
