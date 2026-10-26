//! gRPC client adapter: the core's `NotificationGateway` port, backed by a
//! remote notification service.

use std::time::Duration;

use async_trait::async_trait;
use feast_application::ports::{NotificationDraft, NotificationGateway};
use feast_application::{AppError, AppResult};
use feast_domain::model::{Notification, NotificationList};
use feast_proto::common;
use feast_proto::notification as pb;
use feast_proto::notification::notification_service_client::NotificationServiceClient;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

use crate::convert::*;

#[derive(Clone)]
pub struct GrpcNotificationGateway {
    client: NotificationServiceClient<Channel>,
    caller: String,
    auth: Option<MetadataValue<tonic::metadata::Ascii>>,
}

impl GrpcNotificationGateway {
    /// Connects lazily (the service may start after us); requests time out after `timeout`.
    pub fn new(url: &str, caller: &str, token: Option<&str>, timeout: Duration) -> AppResult<Self> {
        let channel = Endpoint::from_shared(url.to_owned())
            .map_err(|e| AppError::validation("BAD_GRPC_URL", format!("{url}: {e}")))?
            .connect_timeout(Duration::from_secs(3))
            .timeout(timeout)
            .connect_lazy();
        let auth = match token.filter(|t| !t.is_empty()) {
            Some(t) => Some(format!("Bearer {t}").parse().map_err(|_| AppError::validation("BAD_GRPC_TOKEN", "service token must be ASCII"))?),
            None => None,
        };
        Ok(Self { client: NotificationServiceClient::new(channel), caller: caller.to_owned(), auth })
    }

    fn req<T>(&self, msg: T) -> Request<T> {
        let mut r = Request::new(msg);
        if let Some(a) = &self.auth {
            r.metadata_mut().insert("authorization", a.clone());
        }
        r
    }

    fn caller(&self, user_id: &str) -> Option<common::Caller> {
        Some(common::Caller { service: self.caller.clone(), user_id: user_id.to_owned(), request_id: String::new() })
    }
}

#[async_trait]
impl NotificationGateway for GrpcNotificationGateway {
    async fn publish(&self, event_id: &str, event_type: &str, recipients: &[String], draft: &NotificationDraft) -> AppResult<Vec<Notification>> {
        let msg = pb::PublishRequest {
            caller: self.caller(""),
            event_id: event_id.to_owned(),
            event_type: event_type.to_owned(),
            recipient_user_ids: recipients.to_vec(),
            draft: Some(draft_to_pb(draft)),
        };
        let res = self.client.clone().publish(self.req(msg)).await.map_err(from_status)?.into_inner();
        res.created.into_iter().map(notification_from_pb).collect()
    }

    async fn list(&self, user_id: &str, unread_only: bool, limit: u32) -> AppResult<NotificationList> {
        let msg = pb::ListRequest {
            caller: self.caller(user_id),
            user_id: user_id.to_owned(),
            unread_only,
            page: Some(common::PageRequest { limit, cursor: String::new() }),
        };
        let res = self.client.clone().list(self.req(msg)).await.map_err(from_status)?.into_inner();
        Ok(NotificationList { items: res.items.into_iter().map(notification_from_pb).collect::<AppResult<_>>()?, unread: res.unread })
    }

    async fn mark_read(&self, user_id: &str, ids: &[String]) -> AppResult<i64> {
        let msg = pb::MarkReadRequest { caller: self.caller(user_id), user_id: user_id.to_owned(), notification_ids: ids.to_vec() };
        Ok(self.client.clone().mark_read(self.req(msg)).await.map_err(from_status)?.into_inner().updated)
    }

    async fn unread_count(&self, user_id: &str) -> AppResult<i64> {
        let msg = pb::UnreadCountRequest { caller: self.caller(user_id), user_id: user_id.to_owned() };
        Ok(self.client.clone().unread_count(self.req(msg)).await.map_err(from_status)?.into_inner().unread)
    }
}
