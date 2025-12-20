use redis::Commands;
use s3::{creds::Credentials, request::ResponseData};
use std::boxed::Box;

pub type RedisPool = r2d2::Pool<redis::Client>;

pub async fn redis_pool() -> Box<RedisPool> {
    let url = std::env::var("REDIS_URL").unwrap();
    let client = redis::Client::open(url).unwrap();

    let pool = r2d2::Pool::builder()
        .max_size(10)
        .build(client)
        .expect("Failed to create Redis connection pool");
    Box::new(pool)
}

pub async fn put_redis_object(
    pool: &RedisPool,
    key: &str,
    value: &str,
) -> Result<(), redis::RedisError> {
    let mut client = pool.get().unwrap();
    client.set::<&str, &str, ()>(key, value).unwrap();
    Ok(())
}

pub async fn get_redis_object(pool: &RedisPool, key: &str) -> Option<String> {
    let mut client = pool.get().unwrap();
    client.get::<&str, Option<String>>(key).unwrap()
}

fn s3_region() -> s3::Region {
    s3::Region::Custom {
        region: std::env::var("S3_REGION").unwrap(),
        endpoint: std::env::var("S3_ENDPOINT").unwrap(),
    }
}

pub async fn s3_connection() -> s3::Bucket {
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

pub async fn put_s3_object(
    bucket: &s3::Bucket,
    key: &str,
    content: &[u8],
) -> Result<(), s3::error::S3Error> {
    bucket.put_object(key, content).await?;
    Ok(())
}

pub async fn get_s3_object(
    bucket: &s3::Bucket,
    key: &str,
) -> Result<ResponseData, s3::error::S3Error> {
    bucket.get_object(key).await
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[tokio::test]
//     async fn test_s3_connection() {
//         dotenv::dotenv().ok();
//         let content = "Hello, world!".as_bytes();
//         let key = "test.txt";

//         let bucket = get_connection().await;
//         bucket.put_object(key, content).await.unwrap();

//         let result = bucket.get_object(key).await.unwrap();
//         assert_eq!(result.bytes(), content);

//         bucket.delete_object(key).await.unwrap();
//     }
// }
