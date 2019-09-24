use log::warn;
use std::io;
use std::path::{Path, PathBuf};

pub mod meta;
pub mod thumb;

/// Holds the paths that a photo library consists of.
#[derive(Debug)]
pub struct LibraryFiles {
    /// The directory where all the photos are stored.
    /// Photos outside of that directory cannot be indexed.
    pub root_dir: PathBuf,
    /// Path of the Sqlite database containing the photo metadata.
    pub meta_db_file: PathBuf,
    /// Path of the Sqlite database containing the cached thumbnails.
    /// This data can always be regenerated and is safe to delete.
    pub thumb_db_file: PathBuf,
}

impl LibraryFiles {
    pub fn new(root_path: &Path) -> LibraryFiles {
        let root_dir = root_path.to_owned();
        let mut meta_db_file = root_dir.clone();
        meta_db_file.push("photos.db");
        let mut thumb_db_file = root_dir.clone();
        thumb_db_file.push("thumbs.db");

        LibraryFiles {
            root_dir,
            meta_db_file,
            thumb_db_file,
        }
    }

    pub fn root_exists(&self) -> bool {
        self.root_dir.is_dir()
    }

    pub fn meta_db_exists(&self) -> bool {
        self.meta_db_file.is_file()
    }

    pub fn thumb_db_exists(&self) -> bool {
        self.thumb_db_file.is_file()
    }
}

#[derive(Debug)]
pub struct Library {
    root_dir: PathBuf,
    meta_db: meta::MetaDatabase,
    thumb_db: thumb::ThumbDatabase,
}

impl Library {
    fn generate_thumbnail_impl(
        &self,
        photo_path: &Path,
        photo_id: meta::PhotoId,
    ) -> Result<(), failure::Error> {
        match thumb::Thumbnail::generate(photo_path, 400) {
            Ok(thumb) => self.thumb_db.insert_thumbnail(photo_id, Ok(&thumb)),
            Err(err) => {
                let err_msg = format!("{}", err);
                self.thumb_db
                    .insert_thumbnail(photo_id, Err(err_msg.as_ref()))
            }
        }
        .map_err(Into::into)
    }

    pub fn generate_thumbnail(&self, photo_id: meta::PhotoId) -> Result<(), failure::Error> {
        if let Some(photo) = self.meta_db.get_photo(photo_id)? {
            let photo_path = self.get_full_path(&photo);
            self.generate_thumbnail_impl(photo_path.as_ref(), photo_id)
        } else {
            warn!("Requested thumbnail for non-existing photo {:?}", photo_id);
            Ok(())
        }
    }

    /// Retrieve the full path of a photo stored in the database.
    pub fn get_full_path(&self, photo: &meta::Photo) -> PathBuf {
        let mut full_path = self.root_dir.clone();
        let rel_path = Path::new(&photo.relative_path);
        full_path.push(rel_path);
        full_path
    }

    /// Gain access to the underlying photo database.
    #[inline(always)]
    pub fn thumb_db(&self) -> &thumb::ThumbDatabase {
        &self.thumb_db
    }

    /// Gain access to the underlying photo database.
    #[inline(always)]
    pub fn meta_db(&self) -> &meta::MetaDatabase {
        &self.meta_db
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
    pub fn new(root_dir: &Path, absolute_path: &Path) -> io::Result<Self> {
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
