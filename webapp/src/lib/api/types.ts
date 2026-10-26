/**
 * Feast API contract — domain types shared by the web app and the backends,
 * and (later) the Go backend and React Native apps.
 *
 * Conventions
 * - Money is always an integer in paise (₹1 = 100). Never use floats for money.
 * - Timestamps are ISO-8601 strings in UTC; dates are "YYYY-MM-DD" in the city's timezone.
 * - IDs are opaque strings.
 *
 * Keep this file framework-free so it can move into a shared package
 * (packages/core) when the mobile apps arrive.
 */

export type Paise = number;
export type ISODate = string; // YYYY-MM-DD
export type ISODateTime = string;

// ---------- Location ----------

export interface City {
  id: string;
  name: string;
  /** ISO 3166-1 alpha-2 country of the city's market, e.g. "IN". */
  countryCode: string;
  /** IANA time zone for slots and "today", e.g. "Asia/Kolkata". */
  timezone: string;
  /** Overrides the market's food tax here (e.g. Ontario HST 13%). */
  foodTaxPercent?: number;
  areas: string[];
  lat: number;
  lng: number;
}

// ---------- Markets (countries) ----------

/**
 * A country Feast operates in. Everything money- or tax-related comes from here,
 * so launching a new country is data entry for an admin, not a code change.
 * All amounts are integers in the currency's minor unit (paise, pence, cents…;
 * see `currencyDecimals`).
 */
export interface Market {
  countryCode: string;
  name: string;
  /** ISO 4217, e.g. "INR", "GBP", "AED". */
  currency: string;
  /** Digits after the decimal point (2 for INR/GBP, 0 for JPY, 3 for KWD). */
  currencyDecimals: number;
  /** BCP 47 locale for formatting, e.g. "en-IN". */
  locale: string;
  status: MarketStatus;
  /** Tax name shown on bills and invoices: "GST", "VAT", "Sales tax". */
  taxName: string;
  /** Tax on restaurant bills (percent). Cities may override it (e.g. Canadian provinces). */
  foodTaxPercent: number;
  /** Menu prices already include tax (AU/NZ/UK) vs tax added on the bill (IN/CA). */
  pricesIncludeTax: boolean;
  /** Tax on Feast subscriptions (percent). */
  platformTaxPercent: number;
  /** Cost-for-two thresholds (minor units) between price bands 1|2, 2|3, 3|4. */
  priceBands: [number, number, number];
  /** Upfront cover charge per pair of guests at band-4 venues (0 = none). */
  coverChargePerPair: number;
  /** Tip buttons on the pay-bill screen (minor units; 0 = "No tip"). */
  tipPresets: number[];
  /** Subscription prices before tax (minor units). */
  planPrices: Record<PlanId, number>;
  payments: MarketPayments;
  phone: PhoneRules;
  /** Licences/registrations a restaurant must submit to list in this country. */
  documents: DocumentRequirement[];
}

export interface DocumentRequirement {
  /** Stable key, e.g. "fssai". */
  id: string;
  name: string;
  description: string;
  /** Who issues it, e.g. "FSSAI", "Municipal corporation". */
  issuer?: string;
  /** Must be approved before the restaurant can go live. */
  required: boolean;
  /** Label for the licence/registration number; omit if there is none. */
  numberLabel?: string;
  /** The document expires (owner must give an expiry date; expired = not valid). */
  hasExpiry: boolean;
}

export type MarketStatus = "LIVE" | "COMING_SOON";

/** Phone sign-in rules. Numbers are stored in E.164 (+61412345678). */
export interface PhoneRules {
  /** "+91", "+61". */
  dialCode: string;
  /** Regex for the national number (digits only, no leading 0), e.g. "^[6-9]\\d{9}$". */
  pattern: string;
  /** Shown as a placeholder, e.g. "98765 43210". */
  example: string;
}

export interface MarketPayments {
  /** Payment provider id configured on the server: "mock", "razorpay", "stripe"… */
  provider: string;
  /** Methods offered to diners in this market. */
  methods: PaymentMethod[];
  /** Public key/config the client SDK needs (never a secret). Filled by the server. */
  publicKey?: string;
}

/** Where the visitor seems to be (CDN country header / GeoIP on the server). */
export interface GeoResult {
  countryCode?: string;
  /** Feast operates there (a LIVE market). */
  supported: boolean;
  /** Suggested city in that market, if any. */
  cityId?: string;
  source: "header" | "geoip" | "browser" | "default";
  /** Another Feast deployment serves that country: its URL. */
  redirectUrl?: string;
}

/** A payment provider the server can use (credentials live in server config). */
export interface PaymentProviderInfo {
  id: string;
  name: string;
  /** Credentials present in server config, so markets may use it. */
  configured: boolean;
  methods: PaymentMethod[];
}

export type MarketInput = Omit<Market, "payments"> & { payments: Omit<MarketPayments, "publicKey"> };

export interface CreateCityRequest {
  name: string;
  countryCode: string;
  timezone: string;
  foodTaxPercent?: number;
  areas: string[];
  lat: number;
  lng: number;
}

// ---------- Offers ----------

/** Where an offer can be redeemed. Mirrors Dineout's split between pre-booking and walk-in bill offers. */
export type OfferScope = "PREBOOK" | "BILL" | "BANK";

export interface Offer {
  id: string;
  scope: OfferScope;
  title: string; // "Flat 25% off"
  subtitle?: string; // "on total bill"
  percentOff?: number; // 0-100
  flatOff?: Paise;
  maxDiscount?: Paise;
  minBill?: Paise;
  terms: string[];
  /** Hidden from diners by Feast; the owner sees why. Only Feast sets this. */
  moderation?: Moderation;
}

// ---------- Restaurants ----------

export type PriceBand = 1 | 2 | 3 | 4; // ₹ .. ₹₹₹₹

export interface RestaurantSummary {
  id: string;
  slug: string;
  name: string;
  cityId: string;
  area: string;
  cuisines: string[];
  rating: number; // 0-5
  ratingCount: number;
  costForTwo: Paise;
  priceBand: PriceBand;
  /** ISO 4217 currency for every amount about this restaurant. */
  currency: string;
  countryCode: string;
  distanceKm: number;
  isPureVeg: boolean;
  tags: string[]; // "Rooftop", "Live music", "Fine dining"
  /** Cover photo (`image`) plus the generated-art fallback (hue + motif). */
  art: { hue: number; motif: CuisineMotif; image?: string; imageCredit?: string };
  headlineOffer?: string; // "Flat 30% off on pre-booking"
  acceptsBookings: boolean;
  acceptsBillPayment: boolean;
}

export type CuisineMotif =
  | "pizza"
  | "noodles"
  | "curry"
  | "grill"
  | "cafe"
  | "dessert"
  | "seafood"
  | "bar";

export interface OpeningHours {
  day: "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun";
  open: string; // "12:00"
  close: string; // "23:30"
}

/**
 * ONBOARDING: owner is filling details and uploading documents.
 * PENDING_APPROVAL: submitted; Feast admins are reviewing the documents.
 * LIVE / PAUSED: approved (PAUSED = owner temporarily stopped bookings/payments).
 * SUSPENDED: taken down by an admin.
 */
export type RestaurantStatus = "ONBOARDING" | "PENDING_APPROVAL" | "LIVE" | "PAUSED" | "SUSPENDED";

/** Owner-controlled switches. Admins can change them for any restaurant. */
export interface RestaurantSettings {
  bookingsEnabled: boolean;
  billPaymentEnabled: boolean;
  /** Tables offered per 30-minute slot. */
  tablesPerSlot: number;
  /** Most a Feast deal may take off one bill, whatever the offer says (minor units; absent = no cap). */
  maxDiscountPerBill?: Paise;
  /** Most discounted bills per day (absent = unlimited). Past it, bills are payable without the deal. */
  dailyDealLimit?: number;
}

/** Today's deal budget (owner view). */
export interface DealBudget {
  /** Restaurant's local date, YYYY-MM-DD. */
  date: string;
  dailyDealLimit?: number;
  maxDiscountPerBill?: Paise;
  /** Discounted bills paid today plus payments in progress. */
  used: number;
  /** Absent = unlimited. */
  remaining?: number;
}

export interface RestaurantDetail extends RestaurantSummary {
  status: RestaurantStatus;
  settings: RestaurantSettings;
  address: string;
  about: string;
  phone: string;
  hours: OpeningHours[];
  highlights: string[];
  offers: Offer[];
  photos: { id: string; caption: string; hue: number; url?: string; credit?: string }[];
  menuPreview: MenuItem[];
}

// ---------- Discovery (server-driven sections) ----------

/**
 * Home is a list of typed sections returned by the server, so the backend can
 * reorder, add or hide sections without an app release (the "SDUI-lite" idea
 * borrowed from Swiggy's widget responses). Clients ignore unknown types.
 */
export type HomeSection =
  | { type: "HERO_BANNER"; id: string; title: string; subtitle: string; cta: Cta; hue: number }
  | { type: "COLLECTIONS"; id: string; title: string; items: Collection[] }
  | { type: "RESTAURANT_CAROUSEL"; id: string; title: string; subtitle?: string; restaurants: RestaurantSummary[] }
  | { type: "RESTAURANT_LIST"; id: string; title: string; restaurants: RestaurantSummary[] };

export interface Cta {
  label: string;
  /** App route, e.g. "/restaurants/olive-tree?tab=menu". Mobile maps these to native screens. */
  href: string;
}

export interface Collection {
  id: string;
  title: string;
  count: number;
  hue: number;
  query: SearchQuery;
}

export interface HomeResponse {
  city: City;
  sections: HomeSection[];
}

export interface SearchQuery {
  q?: string;
  cuisine?: string;
  tag?: string;
  area?: string;
  priceBand?: PriceBand;
  offer?: OfferScope;
  service?: SearchService;
  pureVeg?: boolean;
  minRating?: number;
  sort?: "relevance" | "rating" | "distance" | "costLow" | "costHigh";
}

/** What a diner can do there on Feast (driven by the owner's service toggles). */
export type SearchService = "BOOKINGS" | "BILL_PAYMENT";

/** One filter option, built from what restaurants actually list. */
export interface Facet {
  value: string;
  label: string;
  /** Restaurants matching this option given the *other* active filters. */
  count: number;
}

/**
 * Filters are derived from live listing data (cuisines, experiences, area,
 * cost for two, offers and services that owners manage), never hard-coded.
 * Options with no matches are omitted unless currently selected.
 */
export interface SearchFacets {
  cuisines: Facet[];
  tags: Facet[];
  areas: Facet[];
  priceBands: Facet[];
  offers: Facet[];
  services: Facet[];
  /** Pure-veg restaurants matching the other filters (0 → hide the chip). */
  pureVeg: number;
}

export interface SearchResponse {
  total: number;
  restaurants: RestaurantSummary[];
  facets: SearchFacets;
}

/** What an owner can change about how their restaurant is listed and found. */
export interface ListingPatch {
  cuisines?: string[];
  tags?: string[];
  isPureVeg?: boolean;
  costForTwo?: Paise;
  about?: string;
  highlights?: string[];
}

// ---------- Menu ----------

export interface MenuItem {
  id: string;
  name: string;
  description: string;
  price: Paise;
  isVeg: boolean;
  isBestseller?: boolean;
  spiceLevel?: 0 | 1 | 2 | 3;
  /** Variant and addon prices are added on top of the base `price`. */
  variants?: { id: string; name: string; price: Paise }[];
  addons?: { id: string; name: string; price: Paise }[];
  available: boolean;
  /** Dish photo: an uploaded photo ("/v1/media/…", relative to the API — use mediaUrl()) or a stock photo URL. */
  image?: string;
  /** Who supplied the photo; missing with an image = stock (older data). */
  imageSource?: "OWNER" | "STOCK" | "GENERATED";
  /** Photographer credit for stock photos. */
  imageCredit?: string;
  /** The owner removed a stock photo; Feast won't add one automatically. */
  stockDeclined?: boolean;
  /** Hidden from diners by Feast; the owner sees why. Only Feast sets this. */
  moderation?: Moderation;
}

export interface MenuCategory {
  id: string;
  name: string;
  items: MenuItem[];
}

export interface Menu {
  restaurantId: string;
  categories: MenuCategory[];
}

// ---------- Table booking ----------

export type MealPeriod = "BREAKFAST" | "LUNCH" | "DINNER";

export interface Slot {
  id: string; // stable: `${restaurantId}:${date}:${time}`
  time: string; // "19:30"
  period: MealPeriod;
  available: boolean;
  seatsLeft: number;
  /** Deal attached to this slot. Off-peak slots carry bigger discounts. */
  deal?: Offer;
}

export interface SlotAvailability {
  restaurantId: string;
  date: ISODate;
  guests: number;
  slots: Slot[];
}

export type BookingStatus = "PENDING_PAYMENT" | "CONFIRMED" | "SEATED" | "CANCELLED" | "COMPLETED" | "NO_SHOW";

export interface CreateBookingRequest {
  restaurantId: string;
  date: ISODate;
  slotId: string;
  guests: number;
  dealId?: string;
  specialRequest?: string;
  contact: { name: string; phone: string };
  /** Idempotency key so retries never double-book. */
  idempotencyKey: string;
}

export interface Booking {
  id: string;
  userId: string;
  code: string; // short code shown at the restaurant, e.g. "FST-7K2Q"
  restaurant: Pick<RestaurantSummary, "id" | "slug" | "name" | "area" | "art">;
  date: ISODate;
  time: string;
  guests: number;
  deal?: Offer;
  specialRequest?: string;
  contact: { name: string; phone: string };
  status: BookingStatus;
  /** Booking fee / cover charge collected upfront, adjusted in the final bill. 0 = free booking. */
  coverCharge: Paise;
  currency: string;
  payment?: PaymentRef;
  createdAt: ISODateTime;
  /** When the diner tapped "I'm here" at the restaurant. */
  checkedInAt?: ISODateTime;
}

// ---------- The restaurant's printed QR ----------

/** What a scanned restaurant QR (`/r/{code}`, or an old table code) opens. */
export interface RestaurantQr {
  code: string;
  restaurant: RestaurantSummary;
}

// ---------- Bills & payments ----------

export interface BillLine {
  label: string;
  amount: Paise; // negative for discounts
  /** TAX_INCLUDED is informational ("includes GST $4.09") and not added to totals. */
  kind: "ITEM_TOTAL" | "TAX" | "TAX_INCLUDED" | "DISCOUNT" | "FEE" | "TIP" | "ADJUSTMENT";
}

export interface Bill {
  id: string;
  restaurantId: string;
  currency: string;
  billAmount: Paise; // amount printed on the restaurant bill
  lines: BillLine[];
  appliedOffer?: Offer;
  savings: Paise;
  payable: Paise;
  /** Why the Feast deal is smaller than the offer or missing (per-bill cap, today's deals used up). */
  dealNotice?: string;
  /** The diner's Feast booking this bill is paid against (Feast deals need one). */
  bookingId?: string;
}

export interface QuoteBillRequest {
  restaurantId: string;
  billAmount: Paise;
  tip?: Paise;
  offerId?: string; // omit to auto-apply the best offer
  /** A confirmed booking for today at this restaurant; its pre-booking deal becomes eligible. */
  bookingId?: string;
}

export type PaymentMethod = "UPI" | "CARD" | "NETBANKING" | "WALLET" | "APPLE_PAY" | "GOOGLE_PAY" | OfflineMethod;
/** Paid at the restaurant's counter and confirmed by the owner (never settled by Feast). */
export type OfflineMethod = "CASH" | "CARD_AT_COUNTER";
/** Who handled the money: Feast's payment provider, or the restaurant at its counter. */
export type Settlement = "ONLINE" | "OFFLINE";

export interface PaymentIntent {
  id: string;
  purpose: PaymentPurpose;
  referenceId: string; // booking id or bill id
  amount: Paise;
  currency: string;
  /** Provider handling this payment (from the restaurant's market). */
  provider: string;
  /** What the provider's client SDK needs: e.g. Razorpay order id + key, Stripe client secret. */
  checkout?: Record<string, string>;
  status: "CREATED" | "SUCCEEDED" | "FAILED";
  /** Set on success: the BillPayment id (purpose BILL) or the Booking id (purpose BOOKING). */
  resultRef?: string;
  failureReason?: string;
  /** The diner/owner who started the payment (only they, or an admin, can confirm it). */
  userId?: string;
}

export interface PaymentRef {
  intentId: string;
  method: PaymentMethod;
  amount: Paise;
  currency: string;
  provider: string;
  paidAt: ISODateTime;
  /** Absent on older records = ONLINE. */
  settlement?: Settlement;
}

export type OfflinePaymentStatus = "PENDING_APPROVAL" | "APPROVED" | "REJECTED" | "EXPIRED";

/** A bill paid at the counter, waiting for (or after) the restaurant's confirmation. */
export interface OfflinePayment {
  id: string;
  billId: string;
  intentId: string;
  restaurant: Pick<RestaurantSummary, "id" | "slug" | "name" | "area" | "art">;
  userId: string;
  dinerName: string;
  method: OfflineMethod;
  amount: Paise;
  currency: string;
  /** Feast deal included in `amount`. */
  savings: Paise;
  bookingId?: string;
  status: OfflinePaymentStatus;
  submittedAt: ISODateTime;
  expiresAt: ISODateTime;
  decidedAt?: ISODateTime;
  decidedBy?: string;
  reason?: string;
  amountReceived?: Paise;
  billPaymentId?: string;
}

// ---------- Audit trail ----------

export type ActorKind = "USER" | "WEBHOOK" | "SYSTEM" | "ANONYMOUS";

export interface AuditActor {
  kind: ActorKind;
  userId?: string;
  role?: Role;
  name?: string;
  ip?: string;
  requestId?: string;
  /** Job or provider for SYSTEM/WEBHOOK actors, e.g. "sweeper", "stripe". */
  source?: string;
}

/** One immutable record of a state change. */
export interface AuditRecord {
  id: string;
  at: ISODateTime;
  restaurantId?: string;
  entityType: string;
  entityId: string;
  /** e.g. "booking.cancelled", "payment.offline_approved". */
  action: string;
  actor: AuditActor;
  subjectUserId?: string;
  fromStatus?: string;
  toStatus?: string;
  amount?: Paise;
  currency?: string;
  note?: string;
  eventId?: string;
}

export interface AuditPage {
  items: AuditRecord[];
  nextCursor?: string;
}

export interface AuditFilter {
  restaurantId?: string;
  entityType?: string;
  entityId?: string;
  actorId?: string;
  userId?: string;
  from?: string;
  to?: string;
  cursor?: string;
  limit?: number;
}

export interface BillPayment {
  id: string;
  userId: string;
  bill: Bill;
  restaurant: Pick<RestaurantSummary, "id" | "slug" | "name" | "area" | "art">;
  payment: PaymentRef;
}

// ---------- Users, roles & auth ----------

/** Social sign-in (OAuth 2.0 / OpenID Connect). */
export type AuthProvider = "GOOGLE" | "APPLE" | "FACEBOOK";

export interface AuthProviderInfo {
  provider: AuthProvider;
  /** OAuth client id / Facebook app id for the web SDK (absent in demo mode). */
  clientId?: string;
  /** Apple JS redirect URI. */
  redirectUri?: string;
  /** Demo mode: no real provider; show a fake account picker. */
  demo: boolean;
}

/** Google/Apple send an OIDC `idToken`; Facebook an `accessToken`. Apple shares the name once, so pass it. */
export interface OAuthLoginRequest {
  idToken?: string;
  accessToken?: string;
  name?: string;
  nonce?: string;
}

/**
 * CUSTOMER: diners. OWNER: restaurant partners, scoped to `restaurantIds`.
 * ADMIN: platform operators — can do everything an owner can, for every restaurant.
 */
export type Role = "CUSTOMER" | "OWNER" | "ADMIN";

export interface User {
  id: string;
  name: string;
  phone?: string;
  email?: string;
  role: Role;
  restaurantIds: string[]; // OWNER only
  status: "ACTIVE" | "BLOCKED";
  createdAt: ISODateTime;
}

export interface AuthSession {
  token: string;
  user: User;
}

// ---------- Partner (owner) portal ----------

export interface PartnerDashboard {
  restaurant: RestaurantDetail;
  date: ISODate;
  bookingsToday: number;
  coversToday: number;
  seatedNow: number;
  /** Diners who tapped "I'm here" today. */
  arrivedToday: number;
  /** Counter payments waiting for the restaurant to confirm. */
  pendingCounterPayments: number;
  revenueToday: Paise; // paid through Feast
  savingsGivenToday: Paise;
  last7Days: { date: ISODate; bookings: number; revenue: Paise }[];
}

export interface MenuItemPatch {
  available?: boolean;
  price?: Paise;
}

// ---------- Admin console ----------

// ---------- Onboarding & compliance documents ----------

export type DocumentStatus = "SUBMITTED" | "APPROVED" | "REJECTED";

export interface ComplianceDocument {
  id: string;
  restaurantId: string;
  requirementId: string;
  fileName: string;
  fileType: string;
  fileSize: number;
  number?: string;
  /** YYYY-MM-DD. */
  expiresOn?: ISODate;
  status: DocumentStatus;
  reviewNote?: string;
  uploadedAt: ISODateTime;
  uploadedBy: string;
  reviewedAt?: ISODateTime;
  reviewedBy?: string;
}

/** Per requirement: MISSING → SUBMITTED → APPROVED/REJECTED; APPROVED can become EXPIRED. */
export type RequirementState = "MISSING" | "SUBMITTED" | "APPROVED" | "REJECTED" | "EXPIRED";

export interface ComplianceItem {
  requirement: DocumentRequirement;
  /** Latest upload for this requirement. */
  document?: ComplianceDocument;
  state: RequirementState;
  /** Approved but expiring within 30 days. */
  expiringSoon?: boolean;
}

export interface ComplianceStatus {
  restaurantId: string;
  restaurantStatus: RestaurantStatus;
  countryCode: string;
  items: ComplianceItem[];
  /** Every required document uploaded (owner can submit for review). */
  readyToSubmit: boolean;
  /** Every required document approved and valid (admin can approve; listing allowed). */
  complete: boolean;
  /** Admin's notes when sending the application back. */
  reviewNotes?: string;
  submittedAt?: ISODateTime;
}

export interface UploadDocumentRequest {
  requirementId: string;
  file: File;
  number?: string;
  expiresOn?: ISODate;
}

export interface PartnerSignupRequest {
  owner: { name: string; email: string; phone?: string; password: string };
  restaurant: { name: string; cityId: string; area: string; cuisines: string[]; costForTwo: Paise; isPureVeg: boolean; about?: string };
}

export interface ApprovalQueueItem {
  restaurant: Pick<RestaurantDetail, "id" | "slug" | "name" | "area" | "cityId" | "countryCode" | "status">;
  owners: { id: string; name: string; email?: string }[];
  compliance: ComplianceStatus;
}

// ---------- Restaurant subscriptions (platform billing) ----------

/**
 * Restaurants list on Feast free for a trial period from when they first go
 * live, then pay a subscription. Unpaid listings get a grace period, then are
 * hidden from diners until paid. Rules: src/lib/api/subscription.ts.
 */
export type SubscriptionStatus = "NOT_STARTED" | "TRIAL" | "ACTIVE" | "PAST_DUE" | "EXPIRED";
export type PlanId = "MONTHLY" | "QUARTERLY" | "YEARLY";

export interface SubscriptionPlan {
  id: PlanId;
  name: string;
  months: number;
  currency: string;
  /** Price before tax. */
  price: Paise;
  taxName: string;
  taxPercent: number;
  /** Price including tax. */
  total: Paise;
  /** Saving vs paying monthly, in percent. */
  savePercent?: number;
}

export interface Subscription {
  restaurantId: string;
  status: SubscriptionStatus;
  /** Last plan paid for. */
  planId?: PlanId;
  trialEndsAt?: ISODateTime;
  /** End of the paid period, if ever paid. */
  paidThrough?: ISODateTime;
  /** When the current trial/paid period ends. */
  endsAt?: ISODateTime;
  /** When the listing is hidden if still unpaid (endsAt + grace). */
  hiddenAfter?: ISODateTime;
  /** Whole days until `endsAt` (negative once overdue). */
  daysLeft?: number;
  /** Visible to diners right now (besides the restaurant being LIVE/PAUSED). */
  listed: boolean;
}

/** A payment from a restaurant to Feast (or an admin-granted extension). */
export interface PlatformPayment {
  id: string;
  invoiceNumber: string;
  restaurant: Pick<RestaurantSummary, "id" | "slug" | "name" | "area" | "art">;
  /** COMP = free days granted by an admin (no charge). */
  planId: PlanId | "COMP";
  periodStart: ISODateTime;
  periodEnd: ISODateTime;
  currency: string;
  amount: Paise;
  /** Tax on the subscription (e.g. GST 18%). */
  tax: Paise;
  taxName: string;
  total: Paise;
  method?: PaymentMethod;
  provider?: string;
  status: "PAID" | "FAILED";
  failureReason?: string;
  note?: string;
  paidBy: string;
  createdAt: ISODateTime;
}

export interface SubscriptionOverview {
  subscription: Subscription;
  plans: SubscriptionPlan[];
  payments: PlatformPayment[];
  rules: { trialDays: number; graceDays: number };
  /** Methods the market's payment provider offers. */
  methods: PaymentMethod[];
}

export interface PaySubscriptionResult {
  subscription: Subscription;
  payment: PlatformPayment;
}

export interface AdminSubscriptionRow {
  restaurant: Pick<RestaurantDetail, "id" | "slug" | "name" | "area" | "cityId" | "status" | "currency" | "countryCode">;
  owners: { id: string; name: string }[];
  subscription: Subscription;
  totalPaid: Paise;
  lastPaymentAt?: ISODateTime;
}

export interface AdminMetrics {
  restaurants: Record<RestaurantStatus, number>;
  users: Record<Role, number>;
  bookings: { total: number; upcoming: number; cancelled: number };
  /** Money is never summed across currencies. */
  payments: { count: number; byCurrency: { currency: string; gmv: Paise; savings: Paise }[] };
  subscriptions: {
    trial: number;
    active: number;
    pastDue: number;
    expired: number;
    /** Trials ending within 7 days. */
    trialsEndingSoon: number;
    /** Per currency: collected this month and in total (incl. tax), and MRR (before tax). */
    revenue: { currency: string; thisMonth: Paise; total: Paise; mrr: Paise }[];
  };
  byCity: { cityId: string; cityName: string; currency: string; restaurants: number; bookings: number; gmv: Paise }[];
}

export interface CreateRestaurantRequest {
  name: string;
  cityId: string;
  area: string;
  cuisines: string[];
  costForTwo: Paise;
  isPureVeg: boolean;
  tags: string[];
  about: string;
  ownerUserId?: string;
}

export type RestaurantPatch = Partial<
  Pick<RestaurantDetail, "name" | "area" | "cuisines" | "costForTwo" | "isPureVeg" | "tags" | "about" | "address" | "phone">
>;

export interface UserPatch {
  role?: Role;
  restaurantIds?: string[];
  status?: User["status"];
  name?: string;
}

// ---------- Errors ----------

export class ApiError extends Error {
  constructor(
    public code: string,
    message: string,
    public status = 400,
  ) {
    super(message);
  }
}

// ---------- Notifications ----------

export type NotificationKind =
  | "BOOKING_CREATED"
  | "BOOKING_CANCELLED"
  | "BOOKING_UPDATED"
  | "PAYMENT_RECEIVED"
  | "SUBSCRIPTION_PAID"
  | "SUBSCRIPTION_EXPIRING"
  | "APPLICATION_SUBMITTED"
  | "DOCUMENT_REVIEWED"
  | "RESTAURANT_APPROVED"
  | "DOCUMENT_EXPIRING"
  | "CASE_UPDATED"
  | "REVIEW_UPDATED"
  | "CONTENT_MODERATED";

export interface Notification {
  id: string;
  userId: string;
  restaurantId?: string;
  kind: NotificationKind;
  title: string;
  body: string;
  /** App route to open, e.g. "/partner/bookings". */
  link?: string;
  eventId: string;
  createdAt: ISODateTime;
  readAt?: ISODateTime;
}

export interface NotificationList {
  items: Notification[];
  unread: number;
}

// ---------- Named request/response shapes (match the Rust contract) ----------

export type PaymentPurpose = "BOOKING" | "BILL" | "SUBSCRIPTION";

export interface CreateBookingResponse {
  booking: Booking;
  /** Present when a cover charge must be paid to confirm the booking. */
  paymentIntent?: PaymentIntent;
}

/** Absent = unchanged; `null` clears the cap/limit. */
export type RestaurantSettingsPatch = Partial<Omit<RestaurantSettings, "maxDiscountPerBill" | "dailyDealLimit">> & {
  maxDiscountPerBill?: Paise | null;
  dailyDealLimit?: number | null;
};

export interface CreateUserRequest {
  name: string;
  email?: string;
  phone?: string;
  role: Role;
  restaurantIds?: string[];
}

// ---------- Trust & safety: moderation, support access, grievances, reviews ----------
// Mirrors backend/rust/crates/domain/src/trust.rs.

/** Small restaurant reference embedded in bookings, payments, cases and reviews. */
export type RestaurantRef = Pick<RestaurantSummary, "id" | "slug" | "name" | "area" | "art">;

/** Set on a menu item / offer / review Feast has hidden. */
export interface Moderation {
  reason: string;
  hiddenAt: ISODateTime;
  hiddenBy: string;
  /** The owner says it's fixed and asked Feast to look again. */
  reviewRequestedAt?: ISODateTime;
  reviewNote?: string;
}

export type ContentKind = "MENU_ITEM" | "OFFER";
export type ModerationAction = "HIDE" | "RESTORE";

export interface ModerateContentRequest {
  kind: ContentKind;
  id: string;
  action: ModerationAction;
  /** Required to hide (10+ characters). */
  reason?: string;
}

export interface ModerationAppealRequest {
  kind: ContentKind;
  id: string;
  note?: string;
}

export interface ModeratedContent {
  restaurant: RestaurantRef;
  kind: ContentKind;
  id: string;
  title: string;
  moderation: Moderation;
}

/** Owner grants (1–72 h) or revokes (0) Feast support access. */
export interface SupportAccessRequest {
  hours: number;
}

export interface SupportAccess {
  /** Feast staff may edit until then (absent = no access). */
  activeUntil?: ISODateTime;
  /** Whether Feast staff may edit right now (access granted, or no owner linked yet). */
  staffCanEdit: boolean;
  /** Newest first. */
  audit: AuditEntry[];
}

export interface AuditEntry {
  id: string;
  restaurantId: string;
  actorId: string;
  actorName: string;
  actorRole: Role;
  action: string;
  summary: string;
  at: ISODateTime;
}

export type CaseKind = "COMPLAINT" | "REPORT";
export type CaseStatus = "AWAITING_RESTAURANT" | "AWAITING_CUSTOMER" | "WITH_FEAST" | "RESOLVED";
export type CaseReason =
  | "TABLE_NOT_HONOURED"
  | "OFFER_NOT_HONOURED"
  | "WRONG_BILL"
  | "FOOD_QUALITY"
  | "HYGIENE"
  | "STAFF_BEHAVIOUR"
  | "NO_SHOW"
  | "ABUSIVE_BEHAVIOUR"
  | "PAYMENT_DISPUTE"
  | "PROPERTY_DAMAGE"
  | "OTHER";
export type CaseRole = "CUSTOMER" | "RESTAURANT" | "FEAST";
export type CaseDecision =
  | "SETTLED_BETWEEN_PARTIES"
  | "NO_ACTION"
  | "REFUND_ADVISED"
  | "RESTAURANT_WARNED"
  | "RESTAURANT_SUSPENDED"
  | "CUSTOMER_WARNED"
  | "CUSTOMER_BLOCKED";

export interface CaseMessage {
  id: string;
  author: CaseRole;
  authorName: string;
  text: string;
  at: ISODateTime;
  /** Feast-only note (never sent to the diner or the restaurant). */
  internal: boolean;
  /** Written by the system (escalation, auto-close). */
  system: boolean;
}

export interface CaseAttachment {
  id: string;
  fileName: string;
  contentType: string;
  size: number;
  uploadedBy: CaseRole;
  at: ISODateTime;
}

export interface CaseOutcome {
  decision: CaseDecision;
  note: string;
  refundAmount?: Paise;
  decidedBy: CaseRole;
  at: ISODateTime;
}

/** A grievance between a diner and a restaurant about one visit. */
export interface Case {
  id: string;
  /** e.g. FC-7KQ2M9 */
  number: string;
  kind: CaseKind;
  status: CaseStatus;
  reason: CaseReason;
  restaurant: RestaurantRef;
  customer: { id: string; name: string };
  bookingId?: string;
  billPaymentId?: string;
  visitDate: ISODate;
  amount?: Paise;
  currency: string;
  messages: CaseMessage[];
  attachments: CaseAttachment[];
  outcome?: CaseOutcome;
  /** Deadline for whoever's turn it is (Feast's target when WITH_FEAST). */
  dueAt?: ISODateTime;
  escalatedAt?: ISODateTime;
  resolvedAt?: ISODateTime;
  createdAt: ISODateTime;
  updatedAt: ISODateTime;
  version: number;
}

/** Exactly one of bookingId / billPaymentId. */
export interface OpenCaseRequest {
  bookingId?: string;
  billPaymentId?: string;
  reason: CaseReason;
  text: string;
}

export interface CaseMessageRequest {
  text: string;
  /** Admin only. */
  internal?: boolean;
}

export interface ResolveCaseRequest {
  decision: CaseDecision;
  note: string;
  refundAmount?: Paise;
}

export interface AdminCaseItem {
  case: Case;
  overdue: boolean;
  restaurantComplaints: number;
  customerReports: number;
}

export type ReviewStatus = "PUBLISHED" | "HIDDEN";

export interface Review {
  id: string;
  restaurant: RestaurantRef;
  /** Absent in public lists. */
  userId?: string;
  /** "Kavya R." */
  authorName: string;
  bookingId?: string;
  billPaymentId?: string;
  visitDate: ISODate;
  rating: number;
  text: string;
  createdAt: ISODateTime;
  reply?: { text: string; at: ISODateTime };
  status: ReviewStatus;
  moderation?: Moderation;
  /** The restaurant reported it to Feast. */
  flag?: { reason: string; at: ISODateTime };
}

export interface ReviewSummary {
  average: number;
  count: number;
  /** index 0 = 1★ … 4 = 5★ (Feast reviews only). */
  distribution: number[];
}

export interface ReviewList {
  summary: ReviewSummary;
  reviews: Review[];
}

/** Exactly one of bookingId / billPaymentId. */
export interface CreateReviewRequest {
  bookingId?: string;
  billPaymentId?: string;
  rating: number;
  text: string;
}

export interface ReviewReplyRequest {
  text: string;
}

export interface ReportReviewRequest {
  reason: string;
}

export interface ModerateReviewRequest {
  action: ModerationAction;
  reason?: string;
}

// ---------- Insights (owner scorecard, diner savings, admin marketplace health) ----------

/** Periods offered: owner and admin 7 | 30 | 90 days; diner 30 | 90 | 365 | 0 (all time). */
export interface InsightsPeriod {
  days: number;
  /** First local date included (absent for all time). */
  from?: ISODate;
  /** Last date included (today). */
  to: ISODate;
}

export interface BookingFunnel {
  /** Bookings for slots in the period (not cancelled), up to today. */
  booked: number;
  /** Diners who came (seated or completed). */
  honoured: number;
  noShow: number;
  cancelled: number;
  /** Confirmed bookings still to come. */
  upcoming: number;
}

export interface DealUsage {
  dealsUsed: number;
  discountsGiven: Paise;
  dailyDealLimit?: number;
  /** Deals available over the period under the daily limit. */
  dealsAvailable?: number;
  /** dealsUsed ÷ dealsAvailable (0–1). */
  budgetUsed?: number;
  maxDiscountPerBill?: Paise;
}

export interface OwnerInsights {
  restaurant: RestaurantRef;
  currency: string;
  period: InsightsPeriod;
  /** Distinct diners who visited through Feast (kept booking or bill paid on Feast). */
  feastDiners: number;
  newDiners: number;
  repeatDiners: number;
  visits: number;
  covers: number;
  bookings: BookingFunnel;
  noShowRate?: number;
  /** Bills paid on Feast. */
  revenue: Paise;
  billsPaid: number;
  averageBill?: Paise;
  /** Bills Feast diners paid at your counter (settled offline, not through Feast). */
  offlineRevenue?: Paise;
  offlineBills?: number;
  coverCharges: Paise;
  deals: DealUsage;
  /** Subscription cost for the period (invoices spread over the days they cover). */
  feastCost: Paise;
  /** revenue ÷ feastCost. */
  returnOnCost?: number;
  byWeekday: { day: string; bookings: number; covers: number; revenue: Paise }[];
  byHour: { hour: number; bookings: number; covers: number }[];
  rating: number;
  ratingCount: number;
  /** Last 6 months of Feast reviews, oldest first. */
  ratingTrend: { month: string; reviews: number; average: number }[];
  complaintsOpened: number;
  openCases: number;
}

export interface DinerInsights {
  currency: string;
  period: InsightsPeriod;
  totalSaved: Paise;
  /** Bills plus upfront cover charges paid through Feast. */
  totalSpent: Paise;
  billsPaid: number;
  visits: number;
  restaurantsVisited: number;
  bookings: BookingFunnel;
  reviewsWritten: number;
  favouriteRestaurants: { restaurant: RestaurantRef; visits: number; spent: Paise; saved: Paise }[];
  favouriteCuisines: { name: string; count: number }[];
  /** Oldest first. */
  byMonth: { month: string; spent: Paise; saved: Paise }[];
}

export interface AdminInsights {
  /** UTC dates for the marketplace as a whole. */
  period: InsightsPeriod;
  restaurants: { live: number; pendingApproval: number; onboarding: number; pausedOrSuspended: number };
  subscriptions: AdminMetrics["subscriptions"];
  /** Never summed across currencies. */
  money: {
    currency: string;
    gmv: Paise;
    billsPaid: number;
    averageBill?: Paise;
    discounts: Paise;
    coverCharges: Paise;
    /** Feast's own revenue in the period: subscription invoices paid (before tax). */
    subscriptionRevenue: Paise;
    mrr: Paise;
    /** Bills paid at restaurants' counters: settled offline, not through Feast (not in gmv). */
    offlineSettled?: Paise;
    offlineBills?: number;
  }[];
  bookings: BookingFunnel;
  noShowRate?: number;
  visits: number;
  /** Distinct diners with a visit in the period. */
  activeDiners: number;
  /** Diner accounts created in the period. */
  newDiners: number;
  disputes: { opened: number; per100Visits?: number; decidedByFeast: number; decidedOnTime?: number; withFeastNow: number; openNow: number };
  reviews: { posted: number; average?: number; hidden: number };
  cities: { cityId: string; cityName: string; currency: string; liveRestaurants: number; bookings: number; gmv: Paise }[];
  topRestaurants: { restaurant: RestaurantRef; cityName: string; currency: string; bookings: number; gmv: Paise; rating: number }[];
}
