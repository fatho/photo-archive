use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct PhotoInfo {
    filename: PathBuf,
    created: PhotoTimestamp,
}

#[derive(Debug, Clone)]
pub enum PhotoTimestamp {
    /// There was no source for the creation timestamp
    None,
    /// The creation timestamp from the EXIF data was used
    Exif(chrono::NaiveDateTime),
    /// The file creation timestamp was used
    File(chrono::DateTime<chrono::Utc>)
}

impl PhotoInfo {
    pub fn load(filename: &Path) -> Result<Self> {
        let file = std::fs::File::open(filename)?;
        let reader = match exif::Reader::new(&mut std::io::BufReader::new(&file)) {
            Err(exif_err) => {
                warn!("Could not read EXIF from {}: {}", filename.to_string_lossy(), exif_err);
                None
            },
            Ok(reader) => Some(reader)
        };

        // exif created time or file time as fallback, or current time if all else fails
        let created = reader.as_ref().and_then(Self::read_exif_created)
                .or_else(|| filename.metadata().ok()?.created().ok().map(chrono::DateTime::from).map(PhotoTimestamp::File))
                .unwrap_or(PhotoTimestamp::None);

        // create a photo info with sensible default data
        let info = Self {
            filename: filename.to_path_buf(),
            created: created,
        };

        // get info from exif tags that we want
        Ok(info)
    }

    fn read_exif_created(reader: &exif::Reader) -> Option<PhotoTimestamp> {
        let created = reader.get_field(exif::Tag::DateTimeOriginal, false);
        let digitized = reader.get_field(exif::Tag::DateTimeDigitized, false);
        let datetime_tag = created.or(digitized)?;

        let ascii = match datetime_tag.value {
            exif::Value::Ascii(ref ascii) => ascii.first(),
            _ => {warn!("No exif time"); None}
        }?;
        let datetime = exif::DateTime::from_ascii(ascii).ok()?;

        let local = chrono::NaiveDate::from_ymd_opt(datetime.year as i32, datetime.month as u32, datetime.day as u32)?
            .and_hms_nano_opt(datetime.hour as u32, datetime.minute as u32, datetime.second as u32, datetime.nanosecond.unwrap_or(0));

        local.map(PhotoTimestamp::Exif)
    }
}

#[derive(Debug)]
pub enum Error {
    /// Root folder doesn't exist or is not a directory
    InvalidRoot,
    Io(std::io::Error),
    PhotoExif(exif::Error),
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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            &Error::InvalidRoot => write!(f, "Invalid root dir"),
            &Error::Io(ref ioerr) => write!(f, "I/O error: {}", ioerr),
            &Error::PhotoExif(ref exif_error) => write!(f, "EXIF error: {}", exif_error)
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Library {
    root_dir: PathBuf,
    photos: Vec<PhotoInfo>
}

impl Library {
    pub fn open<P: Sized + AsRef<Path>>(root_dir: P) -> Result<Self> {
        debug!("Opening library '{}'", root_dir.as_ref().to_string_lossy());

        if root_dir.as_ref().is_dir() {
            let archive = Self {
                root_dir: root_dir.as_ref().to_path_buf(),
                photos: scan_library(root_dir.as_ref())
            };
            Ok(archive)
        } else {
            Err(Error::InvalidRoot)
        }
    }
}

fn scan_library(path: &Path) -> Vec<PhotoInfo> {
    let photo_predicate = |entry: &walkdir::DirEntry| {
        let entry_type = entry.file_type();
        let name = entry.file_name().to_str();
        let is_hidden = name.map_or(false, |s| s.starts_with("."));
        let is_photo = name.and_then(|s| s.split('.').next_back())
                .map_or(false, |s| s == "jpg" || s == "JPG");
        ! is_hidden && (entry_type.is_dir() || is_photo)
    };

    let mut photos = Vec::<PhotoInfo>::new();
    let dirwalker = walkdir::WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(photo_predicate);

    for entry in dirwalker {
        match entry {
            Err(walkerr) => warn!("Error scanning library: {}", walkerr),
            Ok(file) => if ! file.file_type().is_dir() {
                match PhotoInfo::load(file.path()) {
                    Err(load_err) => warn!("Could not load photo: {}", load_err),
                    Ok(photo) => photos.push(photo)
                }
            }
        }
    }

    photos
}