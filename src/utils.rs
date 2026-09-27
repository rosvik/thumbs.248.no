use crate::log;
use crate::log::LogType;
use axum::{body::Body, http::Response};
use regex::Regex;
use reqwest::Url;

// https://wiki.archiveteam.org/index.php/YouTube/Technical_details
// Channel ID regex pattern: [A-Za-z0-9_-]{21}[AQgw]
pub fn validate_channel_id(channel_id: &str) -> bool {
    Regex::new(r"^UC[A-Za-z0-9_-]{21}[AQgw]$")
        .unwrap()
        .is_match(channel_id)
}

/// Validate the video ID is a valid YouTube video ID
///
/// Source: https://wiki.archiveteam.org/index.php/YouTube/Technical_details
pub fn validate_video_id(video_id: &str) -> bool {
    let re = Regex::new(r"^[A-Za-z0-9_-]{10}[AEIMQUYcgkosw048]$").unwrap();
    re.is_match(video_id)
}

// Expected input: https://yt3.googleusercontent.com/xxxxx=s900-c-k-c0x00ffffff-no-rj
// Source quality: https://yt3.googleusercontent.com/xxxxx=s0
pub fn to_source_url(url: &str) -> Option<String> {
    let Ok(image_url) = Url::parse(&url).map_err(|e| e) else {
        return None;
    };
    if image_url.host_str() != Some("yt3.googleusercontent.com") {
        return None;
    }

    let Some(stripped_url) = image_url.as_str().split("=").next() else {
        return None;
    };

    let source_url = format!("{stripped_url}=s0");
    Some(source_url)
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct OgValue {
    pub property: String,
    pub content: String,
}

pub async fn get_og_content(og_url: &str, property: &str) -> Option<String> {
    let Ok(response) = reqwest::get(og_url).await else {
        log!("Error fetching og from og", LogType::Error);
        return None;
    };
    let Ok(body) = response.text().await else {
        log!("Error reading response", LogType::Error);
        return None;
    };
    let Ok(og) = serde_json::from_str::<Vec<OgValue>>(&body) else {
        log!("Error parsing og data", LogType::Error);
        return None;
    };

    og.iter()
        .find(|f| f.property == property)
        .and_then(|f| Some(f.content.clone()))
}

pub fn fallback_response(status: u16) -> Response<Body> {
    let fallback_image = include_bytes!("../fallback.webp");
    Response::builder()
        .status(status)
        .header("Content-Type", "image/webp")
        .body(Body::from(fallback_image.to_vec()))
        .unwrap()
}
