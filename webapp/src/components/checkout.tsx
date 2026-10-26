"use client";

import { useEffect, useRef, useState } from "react";
import { ShieldCheck, XCircle } from "lucide-react";
import type { PaymentIntent } from "@/lib/api/types";
import { money } from "@/lib/format";
import { useMutation } from "@/lib/hooks";
import { TestCheckout } from "./test-checkout";
import { Button, Sheet } from "./ui";

/**
 * Pays a PaymentIntent with whatever provider the country's market uses. The
 * server says which checkout to open in `intent.checkout.mode`:
 * - "razorpay": Razorpay Checkout (UPI, cards, netbanking, wallets) — India.
 * - "stripe": Stripe Payment Element (cards, Apple Pay, Google Pay) — Australia…
 * - "test": test-mode simulator (development/staging servers only).
 * The backend verifies every payment with the provider before marking it paid.
 * Adding a provider: a component here + an adapter on the server.
 */
export function Checkout(props: { open: boolean; intent: PaymentIntent | null; title: string; onClose: () => void; onSuccess: (intent: PaymentIntent) => void }) {
  const mode = props.intent?.checkout?.mode;
  if (props.intent && mode === "razorpay") return <RazorpayCheckout {...props} intent={props.intent} />;
  if (props.intent && mode === "stripe") return <StripeCheckout {...props} intent={props.intent} />;
  return <TestCheckout {...props} />;
}

const loaded = new Map<string, Promise<void>>();
function loadScript(src: string): Promise<void> {
  let p = loaded.get(src);
  if (!p) {
    p = new Promise((resolve, reject) => {
      const el = document.createElement("script");
      el.src = src;
      el.onload = () => resolve();
      el.onerror = () => reject(new Error("Couldn't load the payment page. Check your connection."));
      document.head.appendChild(el);
    });
    loaded.set(src, p);
  }
  return p;
}

type Props = { open: boolean; intent: PaymentIntent; title: string; onClose: () => void; onSuccess: (intent: PaymentIntent) => void };

function RazorpayCheckout({ open, intent, title, onClose, onSuccess }: Props) {
  const confirm = useMutation((api, payload: Record<string, string>) => api.confirmPayment(intent.id, "UPI", undefined, payload));
  const [error, setError] = useState<string | null>(null);
  const started = useRef(false);

  useEffect(() => {
    if (!open || started.current) return;
    started.current = true;
    (async () => {
      try {
        await loadScript("https://checkout.razorpay.com/v1/checkout.js");
        const Razorpay = (window as unknown as { Razorpay: new (o: object) => { open: () => void } }).Razorpay;
        new Razorpay({
          key: intent.checkout?.keyId,
          order_id: intent.checkout?.orderId,
          amount: intent.amount,
          currency: intent.currency,
          name: intent.checkout?.name ?? "Feast",
          description: title,
          handler: async (r: Record<string, string>) => {
            const res = await confirm.run({ razorpay_payment_id: r.razorpay_payment_id, razorpay_order_id: r.razorpay_order_id, razorpay_signature: r.razorpay_signature });
            if (res?.status === "SUCCEEDED") onSuccess(res);
            else setError(res?.failureReason ?? "Payment couldn't be verified");
          },
          modal: { ondismiss: () => onClose() },
        }).open();
      } catch (e) {
        setError((e as Error).message);
      }
    })();
  }, [open, intent, title, confirm, onClose, onSuccess]);

  if (!error && !confirm.pending) return null;
  return (
    <Sheet open={open} onClose={onClose} title="Payment">
      {confirm.pending ? (
        <p className="text-muted">Confirming your payment…</p>
      ) : (
        <p role="alert" className="flex items-start gap-2 rounded-xl bg-red-50 p-3 text-sm text-danger">
          <XCircle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden /> {error}
        </p>
      )}
    </Sheet>
  );
}

type StripeApi = {
  elements: (o: object) => { create: (t: string, o?: object) => { mount: (el: HTMLElement) => void; destroy: () => void } };
  confirmPayment: (o: object) => Promise<{ error?: { message?: string }; paymentIntent?: { id: string; status: string } }>;
};

function StripeCheckout({ open, intent, title, onClose, onSuccess }: Props) {
  const mount = useRef<HTMLDivElement>(null);
  const stripe = useRef<{ api: StripeApi; elements: ReturnType<StripeApi["elements"]> } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const [paying, setPaying] = useState(false);
  const confirm = useMutation((api, payload: Record<string, string>) => api.confirmPayment(intent.id, "CARD", undefined, payload));

  useEffect(() => {
    if (!open || !mount.current) return;
    let el: { destroy: () => void } | undefined;
    (async () => {
      try {
        await loadScript("https://js.stripe.com/v3/");
        const Stripe = (window as unknown as { Stripe: (key: string) => StripeApi }).Stripe;
        const api = Stripe(intent.checkout?.publishableKey ?? "");
        const elements = api.elements({ clientSecret: intent.checkout?.clientSecret, appearance: { theme: "stripe" } });
        const pe = elements.create("payment", { layout: "tabs" });
        pe.mount(mount.current!);
        el = pe;
        stripe.current = { api, elements };
        setReady(true);
      } catch (e) {
        setError((e as Error).message);
      }
    })();
    return () => el?.destroy();
  }, [open, intent]);

  const pay = async () => {
    if (!stripe.current) return;
    setPaying(true);
    setError(null);
    const r = await stripe.current.api.confirmPayment({ elements: stripe.current.elements, redirect: "if_required" });
    if (r.error) {
      setError(r.error.message ?? "Payment failed");
      setPaying(false);
      return;
    }
    const res = await confirm.run({ payment_intent_id: r.paymentIntent?.id ?? "" });
    setPaying(false);
    if (res?.status === "SUCCEEDED") onSuccess(res);
    else setError(res?.failureReason ?? "Payment couldn't be verified");
  };

  return (
    <Sheet
      open={open}
      onClose={paying ? () => {} : onClose}
      title="Payment"
      footer={
        <Button size="lg" className="w-full" onClick={pay} loading={paying} disabled={!ready}>
          Pay {money(intent.amount, intent.currency)}
        </Button>
      }
    >
      <div className="space-y-4">
        <div className="rounded-2xl bg-canvas p-4">
          <p className="text-sm text-muted">{title}</p>
          <p className="text-2xl font-black">{money(intent.amount, intent.currency)}</p>
        </div>
        <div ref={mount} />
        {error && (
          <p role="alert" className="flex items-start gap-2 rounded-xl bg-red-50 p-3 text-sm text-danger">
            <XCircle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden /> {error}
          </p>
        )}
        <p className="flex items-center gap-1.5 text-xs text-muted">
          <ShieldCheck className="h-4 w-4 text-brand" aria-hidden /> Payments are processed securely by Stripe.
        </p>
      </div>
    </Sheet>
  );
}
