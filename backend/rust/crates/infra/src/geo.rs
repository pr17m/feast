//! Visitor geolocation (country only), to show the right country's config or
//! point visitors to their country's deployment.
//!
//! - [`HeaderGeoLocator`]: country header set by the CDN/load balancer
//!   (Cloudflare `cf-ipcountry`, CloudFront `cloudfront-viewer-country`,
//!   Vercel `x-vercel-ip-country`, GCP `x-appengine-country`, or a custom one).
//!   Only trust these when the CDN is the only way in.
//! - [`MaxMindGeoLocator`]: a GeoLite2/GeoIP2 Country database on disk.
//! - [`ChainGeoLocator`]: try several in order.

use std::path::Path;

use feast_application::ports::{GeoLocator, GeoRequest};
use feast_application::{AppError, AppResult};

pub const DEFAULT_HEADERS: [&str; 5] = ["cf-ipcountry", "cloudfront-viewer-country", "x-vercel-ip-country", "x-appengine-country", "x-country-code"];

fn country(code: &str) -> Option<String> {
    let c = code.trim().to_uppercase();
    // "XX" / "T1" are Cloudflare's unknown / Tor markers.
    (c.len() == 2 && c.chars().all(|ch| ch.is_ascii_uppercase()) && c != "XX").then_some(c)
}

pub struct HeaderGeoLocator {
    headers: Vec<String>,
}

impl HeaderGeoLocator {
    pub fn new(headers: Vec<String>) -> Self {
        Self { headers: headers.into_iter().map(|h| h.to_lowercase()).collect() }
    }
}

impl Default for HeaderGeoLocator {
    fn default() -> Self {
        Self::new(DEFAULT_HEADERS.iter().map(|s| s.to_string()).collect())
    }
}

impl GeoLocator for HeaderGeoLocator {
    fn locate(&self, req: &GeoRequest) -> Option<(String, &'static str)> {
        self.headers.iter().find_map(|name| req.headers.iter().find(|(k, _)| k == name).and_then(|(_, v)| country(v))).map(|c| (c, "header"))
    }
}

pub struct MaxMindGeoLocator {
    reader: maxminddb::Reader<Vec<u8>>,
}

impl MaxMindGeoLocator {
    pub fn open(path: &Path) -> AppResult<Self> {
        let reader = maxminddb::Reader::open_readfile(path).map_err(|e| AppError::internal(format!("geo.maxmind_db {}: {e}", path.display())))?;
        Ok(Self { reader })
    }
}

impl GeoLocator for MaxMindGeoLocator {
    fn locate(&self, req: &GeoRequest) -> Option<(String, &'static str)> {
        let ip = req.ip?;
        let rec: maxminddb::geoip2::Country = self.reader.lookup(ip).ok()?;
        rec.country.and_then(|c| c.iso_code).and_then(country).map(|c| (c, "geoip"))
    }
}

pub struct ChainGeoLocator(pub Vec<Box<dyn GeoLocator>>);

impl GeoLocator for ChainGeoLocator {
    fn locate(&self, req: &GeoRequest) -> Option<(String, &'static str)> {
        self.0.iter().find_map(|g| g.locate(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cdn_headers() {
        let g = HeaderGeoLocator::default();
        let req = GeoRequest { ip: None, headers: vec![("cf-ipcountry".into(), "au".into())] };
        assert_eq!(g.locate(&req), Some(("AU".into(), "header")));
        let unknown = GeoRequest { ip: None, headers: vec![("cf-ipcountry".into(), "XX".into())] };
        assert_eq!(g.locate(&unknown), None);
    }
}
