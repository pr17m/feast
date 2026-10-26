//! Contract types shared with every client (web, mobile).
//!
//! These mirror `webapp/src/lib/api/types.ts` field for field and serialize to the
//! same JSON (camelCase fields, SCREAMING_SNAKE enums, absent optionals omitted).
//! Money is always integer paise (`Paise`). Timestamps are RFC 3339 UTC strings
//! with millisecond precision; dates are `YYYY-MM-DD` in the city's timezone.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

pub use crate::media::ImageSource;
pub use crate::trust::{
    AdminCaseItem, AuditEntry, Case, CaseAttachment, CaseChange, CaseDecision, CaseKind, CaseMessage, CaseMessageRequest, CaseOutcome, CaseParty, CaseReason,
    CaseRef, CaseRole, CaseRuleError, CaseStatus, ContentKind, CreateReviewRequest, ModerateContentRequest, ModerateReviewRequest, ModeratedContent,
    Moderation, ModerationAction, ModerationAppealRequest, OpenCaseRequest, RatingBase, ReportReviewRequest, ResolveCaseRequest, Review, ReviewFlag,
    ReviewList, ReviewReply, ReviewReplyRequest, ReviewStatus, ReviewSummary, SupportAccess, SupportAccessRequest,
};

pub type Paise = i64;
/// `YYYY-MM-DD`
pub type IsoDate = String;
/// RFC 3339, e.g. `2026-10-24T12:00:00.000Z`
pub type IsoDateTime = String;

// ---------------------------------------------------------------- location

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct City {
    pub id: String,
    pub name: String,
    /// ISO 3166-1 alpha-2 of the city's market.
    pub country_code: String,
    /// IANA time zone for slots and "today".
    pub timezone: String,
    /// Overrides the market's food tax here (e.g. Canadian provinces).
    pub food_tax_percent: Option<f64>,
    pub areas: Vec<String>,
    pub lat: f64,
    pub lng: f64,
}

// ---------------------------------------------------------------- offers

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OfferScope {
    Prebook,
    Bill,
    Bank,
}

impl OfferScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prebook => "PREBOOK",
            Self::Bill => "BILL",
            Self::Bank => "BANK",
        }
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    #[serde(default)]
    pub id: String,
    pub scope: OfferScope,
    pub title: String,
    pub subtitle: Option<String>,
    pub percent_off: Option<i64>,
    pub flat_off: Option<Paise>,
    pub max_discount: Option<Paise>,
    pub min_bill: Option<Paise>,
    #[serde(default)]
    pub terms: Vec<String>,
    /// Hidden from diners by Feast (the owner sees why).
    pub moderation: Option<Moderation>,
}

// ---------------------------------------------------------------- restaurants

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CuisineMotif {
    Pizza,
    Noodles,
    Curry,
    Grill,
    Cafe,
    Dessert,
    Seafood,
    Bar,
}

impl CuisineMotif {
    pub const ALL: [CuisineMotif; 8] = [Self::Pizza, Self::Noodles, Self::Curry, Self::Grill, Self::Cafe, Self::Dessert, Self::Seafood, Self::Bar];
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Art {
    pub hue: i64,
    pub motif: CuisineMotif,
    /// Cover photo URL; the generated art (hue + motif) shows until it loads or when absent.
    #[serde(default)]
    pub image: Option<String>,
    /// Photographer credit for the cover, e.g. "Unsplash · @name".
    #[serde(default)]
    pub image_credit: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RestaurantStatus {
    /// Owner filling details and uploading documents.
    Onboarding,
    /// Submitted; admins reviewing documents.
    PendingApproval,
    Live,
    Paused,
    Suspended,
}

impl RestaurantStatus {
    pub const ALL: [RestaurantStatus; 5] = [Self::Onboarding, Self::PendingApproval, Self::Live, Self::Paused, Self::Suspended];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Onboarding => "ONBOARDING",
            Self::Live => "LIVE",
            Self::Paused => "PAUSED",
            Self::PendingApproval => "PENDING_APPROVAL",
            Self::Suspended => "SUSPENDED",
        }
    }
    /// Customers can see LIVE and PAUSED restaurants; the others are hidden.
    pub fn is_public(self) -> bool {
        matches!(self, Self::Live | Self::Paused)
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.as_str() == s)
    }
}

/// Owner-controlled switches. Admins can change them for any restaurant.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantSettings {
    pub bookings_enabled: bool,
    pub bill_payment_enabled: bool,
    pub tables_per_slot: i64,
    /// Most a Feast deal may take off one bill, whatever the offer says (None = no cap).
    #[serde(default)]
    pub max_discount_per_bill: Option<Paise>,
    /// Most discounted bills per local day (None = unlimited). Bills stay payable
    /// once it's reached, just without the Feast deal.
    #[serde(default)]
    pub daily_deal_limit: Option<i64>,
}

impl Default for RestaurantSettings {
    fn default() -> Self {
        Self { bookings_enabled: true, bill_payment_enabled: true, tables_per_slot: 6, max_discount_per_bill: None, daily_deal_limit: None }
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantSettingsPatch {
    pub bookings_enabled: Option<bool>,
    pub bill_payment_enabled: Option<bool>,
    pub tables_per_slot: Option<i64>,
    /// Absent = unchanged, null = remove the cap, a number = set it.
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub max_discount_per_bill: Option<Option<Paise>>,
    /// Absent = unchanged, null = unlimited, a number = set it.
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub daily_deal_limit: Option<Option<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpeningHours {
    pub day: String,
    pub open: String,
    pub close: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub id: String,
    pub caption: String,
    pub hue: i64,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub credit: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantSummary {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub city_id: String,
    pub area: String,
    pub cuisines: Vec<String>,
    pub rating: f64,
    pub rating_count: i64,
    pub cost_for_two: Paise,
    pub price_band: i64,
    /// ISO 4217 currency of every amount about this restaurant.
    pub currency: String,
    pub country_code: String,
    pub distance_km: f64,
    pub is_pure_veg: bool,
    pub tags: Vec<String>,
    pub art: Art,
    pub headline_offer: Option<String>,
    pub accepts_bookings: bool,
    pub accepts_bill_payment: bool,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantDetail {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub city_id: String,
    pub area: String,
    pub cuisines: Vec<String>,
    pub rating: f64,
    pub rating_count: i64,
    pub cost_for_two: Paise,
    pub price_band: i64,
    pub currency: String,
    pub country_code: String,
    pub distance_km: f64,
    pub is_pure_veg: bool,
    pub tags: Vec<String>,
    pub art: Art,
    pub headline_offer: Option<String>,
    #[serde(default)]
    pub accepts_bookings: bool,
    #[serde(default)]
    pub accepts_bill_payment: bool,
    pub status: RestaurantStatus,
    pub settings: RestaurantSettings,
    pub address: String,
    pub about: String,
    pub phone: String,
    pub hours: Vec<OpeningHours>,
    pub highlights: Vec<String>,
    pub offers: Vec<Offer>,
    pub photos: Vec<Photo>,
    #[serde(default)]
    pub menu_preview: Vec<MenuItem>,
}

impl RestaurantDetail {
    pub fn prebook_percent(&self) -> i64 {
        self.offers.iter().find(|o| o.scope == OfferScope::Prebook).and_then(|o| o.percent_off).unwrap_or(0)
    }

    pub fn headline(&self) -> Option<String> {
        let p = self.prebook_percent();
        (p > 0).then(|| format!("Flat {p}% off on pre-booking"))
    }

    pub fn is_open_for(&self, service: Service) -> bool {
        self.status == RestaurantStatus::Live
            && match service {
                Service::Bookings => self.settings.bookings_enabled,
                Service::BillPayment => self.settings.bill_payment_enabled,
            }
    }

    /// Recompute derived flags (accepts*, headline) from status/settings/offers.
    pub fn with_derived_flags(mut self) -> Self {
        self.accepts_bookings = self.is_open_for(Service::Bookings);
        self.accepts_bill_payment = self.is_open_for(Service::BillPayment);
        self.headline_offer = self.headline();
        self
    }

    pub fn summary(&self) -> RestaurantSummary {
        RestaurantSummary {
            id: self.id.clone(),
            slug: self.slug.clone(),
            name: self.name.clone(),
            city_id: self.city_id.clone(),
            area: self.area.clone(),
            cuisines: self.cuisines.clone(),
            rating: self.rating,
            rating_count: self.rating_count,
            cost_for_two: self.cost_for_two,
            price_band: self.price_band,
            currency: self.currency.clone(),
            country_code: self.country_code.clone(),
            distance_km: self.distance_km,
            is_pure_veg: self.is_pure_veg,
            tags: self.tags.clone(),
            art: self.art.clone(),
            headline_offer: self.headline(),
            accepts_bookings: self.is_open_for(Service::Bookings),
            accepts_bill_payment: self.is_open_for(Service::BillPayment),
        }
    }

    pub fn reference(&self) -> RestaurantRef {
        RestaurantRef { id: self.id.clone(), slug: self.slug.clone(), name: self.name.clone(), area: self.area.clone(), art: self.art.clone() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Bookings,
    BillPayment,
}

/// Small restaurant reference embedded in bookings and payments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantRef {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub area: String,
    pub art: Art,
}

/// The restaurant aggregate as stored: public detail + menu + staff-only fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Restaurant {
    pub detail: RestaurantDetail,
    pub menu: Vec<MenuCategory>,
    #[serde(default)]
    pub breakfast: bool,
    /// The code in the restaurant's printed QR (`/r/{short_code}`), e.g. `oae1`.
    pub short_code: String,
    /// Feast staff may edit this restaurant's content until then (granted by the owner).
    #[serde(default)]
    pub support_access_until: Option<IsoDateTime>,
    /// Rating before Feast reviews; reviews blend into it.
    #[serde(default)]
    pub rating_base: Option<RatingBase>,
}

impl Restaurant {
    pub fn find_item(&self, item_id: &str) -> Option<&MenuItem> {
        self.menu.iter().flat_map(|c| c.items.iter()).find(|i| i.id == item_id)
    }

    pub fn find_item_mut(&mut self, item_id: &str) -> Option<&mut MenuItem> {
        self.menu.iter_mut().flat_map(|c| c.items.iter_mut()).find(|i| i.id == item_id)
    }

    /// What diners see: no content Feast has hidden, no staff-only fields.
    pub fn customer_view(mut self) -> Self {
        for c in &mut self.menu {
            c.items.retain(|i| i.moderation.is_none());
        }
        self.menu.retain(|c| !c.items.is_empty());
        self.detail.offers.retain(|o| o.moderation.is_none());
        self.detail.menu_preview.retain(|i| i.moderation.is_none());
        self.detail.headline_offer = self.detail.headline();
        self.support_access_until = None;
        self.rating_base = None;
        self
    }
}

// ---------------------------------------------------------------- discovery

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cta {
    pub label: String,
    pub href: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub count: usize,
    pub hue: i64,
    pub query: SearchQuery,
}

/// Server-driven home sections. Clients skip types they don't know.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE", rename_all_fields = "camelCase")]
pub enum HomeSection {
    HeroBanner { id: String, title: String, subtitle: String, cta: Cta, hue: i64 },
    Collections { id: String, title: String, items: Vec<Collection> },
    RestaurantCarousel { id: String, title: String, subtitle: Option<String>, restaurants: Vec<RestaurantSummary> },
    RestaurantList { id: String, title: String, restaurants: Vec<RestaurantSummary> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HomeResponse {
    pub city: City,
    pub sections: Vec<HomeSection>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SearchSort {
    #[default]
    Relevance,
    Rating,
    Distance,
    CostLow,
    CostHigh,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub q: Option<String>,
    pub cuisine: Option<String>,
    pub tag: Option<String>,
    pub area: Option<String>,
    pub price_band: Option<i64>,
    pub offer: Option<OfferScope>,
    pub service: Option<SearchService>,
    pub pure_veg: Option<bool>,
    pub min_rating: Option<f64>,
    pub sort: Option<SearchSort>,
}

/// What a diner can do there on Feast (driven by the owner's service toggles).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SearchService {
    Bookings,
    BillPayment,
}

impl SearchService {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bookings => "BOOKINGS",
            Self::BillPayment => "BILL_PAYMENT",
        }
    }
}

/// One filter option, built from what restaurants actually list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Facet {
    pub value: String,
    pub label: String,
    /// Restaurants matching this option given the *other* active filters.
    pub count: usize,
}

/// Filters derived from live listings (see `listing::search_listings`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchFacets {
    pub cuisines: Vec<Facet>,
    pub tags: Vec<Facet>,
    pub areas: Vec<Facet>,
    pub price_bands: Vec<Facet>,
    pub offers: Vec<Facet>,
    pub services: Vec<Facet>,
    pub pure_veg: usize,
}

/// What an owner can change about how their restaurant is listed and found.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListingPatch {
    pub cuisines: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub is_pure_veg: Option<bool>,
    pub cost_for_two: Option<Paise>,
    pub about: Option<String>,
    pub highlights: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub total: usize,
    pub restaurants: Vec<RestaurantSummary>,
    pub facets: SearchFacets,
}

// ---------------------------------------------------------------- menu

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PricedOption {
    pub id: String,
    pub name: String,
    /// Added on top of the item's base price.
    pub price: Paise,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MenuItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub price: Paise,
    pub is_veg: bool,
    pub is_bestseller: Option<bool>,
    pub spice_level: Option<i64>,
    pub variants: Option<Vec<PricedOption>>,
    pub addons: Option<Vec<PricedOption>>,
    pub available: bool,
    /// Dish photo URL: an uploaded photo (`/v1/media/…`, relative to the API) or a stock photo URL.
    #[serde(default)]
    pub image: Option<String>,
    /// Who supplied the photo (see `media::source_of` for legacy items).
    #[serde(default, skip_serializing_if = "ImageSource::is_none")]
    pub image_source: ImageSource,
    /// Photographer credit for stock photos, e.g. "Unsplash · @name".
    #[serde(default)]
    pub image_credit: Option<String>,
    /// The owner removed a stock photo: don't add another automatically.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stock_declined: bool,
    /// Hidden from diners by Feast (the owner sees why).
    pub moderation: Option<Moderation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MenuCategory {
    pub id: String,
    pub name: String,
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Menu {
    pub restaurant_id: String,
    pub categories: Vec<MenuCategory>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MenuItemPatch {
    pub available: Option<bool>,
    pub price: Option<Paise>,
}

// ---------------------------------------------------------------- booking

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MealPeriod {
    Breakfast,
    Lunch,
    Dinner,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub id: String,
    pub time: String,
    pub period: MealPeriod,
    pub available: bool,
    pub seats_left: i64,
    pub deal: Option<Offer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlotAvailability {
    pub restaurant_id: String,
    pub date: IsoDate,
    pub guests: i64,
    pub slots: Vec<Slot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BookingStatus {
    PendingPayment,
    Confirmed,
    Seated,
    Cancelled,
    Completed,
    NoShow,
}

impl BookingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingPayment => "PENDING_PAYMENT",
            Self::Confirmed => "CONFIRMED",
            Self::Seated => "SEATED",
            Self::Cancelled => "CANCELLED",
            Self::Completed => "COMPLETED",
            Self::NoShow => "NO_SHOW",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "PENDING_PAYMENT" => Self::PendingPayment,
            "CONFIRMED" => Self::Confirmed,
            "SEATED" => Self::Seated,
            "CANCELLED" => Self::Cancelled,
            "COMPLETED" => Self::Completed,
            "NO_SHOW" => Self::NoShow,
            _ => return None,
        })
    }
    /// Statuses that hold a table in a slot.
    pub fn holds_table(self) -> bool {
        matches!(self, Self::Confirmed | Self::Seated | Self::PendingPayment)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub name: String,
    pub phone: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateBookingRequest {
    pub restaurant_id: String,
    pub date: IsoDate,
    pub slot_id: String,
    pub guests: i64,
    pub deal_id: Option<String>,
    pub special_request: Option<String>,
    pub contact: Contact,
    pub idempotency_key: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Booking {
    pub id: String,
    pub user_id: String,
    pub code: String,
    pub restaurant: RestaurantRef,
    pub date: IsoDate,
    pub time: String,
    pub guests: i64,
    pub deal: Option<Offer>,
    pub special_request: Option<String>,
    pub contact: Contact,
    pub status: BookingStatus,
    pub cover_charge: Paise,
    pub currency: String,
    pub payment: Option<PaymentRef>,
    pub created_at: IsoDateTime,
    /// When the diner tapped "I'm here" at the restaurant.
    #[serde(default)]
    pub checked_in_at: Option<IsoDateTime>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateBookingResponse {
    pub booking: Booking,
    pub payment_intent: Option<PaymentIntent>,
}

// ---------------------------------------------------------------- restaurant QR

/// What the restaurant's printed QR code resolves to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantQr {
    pub code: String,
    pub restaurant: RestaurantSummary,
}

// ---------------------------------------------------------------- bills & payments

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BillLineKind {
    ItemTotal,
    Tax,
    /// Informational: tax already included in prices (not added to totals).
    TaxIncluded,
    Discount,
    Fee,
    Tip,
    Adjustment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BillLine {
    pub label: String,
    pub amount: Paise,
    pub kind: BillLineKind,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bill {
    pub id: String,
    pub restaurant_id: String,
    pub currency: String,
    /// What the diner entered from the restaurant's bill.
    pub bill_amount: Paise,
    pub lines: Vec<BillLine>,
    pub applied_offer: Option<Offer>,
    pub savings: Paise,
    pub payable: Paise,
    /// Why the Feast deal is smaller than the offer, or missing (per-bill cap,
    /// today's deals used up, no Feast booking). Shown to the diner.
    #[serde(default)]
    pub deal_notice: Option<String>,
    /// The diner's Feast booking this bill is paid against (deals need one).
    #[serde(default)]
    pub booking_id: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuoteBillRequest {
    pub restaurant_id: String,
    #[serde(default)]
    pub bill_amount: Paise,
    pub tip: Option<Paise>,
    pub offer_id: Option<String>,
    pub booking_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentMethod {
    Upi,
    Card,
    Netbanking,
    Wallet,
    ApplePay,
    GooglePay,
    /// Paid in cash at the restaurant's counter (offline; never settled by Feast).
    Cash,
    /// Paid by card on the restaurant's own terminal (offline; never settled by Feast).
    CardAtCounter,
}

impl PaymentMethod {
    /// Methods a payment provider can take online (offered at checkout).
    pub const ALL: [PaymentMethod; 6] = [Self::Upi, Self::Card, Self::Netbanking, Self::Wallet, Self::ApplePay, Self::GooglePay];
    /// Paid at the restaurant and confirmed by the owner.
    pub const OFFLINE: [PaymentMethod; 2] = [Self::Cash, Self::CardAtCounter];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upi => "UPI",
            Self::Card => "CARD",
            Self::Netbanking => "NETBANKING",
            Self::Wallet => "WALLET",
            Self::ApplePay => "APPLE_PAY",
            Self::GooglePay => "GOOGLE_PAY",
            Self::Cash => "CASH",
            Self::CardAtCounter => "CARD_AT_COUNTER",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().chain(Self::OFFLINE).find(|m| m.as_str().eq_ignore_ascii_case(s))
    }
    pub fn is_offline(self) -> bool {
        Self::OFFLINE.contains(&self)
    }
}

/// Who handled the money for a payment.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Settlement {
    /// Taken by Feast's payment provider and settled to the restaurant.
    #[default]
    Online,
    /// Paid at the restaurant (cash or their own terminal); the owner confirmed it. Feast never held the money.
    Offline,
}

/// A diner paying a Feast bill at the counter, waiting for (or after) the
/// owner's confirmation. See `feast_domain::offline` for the rules.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OfflinePayment {
    pub id: String,
    pub bill_id: String,
    /// The internal payment record (provider "offline") the approval completes.
    pub intent_id: String,
    pub restaurant: RestaurantRef,
    pub user_id: String,
    /// Diner's display name, for the owner's approval list.
    pub diner_name: String,
    pub method: PaymentMethod,
    /// What the diner owes (the bill's payable, after any Feast deal).
    pub amount: Paise,
    pub currency: String,
    /// Feast deal included in `amount` (0 = none).
    pub savings: Paise,
    pub booking_id: Option<String>,
    pub status: OfflinePaymentStatus,
    pub submitted_at: IsoDateTime,
    /// Auto-expires (diner can pay again) if nobody decides by then.
    pub expires_at: IsoDateTime,
    pub decided_at: Option<IsoDateTime>,
    pub decided_by: Option<String>,
    /// Owner's reason (required to reject) or the system's (expired, paid online).
    pub reason: Option<String>,
    pub amount_received: Option<Paise>,
    /// The bill payment created on approval.
    pub bill_payment_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OfflinePaymentStatus {
    PendingApproval,
    Approved,
    Rejected,
    Expired,
}

impl OfflinePaymentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingApproval => "PENDING_APPROVAL",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Expired => "EXPIRED",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::PendingApproval, Self::Approved, Self::Rejected, Self::Expired].into_iter().find(|v| v.as_str().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentPurpose {
    Booking,
    Bill,
    /// A restaurant paying Feast for its listing (reference = restaurant id).
    Subscription,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentStatus {
    Created,
    Succeeded,
    Failed,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentIntent {
    pub id: String,
    pub purpose: PaymentPurpose,
    pub reference_id: String,
    pub amount: Paise,
    pub currency: String,
    /// Payment provider handling it (from the restaurant's market).
    pub provider: String,
    /// What the provider's client SDK needs (Razorpay order id + key, Stripe client secret…).
    pub checkout: Option<BTreeMap<String, String>>,
    /// Provider-side order/intent id.
    pub provider_ref: Option<String>,
    /// Plan being bought (purpose SUBSCRIPTION).
    pub plan_id: Option<PlanId>,
    /// Who started the payment (completes it on their behalf when the
    /// provider's webhook arrives first).
    #[serde(default)]
    pub user_id: Option<String>,
    pub status: PaymentStatus,
    pub result_ref: Option<String>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRef {
    pub intent_id: String,
    pub method: PaymentMethod,
    pub amount: Paise,
    pub currency: String,
    pub provider: String,
    pub paid_at: IsoDateTime,
    /// Online (through Feast) or offline (at the counter, confirmed by the owner).
    #[serde(default)]
    pub settlement: Settlement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BillPayment {
    pub id: String,
    pub user_id: String,
    pub bill: Bill,
    pub restaurant: RestaurantRef,
    pub payment: PaymentRef,
}

// ---------------------------------------------------------------- users & auth

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    Customer,
    Owner,
    Admin,
}

impl Role {
    pub const ALL: [Role; 3] = [Self::Customer, Self::Owner, Self::Admin];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Customer => "CUSTOMER",
            Self::Owner => "OWNER",
            Self::Admin => "ADMIN",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "CUSTOMER" => Self::Customer,
            "OWNER" => Self::Owner,
            "ADMIN" => Self::Admin,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserStatus {
    Active,
    Blocked,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub role: Role,
    #[serde(default)]
    pub restaurant_ids: Vec<String>,
    pub status: UserStatus,
    pub created_at: IsoDateTime,
}

impl User {
    pub fn is_active(&self) -> bool {
        self.status == UserStatus::Active
    }
    /// OWNER of this restaurant, or ADMIN (may view it in the partner portal).
    pub fn can_manage(&self, restaurant_id: &str) -> bool {
        self.role == Role::Admin || self.owns(restaurant_id)
    }
    /// OWNER of this restaurant.
    pub fn owns(&self, restaurant_id: &str) -> bool {
        self.role == Role::Owner && self.restaurant_ids.iter().any(|r| r == restaurant_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub token: String,
    pub user: User,
}

/// Social sign-in providers (OAuth 2.0 / OpenID Connect).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthProvider {
    Google,
    Apple,
    Facebook,
}

impl AuthProvider {
    pub const ALL: [AuthProvider; 3] = [Self::Google, Self::Apple, Self::Facebook];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "GOOGLE",
            Self::Apple => "APPLE",
            Self::Facebook => "FACEBOOK",
        }
    }
    /// Lower-case name used in URLs and config (`google`, `apple`, `facebook`).
    pub fn slug(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Apple => "apple",
            Self::Facebook => "facebook",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str().eq_ignore_ascii_case(s))
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Apple => "Apple",
            Self::Facebook => "Facebook",
        }
    }
}

/// What a client sends after the provider's own sign-in UI succeeds.
/// Google and Apple return an OpenID Connect `idToken`; Facebook Login
/// returns an `accessToken`. Apple only reveals the user's name to the client,
/// once, so the client forwards it in `name`.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OAuthLoginRequest {
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub name: Option<String>,
    /// The nonce the client put in the auth request (replay protection), if any.
    pub nonce: Option<String>,
}

/// A provider the client may offer, with the public config its SDK needs.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthProviderInfo {
    pub provider: AuthProvider,
    /// OAuth client id / Facebook app id for the web SDK (None in demo mode).
    pub client_id: Option<String>,
    /// Apple JS redirect URI (Apple only).
    pub redirect_uri: Option<String>,
    /// Demo mode: no real provider; the client shows a fake account picker.
    pub demo: bool,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserPatch {
    pub role: Option<Role>,
    pub restaurant_ids: Option<Vec<String>>,
    pub status: Option<UserStatus>,
    pub name: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserRequest {
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role: Role,
    pub restaurant_ids: Option<Vec<String>>,
}

// ---------------------------------------------------------------- partner

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub date: IsoDate,
    pub bookings: usize,
    pub revenue: Paise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartnerDashboard {
    pub restaurant: RestaurantDetail,
    pub date: IsoDate,
    pub bookings_today: usize,
    pub covers_today: i64,
    pub seated_now: usize,
    /// Diners who tapped "I'm here" today.
    pub arrived_today: usize,
    /// Counter payments waiting for the restaurant to confirm.
    pub pending_counter_payments: usize,
    pub revenue_today: Paise,
    pub savings_given_today: Paise,
    pub last7_days: Vec<DayStat>,
}

// ---------------------------------------------------------------- admin

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BookingCounts {
    pub total: usize,
    pub upcoming: usize,
    pub cancelled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentTotals {
    pub count: usize,
    /// Never summed across currencies.
    pub by_currency: Vec<CurrencyTotals>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyTotals {
    pub currency: String,
    pub gmv: Paise,
    pub savings: Paise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CityMetrics {
    pub city_id: String,
    pub city_name: String,
    pub currency: String,
    pub restaurants: usize,
    pub bookings: usize,
    pub gmv: Paise,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminMetrics {
    pub restaurants: BTreeMap<String, usize>,
    pub users: BTreeMap<String, usize>,
    pub bookings: BookingCounts,
    pub payments: PaymentTotals,
    pub subscriptions: SubscriptionMetrics,
    pub by_city: Vec<CityMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionMetrics {
    pub trial: usize,
    pub active: usize,
    pub past_due: usize,
    pub expired: usize,
    pub trials_ending_soon: usize,
    pub revenue: Vec<SubscriptionRevenue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRevenue {
    pub currency: String,
    pub this_month: Paise,
    pub total: Paise,
    pub mrr: Paise,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateRestaurantRequest {
    pub name: String,
    pub city_id: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub cuisines: Vec<String>,
    pub cost_for_two: Paise,
    #[serde(default)]
    pub is_pure_veg: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub about: String,
    pub owner_user_id: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestaurantPatch {
    pub name: Option<String>,
    pub area: Option<String>,
    pub cuisines: Option<Vec<String>>,
    /// The price band is derived from this with the market's bands.
    pub cost_for_two: Option<Paise>,
    pub is_pure_veg: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub about: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
}

// ---------------------------------------------------------------- notifications

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationKind {
    BookingCreated,
    BookingCancelled,
    BookingUpdated,
    PaymentReceived,
    SubscriptionPaid,
    SubscriptionExpiring,
    ApplicationSubmitted,
    DocumentReviewed,
    RestaurantApproved,
    DocumentExpiring,
    CaseUpdated,
    ReviewUpdated,
    ContentModerated,
}

impl NotificationKind {
    pub const ALL: [NotificationKind; 13] = [
        Self::BookingCreated,
        Self::BookingCancelled,
        Self::BookingUpdated,
        Self::PaymentReceived,
        Self::SubscriptionPaid,
        Self::SubscriptionExpiring,
        Self::ApplicationSubmitted,
        Self::DocumentReviewed,
        Self::RestaurantApproved,
        Self::DocumentExpiring,
        Self::CaseUpdated,
        Self::ReviewUpdated,
        Self::ContentModerated,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BookingCreated => "BOOKING_CREATED",
            Self::BookingCancelled => "BOOKING_CANCELLED",
            Self::BookingUpdated => "BOOKING_UPDATED",
            Self::PaymentReceived => "PAYMENT_RECEIVED",
            Self::SubscriptionPaid => "SUBSCRIPTION_PAID",
            Self::SubscriptionExpiring => "SUBSCRIPTION_EXPIRING",
            Self::ApplicationSubmitted => "APPLICATION_SUBMITTED",
            Self::DocumentReviewed => "DOCUMENT_REVIEWED",
            Self::RestaurantApproved => "RESTAURANT_APPROVED",
            Self::DocumentExpiring => "DOCUMENT_EXPIRING",
            Self::CaseUpdated => "CASE_UPDATED",
            Self::ReviewUpdated => "REVIEW_UPDATED",
            Self::ContentModerated => "CONTENT_MODERATED",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// An in-app notification for one user, created from a domain event.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    pub restaurant_id: Option<String>,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    /// App route to open, e.g. `/partner/bookings`.
    pub link: Option<String>,
    /// The domain event this came from (idempotency key together with user_id).
    pub event_id: String,
    pub created_at: IsoDateTime,
    pub read_at: Option<IsoDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationList {
    pub items: Vec<Notification>,
    pub unread: i64,
}

// ---------------------------------------------------------------- markets (countries)

/// A country Feast operates in. Money, tax, phone and onboarding rules all come
/// from here; launching a country is admin data entry, not a code change.
/// Amounts are integers in the currency's minor unit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub country_code: String,
    pub name: String,
    pub currency: String,
    pub currency_decimals: u32,
    pub locale: String,
    pub status: MarketStatus,
    pub tax_name: String,
    pub food_tax_percent: f64,
    pub prices_include_tax: bool,
    pub platform_tax_percent: f64,
    pub price_bands: [Paise; 3],
    pub cover_charge_per_pair: Paise,
    pub tip_presets: Vec<Paise>,
    pub plan_prices: PlanPrices,
    pub payments: MarketPayments,
    pub phone: PhoneRules,
    pub documents: Vec<DocumentRequirement>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketStatus {
    Live,
    ComingSoon,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct PlanPrices {
    pub monthly: Paise,
    pub quarterly: Paise,
    pub yearly: Paise,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarketPayments {
    pub provider: String,
    pub methods: Vec<PaymentMethod>,
    /// Public client key for the provider SDK (filled by the server, never stored).
    #[serde(default)]
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhoneRules {
    pub dial_code: String,
    /// Regex for the national number (digits only).
    pub pattern: String,
    pub example: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRequirement {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub issuer: Option<String>,
    pub required: bool,
    pub number_label: Option<String>,
    #[serde(default)]
    pub has_expiry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentProviderInfo {
    pub id: String,
    pub name: String,
    pub configured: bool,
    pub methods: Vec<PaymentMethod>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateCityRequest {
    pub name: String,
    pub country_code: String,
    pub timezone: String,
    pub food_tax_percent: Option<f64>,
    #[serde(default)]
    pub areas: Vec<String>,
    #[serde(default)]
    pub lat: f64,
    #[serde(default)]
    pub lng: f64,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeoResult {
    pub country_code: Option<String>,
    pub supported: bool,
    pub city_id: Option<String>,
    /// header | geoip | default
    pub source: String,
    /// When another deployment serves that country: its URL (from config/directory.toml).
    pub redirect_url: Option<String>,
}

// ---------------------------------------------------------------- subscriptions (platform billing)

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanId {
    Monthly,
    Quarterly,
    Yearly,
}

impl PlanId {
    pub const ALL: [PlanId; 3] = [Self::Monthly, Self::Quarterly, Self::Yearly];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monthly => "MONTHLY",
            Self::Quarterly => "QUARTERLY",
            Self::Yearly => "YEARLY",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }
    pub fn months(self) -> u32 {
        match self {
            Self::Monthly => 1,
            Self::Quarterly => 3,
            Self::Yearly => 12,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Monthly => "Monthly",
            Self::Quarterly => "Quarterly",
            Self::Yearly => "Yearly",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubscriptionStatus {
    NotStarted,
    Trial,
    Active,
    PastDue,
    Expired,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionPlan {
    pub id: PlanId,
    pub name: String,
    pub months: u32,
    pub currency: String,
    pub price: Paise,
    pub tax_name: String,
    pub tax_percent: f64,
    pub total: Paise,
    pub save_percent: Option<i64>,
}

/// What's stored per restaurant; everything else is derived (`subscription::view`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRecord {
    pub trial_ends_at: Option<IsoDateTime>,
    pub paid_through: Option<IsoDateTime>,
    pub plan_id: Option<PlanId>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub restaurant_id: String,
    pub status: SubscriptionStatus,
    pub plan_id: Option<PlanId>,
    pub trial_ends_at: Option<IsoDateTime>,
    pub paid_through: Option<IsoDateTime>,
    pub ends_at: Option<IsoDateTime>,
    pub hidden_after: Option<IsoDateTime>,
    pub days_left: Option<i64>,
    pub listed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlatformPaymentStatus {
    Paid,
    Failed,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPayment {
    pub id: String,
    pub invoice_number: String,
    pub restaurant: RestaurantRef,
    /// "MONTHLY" | "QUARTERLY" | "YEARLY" | "COMP" (admin-granted free days).
    pub plan_id: String,
    pub period_start: IsoDateTime,
    pub period_end: IsoDateTime,
    pub currency: String,
    pub amount: Paise,
    pub tax: Paise,
    pub tax_name: String,
    pub total: Paise,
    pub method: Option<PaymentMethod>,
    pub provider: Option<String>,
    pub status: PlatformPaymentStatus,
    pub failure_reason: Option<String>,
    pub note: Option<String>,
    pub paid_by: String,
    pub created_at: IsoDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionRules {
    pub trial_days: i64,
    pub grace_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionOverview {
    pub subscription: Subscription,
    pub plans: Vec<SubscriptionPlan>,
    pub payments: Vec<PlatformPayment>,
    pub rules: SubscriptionRules,
    pub methods: Vec<PaymentMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaySubscriptionResult {
    pub subscription: Subscription,
    pub payment: PlatformPayment,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminRestaurantRef {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub area: String,
    pub city_id: String,
    pub status: RestaurantStatus,
    pub currency: String,
    pub country_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRef {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdminSubscriptionRow {
    pub restaurant: AdminRestaurantRef,
    pub owners: Vec<OwnerRef>,
    pub subscription: Subscription,
    pub total_paid: Paise,
    pub last_payment_at: Option<IsoDateTime>,
}

// ---------------------------------------------------------------- onboarding & compliance

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DocumentStatus {
    Submitted,
    Approved,
    Rejected,
}

impl DocumentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "SUBMITTED",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Submitted, Self::Approved, Self::Rejected].into_iter().find(|x| x.as_str() == s)
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceDocument {
    pub id: String,
    pub restaurant_id: String,
    pub requirement_id: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size: i64,
    /// Storage key (internal; files are served through the API).
    #[serde(skip_serializing, default)]
    pub storage_key: String,
    pub number: Option<String>,
    pub expires_on: Option<IsoDate>,
    pub status: DocumentStatus,
    pub review_note: Option<String>,
    pub uploaded_at: IsoDateTime,
    pub uploaded_by: String,
    pub reviewed_at: Option<IsoDateTime>,
    pub reviewed_by: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RequirementState {
    Missing,
    Submitted,
    Approved,
    Rejected,
    Expired,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceItem {
    pub requirement: DocumentRequirement,
    pub document: Option<ComplianceDocument>,
    pub state: RequirementState,
    pub expiring_soon: Option<bool>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceStatus {
    pub restaurant_id: String,
    pub restaurant_status: RestaurantStatus,
    pub country_code: String,
    pub items: Vec<ComplianceItem>,
    pub ready_to_submit: bool,
    pub complete: bool,
    pub review_notes: Option<String>,
    pub submitted_at: Option<IsoDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalQueueItem {
    pub restaurant: AdminRestaurantRef,
    pub owners: Vec<OwnerRef>,
    pub compliance: ComplianceStatus,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartnerSignupOwner {
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub password: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartnerSignupRestaurant {
    pub name: String,
    pub city_id: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub cuisines: Vec<String>,
    pub cost_for_two: Paise,
    #[serde(default)]
    pub is_pure_veg: bool,
    pub about: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartnerSignupRequest {
    pub owner: PartnerSignupOwner,
    pub restaurant: PartnerSignupRestaurant,
}
