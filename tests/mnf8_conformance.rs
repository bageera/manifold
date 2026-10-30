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
fn mnf8_branch_golden_vector_exact() {
    // ADR-004: LEFT-predictor golden vector (the measured left-winner file);
    // flags must carry FLAG_PRED_LEFT, decode exact via the ADR-004 walk.
    use manifold::mnf8::FLAG_PRED_LEFT;
    let buf = load("mnf8_branch_left_poster.mnf8");
    let expect = load("mnf8_branch_left_poster.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("branch golden decode");
    assert_eq!(dims.width as usize, expect.len() / 3 / dims.height as usize);
    assert_eq!(px, expect, "MNF8 branch golden pixels");
    assert_eq!(backend, 1);
    let flags = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    assert!(
        flags & FLAG_PRED_LEFT != 0,
        "golden vector is a LEFT-pick file"
    );
}

#[test]
fn mnf8_branch_rejections() {
    // ADR-004 validation vectors: two predictor bits / tokens+left — each
    // Err without panic.
    for name in ["rej_mnf8_two_predictors.mnf8", "rej_mnf8_tokens_left.mnf8"] {
        let buf = load(name);
        let r = std::panic::catch_unwind(|| decode_mnf8(&buf));
        assert!(r.is_ok() && r.unwrap().is_err(), "{name}: must reject");
    }
}

#[test]
fn mnf8_cdiff_golden_vector_exact() {
    // ADR-008: channel-diff golden vector (slate photo crop, PLANAR|PLANE|CD
    // pick). Mode-0 bodies carry [G, R−G, B−G] per pixel; decode = the
    // inverse relabel + the ADR-004 walk; must reconstruct exactly.
    use manifold::mnf6::FLAG_PLANAR;
    use manifold::mnf8::{FLAG_CHANNEL_DIFF, FLAG_PRED_PLANE};
    let buf = load("mnf8_cdiff_slate64.mnf8");
    let expect = load("mnf8_cdiff_slate64.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("cdiff golden decode");
    assert_eq!((dims.width, dims.height), (64, 64));
    assert_eq!(px, expect, "MNF8 cdiff golden pixels");
    assert_eq!(backend, 1);
    let flags = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    assert!(
        flags & FLAG_CHANNEL_DIFF != 0 && flags & FLAG_PLANAR != 0 && flags & FLAG_PRED_PLANE != 0,
        "golden vector is a CD|PLANAR|PLANE pick file: 0x{flags:x}"
    );
}

#[test]
fn mnf8_mix_golden_vector_exact() {
    // MNF13-R5 (v1.13.0): backend 4 = the dual-timescale mix coder.
    // Golden = document 200x280, PLANAR|REF=1 record arm, mix-coded
    // payload ([u32 stream_len][LE words]).
    use manifold::mix_coder;
    use manifold::mnf8::BACKEND_MIX;
    use manifold::mnf8::FLAG_PRED_LEFT;
    let buf = load("mnf8_mix_document.mnf8");
    let expect = load("mnf8_mix_document.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("mix golden decode");
    assert_eq!((dims.width, dims.height), (200, 280));
    assert_eq!(px, expect, "MNF8 mix golden pixels");
    assert_eq!(backend, BACKEND_MIX);
    let flags = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    assert!(
        flags & FLAG_PRED_LEFT != 0 && (flags >> 6) & 3 == 3,
        "golden vector is the LEFT|REF=3|mix pick file: 0x{flags:x}"
    );
    // the mix payload's stream length word must equal the record
    // stream's real length (the walk consumes it exactly; 234 mode-0
    // tiles: Σ(1 + 3·tw·th) for 200x280):
    assert_eq!(
        usize::try_from(u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]])).unwrap(),
        168_234usize,
    );
    let _ = mix_coder::mix_ctx_bucket(0u8); // surface the module
}

#[test]
fn mnf8_mix5_golden_vector_exact() {
    // MNF13-R6 (v1.14.0): backend 5 = the R6 mix coder (16-key context,
    // long-horizon caps). Golden = document 200x280 at the CURRENT pick
    // winner LEFT|REF(=3)|mix5 (flags 0xe0) — generated by the release
    // CLI at the measured pick; assert the flags word names the arms.
    use manifold::mix_coder;
    use manifold::mnf8::BACKEND_MIX5;
    use manifold::mnf8::FLAG_PRED_LEFT;
    let buf = load("mnf8_mix5_document.mnf8");
    let expect = load("mnf8_mix5_document.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("mix5 golden decode");
    assert_eq!((dims.width, dims.height), (200, 280));
    assert_eq!(px, expect, "MNF8 mix5 golden pixels");
    assert_eq!(backend, BACKEND_MIX5);
    let flags = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    assert!(
        flags & FLAG_PRED_LEFT != 0 && (flags >> 6) & 3 == 3,
        "golden vector is the LEFT|REF=3|mix5 pick file: 0x{flags:x}"
    );
    // the mix5 payload's stream length word = the record stream's real
    // length (168,234 = 234 mode-0 tiles: Σ(1 + 3·tw·th) for 200x280)
    assert_eq!(
        usize::try_from(u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]])).unwrap(),
        168_234usize,
    );
    let _ = mix_coder::MIX5_NCTX; // surface the module
}

#[test]
fn mnf8_mix5_rejects() {
    // backend-5 malformed: not-word-aligned payload + bomb stream_len
    // (crafted from the golden by mutation, decoder must Err not panic)
    let mut buf = load("mnf8_mix5_document.mnf8");
    buf[32..36].copy_from_slice(&u32::MAX.to_le_bytes()); // bomb stream_len
    assert!(decode_mnf8(&buf).is_err());
    let mut trunc = load("mnf8_mix5_document.mnf8");
    let n = trunc.len();
    trunc.truncate(n - 3); // unaligned tail
    assert!(decode_mnf8(&trunc).is_err());
}

#[test]
fn mnf8_mix_rejections() {
    // mix payload malformed: unaligned / stream_len bomb — Err, no panic.
    for name in ["rej_mnf8_mix_unaligned.mnf8", "rej_mnf8_mix_bomb.mnf8"] {
        let buf = load(name);
        let r = std::panic::catch_unwind(|| decode_mnf8(&buf));
        assert!(r.is_ok() && r.unwrap().is_err(), "{name}: must reject");
    }
}

#[test]
fn mnf8_ref2_golden_vector_exact() {
    // v1.12 REF field = 2 (R-centered): token wire, tag-0 literals carry
    // [R, G−R, B−R] per pixel.
    use manifold::mnf8::FLAG_REF_R;
    let buf = load("mnf8_ref2_screenshot.mnf8");
    let expect = load("mnf8_ref2_screenshot.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("ref2 golden decode");
    assert_eq!((dims.width, dims.height), (640, 400));
    assert_eq!(px, expect, "MNF8 ref2 golden pixels");
    assert_eq!(backend, 1);
    let flags = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    assert!(
        flags & FLAG_REF_R != 0 && (flags >> 6) & 3 == 2,
        "golden vector is a REF=2 pick file: 0x{flags:x}"
    );
    // The old 1.11.0 reserved mask (!0b1111111) must reject this file —
    // decode with the v1.11 rule:
    assert!(
        flags & !0b1111_1111u32 != 0 || flags & 0b1000_0000u32 != 0,
        "sanity: ref2 file trips the 1.11.0 reserved rule"
    );
}

#[test]
fn mnf8_ref3_golden_vector_exact() {
    // v1.12 REF field = 3 (B-centered): the wire's [B, R−B, G−B].
    let buf = load("mnf8_ref3_terrain160.mnf8");
    let expect = load("mnf8_ref3_terrain160.expected.pixels");
    let (dims, px, backend) = decode_mnf8(&buf).expect("ref3 golden decode");
    assert_eq!((dims.width, dims.height), (160, 120));
    assert_eq!(px, expect, "MNF8 ref3 golden pixels");
    assert_eq!(backend, 1);
    let flags = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    assert_eq!(
        (flags >> 6) & 3,
        3,
        "golden vector is a REF=3 pick file: 0x{flags:x}"
    );
}

#[test]
fn mnf8_cdiff_rejections() {
    // ADR-008 validation vectors: reserved bit 7 / truncated CD payload —
    // each Err without panic.
    for name in [
        "rej_mnf8_reserved_bit8.mnf8",
        "rej_mnf8_cdiff_truncated.mnf8",
    ] {
        let buf = load(name);
        let r = std::panic::catch_unwind(|| decode_mnf8(&buf));
        assert!(r.is_ok() && r.unwrap().is_err(), "{name}: must reject");
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
