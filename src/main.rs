use crate::{
    log::LogType,
    quality::Quality,
    storage::{RedisPool, get_redis_object},
};
use anyhow::Result;
use axum::{
    Extension, Router,
    body::{Body, Bytes},
    extract::Path,
    http::Response,
    response::{Html, IntoResponse},
    routing::{delete, get},
};
use regex::Regex;
use reqwest::StatusCode;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

mod log;
mod quality;
mod storage;

#[derive(Clone)]
pub struct AppState {
    bucket: s3::Bucket,
    redis_pool: Box<RedisPool>,
    admin_token: Uuid,
    recent: Arc<RwLock<VecDeque<String>>>,
}
impl AppState {
    async fn new() -> Self {
        let bucket = storage::s3_connection().await;
        let redis_pool = storage::redis_pool().await;
        AppState {
            bucket,
            redis_pool,
            admin_token: Uuid::new_v4(),
            recent: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    async fn record_recent(&self, video_id: &str) {
        let mut recent = self.recent.write().await;
        recent.retain(|id| id != video_id);
        recent.push_front(video_id.to_string());
        recent.truncate(RECENT_COUNT);
    }
}

/// Number of recently loaded video IDs kept for the admin page
const RECENT_COUNT: usize = 1000;

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
    format!("{video_id}.{}.{}", quality.slug(), quality.file_extension())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let state = AppState::new().await;
    let token = state.admin_token;
    let app = Router::new()
        .route("/", get(index))
        .route("/list", get(list_ids))
        .route("/admin/{token}", get(admin_page))
        .route("/admin/{token}/thumbnails", get(list_recents))
        .route("/admin/{token}/thumbnail/{video_id}", delete(admin_delete))
        .route("/{video_id}", get(get_thumbnail))
        .layer(Extension(state))
        .layer(CorsLayer::new().allow_origin(Any));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:2342").await.unwrap();
    let addr = listener.local_addr().unwrap();
    log!("Listening on http://{addr}", LogType::Debug);
    log!("Admin page: http://{addr}/admin/{token}", LogType::Info);
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

async fn list_ids(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let keys = storage::list_redis_keys(&state.redis_pool).await;
    if let Err(e) = keys {
        log!("ERROR: Error listing thumbnails: {e}", LogType::Error);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Error listing thumbnails".to_string(),
        );
    }
    let ids = keys.unwrap();
    (StatusCode::OK, ids.join("\n"))
}

fn is_admin(token: &str, state: &AppState) -> bool {
    Uuid::parse_str(token).ok() == Some(state.admin_token)
}

async fn admin_page(
    Path(token): Path<String>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    if !is_admin(&token, &state) {
        log!("UNAUTHORIZED: Invalid admin token", LogType::Warning);
        return (StatusCode::UNAUTHORIZED, Html("Not found")).into_response();
    }
    Html(include_str!("../templates/admin.html")).into_response()
}

async fn list_recents(
    Path(token): Path<String>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    if !is_admin(&token, &state) {
        log!("UNAUTHORIZED: Invalid admin token", LogType::Warning);
        return (StatusCode::UNAUTHORIZED, "Not found".to_string());
    }
    let recent = state.recent.read().await;
    let ids = recent.iter().cloned().collect::<Vec<String>>();
    (StatusCode::OK, ids.join("\n"))
}

async fn admin_delete(
    Path((token, video_id)): Path<(String, String)>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    if !is_admin(&token, &state) {
        log!("UNAUTHORIZED: Invalid admin token", LogType::Warning);
        return (StatusCode::UNAUTHORIZED, "Not found");
    }
    if !validate_video_id(&video_id) {
        log!(
            "BAD REQUEST: Invalid video ID: {video_id}",
            LogType::Warning
        );
        return (StatusCode::BAD_REQUEST, "Invalid video ID");
    }

    let s3_key = match get_redis_object(&state.redis_pool, &video_id).await {
        Ok(Some(key)) => key,
        Ok(None) => return (StatusCode::NOT_FOUND, "Thumbnail not found"),
        Err(e) => {
            log!("ERROR: Error looking up {video_id}: {e}", LogType::Error);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error deleting thumbnail",
            );
        }
    };

    if let Err(e) = storage::delete_s3_object(&state.bucket, &s3_key).await {
        log!(
            "ERROR: Error deleting {s3_key} from s3: {e}",
            LogType::Error
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Error deleting thumbnail",
        );
    }
    if let Err(e) = storage::delete_redis_object(&state.redis_pool, &video_id).await {
        log!(
            "ERROR: Error deleting {video_id} from redis: {e}",
            LogType::Error
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Error deleting thumbnail",
        );
    }

    log!("DELETE: {video_id} - {s3_key}", LogType::Info);
    (StatusCode::NO_CONTENT, "")
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
    let now = std::time::Instant::now();
    let cached_data = match fetch_from_cache(&state.bucket, &state.redis_pool, &video_id).await {
        Ok(data) => data,
        Err(_) => {
            return fallback_response(500);
        }
    };
    log!(
        "CACHE READ: {video_id} - {}ms",
        LogType::Performance,
        now.elapsed().as_millis(),
    );
    if let Some((data, quality)) = cached_data {
        log!("CACHE: {video_id} - {quality}", LogType::Debug);
        state.record_recent(&video_id).await;
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

    state.record_recent(&video_id).await;
    save_to_cache(
        state.bucket,
        &state.redis_pool,
        &video_id,
        &quality,
        body.clone(),
    )
    .await;

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
    bucket: s3::Bucket,
    redis_pool: &RedisPool,
    video_id: &str,
    quality: &Quality,
    data: Bytes,
) {
    let key = s3_key(video_id, quality);
    let video_id = video_id.to_string();
    let redis_pool = redis_pool.clone();
    tokio::spawn(async move {
        let result = storage::put_redis_object(&redis_pool, video_id.as_str(), &key).await;
        if let Err(e) = result {
            log!(
                "ERROR: Error saving thumbnail to redis: {e}",
                LogType::Error
            );
        }
        let result = storage::put_s3_object(&bucket, &key, data.as_ref()).await;
        if let Err(e) = result {
            log!("ERROR: Error saving thumbnail to s3: {e}", LogType::Error);
        }
    });
}

async fn fetch_from_cache(
    bucket: &s3::Bucket,
    redis_pool: &RedisPool,
    video_id: &str,
) -> Result<Option<(Vec<u8>, Quality)>> {
    let now = std::time::Instant::now();
    let s3_id = get_redis_object(redis_pool, video_id).await?;
    log!(
        "CACHE READ: Redis - {video_id} - {}ms",
        LogType::Performance,
        now.elapsed().as_millis(),
    );
    if let Some(s3_id) = s3_id {
        let quality = match Quality::from_s3_key(&s3_id) {
            Some(quality) => quality,
            None => {
                log!("ERROR: Invalid S3 key: {s3_id}", LogType::Error);
                return Err(anyhow::anyhow!("Invalid S3 key: {s3_id}"));
            }
        };
        let now = std::time::Instant::now();
        let data = storage::get_s3_object(bucket, &s3_id).await;
        log!(
            "CACHE READ: S3 - {video_id} - {}ms",
            LogType::Performance,
            now.elapsed().as_millis(),
        );
        if let Ok(data) = data {
            return Ok(Some((data.into_bytes().to_vec(), quality)));
        }
    }
    Ok(None)
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
            "aGb3AlQrN9E.maxresdefault.webp".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::JpgMaxres),
            "aGb3AlQrN9E.maxresdefault.jpg".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::WebpSd),
            "aGb3AlQrN9E.sddefault.webp".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::JpgSd),
            "aGb3AlQrN9E.sddefault.jpg".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::WebpHq),
            "aGb3AlQrN9E.hqdefault.webp".to_string()
        );
        assert_eq!(
            s3_key("aGb3AlQrN9E", &Quality::JpgHq),
            "aGb3AlQrN9E.hqdefault.jpg".to_string()
        );
    }
}
