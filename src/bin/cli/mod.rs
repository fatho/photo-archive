//! General CLI functions.
use photo_archive::clone;
use photo_archive::library::{meta, thumb, LibraryFiles};

use failure::bail;
use log::{info, warn};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use lazy_static::lazy_static;

pub mod photos;
pub mod thumbs;

lazy_static! {
    pub static ref PROGRESS_STYLE: indicatif::ProgressStyle =
        indicatif::ProgressStyle::default_bar()
            .progress_chars("=> ")
            .template("{msg} [{wide_bar}] {pos}/{len} ({eta})");
}

/// Contains things that are relevant curing the whole execution of the app.
pub struct AppContext {
    /// A flag that indicates whether the process was interrupted (via SIGINT/Ctrl+C)
    /// and should terminate as fast as possible.
    interrupted: Arc<AtomicBool>,
}

impl AppContext {
    /// Create a new application context.
    /// This also installs a Ctrl+C handler.
    pub fn new() -> Self {
        let interrupted = Arc::new(AtomicBool::new(false));

        let handler_result = ctrlc::set_handler(clone!(interrupted => move || {
            interrupted.store(true, Ordering::SeqCst);
            info!("Interruption received");
        }));

        if let Err(err) = handler_result {
            warn!("Error setting Ctrl+C handler, proceeding anyway: {}", err)
        };

        Self { interrupted }
    }

    /// Check whether the process has received an interruption signal (SIGINT on linux),
    /// and fail if that is the case.
    pub fn check_interrupted(&self) -> std::io::Result<()> {
        if self.interrupted.load(Ordering::SeqCst) {
            Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
        } else {
            Ok(())
        }
    }
}

/// Generate the database files.
/// If overwrite is true, the old database files are renamed and a new database is created.
pub fn init(files: &LibraryFiles, overwrite: bool) -> Result<(), failure::Error> {
    if !files.root_exists() {
        bail!(
            "Library root directory {} not found",
            files.root_dir.to_string_lossy()
        );
    }

    if files.meta_db_exists() {
        if overwrite {
            photo_archive::util::backup_file(&files.meta_db_file, true)?;
        } else {
            bail!("Meta database already exists");
        }
    }

    if files.thumb_db_exists() {
        if overwrite {
            photo_archive::util::backup_file(&files.thumb_db_file, true)?;
        } else {
            bail!("Thumb database already exists");
        }
    }

    let _ = meta::MetaDatabase::open_or_create(&files.meta_db_file)?;
    let _ = thumb::ThumbDatabase::open_or_create(&files.thumb_db_file)?;

    info!("Library initialized");

    Ok(())
}

/// Display some general information about the photo database.
pub fn status(library_files: &LibraryFiles) -> Result<(), failure::Error> {
    let print_status = |name: &'static str, path: &Path, found: bool| {
        println!(
            "{}: {} ({})",
            name,
            path.to_string_lossy(),
            if found { "FOUND" } else { "NOT FOUND" },
        );
    };
    print_status("Root", &library_files.root_dir, library_files.root_exists());

    // TODO: open databases for status as readonly

    print_status(
        "Meta",
        &library_files.meta_db_file,
        library_files.meta_db_exists(),
    );
    if library_files.meta_db_exists() {
        let meta_db = meta::MetaDatabase::open_or_create(&library_files.meta_db_file)?;
        match meta_db.query_count() {
            Ok(count) => println!("  Photo count: {}", count),
            Err(err) => println!("  Photo count: n/a ({})", err),
        }
    }

    print_status(
        "Thumb",
        &library_files.thumb_db_file,
        library_files.thumb_db_exists(),
    );
    if library_files.thumb_db_exists() {
        let _thumb_db = meta::MetaDatabase::open_or_create(&library_files.meta_db_file)?;
    }

    Ok(())
}
