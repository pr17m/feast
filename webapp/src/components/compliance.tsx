import clsx from "clsx";
import { AlertTriangle, CheckCircle2, Clock, FileWarning, XCircle } from "lucide-react";
import { getApi } from "@/lib/api/client";
import type { ComplianceItem, RequirementState } from "@/lib/api/types";

/** Open an uploaded document (owner or admin) in a new tab. */
export async function openDocument(id: string): Promise<void> {
  const tab = window.open("", "_blank");
  const url = await (await getApi()).documentFileUrl(id);
  if (tab) tab.location.href = url;
  else window.open(url, "_blank");
}

export const STATE_META: Record<RequirementState, { label: string; cls: string; icon: typeof Clock }> = {
  MISSING: { label: "Not uploaded", cls: "bg-stone-100 text-stone-700", icon: FileWarning },
  SUBMITTED: { label: "In review", cls: "bg-sky-50 text-sky-800", icon: Clock },
  APPROVED: { label: "Approved", cls: "bg-green-50 text-veg", icon: CheckCircle2 },
  REJECTED: { label: "Rejected", cls: "bg-red-50 text-danger", icon: XCircle },
  EXPIRED: { label: "Expired", cls: "bg-red-50 text-danger", icon: AlertTriangle },
};

export function StateBadge({ item }: { item: ComplianceItem }) {
  const m = STATE_META[item.state];
  const soon = item.expiringSoon && item.state === "APPROVED";
  return (
    <span className={clsx("inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-bold", soon ? "bg-amber-50 text-amber-800" : m.cls)}>
      <m.icon className="h-3.5 w-3.5" aria-hidden />
      {soon ? "Expiring soon" : m.label}
    </span>
  );
}

export function fileSize(bytes: number): string {
  return bytes >= 1024 * 1024 ? `${(bytes / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
