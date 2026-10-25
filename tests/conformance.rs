// MNF5 format conformance (README §6: format conformance vectors).
// Golden vectors decode to expected RGB8; rejection vectors must fail
// without panicking. Vectors are hand-built per docs/format.md.

use codec::{decode, Dims};
use manifold::codec;

fn load(p: &str) -> Vec<u8> {
    let base = env!("CARGO_MANIFEST_DIR");
    std::fs::read(format!("{base}/tests/conformance/{p}")).expect(p)
}

#[test]
fn golden_vectors_decode_exactly() {
    for name in ["solid_32x16", "geom_rot180_16x32"] {
        let buf = load(&format!("{name}.mnf5"));
        let expect = load(&format!("{name}.expected.pixels"));
        let res = decode(&buf).unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
        assert_eq!(res.pixels, expect, "{name}: pixels differ");
        let (w, h) = match name {
            "solid_32x16" => (32, 16),
            _ => (16, 32),
        };
        assert_eq!(
            res.dims,
            Dims {
                width: w,
                height: h
            },
            "{name}: dims"
        );
    }
}

#[test]
fn rejection_vectors_rejected_without_panic() {
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
        // A panic here (unwrap on Err) is the failure signal — any Ok is a bug.
        let buf = load(name);
        assert!(
            decode(&buf).is_err(),
            "{name}: structurally invalid vector was ACCEPTED"
        );
    }
}

// Property-based roundtrips (README §6: property-based testing) —
// deterministic xorshift so failures reproduce.
#[test]
fn property_random_image_roundtrip_seed_1() {
    let mut s: u64 = 1;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    for _ in 0..8 {
        let w = 1 + (rnd() % 64) as u32;
        let h = 1 + (rnd() % 64) as u32;
        let px: Vec<u8> = (0..w as usize * h as usize * 3)
            .map(|_| (rnd() & 0xFF) as u8)
            .collect();
        let d = Dims {
            width: w,
            height: h,
        };
        let tiles = codec::split_tiles(&px, d);
        for geom in [false, true] {
            let raw = codec::encode_tiles(&tiles, true, geom);
            let payload = codec::compress_payload(&raw);
            let mut buf = codec::build_header(d, tiles.len() as u32, payload.len() as u32);
            buf.extend_from_slice(&payload);
            let res = decode(&buf).unwrap_or_else(|e| panic!("roundtrip {w}x{h} geom={geom}: {e}"));
            assert_eq!(res.pixels, px, "roundtrip {w}x{h} geom={geom}");
        }
    }
}
