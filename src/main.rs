// Manifold Encoder v0.5 CLI.
// Subcommands: benchmark, encode, decode, verify. Atlas on by default; --no-atlas.

use manifold::codec::*;
use manifold::img::{self, Image};
use manifold::mnf6::{decode_mnf7, encode_mnf7};
use manifold::vis;
use std::path::{Path, PathBuf};
use std::time::Instant;

const USAGE: &str = "manifold v0.5
usage:
  manifold benchmark <image>
  manifold encode <image> -o out.mnf5 [--no-atlas] [--geom] [--mnf6]
  manifold decode <file.mnf5> -o out.png
  manifold verify <image> <file.mnf5>
  manifold vis <image> [prefix]     atlas mode/savings maps";

fn die(m: &str) -> ! {
    eprintln!("{m}");
    std::process::exit(2)
}

fn load_rgb(path: &Path) -> Image {
    match img::load(path) {
        Ok(im) => im,
        Err(e) => die(&format!("load {}: {e}", path.display())),
    }
}

fn encode_image(src: &Image, use_atlas: bool, use_geom: bool) -> Vec<u8> {
    let dims = Dims {
        width: src.width,
        height: src.height,
    };
    let tiles = split_tiles(&src.pixels, dims);
    let raw = encode_tiles(&tiles, use_atlas, use_geom);
    let payload = compress_payload(&raw);
    let mut buf = build_header(dims, tiles.len() as u32, payload.len() as u32);
    buf.extend_from_slice(&payload);
    buf
}

fn encode_cmd(path: &Path, out: &Path, use_atlas: bool, use_geom: bool, mnf6: bool) {
    let src = load_rgb(path);
    let dims = Dims {
        width: src.width,
        height: src.height,
    };
    let tiles = split_tiles(&src.pixels, dims);
    let t0 = Instant::now();
    let buf = if mnf6 {
        let (buf, st) = encode_mnf7(&src.pixels, dims, use_geom);
        println!(
            "  [mnf6] flags={:#04x} literal={} ycocg={} exact={} geom={} color={} (nontrivial transforms: {})",
            st.chosen_flags,
            st.literal,
            st.ycocg_literal,
            st.ref_exact,
            st.ref_geom,
            st.ref_exact_color + st.ref_geom_color,
            st.refs.iter().flatten().filter(|(_, t)| *t != 0).count()
        );
        buf
    } else {
        encode_image(&src, use_atlas, use_geom)
    };
    let enc_ms = t0.elapsed().as_millis();
    std::fs::write(out, &buf).unwrap_or_else(|e| die(&format!("write {}: {e}", out.display())));
    let mode_label = if mnf6 { "mnf6" } else { "mnf5" };
    println!(
        "encode [{mode_label}] {}: {} bytes, {}x{} px, {} tiles, atlas={}, geom={}, {} ms",
        out.display(),
        buf.len(),
        src.width,
        src.height,
        tiles.len(),
        use_atlas,
        use_geom,
        enc_ms
    );
}

fn decode_cmd(file: &Path, out: &Path) {
    let buf = std::fs::read(file).unwrap_or_else(|e| die(&format!("read {}: {e}", file.display())));
    let (res_dims, res_pixels) =
        if buf.len() >= 4 && (&buf[0..4] == b"MNF6" || &buf[0..4] == b"MNF7") {
            decode_mnf7(&buf).unwrap_or_else(|e| die(&format!("decode mnf6/7: {e}")))
        } else {
            let res = decode(&buf).unwrap_or_else(|e| die(&format!("decode: {e}")));
            (res.dims, res.pixels)
        };
    img::save_png(out, res_dims, &res_pixels)
        .unwrap_or_else(|e| die(&format!("save {}: {e}", out.display())));
    println!(
        "decode {}: {}x{} → {}",
        file.display(),
        res_dims.width,
        res_dims.height,
        out.display()
    );
}

fn verify_cmd(image: &Path, file: &Path) {
    let src = load_rgb(image);
    let buf = std::fs::read(file).unwrap_or_else(|e| die(&format!("read {}: {e}", file.display())));
    let (rdims, rpx) = if buf.len() >= 4 && (&buf[0..4] == b"MNF6" || &buf[0..4] == b"MNF7") {
        decode_mnf7(&buf).unwrap_or_else(|e| die(&format!("decode mnf6/7: {e}")))
    } else {
        let res = decode(&buf).unwrap_or_else(|e| die(&format!("decode: {e}")));
        (res.dims, res.pixels)
    };
    let dims_ok = rdims.width == src.width && rdims.height == src.height;
    if dims_ok && rpx == src.pixels {
        println!(
            "PASS: exact RGB8 reconstruction ({}x{})",
            src.width, src.height
        );
    } else {
        eprintln!("FAIL: reconstruction differs");
        std::process::exit(1);
    }
}

fn vis_cmd(image: &Path, prefix: &Path) {
    let src = load_rgb(image);
    let dims = Dims {
        width: src.width,
        height: src.height,
    };
    let tiles = split_tiles(&src.pixels, dims);
    let (raw, st) = encode_stats(&tiles, true, true);
    let payload = compress_payload(&raw);
    let buf = {
        let mut b = build_header(dims, tiles.len() as u32, payload.len() as u32);
        b.extend_from_slice(&payload);
        b
    };
    let res = decode(&buf).unwrap_or_else(|e| die(&format!("decode: {e}")));
    assert_eq!(res.pixels, src.pixels, "lossless check");
    vis::write_maps(prefix, dims, &st).unwrap_or_else(|e| die(&format!("vis: {e}")));
    println!(
        "vis: {} tiles -> {}.modes.png / {}.savings.png",
        tiles.len(),
        prefix.display(),
        prefix.display()
    );
}

fn benchmark(path: &Path) {
    let src = load_rgb(path);
    let dims = Dims {
        width: src.width,
        height: src.height,
    };
    let n_tiles = tile_grid(dims).0 * tile_grid(dims).1;
    let raw_pixels = dims.width as u64 * dims.height as u64 * 3;

    struct Row {
        label: &'static str,
        bytes: u64,
        enc_ms: u128,
        dec_ms: u128,
    }
    let mut rows: Vec<Row> = Vec::new();

    let arms: [(&str, bool, bool); 3] = [
        ("MNF5-atlas", true, false),     // exact references only (v0.5 ablation)
        ("MNF5-geom-atlas", true, true), // D8 geometric atlas (v0.6)
        ("MNF5-no-atlas", false, false), // baseline: predictive literals
    ];
    for (label, use_atlas, use_geom) in arms {
        let tiles = split_tiles(&src.pixels, dims);
        let t0 = Instant::now();
        let (raw, st) = encode_stats(&tiles, use_atlas, use_geom);
        let payload = compress_payload(&raw);
        let mut buf = build_header(dims, tiles.len() as u32, payload.len() as u32);
        buf.extend_from_slice(&payload);
        let enc_ms = t0.elapsed().as_millis();
        let t1 = Instant::now();
        let res = decode(&buf).unwrap_or_else(|e| die(&format!("benchmark decode: {e}")));
        let dec_ms = t1.elapsed().as_millis();
        assert_eq!(res.pixels, src.pixels, "lossless check failed for {label}");
        rows.push(Row {
            label,
            bytes: buf.len() as u64,
            enc_ms,
            dec_ms,
        });
        if use_atlas {
            println!(
                "  [{label}] tiles={}: literal={} exact-ref={} geom-ref={} (nontrivial transforms: {})",
                tiles.len(),
                st.literal,
                st.ref_exact,
                st.ref_geom,
                st.geom_nontrivial
            );
        }
    }

    let png_size = img::encode_png(dims, &src.pixels)
        .unwrap_or_else(|e| die(&format!("png benchmark encode: {e}")))
        .len();
    rows.push(Row {
        label: "PNG",
        bytes: png_size as u64,
        enc_ms: 0,
        dec_ms: 0,
    });

    println!(
        "image {}x{}, {} tiles, source RGB8 = {} bytes",
        dims.width, dims.height, n_tiles, raw_pixels
    );
    println!(
        "{:<16} {:>9} {:>8} {:>9} {:>8} {:>7} {:>8}",
        "codec", "bytes", "bits/px", "ratio", "enc ms", "dec ms", "total ms"
    );
    for r in &rows {
        let bpp = 8.0 * r.bytes as f64 / (dims.width as f64 * dims.height as f64);
        let ratio = raw_pixels as f64 / r.bytes as f64;
        println!(
            "{:<16} {:>9} {:>8.3} {:>8.2}x {:>8} {:>7} {:>8}",
            r.label,
            r.bytes,
            bpp,
            ratio,
            r.enc_ms,
            r.dec_ms,
            r.enc_ms + r.dec_ms
        );
    }
}

pub fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    match args[0].as_str() {
        "benchmark" => {
            if let Some(p) = args.get(1) {
                benchmark(Path::new(p));
            } else {
                die("benchmark <image>")
            }
        }
        "encode" => {
            let (path, out, use_atlas, use_geom, mnf6) = parse_encode(&args[1..]);
            encode_cmd(&path, &out, use_atlas, use_geom, mnf6);
        }
        "decode" => {
            let (file, out) = parse_flag(&args[1..], "decode <file.mnf5>");
            decode_cmd(&file, &out);
        }
        "vis" => {
            if args.len() < 2 {
                die("vis <image> [prefix]");
            }
            let prefix = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("atlas"));
            vis_cmd(Path::new(&args[1]), &prefix);
        }
        "verify" => {
            if args.len() < 3 {
                die("verify <image> <file.mnf5>");
            }
            verify_cmd(Path::new(&args[1]), Path::new(&args[2]));
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

fn parse_encode(a: &[String]) -> (PathBuf, PathBuf, bool, bool, bool) {
    let mut path: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut atlas = true;
    let mut mnf6 = false;
    // v0.6 geometric mode is OPT-IN: default output stays v0.5-format
    // (modes 0/1 only) for compatibility with the packaged v0.5 decoder.
    let mut geom = false;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "-o" => {
                i += 1;
                out = Some(PathBuf::from(
                    a.get(i).unwrap_or_else(|| die("-o requires a path")),
                ));
            }
            "--no-atlas" => atlas = false,
            "--geom" => geom = true,
            "--mnf6" => mnf6 = true,
            _ => {
                if path.is_none() {
                    path = Some(PathBuf::from(&a[i]));
                }
            }
        }
        i += 1
    }
    (
        path.unwrap_or_else(|| die("encode: missing image path")),
        out.unwrap_or_else(|| die("encode: missing -o out.mnf5")),
        atlas,
        geom,
        mnf6,
    )
}

fn parse_flag(a: &[String], usage: &str) -> (PathBuf, PathBuf) {
    let mut file: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < a.len() {
        if a[i] == "-o" {
            i += 1;
            out = Some(PathBuf::from(
                a.get(i).unwrap_or_else(|| die("-o requires a path")),
            ));
        } else if file.is_none() {
            file = Some(PathBuf::from(&a[i]));
        }
        i += 1;
    }
    (
        file.unwrap_or_else(|| die(usage)),
        out.unwrap_or_else(|| die("missing -o out.png")),
    )
}
