use log::debug;
use std::io;
use std::path::Path;

/// Taken from https://gtk-rs.org/tuto/closures for easily cloning everything that is moved into a closure.
#[macro_export]
macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}

#[derive(Debug, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone)]
pub struct Rect {
    pub top_left: Point,
    pub size: Size,
}

impl Point {
    #[inline(always)]
    pub fn offset(&self, x_off: f64, y_off: f64) -> Point {
        Point {
            x: self.x + x_off,
            y: self.y + y_off,
        }
    }
}

impl Size {
    /// Check whether the other size fits completely into this size.
    #[inline(always)]
    pub fn contains(&self, other: &Size) -> bool {
        other.w <= self.w && other.h <= self.h
    }

    /// Scale the current size so that self.w = target.w or self.h = target.h, while keeping the aspect ratio of self.
    #[inline(always)]
    pub fn scale_to_fit(&self, target: &Size) -> Size {
        if target.w / target.h > self.w / self.h {
            // target is wider, keep height, scale width
            Size {
                w: target.h / self.h * self.w,
                h: target.h,
            }
        } else {
            // target is taller, keep width, scale height
            Size {
                w: target.w,
                h: target.w / self.w * self.h,
            }
        }
    }
}

impl Rect {
    #[inline(always)]
    pub fn centered(&self, size: &Size) -> Rect {
        Rect {
            top_left: Point {
                x: self.top_left.x + (self.size.w - size.w) / 2.0,
                y: self.top_left.y + (self.size.h - size.h) / 2.0,
            },
            size: size.clone(),
        }
    }
}

/// Create a backup of a file, appending `<NUM>.bak` to the while
/// with `<NUM>` being the smallest number such that the resulting file name doesn't exist.
pub fn backup_file(file_path: &Path, rename: bool) -> Result<(), io::Error> {
    for bak_num in 0..10 {
        let mut name = file_path.file_name().unwrap().to_os_string();
        name.push(format!(".{}.bak", bak_num));
        let bak_file = file_path.with_file_name(&name);

        if bak_file.exists() {
            debug!("Backup {} already exists", bak_file.to_string_lossy());
        } else {
            debug!("Backup name available {}", bak_file.to_string_lossy());
            let result = if rename {
                std::fs::rename(file_path, &bak_file)
            } else {
                std::fs::copy(file_path, &bak_file).map(|_| ())
            };
            return result;
        }
    }
    Err(io::Error::new(io::ErrorKind::Other, "Too many backups"))
}
