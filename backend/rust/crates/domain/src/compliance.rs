//! Onboarding-document rules: per requirement MISSING → SUBMITTED →
//! APPROVED/REJECTED; an APPROVED document with a past expiry is EXPIRED.
//! A restaurant may list only when every *required* document is APPROVED.

use crate::model::{ComplianceDocument, ComplianceItem, DocumentRequirement, DocumentStatus, RequirementState};

/// `docs` are all uploads for the restaurant; the latest per requirement counts.
/// Dates are `YYYY-MM-DD` in the restaurant's local time.
pub fn items(requirements: &[DocumentRequirement], docs: &[ComplianceDocument], today: &str, soon: &str) -> Vec<ComplianceItem> {
    requirements
        .iter()
        .map(|req| {
            let latest = docs.iter().filter(|d| d.requirement_id == req.id).max_by(|a, b| a.uploaded_at.cmp(&b.uploaded_at));
            let Some(doc) = latest else {
                return ComplianceItem { requirement: req.clone(), document: None, state: RequirementState::Missing, expiring_soon: None };
            };
            let expired = req.has_expiry && doc.expires_on.as_deref().is_some_and(|e| e < today);
            let state = match doc.status {
                DocumentStatus::Approved if expired => RequirementState::Expired,
                DocumentStatus::Approved => RequirementState::Approved,
                DocumentStatus::Submitted => RequirementState::Submitted,
                DocumentStatus::Rejected => RequirementState::Rejected,
            };
            let expiring_soon = (state == RequirementState::Approved && req.has_expiry && doc.expires_on.as_deref().is_some_and(|e| e <= soon)).then_some(true);
            ComplianceItem { requirement: req.clone(), document: Some(doc.clone()), state, expiring_soon }
        })
        .collect()
}

/// Every required document uploaded and not rejected/expired.
pub fn ready_to_submit(items: &[ComplianceItem]) -> bool {
    items
        .iter()
        .filter(|i| i.requirement.required)
        .all(|i| !matches!(i.state, RequirementState::Missing | RequirementState::Rejected | RequirementState::Expired))
}

/// Every required document approved and valid.
pub fn complete(items: &[ComplianceItem]) -> bool {
    items.iter().filter(|i| i.requirement.required).all(|i| i.state == RequirementState::Approved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: &str, required: bool, has_expiry: bool) -> DocumentRequirement {
        DocumentRequirement { id: id.into(), name: id.into(), description: String::new(), issuer: None, required, number_label: None, has_expiry }
    }
    fn doc(req: &str, status: DocumentStatus, expires: Option<&str>, at: &str) -> ComplianceDocument {
        ComplianceDocument {
            id: format!("d_{req}_{at}"),
            restaurant_id: "r".into(),
            requirement_id: req.into(),
            file_name: "f.pdf".into(),
            file_type: "application/pdf".into(),
            file_size: 1,
            storage_key: String::new(),
            number: None,
            expires_on: expires.map(Into::into),
            status,
            review_note: None,
            uploaded_at: at.into(),
            uploaded_by: "u".into(),
            reviewed_at: None,
            reviewed_by: None,
        }
    }

    #[test]
    fn states_and_gates() {
        let reqs = vec![req("licence", true, true), req("tax", true, false), req("liquor", false, true)];
        let docs = vec![
            doc("licence", DocumentStatus::Rejected, Some("2027-01-01"), "2026-01-01"),
            doc("licence", DocumentStatus::Approved, Some("2026-11-10"), "2026-02-01"), // latest wins
            doc("tax", DocumentStatus::Submitted, None, "2026-02-01"),
        ];
        let it = items(&reqs, &docs, "2026-10-24", "2026-11-23");
        assert_eq!(it[0].state, RequirementState::Approved);
        assert_eq!(it[0].expiring_soon, Some(true));
        assert_eq!(it[1].state, RequirementState::Submitted);
        assert_eq!(it[2].state, RequirementState::Missing);
        assert!(ready_to_submit(&it));
        assert!(!complete(&it));
        // After expiry the licence blocks listing.
        let later = items(&reqs, &docs, "2026-12-01", "2026-12-31");
        assert_eq!(later[0].state, RequirementState::Expired);
        assert!(!ready_to_submit(&later));
    }
}
