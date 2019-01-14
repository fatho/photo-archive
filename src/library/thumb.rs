//! Thumbnail generation

use std::path::Path;
use image::GenericImageView;

/// A JPEG encoded thumbnail image.
pub struct Thumbnail(std::vec::Vec<u8>);

impl Thumbnail {
    /// Generate a thumbnail image where the longest side has at most the given size.
    pub fn new<P: AsRef<Path>>(original_file: P, size: u32) -> super::Result<Thumbnail> {
        let img = image::open(original_file)?;

        let width = img.width();
        let height = img.height();

        let new_img = if width > size || height > size {
            img.resize(size, size, image::imageops::FilterType::Triangle)
        } else {
            img
        };

        let mut jpg = std::vec::Vec::new();
        new_img.write_to(&mut jpg, image::ImageOutputFormat::JPEG(90))?;

        Ok(Thumbnail(jpg))
    }

    /// Return a JPG encoded version of the thumbnail.
    pub fn as_jpg(&self) -> &[u8] {
        self.0.as_ref()
    }
}