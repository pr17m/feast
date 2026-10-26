"use client";

import { useState } from "react";
import clsx from "clsx";
import { AlertTriangle, CheckCircle2, CreditCard, Receipt, Sparkles, XCircle } from "lucide-react";
import { Checkout } from "@/components/checkout";
import { PageHeader } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { SubscriptionBadge, daysLabel, shortDate } from "@/components/subscription-badge";
import { Button, Card, Empty, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { money } from "@/lib/format";
import type { PaymentIntent, PaymentMethod, PlanId, SubscriptionPlan } from "@/lib/api/types";
import { usePartner } from "../partner-context";

const METHOD_LABEL: Record<PaymentMethod, string> = {
  UPI: "UPI",
  CARD: "Card",
  NETBANKING: "Netbanking",
  WALLET: "Wallet",
  APPLE_PAY: "Apple Pay",
  GOOGLE_PAY: "Google Pay",
  CASH: "Cash",
  CARD_AT_COUNTER: "Card at the counter",
};

export default function PartnerSubscription() {
  const { restaurant, reloadRestaurants } = usePartner();
  const toast = useToast();
  const q = useQuery(`subscription:${restaurant.id}`, (api) => api.partnerSubscription(restaurant.id));
  const [plan, setPlan] = useState<SubscriptionPlan | null>(null);
  const [intent, setIntent] = useState<PaymentIntent | null>(null);
  const start = useMutation((api, p: PlanId) => api.subscriptionCheckout(restaurant.id, p));

  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const { subscription: s, plans, payments, rules, methods } = q.data;
  const notLive = s.status === "NOT_STARTED";

  // Plan summary → payment intent with the country's provider → its checkout.
  const checkout = async () => {
    if (!plan) return;
    const pi = await start.run(plan.id);
    if (pi) setIntent(pi);
  };
  const paid = (pi: PaymentIntent) => {
    setIntent(null);
    setPlan(null);
    toast(`Paid ${money(pi.amount, pi.currency)} · thank you!`);
    q.reload();
    reloadRestaurants();
  };

  return (
    <div className="max-w-4xl space-y-5">
      <PageHeader title="Subscription" subtitle={`Free for the first ${rules.trialDays} days on Feast, then a simple subscription keeps you listed.`} />

      <Card className={clsx("space-y-2", s.status === "EXPIRED" && "ring-danger/40", s.status === "PAST_DUE" && "ring-amber-300")}>
        <div className="flex flex-wrap items-center gap-2">
          <SubscriptionBadge s={s} />
          {s.planId && s.status === "ACTIVE" && <span className="text-sm font-semibold">{plans.find((p) => p.id === s.planId)?.name} plan</span>}
        </div>
        {notLive && <p className="text-muted">Your free trial starts the day Feast approves your restaurant and it goes live.</p>}
        {s.status === "TRIAL" && (
          <p className="text-lg">
            <b>{daysLabel(s)}</b> in your free trial — ends {shortDate(s.endsAt)}. Pick a plan any time; it starts when the trial ends.
          </p>
        )}
        {s.status === "ACTIVE" && (
          <p className="text-lg">
            Listed until <b>{shortDate(s.endsAt)}</b> ({daysLabel(s)}). Renew early — new time is added to the end.
          </p>
        )}
        {s.status === "PAST_DUE" && (
          <p className="flex items-start gap-2 text-lg text-amber-900">
            <AlertTriangle className="mt-1 h-5 w-5 shrink-0" aria-hidden />
            <span>
              Your {s.paidThrough ? "plan" : "free trial"} ended on {shortDate(s.endsAt)}. Pay by <b>{shortDate(s.hiddenAfter)}</b> or diners won&apos;t be able to find you.
            </span>
          </p>
        )}
        {s.status === "EXPIRED" && (
          <p className="flex items-start gap-2 text-lg text-danger">
            <XCircle className="mt-1 h-5 w-5 shrink-0" aria-hidden />
            <span>Your listing is hidden from diners since {shortDate(s.hiddenAfter)}. Choose a plan to appear again immediately.</span>
          </p>
        )}
        <p className="text-sm text-muted">After a period ends you have {rules.graceDays} days of grace before the listing is hidden.</p>
      </Card>

      <section aria-label="Plans">
        <h2 className="mb-3 font-bold">Plans</h2>
        <div className="grid gap-3 md:grid-cols-3">
          {plans.map((p) => (
            <div key={p.id} className={clsx("flex flex-col rounded-2xl bg-surface p-4 ring-1", p.id === "YEARLY" ? "ring-brand" : "ring-line")}>
              <div className="flex items-center justify-between">
                <h3 className="font-bold">{p.name}</h3>
                {p.savePercent ? <span className="rounded-full bg-deal-soft px-2 py-0.5 text-xs font-bold text-deal">Save {p.savePercent}%</span> : null}
              </div>
              <p className="mt-2 text-3xl font-black">{money(p.price, p.currency)}</p>
              <p className="text-sm text-muted">
                + {p.taxName} {p.taxPercent}% · {money(p.total, p.currency)} total
              </p>
              <p className="mt-1 text-sm text-muted">
                {p.months === 1 ? "Billed monthly" : `${p.months} months · ${money(Math.round(p.price / p.months), p.currency)}/month`}
              </p>
              <Button className="mt-4" variant={p.id === "YEARLY" ? "primary" : "secondary"} disabled={notLive} onClick={() => setPlan(p)}>
                <CreditCard className="h-4 w-4" aria-hidden /> {s.status === "ACTIVE" ? "Extend" : "Choose"} {p.name.toLowerCase()}
              </Button>
            </div>
          ))}
        </div>
      </section>

      <section aria-label="Invoices">
        <h2 className="mb-3 flex items-center gap-2 font-bold">
          <Receipt className="h-5 w-5 text-brand" aria-hidden /> Invoices
        </h2>
        {payments.length === 0 ? (
          <Empty title="No payments yet" body="Invoices appear here after you pay for a plan." />
        ) : (
          <div className="overflow-x-auto rounded-2xl bg-surface ring-1 ring-line">
            <table className="w-full min-w-[640px] text-left text-sm">
              <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
                <tr>
                  <th className="px-4 py-3">Date</th>
                  <th className="px-4 py-3">Invoice</th>
                  <th className="px-4 py-3">Plan · period</th>
                  <th className="px-4 py-3 text-right">Amount</th>
                  <th className="px-4 py-3 text-right">Tax</th>
                  <th className="px-4 py-3 text-right">Total</th>
                  <th className="px-4 py-3">Status</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {payments.map((p) => (
                  <tr key={p.id}>
                    <td className="whitespace-nowrap px-4 py-3">{shortDate(p.createdAt)}</td>
                    <td className="px-4 py-3 font-mono text-xs">{p.invoiceNumber}</td>
                    <td className="px-4 py-3">
                      {p.planId === "COMP" ? "Free days from Feast" : plans.find((x) => x.id === p.planId)?.name}
                      <span className="block text-xs text-muted">
                        {shortDate(p.periodStart)} – {shortDate(p.periodEnd)}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-right">{money(p.amount, p.currency)}</td>
                    <td className="px-4 py-3 text-right">
                      {money(p.tax, p.currency)} <span className="text-xs text-muted">{p.taxName}</span>
                    </td>
                    <td className="px-4 py-3 text-right font-bold">{money(p.total, p.currency)}</td>
                    <td className="px-4 py-3">
                      {p.status === "PAID" ? (
                        <span className="inline-flex items-center gap-1 text-veg">
                          <CheckCircle2 className="h-4 w-4" aria-hidden /> Paid{p.method ? ` · ${METHOD_LABEL[p.method]}` : ""}
                        </span>
                      ) : (
                        <span className="text-danger">Failed</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <Sheet
        open={!!plan && !intent}
        onClose={start.pending ? () => {} : () => setPlan(null)}
        title={plan ? `${plan.name} plan` : ""}
        footer={
          <Button size="lg" className="w-full" loading={start.pending} onClick={checkout}>
            Continue to pay {plan ? money(plan.total, plan.currency) : ""}
          </Button>
        }
      >
        {plan && (
          <div className="space-y-4">
            <dl className="space-y-1 rounded-2xl bg-canvas p-4 text-sm">
              <div className="flex justify-between">
                <dt className="text-muted">{plan.name} subscription</dt>
                <dd>{money(plan.price, plan.currency)}</dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-muted">
                  {plan.taxName} ({plan.taxPercent}%)
                </dt>
                <dd>{money(plan.total - plan.price, plan.currency)}</dd>
              </div>
              <div className="flex justify-between border-t border-line pt-1 text-base font-bold">
                <dt>Total</dt>
                <dd>{money(plan.total, plan.currency)}</dd>
              </div>
            </dl>
            <p className="text-sm text-muted">Pay with {methods.map((m) => METHOD_LABEL[m]).join(", ")}.</p>
            {start.error && (
              <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">
                {start.error.message}
              </p>
            )}
            <p className="flex items-center gap-1.5 text-xs text-muted">
              <Sparkles className="h-4 w-4 text-brand" aria-hidden /> New time is added after your current {s.status === "TRIAL" ? "trial" : "period"} ends — you never lose days.
            </p>
          </div>
        )}
      </Sheet>
      <Checkout
        open={!!intent}
        intent={intent}
        title={plan ? `Feast ${plan.name.toLowerCase()} plan · ${restaurant.name}` : "Feast subscription"}
        onClose={() => {
          setIntent(null);
          q.reload(); // a declined attempt shows in the invoice list
        }}
        onSuccess={paid}
      />
    </div>
  );
}
