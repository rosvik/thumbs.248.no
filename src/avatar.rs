use crate::log;
use crate::quality::ImageType;
use crate::storage;
use crate::storage::RedisPool;
use crate::{AppState, log::LogType, utils};
use axum::body::Bytes;
use axum::http::HeaderValue;
use axum::{Extension, body::Body, extract::Path, http::Response, response::IntoResponse};
use reqwest::StatusCode;

pub async fn get_avatar(
    Path(channel_id): Path<String>,
    Extension(state): Extension<AppState>,
) -> impl IntoResponse {
    if !utils::validate_channel_id(&channel_id) {
        log!("Invalid channel_id {channel_id}", LogType::Warning);
        return utils::fallback_response(400);
    }

    let redis_key = avatar_redis_key(&channel_id);
    if let Some(s3_key) = storage::get_redis_object(&state.redis_pool, &redis_key)
        .await
        .ok()
        .flatten()
        && let Ok(avatar) = storage::get_s3_object(&state.avatar_bucket, &s3_key).await
    {
        let bytes = avatar.bytes().to_vec();
        let Some(image_type) = ImageType::from_file_name(&s3_key) else {
            log!("Invalid s3 key: {s3_key}", LogType::Error);
            return utils::fallback_response(404);
        };
        let content_type = image_type.content_type();
        log!("CACHE: {channel_id} ({content_type})", LogType::Debug);
        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", content_type)
            .body(bytes.into())
            .unwrap_or_else(|_| {
                log!("Error building body", LogType::Error);
                utils::fallback_response(500)
            });
    }

    let og_url = format!(
        "https://og.248.no/api?url=https%3A%2F%2Fwww.youtube.com%2Fchannel%2F{}",
        channel_id
    );
    let Some(og_image_url) = utils::get_og_content(&og_url, "og:image").await else {
        log!("No og:image field", LogType::Error);
        return utils::fallback_response(404);
    };

    let Some(source_url) = utils::to_source_url(&og_image_url) else {
        log!("Invalid og:image url", LogType::Error);
        return utils::fallback_response(404);
    };

    let Ok(response) = reqwest::get(&source_url).await else {
        log!("Error fetching avatar", LogType::Error);
        return utils::fallback_response(404);
    };
    if response.status() != reqwest::StatusCode::OK {
        log!("Error fetching avatar", LogType::Error);
        return utils::fallback_response(404);
    }

    let content_type = response
        .headers()
        .get("Content-Type")
        .cloned()
        .unwrap_or(HeaderValue::from_static(""));
    let content_type = content_type.to_str().unwrap_or_default();

    let image_type = match ImageType::from_content_type(content_type) {
        Some(image_type) => image_type,
        None => {
            log!("Unknown content type: {content_type}", LogType::Error);
            return utils::fallback_response(404);
        }
    };
    let s3_key = avatar_s3_key(&channel_id, image_type);

    let Ok(avatar) = response.bytes().await else {
        log!("Error reading avatar", LogType::Error);
        return utils::fallback_response(404);
    };

    save_to_cache(
        state.avatar_bucket,
        &state.redis_pool,
        &redis_key,
        &s3_key,
        avatar.clone(),
    )
    .await;

    log!("NEW: {channel_id} ({content_type})", LogType::Info);
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", image_type.content_type())
        .body(Body::from(avatar))
        .unwrap_or_else(|_| {
            log!("Error building body", LogType::Error);
            utils::fallback_response(500)
        })
}

pub fn avatar_redis_key(channel_id: &str) -> String {
    format!("avatar:{channel_id}")
}
pub fn avatar_s3_key(channel_id: &str, image_type: ImageType) -> String {
    format!("{channel_id}.{}", image_type.file_extension())
}

async fn save_to_cache(
    bucket: s3::Bucket,
    redis_pool: &RedisPool,
    redis_key: &str,
    s3_key: &str,
    data: Bytes,
) {
    let redis_pool = redis_pool.clone();
    let redis_key = redis_key.to_string();
    let s3_key = s3_key.to_string();
    tokio::spawn(async move {
        let result = storage::put_redis_object(&redis_pool, &redis_key, &s3_key).await;
        if let Err(e) = result {
            log!(
                "ERROR: Error saving {redis_key} to redis: {e}",
                LogType::Error
            );
        }
        let result = storage::put_s3_object(&bucket, &s3_key, data.as_ref()).await;
        if let Err(e) = result {
            log!("ERROR: Error saving {s3_key} to s3: {e}", LogType::Error);
        }
    });
}
