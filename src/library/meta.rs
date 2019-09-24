//! Photo DB, mainly used as a cache for fast queries.

use std::path::Path;

use chrono::{DateTime, Utc};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use rusqlite::types::ToSql;
use rusqlite::{OptionalExtension, Transaction, NO_PARAMS};

use crate::database;
use crate::database::{Database, Schema};
use crate::formats::PhotoInfo;

/// Database containing metadata about photos.
#[derive(Debug)]
pub struct MetaDatabase {
    db: Database<PhotoDbSchema>,
}

pub type Result<T> = std::result::Result<T, failure::Error>;

/// Key for uniquely identifying a photo.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct PhotoId(pub i64);

/// A row in the photo database
pub struct Photo {
    pub id: PhotoId,
    pub relative_path: String,
    pub info: PhotoInfo,
}

impl MetaDatabase {
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> Result<MetaDatabase> {
        let mut db = database::Database::open_or_create(path)?;
        db.upgrade()?;
        Ok(Self { db })
    }

    pub fn insert_photo(&self, path_str: &str, info: &PhotoInfo) -> Result<PhotoId> {
        let created_str = info.created.map(|ts| ts.to_rfc3339()); // ISO formatted date
        self.db.connection().execute(
            "INSERT INTO photos(rel_path, created, file_hash) VALUES (?1, ?2, ?3)",
            &[&path_str as &dyn ToSql, &created_str, &info.file_hash],
        )?;

        Ok(PhotoId(self.db.connection().last_insert_rowid()))
    }

    pub fn update_photo(&self, id: PhotoId, path_str: &str, info: &PhotoInfo) -> Result<usize> {
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

    pub fn get_photo(&self, id: PhotoId) -> Result<Option<Photo>> {
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

    pub fn query_photo_id_by_path(&self, path_str: &str) -> Result<Option<PhotoId>> {
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

    pub fn query_all_photo_ids(&self) -> Result<std::vec::Vec<PhotoId>> {
        let mut stmt = self
            .db
            .connection()
            .prepare("SELECT id FROM photos ORDER BY created DESC")?;
        let ls: rusqlite::Result<std::vec::Vec<PhotoId>> = stmt
            .query_map(NO_PARAMS, |row| row.get(0).map(PhotoId))?
            .collect();
        ls.map_err(Into::into)
    }

    pub fn query_all_photos(&self) -> Result<Vec<Photo>> {
        let mut stmt = self
            .db
            .connection()
            .prepare("SELECT id, rel_path, created, file_hash FROM photos ORDER BY created DESC")?;
        let ls: rusqlite::Result<Vec<Photo>> =
            stmt.query_map(NO_PARAMS, Self::map_photo_row)?.collect();
        ls.map_err(Into::into)
    }

    pub fn query_count(&self) -> Result<u32> {
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
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, FromPrimitive, ToPrimitive)]
pub enum PhotoDbSchema {
    /// Nothing in there yet
    Empty = 0,
    /// The very first version of the photo library database.
    PhotoTable = 1,
}

impl Schema for PhotoDbSchema {
    fn from_version(version: database::Version) -> Option<Self> {
        <Self as FromPrimitive>::from_u32(version.0)
    }

    fn version(&self) -> database::Version {
        database::Version(self.to_u32().unwrap())
    }

    fn latest() -> Self {
        PhotoDbSchema::PhotoTable
    }

    fn run_upgrade(&self, tx: &Transaction) -> database::Result<()> {
        match self {
            PhotoDbSchema::Empty => Ok(()),
            PhotoDbSchema::PhotoTable => {
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
                Ok(())
            }
        }
    }
}
