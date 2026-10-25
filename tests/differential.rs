// Differential testing: the primary decoder and the independent reference
// decoder must agree everywhere — same accept/reject verdicts, identical
// pixels on acceptance — across golden vectors, rejection vectors, the full
// corpus, and all three encoding arms (README §6: independent implementation).

use manifold::codec::{self, decode, Dims};
use manifold::refdecode::ref_decode;

fn load_conformance(name: &str) -> Vec<u8> {
    let base = env!("CARGO_MANIFEST_DIR");
    std::fs::read(format!("{base}/tests/conformance/{name}")).expect(name)
}

fn encode_arm(px: &[u8], d: Dims, atlas: bool, geom: bool) -> Vec<u8> {
    let tiles = codec::split_tiles(px, d);
    let raw = codec::encode_tiles(&tiles, atlas, geom);
    let payload = codec::compress_payload(&raw);
    let mut buf = codec::build_header(d, tiles.len() as u32, payload.len() as u32);
    buf.extend_from_slice(&payload);
    buf
}

#[test]
fn decoders_agree_on_golden_vectors() {
    for name in ["solid_32x16", "geom_rot180_16x32"] {
        let buf = load_conformance(&format!("{name}.mnf5"));
        let expect = load_conformance(&format!("{name}.expected.pixels"));
        let a = decode(&buf).unwrap_or_else(|e| panic!("{name}: primary: {e}"));
        let b = ref_decode(&buf).unwrap_or_else(|e| panic!("{name}: reference: {e}"));
        assert_eq!(a.pixels, expect, "{name}: primary pixels");
        assert_eq!(b.pixels, expect, "{name}: reference pixels");
        assert_eq!(a.dims, b.dims);
    }
}

#[test]
fn decoders_agree_on_rejections() {
    let cases = [
        "rej_bad_magic.mnf5",
        "rej_zero_dims.mnf5",
        "rej_bad_tilesize.mnf5",
        "rej_bad_tilecount.mnf5",
        "rej_dangling_ref.mnf5",
        "rej_shape_mismatch.mnf5",
        "rej_transform_nonsquare.mnf5",
        "rej_transform_range.mnf5",
        "rej_mode3.mnf5",
        "rej_truncated.mnf5",
        "rej_trailing.mnf5",
        "rej_oversized_dims.mnf5",
        "rej_short_header.mnf5",
        "rej_bad_zlib.mnf5",
    ];
    for name in cases {
        let buf = load_conformance(name);
        let a = decode(&buf);
        let b = ref_decode(&buf);
        assert!(a.is_err(), "{name}: primary accepted invalid stream");
        assert!(b.is_err(), "{name}: reference accepted invalid stream");
    }
}

#[test]
fn decoders_agree_on_corpus_across_arms() {
    let base = env!("CARGO_MANIFEST_DIR");
    let td = format!("{base}/testdata");
    let mut checked = 0;
    for entry in std::fs::read_dir(&td).expect("testdata") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        let img = image::open(&path).expect("open").to_rgb8();
        let d = Dims {
            width: img.width(),
            height: img.height(),
        };
        let px = img.into_raw();
        for (atlas, geom) in [(false, false), (true, false), (true, true)] {
            let buf = encode_arm(&px, d, atlas, geom);
            let a = decode(&buf).unwrap_or_else(|e| {
                panic!(
                    "{} arm {}x{} a={atlas} g={geom}: {e}",
                    path.display(),
                    d.width,
                    d.height
                )
            });
            let b = ref_decode(&buf).unwrap_or_else(|e| {
                panic!(
                    "{} arm {}x{} a={atlas} g={geom}: ref: {e}",
                    path.display(),
                    d.width,
                    d.height
                )
            });
            assert_eq!(a.pixels, px, "{}: primary != source", path.display());
            assert_eq!(b.pixels, a.pixels, "{}: decoders disagree", path.display());
            assert_eq!(a.dims, b.dims);
            checked += 1;
        }
    }
    assert!(
        checked >= 15,
        "expected at least 5 images x 3 arms, got {checked}"
    );
}
