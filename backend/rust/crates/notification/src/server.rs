//! gRPC server for the notification service.

use std::sync::Arc;

use feast_application::ports::NotificationGateway;
use feast_proto::notification as pb;
use feast_proto::notification::notification_service_server::{NotificationService as Rpc, NotificationServiceServer};
use tonic::{Request, Response, Status};

use crate::convert::*;
use crate::service::NotificationService;

pub struct NotificationGrpc {
    svc: Arc<NotificationService>,
}

impl NotificationGrpc {
    pub fn new(svc: Arc<NotificationService>) -> Self {
        Self { svc }
    }
}

fn require_user(user_id: &str) -> Result<(), Status> {
    if user_id.is_empty() {
        return Err(Status::invalid_argument("user_id is required"));
    }
    Ok(())
}

#[tonic::async_trait]
impl Rpc for NotificationGrpc {
    async fn publish(&self, req: Request<pb::PublishRequest>) -> Result<Response<pb::PublishResponse>, Status> {
        let r = req.into_inner();
        if r.event_id.is_empty() {
            return Err(Status::invalid_argument("event_id is required"));
        }
        let draft = draft_from_pb(r.draft.ok_or_else(|| Status::invalid_argument("draft is required"))?)?;
        let created = self.svc.publish(&r.event_id, &r.event_type, &r.recipient_user_ids, &draft).await.map_err(to_status)?;
        Ok(Response::new(pb::PublishResponse { created: created.iter().map(notification_to_pb).collect() }))
    }

    async fn list(&self, req: Request<pb::ListRequest>) -> Result<Response<pb::ListResponse>, Status> {
        let r = req.into_inner();
        require_user(&r.user_id)?;
        let limit = r.page.map(|p| p.limit).filter(|n| *n > 0).unwrap_or(50);
        let list = self.svc.list(&r.user_id, r.unread_only, limit).await.map_err(to_status)?;
        Ok(Response::new(pb::ListResponse { items: list.items.iter().map(notification_to_pb).collect(), unread: list.unread, page: None }))
    }

    async fn mark_read(&self, req: Request<pb::MarkReadRequest>) -> Result<Response<pb::MarkReadResponse>, Status> {
        let r = req.into_inner();
        require_user(&r.user_id)?;
        let updated = self.svc.mark_read(&r.user_id, &r.notification_ids).await.map_err(to_status)?;
        let unread = self.svc.unread_count(&r.user_id).await.map_err(to_status)?;
        Ok(Response::new(pb::MarkReadResponse { updated, unread }))
    }

    async fn unread_count(&self, req: Request<pb::UnreadCountRequest>) -> Result<Response<pb::UnreadCountResponse>, Status> {
        let r = req.into_inner();
        require_user(&r.user_id)?;
        Ok(Response::new(pb::UnreadCountResponse { unread: self.svc.unread_count(&r.user_id).await.map_err(to_status)? }))
    }
}

/// Serve gRPC on a bound listener until `shutdown` resolves. If `token` is set,
/// every call must carry `authorization: Bearer <token>` (service-to-service auth).
pub async fn serve(
    listener: tokio::net::TcpListener,
    svc: Arc<NotificationService>,
    token: Option<String>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), tonic::transport::Error> {
    let expected = token.filter(|t| !t.is_empty()).map(|t| format!("Bearer {t}"));
    let check = move |req: Request<()>| -> Result<Request<()>, Status> {
        if let Some(exp) = &expected {
            let got = req.metadata().get("authorization").and_then(|v| v.to_str().ok());
            if got != Some(exp.as_str()) {
                return Err(Status::unauthenticated("invalid service token"));
            }
        }
        Ok(req)
    };
    if let Ok(addr) = listener.local_addr() {
        tracing::info!(%addr, "notification gRPC server listening");
    }
    tonic::transport::Server::builder()
        .add_service(NotificationServiceServer::with_interceptor(NotificationGrpc::new(svc), check))
        .serve_with_incoming_shutdown(tokio_stream::wrappers::TcpListenerStream::new(listener), shutdown)
        .await
}
