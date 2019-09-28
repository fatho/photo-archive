use rusqlite::types::{FromSql, ToSql};
use std::fmt;
use std::io;
use std::path::Path;

mod jpeg;

pub use jpeg::JpegFormat;

/// Length of a SHA-256 hash in bytes.
const SHA256_BYTES: usize = 32;

/// A SHA-256 hash of some data.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Sha256Hash([u8; SHA256_BYTES]);

impl Sha256Hash {
    /// Parse a byte slice as SHA-256 hash.
    pub fn from_bytes(hash: &[u8]) -> Option<Sha256Hash> {
        if hash.len() == SHA256_BYTES {
            let mut sha256 = Sha256Hash([0; SHA256_BYTES]);
            sha256.0.copy_from_slice(hash);
            Some(sha256)
        } else {
            None
        }
    }

    /// Compute the SHA-256 hash of the given file.
    pub fn hash_file(filename: &Path) -> io::Result<Sha256Hash> {
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

/// General meta-data associated with a photo file.
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

pub trait ImageFormat {
    /// Name of the image format. Used for presenting to the user.
    fn name(&self) -> &str;

    /// Return the typical file extensions of the image files supported by this format.
    fn supported_extension(&self, path: &Path) -> bool;

    /// Read the meta information from a supported image file.
    fn read_info(&self, path: &Path) -> std::io::Result<PhotoInfo>;
}

/// A JPEG encoded thumbnail image.
pub struct Thumbnail(std::vec::Vec<u8>);

impl Thumbnail {
    /// Generate a thumbnail image where the longest side has at most the given size.
    /// TODO: make thumbnail generation part of image format
    pub fn generate<P: AsRef<Path>>(
        original_file: P,
        size: u32,
    ) -> Result<Thumbnail, failure::Error> {
        use image::GenericImageView;
        let img = image::open(original_file)?;

        let width = img.width();
        let height = img.height();

        let new_img = if width > size || height > size {
            img.resize(size, size, image::imageops::FilterType::Triangle)
        } else {
            img
        };

        let mut jpg = std::vec::Vec::new();
        new_img.write_to(&mut jpg, image::ImageOutputFormat::JPEG(90))?;

        Ok(Thumbnail(jpg))
    }

    pub fn from_jpg_bytes(data: std::vec::Vec<u8>) -> Self {
        Thumbnail(data)
    }

    /// Return a JPG encoded version of the thumbnail.
    pub fn as_jpg_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    pub fn into_jpg_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl ToSql for Thumbnail {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.as_jpg_bytes().to_sql()
    }
}

impl FromSql for Thumbnail {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let blob = value.as_blob()?;
        Ok(Thumbnail::from_jpg_bytes(Vec::from(blob)))
    }
}
