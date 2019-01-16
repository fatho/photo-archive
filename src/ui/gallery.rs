//! A widget for displaying a gallery of images base on a gtk::DrawingArea inside a gtk::ScrolledWindow.

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use cairo;
use gdk;
use gtk;

use gio::prelude::*;
use gtk::prelude::*;

pub struct Gallery<T> {
    drawing_area: gtk::DrawingArea,
    viewport: gtk::Viewport,
    scrolled_window: gtk::ScrolledWindow,
    properties: Rc<RefCell<GalleryProperties>>,
    provider: Rc<T>,
}

impl<T> Clone for Gallery<T> {
    fn clone(&self) -> Self {
        Self {
            drawing_area: self.drawing_area.clone(),
            viewport: self.viewport.clone(),
            scrolled_window: self.scrolled_window.clone(),
            properties: self.properties.clone(),
            provider: self.provider.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct GalleryProperties {
    preferred_tile_width: u32,
    preferred_tile_height: u32,
    actual_tile_width: u32,
    actual_tile_height: u32,
    /// number of image tiles per row, computed dynamically
    tiles_per_row: u32,
    /// number of image rows, computed dynamically
    num_rows: u32,
    /// total number of tiles
    num_tiles: u32,
    /// New value to which the scrollbar value should be set on the next resize event.
    /// The reason is that the first resize event is used to recompute the new height,
    /// which then causes a second resize event. And only in the second event has the
    /// scrollbar been updated by the ScrolledWindow container.
    scrollbar_adjust: Option<f64>,
}

impl GalleryProperties {
    pub fn default() -> Self {
        GalleryProperties {
            preferred_tile_width: 300,
            preferred_tile_height: 200,
            actual_tile_width: 300,
            actual_tile_height: 200,
            tiles_per_row: 1,
            num_rows: 0,
            num_tiles: 0,
            scrollbar_adjust: None,
        }
    }
}

pub trait ImageProvider {
    fn image_count(&self) -> u32;

    fn get_image(&self, index: u32) -> cairo::ImageSurface;
}

impl<T> Gallery<T> where T: ImageProvider + 'static {
    pub fn new(provider: T) -> Self {
        let this = Self {
            drawing_area: gtk::DrawingArea::new(),
            viewport: gtk::Viewport::new(None, None),
            scrolled_window: gtk::ScrolledWindow::new(None, None),
            provider: Rc::new(provider),
            properties: Rc::new(RefCell::new(GalleryProperties::default())),
        };

        this.viewport.add(&this.drawing_area);
        this.scrolled_window.add(&this.viewport);
        this.scrolled_window.set_property_hscrollbar_policy(gtk::PolicyType::Never);

        this.notify_provider();

        this.drawing_area.connect_configure_event(clone!(this => move |_, evt| {
            this.on_drawing_configure_event(evt)
        }));

        this.drawing_area.connect_draw(clone!(this => move |_, context| {
            this.on_drawing_draw(context)
        }));

        this
    }

    /// Notify the image view that the data in the provider has changed.
    pub fn notify_provider(&self) {
        // number of tiles per row
        {
            let mut props = self.properties.borrow_mut();
            props.num_tiles = self.provider.image_count();
        }

        self.recompute_size(false);
    }

    fn recompute_tiles(&self) {
        // compute tile size
        let width = self.drawing_area.get_allocated_width().max(0) as u32;
        // number of tiles per row
        let mut props = self.properties.borrow_mut();
        let xcount = (width / props.preferred_tile_width).max(1);
        // number of rows, accomodating for a possible partial row in the end
        let ycount = props.num_tiles / xcount;
        let extras = if props.num_tiles % xcount > 0 { 1 } else { 0 };

        props.tiles_per_row = xcount;
        props.num_rows = ycount + extras;

        // divide additional horizontal space across all tiles and expand vertically accordingly.
        let extra_space = width.saturating_sub(xcount * props.preferred_tile_width);
        let per_tile_extra = extra_space / xcount;

        props.actual_tile_width = props.preferred_tile_width + per_tile_extra;
        props.actual_tile_height = props.preferred_tile_height + (per_tile_extra * props.preferred_tile_height) / props.preferred_tile_width;
    }

    pub fn recompute_size(&self, queue_indirect: bool) {
        self.recompute_tiles();

        let mut props = self.properties.borrow_mut();
        let ycount = props.num_rows;
        let row_height = props.actual_tile_height;

        // compute pixel size
        let height = self.drawing_area.get_allocated_height().max(0) as u32;
        let computed_height = ycount * row_height;

        if computed_height != height {
            // width -1 means: take the whole width of the parent
            self.drawing_area.set_size_request(-1, computed_height as i32);
            // adjust scrollbar
            props.scrollbar_adjust = self.scrolled_window.get_vadjustment()
                .map(|adj| adj.get_value() * computed_height as f64 / height as f64);
            debug!("Queuing scrollbar adjustment {:?}", &props.scrollbar_adjust);

            if queue_indirect {
                // Schedule recomputation on message queue, because we cannot request a
                // recomputation while a resize operation is still going on.
                // Theoretically, it should be possible to sub-class the DrawingArea and
                // override the methods for size computation, but that seems hard to do in Rust,
                // hence this workaround.
                let image_list_draw = self.drawing_area.clone();
                gtk::idle_add(move || {
                    image_list_draw.queue_resize();
                    glib::source::Continue(false)
                });
            } else {
                self.drawing_area.queue_resize();
            }
        } else {
            if let Some(new_value) = props.scrollbar_adjust.take() {
                if let Some(adj) = self.scrolled_window.get_vadjustment() {
                    debug!("Performing scrollbar adjustment to {}/{}", new_value, adj.get_upper());
                    adj.set_value(new_value);
                }
            }
        }
    }

    // Event handlers

    fn on_drawing_configure_event(&self, _evt: &gdk::EventConfigure) -> bool {
        self.recompute_size(true);
        false
    }

    fn on_drawing_draw(&self, context: &cairo::Context) -> gtk::Inhibit {
        // extract the area that needs to be redrawn
        let (clip_start_x, clip_start_y, clip_end_x, clip_end_y) = context.clip_extents();

        // clear background
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.paint();

        // size of the drawn images
        let props = self.properties.borrow();
        let tile_width = props.actual_tile_width as f64;
        let tile_height = props.actual_tile_height as f64;

        // layout of tiles to render
        let xcount = props.tiles_per_row;
        let ycount = props.num_rows;
        let last_y_xcount = props.num_tiles % xcount;

        // determine which tiles have to be redrawn
        let x_idx_start = (clip_start_x / tile_width).floor() as u32;
        let x_idx_end = ((clip_end_x / tile_width).ceil() as u32).min(xcount);

        let y_idx_start = (clip_start_y / tile_height).floor() as u32;
        let y_idx_end = ((clip_end_y / tile_height).ceil() as u32).min(ycount);

        for y in y_idx_start..y_idx_end {
            let cur_xcount = if y < ycount - 1 {
                xcount
            } else {
                last_y_xcount
            };

            for x in x_idx_start..cur_xcount.min(x_idx_end) {
                let (fx, fy) = (x as f64, y as f64);
                let index = y * xcount + x;

                // draw the image
                let surf = self.provider.get_image(index);
                let img_width = surf.get_width() as f64;
                let img_height = surf.get_height() as f64;
                if img_height <= tile_height && img_width <= tile_width {
                    let target_x = fx * tile_width + (tile_width - img_width) / 2.0;
                    let target_y = fy * tile_height + (tile_height - img_height) / 2.0;
                    trace!("Render {} unscaled", index);
                    context.set_source_surface(&*surf, target_x, target_y);
                    context.paint()
                } else {
                    context.save();
                    if img_width / img_height >= tile_width / tile_height {
                        // fit width
                        let scale = tile_width / img_width;
                        trace!("Fit {} to width, scale={}", index, scale);
                        let target_y = fy * tile_height + (tile_height - img_height * scale) / 2.0;

                        context.scale(scale, scale);
                        context.set_source_surface(&*surf, fx * tile_width / scale, target_y / scale);
                    } else {
                        // fit height
                        let scale = tile_height / img_height;
                        trace!("Fit {} to height, scale={}", index, scale);
                        let target_x = fx * tile_width + (tile_width - img_width * scale) / 2.0;

                        context.scale(scale, scale);
                        context.set_source_surface(&*surf, target_x / scale, fy * tile_height / scale);
                    }
                    context.paint();
                    context.restore();
                }
            }
        }

        Inhibit(false)
    }
}

impl<T> AsRef<gtk::Widget> for Gallery<T> {
    fn as_ref(&self) -> &gtk::Widget {
        self.scrolled_window.upcast_ref()
    }
}