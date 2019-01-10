extern crate chrono;
extern crate walkdir;

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

fn build_ui(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);

    window.set_title("First GTK+ Program");
    window.set_border_width(10);
    window.set_position(gtk::WindowPosition::Center);
    window.set_default_size(350, 70);

    window.connect_delete_event(move |win, _| {
        win.destroy();
        Inhibit(false)
    });

    let button = gtk::Button::new_with_label("Click me!");

    window.add(&button);

    window.show_all();
}

fn main() -> io::Result<()> {
    env_logger::init();
    // let application = gtk::Application::new("me.thorand.photo-archive", gio::ApplicationFlags::empty())
    //     .expect("Initialization failed...");

    // application.connect_startup(build_ui);
    // application.connect_activate(|_| {});

    // application.run(&std::env::args().collect::<Vec<_>>());

    let root = Path::new("/home/fatho/Pictures");
    let lib = library::Library::open(root).unwrap();

    println!("Library scanned!");

    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;
    println!("{:?}", lib);

    Ok(())
}
