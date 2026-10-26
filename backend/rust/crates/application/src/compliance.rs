//! Restaurant onboarding: partner signup, licence/document uploads, admin
//! review. Which documents are needed comes from the market (country config),
//! so launching a country never changes this code.
//!
//! Lifecycle: ONBOARDING → (owner submits) PENDING_APPROVAL → (admin approves
//! every required document, then the restaurant) LIVE. Changes requested →
//! back to ONBOARDING with notes.

use feast_domain::compliance::{complete, ready_to_submit};
use feast_domain::model::*;
use feast_domain::DomainEvent;

use crate::admin::NewRestaurant;
use crate::context::Feast;
use crate::error::{AppError, AppResult};

/// File types accepted for documents.
pub const ALLOWED_DOCUMENT_TYPES: [&str; 4] = ["application/pdf", "image/jpeg", "image/png", "image/webp"];

/// An uploaded document, as parsed by the delivery layer (multipart form).
#[derive(Debug, Clone)]
pub struct DocumentUpload {
    pub requirement_id: String,
    pub file_name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub number: Option<String>,
    pub expires_on: Option<String>,
}

/// Admin decision on one document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Review {
    Approve,
    Reject,
}

/// A served document file.
pub struct DocumentFile {
    pub file_name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

impl Feast {
    async fn compliance_status_for(&self, r: &Restaurant) -> AppResult<ComplianceStatus> {
        let d = &r.detail;
        let m = self.market(&d.country_code).await?;
        let docs = self.p.compliance.documents_for(&d.id).await?;
        let tz = self.city_tz(&d.city_id).await?;
        let (items, is_complete) = self.compliance_items(&m, &docs, &tz);
        let info = self.p.compliance.review_info(&d.id).await?;
        Ok(ComplianceStatus {
            restaurant_id: d.id.clone(),
            restaurant_status: d.status,
            country_code: d.country_code.clone(),
            ready_to_submit: ready_to_submit(&items),
            complete: is_complete,
            items,
            review_notes: info.notes,
            submitted_at: info.submitted_at,
        })
    }

    pub async fn compliance(&self, user: &User, restaurant_id: &str) -> AppResult<ComplianceStatus> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        self.compliance_status_for(&r).await
    }

    pub async fn upload_document(&self, user: &User, restaurant_id: &str, up: DocumentUpload) -> AppResult<ComplianceStatus> {
        let r = self.editable_restaurant(user, restaurant_id).await?;
        if r.detail.status == RestaurantStatus::Suspended {
            return Err(AppError::conflict("SUSPENDED", "This restaurant is suspended. Contact Feast support."));
        }
        let m = self.market(&r.detail.country_code).await?;
        let req = m
            .documents
            .iter()
            .find(|d| d.id == up.requirement_id)
            .ok_or_else(|| AppError::validation("INVALID_DOCUMENT", "This document isn't needed in your country"))?;
        if up.bytes.is_empty() {
            return Err(AppError::validation("EMPTY_FILE", "The file is empty"));
        }
        if up.bytes.len() > self.settings.max_upload_bytes {
            return Err(AppError::validation("FILE_TOO_LARGE", format!("Files must be under {} MB", self.settings.max_upload_bytes / 1_048_576)));
        }
        // Trust the file's bytes, not the client's content type.
        let content_type = sniff(&up.bytes).filter(|t| ALLOWED_DOCUMENT_TYPES.contains(t));
        let Some(content_type) = content_type else {
            tracing::debug!(claimed = %up.content_type, "rejected upload with unrecognised content");
            return Err(AppError::validation("INVALID_FILE_TYPE", "Upload a PDF, JPG, PNG or WebP file"));
        };
        let number = up.number.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        if req.number_label.is_some() && number.is_none() {
            return Err(AppError::validation("NUMBER_REQUIRED", format!("Enter the {}", req.number_label.as_deref().unwrap_or("document number"))));
        }
        if number.as_deref().is_some_and(|n| n.chars().count() > 64) {
            return Err(AppError::validation("INVALID_NUMBER", "That number is too long"));
        }
        let tz = self.city_tz(&r.detail.city_id).await?;
        let today = self.p.clock.local_date(&tz, 0);
        let expires_on = up.expires_on.map(|e| e.trim().to_owned()).filter(|e| !e.is_empty());
        if req.has_expiry {
            let Some(e) = &expires_on else {
                return Err(AppError::validation("EXPIRY_REQUIRED", "Enter the expiry date"));
            };
            if chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").is_err() {
                return Err(AppError::validation("INVALID_EXPIRY", "Enter the expiry date as YYYY-MM-DD"));
            }
            if e.as_str() <= today.as_str() {
                return Err(AppError::validation("EXPIRED", "This document has already expired. Upload a current one."));
            }
        }
        let id = self.p.ids.new_id("doc");
        let file_name: String = up.file_name.chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').take(120).collect();
        let key = format!("documents/{}/{}", r.detail.id, id);
        self.p.files.put(&key, up.bytes.clone(), content_type).await?;
        let doc = ComplianceDocument {
            id,
            restaurant_id: r.detail.id.clone(),
            requirement_id: req.id.clone(),
            file_name: if file_name.is_empty() { "document".into() } else { file_name },
            file_type: content_type.to_owned(),
            file_size: up.bytes.len() as i64,
            storage_key: key,
            number,
            expires_on: if req.has_expiry { expires_on } else { None },
            status: DocumentStatus::Submitted,
            review_note: None,
            uploaded_at: self.p.clock.now_iso(),
            uploaded_by: user.id.clone(),
            reviewed_at: None,
            reviewed_by: None,
        };
        self.p.compliance.insert_document(&doc).await?;
        self.audit_direct(Some(&doc.restaurant_id), "document", &doc.id, "document.uploaded", |a| {
            a.to_status = Some("SUBMITTED".into());
            a.note = Some(format!("{} · {}", doc.requirement_id, doc.file_name));
        })
        .await?;
        self.compliance_status_for(&r).await
    }

    /// Owner sends the application for review.
    pub async fn submit_for_review(&self, user: &User, restaurant_id: &str) -> AppResult<ComplianceStatus> {
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        if r.detail.status != RestaurantStatus::Onboarding {
            return Err(AppError::conflict("ALREADY_SUBMITTED", "This application has already been submitted"));
        }
        let status = self.compliance_status_for(&r).await?;
        if !status.ready_to_submit {
            return Err(AppError::validation("DOCUMENTS_MISSING", "Upload every required document first"));
        }
        r.detail.status = RestaurantStatus::PendingApproval;
        self.p.restaurants.update(&r).await?;
        let info = crate::ports::ReviewInfo { notes: None, submitted_at: Some(self.p.clock.now_iso()) };
        self.p.compliance.set_review_info(restaurant_id, &info).await?;
        let ev = self.event(Some(restaurant_id), Some(&user.id), DomainEvent::ApplicationSubmitted { restaurant: r.detail.reference() });
        self.p.outbox.append(&[ev]).await?;
        self.compliance_status_for(&r).await
    }

    /// The file of a document: its restaurant's managers and admins only.
    pub async fn document_file(&self, user: &User, document_id: &str) -> AppResult<DocumentFile> {
        let d = self.p.compliance.get_document(document_id).await?.ok_or_else(|| AppError::not_found("Document"))?;
        self.managed_restaurant(user, &d.restaurant_id).await?;
        let (bytes, content_type) = self.p.files.get(&d.storage_key).await?;
        Ok(DocumentFile { file_name: d.file_name, content_type, bytes })
    }

    // ------------------------------------------------------------ admin

    /// Restaurants waiting for review (and those still onboarding), oldest first.
    pub async fn approval_queue(&self, user: &User) -> AppResult<Vec<ApprovalQueueItem>> {
        self.require_role(user, &[Role::Admin])?;
        let mut out = vec![];
        for r in self.p.restaurants.list().await? {
            if !matches!(r.detail.status, RestaurantStatus::Onboarding | RestaurantStatus::PendingApproval) {
                // Live restaurants with documents waiting (renewed licences) also need review.
                let docs = self.p.compliance.documents_for(&r.detail.id).await?;
                if !docs.iter().any(|d| d.status == DocumentStatus::Submitted) {
                    continue;
                }
            }
            out.push(ApprovalQueueItem {
                restaurant: Self::admin_ref(&r.detail),
                owners: self.owner_refs(&r.detail.id).await?,
                compliance: self.compliance_status_for(&r).await?,
            });
        }
        let rank = |s: RestaurantStatus| {
            if s == RestaurantStatus::PendingApproval {
                0
            } else if s == RestaurantStatus::Onboarding {
                2
            } else {
                1
            }
        };
        out.sort_by(|a, b| {
            rank(a.restaurant.status)
                .cmp(&rank(b.restaurant.status))
                .then_with(|| a.compliance.submitted_at.cmp(&b.compliance.submitted_at))
                .then_with(|| a.restaurant.name.cmp(&b.restaurant.name))
        });
        Ok(out)
    }

    pub async fn review_document(&self, user: &User, document_id: &str, decision: Review, note: Option<String>) -> AppResult<ComplianceStatus> {
        self.require_role(user, &[Role::Admin])?;
        let mut d = self.p.compliance.get_document(document_id).await?.ok_or_else(|| AppError::not_found("Document"))?;
        let r = self.p.restaurants.get(&d.restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        let note = note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        if decision == Review::Reject && note.is_none() {
            return Err(AppError::validation("NOTE_REQUIRED", "Tell the restaurant what's wrong with this document"));
        }
        d.status = if decision == Review::Approve { DocumentStatus::Approved } else { DocumentStatus::Rejected };
        d.review_note = note;
        d.reviewed_at = Some(self.p.clock.now_iso());
        d.reviewed_by = Some(user.id.clone());
        self.p.compliance.update_document(&d).await?;
        let m = self.market(&r.detail.country_code).await?;
        let requirement_name = m.documents.iter().find(|x| x.id == d.requirement_id).map(|x| x.name.clone()).unwrap_or_else(|| d.requirement_id.clone());
        let ev = self.event(Some(&r.detail.id), Some(&user.id), DomainEvent::DocumentReviewed { document: d, requirement_name });
        self.p.outbox.append(&[ev]).await?;
        self.compliance_status_for(&r).await
    }

    /// Send the application back to the owner with notes.
    pub async fn request_changes(&self, user: &User, restaurant_id: &str, notes: &str) -> AppResult<ComplianceStatus> {
        self.require_role(user, &[Role::Admin])?;
        let notes = notes.trim();
        if notes.is_empty() {
            return Err(AppError::validation("NOTE_REQUIRED", "Tell the restaurant what to change"));
        }
        let mut r = self.p.restaurants.get(restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        if r.detail.status == RestaurantStatus::PendingApproval {
            r.detail.status = RestaurantStatus::Onboarding;
            self.p.restaurants.update(&r).await?;
        }
        let mut info = self.p.compliance.review_info(restaurant_id).await?;
        info.notes = Some(notes.to_owned());
        self.p.compliance.set_review_info(restaurant_id, &info).await?;
        let ev = self.event(Some(restaurant_id), Some(&user.id), DomainEvent::ChangesRequested { restaurant: r.detail.reference(), notes: notes.to_owned() });
        self.p.outbox.append(&[ev]).await?;
        self.compliance_status_for(&r).await
    }

    // ------------------------------------------------------------ partner signup

    /// A restaurant owner signs up: creates their partner account (email +
    /// password) and the restaurant in ONBOARDING, then signs them in.
    pub async fn partner_signup(&self, req: PartnerSignupRequest) -> AppResult<AuthSession> {
        let o = &req.owner;
        let name = o.name.trim();
        if name.chars().count() < 2 {
            return Err(AppError::validation("INVALID_NAME", "Enter your name"));
        }
        let email = o.email.trim().to_lowercase();
        if !valid_email(&email) {
            return Err(AppError::validation("INVALID_EMAIL", "Enter a valid email"));
        }
        if o.password.chars().count() < 8 {
            return Err(AppError::validation("WEAK_PASSWORD", "Use at least 8 characters for your password"));
        }
        let phone = o.phone.as_deref().map(str::trim).filter(|p| !p.is_empty());
        if let Some(p) = phone {
            if !self.is_valid_phone(p).await? {
                return Err(AppError::validation("INVALID_PHONE", "Enter a valid mobile number"));
            }
            if self.p.users.find_by_phone(p).await?.is_some() {
                return Err(AppError::conflict("DUPLICATE", "An account with this phone already exists. Sign in instead."));
            }
        }
        if self.p.users.find_by_email(&email).await?.is_some() {
            return Err(AppError::conflict("DUPLICATE", "An account with this email already exists. Sign in instead."));
        }
        let rq = &req.restaurant;
        let city = self.p.markets.get_city(&rq.city_id).await?.ok_or_else(|| AppError::validation("INVALID_CITY", "Pick a city"))?;
        if city.country_code != self.settings.country {
            return Err(AppError::validation("INVALID_CITY", "Feast isn't available for restaurants in that city on this site"));
        }
        let r = self
            .build_restaurant(NewRestaurant {
                name: &rq.name,
                city_id: &rq.city_id,
                area: &rq.area,
                cuisines: &rq.cuisines,
                cost_for_two: rq.cost_for_two,
                is_pure_veg: rq.is_pure_veg,
                tags: &[],
                about: rq.about.as_deref().unwrap_or(""),
            })
            .await?;
        let user = User {
            id: self.p.ids.new_id("u"),
            name: name.to_owned(),
            phone: phone.map(str::to_owned),
            email: Some(email),
            role: Role::Owner,
            restaurant_ids: vec![r.detail.id.clone()],
            status: UserStatus::Active,
            created_at: self.p.clock.now_iso(),
        };
        let hash = self.p.passwords.hash(&o.password)?;
        self.p.restaurants.insert(&r).await?;
        self.p.users.insert(&user, Some(&hash)).await?;
        Ok(AuthSession { token: self.p.tokens.issue(&user)?, user })
    }

    /// Whether every required document is approved (for callers outside this module).
    pub async fn is_compliant(&self, restaurant_id: &str) -> AppResult<bool> {
        let r = self.p.restaurants.get(restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        Ok(complete(&self.compliance_status_for(&r).await?.items))
    }
}

fn valid_email(e: &str) -> bool {
    let Some((local, domain)) = e.split_once('@') else { return false };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && e.len() <= 254 && !e.contains(char::is_whitespace)
}

/// Content type from magic bytes (don't trust the client's header).
pub(crate) fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_and_emails() {
        assert_eq!(sniff(b"%PDF-1.7 ..."), Some("application/pdf"));
        assert_eq!(sniff(b"<html>"), None);
        assert!(valid_email("owner@cafe.com.au"));
        assert!(!valid_email("owner@cafe"));
        assert!(!valid_email("a b@c.com"));
    }
}
