use photo_archive::formats;
use photo_archive::library::{meta, thumb, LibraryFiles};

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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let interrupted = Arc::new(AtomicBool::new(false));
    let r = interrupted.clone();

    if let Err(err) = ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    }) {
        warn!("Error setting Ctrl+C handler, proceeding anyway: {}", err)
    }

    let meta_db = meta::MetaDatabase::open_or_create(&library.meta_db_file)?;
    let supported_formats = formats::load_formats();

    let walker = photo_archive::library::scan_library(&library.root_dir);
    let file_entries = walker.filter_map(|result| match result {
        Ok(entry) => {
            if entry.file_type().is_dir() {
                None
            } else {
                Some(entry)
            }
        }
        Err(err) => {
            warn!("Error scanning library: {}", err);
            None
        }
    });

    let mut photos_total = 0;
    let mut photos_added = 0;
    let mut photos_failed = 0;

    for entry in file_entries {
        if interrupted.load(Ordering::SeqCst) {
            info!("Scanning interrupted");
            break;
        }
        
        photos_total += 1;

        let photo_path = entry.into_path();
        let relative = photo_path.strip_prefix(&library.root_dir).unwrap();
        match relative.to_str() {
            None => {
                photos_failed += 1;
                warn!(
                    "Could not read photo with non-UTF-8 path {}",
                    relative.to_string_lossy()
                );
            }
            Some(path_str) => {
                if meta_db.query_photo_id_by_path(path_str)?.is_none() {
                    info!("New photo: {}", relative.to_string_lossy());

                    let info = supported_formats
                        .iter()
                        .filter(|format| format.supported_extension(&photo_path))
                        .find_map(|format| match format.read_info(&photo_path) {
                            Ok(info) => Some(info),
                            Err(err) => {
                                error!("{} error: {}", format.name(), err);
                                None
                            }
                        });

                    if let Some(info) = info {
                        photos_added += 1;
                        meta_db.insert_photo(path_str, &info)?;
                    } else {
                        photos_failed += 1;
                        error!("Failed to index photo {}", relative.to_string_lossy());
                    }
                };
            }
        }

        debug!("Processing photo {}", photo_path.to_string_lossy());
    }

    info!(
        "Scanning done ({} total, {} added, {} failed)",
        photos_total, photos_added, photos_failed
    );

    Ok(())
}