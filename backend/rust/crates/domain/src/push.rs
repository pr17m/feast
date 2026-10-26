//! Push notifications to the Feast and Feast Partner phone apps.
//!
//! Every in-app notification is also pushed to the user's registered phones
//! (unless they switched push off). The apps register a token from Firebase
//! Cloud Messaging (Android, and iOS through APNs) after sign-in and remove it
//! on sign-out; tokens the push service reports as gone are dropped.

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::model::{IsoDateTime, Notification};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PushPlatform {
    Ios,
    Android,
}

impl PushPlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::Android => "android",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ios" => Some(Self::Ios),
            "android" => Some(Self::Android),
            _ => None,
        }
    }
}

/// Which store app registered the token.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PushApp {
    /// "Feast", for diners.
    Diner,
    /// "Feast Partner", for restaurant owners.
    Partner,
}

impl PushApp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diner => "diner",
            Self::Partner => "partner",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "diner" => Some(Self::Diner),
            "partner" => Some(Self::Partner),
            _ => None,
        }
    }
}

/// Body of `POST /v1/me/push-tokens`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushTokenRequest {
    pub platform: PushPlatform,
    pub token: String,
    pub app: PushApp,
}

/// A registered device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushToken {
    pub token: String,
    pub user_id: String,
    pub platform: PushPlatform,
    pub app: PushApp,
    pub created_at: IsoDateTime,
    pub last_seen_at: IsoDateTime,
}

/// `GET|PUT /v1/me/push-settings`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushSettings {
    pub enabled: bool,
}

/// What a phone receives.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PushMessage {
    pub title: String,
    pub body: String,
    /// App path to open when tapped, e.g. "/bookings/bk_1".
    pub link: Option<String>,
    pub notification_id: String,
    /// NotificationKind, e.g. "BOOKING_CREATED".
    pub kind: String,
}

/// Longest token accepted (FCM tokens are ~160–200 chars).
pub const MAX_TOKEN_LEN: usize = 4096;

/// A token is a single opaque word of safe characters.
pub fn valid_token(token: &str) -> bool {
    (16..=MAX_TOKEN_LEN).contains(&token.len()) && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.' | '/' | '+' | '='))
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

impl PushMessage {
    /// The push for an in-app notification (lock-screen sized).
    pub fn from_notification(n: &Notification) -> Self {
        let kind = serde_json::to_value(n.kind).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
        Self { title: clip(&n.title, 120), body: clip(&n.body, 400), link: n.link.clone().filter(|l| l.starts_with('/')), notification_id: n.id.clone(), kind }
    }
}

/// What the push service said about one send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome {
    Sent,
    /// The app was uninstalled or the token expired: forget the token.
    Unregistered,
    /// Worth trying again later (5xx, 429, timeout, network, auth hiccup).
    Retry(String),
    /// Won't work however often we try (e.g. a malformed message).
    Failed(String),
}

/// Most attempts per phone before a push is given up (logged).
pub const PUSH_MAX_ATTEMPTS: i64 = 5;

/// Wait before retry number `attempts + 1`: 30 s, 1 min, 2 min, 4 min… (≤ 30 min).
pub fn push_backoff_ms(attempts: i64) -> i64 {
    (30_000i64 << (attempts.clamp(1, 10) - 1)).min(30 * 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NotificationKind;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(push_backoff_ms(1), 30_000);
        assert_eq!(push_backoff_ms(2), 60_000);
        assert_eq!(push_backoff_ms(4), 240_000);
        assert_eq!(push_backoff_ms(9), 30 * 60_000);
    }

    #[test]
    fn token_validation() {
        assert!(valid_token("dGVzdC10b2tlbi0xMjM0NTY3ODkw:APA91bH_abc-def"));
        assert!(!valid_token("short"));
        assert!(!valid_token("has space in it 1234567890"));
        assert!(!valid_token(&"a".repeat(MAX_TOKEN_LEN + 1)));
    }

    #[test]
    fn message_from_notification() {
        let n = Notification {
            id: "ntf_1".into(),
            user_id: "u_1".into(),
            restaurant_id: None,
            kind: NotificationKind::BookingCreated,
            title: "t".repeat(200),
            body: "Your table is booked".into(),
            link: Some("https://evil.example/x".into()),
            event_id: "evt".into(),
            created_at: "2026-10-26T10:00:00Z".into(),
            read_at: None,
        };
        let m = PushMessage::from_notification(&n);
        assert_eq!(m.title.chars().count(), 120);
        assert!(m.title.ends_with('…'));
        assert_eq!(m.link, None, "only in-app paths are passed to the phone");
        assert_eq!(m.kind, "BOOKING_CREATED");
        assert_eq!(PushPlatform::parse("ios"), Some(PushPlatform::Ios));
        assert_eq!(PushApp::parse("partner"), Some(PushApp::Partner));
    }
}
