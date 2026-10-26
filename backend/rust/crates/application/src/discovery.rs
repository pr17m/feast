//! Discovery: cities, server-driven home, search, restaurant pages, menus.

use feast_domain::model::*;

use crate::context::Feast;
use crate::error::{AppError, AppResult};

struct CollectionDef {
    id: &'static str,
    title: &'static str,
    tag: &'static str,
    hue: i64,
}

const COLLECTIONS: [CollectionDef; 8] = [
    CollectionDef { id: "date", title: "Date night", tag: "Date night", hue: 340 },
    CollectionDef { id: "rooftop", title: "Rooftops & views", tag: "Rooftop", hue: 205 },
    CollectionDef { id: "music", title: "Live music", tag: "Live music", hue: 270 },
    CollectionDef { id: "fine", title: "Fine dining", tag: "Fine dining", hue: 25 },
    CollectionDef { id: "groups", title: "Large groups", tag: "Large groups", hue: 160 },
    CollectionDef { id: "veg", title: "Pure veg", tag: "Pure veg", hue: 110 },
    CollectionDef { id: "breakfast", title: "Breakfast spots", tag: "All-day breakfast", hue: 45 },
    CollectionDef { id: "quick", title: "Quick bites", tag: "Quick bites", hue: 10 },
];

impl Feast {
    /// Cities of live markets (admins see every city).
    pub async fn list_cities(&self, user: Option<&User>) -> AppResult<Vec<City>> {
        let cities = self.p.markets.list_cities().await?;
        if user.is_some_and(|u| u.role == Role::Admin) {
            return Ok(cities);
        }
        let live: Vec<String> = self.live_markets().await?.into_iter().map(|m| m.country_code).collect();
        Ok(cities.into_iter().filter(|c| live.contains(&c.country_code)).collect())
    }

    pub(crate) async fn live_markets(&self) -> AppResult<Vec<Market>> {
        Ok(self.p.markets.list_markets().await?.into_iter().filter(|m| m.status == MarketStatus::Live).collect())
    }

    /// Markets with the client-side payment key filled in. Public: live only; admins: all.
    pub async fn list_markets(&self, user: Option<&User>) -> AppResult<Vec<Market>> {
        let admin = user.is_some_and(|u| u.role == Role::Admin);
        let mut list = if admin { self.p.markets.list_markets().await? } else { self.live_markets().await? };
        for m in &mut list {
            m.payments.public_key = self.p.payments.get(&m.payments.provider).and_then(|g| g.public_key());
        }
        Ok(list)
    }

    /// Where is the visitor? Country from the CDN header / GeoIP database; if
    /// another deployment serves it, where to send them.
    pub async fn locate(&self, req: &crate::ports::GeoRequest) -> AppResult<GeoResult> {
        let found = self.p.geo.locate(req);
        let live = self.live_markets().await?;
        let cities = self.p.markets.list_cities().await?;
        let default_city = |cc: &str| cities.iter().find(|c| c.country_code == cc).map(|c| c.id.clone());
        Ok(match found {
            Some((cc, source)) if live.iter().any(|m| m.country_code == cc) => {
                GeoResult { city_id: default_city(&cc), country_code: Some(cc), supported: true, source: source.into(), redirect_url: None }
            }
            Some((cc, source)) => GeoResult {
                redirect_url: self.settings.directory.get(&cc).cloned(),
                country_code: Some(cc),
                supported: false,
                city_id: None,
                source: source.into(),
            },
            None => GeoResult {
                country_code: Some(self.settings.country.clone()),
                supported: true,
                city_id: default_city(&self.settings.country),
                source: "default".into(),
                redirect_url: None,
            },
        })
    }

    async fn public_in_city(&self, city_id: &str) -> AppResult<Vec<Restaurant>> {
        let all = self.p.restaurants.list_by_city(city_id).await?;
        let listed = self.listed_ids(&all).await?;
        Ok(all.into_iter().filter(|r| listed.contains(&r.detail.id)).map(Restaurant::customer_view).collect())
    }

    pub async fn home(&self, city_id: &str) -> AppResult<HomeResponse> {
        let city = match self.p.markets.get_city(city_id).await? {
            Some(c) => c,
            None => self.list_cities(None).await?.into_iter().next().ok_or_else(|| AppError::not_found("City"))?,
        };
        let in_city: Vec<RestaurantDetail> = self.public_in_city(&city.id).await?.into_iter().map(|r| r.detail).collect();
        let live: Vec<&RestaurantDetail> = in_city.iter().filter(|d| d.status == RestaurantStatus::Live).collect();

        let mut top = live.clone();
        top.sort_by(|a, b| b.rating.total_cmp(&a.rating));
        let mut deals: Vec<&RestaurantDetail> = live.iter().copied().filter(|d| d.prebook_percent() > 0).collect();
        deals.sort_by_key(|d| std::cmp::Reverse(d.prebook_percent()));
        let mut all: Vec<&RestaurantDetail> = in_city.iter().collect();
        all.sort_by(|a, b| a.distance_km.total_cmp(&b.distance_km));

        let collections = COLLECTIONS
            .iter()
            .map(|c| Collection {
                id: c.id.into(),
                title: c.title.into(),
                hue: c.hue,
                count: in_city.iter().filter(|d| d.tags.iter().any(|t| t == c.tag) || (c.tag == "Pure veg" && d.is_pure_veg)).count(),
                query: if c.tag == "Pure veg" {
                    SearchQuery { pure_veg: Some(true), ..Default::default() }
                } else {
                    SearchQuery { tag: Some(c.tag.into()), ..Default::default() }
                },
            })
            .filter(|c| c.count > 0)
            .collect();

        let mut sections = vec![
            HomeSection::HeroBanner {
                id: "hero_prebook".into(),
                title: "Pre-book a table, save up to 40%".into(),
                subtitle: "Off-peak slots carry the biggest deals. No booking fee at most places.".into(),
                cta: Cta { label: "Explore deals".into(), href: "/search?sort=relevance".into() },
                hue: 172,
            },
            HomeSection::Collections { id: "collections".into(), title: "Find your vibe".into(), items: collections },
        ];
        if !top.is_empty() {
            sections.push(HomeSection::RestaurantCarousel {
                id: "top_rated".into(),
                title: "Top rated near you".into(),
                subtitle: Some("Loved by diners this month".into()),
                restaurants: top.iter().take(6).map(|d| d.summary()).collect(),
            });
        }
        if !deals.is_empty() {
            sections.push(HomeSection::RestaurantCarousel {
                id: "best_deals".into(),
                title: "Biggest pre-booking deals".into(),
                subtitle: None,
                restaurants: deals.iter().take(6).map(|d| d.summary()).collect(),
            });
        }
        sections.push(HomeSection::RestaurantList {
            id: "all".into(),
            title: format!("All restaurants in {}", city.name),
            restaurants: all.iter().map(|d| d.summary()).collect(),
        });
        Ok(HomeResponse { city, sections })
    }

    pub async fn search(&self, city_id: &str, q: &SearchQuery) -> AppResult<SearchResponse> {
        let city = self.p.markets.get_city(city_id).await?.ok_or_else(|| AppError::not_found("City"))?;
        let market = self.market(&city.country_code).await?;
        let in_city = self.public_in_city(city_id).await?;
        let menu_text: std::collections::HashMap<&str, String> = in_city
            .iter()
            .map(|r| (r.detail.id.as_str(), r.menu.iter().flat_map(|c| c.items.iter().map(|i| i.name.as_str())).collect::<Vec<_>>().join(" ")))
            .collect();
        let details: Vec<&RestaurantDetail> = in_city.iter().map(|r| &r.detail).collect();
        let (mut list, facets) = feast_domain::listing::search_listings(&details, q, &market, |d| {
            let mut hay = vec![d.name.as_str(), d.area.as_str()];
            hay.extend(d.cuisines.iter().map(String::as_str));
            hay.extend(d.tags.iter().map(String::as_str));
            hay.push(menu_text.get(d.id.as_str()).map(String::as_str).unwrap_or_default());
            hay.join(" ").to_lowercase()
        });
        match q.sort.unwrap_or_default() {
            SearchSort::Relevance => list.sort_by(|a, b| b.prebook_percent().cmp(&a.prebook_percent()).then(b.rating.total_cmp(&a.rating))),
            SearchSort::Rating => list.sort_by(|a, b| b.rating.total_cmp(&a.rating)),
            SearchSort::Distance => list.sort_by(|a, b| a.distance_km.total_cmp(&b.distance_km)),
            SearchSort::CostLow => list.sort_by_key(|d| d.cost_for_two),
            SearchSort::CostHigh => list.sort_by_key(|d| std::cmp::Reverse(d.cost_for_two)),
        }
        Ok(SearchResponse { total: list.len(), restaurants: list.iter().map(|d| d.summary()).collect(), facets })
    }

    pub async fn restaurant(&self, slug: &str) -> AppResult<RestaurantDetail> {
        let r = self.public_restaurant(slug).await?;
        let mut d = r.detail.clone().with_derived_flags();
        d.menu_preview = r.menu.iter().flat_map(|c| c.items.iter()).filter(|i| i.is_bestseller == Some(true) && i.available).take(4).cloned().collect();
        Ok(d)
    }

    pub async fn menu(&self, restaurant_id: &str) -> AppResult<Menu> {
        let r = self.public_restaurant(restaurant_id).await?;
        Ok(Menu { restaurant_id: r.detail.id, categories: r.menu })
    }
}
