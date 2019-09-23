use rusqlite::types::{FromSql, ToSql};
use std::fmt;
use std::io;
use std::path::Path;
use log::warn;

mod jpeg;

pub use jpeg::JpegFormat;

/// Length of a SHA-256 hash in bytes.
const SHA256_BYTES: usize = 32;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Sha256Hash([u8; SHA256_BYTES]);

impl Sha256Hash {
    pub fn from_bytes(hash: &[u8]) -> Option<Sha256Hash> {
        if hash.len() == SHA256_BYTES {
            let mut sha256 = Sha256Hash([0; SHA256_BYTES]);
            sha256.0.copy_from_slice(hash);
            Some(sha256)
        } else {
            None
        }
    }

    pub fn from_file(filename: &Path) -> io::Result<Sha256Hash> {
        use io::Read;
        use sha2::digest::{FixedOutput, Input};

        let mut file = std::fs::File::open(filename)?;
        let mut file_hasher = sha2::Sha256::default();
        let mut buffer = [0; 4096];
        loop {
            let num_read = file.read(&mut buffer)?;
            file_hasher.input(&buffer[0..num_read]);
            if num_read < buffer.len() {
                break;
            }
        }
        let file_hash =
            Sha256Hash::from_bytes(&file_hasher.fixed_result()).expect("SHA-256 is broken");
        Ok(file_hash)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for Sha256Hash {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let num_bytes = formatter.precision().unwrap_or(std::usize::MAX);
        for b in self.as_bytes().iter().take(num_bytes) {
            write!(formatter, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl ToSql for Sha256Hash {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.as_bytes().to_sql()
    }
}

impl FromSql for Sha256Hash {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let blob = value.as_blob()?;
        Sha256Hash::from_bytes(blob).ok_or(rusqlite::types::FromSqlError::InvalidType)
    }
}

#[derive(Debug)]
pub struct PhotoInfo {
    /// Creation time of the photo
    pub created: Option<chrono::DateTime<chrono::Utc>>,
    /// Hash of the whole file of the photo
    pub file_hash: Sha256Hash,
    // TODO: Also hash the image data of the photo separately,
    // for finding duplicates
    //pub image_data_hash: Sha256Hash,
}

impl PhotoInfo {
    /// Read the photo information from a file,
    pub fn read_with_default_formats(filename: &Path) -> io::Result<PhotoInfo> {
        load_default_formats()
            .into_iter()
            .filter(|format| format.supported_extension(filename))
            .find_map(|format| match format.read_info(filename) {
                Ok(info) => Some(info),
                Err(err) => {
                    warn!("{} error: {}", format.name(), err);
                    None
                }
            })
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidData,
                "File format is not supported",
            ))
    }
}

pub trait ImageFormat {
    fn name(&self) -> &str;

    /// Return the typical file extensions of the image files supported by this format.
    fn supported_extension(&self, path: &Path) -> bool;

    /// Read the meta information from a supported image file.
    fn read_info(&self, path: &Path) -> std::io::Result<PhotoInfo>;
}

pub fn load_default_formats() -> Vec<Box<dyn ImageFormat>> {
    vec![Box::new(JpegFormat)]
}