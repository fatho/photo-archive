//! Thumbnail generation

use std::path::{Path};

use rusqlite::{Transaction, OptionalExtension, NO_PARAMS};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use rusqlite::types::ToSql;
use image::GenericImageView;

use crate::database;
use crate::database::{Database, Schema};

use super::meta::PhotoId;

/// A JPEG encoded thumbnail image.
pub struct Thumbnail(std::vec::Vec<u8>);

impl Thumbnail {
    /// Generate a thumbnail image where the longest side has at most the given size.
    pub fn generate<P: AsRef<Path>>(original_file: P, size: u32) -> crate::errors::Result<Thumbnail> {
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
        Ok(Self {
            db: db,
        })
    }

    pub fn get_thumbnail_state(&self, photo_id: PhotoId) -> database::Result<ThumbnailState> {
        let code = self.db.connection().query_row(
            "SELECT (CASE WHEN thumbnail IS NOT NULL THEN 1 ELSE 0 END) +
                   (CASE WHEN error IS NOT NULL THEN 2 ELSE 0 END)
             FROM thumbnails WHERE id = ?1",
             &[photo_id.0],
             |row| row.get::<_, u32>(0)
        ).optional()?;
        Ok(match code {
            None => ThumbnailState::Absent,
            Some(1) => ThumbnailState::Present,
            _ => ThumbnailState::Error,
        })
    }

    pub fn insert_thumbnail(&self, photo_id: PhotoId, thumbnail: Result<&Thumbnail, &str>) -> database::Result<()> {
        self.db.connection().execute(
            "INSERT INTO thumbnails(id, thumbnail, error) VALUES (?1, ?2, ?3) ON CONFLICT (id) DO UPDATE SET thumbnail=?2, error=?3",
            &[
                &photo_id.0 as &ToSql,
                &thumbnail.ok().map(|t| t.as_jpg()) as &ToSql,
                &thumbnail.err() as &ToSql,
            ])?;
        Ok(())
    }


    pub fn get_thumbnail(&self, photo: PhotoId) -> database::Result<Option<Thumbnail>> {
        self.db.connection().query_row(
            "SELECT thumbnail FROM thumbnails WHERE id = ?1 AND thumbnail IS NOT NULL",
            &[photo.0],
            |row| Thumbnail::from_jpg(row.get(0))
        ).optional().map_err(Into::into)
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
                tx.execute("CREATE TABLE thumbnails(
                    id               INTEGER PRIMARY KEY,
                    thumbnail        BLOB,
                    error            TEXT
                    )", NO_PARAMS)?;
                Ok(())
            },
        }
    }
}
