use std::collections::HashSet;
use std::path::PathBuf;
use std::{fmt, fs};

use regex::Regex;

use super::ImageFile;

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

    pub fn execute(&self) -> Result<Vec<(PathBuf, PathBuf)>, RenameError> {
        let filter = Regex::new(self.filter).map_err(RenameError::InvalidRegex)?;
        let mut renames = Vec::new();
        for file in self.files {
            if file.selected {
                let filename = file.path.file_name().ok_or(RenameError::FilenameNotAccessible(file.path.clone()))?;
                let filename = filename.to_str()
                    .ok_or_else(|| RenameError::NonUnicodeFileName(filename.to_owned()))?;
                let replacement = self.replacement.replace("$name", self.set_name);
                let filename_new = filter.replace(filename, replacement);
                renames.push((file.path.clone(), file.path.with_file_name(PathBuf::from(filename_new.as_ref()))));
            }
        }
        let renames = renames;

        let sources = renames.iter().map(|(from, _)| from).collect::<HashSet<_>>();
        let targets = renames.iter().map(|(_, to)| to).collect::<HashSet<_>>();
        if !targets.is_disjoint(&sources) {
            return Err(RenameError::TargetToSource);
        }
        if targets.len() != renames.len() {
            return Err(RenameError::DuplicateFileNames);
        }
        for (_, target) in &renames {
            if target.exists() {
                return Err(RenameError::TargetExists(target.to_owned()));
            }
        }
        for (from, to) in &renames {
            fs::rename(from, to)
                .map_err(|err| RenameError::OnRename { from: from.clone(), to: to.clone(), err: err.to_string() })?;
        }

        Ok(renames)
    }
}

#[derive(Debug, Clone)]
pub enum RenameError {
    InvalidRegex(regex::Error),
    FilenameNotAccessible(PathBuf),
    NonUnicodeFileName(std::ffi::OsString),
    TargetToSource,
    DuplicateFileNames,
    TargetExists(PathBuf),
    OnRename{ from: PathBuf, to: PathBuf, err: String}
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RenameError::*;
        match self {
            InvalidRegex(err) => write!(f, "Invalid regular expression: {}", err),
            FilenameNotAccessible(path) => write!(
                f, "Filename of path {} could not be obtained", path.to_string_lossy()
            ),
            NonUnicodeFileName(name) => write!(
                f, "File {} has a non-unicode filename", name.to_string_lossy()
            ),
            TargetToSource => write!(
                f, "Some files would be renamed to the original name of another file"
            ),
            DuplicateFileNames => write!(f, "Some files would be renamed to identical names"),
            TargetExists(path) => write!(
                f, "Target file {} already exists", path.to_string_lossy()
            ),
            OnRename{ from, to, err } => write!(
                f, "Error when renaming file {} into {}: {}", 
                from.to_string_lossy(), to.to_string_lossy(), err
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{DEFAULT_PARSER, DEFAULT_REPLACEMENT};
    use super::*;

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
        let renamed = RenameOperation::new(&files, DEFAULT_PARSER, DEFAULT_REPLACEMENT, "holiday")
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