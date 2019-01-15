//! ImageProvider for the image database.

use crate::ui::gallery::ImageProvider;
use crate::library::db;

use std::vec::Vec;
use std::sync::{Arc};
use image::GenericImageView;
use gdk::ContextExt;

pub struct DbImageProvider {
    photo_db: Arc<db::PhotoDatabase>,
    visible_photos: Vec<db::PhotoId>,
    thumb_cache: std::cell::RefCell<lru::LruCache<u32, cairo::ImageSurface>>,
}

impl DbImageProvider {
    pub fn new(db: Arc<db::PhotoDatabase>) -> Self {
        let photos = db.all_photos().unwrap();
        Self {
            photo_db: db,
            visible_photos: photos,
            thumb_cache: std::cell::RefCell::new(lru::LruCache::new(200)),
        }
    }

    fn error_surf() -> cairo::ImageSurface {
        let surf = cairo::ImageSurface::create(cairo::Format::Rgb24, 64, 64).unwrap();
        let context = cairo::Context::new(&surf);
        context.set_source_rgb(1.0, 0.0, 0.0);
        context.paint();
        return surf;
    }
}

impl ImageProvider for DbImageProvider {
    fn image_count(&self) -> u32 {
        self.visible_photos.len() as u32
    }

    fn get_image(&self, index: u32) -> cairo::ImageSurface {
        if index as usize >= self.visible_photos.len() {
            return Self::error_surf()
        }

        let photo = self.visible_photos[index as usize];
        let mut cache = self.thumb_cache.borrow_mut();
        if let Some(value) = cache.get(&index) {
            debug!("Retrieved image {:?} from cache", photo);
            value.clone()
        } else {
            debug!("Generating image {:?}", photo);

            if let Some(thumb) = self.photo_db.get_thumbnail(photo).unwrap() {
                if let Ok(img) = image::load_from_memory(thumb.as_jpg()) {
                    let width = img.width();
                    let height = img.height();
                    let pb = gdk_pixbuf::Pixbuf::new_from_vec(img.to_rgb().into_raw(), gdk_pixbuf::Colorspace::Rgb, false, 8, width as i32, height as i32, width as i32 * 3);

                    let surf = cairo::ImageSurface::create(cairo::Format::Rgb24, width as i32, height as i32).unwrap();
                    let context = cairo::Context::new(&surf);
                    context.set_source_pixbuf(&pb, 0.0, 0.0);
                    context.paint();
                    drop(context);
                    return surf
                }
            }
            return Self::error_surf();
        }
    }
}