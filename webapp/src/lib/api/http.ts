import type { FeastApi } from "./client";
import { getToken } from "./token";
import {
  ApiError,
  type AuditFilter,
  type Refund,
  type RefundRequest,
  type AuditPage,
  type OfflineMethod,
  type OfflinePayment,
  type OfflinePaymentStatus,
  type Paise,
  type AdminInsights,
  type AdminMetrics,
  type DinerInsights,
  type OwnerInsights,
  type AuthSession,
  type DeleteAccountRequest,
  type DeleteAccountResult,
  type PushSettings,
  type PushToken,
  type PushTokenRequest,
  type Bill,
  type BillPayment,
  type Booking,
  type BookingStatus,
  type City,
  type CreateBookingRequest,
  type CreateRestaurantRequest,
  type HomeResponse,
  type AuthProvider,
  type AuthProviderInfo,
  type OAuthLoginRequest,
  type ApprovalQueueItem,
  type ComplianceStatus,
  type CreateCityRequest,
  type DocumentStatus,
  type GeoResult,
  type Market,
  type MarketInput,
  type PartnerSignupRequest,
  type PaymentProviderInfo,
  type UploadDocumentRequest,
  type AdminSubscriptionRow,
  type PaySubscriptionResult,
  type PlanId,
  type PlatformPayment,
  type SubscriptionOverview,
  type ListingPatch,
  type NotificationList,
  type Menu,
  type MenuItem,
  type MenuItemPatch,
  type Offer,
  type PartnerDashboard,
  type PaymentIntent,
  type PaymentMethod,
  type QuoteBillRequest,
  type RestaurantDetail,
  type RestaurantQr,
  type RestaurantPatch,
  type RestaurantSettings,
  type RestaurantSettingsPatch,
  type DealBudget,
  type RestaurantStatus,
  type Role,
  type SearchQuery,
  type SearchResponse,
  type SlotAvailability,
  type User,
  type UserPatch,
  type AdminCaseItem,
  type Case,
  type CaseMessageRequest,
  type CreateReviewRequest,
  type ModeratedContent,
  type ModerateContentRequest,
  type ModerateReviewRequest,
  type ModerationAppealRequest,
  type OpenCaseRequest,
  type ResolveCaseRequest,
  type Review,
  type ReviewList,
  type SupportAccess,
} from "./types";

/** Real backend client for the Rust (or any conforming) backend. Routes: docs/openapi.yaml. */
/** `?a=1&b=2` from the set filter fields (or ""). */
function auditQuery(f: AuditFilter): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(f)) if (v !== undefined && v !== "") q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : "";
}

export class HttpFeastApi implements FeastApi {
  constructor(private baseUrl: string) {}

  private authHeaders(): Record<string, string> {
    const token = getToken();
    return token ? { Authorization: `Bearer ${token}` } : {};
  }

  private async req<T>(method: string, path: string, body?: unknown, query?: Record<string, unknown>): Promise<T> {
    const url = new URL(path, this.baseUrl);
    for (const [k, v] of Object.entries(query ?? {})) {
      if (v !== undefined && v !== null && v !== "") url.searchParams.set(k, String(v));
    }
    const headers: Record<string, string> = this.authHeaders();
    const isForm = typeof FormData !== "undefined" && body instanceof FormData;
    if (body !== undefined && !isForm) headers["Content-Type"] = "application/json";
    let res: Response;
    try {
      res = await fetch(url, { method, headers, body: body === undefined ? undefined : isForm ? (body as FormData) : JSON.stringify(body) });
    } catch {
      throw new ApiError("NETWORK", "Can't reach Feast. Check your connection and try again.", 0);
    }
    if (!res.ok) {
      const err = (await res.json().catch(() => ({}))) as { code?: string; message?: string };
      throw new ApiError(err.code ?? "HTTP_ERROR", err.message ?? res.statusText, res.status);
    }
    return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
  }

  // Auth
  requestOtp(phone: string) {
    return this.req<{ hint?: string }>("POST", "/v1/auth/otp", { phone });
  }
  verifyOtp(phone: string, otp: string, name?: string) {
    return this.req<AuthSession>("POST", "/v1/auth/otp/verify", { phone, otp, name });
  }
  login(email: string, password: string) {
    return this.req<AuthSession>("POST", "/v1/auth/login", { email, password });
  }
  authProviders() {
    return this.req<AuthProviderInfo[]>("GET", "/v1/auth/providers");
  }
  oauthLogin(provider: AuthProvider, req: OAuthLoginRequest) {
    return this.req<AuthSession>("POST", `/v1/auth/oauth/${provider.toLowerCase()}`, req);
  }
  async me() {
    if (!getToken()) return null; // signed out: nothing to ask the server
    try {
      return await this.req<User>("GET", "/v1/me");
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) return null;
      throw e;
    }
  }
  listNotifications(unreadOnly = false, limit = 30) {
    return this.req<NotificationList>("GET", "/v1/me/notifications", undefined, { unreadOnly, limit });
  }
  markNotificationsRead(ids?: string[]) {
    return this.req<{ updated: number; unread: number }>("POST", "/v1/me/notifications/read", { ids: ids ?? [] });
  }
  logout() {
    return this.req<void>("POST", "/v1/auth/logout");
  }
  deleteAccount(req: DeleteAccountRequest) {
    return this.req<DeleteAccountResult>("DELETE", "/v1/me", req);
  }
  registerPushToken(req: PushTokenRequest) {
    return this.req<PushToken>("POST", "/v1/me/push-tokens", req);
  }
  unregisterPushToken(token: string) {
    return this.req<void>("DELETE", `/v1/me/push-tokens/${encodeURIComponent(token)}`);
  }
  getPushSettings() {
    return this.req<PushSettings>("GET", "/v1/me/push-settings");
  }
  setPushSettings(s: PushSettings) {
    return this.req<PushSettings>("PUT", "/v1/me/push-settings", s);
  }

  // Discovery
  listCities() {
    return this.req<City[]>("GET", "/v1/cities");
  }
  locate() {
    return this.req<GeoResult>("GET", "/v1/geo");
  }
  listMarkets() {
    return this.req<Market[]>("GET", "/v1/markets");
  }
  getHome(cityId: string) {
    return this.req<HomeResponse>("GET", "/v1/home", undefined, { city: cityId });
  }
  search(cityId: string, q: SearchQuery) {
    return this.req<SearchResponse>("GET", "/v1/restaurants", undefined, { city: cityId, ...q });
  }
  getRestaurant(slug: string) {
    return this.req<RestaurantDetail>("GET", `/v1/restaurants/${encodeURIComponent(slug)}`);
  }
  getMenu(restaurantId: string) {
    return this.req<Menu>("GET", `/v1/restaurants/${restaurantId}/menu`);
  }

  // Booking
  getSlots(restaurantId: string, date: string, guests: number) {
    return this.req<SlotAvailability>("GET", `/v1/restaurants/${restaurantId}/slots`, undefined, { date, guests });
  }
  createBooking(req: CreateBookingRequest) {
    return this.req<{ booking: Booking; paymentIntent?: PaymentIntent }>("POST", "/v1/bookings", req);
  }
  getBooking(id: string) {
    return this.req<Booking>("GET", `/v1/bookings/${id}`);
  }
  listBookings() {
    return this.req<Booking[]>("GET", "/v1/me/bookings");
  }
  cancelBooking(id: string) {
    return this.req<Booking>("POST", `/v1/bookings/${id}/cancel`);
  }
  checkInBooking(id: string) {
    return this.req<Booking>("POST", `/v1/bookings/${id}/check-in`);
  }

  // The restaurant's printed QR
  getRestaurantQr(code: string) {
    return this.req<RestaurantQr>("GET", `/v1/qr/${encodeURIComponent(code)}`);
  }

  // Bills & payments
  quoteBill(req: QuoteBillRequest) {
    return this.req<Bill>("POST", "/v1/bills/quote", req);
  }
  createPaymentIntent(purpose: PaymentIntent["purpose"], referenceId: string) {
    return this.req<PaymentIntent>("POST", "/v1/payments/intents", { purpose, referenceId });
  }
  confirmPayment(intentId: string, method: PaymentMethod, simulate?: "success" | "failure", providerPayload?: Record<string, string>) {
    return this.req<PaymentIntent>("POST", `/v1/payments/intents/${intentId}/confirm`, { method, simulate, providerPayload });
  }
  paymentRefunds(intentId: string) {
    return this.req<Refund[]>("GET", `/v1/payments/intents/${intentId}/refunds`);
  }
  getBillPayment(id: string) {
    return this.req<BillPayment>("GET", `/v1/bill-payments/${id}`);
  }
  listBillPayments() {
    return this.req<BillPayment[]>("GET", "/v1/me/bill-payments");
  }
  billPaymentHistory(id: string) {
    return this.req<AuditPage>("GET", `/v1/bill-payments/${id}/history`);
  }
  submitOfflinePayment(billId: string, method: OfflineMethod) {
    return this.req<OfflinePayment>("POST", `/v1/bills/${billId}/offline-payment`, { method });
  }
  getOfflinePayment(billId: string) {
    return this.req<OfflinePayment | null>("GET", `/v1/bills/${billId}/offline-payment`);
  }
  bookingHistory(bookingId: string) {
    return this.req<AuditPage>("GET", `/v1/bookings/${bookingId}/history`);
  }

  // Partner
  partnerRestaurants() {
    return this.req<RestaurantDetail[]>("GET", "/v1/partner/restaurants");
  }
  partnerDashboard(id: string) {
    return this.req<PartnerDashboard>("GET", `/v1/partner/restaurants/${id}/dashboard`);
  }
  partnerInsights(id: string, days: 7 | 30 | 90) {
    return this.req<OwnerInsights>("GET", `/v1/partner/restaurants/${id}/insights`, undefined, { days });
  }
  partnerBookings(id: string, date: string) {
    return this.req<Booking[]>("GET", `/v1/partner/restaurants/${id}/bookings`, undefined, { date });
  }
  updateBookingStatus(bookingId: string, status: Extract<BookingStatus, "SEATED" | "NO_SHOW" | "COMPLETED" | "CANCELLED">) {
    return this.req<Booking>("POST", `/v1/partner/bookings/${bookingId}/status`, { status });
  }
  partnerQr(id: string) {
    return this.req<RestaurantQr>("GET", `/v1/partner/restaurants/${id}/qr`);
  }
  partnerPayments(id: string) {
    return this.req<BillPayment[]>("GET", `/v1/partner/restaurants/${id}/payments`);
  }
  partnerOfflinePayments(restaurantId: string, status?: OfflinePaymentStatus) {
    return this.req<OfflinePayment[]>("GET", `/v1/partner/restaurants/${restaurantId}/offline-payments${status ? `?status=${status}` : ""}`);
  }
  approveOfflinePayment(id: string, amountReceived?: Paise) {
    return this.req<OfflinePayment>("POST", `/v1/partner/offline-payments/${id}/approve`, { amountReceived });
  }
  rejectOfflinePayment(id: string, reason: string) {
    return this.req<OfflinePayment>("POST", `/v1/partner/offline-payments/${id}/reject`, { reason });
  }
  restaurantAudit(restaurantId: string, filter: AuditFilter = {}) {
    return this.req<AuditPage>("GET", `/v1/partner/restaurants/${restaurantId}/audit${auditQuery(filter)}`);
  }
  adminAudit(filter: AuditFilter = {}) {
    return this.req<AuditPage>("GET", `/v1/admin/audit${auditQuery(filter)}`);
  }
  partnerMenu(id: string) {
    return this.req<Menu>("GET", `/v1/partner/restaurants/${id}/menu`);
  }
  updateMenuItem(id: string, itemId: string, patch: MenuItemPatch) {
    return this.req<MenuItem>("PATCH", `/v1/partner/restaurants/${id}/menu/items/${itemId}`, patch);
  }
  uploadMenuPhoto(id: string, itemId: string, file: File) {
    const form = new FormData();
    form.set("file", file);
    return this.req<MenuItem>("POST", `/v1/partner/restaurants/${id}/menu/items/${itemId}/photo`, form);
  }
  removeMenuPhoto(id: string, itemId: string) {
    return this.req<MenuItem>("DELETE", `/v1/partner/restaurants/${id}/menu/items/${itemId}/photo`);
  }
  findMenuPhoto(id: string, itemId: string) {
    return this.req<MenuItem>("POST", `/v1/partner/restaurants/${id}/menu/items/${itemId}/photo/find`);
  }
  updateOffers(id: string, offers: Offer[]) {
    return this.req<Offer[]>("PUT", `/v1/partner/restaurants/${id}/offers`, offers);
  }
  updateSettings(id: string, settings: RestaurantSettingsPatch) {
    return this.req<RestaurantSettings>("PATCH", `/v1/partner/restaurants/${id}/settings`, settings);
  }
  dealBudget(id: string) {
    return this.req<DealBudget>("GET", `/v1/partner/restaurants/${id}/deal-budget`);
  }

  updateListing(id: string, patch: ListingPatch) {
    return this.req<RestaurantDetail>("PATCH", `/v1/partner/restaurants/${id}/listing`, patch);
  }
  partnerSignup(req: PartnerSignupRequest) {
    return this.req<AuthSession>("POST", "/v1/partner/signup", req);
  }
  compliance(id: string) {
    return this.req<ComplianceStatus>("GET", `/v1/partner/restaurants/${id}/compliance`);
  }
  uploadDocument(id: string, req: UploadDocumentRequest) {
    const form = new FormData();
    form.set("requirementId", req.requirementId);
    if (req.number) form.set("number", req.number);
    if (req.expiresOn) form.set("expiresOn", req.expiresOn);
    form.set("file", req.file);
    return this.req<ComplianceStatus>("POST", `/v1/partner/restaurants/${id}/documents`, form);
  }
  submitForReview(id: string) {
    return this.req<ComplianceStatus>("POST", `/v1/partner/restaurants/${id}/submit`);
  }
  async documentFileUrl(documentId: string) {
    // Files need the auth header, so fetch and hand back a blob URL.
    const res = await fetch(new URL(`/v1/documents/${documentId}/file`, this.baseUrl), { headers: this.authHeaders() });
    if (!res.ok) throw new ApiError("NOT_FOUND", "Couldn't open that document", res.status);
    return URL.createObjectURL(await res.blob());
  }
  partnerSubscription(id: string) {
    return this.req<SubscriptionOverview>("GET", `/v1/partner/restaurants/${id}/subscription`);
  }
  subscriptionCheckout(id: string, planId: PlanId) {
    return this.req<PaymentIntent>("POST", `/v1/partner/restaurants/${id}/subscription/checkout`, { planId });
  }

  // Admin
  adminMetrics() {
    return this.req<AdminMetrics>("GET", "/v1/admin/metrics");
  }
  adminInsights(days: 7 | 30 | 90) {
    return this.req<AdminInsights>("GET", "/v1/admin/insights", undefined, { days });
  }
  adminRestaurants() {
    return this.req<RestaurantDetail[]>("GET", "/v1/admin/restaurants");
  }
  createRestaurant(req: CreateRestaurantRequest) {
    return this.req<RestaurantDetail>("POST", "/v1/admin/restaurants", req);
  }
  updateRestaurant(id: string, patch: RestaurantPatch) {
    return this.req<RestaurantDetail>("PATCH", `/v1/admin/restaurants/${id}`, patch);
  }
  setRestaurantStatus(id: string, status: RestaurantStatus) {
    return this.req<RestaurantDetail>("POST", `/v1/admin/restaurants/${id}/status`, { status });
  }
  adminUsers() {
    return this.req<User[]>("GET", "/v1/admin/users");
  }
  createUser(input: { name: string; email?: string; phone?: string; role: Role; restaurantIds?: string[] }) {
    return this.req<User>("POST", "/v1/admin/users", input);
  }
  updateUser(userId: string, patch: UserPatch) {
    return this.req<User>("PATCH", `/v1/admin/users/${userId}`, patch);
  }
  adminBookings() {
    return this.req<Booking[]>("GET", "/v1/admin/bookings");
  }
  adminPayments() {
    return this.req<BillPayment[]>("GET", "/v1/admin/payments");
  }
  adminRefunds() {
    return this.req<Refund[]>("GET", "/v1/admin/refunds");
  }
  adminRefund(req: RefundRequest) {
    return this.req<Refund>("POST", "/v1/admin/refunds", req);
  }
  approvalQueue() {
    return this.req<ApprovalQueueItem[]>("GET", "/v1/admin/approvals");
  }
  reviewDocument(documentId: string, decision: Exclude<DocumentStatus, "SUBMITTED">, note?: string) {
    return this.req<ComplianceStatus>("POST", `/v1/admin/documents/${documentId}/review`, { decision, note });
  }
  requestChanges(id: string, note: string) {
    return this.req<ComplianceStatus>("POST", `/v1/admin/restaurants/${id}/request-changes`, { note });
  }
  paymentProviders() {
    return this.req<PaymentProviderInfo[]>("GET", "/v1/admin/payment-providers");
  }
  saveMarket(market: MarketInput) {
    return this.req<Market>("PUT", `/v1/admin/markets/${market.countryCode}`, market);
  }
  createCity(req: CreateCityRequest) {
    return this.req<City>("POST", "/v1/admin/cities", req);
  }
  adminSubscriptions() {
    return this.req<AdminSubscriptionRow[]>("GET", "/v1/admin/subscriptions");
  }
  platformPayments() {
    return this.req<PlatformPayment[]>("GET", "/v1/admin/platform-payments");
  }
  extendSubscription(id: string, days: number, note?: string) {
    return this.req<PaySubscriptionResult>("POST", `/v1/admin/restaurants/${id}/subscription/extend`, { days, note });
  }

  // Grievances
  openCase(req: OpenCaseRequest) {
    return this.req<Case>("POST", "/v1/cases", req);
  }
  myCases() {
    return this.req<Case[]>("GET", "/v1/me/cases");
  }
  restaurantCases(id: string) {
    return this.req<Case[]>("GET", `/v1/partner/restaurants/${id}/cases`);
  }
  getCase(id: string) {
    return this.req<Case>("GET", `/v1/cases/${id}`);
  }
  caseMessage(id: string, req: CaseMessageRequest) {
    return this.req<Case>("POST", `/v1/cases/${id}/messages`, req);
  }
  escalateCase(id: string) {
    return this.req<Case>("POST", `/v1/cases/${id}/escalate`);
  }
  settleCase(id: string) {
    return this.req<Case>("POST", `/v1/cases/${id}/settle`);
  }
  addCaseAttachment(id: string, file: File) {
    const form = new FormData();
    form.set("file", file);
    return this.req<Case>("POST", `/v1/cases/${id}/attachments`, form);
  }
  async caseAttachmentUrl(id: string, attachmentId: string) {
    const res = await fetch(new URL(`/v1/cases/${id}/attachments/${attachmentId}`, this.baseUrl), { headers: this.authHeaders() });
    if (!res.ok) throw new ApiError("NOT_FOUND", "Couldn't open that photo", res.status);
    return URL.createObjectURL(await res.blob());
  }
  adminCases() {
    return this.req<AdminCaseItem[]>("GET", "/v1/admin/cases");
  }
  resolveCase(id: string, req: ResolveCaseRequest) {
    return this.req<Case>("POST", `/v1/admin/cases/${id}/resolve`, req);
  }

  // Reviews
  restaurantReviews(slug: string) {
    return this.req<ReviewList>("GET", `/v1/restaurants/${slug}/reviews`);
  }
  createReview(req: CreateReviewRequest) {
    return this.req<Review>("POST", "/v1/reviews", req);
  }
  myReviews() {
    return this.req<Review[]>("GET", "/v1/me/reviews");
  }
  myInsights(days: 30 | 90 | 365 | 0) {
    return this.req<DinerInsights>("GET", "/v1/me/insights", undefined, { days });
  }
  partnerReviews(id: string) {
    return this.req<ReviewList>("GET", `/v1/partner/restaurants/${id}/reviews`);
  }
  replyReview(id: string, text: string) {
    return this.req<Review>("POST", `/v1/partner/reviews/${id}/reply`, { text });
  }
  reportReview(id: string, reason: string) {
    return this.req<Review>("POST", `/v1/partner/reviews/${id}/report`, { reason });
  }
  adminReviews() {
    return this.req<Review[]>("GET", "/v1/admin/reviews");
  }
  moderateReview(id: string, req: ModerateReviewRequest) {
    return this.req<Review>("POST", `/v1/admin/reviews/${id}/moderate`, req);
  }

  // Moderation & support access
  moderateContent(id: string, req: ModerateContentRequest) {
    return this.req<void>("POST", `/v1/admin/restaurants/${id}/moderation`, req);
  }
  moderationQueue() {
    return this.req<ModeratedContent[]>("GET", "/v1/admin/moderation");
  }
  appealModeration(id: string, req: ModerationAppealRequest) {
    return this.req<void>("POST", `/v1/partner/restaurants/${id}/moderation/appeal`, req);
  }
  supportAccess(id: string) {
    return this.req<SupportAccess>("GET", `/v1/partner/restaurants/${id}/support-access`);
  }
  setSupportAccess(id: string, hours: number) {
    return this.req<SupportAccess>("PUT", `/v1/partner/restaurants/${id}/support-access`, { hours });
  }
}
