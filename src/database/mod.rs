//! Convenience wrapper around Rusqlite with some additional features such as migrations.

use thiserror::Error;
use log::{debug, info};
use rusqlite::{Connection, OptionalExtension, Transaction};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Database<S> {
    conn: Connection,
    schema: S,
    filename: PathBuf,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unknown schema {version}")]
    UnknownSchemaVersion { version: Version },
}

pub type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(Debug, Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version(pub u32);

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A versioned database schema with facilities for migrating from one schema version to the next.
pub trait Schema: Ord {
    fn from_version(version: Version) -> Option<Self>
    where
        Self: Sized;
    fn version(&self) -> Version;
    fn latest() -> Self;

    /// Run the upgrade from the previous to the current schema.
    fn run_upgrade(&self, tx: &Transaction) -> Result<()>;
}

impl<S> Database<S>
where
    S: Schema,
{
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug!("Opening database {}", path.as_ref().to_string_lossy());

        let filename = path.as_ref().to_path_buf();
        let mut conn = Connection::open(path)?;

        // set some sensible defaults
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let current_version = Self::init_for_migrations(&mut conn)?;
        let schema = S::from_version(current_version).ok_or(Error::UnknownSchemaVersion {
            version: current_version,
        })?;

        Ok(Self {
            conn,
            schema,
            filename,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn schema(&self) -> &S {
        &self.schema
    }

    /// Upgrade up to the latest version.
    pub fn upgrade(&mut self) -> Result<()> {
        let start_index = self.schema.version().0 + 1;
        let end_index = S::latest().version().0;

        for version in start_index..=end_index {
            self.run_migration(Version(version))?;
        }
        Ok(())
    }

    // Migrations

    /// Prepare a SQLite database for running migrations by creating a table with a
    /// single column and row containing the current version, if it doesn't exist yet.
    /// Returns the current version of the database.
    fn init_for_migrations(conn: &mut Connection) -> rusqlite::Result<Version> {
        debug!("Initializing database migrations");

        conn.execute(
            "CREATE TABLE IF NOT EXISTS version(version INTEGER)",
            [],
        )?;
        let cur_version_opt = conn
            .query_row("SELECT * FROM version", [], |row| row.get(0))
            .optional()?;
        let cur_version = match cur_version_opt {
            Some(version) => {
                debug!("Found database version {}", version);
                Version(version)
            }
            None => {
                debug!("Found blank database");
                let version = Version(0);
                conn.execute("INSERT INTO version(version) VALUES (?1)", [version.0])?;
                version
            }
        };
        Ok(cur_version)
    }

    fn run_migration(&mut self, target: Version) -> Result<()> {
        info!(
            "{}: Migrating to version {}",
            self.filename.to_string_lossy(),
            target.0
        );

        let new_schema =
            S::from_version(target).ok_or(Error::UnknownSchemaVersion { version: target })?;
        assert_eq!(new_schema.version().0, self.schema.version().0 + 1);

        let tx = self.conn.transaction()?;
        new_schema.run_upgrade(&tx)?;
        tx.execute("UPDATE version SET version = ?1", [target.0])?;
        tx.commit()?;
        self.schema = new_schema;
        Ok(())
    }
}
