use s3::{creds::Credentials, request::ResponseData};
use std::boxed::Box;

fn region() -> s3::Region {
    s3::Region::Custom {
        region: std::env::var("S3_REGION").unwrap(),
        endpoint: std::env::var("S3_ENDPOINT").unwrap(),
    }
}

pub async fn get_connection() -> Box<s3::Bucket> {
    let credentials = Credentials {
        access_key: Some(std::env::var("S3_ACCESS_KEY").unwrap()),
        secret_key: Some(std::env::var("S3_SECRET_KEY").unwrap()),
        expiration: None,
        security_token: None,
        session_token: None,
    };
    s3::Bucket::new(&std::env::var("S3_BUCKET").unwrap(), region(), credentials).unwrap()
}

pub async fn put_object(
    bucket: &s3::Bucket,
    key: &str,
    content: &[u8],
) -> Result<(), s3::error::S3Error> {
    bucket.put_object(key, content).await?;
    Ok(())
}

pub async fn get_object(
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

//         let access_key = std::env::var("S3_ACCESS_KEY").unwrap();
//         let secret_key = std::env::var("S3_SECRET_KEY").unwrap();
//         let region = std::env::var("S3_REGION").unwrap();
//         let endpoint = std::env::var("S3_ENDPOINT").unwrap();
//         let bucket = std::env::var("S3_BUCKET").unwrap();

//         let credentials = Credentials {
//             access_key: Some(access_key),
//             secret_key: Some(secret_key),
//             expiration: None,
//             security_token: None,
//             session_token: None,
//         };
//         let region = s3::Region::Custom { region, endpoint };

//         let s3 = s3::Bucket::new(&bucket, region, credentials).unwrap();

//         let content = "Hello, world!".as_bytes();
//         s3.put_object("/test.txt", content).await.unwrap();
//         let result = s3.get_object("/test.txt").await.unwrap();
//         assert_eq!(result.bytes(), content);

//         s3.delete_object("/test.txt").await.unwrap();
//     }
// }
