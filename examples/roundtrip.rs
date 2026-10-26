//! Encodes synthetic gradient pixels with MNF7 and MNF8 and asserts exact
//! RGB8 reconstruction — the library contract in ten lines each.
use manifold::codec::Dims;
use manifold::mnf6::{decode_mnf7, encode_mnf7};
use manifold::mnf8::{decode_mnf8, encode_mnf8, Profile};

fn main() {
    let (w, h) = (64u32, 48u32);
    let mut px = vec![0u8; (w as usize) * (h as usize) * 3];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;
            px[i] = (x as u8).wrapping_mul(3).wrapping_add(y as u8 / 4);
            px[i + 1] = x as u8 / 2;
            px[i + 2] = (y as u8) * 2;
        }
    }
    let d = Dims {
        width: w,
        height: h,
    };

    let (buf7, st7) = encode_mnf7(&px, d, false);
    let (_, out7) = decode_mnf7(&buf7).expect("decode");
    assert_eq!(out7, px, "MNF7 exact");
    println!(
        "mnf7: {} bytes (literal {}, exact refs {})",
        buf7.len(),
        st7.literal,
        st7.ref_exact
    );

    for (name, prof) in [
        ("balanced", Profile::Balanced),
        ("maximum", Profile::Maximum),
    ] {
        let (buf8, _, backend) = encode_mnf8(&px, d, false, prof);
        let (_, out8, _) = decode_mnf8(&buf8).expect("decode");
        assert_eq!(out8, px, "MNF8 exact");
        println!("mnf8/{name}: {} bytes (backend {backend})", buf8.len());
    }
}
