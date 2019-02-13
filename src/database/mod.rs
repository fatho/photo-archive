//! Convenience wrapper around Rusqlite with some additional features such as migrations.

use std::path::Path;
use rusqlite::{Connection, Transaction, OptionalExtension, NO_PARAMS};
use rusqlite::types::ToSql;

#[derive(Debug)]
pub struct Database<S> {
    conn: Connection,
    schema: S,
}

#[derive(Debug)]
pub enum Error {
    Db(rusqlite::Error),
    UnknownSchemaVersion(Version),
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::Db(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Db(ref err) => err.fmt(f),
            Error::UnknownSchemaVersion(version) => write!(f, "Schema version {} is not known", version.0),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version(pub u32);

/// A versioned database schema with facilities for migrating from one schema version to the next.
pub trait Schema: Ord {
    fn from_version(version: Version) -> Option<Self> where Self: Sized;
    fn version(&self) -> Version;
    fn latest() -> Self;

    /// Run the upgrade from the previous to the current schema.
    fn run_upgrade(&self, tx: &Transaction) -> Result<()>;
}

impl<S> Database<S> where S: Schema {
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        let current_version = Self::init_for_migrations(&mut conn)?;
        let schema = S::from_version(current_version).ok_or(Error::UnknownSchemaVersion(current_version))?;

        Ok(Self {
            conn: conn,
            schema: schema,
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

        conn.execute("CREATE TABLE IF NOT EXISTS version(version INTEGER)", NO_PARAMS)?;
        let cur_version_opt = conn.query_row("SELECT * FROM version", NO_PARAMS, |row| row.get(0)).optional()?;
        let cur_version = match cur_version_opt {
            Some(version) => {
                info!("Found database version {}", version);
                Version(version)
            },
            None => {
                info!("Found blank database");
                let version = Version(0);
                conn.execute("INSERT INTO version(version) VALUES (?1)", &[version.0])?;
                version
            }
        };
        Ok(cur_version)
    }

    fn run_migration(&mut self, target: Version) -> Result<()> {
        info!("Migrating to version {}", target.0);

        let new_schema = S::from_version(target).ok_or(Error::UnknownSchemaVersion(target))?;
        assert_eq!(new_schema.version().0, self.schema.version().0 + 1);

        let tx = self.conn.transaction()?;
        new_schema.run_upgrade(&tx)?;
        tx.execute("UPDATE version SET version = ?1", &[target.0])?;
        tx.commit()?;
        self.schema = new_schema;
        Ok(())
    }
}