//! General CLI functions.
use photo_archive::clone;
use photo_archive::library::{LibraryFiles, PhotoDatabase};

use crate::progresslog::ProgressLogger;
use failure::bail;
use log::{info, warn};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub mod photos;
pub mod thumbs;

/// Contains things that are relevant curing the whole execution of the app,
/// mainly related to the CLI.
pub struct AppContext {
    /// A flag that indicates whether the process was interrupted (via SIGINT/Ctrl+C)
    /// and should terminate as fast as possible.
    interrupted: Arc<AtomicBool>,
    /// Allows displaying a progress bar for interactive command line runs.
    /// When there is no terminal, no progress bar is rendered.
    progress_logger: ProgressLogger,
}

impl AppContext {
    /// Create a new application context.
    /// This also installs a Ctrl+C handler.
    pub fn new(progress_logger: ProgressLogger) -> Self {
        let interrupted = Arc::new(AtomicBool::new(false));

        let handler_result = ctrlc::set_handler(clone!(interrupted => move || {
            interrupted.store(true, Ordering::SeqCst);
            info!("Interruption received");
        }));

        if let Err(err) = handler_result {
            warn!("Error setting Ctrl+C handler, proceeding anyway: {}", err)
        };

        Self {
            interrupted,
            progress_logger,
        }
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

    pub fn progress(&self) -> &ProgressLogger {
        &self.progress_logger
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

    if files.photo_db_exists() {
        if overwrite {
            photo_archive::util::backup_file(&files.photo_db_file, true)?;
        } else {
            bail!("Photo database already exists");
        }
    }

    let _ = PhotoDatabase::open_or_create(&files.photo_db_file)?;

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
        "Photo database",
        &library_files.photo_db_file,
        library_files.photo_db_exists(),
    );
    if library_files.photo_db_exists() {
        let db = PhotoDatabase::open_or_create(&library_files.photo_db_file)?;
        println!("  Photo count: {}", db.query_photo_count()?);
        println!("  Thumbnail count: {}", db.query_thumbnail_count()?);
        println!(
            "  Total thumbnail size: {}",
            indicatif::HumanBytes(db.query_total_thumbnail_size()?)
        );
    }

    Ok(())
}
