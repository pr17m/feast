//! Dish photos: who supplied a menu item's picture, where uploaded photos live,
//! and when Feast may look for a stock photo. Pure rules; storage and the
//! stock-photo search are ports in the application layer.
//!
//! Order of preference for what a diner sees:
//! 1. **Owner** — a photo the restaurant uploaded (always wins).
//! 2. **Stock** — a matching stock photo Feast found (owners can reject it).
//! 3. **None** — no photo; the apps draw the generated cuisine art instead.
//!
//! An AI-generated image would slot in as another finder after stock (see
//! `DishImageFinder` in the application ports); its source would be `Generated`.

use serde::{Deserialize, Serialize};

use crate::model::MenuItem;

/// Where a menu item's photo came from.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImageSource {
    /// No photo: the apps show generated art for the cuisine.
    #[default]
    None,
    /// Uploaded by the restaurant.
    Owner,
    /// A stock photo Feast matched to the dish name.
    Stock,
    /// Made by an image generator (no generator is configured yet).
    Generated,
}

impl ImageSource {
    pub fn is_none(&self) -> bool {
        *self == ImageSource::None
    }
}

/// Largest photo an owner can upload.
pub const MAX_MENU_PHOTO_BYTES: usize = 5 * 1024 * 1024;
/// Upload formats we accept (checked by sniffing the bytes, not the file name).
pub const MENU_PHOTO_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];
/// Storage prefix for files anyone may read. Documents and case photos live
/// under other prefixes and are never served by the public media route.
pub const MEDIA_PREFIX: &str = "media/";
/// Public route that serves `media/…` keys.
pub const MEDIA_ROUTE: &str = "/v1/media/";
/// Stock lookups per worker sweep (keeps within the stock API's hourly quota).
pub const STOCK_LOOKUPS_PER_SWEEP: usize = 20;

fn safe_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
}

/// Storage key for an uploaded dish photo: `media/menu/<restaurant>/<item>/<photo>`.
/// Each upload gets a new photo id, so URLs can be cached forever.
pub fn menu_photo_key(restaurant_id: &str, item_id: &str, photo_id: &str) -> Option<String> {
    [restaurant_id, item_id, photo_id].iter().all(|s| safe_segment(s)).then(|| format!("{MEDIA_PREFIX}menu/{restaurant_id}/{item_id}/{photo_id}"))
}

/// The public URL (relative to the API) for a media key.
pub fn media_url(key: &str) -> String {
    format!("{MEDIA_ROUTE}{}", key.strip_prefix(MEDIA_PREFIX).unwrap_or(key))
}

/// Turn the path after `/v1/media/` back into a storage key, refusing anything
/// that could reach outside the public media area.
pub fn media_key_from_path(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').collect();
    let ok = !path.is_empty() && segments.len() <= 6 && segments.iter().all(|s| safe_segment(s));
    ok.then(|| format!("{MEDIA_PREFIX}{path}"))
}

/// Is this URL one of our uploaded photos (rather than a stock photo URL)?
pub fn is_uploaded_url(url: &str) -> bool {
    url.starts_with(MEDIA_ROUTE)
}

/// Words that describe portion or packaging rather than the dish.
fn noise(word: &str) -> bool {
    word.split('-').all(noise_part)
}

fn noise_part(word: &str) -> bool {
    let w = word.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase();
    w.is_empty()
        || w.chars().next().is_some_and(|c| c.is_ascii_digit())
        || matches!(w.as_str(), "non" | "alcoholic" | "regular" | "large" | "small" | "half" | "full" | "plate" | "pc" | "pcs")
}

/// Search words for a stock photo: the dish name without sizes or
/// qualifiers, plus the restaurant's main cuisine for context.
pub fn stock_query(dish: &str, cuisine: Option<&str>) -> String {
    let dish: Vec<String> = dish.replace(['&', '(', ')', ','], " ").split_whitespace().filter(|w| !noise(w)).map(str::to_owned).collect();
    let mut q = dish.join(" ");
    if let Some(c) = cuisine.map(str::trim).filter(|c| !c.is_empty()) {
        if !q.to_ascii_lowercase().contains(&c.to_ascii_lowercase()) {
            q = format!("{q} {c} food");
        }
    }
    q.trim().to_owned()
}

/// Effective source, treating photos saved before sources were tracked (seed
/// data) as stock photos.
pub fn source_of(item: &MenuItem) -> ImageSource {
    match (item.image_source, item.image.is_some()) {
        (ImageSource::None, true) => ImageSource::Stock,
        (s, true) => s,
        (_, false) => ImageSource::None,
    }
}

/// Should Feast look for a stock photo for this item? Only when it has no
/// photo and the owner hasn't turned stock photos down for it.
pub fn wants_stock(item: &MenuItem) -> bool {
    item.image.is_none() && !item.stock_declined
}

/// The owner uploaded a photo.
pub fn set_owner_photo(item: &mut MenuItem, url: String) {
    item.image = Some(url);
    item.image_source = ImageSource::Owner;
    item.image_credit = None;
    item.stock_declined = false;
}

/// Feast found a stock photo. Never replaces an owner's photo; returns whether it applied.
pub fn set_stock_photo(item: &mut MenuItem, url: String, credit: Option<String>) -> bool {
    if source_of(item) == ImageSource::Owner {
        return false;
    }
    item.image = Some(url);
    item.image_source = ImageSource::Stock;
    item.image_credit = credit;
    item.stock_declined = false;
    true
}

/// The owner removed the photo. Removing a stock photo also stops Feast
/// adding another one automatically (the owner can still ask for one).
pub fn remove_photo(item: &mut MenuItem) -> ImageSource {
    let was = source_of(item);
    item.image = None;
    item.image_source = ImageSource::None;
    item.image_credit = None;
    if was == ImageSource::Stock || was == ImageSource::Generated {
        item.stock_declined = true;
    }
    was
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> MenuItem {
        serde_json::from_value(serde_json::json!({
            "id": "i1", "name": "Masala dosa", "description": "", "price": 100, "isVeg": true, "available": true
        }))
        .unwrap()
    }

    #[test]
    fn keys_stay_inside_media() {
        assert_eq!(menu_photo_key("r_1", "r_1_i0", "ph_abc").as_deref(), Some("media/menu/r_1/r_1_i0/ph_abc"));
        assert_eq!(menu_photo_key("../x", "i", "p"), None);
        assert_eq!(menu_photo_key("r", "i/j", "p"), None);
        assert_eq!(media_url("media/menu/r/i/p"), "/v1/media/menu/r/i/p");
        assert_eq!(media_key_from_path("menu/r/i/p").as_deref(), Some("media/menu/r/i/p"));
        for bad in ["", "../documents/r/d", "menu/../../documents", "menu//x", "menu/a.b", "%2e%2e/x"] {
            assert_eq!(media_key_from_path(bad), None, "{bad}");
        }
        assert!(is_uploaded_url("/v1/media/menu/r/i/p"));
        assert!(!is_uploaded_url("https://images.unsplash.com/photo-1"));
    }

    #[test]
    fn stock_queries_drop_sizes() {
        assert_eq!(stock_query("Dry-aged ribeye 300g", Some("Steakhouse")), "Dry-aged ribeye Steakhouse food");
        assert_eq!(stock_query("Kombucha (non-alcoholic)", None), "Kombucha");
        assert_eq!(stock_query("Burrata & heirloom tomato", Some("Italian")), "Burrata heirloom tomato Italian food");
        assert_eq!(stock_query("Thai green curry", Some("Thai")), "Thai green curry");
    }

    #[test]
    fn source_transitions() {
        let mut i = item();
        assert_eq!(source_of(&i), ImageSource::None);
        assert!(wants_stock(&i));

        // Legacy seed photo counts as stock.
        i.image = Some("https://images.unsplash.com/photo-1".into());
        assert_eq!(source_of(&i), ImageSource::Stock);
        assert!(!wants_stock(&i));

        // Owner rejects it: no photo, and no automatic stock photo again.
        assert_eq!(remove_photo(&mut i), ImageSource::Stock);
        assert!(i.stock_declined && !wants_stock(&i));

        // Owner uploads: wins, clears the decline.
        set_owner_photo(&mut i, "/v1/media/menu/r/i/p".into());
        assert_eq!(source_of(&i), ImageSource::Owner);
        assert!(!i.stock_declined);
        // Stock never replaces an owner's photo.
        assert!(!set_stock_photo(&mut i, "https://x".into(), None));
        assert_eq!(i.image.as_deref(), Some("/v1/media/menu/r/i/p"));

        // Owner removes their own photo: stock may be looked up again.
        assert_eq!(remove_photo(&mut i), ImageSource::Owner);
        assert!(wants_stock(&i));
        assert!(set_stock_photo(&mut i, "https://images.unsplash.com/photo-2".into(), Some("Unsplash · @a".into())));
        assert_eq!(i.image_source, ImageSource::Stock);
    }
}
