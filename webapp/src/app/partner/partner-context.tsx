"use client";

import { createContext, useContext, useEffect, type ReactNode } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import Link from "next/link";
import {
  AlertTriangle,
  CalendarCheck,
  CreditCard,
  Eye,
  FileCheck2,
  Banknote,
  LayoutDashboard,
  MessageSquareWarning,
  Percent,
  Settings,
  History,
  SquareMenu,
  Star,
  Store,
  QrCode,
  TrendingUp,
} from "lucide-react";
import { PortalShell, type PortalNavItem } from "@/components/portal-shell";
import { useAuth } from "@/components/providers";
import { Empty, Skeleton } from "@/components/ui";
import { useLocalState, useQuery } from "@/lib/hooks";
import type { RestaurantDetail } from "@/lib/api/types";
import { setActiveCurrency } from "@/lib/format";

interface PartnerCtx {
  restaurant: RestaurantDetail;
  restaurants: RestaurantDetail[];
  reloadRestaurants: () => void;
  /** Link to a partner page for the selected restaurant (admins can deep-link with ?r=). */
  href: (path: string) => string;
  /** Feast staff looking at a restaurant (not its owner). */
  isStaff: boolean;
  /** May change content here: the owner, or staff while support access is on. */
  canEdit: boolean;
  /** Staff support access is on until (owner granted it). */
  supportUntil?: string;
  reloadAccess: () => void;
}

const Ctx = createContext<PartnerCtx | null>(null);

export function usePartner(): PartnerCtx {
  const c = useContext(Ctx);
  if (!c) throw new Error("usePartner outside partner layout");
  return c;
}

export function PartnerFrame({ children }: { children: ReactNode }) {
  const { user } = useAuth();
  const sp = useSearchParams();
  const router = useRouter();
  const [saved, setSaved] = useLocalState("feast.partner.restaurant", "");
  const list = useQuery(user ? `partnerRestaurants:${user.id}` : null, (api) => api.partnerRestaurants());

  const fromUrl = sp.get("r");
  const restaurants = list.data ?? [];
  const restaurant = restaurants.find((r) => r.id === fromUrl) ?? restaurants.find((r) => r.id === saved) ?? restaurants[0];
  const href = (path: string) => (restaurant ? `${path}?r=${restaurant.id}` : path);
  const isStaff = user?.role === "ADMIN";
  const access = useQuery(restaurant ? `supportAccess:${restaurant.id}` : null, (api) => api.supportAccess(restaurant!.id));
  const canEdit = !isStaff || !!access.data?.staffCanEdit;
  // Counter payments waiting for this restaurant to confirm (badge on Payments).
  const counter = useQuery(restaurant ? `counterPending:${restaurant.id}` : null, (api) => api.partnerOfflinePayments(restaurant!.id, "PENDING_APPROVAL"));
  // Every amount in the portal is in the selected restaurant's currency.
  setActiveCurrency(restaurant?.currency);

  const nav: PortalNavItem[] = [
    { href: "/partner", label: "Dashboard", icon: LayoutDashboard },
    { href: "/partner/insights", label: "Insights", icon: TrendingUp },
    { href: "/partner/bookings", label: "Bookings", icon: CalendarCheck },
    { href: "/partner/qr", label: "QR code", icon: QrCode },
    { href: "/partner/listing", label: "Listing", icon: Store },
    { href: "/partner/menu", label: "Menu", icon: SquareMenu },
    { href: "/partner/offers", label: "Offers", icon: Percent },
    { href: "/partner/payments", label: "Payments", icon: Banknote, badge: counter.data?.length },
    { href: "/partner/reviews", label: "Reviews", icon: Star },
    {
      href: "/partner/cases",
      label: "Issues & disputes",
      icon: MessageSquareWarning,
    },
    { href: "/partner/documents", label: "Documents", icon: FileCheck2 },
    { href: "/partner/subscription", label: "Subscription", icon: CreditCard },
    { href: "/partner/activity", label: "Activity log", icon: History },
    { href: "/partner/settings", label: "Settings", icon: Settings },
  ];

  // Remember a restaurant opened via ?r= (e.g. an admin's "Manage" link) for later navigation.
  useEffect(() => {
    if (fromUrl && fromUrl !== saved) setSaved(fromUrl);
  }, [fromUrl, saved, setSaved]);

  // Customers only see LIVE/PAUSED restaurants; anything else isn't public yet.
  const isPublic = restaurant && (restaurant.status === "LIVE" || restaurant.status === "PAUSED");
  const preview = restaurant
    ? isPublic
      ? { href: `/restaurants/${restaurant.slug}` }
      : {
          href: null,
          note: `Customers can't see ${restaurant.name} until ${restaurant.status === "ONBOARDING" ? "your documents are submitted and approved" : restaurant.status === "PENDING_APPROVAL" ? "Feast approves your documents" : "it's live"}`,
        }
    : undefined;

  const switcher =
    restaurants.length > 0 ? (
      <label className="block">
        <span className="sr-only">Restaurant</span>
        <select
          value={restaurant?.id}
          onChange={(e) => {
            setSaved(e.target.value);
            const url = new URL(window.location.href);
            url.searchParams.set("r", e.target.value);
            router.replace(url.pathname + url.search);
          }}
          className="h-10 w-full rounded-xl bg-canvas px-3 text-sm font-semibold ring-1 ring-line outline-none focus:ring-2 focus:ring-brand"
        >
          {restaurants.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name} · {r.area}
              {r.status !== "LIVE" ? ` (${r.status.toLowerCase().replace("_", " ")})` : ""}
            </option>
          ))}
        </select>
      </label>
    ) : null;

  return (
    <PortalShell title={user?.role === "ADMIN" ? "Partner · admin" : "Partner"} nav={nav} switcher={switcher} preview={preview}>
      {!list.data && <Skeleton className="h-64 w-full" />}
      {list.data && !restaurant && <Empty title="No restaurants linked to your account" body="Ask a Feast admin to link your restaurant to this login." />}
      {restaurant && (
        <Ctx.Provider
          value={{
            restaurant,
            restaurants,
            reloadRestaurants: list.reload,
            href,
            isStaff,
            canEdit,
            supportUntil: access.data?.activeUntil,
            reloadAccess: access.reload,
          }}
        >
          {/* key: remount pages when switching restaurants so their state resets */}
          <div key={restaurant.id}>
            {isStaff && (
              <div
                className={
                  canEdit
                    ? "mb-4 flex items-center gap-2 rounded-2xl bg-amber-50 p-3 text-sm text-amber-900 ring-1 ring-amber-200"
                    : "mb-4 flex items-center gap-2 rounded-2xl bg-canvas p-3 text-sm text-muted ring-1 ring-line"
                }
                role="note"
              >
                <Eye className="h-4 w-4 shrink-0" aria-hidden />
                {canEdit
                  ? access.data?.activeUntil
                    ? `Support access is on until ${new Date(access.data.activeUntil).toLocaleString()} — every change you make is recorded and shown to the owner.`
                    : "No owner is linked yet, so Feast staff can set this restaurant up. Changes are recorded."
                  : "You're viewing as Feast staff. The owner controls this content — you can hide items or offers that break the rules, or ask the owner to grant support access in their Settings."}
              </div>
            )}
            <PartnerAlerts restaurant={restaurant} />
            {children}
          </div>
        </Ctx.Provider>
      )}
    </PortalShell>
  );
}

/**
 * Banners that matter on every partner page: onboarding not finished, licence
 * expiring/expired, free trial ending, subscription lapsed.
 */
function PartnerAlerts({ restaurant }: { restaurant: RestaurantDetail }) {
  const sub = useQuery(`subscription:${restaurant.id}`, (api) => api.partnerSubscription(restaurant.id));
  const comp = useQuery(`compliance:${restaurant.id}`, (api) => api.compliance(restaurant.id));
  const alerts: {
    tone: "warn" | "danger" | "info";
    text: string;
    href: string;
    cta: string;
  }[] = [];
  const c = comp.data;
  if (c && restaurant.status === "ONBOARDING")
    alerts.push({
      tone: "info",
      text: c.reviewNotes
        ? `Feast asked for changes: ${c.reviewNotes}`
        : "Finish onboarding: upload your licences and registrations so Feast can approve your listing.",
      href: "/partner/documents",
      cta: "Upload documents",
    });
  const expired = c?.items.filter((i) => i.requirement.required && i.state === "EXPIRED") ?? [];
  const expiring = c?.items.filter((i) => i.requirement.required && i.expiringSoon) ?? [];
  if (expired.length)
    alerts.push({
      tone: "danger",
      text: `${expired.map((i) => i.requirement.name).join(", ")} expired — diners can't see you until you upload a valid one.`,
      href: "/partner/documents",
      cta: "Update now",
    });
  else if (expiring.length)
    alerts.push({
      tone: "warn",
      text: `${expiring.map((i) => i.requirement.name).join(", ")} expires on ${expiring[0].document?.expiresOn}. Upload the renewed licence to stay listed.`,
      href: "/partner/documents",
      cta: "Upload renewal",
    });
  const s = sub.data?.subscription;
  if (s?.status === "EXPIRED")
    alerts.push({
      tone: "danger",
      text: "Your subscription lapsed — your restaurant is hidden from diners.",
      href: "/partner/subscription",
      cta: "Choose a plan",
    });
  else if (s?.status === "PAST_DUE")
    alerts.push({
      tone: "danger",
      text: `Payment due — your listing will be hidden on ${new Date(s.hiddenAfter!).toLocaleDateString()}.`,
      href: "/partner/subscription",
      cta: "Pay now",
    });
  else if (s?.status === "TRIAL" && (s.daysLeft ?? 99) <= 7)
    alerts.push({
      tone: "warn",
      text: `Your free trial ends in ${s.daysLeft} day${s.daysLeft === 1 ? "" : "s"}. Pick a plan to stay listed.`,
      href: "/partner/subscription",
      cta: "See plans",
    });
  else if (s?.status === "ACTIVE" && (s.daysLeft ?? 99) <= 7)
    alerts.push({
      tone: "info",
      text: `Your plan ends in ${s.daysLeft} day${s.daysLeft === 1 ? "" : "s"}.`,
      href: "/partner/subscription",
      cta: "Renew",
    });
  if (!alerts.length) return null;
  return (
    <div className="mb-5 space-y-2" role="status">
      {alerts.map((a) => (
        <div
          key={a.text}
          className={
            a.tone === "danger"
              ? "flex flex-wrap items-center gap-3 rounded-2xl bg-red-50 p-3 text-sm text-danger ring-1 ring-red-200"
              : a.tone === "warn"
                ? "flex flex-wrap items-center gap-3 rounded-2xl bg-amber-50 p-3 text-sm text-amber-900 ring-1 ring-amber-200"
                : "flex flex-wrap items-center gap-3 rounded-2xl bg-sky-50 p-3 text-sm text-sky-900 ring-1 ring-sky-200"
          }
        >
          <AlertTriangle className="h-4 w-4 shrink-0" aria-hidden />
          <span className="min-w-0 flex-1">{a.text}</span>
          <Link href={a.href} className="shrink-0 font-semibold underline underline-offset-2">
            {a.cta}
          </Link>
        </div>
      ))}
    </div>
  );
}
