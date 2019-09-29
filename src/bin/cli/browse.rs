//! Web server for browsing the photo collection.

use crate::cli;
use actix_web::{web, App, HttpServer};
use log::info;
use photo_archive::library::{LibraryFiles, PhotoDatabase};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct WebData {
    photo_db: Arc<Mutex<PhotoDatabase>>,
    photo_root: PathBuf,
    web_root: Option<PathBuf>,
}

impl WebData {
    pub fn lock_photo_db(&self) -> MutexGuard<PhotoDatabase> {
        if let Ok(guard) = self.photo_db.lock() {
            guard
        } else {
            panic!("Photo database mutex was poisoned")
        }
    }
}

/// Start a webserver for browsing the library.
pub fn browse(
    _context: &mut cli::AppContext,
    library: &LibraryFiles,
    binds: &[String],
    web_root: Option<PathBuf>,
) -> Result<(), failure::Error> {
    let data = WebData {
        photo_root: library.root_dir.to_path_buf(),
        photo_db: Arc::new(Mutex::new(PhotoDatabase::open_or_create(&library.photo_db_file)?)),
        web_root: web_root,
    };

    info!("Starting web server");

    let factory = HttpServer::new(move || {
        App::new()
            .data(data.clone())
            // REST API:
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
            // Frontend:
            .default_service(web::to(handlers::static_file_handler))
    });
    let factory = binds.iter().fold(Ok(factory), |factory, address| factory?.bind(address))?;
    factory.run()?;

    Ok(())
}

mod handlers {
    use actix_web::{http, web, Responder};
    use failure::format_err;
    use log::{error};
    use photo_archive::formats::Sha256Hash;
    use photo_archive::library::{PhotoId, PhotoPath};
    use serde::Serialize;
    use std::path::Path;
    use lazy_static::lazy_static;
    use std::collections::HashMap;

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

    /// A static file that is served by the builtin webserver.
    struct StaticResource {
        content_type: &'static str,
        contents: &'static [u8],
        /// The hash is used as ETag.
        hash: Sha256Hash,
    }

    macro_rules! static_resource {
        ($content_type: expr, $path:expr) => {
            {
                let contents: &'static [u8] = include_bytes!($path);
                StaticResource {
                    content_type: $content_type,
                    contents: contents,
                    hash: Sha256Hash::hash_bytes(contents),
                }
            }
        };
    }

    lazy_static!{
        static ref STATIC_RESOURCES: HashMap<&'static str, StaticResource> = {
            let mut resources = HashMap::new();
            resources.insert("/web/favicon.ico",
                static_resource!("image/x-icon", "../../../web/favicon.ico"));
            resources.insert("/web/favicon.png",
                static_resource!("image/png", "../../../web/favicon.png"));
            resources.insert("/web/index.html",
                static_resource!("text/html; charset=utf-8", "../../../web/index.html"));
            resources.insert("/web/viewer.js",
                static_resource!("text/javascript; charset=utf-8", "../../../web/viewer.js"));
            resources
        };
    }

    fn static_response(
        req_etag: Option<Sha256Hash>,
        content_type: &str,
        etag: Option<Sha256Hash>,
        data: web::Bytes,
    ) -> web::HttpResponse {
        if req_etag.is_some() && req_etag == etag {
            web::HttpResponse::NotModified().into()
        } else {
            if let Some(etag) = etag {
                // If the resource has an etag, send caching headers as well
                web::HttpResponse::Ok()
                    .content_type(content_type)
                    .header("Cache-Control", "private, max-age=3600")
                    .header("ETag", format!("\"{}\"", etag))
                    .body(data)
            } else {
                // Otherwise only send the data
                web::HttpResponse::Ok()
                    .content_type(content_type)
                    .body(data)
            }
        }
    }

    pub fn static_file_handler(data: web::Data<WebData>, request: web::HttpRequest) -> impl Responder {
        error_handler(|| {
            let request_etag = get_if_none_match_sha256(&request);
            let rewritten_path = match request.path() {
                "/" => "/web/index.html",
                "/favicon.ico" => "/web/favicon.ico",
                path => path,
            };
            if let Some(resource) = STATIC_RESOURCES.get(rewritten_path) {
                let (hash, contents) = if let Some(ref web_root) = data.web_root {
                    // At this point, we have established that the path is a valid static resource,
                    // and that it starts with `/web/` (otherwise there would be no matching entry in the hashmap).
                    let filename = web_root.join(Path::new(rewritten_path.trim_start_matches("/web/")));
                    let contents = std::fs::read(&filename)?;
                    let hash = Sha256Hash::hash_bytes(&contents);
                    (hash, contents.into())
                } else {
                    // use the builtin resources
                    (resource.hash.clone(), resource.contents.into())
                };
                Ok(static_response(request_etag, resource.content_type, Some(hash), contents))
            } else {
                Ok(web::HttpResponse::NotFound()
                    .content_type("application/json")
                    .json(ErrorResponse::from("Not found")))
            }
        })
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

    pub fn photo_original_get(
        req: web::HttpRequest,
        data: web::Data<WebData>,
        info: web::Path<i64>,
    ) -> impl Responder {
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
                    let path = PhotoPath::from_relative(&data.photo_root, &photo.relative_path);
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

    pub fn photo_thumbnail_get(
        req: web::HttpRequest,
        data: web::Data<WebData>,
        info: web::Path<i64>,
    ) -> impl Responder {
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
                let etag =
                    etag_result.ok_or(format_err!("Thumbnail {:?} without hash", photo_id))?;
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

    fn error_handler<F: FnOnce() -> Result<web::HttpResponse, failure::Error>>(
        callback: F,
    ) -> web::HttpResponse {
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
        req.headers()
            .get("If-None-Match")
            .and_then(|value| value.as_bytes().get(1..65))
            .and_then(Sha256Hash::from_hex)
    }
}
