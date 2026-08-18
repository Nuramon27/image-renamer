use std::{io::Cursor, path::Path};

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
    let image = image.resize(1920, 1080, image::imageops::FilterType::Lanczos3);
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|error| format!("Could not prepare preview: {error}"))?;
    Ok(png.into_inner())
}

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

    #[test]
    fn embedded_jpeg_prefers_the_largest_preview() {
        let raw = b"RAW\xff\xd8a\xff\xd9metadata\xff\xd8longer preview\xff\xd9";
        assert_eq!(
            embedded_jpeg(raw),
            Some(&b"\xff\xd8longer preview\xff\xd9"[..])
        );
    }
}
