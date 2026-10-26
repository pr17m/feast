//! The restaurant's printed QR code. It holds `/r/{short_code}` and opens the
//! restaurant's page: a diner with a Feast booking there today pays the bill
//! and gets the deal; anyone else is invited to book. QR codes printed before
//! table ordering was removed hold `{short_code}-{table}` and still resolve.

use feast_domain::model::RestaurantQr;

use crate::context::Feast;
use crate::error::{AppError, AppResult, ErrorKind};

/// The restaurant part of a scanned code: `oae1` or a legacy table code `oae1-t7`.
pub fn restaurant_code(scanned: &str) -> String {
    scanned.trim().split('-').next().unwrap_or_default().to_lowercase()
}

impl Feast {
    /// Public: resolve a scanned restaurant QR to the restaurant diners may see.
    pub async fn restaurant_qr(&self, scanned: &str) -> AppResult<RestaurantQr> {
        let code = restaurant_code(scanned);
        let not_found = || AppError::new(ErrorKind::NotFound, "QR_NOT_FOUND", "This QR code isn't linked to a restaurant on Feast");
        if code.is_empty() || code.len() > 32 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(not_found());
        }
        match self.p.restaurants.find_by_short_code(&code).await? {
            Some(r) if self.is_listed(&r.detail).await? => Ok(RestaurantQr { code, restaurant: r.customer_view().detail.summary() }),
            _ => Err(not_found()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::restaurant_code;

    #[test]
    fn reads_new_and_legacy_codes() {
        assert_eq!(restaurant_code("OAE1"), "oae1");
        assert_eq!(restaurant_code(" oae1-t7 "), "oae1");
        assert_eq!(restaurant_code(""), "");
    }
}
