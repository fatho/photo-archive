//! Thumbnail generation

use std::path::Path;

use image::GenericImageView;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use rusqlite::types::ToSql;
use rusqlite::{OptionalExtension, Transaction, NO_PARAMS};

use crate::database;
use crate::database::{Database, Schema};

use super::meta::PhotoId;

/// A JPEG encoded thumbnail image.
pub struct Thumbnail(std::vec::Vec<u8>);

impl Thumbnail {
    /// Generate a thumbnail image where the longest side has at most the given size.
    pub fn generate<P: AsRef<Path>>(
        original_file: P,
        size: u32,
    ) -> Result<Thumbnail, failure::Error> {
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

    pub fn from_jpg(data: std::vec::Vec<u8>) -> Self {
        Thumbnail(data)
    }

    /// Return a JPG encoded version of the thumbnail.
    pub fn as_jpg(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// Database containing metadata about photos.
#[derive(Debug)]
pub struct ThumbDatabase {
    db: Database<ThumbDbSchema>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ThumbnailState {
    Present,
    Absent,
    Error,
}

impl ThumbDatabase {
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> database::Result<ThumbDatabase> {
        let mut db = database::Database::open_or_create(path)?;
        db.upgrade()?;
        Ok(Self { db })
    }

    /// Insert or update the thumbnail for a given photo.
    /// If generating the thumbnail caused an error, store the error message instead
    pub fn insert_thumbnail(
        &self,
        photo_id: PhotoId,
        thumbnail: Result<&Thumbnail, &str>,
    ) -> database::Result<()> {
        self.db.connection().execute(
            "INSERT INTO thumbnails(id, thumbnail, error) VALUES (?1, ?2, ?3) ON CONFLICT (id) DO UPDATE SET thumbnail=?2, error=?3",
            &[
                &photo_id.0 as &dyn ToSql,
                &thumbnail.ok().map(|t| t.as_jpg()) as &dyn ToSql,
                &thumbnail.err() as &dyn ToSql,
            ])?;
        Ok(())
    }

    /// Check whether there is a thumbnail for the given photo in the database.
    pub fn query_thumbnail_state(&self, photo_id: PhotoId) -> database::Result<ThumbnailState> {
        let code = self
            .db
            .connection()
            .query_row(
                "SELECT thumbnail IS NOT NULL FROM thumbnails WHERE id = ?1",
                &[photo_id.0],
                |row| row.get::<_, bool>(0),
            )
            .optional()?;
        Ok(match code {
            None => ThumbnailState::Absent,
            Some(true) => ThumbnailState::Present,
            // since we can have either the thumbnail or the error,
            // we know an error must be present if there was no thumbnail
            Some(false) => ThumbnailState::Error,
        })
    }

    /// Retrieve the thumbnail for a given photo if it exists.
    pub fn query_thumbnail(&self, photo: PhotoId) -> database::Result<Option<Thumbnail>> {
        // TODO: return either thumbnail or the stored error
        self.db
            .connection()
            .query_row(
                "SELECT thumbnail FROM thumbnails WHERE id = ?1 AND thumbnail IS NOT NULL",
                &[photo.0],
                |row| row.get(0).map(Thumbnail::from_jpg),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Delete all cached thumbnails. Cannot be undone.
    pub fn delete_all_thumbnails(&self) -> database::Result<()> {
        self.db.connection().execute("DELETE FROM thumbnails", NO_PARAMS)?;
        // We need to vacuum in order to reclaim the freed space
        self.db.connection().execute("VACUUM", NO_PARAMS)?;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, FromPrimitive, ToPrimitive)]
pub enum ThumbDbSchema {
    /// Nothing in there yet
    Empty = 0,
    /// Adds a table containing thumbnails
    ThumbTable = 1,
}

impl Schema for ThumbDbSchema {
    fn from_version(version: database::Version) -> Option<Self> {
        <Self as FromPrimitive>::from_u32(version.0)
    }

    fn version(&self) -> database::Version {
        database::Version(self.to_u32().unwrap())
    }

    fn latest() -> Self {
        ThumbDbSchema::ThumbTable
    }

    fn run_upgrade(&self, tx: &Transaction) -> database::Result<()> {
        match self {
            ThumbDbSchema::Empty => Ok(()),
            ThumbDbSchema::ThumbTable => {
                tx.execute(
                    "CREATE TABLE thumbnails(
                    id               INTEGER PRIMARY KEY,
                    thumbnail        BLOB,
                    error            TEXT,
                    CONSTRAINT thumbnails_present_xor_error CHECK ((thumbnail IS NOT NULL) = (error IS NULL))
                    )",
                    NO_PARAMS,
                )?;
                Ok(())
            }
        }
    }
}
