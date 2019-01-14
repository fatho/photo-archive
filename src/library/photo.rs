//! Functionality for reading a single photo file.

use std::path::{Path, PathBuf};
use chrono::TimeZone;

#[derive(Debug)]
pub struct Info {
    filename: PathBuf,
    created: PhotoTimestamp,
}

#[derive(Debug, Clone)]
pub enum PhotoTimestamp {
    /// There was no source for the creation timestamp
    None,
    /// The creation timestamp from the EXIF data was used
    Exif(chrono::NaiveDateTime),
    /// The file creation timestamp was used
    File(chrono::DateTime<chrono::Utc>)
}

impl Info {
    pub fn load(filename: &Path) -> super::Result<Self> {
        let file = std::fs::File::open(filename)?;
        let reader = match exif::Reader::new(&mut std::io::BufReader::new(&file)) {
            Err(exif_err) => {
                warn!("Could not read EXIF from {}: {}", filename.to_string_lossy(), exif_err);
                None
            },
            Ok(reader) => Some(reader)
        };

        // exif created time or file time as fallback, or current time if all else fails
        let created = reader.as_ref().and_then(Self::read_exif_created)
                .or_else(|| filename.metadata().ok()?.created().ok().map(chrono::DateTime::from).map(PhotoTimestamp::File))
                .unwrap_or(PhotoTimestamp::None);

        // create a photo info with sensible default data
        let info = Self {
            filename: filename.to_path_buf(),
            created: created,
        };

        // get info from exif tags that we want
        Ok(info)
    }

    fn read_exif_created(reader: &exif::Reader) -> Option<PhotoTimestamp> {
        let created = reader.get_field(exif::Tag::DateTimeOriginal, false);
        let digitized = reader.get_field(exif::Tag::DateTimeDigitized, false);
        let datetime_tag = created.or(digitized)?;

        let ascii = match datetime_tag.value {
            exif::Value::Ascii(ref ascii) => ascii.first(),
            _ => {warn!("No exif time"); None}
        }?;
        let datetime = exif::DateTime::from_ascii(ascii).ok()?;

        let local = chrono::NaiveDate::from_ymd_opt(datetime.year as i32, datetime.month as u32, datetime.day as u32)?
            .and_hms_nano_opt(datetime.hour as u32, datetime.minute as u32, datetime.second as u32, datetime.nanosecond.unwrap_or(0));

        local.map(PhotoTimestamp::Exif)
    }

    pub fn created(&self) -> chrono::DateTime<chrono::Utc> {
        let default = chrono::Utc.from_utc_datetime(&chrono::NaiveDateTime::from_timestamp(0, 0));
        match self.created {
            PhotoTimestamp::None => default,
            PhotoTimestamp::File(timestamp) => timestamp,
            PhotoTimestamp::Exif(some_time) => chrono::Local.from_local_datetime(&some_time).earliest()
                .map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or(default),
        }
    }

}