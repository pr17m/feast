/**
 * Market (country) helpers and the default market. Everything that differs by
 * country — currency, taxes, price bands, plan prices, payment provider,
 * onboarding documents — comes from a `Market`, never from constants in code.
 */
import type { Market, PlanId, PriceBand, SubscriptionPlan } from "./types";

/** Digits after the decimal point for an ISO 4217 currency (via Intl; 2 if unknown). */
export function currencyDecimals(currency: string): number {
  try {
    return new Intl.NumberFormat("en", { style: "currency", currency }).resolvedOptions().maximumFractionDigits ?? 2;
  } catch {
    return 2;
  }
}

/** Minor units per major unit: 100 for INR/GBP, 1 for JPY, 1000 for KWD. */
export function minorPerMajor(m: Pick<Market, "currencyDecimals">): number {
  return 10 ** m.currencyDecimals;
}

export function priceBandFor(costForTwo: number, m: Pick<Market, "priceBands">): PriceBand {
  const [a, b, c] = m.priceBands;
  return costForTwo < a ? 1 : costForTwo < b ? 2 : costForTwo < c ? 3 : 4;
}

/** Sensible cost-for-two range for a market (scaled from its price bands). */
export function costLimits(m: Pick<Market, "priceBands">): { min: number; max: number } {
  return { min: Math.round(m.priceBands[0] / 10), max: m.priceBands[2] * 30 };
}

const PLAN_META: { id: PlanId; name: string; months: number }[] = [
  { id: "MONTHLY", name: "Monthly", months: 1 },
  { id: "QUARTERLY", name: "Quarterly", months: 3 },
  { id: "YEARLY", name: "Yearly", months: 12 },
];

export function taxOn(amount: number, percent: number): number {
  return Math.round((amount * percent) / 100);
}

/** The market's subscription plans, with tax and the saving vs monthly. */
export function plansFor(m: Market): SubscriptionPlan[] {
  const monthly = m.planPrices.MONTHLY;
  return PLAN_META.map(({ id, name, months }) => {
    const price = m.planPrices[id];
    const save = monthly > 0 ? Math.round((1 - price / (monthly * months)) * 100) : 0;
    const plan: SubscriptionPlan = {
      id,
      name,
      months,
      currency: m.currency,
      price,
      taxName: m.taxName,
      taxPercent: m.platformTaxPercent,
      total: price + taxOn(price, m.platformTaxPercent),
    };
    return save > 0 ? { ...plan, savePercent: save } : plan;
  });
}

/** E.164 from a market's dial code and a national number ("0412 345 678" → "+61412345678"). */
export function toE164(m: Pick<Market, "phone">, national: string): string | null {
  const digits = national.replace(/\D/g, "").replace(/^0+/, "");
  return new RegExp(m.phone.pattern).test(digits) ? `${m.phone.dialCode}${digits}` : null;
}

/** The market a stored E.164 number belongs to (longest dial code wins). */
export function marketForPhone<M extends Pick<Market, "phone">>(markets: M[], e164: string): M | undefined {
  return [...markets].sort((a, b) => b.phone.dialCode.length - a.phone.dialCode.length).find((m) => e164.startsWith(m.phone.dialCode) && new RegExp(m.phone.pattern).test(e164.slice(m.phone.dialCode.length)));
}

/** Effective food tax for a city (city override, else market). */
export function foodTaxFor(m: Market, city?: { foodTaxPercent?: number }): number {
  return city?.foodTaxPercent ?? m.foodTaxPercent;
}
