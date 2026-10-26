import type { BillLine, Market, Offer, Paise } from "./api/types";

/**
 * Pure pricing rules. No I/O, no framework — the backends port this file
 * (`feast_domain::pricing`) and are the source of truth; clients only use it
 * for previews. Country-specific numbers (tax, rounding unit, cover charge)
 * come from the restaurant's `Market`.
 */

/** What pricing needs from a market. */
export type PricingMarket = Pick<Market, "currencyDecimals" | "taxName" | "foodTaxPercent" | "pricesIncludeTax" | "coverChargePerPair">;

const unitOf = (m: Pick<Market, "currencyDecimals">) => 10 ** m.currencyDecimals;

/** Discounts are whole currency units (e.g. whole rupees/pounds). */
export function offerDiscount(offer: Offer, amount: Paise, m: Pick<Market, "currencyDecimals">): Paise {
  if (offer.minBill && amount < offer.minBill) return 0;
  let d = 0;
  if (offer.percentOff) d = Math.round((amount * offer.percentOff) / 100);
  if (offer.flatOff) d = Math.max(d, offer.flatOff);
  if (offer.maxDiscount) d = Math.min(d, offer.maxDiscount);
  const unit = unitOf(m);
  d = Math.floor(d / unit) * unit;
  return Math.max(0, Math.min(d, amount));
}

/** Picks the offer that saves the most on this amount. Ties go to the first listed. */
export function bestOffer(offers: Offer[], amount: Paise, m: Pick<Market, "currencyDecimals">): { offer?: Offer; discount: Paise } {
  let best: { offer?: Offer; discount: Paise } = { discount: 0 };
  for (const o of offers) {
    const d = offerDiscount(o, amount, m);
    if (d > best.discount) best = { offer: o, discount: d };
  }
  return best;
}

/**
 * The restaurant's bill from its item total, like a printed bill:
 * - tax-exclusive markets (IN, CA): item total + tax, rounded off to a whole unit;
 * - tax-inclusive markets (AU, NZ, UK): the item total *is* the bill; an
 *   informational line shows the tax it includes.
 * `taxPercent` is the city's effective rate (see `foodTaxFor`).
 */
export function itemTotalWithTax(itemTotal: Paise, m: PricingMarket, taxPercent: number = m.foodTaxPercent): { lines: BillLine[]; total: Paise } {
  const unit = unitOf(m);
  const lines: BillLine[] = [{ label: "Item total", amount: itemTotal, kind: "ITEM_TOTAL" }];
  if (m.pricesIncludeTax) {
    const included = Math.round(itemTotal - itemTotal / (1 + taxPercent / 100));
    if (taxPercent > 0) lines.push({ label: `Includes ${m.taxName} (${taxPercent}%)`, amount: included, kind: "TAX_INCLUDED" });
    return { lines, total: itemTotal };
  }
  const tax = Math.round((itemTotal * taxPercent) / 100);
  const exact = itemTotal + tax;
  const total = Math.round(exact / unit) * unit;
  if (taxPercent > 0) lines.push({ label: `${m.taxName} (${taxPercent}%)`, amount: tax, kind: "TAX" });
  if (total !== exact) lines.push({ label: "Round off", amount: total - exact, kind: "ADJUSTMENT" });
  return { lines, total };
}

export function payableFor(billAmount: Paise, discount: Paise, tip: Paise): Paise {
  return Math.max(0, billAmount - discount + tip);
}

/** Cover charge collected at booking time for premium slots/venues; adjusted against the final bill. */
export function coverChargeFor(priceBand: number, guests: number, m: Pick<Market, "coverChargePerPair">): Paise {
  if (priceBand < 4 || m.coverChargePerPair <= 0) return 0;
  return m.coverChargePerPair * Math.ceil(guests / 2);
}
