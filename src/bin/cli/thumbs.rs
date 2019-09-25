//! CLI implementation for the thumbs subcommand.

use crate::cli;
use photo_archive::library::{meta, thumb, LibraryFiles};
use std::path::Path;
use log::info;

/// Remove all thumbnails
pub fn delete(
    context: &mut cli::AppContext,
    library: &LibraryFiles
) -> Result<(), failure::Error> {
    let thumb_db = thumb::ThumbDatabase::open_or_create(&library.thumb_db_file)?;
    context.check_interrupted()?;

    info!("Deleting all thumbnails");
    thumb_db.delete_all_thumbnails()?;
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
    let meta_db = meta::MetaDatabase::open_or_create(&library.meta_db_file)?;
    let thumb_db = thumb::ThumbDatabase::open_or_create(&library.thumb_db_file)?;

    let all_photos = meta_db.query_all_photo_ids()?;

    info!("Collecting photos to process");

    let collect_progress_bar = indicatif::ProgressBar::new(all_photos.len() as u64)
        .with_style(cli::PROGRESS_STYLE.clone());
    collect_progress_bar.set_message("Collecting photos");

    // compute the set of photos for which thumbnails need to be generated
    let mut photo_queue = Vec::new();
    for photo in meta_db.query_all_photos()? {
        collect_progress_bar.inc(1);
        if context.check_interrupted().is_err() {
            // Don't return yet so that we can clean up the progress bar
            break;
        }
        let state = thumb_db.query_thumbnail_state(photo.id)?;
        if state == thumb::ThumbnailState::Absent
            || (state == thumb::ThumbnailState::Present && regenerate)
            || (state == thumb::ThumbnailState::Error && retry_failed)
        {
            photo_queue.push(photo);
        }
    }

    collect_progress_bar.finish_and_clear();
    context.check_interrupted()?;

    info!("Generating thumbnail images for {} photos", photo_queue.len());

    let generate_progress_bar = indicatif::ProgressBar::new(photo_queue.len() as u64)
        .with_style(cli::PROGRESS_STYLE.clone());
    generate_progress_bar.set_message("Generating thumbnails");

    // actually generate the thumbnails
    for photo in photo_queue {
        generate_progress_bar.inc(1);
        if context.check_interrupted().is_err() {
            // Don't return yet so that we can clean up the progress bar
            break;
        }

        let full_path = library.root_dir.join(Path::new(&photo.relative_path));
        // TODO: add option for thumbnail size
        let thumbnail_result =
            thumb::Thumbnail::generate(&full_path, 400).map_err(|e| format!("{}", e));
        thumb_db.insert_thumbnail(photo.id, thumbnail_result.as_ref().map_err(|e| e.as_ref()))?;
    }

    generate_progress_bar.finish_and_clear();
    context.check_interrupted()?;

    info!("Thumbnail image generation done");

    Ok(())
}
