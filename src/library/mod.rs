use std::path::{Path, PathBuf};

pub mod meta;
pub mod photo;
pub mod thumb;

#[derive(Debug)]
pub enum Error {
    /// Root folder doesn't exist or is not a directory
    InvalidRoot,
    Io(std::io::Error),
    PhotoExif(exif::Error),
    Db(meta::Error),
    Image(image::ImageError),
    // LibraryScanError(walkdir::Error),
}

impl From<exif::Error> for Error {
    fn from(err: exif::Error) -> Error {
        Error::PhotoExif(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(ioerr: std::io::Error) -> Error {
        Error::Io(ioerr)
    }
}

impl From<meta::Error> for Error {
    fn from(err: meta::Error) -> Error {
        Error::Db(err)
    }
}

impl From<image::ImageError> for Error {
    fn from(err: image::ImageError) -> Error {
        Error::Image(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            &Error::InvalidRoot => write!(f, "Invalid root dir"),
            &Error::Io(ref ioerr) => write!(f, "I/O error: {}", ioerr),
            &Error::PhotoExif(ref exif_error) => write!(f, "EXIF error: {}", exif_error),
            &Error::Db(ref err) => write!(f, "Database error: {}", err),
            &Error::Image(ref err) => write!(f, "Image error: {}", err),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Library {
    root_dir: PathBuf,
    meta_db: meta::MetaDatabase,
    thumb_db: thumb::ThumbDatabase,
}

impl Library {
    pub fn open<P: Sized + AsRef<Path>>(root_dir: P) -> Result<Self> {
        info!("Opening library '{}'", root_dir.as_ref().to_string_lossy());

        if root_dir.as_ref().is_dir() {
            // open sqlite photo database
            let mut meta_db_file = root_dir.as_ref().to_owned();
            meta_db_file.push("photos.db");
            let mut thumb_db_file = root_dir.as_ref().to_owned();
            thumb_db_file.push("thumbs.db");

            let meta_db = meta::MetaDatabase::open_or_create(meta_db_file)?;
            let thumb_db = thumb::ThumbDatabase::open_or_create(thumb_db_file)?;

            let archive = Self {
                root_dir: root_dir.as_ref().to_path_buf(),
                meta_db: meta_db,
                thumb_db: thumb_db,
            };
            Ok(archive)
        } else {
            Err(Error::InvalidRoot)
        }
    }

    pub fn refresh(&self) -> Result<()> {
        info!("Rescanning library");

        let root_path = self.root_dir.as_ref();
        scan_library(root_path, |photo_path| {
            let relative = photo_path.strip_prefix(root_path).unwrap();
            match relative.to_str() {
                None => error!("Could not read photo with non-UTF-8 path {}", relative.to_string_lossy()),
                Some(path_str) => {
                    let photo_id = if let Some(existing_id) = self.meta_db.find_photo_by_path(path_str)? {
                        Some(existing_id)
                    } else {
                        info!("New photo: {}", relative.to_string_lossy().as_ref());

                        // load info, do not fail whole operation on error, just log
                        match photo::Info::load(photo_path) {
                            Ok(info) => {
                                Some(self.meta_db.insert_photo(path_str, info.created())?)
                            },
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
                            thumb::ThumbnailState::Error => info!("Generating thumbnail failed ealier, skipping!"),
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

    fn generate_thumbnail_impl(&self, photo_path: &Path, photo_id: meta::PhotoId) -> Result<()> {
        match thumb::Thumbnail::generate(photo_path, 400) {
            Ok(thumb) => self.thumb_db.insert_thumbnail(photo_id, Ok(&thumb)),
            Err(err) => {
                let err_msg = format!("{}", err);
                self.thumb_db.insert_thumbnail(photo_id, Err(err_msg.as_ref()))
            }
        }.map_err(Into::into)
    }

    pub fn generate_thumbnail(&self, photo_id: meta::PhotoId)-> Result<()> {
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

fn scan_library<F>(path: &Path, mut callback: F) -> Result<()> where F: FnMut(&Path) -> Result<()> {
    let photo_predicate = |entry: &walkdir::DirEntry| {
        let entry_type = entry.file_type();
        let name = entry.file_name().to_str();
        let is_hidden = name.map_or(false, |s| s.starts_with("."));
        let is_photo = name.and_then(|s| s.split('.').next_back())
                .map_or(false, |s| s == "jpg" || s == "JPG");
        ! is_hidden && (entry_type.is_dir() || is_photo)
    };

    let dirwalker = walkdir::WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(photo_predicate);

    for entry in dirwalker {
        match entry {
            Err(walkerr) => warn!("Error scanning library: {}", walkerr),
            Ok(file) => if ! file.file_type().is_dir() {
                callback(file.path())?;
            }
        }
    }
    Ok(())
}