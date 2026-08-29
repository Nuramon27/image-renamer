//! Thumbnail generation shared by graphical frontends.

use std::io::Cursor;
use std::path::Path;

const THUMBNAIL_SIZE: u32 = 96;

/// Loads an image preview and encodes a small bicubically-resized PNG thumbnail.
pub fn load(path: &Path) -> Result<Vec<u8>, String> {
    let preview = crate::preview::load(path)?;
    let image = image::load_from_memory(&preview)
        .map_err(|error| format!("Could not decode thumbnail for {}: {error}", path.display()))?
        .resize(
            THUMBNAIL_SIZE,
            THUMBNAIL_SIZE,
            image::imageops::FilterType::CatmullRom,
        );

    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|error| format!("Could not prepare thumbnail: {error}"))?;
    Ok(png.into_inner())
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, GenericImageView, RgbaImage};

    #[test]
    fn thumbnail_size_preserves_aspect_ratio() {
        let image = DynamicImage::ImageRgba8(RgbaImage::new(384, 192)).resize(
            super::THUMBNAIL_SIZE,
            super::THUMBNAIL_SIZE,
            image::imageops::FilterType::CatmullRom,
        );

        assert_eq!(
            image.dimensions(),
            (super::THUMBNAIL_SIZE, super::THUMBNAIL_SIZE / 2)
        );
    }
}
