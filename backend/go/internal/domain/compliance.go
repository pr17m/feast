package domain

// ComplianceItems: per requirement MISSING → SUBMITTED → APPROVED/REJECTED;
// an approved document past its expiry is EXPIRED. Dates are YYYY-MM-DD local.
func ComplianceItems(reqs []DocumentRequirement, docs []ComplianceDocument, today, soon string) []ComplianceItem {
	out := make([]ComplianceItem, 0, len(reqs))
	for _, r := range reqs {
		var latest *ComplianceDocument
		for i := range docs {
			d := &docs[i]
			if d.RequirementID == r.ID && (latest == nil || d.UploadedAt > latest.UploadedAt) {
				latest = d
			}
		}
		if latest == nil {
			out = append(out, ComplianceItem{Requirement: r, State: ReqMissing})
			continue
		}
		expired := r.HasExpiry && latest.ExpiresOn != "" && latest.ExpiresOn < today
		state := ReqSubmitted
		switch latest.Status {
		case DocApproved:
			state = ReqApproved
			if expired {
				state = ReqExpired
			}
		case DocRejected:
			state = ReqRejected
		}
		soonFlag := state == ReqApproved && r.HasExpiry && latest.ExpiresOn != "" && latest.ExpiresOn <= soon
		out = append(out, ComplianceItem{Requirement: r, Document: latest, State: state, ExpiringSoon: soonFlag})
	}
	return out
}

// ReadyToSubmit: every required document uploaded and not rejected/expired.
func ReadyToSubmit(items []ComplianceItem) bool {
	for _, i := range items {
		if i.Requirement.Required && (i.State == ReqMissing || i.State == ReqRejected || i.State == ReqExpired) {
			return false
		}
	}
	return true
}

// Complete: every required document approved and valid (needed to be listed).
func Complete(items []ComplianceItem) bool {
	for _, i := range items {
		if i.Requirement.Required && i.State != ReqApproved {
			return false
		}
	}
	return true
}
