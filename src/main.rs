use axum::{
    Router,
    body::Body,
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
}
impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
}

const DEFAULT_THUMBNAIL_DIR: &str = "thumbnails";
fn thumbnail_dir() -> PathBuf {
    std::env::var("THUMBNAIL_DIR")
        .unwrap_or(DEFAULT_THUMBNAIL_DIR.to_string())
        .into()
}
fn thumbnail_path(video_id: &str, quality: Quality) -> PathBuf {
    thumbnail_dir()
        .join(quality.to_string())
        .join(format!("{video_id}.webp"))
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
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
    let thumbnails = std::fs::read_dir(thumbnail_dir).unwrap();
    let thumbnails = thumbnails
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
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
    if let Some(cached_data) = cached_data {
        println!("Returning cached thumbnail for {video_id}");
        return webp_response(cached_data);
    }

    let url = format!("https://i.ytimg.com/vi_webp/{video_id}/maxresdefault.webp");
    let response = match reqwest::get(&url).await {
        Ok(response) => response,
        Err(e) => {
            println!("Error fetching thumbnail: {url}: {e}");
            return fallback_response(500);
        }
    };

    if response.status() != 200 {
        println!("Error fetching thumbnail: {url}: {}", response.status());
        return fallback_response(response.status().into());
    }

    let body = response.bytes().await.unwrap();

    // Save the image to a file
    let path = thumbnail_path(&video_id, Quality::Maxresdefault);
    let file_data = body.clone();
    tokio::spawn(async move {
        let file = File::create(path).await;
        if let Err(e) = file {
            println!("Error creating thumbnail file: {e}");
            return;
        }
        if let Ok(mut file) = file {
            let result = file.write_all(&file_data).await;
            if let Err(e) = result {
                println!("Error writing thumbnail file: {e}");
            }
        }
    });

    println!("Fetched new thumbnail for {}", video_id);
    webp_response(body.to_vec())
}

async fn fetch_from_cache(video_id: &str) -> Option<Vec<u8>> {
    let path = thumbnail_path(video_id, Quality::Maxresdefault);
    if std::fs::metadata(&path).is_ok() {
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) => {
                println!("Error reading cached thumbnail: {}: {}", path.display(), e);
                return None;
            }
        };
        return Some(data);
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
            thumbnail_path("aGb3AlQrN9E", Quality::Maxresdefault),
            PathBuf::from("thumbnails/maxresdefault/aGb3AlQrN9E.webp")
        );
    }
}
