//! Photo DB, mainly used as a cache for fast queries.

use std::path::{Path};

use chrono::{DateTime, Utc};
use rusqlite::{Transaction, OptionalExtension, NO_PARAMS};
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use rusqlite::types::ToSql;

use crate::database;
use crate::database::{Database, Schema};

/// Database containing metadata about photos.
#[derive(Debug)]
pub struct MetaDatabase {
    db: Database<PhotoDbSchema>,
}

pub type Result<T> = database::Result<T>;
pub type Error = database::Error;

/// Key for uniquely identifying a photo.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct PhotoId(pub i64);

/// A row in the photo database
pub struct Photo {
    pub id: PhotoId,
    pub relative_path: String,
    pub created: DateTime<Utc>
}

impl MetaDatabase {
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> Result<MetaDatabase> {
        let mut db = database::Database::open_or_create(path)?;
        db.upgrade()?;
        Ok(Self {
            db: db,
        })
    }

    pub fn insert_photo(&self, path_str: &str, created: DateTime<Utc>) -> Result<PhotoId> {
        let created_str = created.to_rfc3339(); // ISO formatted date
        self.db.connection().execute(
            "INSERT INTO photos(rel_path, created) VALUES (?1, ?2)",
            &[
                &path_str as &ToSql,
                &created_str
            ])?;

        Ok(PhotoId(self.db.connection().last_insert_rowid()))
    }

    pub fn get_photo(&self, id: PhotoId) -> Result<Option<Photo>> {
        self.db.connection().query_row(
            "SELECT id, rel_path, created FROM photos WHERE id = ?1",
             &[id.0],
             Self::map_photo_row
        ).optional().map_err(Into::into)
    }

    pub fn find_photo_by_path(&self, path_str: &str) -> Result<Option<PhotoId>> {
        self.db.connection().query_row(
            "SELECT id FROM photos WHERE rel_path = ?1",
             &[path_str],
             |row| PhotoId(row.get(0))
        ).optional().map_err(Into::into)
    }

    pub fn all_photos(&self) -> Result<std::vec::Vec<PhotoId>> {
        let mut stmt = self.db.connection().prepare("SELECT id FROM photos ORDER BY created DESC")?;
        let ls: rusqlite::Result<std::vec::Vec<PhotoId>> = stmt.query_map(NO_PARAMS, |row| PhotoId(row.get(0)))?.collect();
        ls.map_err(Into::into)
    }

    fn map_photo_row(row: &rusqlite::Row) -> Photo {
        Photo {
            id: PhotoId(row.get(0)),
            relative_path: row.get(1),
            created: DateTime::parse_from_rfc3339(row.get::<_, String>(2).as_ref())
                .expect("Database corrupted (invalid date in table `photos`)")
                .with_timezone(&Utc),
        }
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
                    created          TEXT NOT NULL  -- Time the photo was created
                    )", NO_PARAMS)?;
                tx.execute("CREATE UNIQUE INDEX photos_rel_path_index ON photos(rel_path)", NO_PARAMS)?;
                Ok(())
            },
        }
    }
}