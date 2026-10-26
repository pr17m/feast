package domain

import (
	"fmt"
	"math"
	"regexp"
	"sort"
	"strings"
)

// PriceBandFor maps cost for two to band 1–4 using the market's thresholds.
func PriceBandFor(costForTwo Minor, m Market) int {
	switch {
	case costForTwo < m.PriceBands[0]:
		return 1
	case costForTwo < m.PriceBands[1]:
		return 2
	case costForTwo < m.PriceBands[2]:
		return 3
	default:
		return 4
	}
}

// CostLimits is the sensible cost-for-two range, scaled from the price bands.
func CostLimits(m Market) (Minor, Minor) {
	return Minor(math.Round(float64(m.PriceBands[0]) / 10)), m.PriceBands[2] * 30
}

func TaxOn(amount Minor, percent float64) Minor {
	return Minor(math.Round(float64(amount) * percent / 100))
}

func PlanPrice(m Market, p PlanID) Minor { return m.PlanPrices[p] }

// PlansFor lists the subscription plans in the market's currency, with tax and savings.
func PlansFor(m Market) []SubscriptionPlan {
	monthly := PlanPrice(m, Monthly)
	out := make([]SubscriptionPlan, 0, 3)
	for _, id := range AllPlans {
		price := PlanPrice(m, id)
		plan := SubscriptionPlan{ID: id, Name: id.Name(), Months: id.Months(), Currency: m.Currency, Price: price, TaxName: m.TaxName,
			TaxPercent: m.PlatformTaxPercent, Total: price + TaxOn(price, m.PlatformTaxPercent)}
		if full := monthly * Minor(id.Months()); id != Monthly && full > 0 {
			if save := int64(math.Round(float64(full-price) / float64(full) * 100)); save > 0 {
				plan.SavePercent = save
			}
		}
		out = append(out, plan)
	}
	return out
}

// ToE164 turns a national number typed by a person into E.164 ("0412 345 678" → "+61412345678").
func ToE164(m Market, national string) (string, bool) {
	digits := strings.TrimLeft(strings.Map(func(r rune) rune {
		if r >= '0' && r <= '9' {
			return r
		}
		return -1
	}, national), "0")
	re, err := regexp.Compile(m.Phone.Pattern)
	if err != nil || !re.MatchString(digits) {
		return "", false
	}
	return m.Phone.DialCode + digits, true
}

// MarketForPhone finds the market a stored E.164 number belongs to (longest dial code wins).
func MarketForPhone(markets []Market, e164 string) *Market {
	sorted := append([]Market(nil), markets...)
	sort.SliceStable(sorted, func(i, j int) bool { return len(sorted[i].Phone.DialCode) > len(sorted[j].Phone.DialCode) })
	for i := range sorted {
		m := sorted[i]
		if rest, ok := strings.CutPrefix(e164, m.Phone.DialCode); ok {
			if re, err := regexp.Compile(m.Phone.Pattern); err == nil && re.MatchString(rest) {
				return &m
			}
		}
	}
	return nil
}

var symbols = map[string]string{"INR": "₹", "AUD": "A$", "NZD": "NZ$", "CAD": "CA$", "GBP": "£", "USD": "$", "EUR": "€", "JPY": "¥"}

// FormatMoney renders minor units, with Indian digit grouping for INR (₹1,23,456).
func FormatMoney(minor Minor, currency string, decimals int) string {
	unit := Minor(math.Pow10(decimals))
	abs := minor
	if abs < 0 {
		abs = -abs
	}
	whole, frac := abs/unit, abs%unit
	body := group(whole, currency == "INR")
	if frac != 0 {
		body = fmt.Sprintf("%s.%0*d", body, decimals, frac)
	}
	sign := ""
	if minor < 0 {
		sign = "-"
	}
	sym, ok := symbols[currency]
	if !ok {
		return sign + currency + " " + body
	}
	return sign + sym + body
}

func group(n Minor, indian bool) string {
	s := fmt.Sprint(n)
	if len(s) <= 3 {
		return s
	}
	head, tail := s[:len(s)-3], s[len(s)-3:]
	size := 3
	if indian {
		size = 2
	}
	var parts []string
	for len(head) > size {
		parts = append([]string{head[len(head)-size:]}, parts...)
		head = head[:len(head)-size]
	}
	if head != "" {
		parts = append([]string{head}, parts...)
	}
	return strings.Join(parts, ",") + "," + tail
}
