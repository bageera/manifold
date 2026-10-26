// Image I/O: decode JPEG/PNG to RGB8, encode PNG. Thin shim over image + png crates.

use crate::codec::Dims;
use crate::error::{Error, Result};
use std::io::BufWriter;
use std::path::Path;

/// Loaded image: RGB8 pixels + dims.
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGB8, row-major (w*h*3).
    pub pixels: Vec<u8>, // RGB8, row-major
}

/// Load a JPEG/PNG file as RGB8 (exactness caveat: JPEG is lossy upstream).
pub fn load(path: &Path) -> Result<Image> {
    let img = image::open(path).map_err(|e| Error::ImageIo(e.to_string()))?;
    let rgb = img.to_rgb8();
    Ok(Image {
        width: rgb.width(),
        height: rgb.height(),
        pixels: rgb.into_raw(),
    })
}

/// Write PNG from RGB8.
pub fn save_png(path: &Path, dims: Dims, pixels: &[u8]) -> Result<()> {
    let f = std::fs::File::create(path)?;
    let bw = BufWriter::new(f);
    let mut enc = png::Encoder::new(bw, dims.width, dims.height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc
        .write_header()
        .map_err(|e| Error::ImageIo(format!("png header: {e}")))?;
    writer
        .write_image_data(pixels)
        .map_err(|e| Error::ImageIo(format!("png data: {e}")))?;
    Ok(())
}

/// In-memory PNG encode for size benchmarks.
pub fn encode_png(dims: Dims, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, dims.width, dims.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc
            .write_header()
            .map_err(|e| Error::ImageIo(format!("png header: {e}")))?;
        writer
            .write_image_data(pixels)
            .map_err(|e| Error::ImageIo(format!("png data: {e}")))?;
    }
    Ok(buf)
}
