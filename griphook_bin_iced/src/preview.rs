use std::cmp::Ordering;
use std::{io::Cursor, path::Path};

use image;
#[cfg(feature = "rsraw")]
use rsraw::{RawImage, ThumbFormat};

#[cfg(feature = "rsraw")]
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

pub struct PreviewLoader;

impl PreviewLoader {
    #[cfg(feature = "rsraw")]
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
    
    #[cfg(not(feature = "rsraw"))]
    pub fn load(path: &Path) -> Result<Vec<u8>, String> {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let image = image::load_from_memory(&bytes)
            .or_else(|_| {
                embedded_jpeg(&bytes)
                    .ok_or(image::ImageError::Decoding(
                        image::error::DecodingError::new(
                            image::error::ImageFormatHint::Unknown,
                            "No embedded JPEG preview",
                        ),
                    ))
                    .and_then(image::load_from_memory)
            })
            .map_err(|error| format!("Could not decode {}: {error}", path.display()))?;
        let mut png = Cursor::new(Vec::new());
        image
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| format!("Could not prepare preview: {error}"))?;
        Ok(png.into_inner())
    }
}

#[cfg(not(feature = "rsraw"))]
fn embedded_jpeg(bytes: &[u8]) -> Option<&[u8]> {
    let mut largest = None;
    let mut offset = 0;
    while let Some(start) = bytes[offset..]
        .windows(2)
        .position(|marker| marker == [0xff, 0xd8])
    {
        let start = offset + start;
        let end = bytes[start + 2..]
            .windows(2)
            .position(|marker| marker == [0xff, 0xd9])
            .map(|end| start + 4 + end)?;
        let candidate = &bytes[start..end];
        if largest.is_none_or(|current: &[u8]| candidate.len() > current.len()) {
            largest = Some(candidate);
        }
        offset = end;
    }
    largest
}


#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "rsraw"))]
    #[test]
    fn embedded_jpeg_prefers_the_largest_preview() {
        let raw = b"RAW\xff\xd8a\xff\xd9metadata\xff\xd8longer preview\xff\xd9";
        assert_eq!(
            embedded_jpeg(raw),
            Some(&b"\xff\xd8longer preview\xff\xd9"[..])
        );
    }
}
