//! ImageProvider for the image database.

use crate::ui::gallery::ImageProvider;
use crate::library::Library;
use crate::library::meta::{PhotoId};
use crate::library::thumb::{Thumbnail};

use std::vec::Vec;
use std::sync::{Arc};
use image::GenericImageView;
use gdk::ContextExt;

pub struct LibImageProvider {
    library: Arc<Library>,
    shown_photos: Vec<PhotoId>,
    image_cache: std::cell::RefCell<lru::LruCache<PhotoId, cairo::ImageSurface>>,
}

impl LibImageProvider {
    pub fn new(library: Arc<Library>) -> Self {
        let photos = library.meta_db().all_photos().unwrap();
        LibImageProvider {
            library: library,
            shown_photos: photos,
            image_cache: std::cell::RefCell::new(lru::LruCache::new(1000)),
        }
    }

    pub fn shown_photos(&self) -> &[PhotoId] {
        self.shown_photos.as_ref()
    }

    pub fn set_shown_photos<I: IntoIterator<Item=PhotoId>>(&mut self, photos: I) {
        self.shown_photos.clear();
        self.shown_photos.extend(photos)
    }

    /// Surface returned when an error occurs while fetching the actual image.
    fn error_surf() -> cairo::ImageSurface {
        let surf = cairo::ImageSurface::create(cairo::Format::Rgb24, 64, 64).unwrap();
        let context = cairo::Context::new(&surf);
        context.set_source_rgb(1.0, 0.0, 0.0);
        context.paint();
        return surf;
    }

    fn thumb_to_surface_cached(&self, photo: PhotoId, thumb: &Thumbnail) -> Option<cairo::ImageSurface> {
        if let Ok(img) = image::load_from_memory(thumb.as_jpg()) {
            let width = img.width();
            let height = img.height();
            debug!("Thumbnail size: {}x{}", width, height);
            let pb = gdk_pixbuf::Pixbuf::new_from_vec(img.to_rgb().into_raw(), gdk_pixbuf::Colorspace::Rgb, false, 8, width as i32, height as i32, width as i32 * 3);

            let surf = cairo::ImageSurface::create(cairo::Format::Rgb24, width as i32, height as i32).unwrap();
            let context = cairo::Context::new(&surf);
            context.set_source_pixbuf(&pb, 0.0, 0.0);
            context.paint();
            drop(context);
            self.image_cache.borrow_mut().put(photo, surf.clone());
            Some(surf)
        } else {
            None
        }
    }
}

impl ImageProvider for LibImageProvider {
    fn image_count(&self) -> u32 {
        self.shown_photos.len() as u32
    }

    fn get_image(&self, index: u32) -> cairo::ImageSurface {
        if index as usize >= self.shown_photos.len() {
            return Self::error_surf()
        }

        let photo = self.shown_photos[index as usize];
        let mut cache = self.image_cache.borrow_mut();
        if let Some(value) = cache.get(&photo) {
            debug!("Retrieved thumbnail {:?} from cache", photo);
            value.clone()
        } else {
            debug!("Loading thumbnail {:?}", photo);

            if let Ok(maybe_thumb) = self.library.thumb_db().get_thumbnail(photo) {
                if let Some(thumb) = maybe_thumb {
                    if let Some(surf) = self.thumb_to_surface_cached(photo, &thumb) {
                        return surf;
                    }
                } else {
                    // TODO: generate thumbnail on demand in the background and ask for refresh later
                }
            }
            return Self::error_surf();
        }
    }
}
