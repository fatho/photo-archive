//! Photo DB, mainly used as a cache for fast queries.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use rusqlite::types::ToSql;

#[derive(Debug)]
pub struct PhotoDatabase {
    conn: Connection,
}

#[derive(Debug)]
pub struct Error(rusqlite::Error);

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

type Result<T> = std::result::Result<T, Error>;

/// Key for uniquely identifying a photo.
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct PhotoId(i64);

impl PhotoDatabase {
    pub fn open_or_create<P: AsRef<Path>>(path: P) -> Result<PhotoDatabase> {
        // TODO: proper error handling
        let mut conn = Connection::open(path).unwrap();

        // migrate to proper version if necessary
        migrations::upgrade(&mut conn).unwrap();

        Ok(Self {
            conn: conn,
        })
    }

    pub fn has_path<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE rel_path = ?1",
            &[path.as_ref().to_string_lossy().as_ref()],
            |row| row.get::<_, u32>(0) > 0
        ).map_err(Error)
    }

    pub fn insert<P: AsRef<Path>>(&self, path: P, created: DateTime<Utc>, thumbnail: &[u8]) -> Result<PhotoId> {
        let created_str = created.to_rfc3339(); // ISO formatted date
        self.conn.execute(
            "INSERT INTO photos(rel_path, created, thumbnail) VALUES (?1, ?2, ?3)",
            &[&path.as_ref().to_string_lossy().as_ref() as &ToSql, &created_str, &thumbnail])?;

        Ok(PhotoId(self.conn.last_insert_rowid()))
    }
}


mod migrations {
    use rusqlite::{Connection, Transaction, NO_PARAMS, OptionalExtension};

    #[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
    struct Version(u32);

    fn init(conn: &mut Connection) -> rusqlite::Result<Version> {
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

    pub fn upgrade(conn: &mut Connection) -> rusqlite::Result<()> {
        let version = init(conn)?;
        let migrations = [
            (Version(1), |tx: &Transaction| {
                tx.execute("CREATE TABLE photos(
                    id             INTEGER PRIMARY KEY,
                    rel_path       TEXT NOT NULL,
                    time_created   TEXT NOT NULL,
                    thumbnail      BLOB)", NO_PARAMS)?;
                tx.execute("CREATE UNIQUE INDEX photos_rel_path_index ON photos(rel_path)", NO_PARAMS)?;
                Ok(())
            })
        ];

        debug!("Running migrations");
        // run all migrations that haven't been run yet
        for migration in migrations.into_iter().skip(version.0 as usize) {
            let version = migration.0;
            run_migration(conn, version, migration.1)?;
        }

        debug!("Database is up to date");

        Ok(())
    }

    fn run_migration<F: FnOnce(&Transaction) -> rusqlite::Result<()>>(conn: &mut Connection, version: Version, run: F) -> rusqlite::Result<()> {
        info!("Migrating to version {}", version.0);
        let tx = conn.transaction()?;
        run(&tx)?;
        tx.execute("UPDATE version SET version = ?1", &[version.0])?;
        tx.commit()
    }
}