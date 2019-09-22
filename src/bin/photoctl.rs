use photo_archive::library::{LibraryFiles, meta, thumb};

use directories;
use failure::bail;
use log::{debug, error, info};
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

    match opts.command {
        Command::Init { overwrite } => init(&library_files, overwrite),
        Command::Scan => {
            unimplemented!()
        },
        Command::Status => {
            println!("Library status");
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

            print_status("Meta", &library_files.meta_db_file, library_files.meta_db_exists());
            if library_files.meta_db_exists() {
                let _meta_db = meta::MetaDatabase::open_or_create(&library_files.meta_db_file)?;
            }

            print_status("Thumb", &library_files.thumb_db_file, library_files.thumb_db_exists());
            if library_files.thumb_db_exists() {
                let _thumb_db = meta::MetaDatabase::open_or_create(&library_files.meta_db_file)?;
            }

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
