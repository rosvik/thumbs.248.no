use std::path::{Path, PathBuf};

use redis::Commands;
use s3::creds::Credentials;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // List all files in ./thumbnails
    let dirs = vec![
        PathBuf::from("../thumbnails/sddefault/jpg"),
        PathBuf::from("../thumbnails/sddefault/webp"),
        PathBuf::from("../thumbnails/hqdefault/jpg"),
        PathBuf::from("../thumbnails/hqdefault/webp"),
        PathBuf::from("../thumbnails/maxresdefault/jpg"),
        PathBuf::from("../thumbnails/maxresdefault/webp"),
    ];

    let redis_client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
    let mut redis_conn = redis_client.get_connection().unwrap();

    let bucket = s3_connection();

    for dir in dirs {
        let files = std::fs::read_dir(dir).unwrap();
        for file in files {
            let now = std::time::Instant::now();
            let file = file.unwrap();
            let path = file.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            let yt_id = file_name.split('.').next().unwrap();
            let s3_key = s3_key(&path);
            redis_conn
                .set::<&str, String, ()>(yt_id, s3_key.clone())
                .unwrap();
            bucket
                .put_object(&s3_key, std::fs::read(file.path()).unwrap().as_slice())
                .await
                .unwrap();
            println!("Uploaded {} to S3 in {:?}", s3_key, now.elapsed());
        }
    }
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
