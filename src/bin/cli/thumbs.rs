//! CLI implementation for the thumbs subcommand.

use crate::cli;
use log::info;
use photo_archive::formats;
use photo_archive::library::{photodb, LibraryFiles};
use std::path::Path;

/// Remove all thumbnails
pub fn delete(context: &mut cli::AppContext, library: &LibraryFiles) -> Result<(), failure::Error> {
    let db = photodb::PhotoDatabase::open_or_create(&library.photo_db_file)?;
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
    let db = photodb::PhotoDatabase::open_or_create(&library.photo_db_file)?;

    let all_photos = db.query_all_photo_ids()?;

    info!("Collecting photos to process");

    context.progress().begin_progress(all_photos.len());

    // compute the set of photos for which thumbnails need to be generated
    let mut photo_queue = Vec::new();
    for photo in db.query_all_photos()? {
        context.progress().inc_progress(1);
        if context.check_interrupted().is_err() {
            // Don't return yet so that we can clean up the progress bar
            break;
        }
        let state = db.query_thumbnail_state(photo.id)?;
        if state == photodb::ThumbnailState::Absent
            || (state == photodb::ThumbnailState::Present && regenerate)
            || (state == photodb::ThumbnailState::Error && retry_failed)
        {
            photo_queue.push(photo);
        }
    }

    context.progress().end_progress();
    context.check_interrupted()?;

    info!(
        "Generating thumbnail images for {} photos",
        photo_queue.len()
    );

    context.progress().begin_progress(photo_queue.len());

    // actually generate the thumbnails
    for photo in photo_queue {
        context.progress().inc_progress(1);
        if context.check_interrupted().is_err() {
            // Don't return yet so that we can clean up the progress bar
            break;
        }

        let full_path = library.root_dir.join(Path::new(&photo.relative_path));
        // TODO: add option for thumbnail size
        let thumbnail_size = 400;
        let thumbnail_result =
            formats::Thumbnail::generate(&full_path, thumbnail_size).map_err(|e| format!("{}", e));
        db.insert_thumbnail(photo.id, &thumbnail_result)?;
    }

    context.progress().end_progress();
    context.check_interrupted()?;

    info!("Thumbnail image generation done");

    Ok(())
}
