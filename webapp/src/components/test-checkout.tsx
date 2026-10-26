"use client";

import clsx from "clsx";
import { useState } from "react";
import { Apple, Building2, CreditCard, Landmark, ShieldCheck, Smartphone, Wallet, XCircle } from "lucide-react";
import type { PaymentIntent, PaymentMethod } from "@/lib/api/types";
import { money } from "@/lib/format";
import { useMutation } from "@/lib/hooks";
import { Button, Sheet } from "./ui";
import { paymentApplied } from "@/components/refunds";

const METHODS: { id: PaymentMethod; label: string; hint: string; icon: typeof Smartphone }[] = [
  { id: "UPI", label: "UPI", hint: "Any UPI app", icon: Smartphone },
  { id: "CARD", label: "Credit / debit card", hint: "Visa, Mastercard, Amex, RuPay", icon: CreditCard },
  { id: "APPLE_PAY", label: "Apple Pay", hint: "Safari on Apple devices", icon: Apple },
  { id: "GOOGLE_PAY", label: "Google Pay", hint: "Chrome and Android", icon: Smartphone },
  { id: "NETBANKING", label: "Netbanking", hint: "All major banks", icon: Landmark },
  { id: "WALLET", label: "Wallet", hint: "Popular wallets", icon: Wallet },
];

/**
 * Test-mode checkout (`checkout.mode = "test"`): shown only when the server
 * runs with test payments (development, staging). No money moves. Real
 * deployments get the provider's own checkout (see checkout.tsx).
 */
export function TestCheckout({
  open,
  intent,
  title,
  onClose,
  onSuccess,
}: {
  open: boolean;
  intent: PaymentIntent | null;
  title: string;
  onClose: () => void;
  onSuccess: (intent: PaymentIntent) => void;
}) {
  // Methods offered in this market (from the intent); fall back to all.
  const allowed = intent?.checkout?.methods?.split(",").filter(Boolean) as PaymentMethod[] | undefined;
  const methods = METHODS.filter((m) => !allowed?.length || allowed.includes(m.id));
  const [picked, setMethod] = useState<PaymentMethod | null>(null);
  const method = picked && methods.some((m) => m.id === picked) ? picked : methods[0]?.id ?? "CARD";
  const currency = intent?.currency;
  const [simulateFailure, setSimulateFailure] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const pay = useMutation((api, id: string, m: PaymentMethod, sim: "success" | "failure") => api.confirmPayment(id, m, sim));

  const submit = async () => {
    if (!intent) return;
    setFailed(null);
    const res = await pay.run(intent.id, method, simulateFailure ? "failure" : "success");
    if (!res) return;
    if (res && paymentApplied(res)) onSuccess(res);
    else setFailed(res.failureReason ?? "Payment failed");
  };

  return (
    <Sheet
      open={open}
      onClose={pay.pending ? () => {} : onClose}
      title="Payment"
      footer={
        <Button size="lg" className="w-full" onClick={submit} loading={pay.pending} disabled={!intent}>
          {pay.pending ? "Processing payment…" : `Pay ${intent ? money(intent.amount, intent.currency) : ""}`}
        </Button>
      }
    >
      <div className="space-y-4">
        <div className="flex items-center justify-between rounded-2xl bg-canvas p-4">
          <div>
            <p className="text-sm text-muted">{title}</p>
            <p className="text-2xl font-black">{intent ? money(intent.amount, currency) : "—"}</p>
          </div>
          <span className="flex items-center gap-1 rounded-full bg-amber-100 px-2.5 py-1 text-xs font-bold text-amber-800">
            TEST MODE
          </span>
        </div>

        {(failed || pay.error) && (
          <div className="flex items-start gap-2 rounded-xl bg-red-50 p-3 text-sm text-danger" role="alert">
            <XCircle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden />
            <span>
              {failed ?? pay.error?.message} No money was deducted. Try again or pick another method.
            </span>
          </div>
        )}

        <fieldset>
          <legend className="mb-2 text-sm font-semibold">Pay using</legend>
          <div className="space-y-2">
            {methods.map((m) => (
              <label
                key={m.id}
                className={clsx(
                  "flex cursor-pointer items-center gap-3 rounded-xl p-3 ring-1 transition-colors",
                  method === m.id ? "bg-brand-soft ring-brand" : "ring-line hover:bg-canvas",
                )}
              >
                <input type="radio" name="method" className="sr-only" checked={method === m.id} onChange={() => setMethod(m.id)} />
                <m.icon className="h-5 w-5 text-brand" aria-hidden />
                <span className="flex-1">
                  <span className="block font-semibold">{m.label}</span>
                  <span className="text-sm text-muted">{m.hint}</span>
                </span>
                <span
                  className={clsx("h-4 w-4 rounded-full border-2", method === m.id ? "border-brand bg-brand shadow-[inset_0_0_0_2px_white]" : "border-faint")}
                  aria-hidden
                />
              </label>
            ))}
          </div>
        </fieldset>

        <label className="flex items-center justify-between gap-3 rounded-xl border border-dashed border-line p-3 text-sm">
          <span className="flex items-center gap-2 text-muted">
            <Building2 className="h-4 w-4" aria-hidden />
            Test mode: simulate a declined payment
          </span>
          <input type="checkbox" checked={simulateFailure} onChange={(e) => setSimulateFailure(e.target.checked)} className="h-4 w-4 accent-[var(--color-brand)]" />
        </label>

        <p className="flex items-center gap-1.5 text-xs text-muted">
          <ShieldCheck className="h-4 w-4 text-brand" aria-hidden />
          Test payments — no real money moves.
        </p>
      </div>
    </Sheet>
  );
}
