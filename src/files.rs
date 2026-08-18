use regex::Regex;
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

pub const DEFAULT_FILTER: &str = r"^img_(?P<date>\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}(?:-\d+)?)_(?P<author>[\w\d-]*)_(?P<camera>[\w\d-]*)\.(?P<ext>ORF)$";
pub const DEFAULT_REPLACEMENT: &str = "img_$date_$author_$camera_$name.$ext";

const IMAGE_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "dng", "jpeg", "jpg", "nef", "nrw", "orf", "pef", "png", "raf",
    "raw", "rw2", "sr2", "tif", "tiff",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageFile {
    pub path: PathBuf,
    pub selected: bool,
}

pub fn scan_directory(directory: &Path, filter: &str) -> Result<Vec<ImageFile>, String> {
    let filter = Regex::new(filter).map_err(|error| format!("Invalid filter: {error}"))?;
    let mut files = fs::read_dir(directory)
        .map_err(|error| format!("Could not read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_image(path))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| filter.is_match(name))
        })
        .map(|path| ImageFile {
            path,
            selected: false,
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.path.file_name().cmp(&right.path.file_name()));
    Ok(files)
}

pub fn rename_selected(
    files: &[ImageFile],
    filter: &str,
    replacement: &str,
    set_name: &str,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let filter = Regex::new(filter).map_err(|error| format!("Invalid filter: {error}"))?;
    let renames = files
        .iter()
        .filter(|file| file.selected)
        .map(|file| {
            let name = file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("Invalid file name: {}", file.path.display()))?;
            let captures = filter
                .captures(name)
                .ok_or_else(|| format!("File no longer matches filter: {name}"))?;
            let mut replacement = replacement.replace("$name", set_name);
            for (index, capture_name) in filter.capture_names().enumerate() {
                if let Some(capture_name) = capture_name {
                    replacement = replacement.replace(
                        &format!("${capture_name}"),
                        captures.get(index).map_or("", |capture| capture.as_str()),
                    );
                }
            }
            let mut output = String::new();
            captures.expand(&replacement, &mut output);
            if output.is_empty()
                || Path::new(&output)
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some(&output)
            {
                return Err(format!("Invalid replacement name: {output}"));
            }
            Ok((file.path.clone(), file.path.with_file_name(output)))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let sources = renames.iter().map(|(from, _)| from).collect::<HashSet<_>>();
    let targets = renames.iter().map(|(_, to)| to).collect::<HashSet<_>>();
    if targets.len() != renames.len() {
        return Err("Replacement produces duplicate file names".into());
    }
    if let Some((_, target)) = renames
        .iter()
        .find(|(_, target)| target.exists() && !sources.contains(target))
    {
        return Err(format!("Target already exists: {}", target.display()));
    }

    let staged = renames
        .iter()
        .enumerate()
        .map(|(index, (from, _))| {
            let temporary = from.with_file_name(format!(".image-renamer-{index}.tmp"));
            fs::rename(from, &temporary).map_err(|error| rename_error(from, &temporary, error))?;
            Ok((temporary, from))
        })
        .collect::<Result<Vec<_>, String>>()?;
    for ((temporary, _), (_, target)) in staged.iter().zip(&renames) {
        fs::rename(temporary, target).map_err(|error| rename_error(temporary, target, error))?;
    }
    Ok(renames)
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn rename_error(from: &Path, to: &Path, error: io::Error) -> String {
    format!(
        "Could not rename {} to {}: {error}",
        from.display(),
        to.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_extensions_are_case_insensitive() {
        assert!(is_image(Path::new("photo.ORF")));
        assert!(is_image(Path::new("photo.jpg")));
        assert!(!is_image(Path::new("notes.txt")));
    }

    #[test]
    fn replacement_expands_captures_and_set_name() {
        let directory = std::env::temp_dir().join(format!("image-renamer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let source = directory.join("img_2024-01-02_03-04-05_ada_camera.ORF");
        fs::write(&source, []).unwrap();
        let renamed = rename_selected(
            &[ImageFile {
                path: source.clone(),
                selected: true,
            }],
            DEFAULT_FILTER,
            DEFAULT_REPLACEMENT,
            "holiday",
        )
        .unwrap();
        assert_eq!(
            renamed[0].1.file_name().unwrap(),
            "img_2024-01-02_03-04-05_ada_camera_holiday.ORF"
        );
        assert!(renamed[0].1.exists());
        let _ = fs::remove_dir_all(directory);
    }
}
