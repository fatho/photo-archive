#[derive(Debug)]
pub enum Error {
    /// Root folder doesn't exist or is not a directory
    InvalidRoot,
    Io(std::io::Error),
    PhotoExif(exif::Error),
    Db(crate::database::Error),
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

impl From<crate::database::Error> for Error {
    fn from(err: crate::database::Error) -> Error {
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

pub type Result<T> = std::result::Result<T, Error>;
