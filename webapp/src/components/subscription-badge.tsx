import clsx from "clsx";
import type { Subscription, SubscriptionStatus } from "@/lib/api/types";

export const SUB_STATUS: Record<SubscriptionStatus, { label: string; cls: string }> = {
  NOT_STARTED: { label: "Not started", cls: "bg-stone-100 text-stone-700" },
  TRIAL: { label: "Free trial", cls: "bg-sky-50 text-sky-800" },
  ACTIVE: { label: "Active", cls: "bg-green-50 text-veg" },
  PAST_DUE: { label: "Payment due", cls: "bg-amber-50 text-amber-800" },
  EXPIRED: { label: "Expired · hidden", cls: "bg-red-50 text-danger" },
};

export function SubscriptionBadge({ s, className }: { s: Pick<Subscription, "status">; className?: string }) {
  const m = SUB_STATUS[s.status];
  return <span className={clsx("inline-flex rounded-full px-2 py-0.5 text-xs font-bold", m.cls, className)}>{m.label}</span>;
}

export function shortDate(iso?: string): string {
  return iso ? new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" }) : "—";
}

/** "5 days left", "ends today", "3 days overdue". */
export function daysLabel(s: Pick<Subscription, "daysLeft">): string {
  const d = s.daysLeft ?? 0;
  if (d > 1) return `${d} days left`;
  if (d === 1) return "1 day left";
  if (d === 0) return "ends today";
  return `${-d} day${d === -1 ? "" : "s"} overdue`;
}
