//! Conversions between domain types / app errors and gRPC messages / statuses.

use feast_application::ports::NotificationDraft;
use feast_application::{AppError, ErrorKind};
use feast_domain::model::{Notification, NotificationKind};
use feast_proto::notification as pb;
use tonic::{Code, Status};

pub fn kind_to_pb(k: NotificationKind) -> pb::NotificationKind {
    match k {
        NotificationKind::BookingCreated => pb::NotificationKind::BookingCreated,
        NotificationKind::BookingCancelled => pb::NotificationKind::BookingCancelled,
        NotificationKind::BookingUpdated => pb::NotificationKind::BookingUpdated,
        NotificationKind::PaymentReceived => pb::NotificationKind::PaymentReceived,
        NotificationKind::SubscriptionPaid => pb::NotificationKind::SubscriptionPaid,
        NotificationKind::SubscriptionExpiring => pb::NotificationKind::SubscriptionExpiring,
        NotificationKind::ApplicationSubmitted => pb::NotificationKind::ApplicationSubmitted,
        NotificationKind::DocumentReviewed => pb::NotificationKind::DocumentReviewed,
        NotificationKind::RestaurantApproved => pb::NotificationKind::RestaurantApproved,
        NotificationKind::DocumentExpiring => pb::NotificationKind::DocumentExpiring,
        NotificationKind::CaseUpdated => pb::NotificationKind::CaseUpdated,
        NotificationKind::ReviewUpdated => pb::NotificationKind::ReviewUpdated,
        NotificationKind::ContentModerated => pb::NotificationKind::ContentModerated,
    }
}

pub fn kind_from_pb(k: i32) -> Option<NotificationKind> {
    Some(match pb::NotificationKind::try_from(k).ok()? {
        pb::NotificationKind::Unspecified => return None,
        pb::NotificationKind::BookingCreated => NotificationKind::BookingCreated,
        pb::NotificationKind::BookingCancelled => NotificationKind::BookingCancelled,
        pb::NotificationKind::BookingUpdated => NotificationKind::BookingUpdated,
        pb::NotificationKind::PaymentReceived => NotificationKind::PaymentReceived,
        pb::NotificationKind::SubscriptionPaid => NotificationKind::SubscriptionPaid,
        pb::NotificationKind::SubscriptionExpiring => NotificationKind::SubscriptionExpiring,
        pb::NotificationKind::ApplicationSubmitted => NotificationKind::ApplicationSubmitted,
        pb::NotificationKind::DocumentReviewed => NotificationKind::DocumentReviewed,
        pb::NotificationKind::RestaurantApproved => NotificationKind::RestaurantApproved,
        pb::NotificationKind::DocumentExpiring => NotificationKind::DocumentExpiring,
        pb::NotificationKind::CaseUpdated => NotificationKind::CaseUpdated,
        pb::NotificationKind::ReviewUpdated => NotificationKind::ReviewUpdated,
        pb::NotificationKind::ContentModerated => NotificationKind::ContentModerated,
    })
}

fn opt(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

pub fn notification_to_pb(n: &Notification) -> pb::Notification {
    pb::Notification {
        id: n.id.clone(),
        user_id: n.user_id.clone(),
        restaurant_id: n.restaurant_id.clone().unwrap_or_default(),
        kind: kind_to_pb(n.kind) as i32,
        title: n.title.clone(),
        body: n.body.clone(),
        link: n.link.clone().unwrap_or_default(),
        event_id: n.event_id.clone(),
        created_at: n.created_at.clone(),
        read_at: n.read_at.clone().unwrap_or_default(),
    }
}

pub fn notification_from_pb(n: pb::Notification) -> Result<Notification, AppError> {
    Ok(Notification {
        kind: kind_from_pb(n.kind).ok_or_else(|| AppError::internal("notification with unknown kind"))?,
        id: n.id,
        user_id: n.user_id,
        restaurant_id: opt(n.restaurant_id),
        title: n.title,
        body: n.body,
        link: opt(n.link),
        event_id: n.event_id,
        created_at: n.created_at,
        read_at: opt(n.read_at),
    })
}

pub fn draft_to_pb(d: &NotificationDraft) -> pb::NotificationDraft {
    pb::NotificationDraft {
        kind: kind_to_pb(d.kind) as i32,
        title: d.title.clone(),
        body: d.body.clone(),
        link: d.link.clone().unwrap_or_default(),
        restaurant_id: d.restaurant_id.clone().unwrap_or_default(),
    }
}

pub fn draft_from_pb(d: pb::NotificationDraft) -> Result<NotificationDraft, Status> {
    Ok(NotificationDraft {
        kind: kind_from_pb(d.kind).ok_or_else(|| Status::invalid_argument("draft.kind is required"))?,
        title: d.title,
        body: d.body,
        link: opt(d.link),
        restaurant_id: opt(d.restaurant_id),
    })
}

/// App error → gRPC status. The stable app error code travels in metadata.
pub fn to_status(e: AppError) -> Status {
    let code = match e.kind {
        ErrorKind::Validation => Code::InvalidArgument,
        ErrorKind::Unauthenticated => Code::Unauthenticated,
        ErrorKind::Forbidden => Code::PermissionDenied,
        ErrorKind::NotFound => Code::NotFound,
        ErrorKind::Conflict => Code::FailedPrecondition,
        ErrorKind::Unavailable => Code::Unavailable,
        ErrorKind::Internal => Code::Internal,
    };
    let mut s = Status::new(code, e.message);
    if let Ok(v) = e.code.parse() {
        s.metadata_mut().insert("x-feast-error-code", v);
    }
    s
}

/// gRPC status → app error (client side).
pub fn from_status(s: Status) -> AppError {
    let kind = match s.code() {
        Code::InvalidArgument | Code::OutOfRange => ErrorKind::Validation,
        Code::Unauthenticated => ErrorKind::Unauthenticated,
        Code::PermissionDenied => ErrorKind::Forbidden,
        Code::NotFound => ErrorKind::NotFound,
        Code::FailedPrecondition | Code::AlreadyExists | Code::Aborted => ErrorKind::Conflict,
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted | Code::Cancelled => ErrorKind::Unavailable,
        _ => ErrorKind::Internal,
    };
    if kind == ErrorKind::Internal {
        return AppError::internal(format!("notification service: {s}"));
    }
    if kind == ErrorKind::Unavailable {
        tracing::warn!(status = %s, "notification service unavailable");
        return AppError::unavailable("Notifications are temporarily unavailable");
    }
    let code = s.metadata().get("x-feast-error-code").and_then(|v| v.to_str().ok()).unwrap_or("NOTIFICATION_ERROR").to_owned();
    AppError::new(kind, code, s.message())
}
