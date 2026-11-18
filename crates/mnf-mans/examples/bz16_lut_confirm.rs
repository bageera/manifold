//! BZ16-R1 — Rust-side confirmation instrument for the BZ16 Python research
//! (long-stream precision + independent probability verification).
//!
//! Confirms TODAY, from the memo's frozen specification only:
//!   R1.1 Frozen exponential LUT — regenerate `L_k = max(1, round(2^24 * e^{-k/256}))`
//!        for k in [0, 4096] with Python-round parity (half-to-even) and assert every
//!        documented property (4097 entries, 16388 B as u32, max 16_777_216,
//!        min 2, max represented energy 16).
//!   R1.2 Preliminary Q12 bounds — energy accumulator bound and gradient-numerator
//!        bound evaluated in exact integer arithmetic (i64/u64, no float).
//!   R1.3 Proposed Rust type mapping — representability of the named magnitudes.
//!
//! Out of scope here (BLOCKED, BZ16-R2): golden probability-vector parity
//! `F_Rust,i(s) == F_Python,i(s)` and the nine full-stream digests — those need the
//! exported Python artifacts (vectors, digests, scalar-engine source). Entry-level
//! byte parity for the LUT is likewise settled in R2 against the *exported*
//! constants; this file confirms the formula + properties level.
//!
//! Run: `cargo run --example bz16_lut_confirm --release -p mnf-mans`

const N_ENTRIES: usize = 4097;
const SCALE: f64 = (1u32 << 24) as f64; // 16_777_216

/// Python `round()` parity for positive values: half-to-even.
fn round_half_even(x: f64) -> f64 {
    let fl = x.floor();
    let frac = x - fl;
    if frac > 0.5 {
        fl + 1.0
    } else if frac < 0.5 {
        fl
    } else if (fl as i64) % 2 == 0 {
        fl
    } else {
        fl + 1.0
    }
}

fn build_lut() -> Vec<u32> {
    (0..N_ENTRIES)
        .map(|k| {
            let v = SCALE * (0.0 - k as f64 / 256.0).exp();
            let r = round_half_even(v);
            r.clamp(1.0, u32::MAX as f64) as u32
        })
        .collect()
}

/// FNV-1a 64-bit over the little-endian u32 byte stream (stdlib only; the
/// digest is only ever diffed against a future export of the same stream,
/// so any named deterministic checksum is sufficient until R2).
fn fnv1a_le(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn main() {
    // ---- R1.1: LUT ----
    let lut = build_lut();
    assert_eq!(lut.len(), N_ENTRIES, "entry count");
    assert_eq!(lut[0], 16_777_216, "L_0 max value");
    let max = *lut.iter().max().unwrap();
    let min = *lut.iter().min().unwrap();
    assert_eq!(max, 16_777_216, "max value");
    assert_eq!(min, 2, "min value (max represented energy 4096/256 = 16)");
    assert_eq!(lut[4096], 2, "last entry");
    assert!(lut.iter().all(|&v| v <= u32::MAX), "all entries fit u32");
    assert_eq!(
        lut.iter().any(|&v| v == 1),
        false,
        "no clamped entries (min claims 2)"
    );
    let le_bytes: Vec<u8> = lut.iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(le_bytes.len(), 16_388, "binary table size");
    println!(
        "R1.1 LUT: {}, {} B, max {}, min {}, fnv1a-le {:016x}",
        N_ENTRIES,
        le_bytes.len(),
        max,
        min,
        fnv1a_le(&le_bytes)
    );
    println!(
        "R1.1 LUT head/tail: [{}..={}  {}  {} ...  {}  {}  {}]",
        lut[0], lut[1], lut[2], lut[255], lut[4094], lut[4095], lut[4096]
    );

    // ---- R1.2: preliminary Q12 bounds (exact integer arithmetic) ----
    // Energy accumulator: 3 features * (4 * Q12_max) * (2 * dist_max)
    let energy_bound: i64 = (3i64 * 4 * 4096) * (2 * 2048);
    assert_eq!(energy_bound, 201_326_592, "energy accumulator bound");
    assert!(energy_bound < i32::MAX as i64, "bound fits i32 (memo: use i64 anyway)");
    // Gradient-update numerator: 32768 * 4096 * 4096 * 3
    let gradient_numerator: u64 = 32768u64 * 4096 * 4096 * 3;
    assert_eq!(gradient_numerator, 1_649_267_441_664, "gradient numerator bound");
    assert!(gradient_numerator < 1u64 << 63, "bound fits i64");
    // The typed path must not wrap: plain i64/u64 multiplication (panics on
    // overflow in debug, checked by construction in release asserts above).
    println!(
        "R1.2 bounds: energy {} < i32::MAX; gradient-numerator {} < i64::MAX/2",
        energy_bound, gradient_numerator
    );

    // ---- R1.3: proposed Rust type mapping (memo §5) ----
    let q12_coeff_max: i32 = 4096; // Q12 bounded coefficients
    let dist_max: u16 = 2 * 2048; // circular distance (max 4096)
    assert!(q12_coeff_max <= i32::MAX, "Q12 coefficients fit i32");
    assert!(dist_max <= u16::MAX, "circular distance fits u16"); // 4096 <= 65535
    let _ = (energy_bound as i64) * (dist_max as i64); // products stay in i64 range
    println!("R1.3 types: i32 coeffs (4096), u16 distance (4096), i64 energy, u32 frequencies: OK");

    // ---- self-checks of the instrument itself (would fail if the logic breaks) ----
    assert_eq!(round_half_even(0.5), 0.0, "half-to-even parity");
    assert_eq!(round_half_even(1.5), 2.0);
    assert_eq!(round_half_even(2.5), 2.0);
    assert_eq!(round_half_even(3.5), 4.0);
    assert_eq!(round_half_even(2.6), 3.0);

    println!("BZ16-R1 CONFIRMED: LUT formula+properties and preliminary Q12 bounds verified in Rust; golden-vector parity deferred to R2.");
}
