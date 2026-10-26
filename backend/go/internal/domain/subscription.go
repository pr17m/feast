package domain

import (
	"fmt"
	"math"
	"time"
)

// Rules: free trial from first going live, then grace days before hiding.
type Rules struct {
	TrialDays int
	GraceDays int
}

var DefaultRules = Rules{TrialDays: 30, GraceDays: 7}

// AddMonths adds calendar months, clamping to month end (31 Jan + 1 → 28/29 Feb).
func AddMonths(t time.Time, months int) time.Time {
	y, m, d := t.Date()
	first := time.Date(y, m+time.Month(months), 1, t.Hour(), t.Minute(), t.Second(), t.Nanosecond(), t.Location())
	last := first.AddDate(0, 1, -1).Day()
	if d > last {
		d = last
	}
	return time.Date(first.Year(), first.Month(), d, t.Hour(), t.Minute(), t.Second(), t.Nanosecond(), t.Location())
}

// PeriodEnd is the end of whatever the restaurant currently has (trial or paid).
func PeriodEnd(r SubscriptionRecord) *time.Time {
	var end *time.Time
	for _, t := range []*time.Time{r.TrialEndsAt, r.PaidThrough} {
		if t != nil && (end == nil || t.After(*end)) {
			end = t
		}
	}
	return end
}

// View derives the subscription state shown to owners and admins.
func View(restaurantID string, rec *SubscriptionRecord, now time.Time, rules Rules) Subscription {
	if rec == nil || PeriodEnd(*rec) == nil {
		return Subscription{RestaurantID: restaurantID, Status: NotStarted}
	}
	end := *PeriodEnd(*rec)
	hidden := end.AddDate(0, 0, rules.GraceDays)
	paid := rec.PaidThrough != nil && rec.PaidThrough.After(now)
	status := Expired
	switch {
	case now.Before(end) && paid:
		status = Active
	case now.Before(end):
		status = Trial
	case now.Before(hidden):
		status = PastDue
	}
	days := int64(math.Ceil(end.Sub(now).Hours() / 24))
	return Subscription{RestaurantID: restaurantID, Status: status, PlanID: rec.PlanID, TrialEndsAt: rec.TrialEndsAt, PaidThrough: rec.PaidThrough,
		EndsAt: &end, HiddenAfter: &hidden, DaysLeft: &days, Listed: status != Expired}
}

// StartTrial starts the free trial the first time a restaurant goes live.
func StartTrial(rec *SubscriptionRecord, now time.Time, rules Rules) SubscriptionRecord {
	if rec != nil && rec.TrialEndsAt != nil {
		return *rec
	}
	out := SubscriptionRecord{}
	if rec != nil {
		out = *rec
	}
	t := now.AddDate(0, 0, rules.TrialDays)
	out.TrialEndsAt = &t
	return out
}

// Renew adds a paid period starting at max(now, current end).
func Renew(rec SubscriptionRecord, plan PlanID, now time.Time) (SubscriptionRecord, time.Time, time.Time) {
	start := now
	if e := PeriodEnd(rec); e != nil && e.After(now) {
		start = *e
	}
	end := AddMonths(start, plan.Months())
	rec.PaidThrough, rec.PlanID = &end, plan
	return rec, start, end
}

// InvoiceNumber formats like FST-202610-00042.
func InvoiceNumber(at time.Time, seq int64) string {
	return fmt.Sprintf("FST-%04d%02d-%05d", at.Year(), int(at.Month()), seq)
}
