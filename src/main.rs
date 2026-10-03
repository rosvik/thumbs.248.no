use crate::{
    log::LogType,
    quality::Quality,
    storage::{RedisPool, get_redis_object},
};
use anyhow::Result;
use axum::{
    Extension, Router,
    body::{Body, Bytes},
    extract::{
        Path,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{self, HeaderMap, Response},
    response::{Html, IntoResponse},
    routing::{delete, get},
};
use reqwest::StatusCode;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

mod avatar;
mod log;
mod quality;
mod storage;
mod utils;

#[derive(Clone)]
pub struct AppState {
    thumbs_bucket: s3::Bucket,
    avatar_bucket: s3::Bucket,
    redis_pool: Box<RedisPool>,
    access_token: Option<String>,
    loaded: broadcast::Sender<String>,
}
impl AppState {
    async fn new() -> Self {
        let thumbs_bucket = storage::s3_connection("S3_THUMBNAIL_BUCKET").await;
        let avatar_bucket = storage::s3_connection("S3_AVATAR_BUCKET").await;
        let redis_pool = storage::redis_pool().await;
        let access_token = std::env::var("ACCESS_TOKEN")
            .ok()
            .filter(|token| !token.is_empty());
        let (loaded, _) = broadcast::channel(64);
        AppState {
            thumbs_bucket,
            avatar_bucket,
            redis_pool,
            access_token,
            loaded,
        }
    }

    /// Broadcast to connected admin pages
    fn announce_load(&self, video_id: &str, cache_hit: bool) {
        let flag = if cache_hit { "CACHE" } else { "NEW" };
        let _ = self.loaded.send(format!("{flag} {video_id}"));
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

fn thumbnail_s3_key(video_id: &str, quality: &Quality) -> String {
    format!("{video_id}.{}.{}", quality.slug(), quality.file_extension())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let state = AppState::new().await;
    if state.access_token.is_none() {
        log!("ACCESS_TOKEN not set", LogType::Warning);
    }
    let app = Router::new()
        .route("/", get(index))
        .route("/list", get(list_ids))
        .route("/firehose", get(firehose))
        .route("/avatar/{channel_id}", get(avatar::get_avatar))
        .route("/delete/{video_id}", delete(admin_delete))
        .route("/{video_id}", get(get_thumbnail))
        .layer(Extension(state))
        .layer(CorsLayer::new().allow_origin(Any));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:2342").await.unwrap();
    let addr = listener.local_addr().unwrap();
    log!("Listening on http://{addr}", LogType::Debug);
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

async fn list_ids(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let pattern = "???????????"; // 11 characters, matching video ID
    let now = std::time::Instant::now();
    let keys = storage::list_redis_keys(&state.redis_pool, pattern).await;
    let elapsed = now.elapsed().as_millis();

    if let Err(e) = keys {
        log!("ERROR: Error listing thumbnails: {e}", LogType::Error);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Error listing thumbnails".to_string(),
        );
    }
    // Video IDs never contain ':', so this skips prefixed keys like `avatar:{channel_id}`
    let ids: Vec<String> = keys
        .unwrap()
        .into_iter()
        .filter(|key| !key.contains(':'))
        .collect();

    let count = ids.len();
    log!("LIST: {count} keys - {elapsed}ms", LogType::Debug);
    (StatusCode::OK, ids.join("\n"))
}

fn check_auth(headers: &HeaderMap, state: &AppState) -> bool {
    let authorization = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let token = authorization.strip_prefix("Bearer ").unwrap_or("");

    state.access_token.as_deref() == Some(token)
}

async fn firehose(
    ws: WebSocketUpgrade,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    let loaded = state.loaded.subscribe();
    ws.on_upgrade(move |socket| firehose_stream(socket, loaded))
        .into_response()
}

async fn firehose_stream(mut socket: WebSocket, mut loaded: broadcast::Receiver<String>) {
    log!("FIREHOSE: Connected", LogType::Debug);
    loop {
        match loaded.recv().await {
            Ok(video_id) => {
                if socket.send(Message::text(video_id)).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => log!(
                "FIREHOSE: Lagged, skipped {skipped} loads",
                LogType::Warning
            ),
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    log!("FIREHOSE: Disconnected", LogType::Debug);
}

async fn admin_delete(
    headers: HeaderMap,
    Path(video_id): Path<String>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state) {
        log!("UNAUTHORIZED: Invalid access token", LogType::Warning);
        return (StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    if !utils::validate_video_id(&video_id) {
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

    if let Err(e) = storage::delete_s3_object(&state.thumbs_bucket, &s3_key).await {
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
    if !utils::validate_video_id(&video_id) {
        log!("NOT FOUND: Invalid video ID: {video_id}", LogType::Warning);
        return utils::fallback_response(400);
    }

    // If the image is already cached, return it
    let now = std::time::Instant::now();
    let cached_data =
        match fetch_from_cache(&state.thumbs_bucket, &state.redis_pool, &video_id).await {
            Ok(data) => data,
            Err(_) => {
                return utils::fallback_response(500);
            }
        };
    log!(
        "CACHE READ: {video_id} - {}ms",
        LogType::Performance,
        now.elapsed().as_millis(),
    );
    if let Some((data, quality)) = cached_data {
        log!("CACHE: {video_id} - {quality}", LogType::Debug);
        state.announce_load(&video_id, true);
        return thumbnail_response(data, &quality, true);
    }

    let (bytes, quality) = match fetch_best_quality(&video_id).await {
        Ok((body, quality)) => (body, quality),
        Err(e) => return utils::fallback_response(e.as_u16()),
    };

    state.announce_load(&video_id, false);
    save_to_cache(
        state.thumbs_bucket,
        &state.redis_pool,
        &video_id,
        &quality,
        bytes.clone(),
    )
    .await;

    log!("NEW: {video_id} - {quality}", LogType::Info);
    thumbnail_response(bytes, &quality, false)
}

async fn fetch_best_quality(video_id: &str) -> Result<(Bytes, Quality), StatusCode> {
    for quality in SUPPORTED_QUALITIES {
        match fetch_thumbnail(&video_id, &quality).await {
            Ok(bytes) => return Ok((bytes, quality)),
            Err(e) => {
                if e != StatusCode::NOT_FOUND {
                    return Err(e);
                }
                continue;
            }
        }
    }
    return Err(StatusCode::NOT_FOUND);
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
    let key = thumbnail_s3_key(video_id, quality);
    let video_id = video_id.to_string();
    let redis_pool = redis_pool.clone();
    tokio::spawn(async move {
        let s3_result = storage::put_s3_object(&bucket, &key, data.as_ref()).await;
        if let Err(e) = s3_result {
            log!("ERROR: Error saving {key} to s3: {e}", LogType::Error);
            return;
        }
        let redis_result = storage::put_redis_object(&redis_pool, video_id.as_str(), &key).await;
        if let Err(e) = redis_result {
            log!(
                "ERROR: Error saving {video_id} to redis: {e}",
                LogType::Error
            );
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

fn thumbnail_response(data: impl Into<Body>, quality: &Quality, cache_hit: bool) -> Response<Body> {
    let content_type = match quality.file_extension() {
        "webp" => "image/webp",
        "jpg" => "image/jpeg",
        _ => panic!("Unsupported file extension: {}", quality.file_extension()),
    };
    Response::builder()
        .header("Content-Type", content_type)
        .header("Quality", quality.slug())
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_path() {
        assert_eq!(
            thumbnail_s3_key("aGb3AlQrN9E", &Quality::WebpMaxres),
            "aGb3AlQrN9E.maxresdefault.webp".to_string()
        );
        assert_eq!(
            thumbnail_s3_key("aGb3AlQrN9E", &Quality::JpgMaxres),
            "aGb3AlQrN9E.maxresdefault.jpg".to_string()
        );
        assert_eq!(
            thumbnail_s3_key("aGb3AlQrN9E", &Quality::WebpSd),
            "aGb3AlQrN9E.sddefault.webp".to_string()
        );
        assert_eq!(
            thumbnail_s3_key("aGb3AlQrN9E", &Quality::JpgSd),
            "aGb3AlQrN9E.sddefault.jpg".to_string()
        );
        assert_eq!(
            thumbnail_s3_key("aGb3AlQrN9E", &Quality::WebpHq),
            "aGb3AlQrN9E.hqdefault.webp".to_string()
        );
        assert_eq!(
            thumbnail_s3_key("aGb3AlQrN9E", &Quality::JpgHq),
            "aGb3AlQrN9E.hqdefault.jpg".to_string()
        );
    }
}
