//! CLI functions specific to the `photos` subcommand.

use photo_archive::clone;
use photo_archive::library::{meta, LibraryFiles};

use log::{error, info, warn};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::cli;

/// List all the photos in the database/
pub fn list(
    context: &mut cli::AppContext,
    library: &LibraryFiles,
) -> Result<(), failure::Error> {
    use std::borrow::Cow;

    let meta_db = meta::MetaDatabase::open_or_create(&library.meta_db_file)?;

    let photos = meta_db.query_all_photos()?;

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
#[derive(Clone)]
struct ScanStatCollector {
    /// The total number of photo files that were seen during collection
    total: usize,
    /// The number of photo files that were skipped because they already exist in the database
    skipped: usize,
    /// The number of photo files that were added to the database
    added: usize,
    /// The number of photo files that could not be added to the database
    failed: usize,
}

#[rustfmt::skip]
impl ScanStatCollector {
    pub fn new() -> Self {
        Self {
            total: 0,
            skipped: 0,
            added: 0,
            failed: 0,
        }
    }

    pub fn inc_total(&mut self) { self.total += 1; }
    pub fn inc_skipped(&mut self) { self.skipped += 1; }
    pub fn inc_added(&mut self) { self.added += 1; }
    pub fn inc_failed(&mut self) { self.failed += 1; }

    pub fn total(&self) -> usize { self.total }
    pub fn skipped(&self) -> usize { self.skipped }
    pub fn added(&self) -> usize { self.added }
    pub fn failed(&self) -> usize { self.failed }
}

/// Scan the photo library or subtrees of it for new and updated photos, optionally in parallel.
pub fn scan(
    context: &mut cli::AppContext,
    library: &LibraryFiles,
    parallel_scan_threads: Option<usize>,
    rescan: bool,
    paths: &[&Path],
) -> Result<(), failure::Error> {
    use photo_archive::formats::PhotoInfo;
    use photo_archive::library::meta::PhotoId;
    use photo_archive::library::PhotoPath;
    use std::sync::mpsc;

    type ScanResult = (
        Option<PhotoId>,
        PhotoPath,
        Result<PhotoInfo, std::io::Error>,
    );

    let meta_db = meta::MetaDatabase::open_or_create(&library.meta_db_file)?;
    let mut stats = ScanStatCollector::new();

    // STEP 1 - Collect files

    let collect_progress_bar = indicatif::ProgressBar::new_spinner();
    collect_progress_bar.set_message("Collecting files");

    let mut files_to_scan = Vec::new();
    let mut add_file = |filename: PathBuf| -> Result<(), failure::Error> {
        stats.inc_total();

        collect_progress_bar.tick();
        context.check_interrupted()?;

        match PhotoPath::new(&library.root_dir, &filename) {
            Ok(path) => {
                let existing = meta_db.query_photo_id_by_path(&path.relative_path)?;
                if rescan || existing.is_none() {
                    files_to_scan.push((existing, path));
                } else {
                    stats.inc_skipped();
                }
            }
            Err(err) => {
                stats.inc_failed();
                warn!("Failed to collect {}: {}", filename.to_string_lossy(), err);
            }
        }
        Ok(())
    };

    for scan_path in paths {
        if scan_path.is_dir() {
            photo_archive::library::scan_library(scan_path)
                .filter_map(|result| match result {
                    Ok(entry) => {
                        if entry.file_type().is_dir() {
                            None
                        } else {
                            Some(entry.into_path())
                        }
                    }
                    Err(err) => {
                        warn!("Error scanning library: {}", err);
                        None
                    }
                })
                .map(&mut add_file)
                .collect::<Result<(), _>>()?;
        } else {
            add_file(scan_path.to_path_buf())?;
        }
    }

    collect_progress_bar.finish_and_clear();

    info!(
        "Collected {} files ({} skipped, {} failed)",
        files_to_scan.len(),
        stats.skipped(),
        stats.failed()
    );

    // STEP 2 - Scan files

    let progress_bar = indicatif::ProgressBar::new(0).with_style(
        indicatif::ProgressStyle::default_bar()
            .progress_chars("=> ")
            .template("{msg} [{wide_bar}] {pos}/{len} ({eta})"),
    );
    progress_bar.set_length(files_to_scan.len() as u64);
    progress_bar.set_message("Scanning");

    let insert_result =
        |(photo_id, photo_path, scan_result): ScanResult| -> Result<(), failure::Error> {
            match scan_result {
                Ok(info) => {
                    if let Some(existing_id) = photo_id {
                        meta_db.update_photo(existing_id, &photo_path.relative_path, &info)?;
                    } else {
                        meta_db.insert_photo(&photo_path.relative_path, &info)?;
                    };
                    stats.inc_added()
                }
                Err(err) => {
                    error!(
                        "Failed to scan {}: {}",
                        photo_path.full_path.to_string_lossy(),
                        err
                    );
                    stats.inc_failed()
                }
            }
            progress_bar.inc(1);
            Ok(())
        };

    if let Some(thread_count) = parallel_scan_threads {
        // The scanner threads synchronize their input via an atomic index into the files_to_scan vector,
        // and yield the results back to the main thread via channels.
        let file_index = Arc::new(AtomicUsize::new(0));
        let files_to_scan = Arc::new(files_to_scan);
        let (photo_info_sender, photo_info_receiver) = mpsc::channel::<ScanResult>();

        let scan_threads: Vec<std::thread::JoinHandle<()>> = std::iter::repeat_with(|| {
            std::thread::spawn(
                clone!(photo_info_sender, file_index, files_to_scan => move || {
                    loop {
                        let next = file_index.fetch_add(1, Ordering::SeqCst);
                        if next >= files_to_scan.len() {
                            break;
                        }
                        let (photo_id, path) = files_to_scan[next].clone();
                        let info_or_error = PhotoInfo::read_with_default_formats(&path.full_path);
                        if photo_info_sender.send((photo_id, path, info_or_error)).is_err() {
                            // Scanning was aborted (the receiver is gone)
                            break;
                        }
                    }
                }),
            )
        })
        .take(thread_count.max(1)) // use at least 1 thread
        .collect();

        // Drop our own copy of the sender so that the receiver stops once all threads are done
        drop(photo_info_sender);

        // Gather the results from the scan threads and insert them into the DB
        photo_info_receiver
            .into_iter()
            .take_while(|_| context.check_interrupted().is_ok())
            .map(insert_result)
            .collect::<Result<(), _>>()?;

        // Wait for the threads to finish. Once we reached this point, they should have already stopped scanning.
        // The first panic from inside the threads is propagated once all threads have stopped.
        let results: Vec<_> = scan_threads
            .into_iter()
            .map(|thread| thread.join())
            .collect();
        for thread_result in results {
            if let Err(panic_object) = thread_result {
                panic!(panic_object)
            }
        }
    } else {
        // Sequential implementation for when parallelism has been disabled
        files_to_scan
            .into_iter()
            .map(|(photo_id, path)| {
                let scan_result = PhotoInfo::read_with_default_formats(&path.full_path);
                (photo_id, path, scan_result)
            })
            .take_while(|_| context.check_interrupted().is_ok())
            .map(insert_result)
            .collect::<Result<(), _>>()?;
    }

    progress_bar.finish_and_clear();

    info!(
        "Scanning done ({} total, {} added, {} failed, {} skipped)",
        stats.total(),
        stats.added(),
        stats.failed(),
        stats.skipped(),
    );

    Ok(context.check_interrupted()?)
}
