use axum::{
    Router,
    body::{Body, Bytes},
    extract::Path,
    http::Response,
    response::{Html, IntoResponse},
    routing::get,
};
use regex::Regex;
use std::{fmt, path::PathBuf};
use tokio::{fs::File, io::AsyncWriteExt};
use tower_http::cors::{Any, CorsLayer};

#[derive(Debug)]
enum Quality {
    Maxresdefault,
    Sddefault,
    Hqdefault,
}
impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
}

/// Supported qualities for thumbnails, in order of preference
const SUPPORTED_QUALITIES: [Quality; 3] = [
    Quality::Maxresdefault,
    Quality::Sddefault,
    Quality::Hqdefault,
];

const DEFAULT_THUMBNAIL_DIR: &str = "thumbnails";
fn thumbnail_dir() -> PathBuf {
    std::env::var("THUMBNAIL_DIR")
        .unwrap_or(DEFAULT_THUMBNAIL_DIR.to_string())
        .into()
}
fn thumbnail_path(video_id: &str, quality: &Quality) -> PathBuf {
    thumbnail_dir()
        .join(quality.to_string())
        .join(format!("{video_id}.webp"))
}
fn init_thumbnail_dirs() {
    for quality in SUPPORTED_QUALITIES {
        match std::fs::create_dir_all(thumbnail_dir().join(quality.to_string())) {
            Ok(_) => (),
            Err(e) => println!("Error creating thumbnail directory: {e}"),
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    init_thumbnail_dirs();
    let app = Router::new()
        .route("/", get(index))
        .route("/all", get(get_all_thumbnails))
        .route("/{video_id}", get(get_thumbnail))
        .layer(CorsLayer::new().allow_origin(Any));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:2342").await.unwrap();
    println!("Listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

async fn get_all_thumbnails() -> impl IntoResponse {
    let thumbnail_dir = thumbnail_dir();
    let mut thumbnails: Vec<PathBuf> = Vec::new();

    for quality in SUPPORTED_QUALITIES {
        let dir = std::fs::read_dir(thumbnail_dir.join(quality.to_string())).unwrap();
        let files = dir.map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
        thumbnails.extend(files);
    }

    thumbnails
        .iter()
        .map(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .split(".")
                .next()
                .unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn get_thumbnail(Path(video_id): Path<String>) -> impl IntoResponse {
    if !validate_video_id(&video_id) {
        return fallback_response(400);
    }

    // If the image is already cached, return it
    let cached_data = fetch_from_cache(&video_id).await;
    if let Some((data, quality)) = cached_data {
        println!("Returning cached {quality} thumbnail for {video_id}");
        return webp_response(data);
    }

    let mut quality: Option<Quality> = None;
    let mut body: Option<Bytes> = None;
    for q in SUPPORTED_QUALITIES {
        if let Ok(b) = fetch_thumbnail(&video_id, &q).await {
            body = Some(b);
            quality = Some(q);
            break;
        }
    }
    if body.is_none() || quality.is_none() {
        return fallback_response(500);
    }
    let body = body.unwrap();
    let quality = quality.unwrap();

    save_to_cache(&video_id, &quality, body.clone()).await;

    println!("Fetched {quality} thumbnail for {video_id}");
    webp_response(body.to_vec())
}

async fn fetch_thumbnail(
    video_id: &str,
    quality: &Quality,
) -> Result<Bytes, Box<dyn std::error::Error>> {
    let url = format!("https://i.ytimg.com/vi_webp/{video_id}/{quality}.webp");
    let response = match reqwest::get(&url).await {
        Ok(response) => response,
        Err(e) => {
            println!("Error fetching {quality} thumbnail: {url}: {e}");
            return Err(Box::new(e));
        }
    };

    if response.status() != 200 {
        println!(
            "Error fetching {quality} thumbnail for {video_id}: {}",
            response.status()
        );
        return Err(Box::new(std::io::Error::other(
            response.status().as_str().to_string(),
        )));
    }

    Ok(response.bytes().await?)
}

async fn save_to_cache(video_id: &str, quality: &Quality, data: Bytes) {
    let path = thumbnail_path(video_id, quality);
    tokio::spawn(async move {
        let file = File::create(path).await;
        if let Err(e) = file {
            println!("Error creating thumbnail file: {e}");
            return;
        }
        if let Ok(mut file) = file {
            let result = file.write_all(&data).await;
            if let Err(e) = result {
                println!("Error writing thumbnail file: {e}");
            }
        }
    });
}

async fn fetch_from_cache(video_id: &str) -> Option<(Vec<u8>, Quality)> {
    for quality in SUPPORTED_QUALITIES {
        let path = thumbnail_path(video_id, &quality);
        if std::fs::metadata(&path).is_ok() {
            let data = match std::fs::read(&path) {
                Ok(data) => data,
                Err(e) => {
                    println!("Error reading cached thumbnail: {}: {}", path.display(), e);
                    return None;
                }
            };
            return Some((data, quality));
        }
    }
    None
}

fn webp_response(data: Vec<u8>) -> Response<Body> {
    Response::builder()
        .header("Content-Type", "image/webp")
        .body(Body::from(data))
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
    fn test_thumbnail_dir() {
        assert_eq!(thumbnail_dir(), PathBuf::from("thumbnails"));
    }

    #[test]
    fn test_thumbnail_path() {
        assert_eq!(
            thumbnail_path("aGb3AlQrN9E", &Quality::Maxresdefault),
            PathBuf::from("thumbnails/maxresdefault/aGb3AlQrN9E.webp")
        );
        assert_eq!(
            thumbnail_path("aGb3AlQrN9E", &Quality::Sddefault),
            PathBuf::from("thumbnails/sddefault/aGb3AlQrN9E.webp")
        );
    }
}
