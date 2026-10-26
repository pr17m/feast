import type { Market, Paise } from "./api/types";

/*
 * Money formatting. Amounts are integers in the currency's minor unit.
 * Screens run inside a "market scope" (the city being browsed, the restaurant
 * being managed) that sets the default currency; pass `currency` explicitly
 * when a list can mix markets (admin views).
 */

let activeCurrency = "INR";
const locales = new Map<string, string>([["INR", "en-IN"]]);
const decimals = new Map<string, number>([["INR", 2]]);

/** Teach the formatter each market's locale and decimals. */
export function registerMarkets(markets: Pick<Market, "currency" | "locale" | "currencyDecimals">[]): void {
  for (const m of markets) {
    locales.set(m.currency, m.locale);
    decimals.set(m.currency, m.currencyDecimals);
  }
}

export function setActiveCurrency(currency: string | undefined): void {
  if (currency) activeCurrency = currency;
}

export function getActiveCurrency(): string {
  return activeCurrency;
}

function digits(currency: string): number {
  const known = decimals.get(currency);
  if (known !== undefined) return known;
  try {
    return new Intl.NumberFormat("en", { style: "currency", currency }).resolvedOptions().maximumFractionDigits ?? 2;
  } catch {
    return 2;
  }
}

const formatters = new Map<string, Intl.NumberFormat>();
function formatter(currency: string, fraction: number): Intl.NumberFormat {
  const key = `${currency}:${fraction}`;
  let f = formatters.get(key);
  if (!f) {
    try {
      f = new Intl.NumberFormat(locales.get(currency) ?? "en", { style: "currency", currency, minimumFractionDigits: fraction, maximumFractionDigits: fraction });
    } catch {
      f = new Intl.NumberFormat("en", { minimumFractionDigits: fraction, maximumFractionDigits: fraction });
    }
    formatters.set(key, f);
  }
  return f;
}

/** "₹1,800", "£45.50", "AED 120". Whole amounts drop the decimals. */
export function money(minor: Paise, currency: string = activeCurrency, opts: { signed?: boolean } = {}): string {
  const d = digits(currency);
  const major = Math.abs(minor) / 10 ** d;
  const s = formatter(currency, Number.isInteger(major) ? 0 : d).format(major);
  if (minor < 0) return `−${s}`;
  return opts.signed && minor > 0 ? `+${s}` : s;
}

/** Major units (as typed by a person) → integer minor units. */
export function toMinor(major: number | string, currency: string = activeCurrency): number {
  return Math.round(Number(major) * 10 ** digits(currency));
}

/** Integer minor units → major units for an input field. */
export function toMajor(minor: number, currency: string = activeCurrency): string {
  return String(minor / 10 ** digits(currency));
}

/** The currency's symbol, e.g. "₹", "£", "AED". */
export function currencySymbol(currency: string = activeCurrency): string {
  try {
    return formatter(currency, 0).formatToParts(0).find((p) => p.type === "currency")?.value ?? currency;
  } catch {
    return currency;
  }
}

/** "₹₹₹" style price band. */
export function priceBand(band: number, currency: string = activeCurrency): string {
  const sym = currencySymbol(currency);
  return sym.length <= 2 ? sym.repeat(band) : `${sym} ${"•".repeat(band)}`;
}

/** Local calendar date as YYYY-MM-DD (not UTC). */
export function toISODate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function fromISODate(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(d: Date, n: number): Date {
  const x = new Date(d);
  x.setDate(x.getDate() + n);
  return x;
}

export function dayLabel(iso: string, today = new Date()): string {
  const d = fromISODate(iso);
  const t = toISODate(today);
  if (iso === t) return "Today";
  if (iso === toISODate(addDays(today, 1))) return "Tomorrow";
  return d.toLocaleDateString(undefined, { weekday: "short" });
}

export function longDate(iso: string): string {
  return fromISODate(iso).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

export function time12(hhmm: string): string {
  const [h, m] = hhmm.split(":").map(Number);
  const suffix = h >= 12 ? "PM" : "AM";
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return `${h12}:${String(m).padStart(2, "0")} ${suffix}`;
}

export function timeAgo(iso: string, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - new Date(iso).getTime()) / 1000));
  if (s < 60) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  return `${h} hr ago`;
}

/** Short money label for charts: ₹850, ₹12.4k, ₹1.2L. */
/** Short form for charts: "₹1.2L", "₹12k", "$3.4k" (lakh only for INR). */
export function compactMoney(minor: Paise, currency: string = activeCurrency): string {
  const major = minor / 10 ** digits(currency);
  const sym = currencySymbol(currency);
  if (currency === "INR" && major >= 100000) return `${sym}${(major / 100000).toFixed(1).replace(/\.0$/, "")}L`;
  if (major >= 1_000_000) return `${sym}${(major / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (major >= 1000) return `${sym}${(major / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  return `${sym}${Math.round(major)}`;
}
