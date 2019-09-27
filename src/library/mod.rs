use std::io;
use std::path::{Path, PathBuf};

pub mod photodb;

/// Holds the paths that a photo library consists of.
#[derive(Debug)]
pub struct LibraryFiles {
    /// The directory where all the photos are stored.
    /// Photos outside of that directory cannot be indexed.
    pub root_dir: PathBuf,
    /// Path of the Sqlite database containing the photo metadata and cached thumbnails.
    pub photo_db_file: PathBuf,
}

impl LibraryFiles {
    pub fn new(root_path: &Path) -> LibraryFiles {
        let root_dir = root_path.to_owned();
        let photo_db_file = root_dir.join("photos.db");

        LibraryFiles {
            root_dir,
            photo_db_file,
        }
    }

    pub fn root_exists(&self) -> bool {
        self.root_dir.is_dir()
    }

    pub fn photo_db_exists(&self) -> bool {
        self.photo_db_file.is_file()
    }

    /// Retrieve the full path of a photo stored in the database.
    pub fn get_full_path(&self, photo: &photodb::Photo) -> PathBuf {
        let mut full_path = self.root_dir.clone();
        let rel_path = Path::new(&photo.relative_path);
        full_path.push(rel_path);
        full_path
    }
}

/// Path to a photo file, providing fast access to both the relative path
/// to some root directory and to the absolute path.
/// Currently only supports paths that can be encoded as UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoPath {
    pub full_path: PathBuf,
    pub relative_path: String,
}

impl PhotoPath {
    /// Create a new photo path.
    ///
    /// # Errors
    ///
    /// Returns an error when the absolute photo path is not a subdirectory of the root directory,
    /// or when the path is not representable as UTF-8.
    pub fn from_absolute(root_dir: &Path, absolute_path: &Path) -> io::Result<Self> {
        let relative_path = absolute_path
            .strip_prefix(root_dir)
            .map_err(|_| io::Error::from(io::ErrorKind::NotFound))?;
        let relative_str = match relative_path.to_str() {
            None =>
            // TODO: support weird encodings in paths
            {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "non-UTF-8 representable path not supported",
                ))
            }
            Some(path_str) => Ok(path_str.to_owned()),
        }?;
        Ok(Self {
            full_path: absolute_path.to_path_buf(),
            relative_path: relative_str,
        })
    }

    /// Retrieve the full path of a photo stored in the database.
    pub fn from_relative(root_dir: &Path, relative_path: &str) -> Self {
        let full_path = root_dir.join(Path::new(relative_path));
        Self {
            full_path,
            relative_path: relative_path.to_owned(),
        }
    }
}

/// Return an iterator for enumerating all non-hidden files
/// and directories under the given root path.
pub fn scan_library(path: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    let photo_predicate = |entry: &walkdir::DirEntry| {
        let name = entry.file_name().to_str();
        // TODO: support windows hidden files
        let is_hidden = name.map_or(false, |s| s.starts_with('.'));
        !is_hidden
    };

    walkdir::WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(photo_predicate)
}
