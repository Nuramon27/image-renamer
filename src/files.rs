use regex::Regex;
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

pub const DEFAULT_FILTER: &str = r"^img_(?P<date>\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}(?:-\d+)?)_(?P<author>[\w\d-]*)_(?P<camera>[\w\d-]*)\.(?P<ext>ORF)$";
pub const DEFAULT_REPLACEMENT: &str = "img_${date}_${author}_${camera}_$name.${ext}";

const IMAGE_EXTENSIONS: &[&str] = &[
    "3fr", "arw", "cr2", "cr3", "dng", "jpeg", "jpg", "nef", "nrw", "orf", "pef", "png", "raf",
    "raw", "rw2", "sr2", "tif", "tiff",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageFile {
    pub path: PathBuf,
    pub selected: bool,
}

pub struct ImageDirectory {
    path: PathBuf,
}

impl ImageDirectory {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn scan(&self, filter: &str) -> Result<Vec<ImageFile>, String> {
        let filter = Regex::new(filter).map_err(|error| format!("Invalid filter: {error}"))?;
        let mut files = fs::read_dir(&self.path)
            .map_err(|error| format!("Could not read {}: {error}", self.path.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && ImageFile::is_supported(path))
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
}

impl ImageFile {
    fn is_supported(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
            })
    }
}

pub struct RenameOperation<'a> {
    files: &'a [ImageFile],
    filter: &'a str,
    replacement: &'a str,
    set_name: &'a str,
}

impl<'a> RenameOperation<'a> {
    pub fn new(
        files: &'a [ImageFile],
        filter: &'a str,
        replacement: &'a str,
        set_name: &'a str,
    ) -> Self {
        Self {
            files,
            filter,
            replacement,
            set_name,
        }
    }

    pub fn execute(&self) -> Result<Vec<(PathBuf, PathBuf)>, String> {
        let filter = Regex::new(self.filter).map_err(|error| format!("Invalid filter: {error}"))?;
        let mut renames = Vec::new();
        for file in self.files {
            if file.selected {
                let filename = file
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| format!("Invalid file name: {}", file.path.display()))?;
                let replacement = self.replacement.replace("$name", self.set_name);
                let filename_new = filter.replace(filename, replacement);
                renames.push((file.path.clone(), file.path.with_file_name(PathBuf::from(filename_new.as_ref()))));
            }
        }
        let renames = renames;

        let sources = renames.iter().map(|(from, _)| from).collect::<HashSet<_>>();
        let targets = renames.iter().map(|(_, to)| to).collect::<HashSet<_>>();
        if !targets.is_disjoint(&sources) {
            return Err("Some files may be renamed to the name of another file.".into());
        }
        if targets.len() != renames.len() {
            return Err("Replacement produces duplicate file names".into());
        }
        if let Some((_, target)) = renames
            .iter()
            .find(|(_, target)| target.exists())
        {
            return Err(format!("Target already exists: {}", target.display()));
        }
        for (from, to) in &renames {
            fs::rename(from, to)
                .map_err(|error| rename_error(from, &to, error))?;
        }

        Ok(renames)
    }
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
        assert!(ImageFile::is_supported(Path::new("photo.ORF")));
        assert!(ImageFile::is_supported(Path::new("photo.jpg")));
        assert!(!ImageFile::is_supported(Path::new("notes.txt")));
    }

    #[test]
    #[ignore]
    // Ignored because it manipulates disc
    fn replacement_expands_captures_and_set_name() {
        let directory = std::env::temp_dir().join(format!("image-renamer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let source = directory.join("img_2024-01-02_03-04-05_ada_camera.ORF");
        fs::write(&source, []).unwrap();
        let files = [ImageFile {
            path: source.clone(),
            selected: true,
        }];
        let renamed = RenameOperation::new(&files, DEFAULT_FILTER, DEFAULT_REPLACEMENT, "holiday")
            .execute()
            .unwrap();
        assert_eq!(
            renamed[0].1.file_name().unwrap(),
            "img_2024-01-02_03-04-05_ada_camera_holiday.ORF"
        );
        assert!(renamed[0].1.exists());
        let _ = fs::remove_dir_all(directory);
    }
}
