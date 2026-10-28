// MNF9 bridge (extended): stdin = w:u32, h:u32, mode:u8, then the plane.
//   mode 0: plane → zlib9(plane), rans_mixed(plane)
//   mode 1: plane → lz77_2d wire → zlib9(wire), rans_mixed(wire)
// Exactness asserted in Rust on every coded representation:
//   plane: rANS sub-stream roundtrips (inside rans_mixed) + lz2d decode
//   wire:  same + tokens::decode_plane == plane
// stdout: "z9 rans" for the requested representation, plus token stats for
// mode 1 ("tok_z9 tok_rans lits matches plane_z9") — plane_z9 included so
// one bridge call reports both arms per mode (fewer cargo run turns).
use mnf_mans::lz77_2d::{decode_plane, encode_plane, zlib9_len};
use mnf_mans::rans_mixed::rans_bytes;
use std::io::Read;

fn main() {
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data).expect("stdin");
    if data.len() < 9 {
        panic!("stdin: w+h+mode header");
    }
    let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mode = data[8];
    let plane = &data[9..];
    assert_eq!(plane.len(), w * h * 3, "plane size");
    if mode == 0 {
        let z9 = zlib9_len(plane);
        let (total, _, _, _, _) = rans_bytes(plane);
        println!("{z9} {total}");
    } else {
        let (wire, st) = encode_plane(plane, w, h);
        let back = decode_plane(&wire, w, h).expect("wire decode");
        assert_eq!(back, plane, "token wire exactness");
        let z9 = zlib9_len(&wire);
        let (total, _, _, _, _) = rans_bytes(&wire);
        let plane_z9 = zlib9_len(plane);
        println!("{z9} {total} {} {} {plane_z9}", st.literals, st.matches);
    }
}
