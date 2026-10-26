// Bridge: read raw stream bytes on stdin, print "mixed_bytes zlib9_bytes" —
// lets the corpus gate drive the Rust mixed coder from Python.
use std::io::Read;

fn main() {
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data).expect("stdin");
    let mixed = mnf_mans::mixed_bytes(&data);
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    std::io::Write::write_all(&mut e, &data).expect("zlib");
    let zlib = e.finish().expect("finish").len();
    // exactness: structural parse must rebuild the input
    let toks = mnf_mans::parse(&data);
    assert_eq!(mnf_mans::rebuild(&toks, data.len()), data, "inverse");
    println!("{mixed} {zlib}");
}
