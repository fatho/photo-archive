//! CLI implementation for the thumbs subcommand.

use crate::cli;
use failure::format_err;
use log::info;
use photo_archive::formats;
use photo_archive::library::{LibraryFiles, PhotoDatabase, ThumbnailState};
use rayon::prelude::*;
use std::path::Path;
use std::sync::Mutex;

/// List all thumbnails and show statistics.
pub fn list(context: &mut cli::AppContext, library: &LibraryFiles, errors: bool) -> Result<(), failure::Error> {
    use std::fmt::Write;

    let db = PhotoDatabase::open_or_create(&library.photo_db_file)?;
    context.check_interrupted()?;

    let infos = db.query_thumbnail_infos()?;

    let mut line = String::new();

    println!("Photo\tRelative path\tSize\tHash\tError");

    for info in infos {
        line.clear();
        context.check_interrupted()?;

        if errors && ! info.error.is_some() {
            // TODO: filter in the database instead
            continue;
        }

        write!(&mut line, "{}\t{}\t", info.photo_id.0, &info.relative_path).unwrap();

        if let Some(size) = info.size_bytes {
            write!(&mut line, "{}\t", indicatif::HumanBytes(size as u64)).unwrap();
        } else {
            write!(&mut line, "n/a\t").unwrap();
        }

        if let Some(hash) = info.hash {
            write!(&mut line, "{:.8}..\t", hash).unwrap();
        } else {
            write!(&mut line, "n/a\t").unwrap();
        }

        write!(&mut line, "{}", info.error.unwrap_or(String::new())).unwrap();

        println!("{}", line);
    }

    Ok(())
}

/// Remove all thumbnails
pub fn delete(context: &mut cli::AppContext, library: &LibraryFiles) -> Result<(), failure::Error> {
    let db = PhotoDatabase::open_or_create(&library.photo_db_file)?;
    context.check_interrupted()?;

    info!("Deleting all thumbnails");
    db.delete_all_thumbnails()?;
    info!("Thumbnails deleted");
    Ok(())
}

/// Generate thumbnail image for all the photos currently stored in the photo database.
pub fn generate(
    context: &mut cli::AppContext,
    library: &LibraryFiles,
    regenerate: bool,
    retry_failed: bool,
) -> Result<(), failure::Error> {
    let photo_db = PhotoDatabase::open_or_create(&library.photo_db_file)?;

    let all_photos = photo_db.query_all_photo_ids()?;

    info!("Collecting photos to process");

    let progress_bar = context.progress().begin_progress(all_photos.len());

    // compute the set of photos for which thumbnails need to be generated
    let mut photo_queue = Vec::new();
    for photo in photo_db.query_all_photos()? {
        progress_bar.sender().inc_progress(1);
        if context.check_interrupted().is_err() {
            // Don't return yet so that we can clean up the progress bar
            break;
        }
        let state = photo_db.query_thumbnail_state(photo.id)?;
        if state == ThumbnailState::Absent
            || (state == ThumbnailState::Present && regenerate)
            || (state == ThumbnailState::Error && retry_failed)
        {
            photo_queue.push(photo);
        }
    }

    drop(progress_bar);
    context.check_interrupted()?;

    info!(
        "Generating thumbnail images for {} photos",
        photo_queue.len()
    );

    let progress_bar = context.progress().begin_progress(photo_queue.len());
    let synced_photo_db = Mutex::new(photo_db);

    // actually generate the thumbnails
    photo_queue
        .into_par_iter()
        .map(|photo| {
            context.check_interrupted()?;

            progress_bar.sender().inc_progress(1);

            let full_path = library.root_dir.join(Path::new(&photo.relative_path));
            // TODO: add option for thumbnail size
            let thumbnail_size = 400;
            let thumbnail_result = formats::Thumbnail::generate(&full_path, thumbnail_size)
                .map_err(|e| format!("{}", e));
            synced_photo_db
                .lock()
                .map_err(|_| format_err!("Database mutex was poisoned"))?
                .insert_thumbnail(photo.id, &thumbnail_result)
        })
        .collect::<Result<(), failure::Error>>()?;

    drop(progress_bar);
    context.check_interrupted()?;

    info!("Thumbnail image generation done");

    Ok(())
}
