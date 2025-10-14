/*
This test suite exists to provide one example for each supported quality where
the quality is the best available. Because if every quality supported CAN be
the best, we have to go through every version to find the best. If we can't find
and example where the quality is the best, it might be a waste to check it.
*/

/// maxresdefault.webp can be best
/// <https://i.ytimg.com/vi_webp/aGb3AlQrN9E/maxresdefault.webp>
#[tokio::test]
async fn test_maxresdefault_webp() {
    let id = "aGb3AlQrN9E";
    assert_eq!(get(id, "maxresdefault", "webp").await, 200);
}

/// maxresdefault.jpg can be best
/// <https://i.ytimg.com/vi/v5Zo0mUO-GE/maxresdefault.jpg>
#[tokio::test]
async fn test_maxresdefault_jpg() {
    let id = "v5Zo0mUO-GE";
    assert_eq!(get(id, "maxresdefault", "webp").await, 404);
    assert_eq!(get(id, "maxresdefault", "jpg").await, 200);
}

/// sddefault.webp can be best
/// <https://i.ytimg.com/vi_webp/OVoqDpjN_Do/sddefault.webp>
#[tokio::test]
async fn test_sddefault_webp() {
    let id = "OVoqDpjN_Do";
    assert_eq!(get(id, "maxresdefault", "webp").await, 404);
    assert_eq!(get(id, "maxresdefault", "jpg").await, 404);
    assert_eq!(get(id, "sddefault", "webp").await, 200);
}

/// sddefault.jpg can be best
/// <https://i.ytimg.com/vi/vgD1tVd9ubA/sddefault.jpg>
#[tokio::test]
async fn test_sddefault_jpg() {
    let id = "vgD1tVd9ubA";
    assert_eq!(get(id, "maxresdefault", "webp").await, 404);
    assert_eq!(get(id, "maxresdefault", "jpg").await, 404);
    assert_eq!(get(id, "sddefault", "webp").await, 404);
    assert_eq!(get(id, "sddefault", "jpg").await, 200);
}

/// hqdefault.webp can be best
/// <https://i.ytimg.com/vi_webp/6xKLBne1CoI/hqdefault.webp>
#[tokio::test]
async fn test_hqdefault_webp() {
    let id = "6xKLBne1CoI";
    assert_eq!(get(id, "maxresdefault", "webp").await, 404);
    assert_eq!(get(id, "maxresdefault", "jpg").await, 404);
    assert_eq!(get(id, "sddefault", "webp").await, 404);
    assert_eq!(get(id, "sddefault", "jpg").await, 404);
    assert_eq!(get(id, "hqdefault", "webp").await, 200);
}

/// hqdefault.jpg can be best
/// <https://i.ytimg.com/vi/VLM5ECY07nw/hqdefault.jpg>
#[tokio::test]
async fn test_hqdefault_jpg() {
    let id = "VLM5ECY07nw";
    assert_eq!(get(id, "maxresdefault", "webp").await, 404);
    assert_eq!(get(id, "maxresdefault", "jpg").await, 404);
    assert_eq!(get(id, "sddefault", "webp").await, 404);
    assert_eq!(get(id, "sddefault", "jpg").await, 404);
    assert_eq!(get(id, "hqdefault", "webp").await, 404);
    assert_eq!(get(id, "hqdefault", "jpg").await, 200);
}

async fn get(id: &str, quality: &str, format: &str) -> reqwest::StatusCode {
    async fn get_status(url: String) -> reqwest::StatusCode {
        reqwest::get(url).await.unwrap().status()
    }
    match format {
        "webp" => get_status(format!("https://i.ytimg.com/vi_webp/{id}/{quality}.webp",)).await,
        "jpg" => get_status(format!("https://i.ytimg.com/vi/{id}/{quality}.jpg")).await,
        _ => panic!("Unsupported format: {}", format),
    }
}
