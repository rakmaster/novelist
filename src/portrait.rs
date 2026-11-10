use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use eframe::egui::ColorImage;
use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;

const MAX_SIDE: u32 = 768;

/// Decode any supported image (jpeg/png), downscale it to at most `MAX_SIDE`
/// pixels, and return it as base64-encoded JPEG for storage in the book file.
pub fn encode(bytes: &[u8]) -> Result<String, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("Unsupported image: {e}"))?;
    let rgb = DynamicImage::ImageRgb8(img.thumbnail(MAX_SIDE, MAX_SIDE).to_rgb8());
    let mut out = Vec::new();
    rgb.write_with_encoder(JpegEncoder::new_with_quality(&mut out, 90))
        .map_err(|e| format!("Could not encode image: {e}"))?;
    Ok(STANDARD.encode(out))
}

pub fn decode(b64: &str) -> Option<ColorImage> {
    let bytes = STANDARD.decode(b64).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::io::Cursor;

    #[test]
    fn large_png_is_downscaled_and_round_trips() {
        let mut png = Vec::new();
        RgbImage::from_pixel(2000, 1000, image::Rgb([200, 40, 40]))
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let b64 = encode(&png).unwrap();
        let img = decode(&b64).unwrap();
        assert_eq!(img.size, [768, 384]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(encode(b"not an image").is_err());
        assert!(decode("%%%").is_none());
    }
}
