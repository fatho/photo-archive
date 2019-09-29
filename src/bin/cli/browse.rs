//! Web server for browsing the photo collection.

use crate::cli;
use actix_web::{web, App, HttpServer};
use log::{info};
use photo_archive::library::{LibraryFiles, PhotoDatabase};
use std::sync::{Arc, Mutex, MutexGuard};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct WebData {
    photo_db: Arc<Mutex<PhotoDatabase>>,
    root_dir: PathBuf,
}

impl WebData {
    pub fn new(root_dir: PathBuf, db: PhotoDatabase) -> Self {
        Self {
            photo_db: Arc::new(Mutex::new(db)),
            root_dir,
        }
    }

    pub fn lock_photo_db(&self) -> MutexGuard<PhotoDatabase> {
        if let Ok(guard) = self.photo_db.lock() {
            guard
        } else {
            panic!("Photo database mutex was poisoned")
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }
}

/// Start a webserver for browsing the library.
pub fn browse(
    _context: &mut cli::AppContext,
    library: &LibraryFiles,
    port: u16,
) -> Result<(), failure::Error> {
    let address = format!("localhost:{}", port);

    let data = WebData::new(
        library.root_dir.to_path_buf(),
        PhotoDatabase::open_or_create(&library.photo_db_file)?,
    );

    info!("Starting web server");
    info!("You can access the server at http://{}/", address);

    HttpServer::new(move || {
        App::new()
            .data(data.clone())
            .service(web::resource("/photos").route(web::get().to(handlers::photos_get)))
            .service(web::resource("/photos/{id}").route(web::get().to(handlers::photo_get)))
            .service(
                web::resource("/photos/{id}/thumbnail")
                    .route(web::get().to(handlers::photo_thumbnail_get)),
            )
            .service(
                web::resource("/photos/{id}/original")
                    .route(web::get().to(handlers::photo_original_get)),
            )
            .service(
                web::resource("/").route(web::get().to(handlers::app_get)),
            )
            .default_service(web::to(handlers::builtin_file_get))
    })
    .bind(&address)?
    .run()?;

    Ok(())
}

mod handlers {
    use actix_web::{http, web, Responder};
    use log::{info, error};
    use photo_archive::library::{PhotoId, PhotoPath};
    use photo_archive::formats::Sha256Hash;
    use serde::Serialize;
    use failure::format_err;
    use std::path::Path;
    use std::borrow::Cow;

    use super::WebData;

    /// JSON formatted error response returned by all endpoints.
    #[derive(Serialize)]
    struct ErrorResponse {
        message: String,
    }

    impl ErrorResponse {
        pub fn new(message: String) -> Self {
            Self { message }
        }
    }

    impl<'a> From<&'a str> for ErrorResponse {
        fn from(message: &'a str) -> Self {
            Self::new(message.to_string())
        }
    }

    #[derive(Serialize)]
    struct PhotoObject {
        id: PhotoId,
        relative_path: String,
        created: Option<chrono::DateTime<chrono::Utc>>,
    }

    // static APP_HTML: &'static [u8] = include_bytes!("../../../web/index.html");

    pub fn develop_get(req: web::HttpRequest) -> impl Responder {
        error_handler(|| {
            let current_dir = std::env::current_dir()?;
            let webdir = current_dir.join("web");
            let filename = current_dir.join(Path::new(req.path().trim_start_matches('/'))).canonicalize()?;
            let _ = filename.strip_prefix(webdir)?;

            let content_type = filename.extension().and_then(|ext|
                if ext == "js" {
                    Some("text/javascript; charset=utf-8")
                } else if ext == "css" {
                    Some("text/css; charset=utf-8")
                } else {
                    None
                }
            );
            let contents = std::fs::read(filename)?;

            Ok(web::HttpResponse::Ok()
                .content_type(content_type.unwrap_or("application/octet-stream"))
                .body(contents))
        })
    }

    pub fn builtin_file_get(req: web::HttpRequest) -> impl Responder {
        error_handler(|| {
            let req_etag = get_if_none_match_sha256(&req);
            match req.path() {
                "/favicon.ico" => {
                    // Ok(static_response(req_etag, "image/x-icon", None, include_bytes!("../../../web/favicon.ico")))
                    let (content_type, contents) = read_web_file("/web/favicon.ico")?;
                    let hash = Sha256Hash::hash_bytes(&contents);
                    Ok(dynamic_response(req_etag, "image/x-icon", Some(hash), contents))
                }
                path if path.starts_with("/web") => {
                    let (content_type, contents) = read_web_file(path)?;
                    let hash = Sha256Hash::hash_bytes(&contents);
                    Ok(dynamic_response(req_etag, content_type, Some(hash), contents))
                }
                _ => {
                    Ok(web::HttpResponse::NotFound()
                        .content_type("application/json")
                        .json(ErrorResponse::from("Not found")))
                }
            }
        })
    }

    fn static_response(req_etag: Option<Sha256Hash>, content_type: &str, etag: Option<Sha256Hash>, data: &'static [u8]) -> web::HttpResponse {
        if req_etag.is_some() && req_etag == etag {
            web::HttpResponse::NotModified().into()
        } else {
            if let Some(etag) = etag {
                web::HttpResponse::Ok()
                .content_type(content_type)
                .header("Cache-Control", "private, max-age=3600")
                .header("ETag", format!("\"{}\"", etag))
                .body(data)
            } else {
                web::HttpResponse::Ok()
                .content_type(content_type)
                .body(data)
            }
        }
    }

    fn dynamic_response(req_etag: Option<Sha256Hash>,content_type: &str, etag: Option<Sha256Hash>, data: Vec<u8>) -> web::HttpResponse {
        if req_etag.is_some() && req_etag == etag {
            web::HttpResponse::NotModified().into()
        } else {
            if let Some(etag) = etag {
                web::HttpResponse::Ok()
                .content_type(content_type)
                .header("Cache-Control", "private, max-age=3600")
                .header("ETag", format!("\"{}\"", etag))
                .body(data)
            } else {
                web::HttpResponse::Ok()
                .content_type(content_type)
                .body(data)
            }
        }
    }

    /// In development mode, read the web files from the filesystem instead of statically compiling them into the binary.
    fn read_web_file(path: &str) -> Result<(&'static str, Vec<u8>), failure::Error> {
        let current_dir = std::env::current_dir()?;
        let webdir = current_dir.join("web");
        let filename = current_dir.join(Path::new(path.trim_start_matches('/'))).canonicalize()?;
        let _ = filename.strip_prefix(webdir)?;

        let content_type = filename.extension().and_then(|ext|
            if ext == "js" {
                Some("text/javascript; charset=utf-8")
            } else if ext == "css" {
                Some("text/css; charset=utf-8")
            } else {
                None
            }
        );
        let contents = std::fs::read(filename)?;
        Ok((content_type.unwrap_or("application/octet-stream"), contents))
    }

    pub fn app_get() -> impl Responder {
        web::HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(std::fs::read("web/index.html").unwrap())
    }

    pub fn photos_get(data: web::Data<WebData>) -> impl Responder {
        error_handler(|| {
            let photos = data.lock_photo_db().query_all_photos()?;
            let photo_objects = photos
                .into_iter()
                .map(|photo| PhotoObject {
                    id: photo.id,
                    relative_path: photo.relative_path,
                    created: photo.info.created,
                })
                .collect::<Vec<_>>();

            Ok(web::HttpResponse::Ok()
                .content_type("application/json")
                .json(photo_objects))
        })
    }

    pub fn photo_get(data: web::Data<WebData>, info: web::Path<i64>) -> impl Responder {
        error_handler(|| {
            let photo = data.lock_photo_db().get_photo(PhotoId(*info))?;

            let response = if let Some(photo) = photo {
                web::HttpResponse::Ok()
                    .content_type("application/json")
                    .json(PhotoObject {
                        id: photo.id,
                        relative_path: photo.relative_path,
                        created: photo.info.created,
                    })
            } else {
                web::HttpResponse::NotFound()
                    .content_type("application/json")
                    .json(ErrorResponse::from("Photo not found"))
            };
            Ok(response)
        })
    }

    pub fn photo_original_get(req: web::HttpRequest, data: web::Data<WebData>, info: web::Path<i64>) -> impl Responder {
        error_handler(|| {
            let photo_id = PhotoId(*info);
            let etag_request = get_if_none_match_sha256(&req);

            let result = {
                let db = data.lock_photo_db();
                let maybe_photo = db.get_photo(photo_id)?;
                if let Some(photo) = maybe_photo {
                    // early exit if the etag matches
                    if Some(&photo.info.file_hash) == etag_request.as_ref() {
                        return Ok(web::HttpResponse::NotModified().into());
                    }
                    // otherwise load the image file
                    let path = PhotoPath::from_relative(data.root_dir(), &photo.relative_path);
                    let data = std::fs::read(path.full_path)?;
                    Some((data, photo.info.file_hash, "image/jpeg"))
                } else {
                    None
                }
            };

            let response = if let Some((image_data, etag, content_type)) = result {
                web::HttpResponse::Ok()
                    .content_type(content_type)
                    .header("ETag", format!("\"{}\"", etag))
                    .header("Cache-Control", "private, max-age=3600")
                    .body(image_data)
            } else {
                web::HttpResponse::NotFound()
                    .content_type("application/json")
                    .json(ErrorResponse::from("Photo not found"))
            };
            Ok(response)
        })
    }

    pub fn photo_thumbnail_get(req: web::HttpRequest, data: web::Data<WebData>, info: web::Path<i64>) -> impl Responder {
        error_handler(|| {
            let photo_id = PhotoId(*info);
            let etag_request = get_if_none_match_sha256(&req);

            let (etag_result, thumbnail_result) = {
                let db = data.lock_photo_db();
                let etag_result = db.query_thumbnail_hash(photo_id)?;
                // early exit if the etag matches
                if let Some(etag) = db.query_thumbnail_hash(photo_id)? {
                    if Some(etag) == etag_request {
                        return Ok(web::HttpResponse::NotModified().into());
                    }
                }
                // otherwise, get the thumbnail and send it
                (etag_result, db.query_thumbnail(photo_id)?)
            };

            let response = if let Some(thumbnail) = thumbnail_result {
                let etag = etag_result.ok_or(format_err!("Thumbnail {:?} without hash", photo_id))?;
                web::HttpResponse::Ok()
                    .content_type("image/jpeg")
                    .header("ETag", format!("\"{}\"", etag))
                    .header("Cache-Control", "private, max-age=3600")
                    .body(thumbnail.into_jpg_bytes())
            } else {
                web::HttpResponse::NotFound()
                    .content_type("application/json")
                    .json(ErrorResponse::from("This photo has no thumbnail"))
            };
            Ok(response)
        })
    }


    fn error_handler<F: FnOnce() -> Result<web::HttpResponse, failure::Error>>(callback: F) -> web::HttpResponse {
        match callback() {
            Ok(response) => response,
            Err(err) => {
                error!("Error while handling request: {}", err);
                web::HttpResponse::build(http::StatusCode::INTERNAL_SERVER_ERROR)
                    .content_type("application/json")
                    .json(ErrorResponse::from("internal server error"))
            }
        }
    }

    fn get_if_none_match_sha256(req: &web::HttpRequest) -> Option<Sha256Hash> {
        req.headers().get("If-None-Match").and_then(|value| value.as_bytes().get(1..65)).and_then(Sha256Hash::from_hex)
    }
}
