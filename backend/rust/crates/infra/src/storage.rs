//! File storage adapters (onboarding documents).
//!
//! - [`LocalDiskStorage`]: a directory (development, single-server installs).
//! - [`S3Storage`]: any S3-compatible bucket — AWS S3, Google Cloud Storage
//!   (interoperability endpoint + HMAC keys), Cloudflare R2, MinIO…
//! - [`AzureBlobStorage`]: Azure Blob Storage (Shared Key or SAS token).
//!
//! One bucket/container per country deployment so documents stay in-region.

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use feast_application::ports::FileStorage;
use feast_application::{AppError, AppResult};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use serde::Deserialize;

/// Keys are `documents/<restaurant>/<doc id>`; refuse anything that could escape the root.
fn safe_key(key: &str) -> AppResult<&str> {
    let ok = !key.is_empty()
        && Path::new(key).components().all(|c| matches!(c, Component::Normal(_)))
        && key.chars().all(|c| c.is_ascii_alphanumeric() || "/_-.".contains(c));
    if ok {
        Ok(key)
    } else {
        Err(AppError::internal(format!("invalid storage key {key:?}")))
    }
}

pub struct LocalDiskStorage {
    root: PathBuf,
}

impl LocalDiskStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl FileStorage for LocalDiskStorage {
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> AppResult<()> {
        let path = self.root.join(safe_key(key)?);
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(|e| AppError::internal(format!("storage: {e}")))?;
        }
        tokio::fs::write(&path, bytes).await.map_err(|e| AppError::internal(format!("storage: {e}")))?;
        tokio::fs::write(path.with_extension("type"), content_type).await.map_err(|e| AppError::internal(format!("storage: {e}")))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> AppResult<(Vec<u8>, String)> {
        let path = self.root.join(safe_key(key)?);
        let bytes = tokio::fs::read(&path).await.map_err(|_| AppError::not_found("File"))?;
        let ct = tokio::fs::read_to_string(path.with_extension("type")).await.unwrap_or_else(|_| "application/octet-stream".into());
        Ok((bytes, ct))
    }

    async fn health(&self) -> AppResult<()> {
        tokio::fs::create_dir_all(&self.root).await.map_err(|e| AppError::internal(format!("storage dir {}: {e}", self.root.display())))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct S3Config {
    /// e.g. `https://s3.ap-south-1.amazonaws.com` or `https://<account>.r2.cloudflarestorage.com`.
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    /// Empty = the standard AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / AWS_SESSION_TOKEN env vars.
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    /// Path-style URLs (MinIO); AWS and R2 work with virtual-host style.
    #[serde(default)]
    pub path_style: bool,
    /// Prefix inside the bucket, e.g. `feast-au/`.
    #[serde(default)]
    pub prefix: String,
}

pub struct S3Storage {
    bucket: Bucket,
    creds: Credentials,
    prefix: String,
    http: reqwest::Client,
}

const PRESIGN: Duration = Duration::from_secs(60);

impl S3Storage {
    pub fn new(cfg: S3Config) -> AppResult<Self> {
        let endpoint = cfg.endpoint.parse().map_err(|e| AppError::internal(format!("storage.s3.endpoint: {e}")))?;
        let style = if cfg.path_style { UrlStyle::Path } else { UrlStyle::VirtualHost };
        let bucket = Bucket::new(endpoint, style, cfg.bucket, cfg.region).map_err(|e| AppError::internal(format!("storage.s3: {e}")))?;
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().map_err(|e| AppError::internal(e.to_string()))?;
        let creds = if cfg.access_key_id.is_empty() {
            Credentials::from_env()
                .ok_or_else(|| AppError::internal("storage.s3: set access_key_id/secret_access_key or the AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY env vars"))?
        } else {
            Credentials::new(cfg.access_key_id, cfg.secret_access_key)
        };
        Ok(Self { bucket, creds, prefix: cfg.prefix, http })
    }

    fn object(&self, key: &str) -> AppResult<String> {
        Ok(format!("{}{}", self.prefix, safe_key(key)?))
    }
}

fn s3_err(e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("s3: {e}"))
}

#[async_trait]
impl FileStorage for S3Storage {
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> AppResult<()> {
        let obj = self.object(key)?;
        let url = self.bucket.put_object(Some(&self.creds), &obj).sign(PRESIGN);
        let res = self.http.put(url).header("content-type", content_type).body(bytes).send().await.map_err(s3_err)?;
        if !res.status().is_success() {
            return Err(s3_err(format!("put {obj}: {}", res.status())));
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> AppResult<(Vec<u8>, String)> {
        let obj = self.object(key)?;
        let url = self.bucket.get_object(Some(&self.creds), &obj).sign(PRESIGN);
        let res = self.http.get(url).send().await.map_err(s3_err)?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::not_found("File"));
        }
        if !res.status().is_success() {
            return Err(s3_err(format!("get {obj}: {}", res.status())));
        }
        let ct = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_owned();
        let bytes = res.bytes().await.map_err(s3_err)?.to_vec();
        Ok((bytes, ct))
    }

    async fn health(&self) -> AppResult<()> {
        let mut action = self.bucket.list_objects_v2(Some(&self.creds));
        action.with_max_keys(1);
        let res = self.http.get(action.sign(PRESIGN)).send().await.map_err(s3_err)?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(s3_err(format!("bucket check: {}", res.status())))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzureBlobConfig {
    /// Storage account name.
    pub account: String,
    pub container: String,
    /// Account key (base64), for Shared Key auth…
    #[serde(default)]
    pub access_key: String,
    /// …or a SAS token (`sv=…&sig=…`) with read/write on the container.
    #[serde(default)]
    pub sas_token: String,
    /// Default `https://<account>.blob.core.windows.net`; set for Azurite or sovereign clouds.
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub prefix: String,
}

pub struct AzureBlobStorage {
    cfg: AzureBlobConfig,
    key: Vec<u8>,
    base: String,
    http: reqwest::Client,
}

const AZURE_VERSION: &str = "2021-08-06";

impl AzureBlobStorage {
    pub fn new(cfg: AzureBlobConfig) -> AppResult<Self> {
        use base64::Engine;
        if cfg.account.is_empty() || cfg.container.is_empty() || (cfg.access_key.is_empty() && cfg.sas_token.is_empty()) {
            return Err(AppError::internal("storage.azure: account, container and access_key or sas_token are required"));
        }
        let key = if cfg.access_key.is_empty() {
            vec![]
        } else {
            base64::engine::general_purpose::STANDARD
                .decode(cfg.access_key.trim())
                .map_err(|_| AppError::internal("storage.azure.access_key must be base64"))?
        };
        let endpoint =
            if cfg.endpoint.is_empty() { format!("https://{}.blob.core.windows.net", cfg.account) } else { cfg.endpoint.trim_end_matches('/').to_owned() };
        let base = format!("{endpoint}/{}", cfg.container);
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().map_err(|e| AppError::internal(e.to_string()))?;
        Ok(Self { cfg, key, base, http })
    }

    fn blob(&self, key: &str) -> AppResult<String> {
        Ok(format!("{}{}", self.cfg.prefix, safe_key(key)?))
    }

    /// Create the container if it doesn't exist (needs Shared Key or an account SAS).
    pub async fn ensure_container(&self) -> AppResult<()> {
        let res = self.request(reqwest::Method::PUT, "", &[("restype", "container")], None).header("content-length", 0).send().await.map_err(az_err)?;
        match res.status().as_u16() {
            200..=299 | 409 => Ok(()),
            s => Err(az_err(format!("create container: {s} {}", res.text().await.unwrap_or_default()))),
        }
    }

    /// Shared Key signature (https://learn.microsoft.com/rest/api/storageservices/authorize-with-shared-key).
    fn sign(&self, verb: &str, path: &str, query: &[(&str, &str)], len: usize, content_type: &str, headers: &[(&str, String)]) -> String {
        use base64::Engine;
        use hmac::{Hmac, Mac};
        let mut canon_headers: Vec<String> = headers.iter().map(|(k, v)| format!("{}:{}", k.to_lowercase(), v)).collect();
        canon_headers.sort();
        // Azurite (path-style endpoint) puts the account in the path too.
        let mut resource = format!("/{}{}", self.cfg.account, path);
        let mut q: Vec<(&str, &str)> = query.to_vec();
        q.sort();
        for (k, v) in q {
            resource.push_str(&format!("\n{}:{}", k.to_lowercase(), v));
        }
        let length = if len == 0 { String::new() } else { len.to_string() };
        let to_sign = format!("{verb}\n\n\n{length}\n\n{content_type}\n\n\n\n\n\n\n{}\n{resource}", canon_headers.join("\n"));
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&self.key).expect("hmac key");
        mac.update(to_sign.as_bytes());
        format!("SharedKey {}:{}", self.cfg.account, base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
    }

    fn request(&self, verb: reqwest::Method, blob: &str, query: &[(&str, &str)], body: Option<(Vec<u8>, &str)>) -> reqwest::RequestBuilder {
        let url = format!("{}/{}", self.base, blob);
        let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let mut headers = vec![("x-ms-date", date), ("x-ms-version", AZURE_VERSION.to_owned())];
        if body.is_some() {
            headers.push(("x-ms-blob-type", "BlockBlob".to_owned()));
        }
        let (len, ct) = body.as_ref().map(|(b, ct)| (b.len(), *ct)).unwrap_or((0, ""));
        let mut qs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let mut auth = None;
        if self.cfg.sas_token.is_empty() {
            let path = url::Url::parse(&url).map(|u| u.path().to_owned()).unwrap_or_default();
            auth = Some(self.sign(verb.as_str(), &path, query, len, ct, &headers));
        } else {
            qs.push(self.cfg.sas_token.trim_start_matches('?').to_owned());
        }
        let full = if qs.is_empty() { url } else { format!("{url}?{}", qs.join("&")) };
        let mut req = self.http.request(verb, full);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        if let Some(a) = auth {
            req = req.header("authorization", a);
        }
        if let Some((bytes, ct)) = body {
            req = req.header("content-type", ct).header("content-length", bytes.len()).body(bytes);
        }
        req
    }
}

#[async_trait]
impl FileStorage for AzureBlobStorage {
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> AppResult<()> {
        let blob = self.blob(key)?;
        let res = self.request(reqwest::Method::PUT, &blob, &[], Some((bytes, content_type))).send().await.map_err(az_err)?;
        if !res.status().is_success() {
            let status = res.status();
            return Err(az_err(format!("put {blob}: {status} {}", res.text().await.unwrap_or_default())));
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> AppResult<(Vec<u8>, String)> {
        let blob = self.blob(key)?;
        let res = self.request(reqwest::Method::GET, &blob, &[], None).send().await.map_err(az_err)?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(AppError::not_found("File"));
        }
        if !res.status().is_success() {
            return Err(az_err(format!("get {blob}: {}", res.status())));
        }
        let ct = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_owned();
        Ok((res.bytes().await.map_err(az_err)?.to_vec(), ct))
    }

    async fn health(&self) -> AppResult<()> {
        // List at most one blob: proves the container exists and credentials work.
        let url_query = [("comp", "list"), ("maxresults", "1"), ("restype", "container")];
        let res = self.request(reqwest::Method::GET, "", &url_query, None).send().await.map_err(az_err)?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(az_err(format!("container check: {}", res.status())))
        }
    }
}

fn az_err(e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("azure blob: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert!(safe_key("documents/r_1/doc_1").is_ok());
        assert!(safe_key("../etc/passwd").is_err());
        assert!(safe_key("/abs").is_err());
        assert!(safe_key("a b").is_err());
    }

    #[tokio::test]
    async fn local_round_trip() {
        let dir = std::env::temp_dir().join(format!("feast-storage-{}", std::process::id()));
        let s = LocalDiskStorage::new(&dir);
        s.put("documents/r1/d1", b"%PDF-1.4".to_vec(), "application/pdf").await.unwrap();
        let (b, ct) = s.get("documents/r1/d1").await.unwrap();
        assert_eq!(b, b"%PDF-1.4");
        assert_eq!(ct, "application/pdf");
        let _ = std::fs::remove_dir_all(dir);
    }
}
