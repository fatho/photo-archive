//! A widget for displaying a list of images base on a gtk::DrawingArea inside a gtk::ScrolledWindow.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use cairo;
use gdk;
use gtk;

use gio::prelude::*;
use gtk::prelude::*;

#[derive(Clone)]
pub struct ImageList {
    drawing_area: gtk::DrawingArea,
    viewport: gtk::Viewport,
    scrolled_window: gtk::ScrolledWindow,
    provider: Arc<Mutex<ImageProvider>>,
    properties: Rc<RefCell<ImageListProperties>>,
}

#[derive(Clone, Debug)]
struct ImageListProperties {
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
}

impl ImageListProperties {
    pub fn default() -> Self {
        ImageListProperties {
            preferred_tile_width: 300,
            preferred_tile_height: 200,
            actual_tile_width: 300,
            actual_tile_height: 200,
            tiles_per_row: 1,
            num_rows: 0,
            num_tiles: 0,
        }
    }
}

pub trait ImageProvider {
    fn image_count(&self) -> u32;
}

impl ImageList {
    pub fn new<T: ImageProvider + 'static>(provider: T) -> Self {
        let this = Self {
            drawing_area: gtk::DrawingArea::new(),
            viewport: gtk::Viewport::new(None, None),
            scrolled_window: gtk::ScrolledWindow::new(None, None),
            provider: Arc::new(Mutex::new(provider)),
            properties: Rc::new(RefCell::new(ImageListProperties::default())),
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
            props.num_tiles = self.provider.lock().unwrap().image_count();
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

        let extra_space = width.saturating_sub(xcount * props.preferred_tile_width);
        let per_tile_extra = extra_space / xcount;

        props.actual_tile_width = props.preferred_tile_width + per_tile_extra;
        props.actual_tile_height = props.preferred_tile_height + (per_tile_extra * props.preferred_tile_height) / props.preferred_tile_width;
    }

    pub fn recompute_size(&self, queue_indirect: bool) {
        self.recompute_tiles();

        // compute pixel size
        let height = self.drawing_area.get_allocated_height().max(0) as u32;
        let computed_height = self.get_height();
        println!("recompute_size, height={}, computed_height={}", height, computed_height);
        if computed_height != height {
            self.drawing_area.set_size_request(-1, computed_height as i32);
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
        }
    }

    // Utility functions

    fn get_height(&self) -> u32 {
        let props = self.properties.borrow();
        let ycount = props.num_rows;
        let row_height = props.actual_tile_height;

        ycount * row_height
    }

    // Event handlers

    fn on_drawing_configure_event(&self, _evt: &gdk::EventConfigure) -> bool {
        self.recompute_size(true);
        false
    }

    fn on_drawing_draw(&self, context: &cairo::Context) -> gtk::Inhibit {
        // extract the area that needs to be redrawn
        let (clip_start_x, clip_start_y, clip_end_x, clip_end_y) = context.clip_extents();

        // size of the drawn images
        let props = self.properties.borrow();
        let img_width = props.actual_tile_width as f64;
        let img_height = props.actual_tile_height as f64;

        // number of tiles to render
        let xcount = props.tiles_per_row;
        let ycount = props.num_rows;
        let last_y_xcount = props.num_tiles % xcount;

        let x_idx_start = (clip_start_x / img_width).floor() as u32;
        let x_idx_end = ((clip_end_x / img_width).ceil() as u32).min(xcount);

        let y_idx_start = (clip_start_y / img_height).floor() as u32;
        let y_idx_end = ((clip_end_y / img_height).ceil() as u32).min(ycount);

        // placeholder draw style
        context.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        context.set_line_width(2.0);

        for y in y_idx_start..y_idx_end {
            let cur_xcount = if y < ycount - 1 {
                xcount
            } else {
                last_y_xcount
            };

            for x in x_idx_start..cur_xcount.min(x_idx_end) {
                // draw a placeholder
                let (fx, fy) = (x as f64, y as f64);
                context.move_to(fx * img_width, fy * img_height);
                context.line_to(fx * img_width + img_width, fy * img_height);
                context.line_to(fx * img_width + img_width, fy * img_height + img_height);
                context.line_to(fx * img_width, fy * img_height + img_height);
                context.move_to(fx * img_width, fy * img_height);
                context.stroke();
                context.move_to(fx * img_width + img_width / 2.0, fy * img_height + img_height / 2.0);
                let s = format!("x: {} y: {} idx: {}", x, y, y * xcount + x);
                context.show_text(s.as_ref());
            }
        }

        Inhibit(false)
    }
}

impl AsRef<gtk::Widget> for ImageList {
    fn as_ref(&self) -> &gtk::Widget {
        self.scrolled_window.upcast_ref()
    }
}