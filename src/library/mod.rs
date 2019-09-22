use log::{debug, error, info, warn};
use std::io;
use std::path::{Path, PathBuf};

pub mod meta;
pub mod photo;
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

    /// Return whether all library files exists.
    pub fn present(&self) -> bool {
        self.root_exists() && self.meta_db_exists() && self.thumb_db_exists()
    }

    pub fn remove_with_backup(&self) -> Result<(), io::Error> {
        if self.meta_db_exists() {
            Self::make_backup(&self.meta_db_file, true)?;
        }
        if self.thumb_db_exists() {
            Self::make_backup(&self.thumb_db_file, true)?;
        }
        Ok(())
    }

    fn make_backup(file_path: &Path, rename: bool) -> Result<(), io::Error> {
        for bak_num in 0..10 {
            let bak_file = file_path.with_extension(format!("{}.bak", bak_num));

            if !bak_file.exists() {
                let result = if rename {
                    std::fs::rename(file_path, &bak_file)
                } else {
                    std::fs::copy(file_path, &bak_file).map(|_| ())
                };
                match result {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        error!(
                            "Could not backup {} to {} due to {}",
                            file_path.to_string_lossy(),
                            bak_file.to_string_lossy(),
                            err
                        );
                    }
                }
            }
        }
        Err(io::Error::new(io::ErrorKind::Other, "Too many backups"))
    }
}

#[derive(Debug)]
pub struct Library {
    root_dir: PathBuf,
    meta_db: meta::MetaDatabase,
    thumb_db: thumb::ThumbDatabase,
}

impl Library {
    pub fn open_or_create(files: &LibraryFiles) -> Result<Library, failure::Error> {
        info!("Opening library '{}'", files.root_dir.to_string_lossy());

        if files.root_exists() {
            // open sqlite photo database
            let meta_db = meta::MetaDatabase::open_or_create(&files.meta_db_file)?;
            let thumb_db = thumb::ThumbDatabase::open_or_create(&files.thumb_db_file)?;

            let archive = Self {
                root_dir: files.root_dir.clone(),
                meta_db: meta_db,
                thumb_db: thumb_db,
            };
            Ok(archive)
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound).into())
        }
    }

    pub fn refresh(&self) -> Result<(), failure::Error> {
        info!("Rescanning library");

        let root_path = self.root_dir.as_ref();
        scan_library(root_path, |photo_path| {
            let relative = photo_path.strip_prefix(root_path).unwrap();
            match relative.to_str() {
                None => error!(
                    "Could not read photo with non-UTF-8 path {}",
                    relative.to_string_lossy()
                ),
                Some(path_str) => {
                    let photo_id = if let Some(existing_id) =
                        self.meta_db.find_photo_by_path(path_str)?
                    {
                        Some(existing_id)
                    } else {
                        info!("New photo: {}", relative.to_string_lossy().as_ref());

                        // load info, do not fail whole operation on error, just log
                        match photo::Info::load(photo_path) {
                            Ok(info) => Some(self.meta_db.insert_photo(path_str, info.created())?),
                            Err(err) => {
                                error!("Could not read photo: {}", err);
                                None
                            }
                        }
                    };
                    if let Some(photo_id) = photo_id {
                        // generate thumbnail
                        // TODO: parallelize generating thumbnails so UI shows immediately
                        match self.thumb_db.get_thumbnail_state(photo_id)? {
                            thumb::ThumbnailState::Error => {
                                info!("Generating thumbnail failed ealier, skipping!")
                            }
                            thumb::ThumbnailState::Present => debug!("Thumbnail already exists"),
                            thumb::ThumbnailState::Absent => {
                                info!("Generating thumbnail!");
                                self.generate_thumbnail_impl(photo_path, photo_id)?;
                            }
                        }
                    }
                }
            }
            Ok(())
        })
    }

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

fn scan_library<F>(path: &Path, mut callback: F) -> Result<(), failure::Error>
where
    F: FnMut(&Path) -> Result<(), failure::Error>,
{
    let photo_predicate = |entry: &walkdir::DirEntry| {
        let entry_type = entry.file_type();
        let name = entry.file_name().to_str();
        let is_hidden = name.map_or(false, |s| s.starts_with("."));
        let is_photo = name
            .and_then(|s| s.split('.').next_back())
            .map_or(false, |s| s == "jpg" || s == "JPG");
        !is_hidden && (entry_type.is_dir() || is_photo)
    };

    let dirwalker = walkdir::WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(photo_predicate);

    for entry in dirwalker {
        match entry {
            Err(walkerr) => warn!("Error scanning library: {}", walkerr),
            Ok(file) => {
                if !file.file_type().is_dir() {
                    callback(file.path())?;
                }
            }
        }
    }
    Ok(())
}
