use super::{ImageFormat, PhotoInfo, Sha256Hash};
use log::debug;
use std::path::Path;

pub struct JpegFormat;

impl ImageFormat for JpegFormat {
    fn name(&self) -> &str {
        "JPEG"
    }

    fn supported_extension(&self, path: &Path) -> bool {
        path.extension().map_or(false, |ext| {
            ext == "jpg" || ext == "JPG" || ext == "jpeg" || ext == "JPEG"
        })
    }

    fn read_info(&self, filename: &Path) -> std::io::Result<PhotoInfo> {
        let created = read_exif_datetime(filename).or_else(|| {
            filename
                .metadata()
                .and_then(|meta| meta.created())
                .map(chrono::DateTime::from)
                .ok()
        });

        let file_hash = Sha256Hash::hash_file(filename)?;

        Ok(PhotoInfo { created, file_hash })
    }
}

fn read_exif_datetime(filename: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let file = std::fs::File::open(filename).ok()?;
    let exif_reader = exif::Reader::new(&mut std::io::BufReader::new(file))
        .map(Some)
        .unwrap_or_else(|exif_err| {
            debug!(
                "Could not read EXIF from {}: {}",
                filename.to_string_lossy(),
                exif_err
            );
            None
        })?;

    let created_exif = exif_reader.get_field(exif::Tag::DateTimeOriginal, false);
    let digitized_exif = exif_reader.get_field(exif::Tag::DateTimeDigitized, false);

    created_exif
        .or(digitized_exif)
        .and_then(|datetime_field| parse_exif_datetime(&datetime_field.value))
}

fn parse_exif_datetime(exif_datetime: &exif::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;

    let ascii = match exif_datetime {
        exif::Value::Ascii(ref ascii) => ascii.first(),
        _ => None,
    }?;

    let datetime = exif::DateTime::from_ascii(ascii).ok()?;

    let local = chrono::NaiveDate::from_ymd_opt(
        i32::from(datetime.year),
        u32::from(datetime.month),
        u32::from(datetime.day),
    )?
    .and_hms_nano_opt(
        u32::from(datetime.hour),
        u32::from(datetime.minute),
        u32::from(datetime.second),
        datetime.nanosecond.unwrap_or(0),
    )?;

    chrono::Local
        .from_local_datetime(&local)
        .earliest()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}
