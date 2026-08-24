//! Contains everything necessary for the view and manipulation of files.

pub mod rename;

use std::path::{Path, PathBuf};
use std::{fmt, fs};

use regex::Regex;

pub use rename::RenameOperation;

/// The default regular expression for parsing image files
pub const DEFAULT_PARSER: &str = r"^img_(?P<date>\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}(?:-\d+)?)_(?P<author>[\w\d-]*)_(?P<camera>[\w\d-]*)\.(?P<ext>ORF)$";
/// The default replacement expression
pub const DEFAULT_REPLACEMENT: &str = "img_${date}_${author}_${camera}_$name.${ext}";

/// The supported image extensions
const IMAGE_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "dng", "jpeg", "jpg", "nef", "nrw", "orf", "pef", "png", "raf",
    "raw", "rw2", "sr2", "tif", "tiff", "webp",
];

/// An image file in the directory
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageFile {
    /// The path to the file
    pub path: PathBuf,
    /// Whether the file is selected for renaming
    pub selected: bool,
}

/// The current directory
pub struct ImageDirectory {
    path: PathBuf,
}

impl ImageDirectory {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns all supported images in `self.path` that match `filter`.
    pub fn scan(&self, filter: &str) -> Result<Vec<ImageFile>, FileError> {
        let filter = Regex::new(filter)?;
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.path)
            .map_err(|err| FileError::ReadDirectory { dir: self.path.clone(), err: err.to_string() })?
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.is_file() && ImageFile::is_supported(&path) {
                if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
                    if filter.is_match(filename) {
                        files.push(ImageFile { 
                            path, 
                            selected: false
                        })
                    }
                }
            }
        }
        files.sort_by(|left, right| left.path.file_name().cmp(&right.path.file_name()));
        Ok(files)
    }
}

impl ImageFile {
    /// Checks whether the file at `path` is an image supported
    /// by our image previewer.
    fn is_supported(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
            })
    }
}

#[derive(Debug, Clone)]
pub enum FileError {
    InvalidRegex(regex::Error),
    ReadDirectory{ dir: PathBuf, err: String }
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::InvalidRegex(err) => write!(f, "Invalid regular expression: {}", err),
            FileError::ReadDirectory{ dir, err } => write!(
                f, "Could not read directory {}: {}", 
                dir.as_os_str().to_string_lossy(), err
            ),
        }
    }
}

impl From<regex::Error> for FileError {
    fn from(value: regex::Error) -> Self {
        FileError::InvalidRegex(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parser_is_valid() {
        assert!(Regex::new(DEFAULT_PARSER).is_ok());
    }

    #[test]
    fn image_extensions_are_case_insensitive() {
        assert!(ImageFile::is_supported(Path::new("photo.ORF")));
        assert!(ImageFile::is_supported(Path::new("photo.jpg")));
        assert!(!ImageFile::is_supported(Path::new("notes.txt")));
    }
}
