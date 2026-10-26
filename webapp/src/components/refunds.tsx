"use client";

import clsx from "clsx";
import { RotateCcw } from "lucide-react";
import type { PaymentIntent, PaymentRef, Refund, RefundReason, RefundStatus } from "@/lib/api/types";
import { useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";
import { Card } from "@/components/ui";

export const REFUND_REASON: Record<RefundReason, string> = {
  CANCELLED_IN_TIME: "Cancelled at least 2 hours before",
  CANCELLED_BY_RESTAURANT: "Cancelled by the restaurant",
  ACCOUNT_DELETED: "Account deleted",
  PAID_AFTER_CANCELLATION: "Paid after the booking was cancelled",
  ALREADY_PAID: "Already paid",
  CASE_DECISION: "Feast's decision on your complaint",
  MANUAL: "Refunded by Feast",
};

export const REFUND_STATUS: Record<RefundStatus, { label: string; tone: string }> = {
  PENDING: { label: "Refund on its way", tone: "bg-amber-50 text-amber-800 ring-amber-200" },
  SUCCEEDED: { label: "Refunded", tone: "bg-green-50 text-veg ring-green-200" },
  FAILED: { label: "Refund failed — Feast support will contact you", tone: "bg-red-50 text-danger ring-red-200" },
};

/** A confirmed payment that was applied (booking confirmed / bill paid). A payment that
 *  arrived too late (booking cancelled, bill already paid) succeeds at the provider but is
 *  refunded instead: it has no result and says why. */
export function paymentApplied(i: PaymentIntent | null | undefined): boolean {
  return !!i && i.status === "SUCCEEDED" && !!i.resultRef;
}

export function RefundRow({ r }: { r: Refund }) {
  const st = REFUND_STATUS[r.status];
  return (
    <div className="flex items-start justify-between gap-3 text-sm">
      <div>
        <p className="font-semibold">{money(r.amount, r.currency)}</p>
        <p className="text-xs text-muted">
          {REFUND_REASON[r.reason]} · {new Date(r.createdAt).toLocaleDateString(undefined, { dateStyle: "medium" })}
        </p>
        {r.status === "FAILED" && r.failure && <p className="text-xs text-danger">{r.failure}</p>}
      </div>
      <span className={clsx("shrink-0 rounded-full px-2 py-0.5 text-xs font-semibold ring-1", st.tone)}>{st.label}</span>
    </div>
  );
}

/** Refunds of a payment, for the diner's booking page and receipt. Counter payments
 *  never went through Feast, so Feast never refunds them. */
export function RefundsCard({ payment, version = "" }: { payment: PaymentRef | undefined; version?: string }) {
  const online = !!payment && payment.settlement !== "OFFLINE";
  const q = useQuery(online ? `refunds:${payment!.intentId}:${version}` : null, (api) => api.paymentRefunds(payment!.intentId));
  if (!payment) return null;
  if (!online) {
    return (
      <p className="rounded-lg bg-canvas px-3 py-2 text-xs text-muted ring-1 ring-line">
        Paid at the counter — refunds are between you and the restaurant.
      </p>
    );
  }
  const refunds = q.data ?? [];
  if (!refunds.length) return null;
  return (
    <Card className="space-y-3">
      <h2 className="flex items-center gap-2 font-bold">
        <RotateCcw className="h-4 w-4" aria-hidden /> Refunds
      </h2>
      {refunds.map((r) => (
        <RefundRow key={r.id} r={r} />
      ))}
      <p className="text-xs text-muted">Refunds go back to the card or account you paid with, usually within 5–10 business days.</p>
    </Card>
  );
}
