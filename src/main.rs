use axum::{
    Router, body::Body, extract::Path, http::Response, response::IntoResponse, routing::get,
};
use regex::Regex;
use tokio::{fs::File, io::AsyncWriteExt};

const THUMBNAIL_DIR: &str = "thumbnails";

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/{video_id}", get(get_thumbnail));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:2342").await.unwrap();
    println!("Listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
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
    let thumbnail_dir = std::env::var("THUMBNAIL_DIR").unwrap_or(THUMBNAIL_DIR.to_string());
    let path = format!("{thumbnail_dir}/{video_id}.webp");
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

async fn fetch_from_cache(video_id: &String) -> Option<Vec<u8>> {
    let path = format!("thumbnails/{video_id}.webp");
    if std::fs::metadata(&path).is_ok() {
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) => {
                println!("Error reading cached thumbnail: {}: {}", path, e);
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
