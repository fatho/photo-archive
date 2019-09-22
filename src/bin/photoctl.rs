use photo_archive::formats;
use photo_archive::library::{meta, thumb, LibraryFiles, MetaInserter};

use directories;
use failure::bail;
use log::{debug, error, info, warn};
use std::path::{Path, PathBuf};
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
}

#[derive(Debug, StructOpt)]
enum PhotosCommand {
    /// List all photos in the database
    List,
    /// Scan the library for new and updated photos.
    Scan,
}

fn main() {
    env_logger::init_from_env(
        env_logger::Env::new()
            .filter("PHOTOCTL_LOG")
            .write_style("PHOTOCTL_LOG_STYLE"),
    );

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
    let photo_root = opts.photo_root.unwrap_or_else(|| {
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

    match opts.command {
        Command::Init { overwrite } => init(&library_files, overwrite),
        Command::Status => status(&library_files),
        Command::Photos { command } => match command {
            PhotosCommand::List => photos_list(&library_files),
            PhotosCommand::Scan => photos_scan(&library_files),
        },
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

    println!("ID\tRelative path\tCreated\tSHA-256");
    for photo in photos.iter() {
        println!(
            "{}\t{}\t{}\t{}",
            photo.id.0,
            photo.relative_path,
            photo.info.created.map_or(Cow::Borrowed("-"), |ts| Cow::Owned(ts.to_rfc3339())),
            photo.info.file_hash
        );
    }
    println!("(total: {})", photos.len());

    Ok(())
}

fn photos_scan(library: &LibraryFiles) -> Result<(), failure::Error> {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use photo_archive::library::InsertResult;

    let interrupted = Arc::new(AtomicBool::new(false));
    let r = interrupted.clone();

    if let Err(err) = ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    }) {
        warn!("Error setting Ctrl+C handler, proceeding anyway: {}", err)
    }

    let meta_db = meta::MetaDatabase::open_or_create(&library.meta_db_file)?;

    let progress_bar = indicatif::ProgressBar::new(0)
        .with_style(indicatif::ProgressStyle::default_bar()
            .progress_chars("=> ")
            .template("{msg} [{wide_bar}] {pos}/{len} ({eta})"));
    progress_bar.set_message("Scanning");
    
    let (file_sender, file_receiver) = crossbeam_channel::unbounded();

    let scanner_thread = {
        let root_dir = library.root_dir.clone();
        let interrupted_scanner = interrupted.clone();
        let scan_progress_bar = progress_bar.clone();
        std::thread::spawn(move || {
            let walker = photo_archive::library::scan_library(&root_dir);
            let file_entries = walker.filter_map(|result| match result {
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
            });

            for filename in file_entries {
                if interrupted_scanner.load(Ordering::SeqCst) {
                    break;
                }
                scan_progress_bar.inc_length(1);
                if let Err(_) = file_sender.send(filename) {
                    break;
                }
            }
        })
    };

    let photos_total = Arc::new(AtomicUsize::new(0));
    let photos_added = Arc::new(AtomicUsize::new(0));
    let photos_failed = Arc::new(AtomicUsize::new(0));

    let inserter_thread = {
        let root_dir = library.root_dir.clone();
        let interrupted_inserter = interrupted.clone();
        let this_photos_total = photos_total.clone();
        let this_photos_added = photos_added.clone();
        let this_photos_failed = photos_failed.clone();
        let this_receiver = file_receiver.clone();
        let insert_progress_bar = progress_bar.clone();
        std::thread::spawn(move || {
            let supported_formats = formats::load_formats();
            let inserter = MetaInserter::new(
                &root_dir,
                &meta_db,
                supported_formats
            );

            for filename in this_receiver.into_iter() {
                if interrupted_inserter.load(Ordering::SeqCst) {
                    break;
                }

                this_photos_total.fetch_add(1, Ordering::SeqCst);

                match inserter.insert_idempotent(&filename) {
                    Err(err) => {
                        this_photos_failed.fetch_add(1, Ordering::SeqCst);
                        error!("Error for {}: {}", filename.to_string_lossy(), err);
                    },
                    Ok(InsertResult::Inserted { .. }) => {
                        this_photos_added.fetch_add(1, Ordering::SeqCst);
                    },
                    Ok(_) => {},
                }
                insert_progress_bar.inc(1);
            }
        })
    };

    let scanner_result = scanner_thread.join();
    let inserter_result = inserter_thread.join();

    progress_bar.finish_and_clear();

    if scanner_result.err().or(inserter_result.err()).is_some() {
        error!("Some thread panicked");
    }

    info!(
        "Scanning done ({} total, {} added, {} failed)",
        photos_total.load(Ordering::SeqCst),
        photos_added.load(Ordering::SeqCst),
        photos_failed.load(Ordering::SeqCst),
    );

    if interrupted.load(Ordering::SeqCst) {
        Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into())
    } else {
        Ok(())
    }
}