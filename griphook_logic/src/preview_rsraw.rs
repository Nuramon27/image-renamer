use std::cmp::Ordering;
use std::path::Path;
use std::io::Cursor;

use rsraw::{RawImage, ThumbFormat};


/// Determines which image format to prioritize when loading thumbnail images
/// from a RAW.
fn cmp_formats(format_a: ThumbFormat, format_b: ThumbFormat) -> Ordering {
    match (format_a, format_b) {
        (x, y) if x == y => Ordering::Equal,
        (_, ThumbFormat::Jpeg) => Ordering::Less,
        (ThumbFormat::Jpeg, _) => Ordering::Greater,
        (_, ThumbFormat::Bitmap) => Ordering::Less,
        (ThumbFormat::Bitmap, _) => Ordering::Greater,
        (_, ThumbFormat::Bitmap16) => Ordering::Less,
        (ThumbFormat::Bitmap16, _) => Ordering::Greater,
        (_, _) => Ordering::Equal
    }
}

pub fn load(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let image = if let Ok(image) = image::load_from_memory(&bytes) {
        image
    } else {
        let mut raw_image = RawImage::open(&bytes)
            .map_err(|error| format!("Could not open RAW image: {error}"))?;
        let thumbs = raw_image.extract_thumbs()
            .map_err(|error| format!("Could not extract thumbnail: {error}"))?;
        let thumb = thumbs
            .iter()
            .max_by(|thumb_a, thumb_b| cmp_formats(thumb_a.format, thumb_b.format)
                .then_with(|| (thumb_a.height * thumb_a.width).cmp(&(thumb_b.height * thumb_b.width)))
            )
            .ok_or_else(|| format!("No thumbnails in image"))?;
        image::load_from_memory(&thumb.data)
            .map_err(|error| format!("Thumbnail format not supported: {error}"))?
    };
    let image = image.resize(1920, 1080, image::imageops::FilterType::Lanczos3);
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|error| format!("Could not prepare preview: {error}"))?;
    Ok(png.into_inner())
}