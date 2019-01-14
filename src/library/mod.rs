use std::path::{Path, PathBuf};

pub mod db;
pub mod photo;

#[derive(Debug)]
pub enum Error {
    /// Root folder doesn't exist or is not a directory
    InvalidRoot,
    Io(std::io::Error),
    PhotoExif(exif::Error),
    Db(db::Error),
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

impl From<db::Error> for Error {
    fn from(err: db::Error) -> Error {
        Error::Db(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            &Error::InvalidRoot => write!(f, "Invalid root dir"),
            &Error::Io(ref ioerr) => write!(f, "I/O error: {}", ioerr),
            &Error::PhotoExif(ref exif_error) => write!(f, "EXIF error: {}", exif_error),
            &Error::Db(ref err) => write!(f, "Database error: {}", err),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Library {
    root_dir: PathBuf,
    db: db::PhotoDatabase,
}

impl Library {
    pub fn open<P: Sized + AsRef<Path>>(root_dir: P) -> Result<Self> {
        info!("Opening library '{}'", root_dir.as_ref().to_string_lossy());

        if root_dir.as_ref().is_dir() {
            // open sqlite photo database
            let mut db_file = root_dir.as_ref().to_owned();
            db_file.push("photos.db");

            let photo_db = db::PhotoDatabase::open_or_create(db_file)?;

            let archive = Self {
                root_dir: root_dir.as_ref().to_path_buf(),
                db: photo_db
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
            if ! self.db.has_path(relative)? {
                info!("New photo: {}", relative.to_string_lossy().as_ref());

                // load info
                match photo::Info::load(photo_path) {
                    Ok(info) => {

                    },
                    Err(err) => {
                        // do not fail whole operation here, just log the bad file
                        warn!("Could not read photo: {}", err);
                    }
                }
            }
            Ok(())
        })
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