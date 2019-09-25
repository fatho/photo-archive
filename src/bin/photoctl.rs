use photo_archive::library::LibraryFiles;

use directories;
use log::{debug, error, info, warn};
use std::io;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

mod cli;

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
    /// Display statistics and photodb information about the photo library.
    Status,
    /// Operate on the photo database
    Photos {
        #[structopt(subcommand)]
        command: PhotosCommand,
    },
    /// Operate on the thumbnail database
    Thumbnails {
        #[structopt(subcommand)]
        command: ThumbnailsCommand,
    },
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
        #[allow(clippy::option_option)] // we need to distinguish ``, `-j` and `-j <num>`
        jobs: Option<Option<usize>>,
        /// Also scan files that alrady exist in the database
        #[structopt(short, long)]
        rescan: bool,
        /// The paths to scan. Must be contained within the library root path.
        /// If no paths are specified, the whole library is rescanned.
        #[structopt(parse(from_os_str))]
        paths: Vec<PathBuf>,
    },
}

#[derive(Debug, StructOpt)]
enum ThumbnailsCommand {
    /// Remove all cached thumbnail images, cannot be undone.
    Delete,
    /// Generate thumbnails for images in the photo database
    Generate {
        #[structopt(short, long)]
        /// Generate thumbnails also for images that already have one.
        regenerate: bool,
        #[structopt(short = "f", long)]
        /// Generate thumbnails also for images where thumbnail generation previously failed.
        retry_failed: bool,
    },
    /// Remove cached thumbnails that are no longer referenced from a photo
    Gc,
}

fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stderr,
    )
    .unwrap();

    let opts = GlobalOpts::from_args();

    debug!("Options: {:?}", opts);

    // Defer the actual work to `run` so that all destructors of relevant objects
    // such as the sqlite connection can still run before exiting the process.
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

/// Dispatch work to other functions based on the program options that were given.
/// In case of a failure, it returns an error.
/// Exit is not called and a Ctrl+C handler is installed.
fn run(opts: GlobalOpts) -> Result<(), failure::Error> {
    let mut context = cli::AppContext::new();

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
        Command::Init { overwrite } => cli::init(&library_files, *overwrite),
        Command::Status => cli::status(&library_files),
        Command::Photos { command } => match command {
            PhotosCommand::List => cli::photos::list(&mut context, &library_files),
            PhotosCommand::Scan {
                jobs,
                rescan,
                paths,
            } => {
                let num_threads = jobs.map(|count| count.unwrap_or_else(num_cpus::get).min(1024));
                let paths_to_scan: Vec<&Path> = if paths.is_empty() {
                    vec![&library_files.root_dir]
                } else {
                    paths
                        .iter()
                        .filter_map(|path| {
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
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "No valid paths specified",
                    )
                    .into());
                }
                cli::photos::scan(
                    &mut context,
                    &library_files,
                    num_threads,
                    *rescan,
                    &paths_to_scan,
                )
            }
        },
        Command::Thumbnails { command } => match command {
            ThumbnailsCommand::Generate {
                regenerate,
                retry_failed,
            } => cli::thumbs::generate(&mut context, &library_files, *regenerate, *retry_failed),
            ThumbnailsCommand::Gc => Ok(()),
            ThumbnailsCommand::Delete => cli::thumbs::delete(&mut context, &library_files),
        },
    }
}
