//! Web server for browsing the photo collection.

use crate::cli;
use actix_web::{web, App, HttpServer};
use log::{info};
use photo_archive::library::{LibraryFiles, PhotoDatabase};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub struct WebData {
    photo_db: Arc<Mutex<PhotoDatabase>>,
}

impl WebData {
    pub fn new(db: PhotoDatabase) -> Self {
        Self {
            photo_db: Arc::new(Mutex::new(db)),
        }
    }

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
    context: &mut cli::AppContext,
    library: &LibraryFiles,
    port: u16,
) -> Result<(), failure::Error> {
    let address = format!("localhost:{}", port);

    let data = WebData::new(PhotoDatabase::open_or_create(&library.photo_db_file)?);

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
            .default_service(web::to(web::HttpResponse::NotFound))
    })
    .bind(&address)?
    .run()?;

    context.check_interrupted()?;

    Ok(())
}

mod handlers {
    use actix_web::{http, web, Responder};
    use log::error;
    use photo_archive::library::PhotoId;
    use serde::Serialize;

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

    pub fn photos_get(data: web::Data<WebData>) -> impl Responder {
        let photo_result = data.lock_photo_db().query_all_photos();
        match photo_result {
            Ok(photos) => {
                let photo_objects = photos
                    .into_iter()
                    .map(|photo| PhotoObject {
                        id: photo.id,
                        relative_path: photo.relative_path,
                        created: photo.info.created,
                    })
                    .collect::<Vec<_>>();

                web::HttpResponse::build(http::StatusCode::OK)
                    .content_type("application/json")
                    .json(photo_objects)
            }
            Err(err) => {
                error!("Error retrieving photos: {}", err);

                web::HttpResponse::build(http::StatusCode::INTERNAL_SERVER_ERROR)
                    .content_type("application/json")
                    .json(ErrorResponse::from("Photo not found"))
            }
        }
    }

    pub fn photo_get(data: web::Data<WebData>, info: web::Path<i64>) -> impl Responder {
        let photo_result = data.lock_photo_db().get_photo(PhotoId(*info));

        match photo_result {
            Ok(Some(photo)) => web::HttpResponse::build(http::StatusCode::OK)
                .content_type("application/json")
                .json(PhotoObject {
                    id: photo.id,
                    relative_path: photo.relative_path,
                    created: photo.info.created,
                }),
            Ok(None) => web::HttpResponse::build(http::StatusCode::NOT_FOUND)
                .content_type("application/json")
                .json(ErrorResponse::from("Photo not found")),
            Err(err) => {
                error!("Error retrieving photo with id {}: {}", *info, err);

                web::HttpResponse::build(http::StatusCode::INTERNAL_SERVER_ERROR)
                    .content_type("application/json")
                    .json(ErrorResponse::from("error while retrieving the photo"))
            }
        }
    }

    pub fn photo_thumbnail_get(data: web::Data<WebData>, info: web::Path<i64>) -> impl Responder {
        let thumbnail_result = data.lock_photo_db().query_thumbnail(PhotoId(*info));

        // TODO: set caching headers, use checksum as E-Tag

        match thumbnail_result {
            Ok(Some(thumbnail)) => web::HttpResponse::build(http::StatusCode::OK)
                .content_type("image/jpeg")
                .body(thumbnail.into_jpg_bytes()),
            Ok(None) => web::HttpResponse::build(http::StatusCode::NOT_FOUND)
                .content_type("application/json")
                .json(ErrorResponse::from("This photo has no thumbnail")),
            Err(err) => {
                error!("Error retrieving photo with id {}: {}", *info, err);

                web::HttpResponse::build(http::StatusCode::INTERNAL_SERVER_ERROR)
                    .content_type("application/json")
                    .json(ErrorResponse::from("error while retrieving the thumbnail"))
            }
        }
    }
}
