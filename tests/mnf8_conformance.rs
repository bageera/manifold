// MNF8 conformance: golden vector decode + rejection vectors + corpus
// roundtrips through both profiles, mirroring mnf6_conformance.rs.
use manifold::codec::Dims;
use manifold::mnf8::{decode_mnf8, encode_mnf8, Profile, FLAG_TOKENS};

fn load(name: &str) -> Vec<u8> {
    let base = env!("CARGO_MANIFEST_DIR");
    std::fs::read(format!("{base}/tests/conformance/{name}")).expect(name)
}

#[test]
fn mnf8_golden_vector_exact() {
    let buf = load("mnf8_gradient_64x64.mnf8");
    let expect = load("mnf8_gradient_64x64.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("golden vector decode");
    assert_eq!((dims.width, dims.height), (64, 64));
    assert_eq!(px, expect, "MNF8 golden pixels");
    assert_eq!(backend, 1, "balanced profile pins zstd-19");
}

#[test]
fn mnf8_tokens_golden_vector_exact() {
    // ADR-003: token-wire payload pinned byte-exact; the file's flags word
    // must carry FLAG_TOKENS and decode must reconstruct exactly through
    // token walk + causal MED inversion.
    let buf = load("mnf8_tokens_gradient_64x64.mnf8");
    let expect = load("mnf8_tokens_gradient_64x64.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("tokens golden decode");
    assert_eq!((dims.width, dims.height), (64, 64));
    assert_eq!(px, expect, "MNF8 tokens golden pixels");
    assert_eq!(backend, 1, "tokens golden pins zstd-19");
    let flags = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    assert!(
        flags & FLAG_TOKENS != 0,
        "golden vector is a token-pick file"
    );
}

#[test]
fn mnf8_rejection_vectors_rejected_without_panic() {
    for name in [
        "rej_mnf8_unknown_backend.mnf8",
        "rej_mnf8_reserved_backend.mnf8",
        "rej_mnf8_reserved_flag.mnf8",
        "rej_mnf8_tokens_planar.mnf8",
        "rej_mnf8_tokens_flag.mnf8",
        "rej_mnf8_bad_magic.mnf8",
    ] {
        let buf = load(name);
        let r = std::panic::catch_unwind(|| decode_mnf8(&buf));
        assert!(
            r.is_ok() && r.unwrap().is_err(),
            "{name}: must reject without panic"
        );
    }
}

#[test]
fn mnf8_corpus_roundtrips_both_profiles() {
    let base = env!("CARGO_MANIFEST_DIR");
    for name in [
        "screenshot_640x400.png",
        "solid_256x256.png",
        "gradient_320x240.png",
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
        for profile in [Profile::Balanced, Profile::Maximum] {
            let (buf, _, backend) = encode_mnf8(&px, d, true, profile);
            let (rd, rpx, _) =
                decode_mnf8(&buf).unwrap_or_else(|e| panic!("{name} {profile:?}: {e}"));
            assert_eq!(rpx, px, "{name} {profile:?} backend {backend}");
            assert_eq!((rd.width, rd.height), (d.width, d.height));
        }
    }
}
