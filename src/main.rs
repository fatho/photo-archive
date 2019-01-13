extern crate chrono;
extern crate walkdir;

extern crate cairo;
extern crate gdk;
extern crate glib;
extern crate gtk;
extern crate gio;

extern crate exif;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::io::{self, Read};
use std::path::Path;

use gio::prelude::*;
use gtk::prelude::*;

mod library;
#[macro_use]
mod util;

fn build_ui(application: &gtk::Application) {
    let glade_src = include_str!("../resources/ui.glade");
    let builder = gtk::Builder::new_from_string(glade_src);

    let window: gtk::ApplicationWindow = builder.get_object("main_window").unwrap();

    window.set_application(application);
    window.connect_delete_event(move |win, _| {
        win.destroy();
        Inhibit(false)
    });

    // create image list widget
    let image_list_draw: gtk::DrawingArea = builder.get_object("image_list_draw").unwrap();
    let image_list_viewport: gtk::Viewport = builder.get_object("image_list_viewport").unwrap();
    let image_list_scroll: gtk::ScrolledWindow = builder.get_object("image_list_scroll").unwrap();

    //image_list.set_hexpand(true);
    // image_list.set_vexpand(false);
    //image_list.set_can_focus(true);

    let test_image_count = 1000;
    let img_width = 300f64;
    let img_height = 200f64;

    //image_list_draw.set_size_request(img_width as i32, img_height as i32 * test_image_count as i32);
    image_list_draw.connect_configure_event(clone!(image_list_draw => move |this, evt| {
        let (cfg_width, cfg_height) = evt.get_size();
        let xcount = (cfg_width / img_width as u32).max(1);
        let ycount = (test_image_count + xcount - 1) / xcount;
        let ysize = ycount * (img_height as u32);
        println!("configure: old height={} -> new height={}", cfg_height, ysize);
        if ysize != cfg_height {
            this.set_size_request(-1, ysize as i32);
            gtk::idle_add(clone!(image_list_draw => move || {
                image_list_draw.queue_resize();
                glib::source::Continue(false)
            }));
        }
        false
    }));

    image_list_draw.connect_size_allocate(move |this, size| {
        println!("size-allocate: {:?}", size);
    });

    image_list_draw.connect_event(|this, event| {
        let t =  event.get_event_type();
        if t == gdk::EventType::MotionNotify || t == gdk::EventType::Scroll {
            return Inhibit(false);
        }
        println!("image_list_draw: {:?}", event.get_event_type());
        Inhibit(false)
    });

    image_list_viewport.connect_event(|this, event| {
        let t =  event.get_event_type();
        if t == gdk::EventType::MotionNotify || t == gdk::EventType::Scroll {
            return Inhibit(false);
        }
        println!("image_list_viewport: {:?}", event.get_event_type());
        Inhibit(false)
    });

    image_list_scroll.connect_event(|this, event| {
        let t =  event.get_event_type();
        if t == gdk::EventType::MotionNotify || t == gdk::EventType::Scroll {
            return Inhibit(false);
        }
        println!("image_list_scroll: {:?}", event.get_event_type());
        Inhibit(false)
    });

    let scroll_parent = image_list_scroll.clone();
    image_list_draw.connect_draw(move |this, context| {
        let (_clip_start_x, clip_start_y, _clip_end_x, clip_end_y) = context.clip_extents();

        let offset = scroll_parent.get_vadjustment().unwrap().get_value();
        let height = scroll_parent.get_vadjustment().unwrap().get_page_size();

        let draw_rect = context.clip_extents();

        // size of the drawn images
        let width = this.get_allocation().width as f64;
        let xcount = (width / img_width).floor() as u32;
        let ycount = test_image_count / xcount;
        let last_y_xcount = test_image_count % xcount;

        context.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        context.set_line_width(2.0);

        let y_idx_start = (clip_start_y / img_height).floor() as u32;
        let y_idx_end = ((clip_end_y / img_height).ceil() as u32).min(ycount);

        let mut painted: u32 = 0;
        for y in y_idx_start..=y_idx_end {
            let cur_xcount = if y < ycount {
                xcount
            } else {
                last_y_xcount
            };
            for x in 0..cur_xcount {
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
                painted += 1;
            }
        }

        // println!("Painted {} images at offset {} height {}, clip {:?}", painted, offset, height, draw_rect);
        Inhibit(false)
    });

    // TODO: invalidate drawing area on scroll

    window.show_all();
}

fn main() {
    env_logger::init();
    let application = gtk::Application::new("me.thorand.photo-archive", gio::ApplicationFlags::empty())
        .expect("Initialization failed...");

    application.connect_startup(build_ui);
    application.connect_activate(|_| {});

    application.run(&std::env::args().collect::<Vec<_>>());

    // let root = Path::new("/home/fatho/Pictures");
    // let lib = library::Library::open(root).unwrap();

    // println!("Library scanned!");

    // let mut buffer = String::new();
    // io::stdin().read_line(&mut buffer)?;
    // println!("{:?}", lib);
}
