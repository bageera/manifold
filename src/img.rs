// Image I/O: decode JPEG/PNG to RGB8, encode PNG. Thin shim over image + png crates.

use crate::codec::Dims;
use std::io::BufWriter;
use std::path::Path;

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGB8, row-major
}

pub fn load(path: &Path) -> Result<Image, String> {
    let img = image::open(path).map_err(|e| format!("{e}"))?;
    let rgb = img.to_rgb8();
    Ok(Image {
        width: rgb.width(),
        height: rgb.height(),
        pixels: rgb.into_raw(),
    })
}

pub fn save_png(path: &Path, dims: Dims, pixels: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{e}"))?;
    let bw = BufWriter::new(f);
    let mut enc = png::Encoder::new(bw, dims.width, dims.height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| format!("png header: {e}"))?;
    writer.write_image_data(pixels).map_err(|e| format!("png data: {e}"))?;
    Ok(())
}

/// In-memory PNG encode for size benchmarks.
pub fn encode_png(dims: Dims, pixels: &[u8]) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, dims.width, dims.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| format!("png header: {e}"))?;
        writer.write_image_data(pixels).map_err(|e| format!("png data: {e}"))?;
    }
    Ok(buf)
}