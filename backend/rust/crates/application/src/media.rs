//! Dish photos: owners upload their own; otherwise Feast finds a matching
//! stock photo (when a finder is configured); otherwise the apps draw the
//! generated cuisine art. Uploaded photos are public files under `media/`.

use feast_domain::media::{self, ImageSource};
use feast_domain::model::{MenuItem, Restaurant, User};

use crate::context::Feast;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::ports::DishQuery;

/// An uploaded photo, as received.
pub struct PhotoUpload {
    pub bytes: Vec<u8>,
}

/// A public media file.
pub struct MediaFile {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

impl Feast {
    /// Owner (or staff with support access) uploads a photo for one dish.
    pub async fn upload_menu_photo(&self, user: &User, restaurant_id: &str, item_id: &str, up: PhotoUpload) -> AppResult<MenuItem> {
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        if r.find_item_mut(item_id).is_none() {
            return Err(AppError::not_found("Menu item"));
        }
        let limit = media::MAX_MENU_PHOTO_BYTES.min(self.settings.max_upload_bytes.max(1));
        if up.bytes.is_empty() || up.bytes.len() > limit {
            return Err(AppError::validation("FILE_TOO_LARGE", format!("Photos must be under {} MB", limit / (1024 * 1024))));
        }
        let Some(content_type) = crate::compliance::sniff(&up.bytes).filter(|t| media::MENU_PHOTO_TYPES.contains(t)) else {
            return Err(AppError::validation("INVALID_FILE_TYPE", "Upload a JPG, PNG or WebP photo"));
        };
        let photo_id = self.p.ids.new_id("ph");
        let key =
            media::menu_photo_key(restaurant_id, item_id, &photo_id).ok_or_else(|| AppError::validation("INVALID_ID", "That menu item can't have a photo"))?;
        self.p.files.put(&key, up.bytes, content_type).await?;
        let item = r.find_item_mut(item_id).expect("checked above");
        media::set_owner_photo(item, media::media_url(&key));
        let item = item.clone();
        self.p.restaurants.update(&r).await?;
        self.audit_staff_edit(user, restaurant_id, "menu.photo_uploaded", format!("{}: new photo", item.name)).await?;
        Ok(item)
    }

    /// Owner removes a dish's photo (theirs or a stock one).
    pub async fn remove_menu_photo(&self, user: &User, restaurant_id: &str, item_id: &str) -> AppResult<MenuItem> {
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        let item = r.find_item_mut(item_id).ok_or_else(|| AppError::not_found("Menu item"))?;
        let was = media::remove_photo(item);
        let item = item.clone();
        self.p.restaurants.update(&r).await?;
        let what = if was == ImageSource::Owner { "photo removed" } else { "stock photo turned down" };
        self.audit_staff_edit(user, restaurant_id, "menu.photo_removed", format!("{}: {what}", item.name)).await?;
        Ok(item)
    }

    /// Owner asks Feast to find a photo for a dish now ("Find a photo").
    pub async fn find_menu_photo(&self, user: &User, restaurant_id: &str, item_id: &str) -> AppResult<MenuItem> {
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        if !self.p.dish_images.enabled() {
            return Err(AppError::new(ErrorKind::Conflict, "STOCK_PHOTOS_OFF", "Stock photos aren't set up on this Feast deployment — upload your own photo"));
        }
        let cuisine = r.detail.cuisines.first().cloned();
        let q = {
            let item = r.find_item_mut(item_id).ok_or_else(|| AppError::not_found("Menu item"))?;
            if media::source_of(item) == ImageSource::Owner {
                return Err(AppError::conflict("HAS_OWN_PHOTO", "This dish already has your photo"));
            }
            dish_query(restaurant_id, cuisine, item)
        };
        let found = self.p.dish_images.find(&q).await?.ok_or_else(|| AppError::not_found("A matching photo"))?;
        let item = r.find_item_mut(item_id).expect("checked above");
        media::set_stock_photo(item, found.url, found.credit);
        item.image_source = found.source;
        let item = item.clone();
        self.p.restaurants.update(&r).await?;
        self.audit_staff_edit(user, restaurant_id, "menu.photo_found", format!("{}: stock photo added", item.name)).await?;
        Ok(item)
    }

    /// Worker: give dishes without a photo a matching stock photo, a few at a
    /// time (stock APIs have hourly quotas). Owners' photos and dishes whose
    /// stock photo the owner turned down are left alone.
    pub async fn fill_missing_menu_photos(&self) -> AppResult<usize> {
        if !self.p.dish_images.enabled() {
            return Ok(0);
        }
        let mut budget = media::STOCK_LOOKUPS_PER_SWEEP;
        let mut filled = 0;
        for mut r in self.p.restaurants.list().await? {
            if budget == 0 {
                break;
            }
            let wanted: Vec<DishQuery> = r
                .menu
                .iter()
                .flat_map(|c| c.items.iter())
                .filter(|i| media::wants_stock(i))
                .take(budget)
                .map(|i| dish_query(&r.detail.id, r.detail.cuisines.first().cloned(), i))
                .collect();
            if wanted.is_empty() {
                continue;
            }
            let mut changed = false;
            for q in wanted {
                budget -= 1;
                let found = match self.p.dish_images.find(&q).await {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::warn!(error = %e, dish = %q.dish, "stock photo lookup failed");
                        budget = 0;
                        break;
                    }
                };
                let Some(found) = found else { continue };
                if let Some(item) = r.find_item_mut(&q.item_id) {
                    // Re-check: the owner may have uploaded meanwhile (we hold an older copy only briefly).
                    if media::wants_stock(item) && media::set_stock_photo(item, found.url, found.credit) {
                        item.image_source = found.source;
                        changed = true;
                        filled += 1;
                    }
                }
            }
            if changed {
                // Merge onto the latest copy so a concurrent owner edit isn't lost.
                if let Some(mut latest) = self.p.restaurants.get(&r.detail.id).await? {
                    merge_found_photos(&mut latest, &r);
                    self.p.restaurants.update(&latest).await?;
                }
            }
        }
        Ok(filled)
    }

    /// Public: an uploaded photo (only keys under `media/`).
    pub async fn media_file(&self, path: &str) -> AppResult<MediaFile> {
        let key = media::media_key_from_path(path).ok_or_else(|| AppError::not_found("File"))?;
        let (bytes, content_type) = self.p.files.get(&key).await?;
        if !media::MENU_PHOTO_TYPES.contains(&content_type.as_str()) {
            return Err(AppError::not_found("File"));
        }
        Ok(MediaFile { content_type, bytes })
    }
}

fn dish_query(restaurant_id: &str, cuisine: Option<String>, item: &MenuItem) -> DishQuery {
    DishQuery { restaurant_id: restaurant_id.to_owned(), item_id: item.id.clone(), dish: item.name.clone(), description: item.description.clone(), cuisine }
}

/// Copy stock photos found on `found` onto `latest`, only where `latest`
/// still wants one.
fn merge_found_photos(latest: &mut Restaurant, found: &Restaurant) {
    for item in latest.menu.iter_mut().flat_map(|c| c.items.iter_mut()) {
        if !media::wants_stock(item) {
            continue;
        }
        if let Some(f) = found.menu.iter().flat_map(|c| c.items.iter()).find(|f| f.id == item.id && f.image.is_some()) {
            if f.image_source == ImageSource::Stock || f.image_source == ImageSource::Generated {
                item.image = f.image.clone();
                item.image_source = f.image_source;
                item.image_credit = f.image_credit.clone();
            }
        }
    }
}
