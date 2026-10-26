package domain

// Same cases as the Rust tests (feast-domain), so both backends agree.

import (
	"testing"
	"time"
)

func australia() Market {
	return Market{CountryCode: "AU", Currency: "AUD", CurrencyDecimals: 2, TaxName: "GST", PlatformTaxPercent: 10,
		PriceBands: [3]Minor{6000, 12000, 22000}, PlanPrices: map[PlanID]Minor{Monthly: 5900, Quarterly: 15900, Yearly: 59000},
		Phone: PhoneRules{DialCode: "+61", Pattern: `^4\d{8}$`}}
}

func TestMoney(t *testing.T) {
	cases := []struct {
		minor    Minor
		currency string
		want     string
	}{{101_800, "INR", "₹1,018"}, {12_345_678_00, "INR", "₹1,23,45,678"}, {6_925, "INR", "₹69.25"}, {4_550, "AUD", "A$45.50"}, {123_456_78, "AUD", "A$123,456.78"}}
	for _, c := range cases {
		if got := FormatMoney(c.minor, c.currency, 2); got != c.want {
			t.Errorf("FormatMoney(%d,%s) = %q, want %q", c.minor, c.currency, got, c.want)
		}
	}
}

func TestPlansAndBands(t *testing.T) {
	m := australia()
	p := PlansFor(m)
	if p[0].Total != 6490 || p[2].SavePercent != 17 {
		t.Fatalf("plans %+v", p)
	}
	if PriceBandFor(9000, m) != 2 || PriceBandFor(30000, m) != 4 {
		t.Fatal("bands")
	}
	if e, ok := ToE164(m, "0412 345 678"); !ok || e != "+61412345678" {
		t.Fatal("e164", e)
	}
	if MarketForPhone([]Market{m}, "+61412345678") == nil || MarketForPhone([]Market{m}, "+919876543210") != nil {
		t.Fatal("market for phone")
	}
}

func TestSubscriptionLifecycle(t *testing.T) {
	now := time.Date(2026, 1, 31, 10, 0, 0, 0, time.UTC)
	rec := StartTrial(nil, now, DefaultRules)
	if v := View("r", &rec, now, DefaultRules); v.Status != Trial || *v.DaysLeft != 30 || !v.Listed {
		t.Fatalf("trial %+v", v)
	}
	later := now.AddDate(0, 0, 33)
	if v := View("r", &rec, later, DefaultRules); v.Status != PastDue || !v.Listed {
		t.Fatalf("past due %+v", v)
	}
	if v := View("r", &rec, now.AddDate(0, 0, 40), DefaultRules); v.Status != Expired || v.Listed {
		t.Fatalf("expired %+v", v)
	}
	rec2, start, end := Renew(rec, Monthly, now)
	if !start.Equal(*rec.TrialEndsAt) || end != AddMonths(start, 1) || rec2.PlanID != Monthly {
		t.Fatal("renew starts at trial end")
	}
	if AddMonths(time.Date(2026, 1, 31, 0, 0, 0, 0, time.UTC), 1).Day() != 28 {
		t.Fatal("month-end clamp")
	}
	if InvoiceNumber(now, 42) != "FST-202601-00042" {
		t.Fatal(InvoiceNumber(now, 42))
	}
}

func TestCompliance(t *testing.T) {
	reqs := []DocumentRequirement{{ID: "licence", Required: true, HasExpiry: true}, {ID: "tax", Required: true}, {ID: "liquor", HasExpiry: true}}
	docs := []ComplianceDocument{
		{RequirementID: "licence", Status: DocRejected, ExpiresOn: "2027-01-01", UploadedAt: "2026-01-01"},
		{RequirementID: "licence", Status: DocApproved, ExpiresOn: "2026-11-10", UploadedAt: "2026-02-01"},
		{RequirementID: "tax", Status: DocSubmitted, UploadedAt: "2026-02-01"},
	}
	it := ComplianceItems(reqs, docs, "2026-10-24", "2026-11-23")
	if it[0].State != ReqApproved || !it[0].ExpiringSoon || it[1].State != ReqSubmitted || it[2].State != ReqMissing {
		t.Fatalf("%+v", it)
	}
	if !ReadyToSubmit(it) || Complete(it) {
		t.Fatal("gates")
	}
	if later := ComplianceItems(reqs, docs, "2026-12-01", "2026-12-31"); later[0].State != ReqExpired || ReadyToSubmit(later) {
		t.Fatal("expiry")
	}
}
