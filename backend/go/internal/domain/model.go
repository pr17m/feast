// Package domain holds Feast's pure business rules — a port of the Rust crate
// feast-domain. No I/O. Money is int64 minor units of the market's currency.
package domain

import "time"

type Minor = int64

type PlanID string

const (
	Monthly   PlanID = "MONTHLY"
	Quarterly PlanID = "QUARTERLY"
	Yearly    PlanID = "YEARLY"
)

var AllPlans = []PlanID{Monthly, Quarterly, Yearly}

func (p PlanID) Months() int {
	switch p {
	case Quarterly:
		return 3
	case Yearly:
		return 12
	default:
		return 1
	}
}

func (p PlanID) Name() string {
	switch p {
	case Quarterly:
		return "Quarterly"
	case Yearly:
		return "Yearly"
	default:
		return "Monthly"
	}
}

type PhoneRules struct {
	DialCode string `json:"dialCode"`
	Pattern  string `json:"pattern"`
	Example  string `json:"example"`
}

type MarketPayments struct {
	Provider  string   `json:"provider"`
	Methods   []string `json:"methods"`
	PublicKey string   `json:"publicKey,omitempty"`
}

type DocumentRequirement struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description"`
	Issuer      string `json:"issuer,omitempty"`
	Required    bool   `json:"required"`
	NumberLabel string `json:"numberLabel,omitempty"`
	HasExpiry   bool   `json:"hasExpiry"`
}

// Market is one country's rules (config/countries/<cc>.yaml).
type Market struct {
	CountryCode        string                `json:"countryCode"`
	Name               string                `json:"name"`
	Currency           string                `json:"currency"`
	CurrencyDecimals   int                   `json:"currencyDecimals"`
	Locale             string                `json:"locale"`
	Status             string                `json:"status"`
	TaxName            string                `json:"taxName"`
	FoodTaxPercent     float64               `json:"foodTaxPercent"`
	PricesIncludeTax   bool                  `json:"pricesIncludeTax"`
	PlatformTaxPercent float64               `json:"platformTaxPercent"`
	PriceBands         [3]Minor              `json:"priceBands"`
	CoverChargePerPair Minor                 `json:"coverChargePerPair"`
	TipPresets         []Minor               `json:"tipPresets"`
	PlanPrices         map[PlanID]Minor      `json:"planPrices"`
	Payments           MarketPayments        `json:"payments"`
	Phone              PhoneRules            `json:"phone"`
	Documents          []DocumentRequirement `json:"documents"`
}

type SubscriptionPlan struct {
	ID          PlanID  `json:"id"`
	Name        string  `json:"name"`
	Months      int     `json:"months"`
	Currency    string  `json:"currency"`
	Price       Minor   `json:"price"`
	TaxName     string  `json:"taxName"`
	TaxPercent  float64 `json:"taxPercent"`
	Total       Minor   `json:"total"`
	SavePercent int64   `json:"savePercent,omitempty"`
}

type SubscriptionRecord struct {
	TrialEndsAt *time.Time `json:"trialEndsAt,omitempty"`
	PaidThrough *time.Time `json:"paidThrough,omitempty"`
	PlanID      PlanID     `json:"planId,omitempty"`
}

type SubscriptionStatus string

const (
	NotStarted SubscriptionStatus = "NOT_STARTED"
	Trial      SubscriptionStatus = "TRIAL"
	Active     SubscriptionStatus = "ACTIVE"
	PastDue    SubscriptionStatus = "PAST_DUE"
	Expired    SubscriptionStatus = "EXPIRED"
)

type Subscription struct {
	RestaurantID string             `json:"restaurantId"`
	Status       SubscriptionStatus `json:"status"`
	PlanID       PlanID             `json:"planId,omitempty"`
	TrialEndsAt  *time.Time         `json:"trialEndsAt,omitempty"`
	PaidThrough  *time.Time         `json:"paidThrough,omitempty"`
	EndsAt       *time.Time         `json:"endsAt,omitempty"`
	HiddenAfter  *time.Time         `json:"hiddenAfter,omitempty"`
	DaysLeft     *int64             `json:"daysLeft,omitempty"`
	Listed       bool               `json:"listed"`
}

type DocumentStatus string

const (
	DocSubmitted DocumentStatus = "SUBMITTED"
	DocApproved  DocumentStatus = "APPROVED"
	DocRejected  DocumentStatus = "REJECTED"
)

type ComplianceDocument struct {
	ID            string         `json:"id"`
	RestaurantID  string         `json:"restaurantId"`
	RequirementID string         `json:"requirementId"`
	Status        DocumentStatus `json:"status"`
	ExpiresOn     string         `json:"expiresOn,omitempty"` // YYYY-MM-DD
	UploadedAt    string         `json:"uploadedAt"`
}

type RequirementState string

const (
	ReqMissing   RequirementState = "MISSING"
	ReqSubmitted RequirementState = "SUBMITTED"
	ReqApproved  RequirementState = "APPROVED"
	ReqRejected  RequirementState = "REJECTED"
	ReqExpired   RequirementState = "EXPIRED"
)

type ComplianceItem struct {
	Requirement  DocumentRequirement `json:"requirement"`
	Document     *ComplianceDocument `json:"document,omitempty"`
	State        RequirementState    `json:"state"`
	ExpiringSoon bool                `json:"expiringSoon,omitempty"`
}
