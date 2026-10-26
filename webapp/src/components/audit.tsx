"use client";

import clsx from "clsx";
import { useState } from "react";
import { useQuery } from "@/lib/hooks";
import { Bot, CreditCard, ShieldCheck, User as UserIcon } from "lucide-react";
import type { AuditActor, AuditRecord, OfflineMethod, PaymentRef } from "@/lib/api/types";
import { money } from "@/lib/format";

const ACTION_LABEL: Record<string, string> = {
  "booking.created": "Booking made",
  "booking.confirmed": "Booking confirmed (charge paid)",
  "booking.cancelled": "Booking cancelled",
  "booking.status_changed": "Booking status changed",
  "booking.checked_in": "Diner tapped “I'm here”",
  // Table ordering was removed in Oct 2026; these label older records.
  "order.placed": "Order placed",
  "order.status_changed": "Order status changed",
  "service.requested": "Table asked for service",
  "service.resolved": "Service request handled",
  "table_session.started": "Table opened",
  "payment.started": "Payment started",
  "payment.succeeded": "Payment received",
  "payment.failed": "Payment failed",
  "payment.offline_submitted": "Counter payment submitted",
  "payment.offline_approved": "Counter payment confirmed",
  "payment.offline_rejected": "Counter payment rejected",
  "payment.offline_expired": "Counter payment expired",
  "deal.claimed": "Feast deal held",
  "deal.used": "Feast deal used",
  "deal.released": "Feast deal released",
  "subscription.paid": "Subscription paid",
  "application.submitted": "Application submitted",
  "application.changes_requested": "Changes requested",
  "document.uploaded": "Document uploaded",
  "document.reviewed": "Document reviewed",
  "restaurant.approved": "Restaurant approved",
  "restaurant.created": "Restaurant created",
  "restaurant.status_changed": "Restaurant status changed",
  "case.opened": "Issue opened",
  "case.message": "Issue: new message",
  "case.escalated": "Issue sent to Feast",
  "case.auto_escalated": "Issue sent to Feast (no reply in time)",
  "case.resolved": "Issue resolved",
  "case.auto_closed": "Issue closed (no reply in time)",
  "review.posted": "Review posted",
  "review.replied": "Owner replied to a review",
  "review.reported": "Review reported",
  "review.moderated": "Review moderated",
  "content.moderated": "Content moderated",
  "content.appealed": "Moderation appealed",
  "support_access.changed": "Support access changed",
  "user.created": "User created",
  "user.updated": "User updated",
  "user.blocked": "User blocked",
  "user.unblocked": "User unblocked",
};

export function actionLabel(action: string): string {
  if (ACTION_LABEL[action]) return ACTION_LABEL[action];
  const [entity, verb] = action.split(".");
  return `${(entity ?? "").replace(/_/g, " ")} ${(verb ?? "").replace(/_/g, " ")}`.trim().replace(/^\w/, (c) => c.toUpperCase());
}

export function actorLabel(a: AuditActor): string {
  switch (a.kind) {
    case "USER":
      return `${a.name ?? "Someone"}${a.role ? ` (${a.role.toLowerCase()})` : ""}`;
    case "WEBHOOK":
      return `Payment provider${a.source ? ` · ${a.source}` : ""}`;
    case "SYSTEM":
      return `Feast${a.source && a.source !== "unknown" ? ` · ${a.source}` : ""}`;
    default:
      return "Guest";
  }
}

const tag = (s: string) => s.toLowerCase().replace(/_/g, " ");

export const OFFLINE_METHOD_LABEL: Record<OfflineMethod, string> = { CASH: "Cash at the counter", CARD_AT_COUNTER: "Card at the counter" };

/** "Paid on Feast" or "Paid at the counter, confirmed by the restaurant". */
export function settlementLabel(p: Pick<PaymentRef, "settlement" | "method">): string {
  return p.settlement === "OFFLINE" ? `${OFFLINE_METHOD_LABEL[p.method as OfflineMethod] ?? "At the counter"} · settled offline, not through Feast` : "Paid on Feast";
}

function ActorIcon({ a }: { a: AuditActor }) {
  const Icon = a.kind === "USER" ? (a.role === "ADMIN" ? ShieldCheck : UserIcon) : a.kind === "WEBHOOK" ? CreditCard : Bot;
  return <Icon className="h-4 w-4" aria-hidden />;
}

/** Newest-first list of audit records. `showEntity` adds what each record is about. */
export function AuditTimeline({ items, showEntity, showRequest }: { items: AuditRecord[]; showEntity?: boolean; showRequest?: boolean }) {
  if (items.length === 0) return <p className="text-sm text-muted">Nothing recorded yet.</p>;
  return (
    <ol className="space-y-3" aria-label="History">
      {items.map((r) => (
        <li key={r.id} className="flex gap-3">
          <span className={clsx("mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full", r.actor.kind === "USER" ? "bg-brand-soft text-brand" : "bg-canvas text-muted ring-1 ring-line")}>
            <ActorIcon a={r.actor} />
          </span>
          <div className="min-w-0 flex-1 text-sm">
            <p className="font-semibold">
              {actionLabel(r.action)}
              {(r.fromStatus || r.toStatus) && (
                <span className="font-normal text-muted">
                  {" "}
                  · {r.fromStatus ? `${tag(r.fromStatus)} → ` : ""}
                  {r.toStatus ? tag(r.toStatus) : ""}
                </span>
              )}
              {r.amount !== undefined && r.amount !== null && <span className="font-normal"> · {money(r.amount, r.currency)}</span>}
            </p>
            {r.note && <p className="break-words text-muted">{r.note}</p>}
            <p className="text-xs text-faint">
              {new Date(r.at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })} · {actorLabel(r.actor)}
              {showEntity && (
                <>
                  {" "}
                  · {tag(r.entityType)} <span className="font-mono">{r.entityId}</span>
                </>
              )}
              {showRequest && r.actor.ip && <> · IP {r.actor.ip}</>}
              {showRequest && r.actor.requestId && (
                <>
                  {" "}
                  · req <span className="font-mono">{r.actor.requestId.slice(0, 8)}</span>
                </>
              )}
            </p>
          </div>
        </li>
      ))}
    </ol>
  );
}

/** Entity types offered as filters in the activity log / audit explorer. */
export const AUDIT_ENTITY_TYPES = [
  "booking",
  "bill",
  "bill_payment",
  "payment",
  "offline_payment",
  "case",
  "review",
  "document",
  "restaurant",
  "subscription_payment",
  "menu",
  "offers",
  "settings",
  "user",
] as const;

/** Filterable, paginated audit log (partner activity log and admin explorer). */
export function AuditExplorer({
  load,
  scopeKey,
  admin,
}: {
  load: (api: import("@/lib/api/client").FeastApi, f: import("@/lib/api/types").AuditFilter) => Promise<import("@/lib/api/types").AuditPage>;
  scopeKey: string;
  admin?: boolean;
}) {
  const [draft, setDraft] = useState<AuditFilterState>(EMPTY_FILTER);
  const [filter, setFilter] = useState<AuditFilterState>(EMPTY_FILTER);
  const [pages, setPages] = useState<AuditRecord[][]>([]);
  const [cursor, setCursor] = useState<string | undefined>(undefined);
  const key = `audit:${scopeKey}:${JSON.stringify(filter)}:${cursor ?? ""}`;
  const q = useQuery(key, (api) =>
    load(api, {
      entityType: filter.entityType || undefined,
      entityId: filter.entityId || undefined,
      restaurantId: filter.restaurantId || undefined,
      actorId: filter.actorId || undefined,
      userId: filter.userId || undefined,
      from: filter.from || undefined,
      to: filter.to ? nextDay(filter.to) : undefined,
      cursor,
      limit: 50,
    }),
  );
  const shown = [...pages.flat(), ...(q.data?.items ?? [])];
  const field = (name: keyof AuditFilterState, label: string, extra?: { type?: string; placeholder?: string }) => (
    <label className="flex flex-col gap-1 text-xs font-semibold text-muted">
      {label}
      <input
        type={extra?.type ?? "text"}
        value={draft[name]}
        placeholder={extra?.placeholder}
        onChange={(e) => setDraft({ ...draft, [name]: e.target.value })}
        className="h-9 rounded-lg bg-surface px-2 text-sm font-normal text-ink ring-1 ring-line focus:ring-brand"
      />
    </label>
  );
  return (
    <div className="space-y-4">
      <form
        className="grid grid-cols-2 gap-3 rounded-2xl bg-canvas p-3 ring-1 ring-line md:grid-cols-4 lg:grid-cols-6"
        onSubmit={(e) => {
          e.preventDefault();
          setPages([]);
          setCursor(undefined);
          setFilter(draft);
        }}
      >
        <label className="flex flex-col gap-1 text-xs font-semibold text-muted">
          What
          <select
            value={draft.entityType}
            onChange={(e) => setDraft({ ...draft, entityType: e.target.value })}
            className="h-9 rounded-lg bg-surface px-2 text-sm font-normal text-ink ring-1 ring-line"
          >
            <option value="">Everything</option>
            {AUDIT_ENTITY_TYPES.map((t) => (
              <option key={t} value={t}>
                {t.replace(/_/g, " ")}
              </option>
            ))}
          </select>
        </label>
        {field("entityId", "ID", { placeholder: "bk_… / bill_…" })}
        {admin && field("restaurantId", "Restaurant ID", { placeholder: "r_…" })}
        {admin && field("actorId", "Done by (user ID)", { placeholder: "u_…" })}
        {field("from", "From", { type: "date" })}
        {field("to", "To", { type: "date" })}
        <div className="col-span-2 flex items-end gap-2 md:col-span-4 lg:col-span-6">
          <button type="submit" className="h-9 rounded-lg bg-brand px-4 text-sm font-semibold text-white">
            Apply
          </button>
          <button
            type="button"
            className="h-9 rounded-lg px-3 text-sm font-semibold text-muted hover:text-ink"
            onClick={() => {
              setDraft(EMPTY_FILTER);
              setFilter(EMPTY_FILTER);
              setPages([]);
              setCursor(undefined);
            }}
          >
            Clear
          </button>
        </div>
      </form>
      {q.error && <p role="alert" className="text-sm text-danger">{q.error.message}</p>}
      {!q.data && !q.error && <div className="h-40 animate-pulse rounded-2xl bg-line/50" />}
      {q.data && <AuditTimeline items={shown} showEntity showRequest={admin} />}
      {q.data?.nextCursor && (
        <button
          type="button"
          className="w-full rounded-xl py-2 text-sm font-semibold text-brand ring-1 ring-line hover:bg-brand-soft"
          onClick={() => {
            setPages((p) => [...p, q.data!.items]);
            setCursor(q.data!.nextCursor);
          }}
        >
          Show older
        </button>
      )}
    </div>
  );
}

interface AuditFilterState {
  entityType: string;
  entityId: string;
  restaurantId: string;
  actorId: string;
  userId: string;
  from: string;
  to: string;
}
const EMPTY_FILTER: AuditFilterState = { entityType: "", entityId: "", restaurantId: "", actorId: "", userId: "", from: "", to: "" };

/** The day after `YYYY-MM-DD` (the "to" date is inclusive for people, exclusive for the API). */
function nextDay(d: string): string {
  const t = new Date(`${d}T00:00:00Z`);
  t.setUTCDate(t.getUTCDate() + 1);
  return t.toISOString().slice(0, 10);
}

/** History card for one booking or payment (diner/owner). */
export function HistoryCard({ title, load, scopeKey }: { title: string; load: (api: import("@/lib/api/client").FeastApi) => Promise<import("@/lib/api/types").AuditPage>; scopeKey: string }) {
  const q = useQuery(`history:${scopeKey}`, load);
  return (
    <details className="rounded-2xl bg-surface p-4 ring-1 ring-line">
      <summary className="cursor-pointer font-bold">{title}</summary>
      <div className="mt-3">
        {q.error && <p className="text-sm text-danger">{q.error.message}</p>}
        {!q.data && !q.error && <p className="text-sm text-muted">Loading…</p>}
        {q.data && <AuditTimeline items={q.data.items} />}
      </div>
    </details>
  );
}
