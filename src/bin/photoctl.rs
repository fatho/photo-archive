use photo_archive::library::LibraryFiles;

use directories;
use log::{debug, error, info, warn};
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

mod cli;
mod progresslog;

#[derive(Debug, StructOpt)]
#[structopt(about = "photoctl - command line photo library manager")]
struct GlobalOpts {
    #[structopt(short, long, parse(from_os_str))]
    /// The root directory of the photo library to be used, if it is not the user's photo directory.
    photo_root: Option<PathBuf>,

    /// How verbose should the log output be. Valid values are `error`, `warn`, `info`, `debug`, `trace` and `off`.
    #[structopt(short, long, default_value = "info")]
    verbosity: log::LevelFilter,

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
    /// Generate shell completion values.
    Completion {
        /// The shell for which the completions should be generated.
        #[structopt(short, long)]
        shell: structopt::clap::Shell,
    },
    Browse {
        /// On which addresses the web server should listen.
        #[structopt(short, long, default_value = "localhost:8076")]
        bind: Vec<String>,

        /// The source path from where the web frontend is hosted.
        ///
        /// If it is not specified, the resources that where compiled into photoctl are used.
        /// This option is mainly useful for development, because it allows working on the
        /// frontend without recompiling the Rust part of the application.
        #[structopt(short, long, parse(from_os_str))]
        web_root: Option<PathBuf>,
    },
}

#[derive(Debug, StructOpt)]
enum PhotosCommand {
    /// List all photos in the database
    List,
    /// Scan the library for new and updated photos.
    Scan {
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
}

fn main() {
    let opts = GlobalOpts::from_args();

    let progress_logger = progresslog::TermProgressLogger::init(opts.verbosity).unwrap();
    let mut context = cli::AppContext::new(progress_logger);

    debug!("Options: {:?}", opts);

    // Defer the actual work to `run` so that all destructors of relevant objects
    // such as the sqlite connection can still run before exiting the process.
    match run(opts, &mut context) {
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
fn run(opts: GlobalOpts, context: &mut cli::AppContext) -> Result<(), failure::Error> {
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

    match opts.command {
        Command::Init { overwrite } => cli::init(&library_files, overwrite),
        Command::Status => cli::status(&library_files),
        Command::Photos { command } => match command {
            PhotosCommand::List => cli::photos::list(context, &library_files),
            PhotosCommand::Scan { rescan, paths } => {
                let paths_to_scan: Vec<PathBuf> = if paths.is_empty() {
                    vec![library_files.root_dir.clone()]
                } else {
                    paths
                        .iter()
                        .filter_map(|path| {
                            if path.strip_prefix(&library_files.root_dir).is_ok() {
                                Some(path.clone())
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
                cli::photos::scan(context, &library_files, rescan, &paths_to_scan)
            }
        },
        Command::Thumbnails { command } => match command {
            ThumbnailsCommand::Generate {
                regenerate,
                retry_failed,
            } => cli::thumbs::generate(context, &library_files, regenerate, retry_failed),
            ThumbnailsCommand::Delete => cli::thumbs::delete(context, &library_files),
        },
        Command::Completion { shell } => {
            GlobalOpts::clap().gen_completions_to(
                "photoctl",
                shell,
                &mut std::io::stdout().lock(),
            );
            Ok(())
        }
        Command::Browse { bind, web_root } => cli::browse::browse(context, &library_files, &bind, web_root),
    }
}
