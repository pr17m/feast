"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { CheckCircle2 } from "lucide-react";
import { Card, ErrorState, LinkButton, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";
import { VisitActions } from "@/components/visit-actions";
import { HistoryCard, OFFLINE_METHOD_LABEL, settlementLabel } from "@/components/audit";

export default function PaymentReceiptPage() {
  const { id } = useParams<{ id: string }>();
  const p = useQuery(`billPayment:${id}`, (api) => api.getBillPayment(id));

  if (p.error) return <ErrorState error={p.error} onRetry={p.reload} />;
  if (!p.data) return <Skeleton className="mx-auto h-96 max-w-md" />;
  const { bill, restaurant, payment } = p.data;

  return (
    <div className="mx-auto max-w-md space-y-4">
      <div className="flex flex-col items-center gap-2 rounded-3xl bg-brand px-6 py-8 text-center text-white">
        <CheckCircle2 className="h-12 w-12" aria-hidden />
        <p className="text-sm text-white/80">
          {payment.settlement === "OFFLINE" ? `Paid at the counter · confirmed by ${restaurant.name}` : `Paid to ${restaurant.name}`}
        </p>
        <h1 className="text-4xl font-black">{money(payment.amount, payment.currency)}</h1>
        {bill.savings > 0 && <p className="rounded-full bg-white/15 px-3 py-1 text-sm font-semibold">You saved {money(bill.savings, bill.currency)}</p>}
      </div>

      <Card className="space-y-1.5">
        <h2 className="mb-1 font-bold">Receipt</h2>
        {bill.lines.map((l) => (
          <div key={l.label} className={`flex justify-between text-sm ${l.kind === "DISCOUNT" ? "text-veg" : ""}`}>
            <span className={l.kind === "DISCOUNT" ? "" : "text-muted"}>{l.label}</span>
            <span>{money(l.amount, bill.currency)}</span>
          </div>
        ))}
        <div className="flex justify-between border-t border-line pt-2 font-bold">
          <span>{payment.settlement === "OFFLINE" ? `Paid ${OFFLINE_METHOD_LABEL[payment.method as keyof typeof OFFLINE_METHOD_LABEL]?.toLowerCase() ?? "at the counter"}` : `Paid via ${payment.method}`}</span>
          <span>{money(payment.amount, payment.currency)}</span>
        </div>
        <p className="pt-2 text-xs text-muted">
          {new Date(payment.paidAt).toLocaleString(undefined, {
            dateStyle: "medium",
            timeStyle: "short",
          })}{" "}
          · Ref {payment.intentId}
        </p>
        {payment.settlement === "OFFLINE" && (
          <p className="rounded-lg bg-canvas px-3 py-2 text-xs text-muted ring-1 ring-line">{settlementLabel(payment)}: confirmed by {restaurant.name}.</p>
        )}
      </Card>

      <p className="text-center text-sm text-muted">Show this screen to your server.</p>
      <div className="grid grid-cols-2 gap-3">
        <LinkButton href={`/restaurants/${restaurant.slug}`} variant="secondary">
          View restaurant
        </LinkButton>
        <LinkButton href="/">Done</LinkButton>
      </div>
      <VisitActions billPaymentId={p.data.id} restaurantName={restaurant.name} visitDate={payment.paidAt.slice(0, 10)} canReview />
      <HistoryCard title="Payment history" scopeKey={`bp:${p.data.id}`} load={(api) => api.billPaymentHistory(p.data!.id)} />
      <Link href="/bookings" className="block text-center text-sm font-semibold text-brand">
        All bookings & payments
      </Link>
    </div>
  );
}
