//! CLI functions specific to the `photos` subcommand.

use photo_archive::formats::{ImageFormat, JpegFormat};
use photo_archive::library::{LibraryFiles, PhotoDatabase, PhotoId, PhotoPath};

use anyhow::format_err;
use log::{error, info, trace, warn};
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::cli;

/// List all the photos in the database/
pub fn list(context: &mut cli::AppContext, library: &LibraryFiles) -> Result<(), anyhow::Error> {
    use std::borrow::Cow;

    let photo_db = PhotoDatabase::open_or_create(&library.photo_db_file)?;

    let photos = photo_db.query_all_photos()?;

    println!("total {}", photos.len());
    println!("ID\tCreated\tSHA-256\tRelative path");
    for photo in photos.iter() {
        context.check_interrupted()?;
        println!(
            "{}\t{}\t{:.8}..\t{}",
            photo.id.0,
            photo
                .info
                .created
                .map_or(Cow::Borrowed("-"), |ts| Cow::Owned(ts.to_rfc3339())),
            photo.info.file_hash,
            photo.relative_path,
        );
    }

    Ok(())
}

/// Keep track of some statistics while scanning the photo library.
struct ScanStatCollector {
    /// The total number of photo files that were seen during collection
    total: AtomicUsize,
    /// The number of photo files that were skipped because they already exist in the database
    skipped: AtomicUsize,
    /// The number of photo files that were added to the database
    added: AtomicUsize,
    /// The number of photo files that could not be added to the database
    failed: AtomicUsize,
}

#[rustfmt::skip]
impl ScanStatCollector {
    pub fn new() -> Self {
        Self {
            total: AtomicUsize::new(0),
            skipped: AtomicUsize::new(0),
            added: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
        }
    }

    pub fn inc_total(&self) { self.total.fetch_add(1, Ordering::SeqCst); }
    pub fn inc_skipped(&self) { self.skipped.fetch_add(1, Ordering::SeqCst); }
    pub fn inc_added(&self) { self.added.fetch_add(1, Ordering::SeqCst); }
    pub fn inc_failed(&self) { self.failed.fetch_add(1, Ordering::SeqCst); }

    pub fn total(&self) -> usize { self.total.load(Ordering::SeqCst) }
    pub fn skipped(&self) -> usize { self.skipped.load(Ordering::SeqCst) }
    pub fn added(&self) -> usize { self.added.load(Ordering::SeqCst) }
    pub fn failed(&self) -> usize { self.failed.load(Ordering::SeqCst) }
}

/// Scan the photo library or subtrees of it for new and updated photos, optionally in parallel.
pub fn scan(
    context: &mut cli::AppContext,
    library: &LibraryFiles,
    rescan: bool,
    paths: &[PathBuf],
) -> Result<(), anyhow::Error> {
    let photo_db = PhotoDatabase::open_or_create(&library.photo_db_file)?;
    let mut stats = ScanStatCollector::new();

    // STEP 1 - Collect files
    let files_to_scan = scan_collect(context, library, &photo_db, &mut stats, rescan, paths)?;

    info!(
        "Collected {} files ({} skipped, {} failed)",
        files_to_scan.len(),
        stats.skipped(),
        stats.failed()
    );

    // STEP 2 - Scan files

    info!("Scanning files");

    let progress_bar = context.progress().begin_progress(files_to_scan.len());

    let synced_photo_db = Mutex::new(photo_db);

    // Sequential implementation for when parallelism has been disabled
    files_to_scan
        .into_par_iter()
        .map(|scan_job| -> Result<(), anyhow::Error> {
            context.check_interrupted()?;

            let scan_result = JpegFormat.read_info(&scan_job.path.full_path);

            match scan_result {
                Ok(info) => {
                    if let Some(existing_id) = scan_job.existing_id {
                        synced_photo_db
                            .lock()
                            .map_err(|_| format_err!("Database mutex was poisoned"))?
                            .update_photo(existing_id, &scan_job.path.relative_path, &info)?;
                    } else {
                        synced_photo_db
                            .lock()
                            .map_err(|_| format_err!("Database mutex was poisoned"))?
                            .insert_photo(&scan_job.path.relative_path, &info)?;
                    };
                    stats.inc_added()
                }
                Err(err) => {
                    error!(
                        "Failed to scan {}: {}",
                        scan_job.path.full_path.to_string_lossy(),
                        err
                    );
                    stats.inc_failed()
                }
            }

            progress_bar.sender().inc_progress(1);
            Ok(())
        })
        .collect::<Result<(), anyhow::Error>>()?;

    drop(progress_bar);

    info!(
        "Scanning done ({} total, {} added, {} failed, {} skipped)",
        stats.total(),
        stats.added(),
        stats.failed(),
        stats.skipped(),
    );

    Ok(context.check_interrupted()?)
}

/// Task description for scanning a photo.
struct ScanJob {
    /// The id of the photo in the database, if it already exists.
    existing_id: Option<PhotoId>,
    /// The path to the photo.
    path: PhotoPath,
}

fn scan_collect(
    context: &mut cli::AppContext,
    library: &LibraryFiles,
    photo_db: &PhotoDatabase,
    stats: &mut ScanStatCollector,
    rescan: bool,
    paths: &[PathBuf],
) -> Result<Vec<ScanJob>, anyhow::Error> {
    paths
        .iter()
        // First collect all supported photo files from the supplied paths
        .flat_map(|scan_path| {
            if scan_path.is_dir() {
                info!("Collecting files in {}", scan_path.to_string_lossy());

                let dir_iter = photo_archive::library::scan_library(scan_path).filter_map(
                    |result| match result {
                        Ok(entry) => {
                            if entry.file_type().is_file()
                                && JpegFormat.supported_extension(entry.path())
                            {
                                Some(entry.into_path())
                            } else {
                                trace!(
                                    "Not scanning {}: file format not supported",
                                    entry.path().to_string_lossy()
                                );
                                None
                            }
                        }
                        Err(err) => {
                            warn!("Error scanning library: {}", err);
                            None
                        }
                    },
                );
                let dynamic_iter: Box<dyn Iterator<Item = PathBuf>> = Box::new(dir_iter);
                dynamic_iter
            } else {
                info!("Collecting file {}", scan_path.to_string_lossy());
                let file_iter = std::iter::once(scan_path.to_path_buf());
                let dynamic_iter: Box<dyn Iterator<Item = PathBuf>> = Box::new(file_iter);
                dynamic_iter
            }
        })
        // Then look up each of them in the database and create the ScanJob
        .map(|filename| -> Result<Option<ScanJob>, anyhow::Error> {
            stats.inc_total();
            context.check_interrupted()?;

            let scan_job = match PhotoPath::from_absolute(&library.root_dir, &filename) {
                Ok(path) => {
                    let existing = photo_db.query_photo_id_by_path(&path.relative_path)?;
                    if rescan || existing.is_none() {
                        Some(ScanJob {
                            existing_id: existing,
                            path,
                        })
                    } else {
                        stats.inc_skipped();
                        None
                    }
                }
                Err(err) => {
                    stats.inc_failed();
                    warn!("Failed to collect {}: {}", filename.to_string_lossy(), err);
                    None
                }
            };
            Ok(scan_job)
        })
        .filter_map(Result::transpose)
        .collect::<Result<Vec<ScanJob>, _>>()
}
