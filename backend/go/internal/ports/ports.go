// Package ports mirrors the Rust `feast_application::ports`: the interfaces
// business code needs. Adapters (Postgres, Redis, Stripe, Razorpay, S3, Azure
// Blob, Twilio…) implement them; the composition root picks them from config.
package ports

import (
	"context"
	"time"

	"github.com/feast/backend/go/internal/domain"
)

// ---- persistence (one Postgres adapter implements all; schema = ../migrations/postgres)

type MarketRepository interface {
	ListMarkets(ctx context.Context) ([]domain.Market, error)
	GetMarket(ctx context.Context, countryCode string) (*domain.Market, error)
	UpsertMarket(ctx context.Context, m domain.Market) error
}

type SubscriptionRepository interface {
	GetRecord(ctx context.Context, restaurantID string) (*domain.SubscriptionRecord, error)
	AllRecords(ctx context.Context) (map[string]domain.SubscriptionRecord, error)
	PutRecord(ctx context.Context, restaurantID string, rec domain.SubscriptionRecord) error
	NextInvoiceSeq(ctx context.Context) (int64, error)
}

type ComplianceRepository interface {
	DocumentsFor(ctx context.Context, restaurantID string) ([]domain.ComplianceDocument, error)
	InsertDocument(ctx context.Context, d domain.ComplianceDocument) error
	UpdateDocument(ctx context.Context, d domain.ComplianceDocument) error
}

// Event is the outbox/bus envelope (JSON identical to the Rust EventEnvelope).
type Event struct {
	ID           string    `json:"id"`
	Type         string    `json:"type"`
	OccurredAt   time.Time `json:"occurredAt"`
	RestaurantID string    `json:"restaurantId,omitempty"`
	UserID       string    `json:"userId,omitempty"`
	Payload      []byte    `json:"payload"`
}

type Outbox interface {
	Append(ctx context.Context, events []Event) error
	FetchPending(ctx context.Context, limit int) ([]Event, error)
	MarkPublished(ctx context.Context, ids []string) error
}

// ---- messaging

type MessageBus interface {
	Publish(ctx context.Context, e Event) error
	// Subscribe as a competing consumer of `group` (Redis Streams consumer group).
	Subscribe(ctx context.Context, group, consumer string) (<-chan Event, func(Event) error, error)
	Health(ctx context.Context) error
}

// ---- payments: one adapter per provider, chosen per market

type CheckoutSession struct {
	ProviderRef string
	Checkout    map[string]string // public data for the client SDK (never secrets)
}

type ChargeOutcome struct {
	Succeeded         bool
	Method            string
	ProviderPaymentID string
	FailureReason     string
}

type PaymentIntent struct {
	ID          string
	Amount      domain.Minor
	Currency    string
	Provider    string
	ProviderRef string
}

type PaymentGateway interface {
	ID() string
	PublicKey() string
	CreateOrder(ctx context.Context, intent PaymentIntent) (CheckoutSession, error)
	// Charge verifies with the provider; payload from the client is only a pointer.
	Charge(ctx context.Context, intent PaymentIntent, method string, payload map[string]string) (ChargeOutcome, error)
}

type PaymentProviders interface {
	Get(id string) (PaymentGateway, bool)
}

// ---- other services

type FileStorage interface {
	Put(ctx context.Context, key string, data []byte, contentType string) error
	Get(ctx context.Context, key string) ([]byte, string, error)
	Health(ctx context.Context) error
}

type OtpService interface {
	Send(ctx context.Context, phone string) (hint string, err error)
	Verify(ctx context.Context, phone, code string) (bool, error)
}

type Clock interface {
	Now() time.Time
	LocalDate(tz string, days int) string
}
