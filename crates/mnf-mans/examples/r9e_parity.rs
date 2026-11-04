// MNF13-R9-E parity driver: the SHIPPED backend-6 evolution must reproduce
// the R9 bridge's measured wire. stdin = raw symbol stream; stdout = the
// mix payload exactly as backend_compress(BACKEND_MIX6) would emit it
// ([u32 stream_len][LE words]) — byte-comparable with the backend payload
// and with r9_audit 'e'-mode output. The exact roundtrip is asserted here
// before any bytes are emitted.
use std::io::Read;

fn main() {
    let mut stdin = Vec::new();
    std::io::stdin().read_to_end(&mut stdin).expect("stdin");
    let data = stdin;
    let wire =
        manifold::mix_coder::mix_encode_with(&manifold::mix_coder::MixParams::BACKEND6, &data);
    let back = manifold::mix_coder::mix_decode_with(
        &manifold::mix_coder::MixParams::BACKEND6,
        &wire,
        data.len(),
    )
    .expect("decode");
    assert_eq!(back, data, "backend 6 exact roundtrip");
    use std::io::Write as _;
    std::io::stdout().write_all(&wire).expect("write");
}
