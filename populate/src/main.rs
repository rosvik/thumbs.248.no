use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures::stream::{self, StreamExt};
use redis::Commands;
use s3::creds::Credentials;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let thumbnails_dir = std::env::var("THUMBNAILS_DIR").unwrap();
    // List all files in ./thumbnails
    let dirs = vec![
        PathBuf::from(format!("{}/sddefault/jpg", thumbnails_dir)),
        PathBuf::from(format!("{}/sddefault/webp", thumbnails_dir)),
        PathBuf::from(format!("{}/hqdefault/jpg", thumbnails_dir)),
        PathBuf::from(format!("{}/hqdefault/webp", thumbnails_dir)),
        PathBuf::from(format!("{}/maxresdefault/jpg", thumbnails_dir)),
        PathBuf::from(format!("{}/maxresdefault/webp", thumbnails_dir)),
    ];

    let redis_client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
    let redis_conn = Arc::new(Mutex::new(redis_client.get_connection().unwrap()));

    let bucket = Arc::new(s3_connection());

    // Collect all file paths first
    let mut all_files = Vec::new();
    for dir in dirs {
        println!("Collecting thumbnails from {}", dir.display());
        let files = std::fs::read_dir(dir).unwrap();
        for file in files {
            let file = file.unwrap();
            all_files.push(file.path());
        }
    }

    println!(
        "Processing {} files in parallel (10 at a time)",
        all_files.len()
    );

    // Process files in parallel with a concurrency limit of 10
    let futures = all_files.into_iter().map(|path| {
        let redis_conn = Arc::clone(&redis_conn);
        let bucket = Arc::clone(&bucket);

        async move {
            let now = std::time::Instant::now();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            let yt_id = file_name.split('.').next().unwrap();
            let s3_key = s3_key(&path);

            // Read file content
            let file_content = std::fs::read(&path).unwrap();

            // Set Redis key
            {
                let mut conn = redis_conn.lock().unwrap();
                match conn.set::<&str, String, ()>(yt_id, s3_key.clone()) {
                    Ok(_) => (),
                    Err(e) => {
                        // Append output to file
                        let mut file = std::fs::OpenOptions::new()
                            .append(true)
                            .open("error_redis.txt")
                            .unwrap();
                        file.write_all(
                            format!("ERROR: Error setting Redis key for {yt_id}: {e}\n").as_bytes(),
                        )
                        .unwrap();
                        println!("ERROR: Error setting Redis key for {yt_id}: {e}");
                    }
                };
            }

            // Upload to S3
            bucket
                .put_object(&s3_key, file_content.as_slice())
                .await
                .unwrap();

            println!("Uploaded {} to S3 in {:?}", s3_key, now.elapsed());
        }
    });

    // Process with concurrency limit of 10
    stream::iter(futures)
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;
}

fn s3_key(path: &Path) -> String {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    let yt_id = file_name.split('.').next().unwrap();
    let path_parts = path.components().collect::<Vec<_>>();
    let file_extension = path_parts
        .get(path_parts.len() - 2)
        .unwrap()
        .as_os_str()
        .to_str()
        .unwrap();
    let quality_str = path_parts
        .get(path_parts.len() - 3)
        .unwrap()
        .as_os_str()
        .to_str()
        .unwrap();
    format!("{yt_id}.{quality_str}.{file_extension}")
}

fn s3_region() -> s3::Region {
    s3::Region::Custom {
        region: std::env::var("S3_REGION").unwrap(),
        endpoint: std::env::var("S3_ENDPOINT").unwrap(),
    }
}
pub fn s3_connection() -> s3::Bucket {
    let credentials = Credentials {
        access_key: Some(std::env::var("S3_ACCESS_KEY").unwrap()),
        secret_key: Some(std::env::var("S3_SECRET_KEY").unwrap()),
        expiration: None,
        security_token: None,
        session_token: None,
    };
    let mut bucket = s3::Bucket::new(
        &std::env::var("S3_BUCKET").unwrap(),
        s3_region(),
        credentials,
    )
    .unwrap();
    if std::env::var("S3_PATH_STYLE").unwrap_or_default() == "true" {
        bucket.set_path_style();
    }
    *bucket
}
