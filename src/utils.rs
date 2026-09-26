use regex::Regex;
use reqwest::Url;

// https://wiki.archiveteam.org/index.php/YouTube/Technical_details
// Channel ID regex pattern: [A-Za-z0-9_-]{21}[AQgw]
pub fn validate_channel_id(channel_id: &str) -> bool {
    Regex::new(r"^UC[A-Za-z0-9_-]{21}[AQgw]$")
        .unwrap()
        .is_match(channel_id)
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

pub fn get_og_content(og: &[OgValue], property: &str) -> Option<String> {
    og.iter()
        .find(|f| f.property == property)
        .and_then(|f| Some(f.content.clone()))
}
