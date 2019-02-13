
extern crate bit_set;
extern crate chrono;
extern crate directories;
extern crate lru;
extern crate rusqlite;
extern crate walkdir;

extern crate cairo;
extern crate gdk;
extern crate gdk_pixbuf;
extern crate glib;
extern crate gtk;
extern crate gio;

extern crate image;
extern crate exif;

#[macro_use]
extern crate log;
extern crate env_logger;

use std::path::Path;

use gio::prelude::*;
use gtk::prelude::*;

mod adapters;
mod library;
#[macro_use]
mod util;
mod ui;
mod database;

fn build_ui(application: &gtk::Application) {
    let glade_src = include_str!("../resources/ui.glade");
    let builder = gtk::Builder::new_from_string(glade_src);

    let window: gtk::ApplicationWindow = builder.get_object("main_window").unwrap();

    window.set_application(application);
    window.connect_delete_event(move |win, _| {
        win.destroy();
        Inhibit(false)
    });

    let main_pane: gtk::Paned = builder.get_object("main_pane").unwrap();

    let user_dirs = directories::UserDirs::new().expect("Cannot access user directories");
    let photo_path = user_dirs.picture_dir().expect("Picture directory not found");
    let photo_root = Path::new(photo_path);
    let photo_lib = library::Library::open(photo_root).unwrap();
    photo_lib.refresh().unwrap();
    let arc_photo_lib = std::sync::Arc::new(photo_lib);

    let gallery = ui::gallery::Gallery::new(adapters::image_provider::LibImageProvider::new(arc_photo_lib));

    main_pane.add2(gallery.as_ref());

    window.show_all();
}

fn main() {
    env_logger::init_from_env(env_logger::Env::new()
        .filter("PHOTO_LIBRARY_LOG")
        .write_style("PHOTO_LIBRARY_LOG_STYLE")
    );

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
