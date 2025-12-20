use crate::{log::LogType, quality::Quality};
use axum::{
    Extension, Router,
    body::{Body, Bytes},
    extract::Path,
    http::Response,
    response::{Html, IntoResponse},
    routing::get,
};
use regex::Regex;
use reqwest::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

mod log;
mod quality;
mod storage;

#[derive(Clone)]
pub struct AppState {
    bucket: Arc<Mutex<Box<s3::Bucket>>>,
}
impl AppState {
    async fn new() -> Self {
        let bucket = storage::get_connection().await;
        AppState {
            bucket: Arc::new(Mutex::new(bucket)),
        }
    }
}

/// Supported qualities for thumbnails, in order of preference
const SUPPORTED_QUALITIES: [Quality; 6] = [
    Quality::WebpMaxres,
    Quality::JpgMaxres,
    Quality::WebpSd,
    Quality::JpgSd,
    Quality::WebpHq,
    Quality::JpgHq,
];

fn s3_key(video_id: &str, quality: &Quality) -> String {
    let prefix = video_id.split_at(2).0;
    format!(
        "{prefix}.{}.{video_id}.{}",
        quality.slug(),
        quality.file_extension()
    )
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let app = Router::new()
        .route("/", get(index))
        .route("/{video_id}", get(get_thumbnail))
        .layer(Extension(AppState::new().await))
        .layer(CorsLayer::new().allow_origin(Any));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:2342").await.unwrap();
    log!(
        "Listening on http://{}",
        LogType::Debug,
        listener.local_addr().unwrap(),
    );
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

async fn get_thumbnail(
    Path(video_id): Path<String>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    if !validate_video_id(&video_id) {
        log!("NOT FOUND: Invalid video ID: {video_id}", LogType::Warning);
        return fallback_response(400);
    }

    // If the image is already cached, return it
    let bucket = state.bucket.lock().await;
    let cached_data = fetch_from_cache(&bucket, &video_id).await;
    drop(bucket);
    if let Some((data, quality)) = cached_data {
        log!("CACHE: {video_id} - {quality}", LogType::Debug);
        return image_response(data, &quality, true);
    }

    let mut quality: Option<Quality> = None;
    let mut body: Option<Bytes> = None;
    for q in SUPPORTED_QUALITIES {
        match fetch_thumbnail(&video_id, &q).await {
            Ok(b) => {
                body = Some(b);
                quality = Some(q);
                break;
            }
            Err(e) => {
                if e != StatusCode::NOT_FOUND {
                    return fallback_response(e.as_u16());
                }
                continue;
            }
        }
    }
    if body.is_none() || quality.is_none() {
        return fallback_response(500);
    }
    let body = body.unwrap();
    let quality = quality.unwrap();

    save_to_cache(state.bucket, &video_id, &quality, body.clone()).await;

    log!("NEW: {video_id} - {quality}", LogType::Info);
    image_response(body, &quality, false)
}

async fn fetch_thumbnail(video_id: &str, quality: &Quality) -> Result<Bytes, StatusCode> {
    let now = std::time::Instant::now();
    let webp_postfix = if quality.file_extension() == "webp" {
        "_webp"
    } else {
        ""
    };
    let url = format!(
        "https://i.ytimg.com/vi{webp_postfix}/{video_id}/{}.{}",
        quality.slug(),
        quality.file_extension()
    );
    let response = match reqwest::get(&url).await {
        Ok(response) => response,
        Err(e) => {
            log!(
                "ERROR: Error fetching {quality} thumbnail: {url}: {e}",
                LogType::Error
            );
            return Err(e.status().unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
        }
    };
    log!(
        "YOUTUBE FETCH: {quality} - {video_id} - {}ms",
        LogType::Performance,
        now.elapsed().as_millis(),
    );
    if response.status() != StatusCode::OK {
        if response.status() != StatusCode::NOT_FOUND {
            log!(
                "ERROR: Error fetching {quality} thumbnail for {video_id}: {}",
                LogType::Error,
                response.status(),
            );
        }
        return Err(response.status());
    }

    match response.bytes().await {
        Ok(bytes) => Ok(bytes),
        Err(e) => {
            log!(
                "ERROR: Error reading response for {quality} thumbnail for {video_id}: {e}",
                LogType::Error,
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn save_to_cache(
    bucket: Arc<Mutex<Box<s3::Bucket>>>,
    video_id: &str,
    quality: &Quality,
    data: Bytes,
) {
    let key = s3_key(video_id, quality);
    tokio::spawn(async move {
        let bucket = bucket.lock().await;
        let result = storage::put_object(&bucket, &key, data.as_ref()).await;
        if let Err(e) = result {
            log!(
                "ERROR: Error saving thumbnail to cache: {e}",
                LogType::Error
            );
        }
    });
}

async fn fetch_from_cache(bucket: &s3::Bucket, video_id: &str) -> Option<(Vec<u8>, Quality)> {
    for quality in SUPPORTED_QUALITIES {
        let key = s3_key(video_id, &quality);
        let now = std::time::Instant::now();
        if let Ok(data) = storage::get_object(bucket, &key).await {
            log!(
                "CACHE LOOKUP: {video_id} - {quality} - {}ms",
                LogType::Performance,
                now.elapsed().as_millis(),
            );
            log!(
                "CACHE READ: {video_id} - {quality} - {}ms",
                LogType::Performance,
                now.elapsed().as_millis(),
            );
            return Some((data.into_bytes().to_vec(), quality));
        }
        log!(
            "CACHE MISS: {video_id} - {quality} - {}ms",
            LogType::Performance,
            now.elapsed().as_millis(),
        );
    }
    None
}

fn image_response(data: impl Into<Body>, quality: &Quality, cache_hit: bool) -> Response<Body> {
    let content_type = match quality.file_extension() {
        "webp" => "image/webp",
        "jpg" => "image/jpeg",
        _ => panic!("Unsupported file extension: {}", quality.file_extension()),
    };
    Response::builder()
        .header("Content-Type", content_type)
        .header(
            "Cache-Status",
            match cache_hit {
                true => "ThumbsCache; hit",
                false => "ThumbsCache; fwd=uri-miss; stored",
            },
        )
        .body(data.into())
        .unwrap()
}

fn fallback_response(status: u16) -> Response<Body> {
    let fallback_image = include_bytes!("../fallback.webp");
    Response::builder()
        .status(status)
        .header("Content-Type", "image/webp")
        .body(Body::from(fallback_image.to_vec()))
        .unwrap()
}

/// Validate the video ID is a valid YouTube video ID
///
/// Source: https://wiki.archiveteam.org/index.php/YouTube/Technical_details
fn validate_video_id(video_id: &str) -> bool {
    let re = Regex::new(r"^[A-Za-z0-9_-]{10}[AEIMQUYcgkosw048]$").unwrap();
    re.is_match(video_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_path() {
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::WebpMaxres),
            "aG.maxresdefault.aGb3AlQrN9E.webp".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::JpgMaxres),
            "aG.maxresdefault.aGb3AlQrN9E.jpg".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::WebpSd),
            "aG.sddefault.aGb3AlQrN9E.webp".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::JpgSd),
            "aG.sddefault.aGb3AlQrN9E.jpg".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::WebpHq),
            "aG.hqdefault.aGb3AlQrN9E.webp".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::JpgHq),
            "aG.hqdefault.aGb3AlQrN9E.jpg".to_string()
        );
    }
}
