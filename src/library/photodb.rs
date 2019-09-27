//! Photo DB, mainly used as a cache for fast queries.

use std::path::Path;

use chrono::{DateTime, Utc};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use rusqlite::types::ToSql;
use rusqlite::{OptionalExtension, Transaction, NO_PARAMS};

use crate::database;
use crate::database::{Database, Schema};
use crate::formats::{PhotoInfo, Thumbnail};

/// Database containing metadata about photos.
#[derive(Debug)]
pub struct PhotoDatabase {
    db: Database<PhotoDbSchema>,
}

/// Key for uniquely identifying a photo.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct PhotoId(pub i64);

/// A row in the photo database
pub struct Photo {
    pub id: PhotoId,
    pub relative_path: String,
    pub info: PhotoInfo,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ThumbnailState {
    Present,
    Absent,
    Error,
}

impl PhotoDatabase {
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> database::Result<PhotoDatabase> {
        let mut db = database::Database::open_or_create(path)?;
        db.upgrade()?;
        Ok(Self { db })
    }

    pub fn insert_photo(&self, path_str: &str, info: &PhotoInfo) -> database::Result<PhotoId> {
        let created_str = info.created.map(|ts| ts.to_rfc3339()); // ISO formatted date
        self.db.connection().execute(
            "INSERT INTO photos(rel_path, created, file_hash) VALUES (?1, ?2, ?3)",
            &[&path_str as &dyn ToSql, &created_str, &info.file_hash],
        )?;

        Ok(PhotoId(self.db.connection().last_insert_rowid()))
    }

    pub fn update_photo(
        &self,
        id: PhotoId,
        path_str: &str,
        info: &PhotoInfo,
    ) -> database::Result<usize> {
        let created_str = info.created.map(|ts| ts.to_rfc3339()); // ISO formatted date
        Ok(self.db.connection().execute(
            "UPDATE photos SET rel_path = ?1, created = ?2, file_hash = ?3 WHERE id = ?4",
            &[
                &path_str as &dyn ToSql,
                &created_str,
                &info.file_hash,
                &id.0,
            ],
        )?)
    }

    pub fn get_photo(&self, id: PhotoId) -> database::Result<Option<Photo>> {
        self.db
            .connection()
            .query_row(
                "SELECT id, rel_path, created, file_hash FROM photos WHERE id = ?1",
                &[id.0],
                Self::map_photo_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn query_photo_id_by_path(&self, path_str: &str) -> database::Result<Option<PhotoId>> {
        self.db
            .connection()
            .query_row(
                "SELECT id FROM photos WHERE rel_path = ?1",
                &[path_str],
                |row| row.get(0).map(PhotoId),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn query_all_photo_ids(&self) -> database::Result<std::vec::Vec<PhotoId>> {
        let mut stmt = self
            .db
            .connection()
            .prepare("SELECT id FROM photos ORDER BY created DESC")?;
        let ls: rusqlite::Result<std::vec::Vec<PhotoId>> = stmt
            .query_map(NO_PARAMS, |row| row.get(0).map(PhotoId))?
            .collect();
        ls.map_err(Into::into)
    }

    pub fn query_all_photos(&self) -> database::Result<Vec<Photo>> {
        let mut stmt = self
            .db
            .connection()
            .prepare("SELECT id, rel_path, created, file_hash FROM photos ORDER BY created DESC")?;
        let ls: rusqlite::Result<Vec<Photo>> =
            stmt.query_map(NO_PARAMS, Self::map_photo_row)?.collect();
        ls.map_err(Into::into)
    }

    pub fn query_photo_count(&self) -> database::Result<u32> {
        self.db
            .connection()
            .query_row("SELECT COUNT(*) FROM photos", NO_PARAMS, |row| row.get(0))
            .map_err(Into::into)
    }

    fn map_photo_row(row: &rusqlite::Row) -> rusqlite::Result<Photo> {
        Ok(Photo {
            id: PhotoId(row.get(0)?),
            relative_path: row.get(1)?,
            info: PhotoInfo {
                created: row.get::<_, Option<String>>(2)?.map(|ts_str| {
                    DateTime::parse_from_rfc3339(&ts_str)
                        .expect("Database corrupted (invalid date in table `photos`)")
                        .with_timezone(&Utc)
                }),
                file_hash: row.get(3)?,
            },
        })
    }

    /// Insert or update the thumbnail for a given photo.
    /// If generating the thumbnail caused an error, store the error message instead
    pub fn insert_thumbnail<E: AsRef<str>>(
        &self,
        photo_id: PhotoId,
        thumbnail: &Result<Thumbnail, E>,
    ) -> database::Result<()> {
        self.db.connection().execute(
            "INSERT INTO thumbnails(photo_id, thumbnail, error) VALUES (?1, ?2, ?3) ON CONFLICT (photo_id) DO UPDATE SET thumbnail=?2, error=?3",
            &[
                &photo_id.0 as &dyn ToSql,
                &thumbnail.as_ref().ok() as &dyn ToSql,
                &thumbnail.as_ref().err().map(|err| err.as_ref()) as &dyn ToSql,
            ])?;
        Ok(())
    }

    /// Check whether there is a thumbnail for the given photo in the database.
    pub fn query_thumbnail_state(&self, photo_id: PhotoId) -> database::Result<ThumbnailState> {
        let code = self
            .db
            .connection()
            .query_row(
                "SELECT thumbnail IS NOT NULL FROM thumbnails WHERE photo_id = ?1",
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
                "SELECT thumbnail FROM thumbnails WHERE photo_id = ?1 AND thumbnail IS NOT NULL",
                &[photo.0],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn query_thumbnail_count(&self) -> database::Result<u32> {
        self.db
            .connection()
            .query_row("SELECT COUNT(*) FROM thumbnails", NO_PARAMS, |row| {
                row.get(0)
            })
            .map_err(Into::into)
    }

    pub fn query_total_thumbnail_size(&self) -> database::Result<u64> {
        self.db
            .connection()
            .query_row("SELECT COALESCE(SUM(LENGTH(thumbnail)), 0) FROM thumbnails WHERE thumbnail IS NOT NULL", NO_PARAMS, |row| row.get(0))
            .map(|size_i: i64| size_i as u64)
            .map_err(Into::into)
    }

    /// Delete all cached thumbnails. Cannot be undone.
    pub fn delete_all_thumbnails(&self) -> database::Result<()> {
        self.db
            .connection()
            .execute("DELETE FROM thumbnails", NO_PARAMS)?;
        // We need to vacuum in order to reclaim the freed space
        self.db.connection().execute("VACUUM", NO_PARAMS)?;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, FromPrimitive, ToPrimitive)]
pub enum PhotoDbSchema {
    /// Nothing in there yet
    Empty = 0,
    /// The very first version of the photo library database.
    InitialVersion = 1,
}

impl Schema for PhotoDbSchema {
    fn from_version(version: database::Version) -> Option<Self> {
        <Self as FromPrimitive>::from_u32(version.0)
    }

    fn version(&self) -> database::Version {
        database::Version(self.to_u32().unwrap())
    }

    fn latest() -> Self {
        PhotoDbSchema::InitialVersion
    }

    fn run_upgrade(&self, tx: &Transaction) -> database::Result<()> {
        match self {
            PhotoDbSchema::Empty => Ok(()),
            PhotoDbSchema::InitialVersion => {
                tx.execute("CREATE TABLE photos(
                    id               INTEGER PRIMARY KEY,
                    rel_path         TEXT NOT NULL, -- Relative path to the library root. SQLite uses UTF-8 by default, which cannot represent all paths.
                    created          TEXT,          -- Time the photo was created
                    file_hash        BLOB NOT NULL  -- Hash of the photo file
                    )", NO_PARAMS)?;
                tx.execute(
                    "CREATE UNIQUE INDEX photos_rel_path_index ON photos(rel_path)",
                    NO_PARAMS,
                )?;
                tx.execute(
                    "CREATE TABLE thumbnails(
                    photo_id  INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
                    thumbnail BLOB,
                    error     TEXT,
                    CONSTRAINT thumbnails_present_xor_error CHECK ((thumbnail IS NOT NULL) = (error IS NULL))
                    )",
                    NO_PARAMS,
                )?;
                Ok(())
            }
        }
    }
}
