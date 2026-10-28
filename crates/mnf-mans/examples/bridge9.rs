// MNF9 bridge: read (w: u32 le, h: u32 le, plane = w*h*3 bytes) on stdin,
// print "tok_len lit_len literal_tokens match_tokens" —
//   tok_len  = zlib9(token wire) via the Rust lz77_2d arm,
//   lit_len  = zlib9(raw plane),
// exactness asserted first: decode_plane(encode_plane(plane)) == plane.
// Lets the corpus gate drive the Rust token arm from Python (one process).
use mnf_mans::{decode_plane, encode_plane, zlib9_len};
use std::io::Read;

fn main() {
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data).expect("stdin");
    if data.len() < 8 {
        panic!("stdin: w+h header");
    }
    let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let plane = &data[8..];
    assert_eq!(plane.len(), w * h * 3, "plane size");
    let (wire, st) = encode_plane(plane, w, h);
    let back = decode_plane(&wire, w, h).expect("decode_plane");
    assert_eq!(back, plane, "token wire exactness");
    let tok = zlib9_len(&wire);
    let lit = zlib9_len(plane);
    println!("{} {} {} {}", tok, lit, st.literals, st.matches);
}
