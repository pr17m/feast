import type {
  AuditFilter,
  AuditPage,
  OfflineMethod,
  OfflinePayment,
  OfflinePaymentStatus,
  Paise,
  AdminInsights,
  AdminMetrics,
  DinerInsights,
  OwnerInsights,
  AuthSession,
  Bill,
  BillPayment,
  Booking,
  BookingStatus,
  City,
  CreateBookingRequest,
  CreateRestaurantRequest,
  HomeResponse,
  ISODate,
  Menu,
  MenuItem,
  ListingPatch,
  ApprovalQueueItem,
  AuthProvider,
  AuthProviderInfo,
  OAuthLoginRequest,
  GeoResult,
  ComplianceStatus,
  CreateCityRequest,
  DocumentStatus,
  Market,
  MarketInput,
  PartnerSignupRequest,
  PaymentProviderInfo,
  UploadDocumentRequest,
  AdminSubscriptionRow,
  PaySubscriptionResult,
  PlanId,
  PlatformPayment,
  SubscriptionOverview,
  MenuItemPatch,
  Offer,
  PartnerDashboard,
  PaymentIntent,
  PaymentMethod,
  QuoteBillRequest,
  RestaurantDetail,
  RestaurantQr,
  RestaurantPatch,
  RestaurantSettings,
  RestaurantStatus,
  SearchQuery,
  SearchResponse,
  SlotAvailability,
  User,
  UserPatch,
  NotificationList,
  CreateBookingResponse,
  CreateUserRequest,
  RestaurantSettingsPatch,
  DealBudget,
  AdminCaseItem,
  Case,
  CaseMessageRequest,
  CreateReviewRequest,
  ModeratedContent,
  ModerateContentRequest,
  ModerateReviewRequest,
  ModerationAppealRequest,
  OpenCaseRequest,
  ResolveCaseRequest,
  Review,
  ReviewList,
  SupportAccess,
} from "./types";

/**
 * Every call the UI makes goes through this interface. Each method maps 1:1 to a
 * REST endpoint of the Feast backend (comment on each line; contract in
 * backend/spec/openapi.yaml). Implemented by `HttpFeastApi`.
 *
 * Each country has its own deployment (web + API); `NEXT_PUBLIC_API_BASE_URL`
 * points this build at its country's API.
 *
 * Authorization is enforced by the backend, never only by the UI:
 *  - public:   discovery endpoints
 *  - customer: any signed-in user
 *  - owner:    OWNER of that restaurant, or ADMIN
 *  - admin:    ADMIN only
 */
export interface FeastApi {
  // ---- Auth ----
  requestOtp(phone: string): Promise<{ hint?: string }>; // POST /v1/auth/otp                       public
  verifyOtp(phone: string, otp: string, name?: string): Promise<AuthSession>; // POST /v1/auth/otp/verify   public (customers)
  login(email: string, password: string): Promise<AuthSession>; // POST /v1/auth/login                  public (owners, admins)
  authProviders(): Promise<AuthProviderInfo[]>; // GET  /v1/auth/providers                   public
  oauthLogin(provider: AuthProvider, req: OAuthLoginRequest): Promise<AuthSession>; // POST /v1/auth/oauth/{google|apple|facebook}   public
  me(): Promise<User | null>; // GET  /v1/me
  listNotifications(unreadOnly?: boolean, limit?: number): Promise<NotificationList>; // GET  /v1/me/notifications?unreadOnly=&limit=
  markNotificationsRead(ids?: string[]): Promise<{ updated: number; unread: number }>; // POST /v1/me/notifications/read   (no ids = all)
  logout(): Promise<void>; // POST /v1/auth/logout

  // ---- Discovery (public) ----
  listCities(): Promise<City[]>; // GET  /v1/cities
  locate(): Promise<GeoResult>; // GET  /v1/geo   (country from IP / CDN header)
  getHome(cityId: string): Promise<HomeResponse>; // GET  /v1/home?city=
  search(cityId: string, query: SearchQuery): Promise<SearchResponse>; // GET  /v1/restaurants?city=&q=&cuisine=&tag=&pureVeg=&minRating=&sort=
  getRestaurant(slug: string): Promise<RestaurantDetail>; // GET  /v1/restaurants/{slug}
  getMenu(restaurantId: string): Promise<Menu>; // GET  /v1/restaurants/{id}/menu

  // ---- Table booking (customer) ----
  getSlots(restaurantId: string, date: ISODate, guests: number): Promise<SlotAvailability>; // GET  /v1/restaurants/{id}/slots?date=&guests=   public
  createBooking(req: CreateBookingRequest): Promise<CreateBookingResponse>; // POST /v1/bookings
  getBooking(id: string): Promise<Booking>; // GET  /v1/bookings/{id}
  listBookings(): Promise<Booking[]>; // GET  /v1/me/bookings
  cancelBooking(id: string): Promise<Booking>; // POST /v1/bookings/{id}/cancel
  checkInBooking(id: string): Promise<Booking>; // POST /v1/bookings/{id}/check-in   "I'm here" (30 min before to 2 h after the slot)

  // ---- The restaurant's printed QR ----
  getRestaurantQr(code: string): Promise<RestaurantQr>; // GET  /v1/qr/{code}   public (also resolves old table codes like "oae1-t7")

  // ---- Pay bill & payments (customer) ----
  quoteBill(req: QuoteBillRequest): Promise<Bill>; // POST /v1/bills/quote
  createPaymentIntent(purpose: PaymentIntent["purpose"], referenceId: string): Promise<PaymentIntent>; // POST /v1/payments/intents
  confirmPayment(intentId: string, method: PaymentMethod, simulate?: "success" | "failure", providerPayload?: Record<string, string>): Promise<PaymentIntent>; // POST /v1/payments/intents/{id}/confirm
  getBillPayment(id: string): Promise<BillPayment>; // GET  /v1/bill-payments/{id}
  listBillPayments(): Promise<BillPayment[]>; // GET  /v1/me/bill-payments
  billPaymentHistory(id: string): Promise<AuditPage>; // GET  /v1/bill-payments/{id}/history
  submitOfflinePayment(billId: string, method: OfflineMethod): Promise<OfflinePayment>; // POST /v1/bills/{id}/offline-payment
  getOfflinePayment(billId: string): Promise<OfflinePayment | null>; // GET  /v1/bills/{id}/offline-payment
  bookingHistory(bookingId: string): Promise<AuditPage>; // GET  /v1/bookings/{id}/history

  // ---- Partner portal (owner of the restaurant, or admin) ----
  partnerRestaurants(): Promise<RestaurantDetail[]>; // GET   /v1/partner/restaurants
  partnerDashboard(restaurantId: string): Promise<PartnerDashboard>; // GET   /v1/partner/restaurants/{id}/dashboard
  partnerInsights(restaurantId: string, days: 7 | 30 | 90): Promise<OwnerInsights>; // GET   /v1/partner/restaurants/{id}/insights?days=
  partnerBookings(restaurantId: string, date: ISODate): Promise<Booking[]>; // GET   /v1/partner/restaurants/{id}/bookings?date=
  updateBookingStatus(bookingId: string, status: Extract<BookingStatus, "SEATED" | "NO_SHOW" | "COMPLETED" | "CANCELLED">): Promise<Booking>; // POST  /v1/partner/bookings/{id}/status
  partnerQr(restaurantId: string): Promise<RestaurantQr>; // GET   /v1/partner/restaurants/{id}/qr   the restaurant's printed QR code
  partnerPayments(restaurantId: string): Promise<BillPayment[]>; // GET   /v1/partner/restaurants/{id}/payments
  partnerOfflinePayments(restaurantId: string, status?: OfflinePaymentStatus): Promise<OfflinePayment[]>; // GET   /v1/partner/restaurants/{id}/offline-payments?status=
  approveOfflinePayment(id: string, amountReceived?: Paise): Promise<OfflinePayment>; // POST  /v1/partner/offline-payments/{id}/approve
  rejectOfflinePayment(id: string, reason: string): Promise<OfflinePayment>; // POST  /v1/partner/offline-payments/{id}/reject
  restaurantAudit(restaurantId: string, filter?: AuditFilter): Promise<AuditPage>; // GET   /v1/partner/restaurants/{id}/audit
  adminAudit(filter?: AuditFilter): Promise<AuditPage>; // GET   /v1/admin/audit
  partnerMenu(restaurantId: string): Promise<Menu>; // GET   /v1/partner/restaurants/{id}/menu   (includes unavailable items)
  updateMenuItem(restaurantId: string, itemId: string, patch: MenuItemPatch): Promise<MenuItem>; // PATCH /v1/partner/restaurants/{id}/menu/items/{itemId}
  uploadMenuPhoto(restaurantId: string, itemId: string, file: File): Promise<MenuItem>; // POST   /v1/partner/restaurants/{id}/menu/items/{itemId}/photo   multipart (JPG/PNG/WebP ≤ 5 MB)
  removeMenuPhoto(restaurantId: string, itemId: string): Promise<MenuItem>; // DELETE /v1/partner/restaurants/{id}/menu/items/{itemId}/photo
  findMenuPhoto(restaurantId: string, itemId: string): Promise<MenuItem>; // POST   /v1/partner/restaurants/{id}/menu/items/{itemId}/photo/find   (stock photo)
  updateOffers(restaurantId: string, offers: Offer[]): Promise<Offer[]>; // PUT   /v1/partner/restaurants/{id}/offers
  updateSettings(restaurantId: string, settings: RestaurantSettingsPatch): Promise<RestaurantSettings>; // PATCH /v1/partner/restaurants/{id}/settings
  dealBudget(restaurantId: string): Promise<DealBudget>; // GET   /v1/partner/restaurants/{id}/deal-budget   today's deals used vs the daily limit
  updateListing(restaurantId: string, patch: ListingPatch): Promise<RestaurantDetail>; // PATCH /v1/partner/restaurants/{id}/listing
  partnerSignup(req: PartnerSignupRequest): Promise<AuthSession>; // POST  /v1/partner/signup   public: owner account + draft restaurant
  compliance(restaurantId: string): Promise<ComplianceStatus>; // GET   /v1/partner/restaurants/{id}/compliance   (owner or admin)
  uploadDocument(restaurantId: string, req: UploadDocumentRequest): Promise<ComplianceStatus>; // POST  /v1/partner/restaurants/{id}/documents   multipart/form-data
  submitForReview(restaurantId: string): Promise<ComplianceStatus>; // POST  /v1/partner/restaurants/{id}/submit
  documentFileUrl(documentId: string): Promise<string>; // GET   /v1/documents/{id}/file   (owner or admin; returns a URL to open)
  partnerSubscription(restaurantId: string): Promise<SubscriptionOverview>; // GET   /v1/partner/restaurants/{id}/subscription
  subscriptionCheckout(restaurantId: string, planId: PlanId): Promise<PaymentIntent>; // POST /v1/partner/restaurants/{id}/subscription/checkout   → confirmPayment

  // ---- Admin console (admin only) ----
  adminMetrics(): Promise<AdminMetrics>; // GET   /v1/admin/metrics
  adminInsights(days: 7 | 30 | 90): Promise<AdminInsights>; // GET   /v1/admin/insights?days=
  adminRestaurants(): Promise<RestaurantDetail[]>; // GET   /v1/admin/restaurants
  createRestaurant(req: CreateRestaurantRequest): Promise<RestaurantDetail>; // POST  /v1/admin/restaurants
  updateRestaurant(restaurantId: string, patch: RestaurantPatch): Promise<RestaurantDetail>; // PATCH /v1/admin/restaurants/{id}
  setRestaurantStatus(restaurantId: string, status: RestaurantStatus): Promise<RestaurantDetail>; // POST  /v1/admin/restaurants/{id}/status
  adminUsers(): Promise<User[]>; // GET   /v1/admin/users
  createUser(input: CreateUserRequest): Promise<User>; // POST  /v1/admin/users
  updateUser(userId: string, patch: UserPatch): Promise<User>; // PATCH /v1/admin/users/{id}
  adminBookings(): Promise<Booking[]>; // GET   /v1/admin/bookings
  adminPayments(): Promise<BillPayment[]>; // GET   /v1/admin/payments
  approvalQueue(): Promise<ApprovalQueueItem[]>; // GET   /v1/admin/approvals
  reviewDocument(documentId: string, decision: Exclude<DocumentStatus, "SUBMITTED">, note?: string): Promise<ComplianceStatus>; // POST  /v1/admin/documents/{id}/review
  requestChanges(restaurantId: string, note: string): Promise<ComplianceStatus>; // POST  /v1/admin/restaurants/{id}/request-changes
  listMarkets(): Promise<Market[]>; // GET   /v1/markets   public (this deployment's market)
  paymentProviders(): Promise<PaymentProviderInfo[]>; // GET   /v1/admin/payment-providers
  saveMarket(market: MarketInput): Promise<Market>; // PUT   /v1/admin/markets/{countryCode}   (this deployment's country only)
  createCity(req: CreateCityRequest): Promise<City>; // POST  /v1/admin/cities
  adminSubscriptions(): Promise<AdminSubscriptionRow[]>; // GET   /v1/admin/subscriptions
  platformPayments(): Promise<PlatformPayment[]>; // GET   /v1/admin/platform-payments
  extendSubscription(restaurantId: string, days: number, note?: string): Promise<PaySubscriptionResult>; // POST  /v1/admin/restaurants/{id}/subscription/extend

  // ---- Grievances (diner ↔ restaurant, Feast decides when escalated) ----
  openCase(req: OpenCaseRequest): Promise<Case>; // POST /v1/cases   diner → complaint; owner → report about a diner
  myCases(): Promise<Case[]>; // GET  /v1/me/cases   (as a diner)
  restaurantCases(restaurantId: string): Promise<Case[]>; // GET  /v1/partner/restaurants/{id}/cases
  getCase(id: string): Promise<Case>; // GET  /v1/cases/{id}   (parties or admin)
  caseMessage(id: string, req: CaseMessageRequest): Promise<Case>; // POST /v1/cases/{id}/messages
  escalateCase(id: string): Promise<Case>; // POST /v1/cases/{id}/escalate
  settleCase(id: string): Promise<Case>; // POST /v1/cases/{id}/settle   (diner: "it's sorted")
  addCaseAttachment(id: string, file: File): Promise<Case>; // POST /v1/cases/{id}/attachments   multipart (photo)
  caseAttachmentUrl(id: string, attachmentId: string): Promise<string>; // GET  /v1/cases/{id}/attachments/{attachmentId}
  adminCases(): Promise<AdminCaseItem[]>; // GET  /v1/admin/cases
  resolveCase(id: string, req: ResolveCaseRequest): Promise<Case>; // POST /v1/admin/cases/{id}/resolve

  // ---- Reviews (verified visits) ----
  restaurantReviews(slug: string): Promise<ReviewList>; // GET  /v1/restaurants/{slug}/reviews   public
  createReview(req: CreateReviewRequest): Promise<Review>; // POST /v1/reviews
  myReviews(): Promise<Review[]>; // GET  /v1/me/reviews
  myInsights(days: 30 | 90 | 365 | 0): Promise<DinerInsights>; // GET  /v1/me/insights?days= (0 = all time)
  partnerReviews(restaurantId: string): Promise<ReviewList>; // GET  /v1/partner/restaurants/{id}/reviews
  replyReview(id: string, text: string): Promise<Review>; // POST /v1/partner/reviews/{id}/reply
  reportReview(id: string, reason: string): Promise<Review>; // POST /v1/partner/reviews/{id}/report
  adminReviews(): Promise<Review[]>; // GET  /v1/admin/reviews   (reported or hidden)
  moderateReview(id: string, req: ModerateReviewRequest): Promise<Review>; // POST /v1/admin/reviews/{id}/moderate

  // ---- Content moderation & support access ----
  moderateContent(restaurantId: string, req: ModerateContentRequest): Promise<void>; // POST /v1/admin/restaurants/{id}/moderation
  moderationQueue(): Promise<ModeratedContent[]>; // GET  /v1/admin/moderation
  appealModeration(restaurantId: string, req: ModerationAppealRequest): Promise<void>; // POST /v1/partner/restaurants/{id}/moderation/appeal
  supportAccess(restaurantId: string): Promise<SupportAccess>; // GET  /v1/partner/restaurants/{id}/support-access
  setSupportAccess(restaurantId: string, hours: number): Promise<SupportAccess>; // PUT  /v1/partner/restaurants/{id}/support-access   (owner only)
}

let instance: FeastApi | null = null;

declare global {
  interface Window {
    __FEAST_CONFIG__?: { apiBaseUrl?: string; country?: string };
  }
}

/**
 * This country's API: deploy-time config (/runtime-config.js), else build-time
 * env, else local dev. "/" means the same origin as the web app (web and API
 * behind one domain / reverse proxy — the default for deployments).
 */
export function apiBaseUrl(): string {
  const configured = (typeof window !== "undefined" && window.__FEAST_CONFIG__?.apiBaseUrl) || process.env.NEXT_PUBLIC_API_BASE_URL || "http://localhost:8080";
  if (configured === "/" && typeof window !== "undefined") return window.location.origin;
  return configured.replace(/\/$/, "");
}

/**
 * Where to load an image from: uploaded photos come back as API paths
 * ("/v1/media/…"), so resolve them against this country's API; full URLs
 * (stock photos) pass through.
 */
export function mediaUrl(src: string | undefined | null): string | undefined {
  if (!src) return undefined;
  return src.startsWith("/v1/") ? `${apiBaseUrl()}${src}` : src;
}

export async function getApi(): Promise<FeastApi> {
  if (instance) return instance;
  const { HttpFeastApi } = await import("./http");
  instance = new HttpFeastApi(apiBaseUrl());
  return instance;
}
