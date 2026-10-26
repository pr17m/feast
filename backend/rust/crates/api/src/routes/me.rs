use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use feast_domain::model::{AuthProvider, NotificationList, User};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;

use crate::error::{ApiError, ApiResult};
use crate::extract::{Caller, OptBody, Q};
use crate::realtime::{route_for, LiveEvent};
use crate::AppState;

pub async fn me(Caller(u): Caller) -> Json<User> {
    Json(u)
}

pub async fn identities(State(s): State<AppState>, Caller(u): Caller) -> ApiResult<Json<Vec<AuthProvider>>> {
    Ok(Json(s.feast.my_identities(&u).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQ {
    unread_only: Option<bool>,
    limit: Option<u32>,
}

pub async fn notifications(State(s): State<AppState>, Caller(u): Caller, Q(q): Q<ListQ>) -> ApiResult<Json<NotificationList>> {
    Ok(Json(s.feast.my_notifications(&u, q.unread_only.unwrap_or(false), q.limit.unwrap_or(30)).await?))
}

#[derive(Deserialize, Default)]
pub struct ReadReq {
    #[serde(default)]
    ids: Vec<String>,
}

pub async fn mark_read(State(s): State<AppState>, Caller(u): Caller, OptBody(body): OptBody<ReadReq>) -> ApiResult<Json<Value>> {
    let ids = body.ids;
    let updated = s.feast.mark_notifications_read(&u, &ids).await?;
    let unread = s.feast.my_notifications(&u, true, 1).await?.unread;
    Ok(Json(json!({ "updated": updated, "unread": unread })))
}

#[derive(Deserialize)]
pub struct EventsQ {
    /// EventSource can't send headers, so the stream also accepts the token here.
    access_token: Option<String>,
}

/// `GET /v1/me/events` — Server-Sent Events:
/// - `ready`        `{ "unread": n }` once on connect
/// - `notification` a new `Notification` for this user
/// - `change`       `{ type, restaurantId, eventId }` — refetch affected screens
pub async fn events(
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
    Q(q): Q<EventsQ>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .or(q.access_token)
        .ok_or_else(|| ApiError(feast_application::AppError::unauthenticated("Please sign in")))?;
    let user = s.feast.authenticate(&token).await?;
    let unread = s.feast.my_notifications(&user, true, 1).await.map(|l| l.unread).unwrap_or(0);

    let hub = s.hub.clone();
    let rx = hub.subscribe();
    let ready = stream::once(async move { Ok(Event::default().event("ready").data(json!({ "unread": unread }).to_string())) });
    let live = BroadcastStream::new(rx).filter_map(move |item| {
        let user = user.clone();
        async move {
            let env = item.ok()?; // lagged receivers just skip ahead
            Some(Ok(match route_for(&user, &env)? {
                LiveEvent::Notification(n) => Event::default().event("notification").id(env.id.clone()).data(n.to_string()),
                LiveEvent::Change(c) => Event::default().event("change").id(env.id.clone()).data(c.to_string()),
            }))
        }
    });
    // Count the connection for as long as the stream lives.
    let guard = hub.connected();
    let until_shutdown = ready.chain(live).take_until(hub.closed()).boxed();
    let counted = stream::unfold((until_shutdown, guard), |(mut st, g)| async move { st.next().await.map(|ev| (ev, (st, g))) });
    Ok(Sse::new(counted).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping")))
}
