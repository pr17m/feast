//! Dish photos: uploaded files round-trip through local storage under the
//! public `media/` prefix, and the Unsplash finder against a fake Unsplash.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use feast_application::ports::{DishImageFinder, DishQuery, FileStorage};
use feast_domain::media::{media_key_from_path, media_url, menu_photo_key, ImageSource, MEDIA_ROUTE};
use feast_infra::media::{UnsplashConfig, UnsplashFinder};
use feast_infra::storage::LocalDiskStorage;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

#[tokio::test]
async fn uploaded_photo_round_trips_through_its_public_url() {
    let dir = std::env::temp_dir().join(format!("feast-media-test-{}", std::process::id()));
    let files = LocalDiskStorage::new(&dir);
    let key = menu_photo_key("r_harbour_vine", "r_harbour_vine_i0", "ph_01abc").unwrap();
    files.put(&key, PNG.to_vec(), "image/png").await.unwrap();

    // The URL handed to apps maps back to the same key, and only inside media/.
    let url = media_url(&key);
    let path = url.strip_prefix(MEDIA_ROUTE).unwrap();
    let back = media_key_from_path(path).unwrap();
    assert_eq!(back, key);
    let (bytes, ct) = files.get(&back).await.unwrap();
    assert_eq!((bytes.as_slice(), ct.as_str()), (PNG, "image/png"));

    // A private document stored next to it can't be reached through the media route.
    files.put("documents/r_harbour_vine/doc1", b"%PDF-1.7".to_vec(), "application/pdf").await.unwrap();
    assert!(media_key_from_path("../documents/r_harbour_vine/doc1").is_none());
    assert_ne!(media_key_from_path("documents/r_harbour_vine/doc1").unwrap(), "documents/r_harbour_vine/doc1");
    let _ = std::fs::remove_dir_all(dir);
}

/// Fake Unsplash: search returns a premium photo, an unrelated one and a match.
fn fake_unsplash() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    let base = addr.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut s = stream.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            log.lock().unwrap().push(req.lines().next().unwrap_or_default().to_owned());
            let body = if req.starts_with("GET /search/photos") {
                serde_json::json!({ "results": [
                    { "urls": { "raw": "https://plus.unsplash.com/premium_photo-1?ixid=p" }, "links": {}, "user": { "name": "P", "username": "p" },
                      "alt_description": "masala dosa", "premium": true, "plus": true },
                    { "urls": { "raw": "https://images.unsplash.com/photo-2?ixid=m" }, "links": {}, "user": { "name": "M", "username": "m" },
                      "alt_description": "mountain lake" },
                    { "urls": { "raw": "https://images.unsplash.com/photo-3?ixid=d" }, "links": { "download_location": format!("{base}/photos/3/download") },
                      "user": { "name": "Dee Sa", "username": "deesa" }, "alt_description": "crispy masala dosa with chutney" }
                ]})
                .to_string()
            } else {
                "{}".to_owned()
            };
            let _ = write!(s, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
        }
    });
    (addr, seen)
}

#[tokio::test]
async fn unsplash_finder_picks_a_free_matching_photo_and_reports_the_use() {
    let (base, seen) = fake_unsplash();
    let finder = UnsplashFinder::new(UnsplashConfig { access_key: "k".into(), app_name: "feast".into(), api_url: base, width: 640 }).unwrap();
    assert!(finder.enabled());
    let q = DishQuery {
        restaurant_id: "r".into(),
        item_id: "i".into(),
        dish: "Masala dosa".into(),
        description: String::new(),
        cuisine: Some("South Indian".into()),
    };
    let found = finder.find(&q).await.unwrap().unwrap();
    assert_eq!(found.url, "https://images.unsplash.com/photo-3?ixid=d&auto=format&fit=crop&w=640&q=80");
    assert_eq!(found.source, ImageSource::Stock);
    assert!(found.credit.unwrap().starts_with("Dee Sa · Unsplash"));
    // The download_location ping runs in the background.
    for _ in 0..50 {
        if seen.lock().unwrap().iter().any(|l| l.starts_with("GET /photos/3/download")) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let lines = seen.lock().unwrap().clone();
    assert!(lines[0].starts_with("GET /search/photos?query=Masala+dosa+South+Indian+food"), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("GET /photos/3/download")), "{lines:?}");

    // Nothing in the results mentions the dish → no photo.
    let q = DishQuery { dish: "Pavlova".into(), ..q };
    assert!(finder.find(&q).await.unwrap().is_none());
}
