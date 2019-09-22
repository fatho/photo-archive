use log::{warn};
use std::path::{Path, PathBuf};

pub mod meta;
pub mod thumb;

#[derive(Debug)]
pub struct LibraryFiles {
    pub root_dir: PathBuf,
    pub meta_db_file: PathBuf,
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

pub fn scan_library(path: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    let photo_predicate = |entry: &walkdir::DirEntry| {
        let entry_type = entry.file_type();
        let name = entry.file_name().to_str();
        let is_hidden = name.map_or(false, |s| s.starts_with('.'));
        let is_photo = name
            .and_then(|s| s.split('.').next_back())
            .map_or(false, |s| s == "jpg" || s == "JPG");
        !is_hidden && (entry_type.is_dir() || is_photo)
    };

    walkdir::WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(photo_predicate)
}
