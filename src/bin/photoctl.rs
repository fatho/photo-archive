use photo_archive::formats;
use photo_archive::library::{meta, thumb, LibraryFiles, MetaInserter};
use photo_archive::clone;

use directories;
use failure::bail;
use log::{debug, error, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::io;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "photoctl - command line photo library manager")]
struct GlobalOpts {
    #[structopt(short, long, parse(from_os_str))]
    /// The root directory of the photo library to be used, if it is not the user's photo directory.
    photo_root: Option<PathBuf>,

    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Initialize the photo library
    Init {
        #[structopt(short, long)]
        /// Re-initialize an existing library.
        overwrite: bool,
    },
    /// Display statistics and meta information about the photo library.
    Status,
    /// Operate on the photo database
    Photos {
        #[structopt(subcommand)]
        command: PhotosCommand,
    },
    /// Operate on the thumbnail database
    Thumbnails {
        #[structopt(subcommand)]
        command: ThumbnailsCommand
    }
}

#[derive(Debug, StructOpt)]
enum PhotosCommand {
    /// List all photos in the database
    List,
    /// Scan the library for new and updated photos.
    Scan {
        /// Enable parallel file scanning using as many threads as indicated,
        /// in addition to the main thread that does the inserting.
        /// Defaults to the number of CPUs if no number is specified.
        /// If omitted, scanning and inserting into the database happens sequentially.
        #[structopt(short, long)]
        jobs: Option<Option<usize>>,
        /// Also scan files that alrady exist in the database
        #[structopt(short, long)]
        all: bool,
        /// The paths to scan. Must be contained within the library root path.
        /// If no paths are specified, the whole library is rescanned.
        #[structopt(parse(from_os_str))]
        paths: Vec<PathBuf>,
    },
}

#[derive(Debug, StructOpt)]
enum ThumbnailsCommand {
    /// Remove all cached thumbnail images
    Wipe,
    /// Generate thumbnails for images in the photo database
    Generate {
        #[structopt(short, long)]
        /// Generate thumbnails also for images that already have one.
        regenerate: bool,
    },
    /// Remove cached thumbnails that are no longer referenced from a photo
    Gc,
}

fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stderr
    ).unwrap();

    let opts = GlobalOpts::from_args();

    debug!("Options: {:?}", opts);

    match run(opts) {
        Err(err) => {
            error!("Exiting due to error: {}", err);
            std::process::exit(1);
        }
        Ok(()) => {
            std::process::exit(0);
        }
    }
}

fn run(opts: GlobalOpts) -> Result<(), failure::Error> {
    let mut context = AppContext::new();

    let photo_root = opts.photo_root.clone().unwrap_or_else(|| {
        let user_dirs = directories::UserDirs::new().expect("Cannot access user directories");
        let photo_path = user_dirs
            .picture_dir()
            .expect("Picture directory not found");
        PathBuf::from(photo_path)
    });

    let library_files = LibraryFiles::new(&photo_root);
    info!(
        "Using library: {}",
        library_files.root_dir.to_string_lossy()
    );

    match &opts.command {
        Command::Init { overwrite } => init(&library_files, *overwrite),
        Command::Status => status(&library_files),
        Command::Photos { command } => match command {
            PhotosCommand::List => photos_list(&library_files),
            PhotosCommand::Scan { jobs, all, paths } => {
                const MAX_SCAN_THREADS: usize = 10;
                let num_threads = jobs.unwrap_or(Some(0)).unwrap_or_else(|| num_cpus::get());
                if num_threads > MAX_SCAN_THREADS {
                    warn!("Cannot use more than {} threads for scanning", MAX_SCAN_THREADS);
                }
                let paths_to_scan: Vec<&Path> = if paths.is_empty() {
                    vec![&library_files.root_dir]
                } else {
                    paths.iter().filter_map(|path| {
                        if path.strip_prefix(&library_files.root_dir).is_ok() {
                            Some(path.as_ref())
                        } else {
                            warn!("Ignoring non-library path {}", path.to_string_lossy());
                            None
                        }
                    })
                    .collect()
                };
                if paths_to_scan.is_empty() {
                    return Err(io::Error::new(io::ErrorKind::InvalidInput, "No valid paths specified").into());
                }
                photos_scan(&mut context, &library_files, num_threads.min(MAX_SCAN_THREADS), *all, &paths_to_scan)
            },
        },
        Command::Thumbnails { command } => Ok(()),
    }
}

struct AppContext {
    interrupted: Arc<AtomicBool>,
}

impl AppContext {
    /// Create a new application context.
    /// This also installs a Ctrl+C handler.
    fn new() -> Self {
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
    fn check_interrupted(&self) -> std::io::Result<()> {
        if self.interrupted.load(Ordering::SeqCst) {
            Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
        } else {
            Ok(())
        }
    }
}

fn init(files: &LibraryFiles, overwrite: bool) -> Result<(), failure::Error> {
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

fn status(library_files: &LibraryFiles) -> Result<(), failure::Error> {
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

fn photos_list(library: &LibraryFiles) -> Result<(), failure::Error> {
    use std::borrow::Cow;

    let meta_db = meta::MetaDatabase::open_or_create(&library.meta_db_file)?;

    let photos = meta_db.query_all_photos()?;

    println!("total {}", photos.len());
    println!("ID\tCreated\tSHA-256\tRelative path");
    for photo in photos.iter() {
        println!(
            "{}\t{}\t{:.8}..\t{}",
            photo.id.0,
            photo.info.created.map_or(Cow::Borrowed("-"), |ts| Cow::Owned(ts.to_rfc3339())),
            photo.info.file_hash,
            photo.relative_path,
        );
    }

    Ok(())
}

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

impl ScanStatCollector {
    pub fn new() -> Self {
        Self {
            total: 0,
            skipped: 0,
            added: 0,
            failed: 0,
        }
    }

    pub fn inc_total(&mut self) {
        self.total += 1;
    }

    pub fn inc_skipped(&mut self) {
        self.skipped += 1;
    }

    pub fn inc_added(&mut self) {
        self.added += 1;
    }

    pub fn inc_failed(&mut self) {
        self.failed += 1;
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn skipped(&self) -> usize {
        self.skipped
    }

    pub fn added(&self) -> usize {
        self.added
    }

    pub fn failed(&self) -> usize {
        self.failed
    }
}

fn photos_scan(context: &mut AppContext, library: &LibraryFiles, num_scan_threads: usize, all: bool, paths: &[&Path]) -> Result<(), failure::Error> {
    use photo_archive::library::PhotoPath;
    use photo_archive::formats::PhotoInfo;
    use photo_archive::library::meta::PhotoId;
    use std::sync::mpsc;

    type ScanResult = (Option<PhotoId>, PhotoPath, Result<PhotoInfo, std::io::Error>);

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
                if all || existing.is_none() {
                    files_to_scan.push((existing, path));
                } else {
                    stats.inc_skipped();
                }
            },
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

    info!("Collected {} files ({} skipped, {} failed)", files_to_scan.len(), stats.skipped(), stats.failed());

    // STEP 2 - Scan files

    let progress_bar = indicatif::ProgressBar::new(0)
        .with_style(indicatif::ProgressStyle::default_bar()
            .progress_chars("=> ")
            .template("{msg} [{wide_bar}] {pos}/{len} ({eta})"));
    progress_bar.set_length(files_to_scan.len() as u64);
    progress_bar.set_message("Scanning");

    let insert_result = |(photo_id, photo_path, scan_result): ScanResult| -> Result<(), failure::Error> {
        match scan_result {
            Ok(info) => {
                if let Some(existing_id) = photo_id {
                    meta_db.update_photo(existing_id, &photo_path.relative_path, &info)?;
                } else {
                    meta_db.insert_photo(&photo_path.relative_path, &info)?;
                };
                stats.inc_added()
            },
            Err(err) => {
                error!("Failed to scan {}: {}", photo_path.full_path.to_string_lossy(), err);
                stats.inc_failed()
            }
        }
        progress_bar.inc(1);
        Ok(())
    };

    if num_scan_threads > 0 {
        // The scanner threads synchronize their input via an atomic index into the files_to_scan vector,
        // and yield the results back to the main thread via channels.
        let file_index = Arc::new(AtomicUsize::new(0));
        let files_to_scan = Arc::new(files_to_scan);
        let (photo_info_sender, photo_info_receiver) = mpsc::channel::<ScanResult>();

        let scan_threads: Vec<std::thread::JoinHandle<()>> = std::iter::repeat_with(|| {
            std::thread::spawn(clone!(photo_info_sender, file_index, files_to_scan => move || {
                loop {
                    let next = file_index.fetch_add(1, Ordering::SeqCst);
                    if next >= files_to_scan.len() {
                        break;
                    }
                    let (photo_id, path) = files_to_scan[next].clone();
                    let info_or_error = PhotoInfo::read_with_default_formats(&path.full_path);
                    if let Err(_) = photo_info_sender.send((photo_id, path, info_or_error)) {
                        // scanning was aborted
                        break;
                    }
                }
            }))
        })
        .take(num_scan_threads)
        .collect();

        // Drop our own copy of the sender so that the receiver stops once all threads are done
        drop(photo_info_sender);

        // Gather the results from the scan threads and insert them into the DB
        photo_info_receiver.into_iter()
            .take_while(|_| context.check_interrupted().is_ok())
            .map(insert_result)
            .collect::<Result<(), _>>()?;

        // Wait for the threads to finish. Once we reached this point, they should have already stopped scanning.
        // The first panic from inside the threads is propagated once all threads have stopped.
        let results: Vec<_> = scan_threads.into_iter().map(|thread| thread.join()).collect();
        for thread_result in results {
            if let Err(panic_object) = thread_result {
                panic!(panic_object)
            }
        }
    } else {
        // Sequential implementation for when parallelism has been disabled
        files_to_scan.into_iter()
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