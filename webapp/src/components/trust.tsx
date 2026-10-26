"use client";

import clsx from "clsx";
import Link from "next/link";
import { useRef, useState } from "react";
import { AlertTriangle, Camera, CheckCircle2, EyeOff, Flag, ImageIcon, Lock, MessageSquare, RotateCcw, Scale, Send, ShieldCheck, Star } from "lucide-react";
import { useToast } from "@/components/providers";
import { Button, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { longDate, money, timeAgo, toMinor } from "@/lib/format";
import type { Case, CaseDecision, CaseKind, CaseReason, CaseRole, CaseStatus, ContentKind, Moderation, Review } from "@/lib/api/types";

const field = "w-full rounded-xl bg-canvas px-3 py-2 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

// ================================================================ labels

export const REASON_LABEL: Record<CaseReason, string> = {
  TABLE_NOT_HONOURED: "Table not honoured",
  OFFER_NOT_HONOURED: "Offer not honoured",
  WRONG_BILL: "Wrong bill",
  FOOD_QUALITY: "Food quality",
  HYGIENE: "Hygiene",
  STAFF_BEHAVIOUR: "Staff behaviour",
  NO_SHOW: "No-show",
  ABUSIVE_BEHAVIOUR: "Abusive behaviour",
  PAYMENT_DISPUTE: "Payment dispute",
  PROPERTY_DAMAGE: "Property damage",
  OTHER: "Something else",
};

export const COMPLAINT_REASONS: CaseReason[] = [
  "TABLE_NOT_HONOURED",
  "OFFER_NOT_HONOURED",
  "WRONG_BILL",
  "FOOD_QUALITY",
  "HYGIENE",
  "STAFF_BEHAVIOUR",
  "OTHER",
];
export const REPORT_REASONS: CaseReason[] = ["NO_SHOW", "ABUSIVE_BEHAVIOUR", "PAYMENT_DISPUTE", "PROPERTY_DAMAGE", "OTHER"];

export const DECISION_LABEL: Record<CaseDecision, string> = {
  SETTLED_BETWEEN_PARTIES: "Settled",
  NO_ACTION: "No action",
  REFUND_ADVISED: "Refund advised",
  RESTAURANT_WARNED: "Restaurant warned",
  RESTAURANT_SUSPENDED: "Restaurant suspended",
  CUSTOMER_WARNED: "Diner warned",
  CUSTOMER_BLOCKED: "Diner blocked",
};

const DECISIONS: Record<CaseKind, CaseDecision[]> = {
  COMPLAINT: ["NO_ACTION", "REFUND_ADVISED", "RESTAURANT_WARNED", "RESTAURANT_SUSPENDED", "CUSTOMER_WARNED"],
  REPORT: ["NO_ACTION", "CUSTOMER_WARNED", "CUSTOMER_BLOCKED", "RESTAURANT_WARNED"],
};

function statusText(c: Case, viewer: CaseRole): string {
  switch (c.status) {
    case "AWAITING_RESTAURANT":
      return viewer === "RESTAURANT" ? "Your reply needed" : "Waiting for the restaurant";
    case "AWAITING_CUSTOMER":
      return viewer === "CUSTOMER" ? "Your reply needed" : "Waiting for the diner";
    case "WITH_FEAST":
      return viewer === "FEAST" ? "Your decision needed" : "With Feast";
    case "RESOLVED":
      return c.outcome ? DECISION_LABEL[c.outcome.decision] : "Resolved";
  }
}

function needsMe(status: CaseStatus, viewer: CaseRole) {
  return (
    (status === "AWAITING_RESTAURANT" && viewer === "RESTAURANT") ||
    (status === "AWAITING_CUSTOMER" && viewer === "CUSTOMER") ||
    (status === "WITH_FEAST" && viewer === "FEAST")
  );
}

export function CaseStatusBadge({ c, viewer }: { c: Case; viewer: CaseRole }) {
  const mine = needsMe(c.status, viewer);
  return (
    <span
      className={clsx(
        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-semibold",
        c.status === "RESOLVED"
          ? "bg-emerald-50 text-emerald-800"
          : mine
            ? "bg-amber-100 text-amber-900"
            : c.status === "WITH_FEAST"
              ? "bg-sky-50 text-sky-900"
              : "bg-canvas text-muted ring-1 ring-line",
      )}
    >
      {c.status === "RESOLVED" ? (
        <CheckCircle2 className="h-3 w-3" aria-hidden />
      ) : c.status === "WITH_FEAST" ? (
        <Scale className="h-3 w-3" aria-hidden />
      ) : (
        <MessageSquare className="h-3 w-3" aria-hidden />
      )}
      {statusText(c, viewer)}
    </span>
  );
}

function due(iso?: string) {
  if (!iso) return null;
  const d = new Date(iso);
  const late = d.getTime() < Date.now();
  const label = d.toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "numeric",
    minute: "2-digit",
  });
  return <span className={late ? "font-semibold text-danger" : "text-muted"}>{late ? `Overdue since ${label}` : `Reply by ${label}`}</span>;
}

// ================================================================ case list

export function CaseList({ cases, viewer, hrefFor, empty }: { cases: Case[]; viewer: CaseRole; hrefFor: (c: Case) => string; empty: React.ReactNode }) {
  if (!cases.length) return <>{empty}</>;
  return (
    <ul className="divide-y divide-line overflow-hidden rounded-2xl bg-surface ring-1 ring-line">
      {cases.map((c) => (
        <li key={c.id}>
          <Link href={hrefFor(c)} className="flex flex-wrap items-center gap-3 p-4 hover:bg-canvas">
            <div className="min-w-0 flex-1">
              <p className="font-semibold">
                {REASON_LABEL[c.reason]} <span className="font-normal text-muted">· {c.number}</span>
              </p>
              <p className="truncate text-sm text-muted">
                {c.kind === "COMPLAINT"
                  ? viewer === "CUSTOMER"
                    ? c.restaurant.name
                    : `From ${c.customer.name}`
                  : viewer === "CUSTOMER"
                    ? `Report by ${c.restaurant.name}`
                    : `About ${c.customer.name}`}
                {" · visit "}
                {longDate(c.visitDate)} · {timeAgo(c.updatedAt)}
              </p>
            </div>
            <CaseStatusBadge c={c} viewer={viewer} />
          </Link>
        </li>
      ))}
    </ul>
  );
}

// ================================================================ open a case

export function ReportProblemSheet({
  open,
  onClose,
  kind,
  bookingId,
  billPaymentId,
  subject,
  onOpened,
}: {
  open: boolean;
  onClose: () => void;
  kind: CaseKind;
  bookingId?: string;
  billPaymentId?: string;
  /** "Harbour & Vine · Sat 24 Oct" or "Jack W. · table for 4" */
  subject: string;
  onOpened: (c: Case) => void;
}) {
  const reasons = kind === "COMPLAINT" ? COMPLAINT_REASONS : REPORT_REASONS;
  const [reason, setReason] = useState<CaseReason>(reasons[0]);
  const [text, setText] = useState("");
  const create = useMutation((api) => api.openCase({ bookingId, billPaymentId, reason, text }));
  const valid = text.trim().length >= 10;
  return (
    <Sheet
      open={open}
      onClose={create.pending ? () => {} : onClose}
      title={kind === "COMPLAINT" ? "Report a problem" : "Report a diner to Feast"}
      footer={
        <Button
          className="w-full"
          disabled={!valid}
          loading={create.pending}
          onClick={async () => {
            const c = await create.run();
            if (c) onOpened(c);
          }}
        >
          <Send className="h-4 w-4" aria-hidden /> {kind === "COMPLAINT" ? "Send to the restaurant" : "Send to Feast"}
        </Button>
      }
    >
      <div className="space-y-4">
        <p className="text-sm text-muted">{subject}</p>
        <fieldset>
          <legend className="mb-2 text-sm font-semibold">What happened?</legend>
          <div className="flex flex-wrap gap-2">
            {reasons.map((r) => (
              <button
                key={r}
                type="button"
                onClick={() => setReason(r)}
                aria-pressed={reason === r}
                className={clsx(
                  "rounded-full px-3 py-1.5 text-sm ring-1",
                  reason === r ? "bg-brand text-white ring-brand" : "bg-surface ring-line hover:bg-canvas",
                )}
              >
                {REASON_LABEL[r]}
              </button>
            ))}
          </div>
        </fieldset>
        <label className="block">
          <span className="mb-1 block text-sm font-semibold">Details</span>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={5}
            maxLength={2000}
            className={field}
            placeholder={
              kind === "COMPLAINT"
                ? "Tell the restaurant what went wrong and what would make it right."
                : "What happened, and when? Feast will hear the diner's side too."
            }
          />
          <span className="mt-1 block text-xs text-muted">At least 10 characters. You can add photos after sending.</span>
        </label>
        <p className="rounded-xl bg-canvas p-3 text-xs text-muted">
          {kind === "COMPLAINT"
            ? "The restaurant has 48 hours to reply. If they don't, or you're not happy with their answer, Feast steps in and decides."
            : "Feast reviews reports within 72 hours. The diner is told and can give their side. Repeat or serious behaviour can lead to a warning or a block."}
        </p>
        {create.error && (
          <p role="alert" className="text-sm text-danger">
            {create.error.message}
          </p>
        )}
      </div>
    </Sheet>
  );
}

// ================================================================ case thread

function Attachment({ caseId, a }: { caseId: string; a: Case["attachments"][number] }) {
  const [busy, setBusy] = useState(false);
  return (
    <button
      type="button"
      disabled={busy}
      onClick={async () => {
        setBusy(true);
        try {
          const { getApi } = await import("@/lib/api/client");
          const url = await (await getApi()).caseAttachmentUrl(caseId, a.id);
          window.open(url, "_blank", "noopener");
        } finally {
          setBusy(false);
        }
      }}
      className="inline-flex items-center gap-1.5 rounded-lg bg-canvas px-2 py-1 text-xs ring-1 ring-line hover:bg-surface"
    >
      <ImageIcon className="h-3.5 w-3.5" aria-hidden /> {a.fileName}
    </button>
  );
}

const AUTHOR_STYLE: Record<CaseRole, string> = {
  CUSTOMER: "bg-surface ring-1 ring-line",
  RESTAURANT: "bg-brand-soft",
  FEAST: "bg-sky-50 ring-1 ring-sky-200",
};

export function CaseThread({ caseId, viewer }: { caseId: string; viewer: CaseRole }) {
  const toast = useToast();
  const q = useQuery(`case:${caseId}`, (api) => api.getCase(caseId));
  const [text, setText] = useState("");
  const [internal, setInternal] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const send = useMutation((api) =>
    api.caseMessage(caseId, {
      text,
      internal: viewer === "FEAST" ? internal : undefined,
    }),
  );
  const escalate = useMutation((api) => api.escalateCase(caseId));
  const settle = useMutation((api) => api.settleCase(caseId));
  const upload = useMutation((api, f: File) => api.addCaseAttachment(caseId, f));

  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const c = q.data;
  const open = c.status !== "RESOLVED";
  const canEscalate = open && c.status !== "WITH_FEAST" && viewer !== "FEAST";
  const err = send.error ?? escalate.error ?? settle.error ?? upload.error;

  return (
    <div className="space-y-4">
      <div className="rounded-2xl bg-surface p-4 ring-1 ring-line">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-xs font-semibold tracking-wide text-muted uppercase">
              {c.kind === "COMPLAINT" ? "Complaint" : "Report about a diner"} · {c.number}
            </p>
            <h2 className="text-xl font-bold">{REASON_LABEL[c.reason]}</h2>
            <p className="text-sm text-muted">
              {c.restaurant.name} · {c.customer.name} · visit {longDate(c.visitDate)}
              {c.amount ? ` · ${money(c.amount, c.currency)}` : ""}
            </p>
          </div>
          <div className="text-right text-sm">
            <CaseStatusBadge c={c} viewer={viewer} />
            {open && <p className="mt-1">{due(c.dueAt)}</p>}
          </div>
        </div>
        {c.outcome && (
          <div className="mt-3 rounded-xl bg-emerald-50 p-3 text-sm text-emerald-900">
            <p className="font-semibold">
              <ShieldCheck className="mr-1 inline h-4 w-4" aria-hidden />
              {DECISION_LABEL[c.outcome.decision]}
              {c.outcome.refundAmount ? ` · ${money(c.outcome.refundAmount, c.currency)}` : ""}
            </p>
            <p>{c.outcome.note}</p>
          </div>
        )}
      </div>

      <ol className="space-y-3" aria-label="Messages">
        {c.messages.map((m) => (
          <li
            key={m.id}
            className={clsx("rounded-2xl p-3", m.internal ? "bg-amber-50 ring-1 ring-amber-200" : m.system ? "bg-canvas text-muted" : AUTHOR_STYLE[m.author])}
          >
            <p className="mb-1 flex flex-wrap items-center gap-2 text-xs text-muted">
              <b className="text-ink">{m.authorName}</b>
              {m.internal && (
                <span className="inline-flex items-center gap-1 rounded-full bg-amber-100 px-2 text-amber-900">
                  <Lock className="h-3 w-3" aria-hidden /> Internal note — only Feast sees this
                </span>
              )}
              <span>{timeAgo(m.at)}</span>
            </p>
            <p className="text-sm whitespace-pre-wrap">{m.text}</p>
          </li>
        ))}
      </ol>

      {c.attachments.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {c.attachments.map((a) => (
            <Attachment key={a.id} caseId={c.id} a={a} />
          ))}
        </div>
      )}

      {err && (
        <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">
          {err.message}
        </p>
      )}

      {open && (
        <div className="space-y-3 rounded-2xl bg-surface p-4 ring-1 ring-line">
          <label className="block">
            <span className="sr-only">Your message</span>
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={3}
              maxLength={2000}
              className={field}
              placeholder={viewer === "FEAST" && internal ? "Note for the Feast team" : "Write a reply"}
            />
          </label>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              disabled={text.trim().length < 2}
              loading={send.pending}
              onClick={async () => {
                const r = await send.run();
                if (r) {
                  setText("");
                  q.reload();
                }
              }}
            >
              <Send className="h-4 w-4" aria-hidden /> {viewer === "FEAST" && internal ? "Add note" : "Send"}
            </Button>
            {viewer === "FEAST" && (
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={internal} onChange={(e) => setInternal(e.target.checked)} /> Internal note
              </label>
            )}
            {viewer !== "FEAST" && c.attachments.length < 3 && (
              <>
                <input
                  ref={fileRef}
                  type="file"
                  accept="image/jpeg,image/png,image/webp"
                  className="hidden"
                  onChange={async (e) => {
                    const f = e.target.files?.[0];
                    e.target.value = "";
                    if (f && (await upload.run(f))) {
                      toast("Photo added");
                      q.reload();
                    }
                  }}
                />
                <Button size="sm" variant="secondary" loading={upload.pending} onClick={() => fileRef.current?.click()}>
                  <Camera className="h-4 w-4" aria-hidden /> Add photo
                </Button>
              </>
            )}
            <span className="flex-1" />
            {viewer === "CUSTOMER" && c.kind === "COMPLAINT" && (
              <Button
                size="sm"
                variant="secondary"
                loading={settle.pending}
                onClick={async () => {
                  if (await settle.run()) {
                    toast("Marked as resolved — thanks for letting us know");
                    q.reload();
                  }
                }}
              >
                <CheckCircle2 className="h-4 w-4" aria-hidden /> It&apos;s sorted
              </Button>
            )}
            {canEscalate && (
              <Button
                size="sm"
                variant="secondary"
                loading={escalate.pending}
                onClick={async () => {
                  if (await escalate.run()) {
                    toast("Feast will look into this");
                    q.reload();
                  }
                }}
              >
                <Scale className="h-4 w-4" aria-hidden /> Ask Feast to step in
              </Button>
            )}
          </div>
        </div>
      )}
      {viewer === "FEAST" && open && <ResolvePanel c={c} onDone={q.reload} />}
    </div>
  );
}

function ResolvePanel({ c, onDone }: { c: Case; onDone: () => void }) {
  const toast = useToast();
  const options = DECISIONS[c.kind];
  const [decision, setDecision] = useState<CaseDecision>(options[0]);
  const [note, setNote] = useState("");
  const [refund, setRefund] = useState("");
  const resolve = useMutation((api) =>
    api.resolveCase(c.id, {
      decision,
      note,
      refundAmount: decision === "REFUND_ADVISED" && refund ? toMinor(refund, c.currency) : undefined,
    }),
  );
  const severe = decision === "RESTAURANT_SUSPENDED" || decision === "CUSTOMER_BLOCKED";
  return (
    <section className="space-y-3 rounded-2xl bg-sky-50 p-4 ring-1 ring-sky-200" aria-label="Decide this case">
      <h3 className="flex items-center gap-2 font-bold">
        <Scale className="h-5 w-5" aria-hidden /> Decide
      </h3>
      <div className="flex flex-wrap gap-2">
        {options.map((d) => (
          <button
            key={d}
            type="button"
            aria-pressed={decision === d}
            onClick={() => setDecision(d)}
            className={clsx("rounded-full px-3 py-1.5 text-sm ring-1", decision === d ? "bg-ink text-white ring-ink" : "bg-surface ring-line hover:bg-canvas")}
          >
            {DECISION_LABEL[d]}
          </button>
        ))}
      </div>
      {decision === "REFUND_ADVISED" && (
        <label className="block max-w-xs">
          <span className="mb-1 block text-sm font-semibold">
            Refund amount
            {c.amount ? ` (paid ${money(c.amount, c.currency)})` : ""}
          </span>
          <input
            value={refund}
            onChange={(e) => setRefund(e.target.value.replace(/[^\d.]/g, ""))}
            inputMode="decimal"
            className={field}
            placeholder={c.amount ? "Leave empty for the full amount" : ""}
          />
        </label>
      )}
      <label className="block">
        <span className="mb-1 block text-sm font-semibold">Note to both sides</span>
        <textarea value={note} onChange={(e) => setNote(e.target.value)} rows={3} className={field} placeholder="What Feast found and why this decision" />
      </label>
      {severe && (
        <p className="flex items-center gap-2 text-sm text-danger">
          <AlertTriangle className="h-4 w-4" aria-hidden />
          {decision === "RESTAURANT_SUSPENDED" ? "The restaurant is hidden from diners immediately." : "The diner's account is blocked immediately."}
        </p>
      )}
      {resolve.error && (
        <p role="alert" className="text-sm text-danger">
          {resolve.error.message}
        </p>
      )}
      <Button
        disabled={note.trim().length < 10}
        loading={resolve.pending}
        onClick={async () => {
          if (await resolve.run()) {
            toast("Case resolved — both sides were notified");
            onDone();
          }
        }}
      >
        Resolve case
      </Button>
    </section>
  );
}

// ================================================================ stars & reviews

export function Stars({ value, size = "sm" }: { value: number; size?: "sm" | "md" }) {
  return (
    <span className="inline-flex" aria-label={`${value} out of 5 stars`}>
      {[1, 2, 3, 4, 5].map((n) => (
        <Star key={n} className={clsx(size === "md" ? "h-5 w-5" : "h-4 w-4", n <= value ? "fill-amber-400 text-amber-400" : "text-faint")} aria-hidden />
      ))}
    </span>
  );
}

export function RateVisitSheet({
  open,
  onClose,
  bookingId,
  billPaymentId,
  restaurantName,
  onDone,
}: {
  open: boolean;
  onClose: () => void;
  bookingId?: string;
  billPaymentId?: string;
  restaurantName: string;
  onDone: (r: Review) => void;
}) {
  const [rating, setRating] = useState(0);
  const [text, setText] = useState("");
  const create = useMutation((api) => api.createReview({ bookingId, billPaymentId, rating, text }));
  const needText = rating > 0 && rating <= 2 && text.trim().length < 10;
  return (
    <Sheet
      open={open}
      onClose={create.pending ? () => {} : onClose}
      title={`Rate ${restaurantName}`}
      footer={
        <Button
          className="w-full"
          disabled={!rating || needText}
          loading={create.pending}
          onClick={async () => {
            const r = await create.run();
            if (r) onDone(r);
          }}
        >
          Post review
        </Button>
      }
    >
      <div className="space-y-4">
        <div className="flex justify-center gap-1" role="radiogroup" aria-label="Rating">
          {[1, 2, 3, 4, 5].map((n) => (
            <button
              key={n}
              type="button"
              role="radio"
              aria-checked={rating === n}
              aria-label={`${n} star${n > 1 ? "s" : ""}`}
              onClick={() => setRating(n)}
              className="p-1"
            >
              <Star className={clsx("h-9 w-9", n <= rating ? "fill-amber-400 text-amber-400" : "text-faint")} />
            </button>
          ))}
        </div>
        <label className="block">
          <span className="mb-1 block text-sm font-semibold">Your review {rating > 0 && rating <= 2 ? "(tell them what went wrong)" : "(optional)"}</span>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={4}
            maxLength={1000}
            className={field}
            placeholder="What did you love? What could be better?"
          />
        </label>
        <p className="text-xs text-muted">
          Shown with your first name and initial. Only diners who visited can review — that&apos;s why Feast reviews can be trusted.
        </p>
        {create.error && (
          <p role="alert" className="text-sm text-danger">
            {create.error.message}
          </p>
        )}
      </div>
    </Sheet>
  );
}

export function ReviewCard({ r, children }: { r: Review; children?: React.ReactNode }) {
  return (
    <article className={clsx("rounded-2xl p-4 ring-1", r.status === "HIDDEN" ? "bg-canvas ring-line opacity-80" : "bg-surface ring-line")}>
      <header className="mb-1 flex flex-wrap items-center gap-2">
        <Stars value={r.rating} />
        <b className="text-sm">{r.authorName}</b>
        <span className="text-xs text-muted">visited {longDate(r.visitDate)}</span>
        {r.status === "HIDDEN" && (
          <span className="inline-flex items-center gap-1 rounded-full bg-red-50 px-2 text-xs font-semibold text-danger">
            <EyeOff className="h-3 w-3" aria-hidden /> Hidden by Feast
          </span>
        )}
        {r.flag && (
          <span className="inline-flex items-center gap-1 rounded-full bg-amber-100 px-2 text-xs font-semibold text-amber-900">
            <Flag className="h-3 w-3" aria-hidden /> Reported
          </span>
        )}
      </header>
      {r.text && <p className="text-sm whitespace-pre-wrap">{r.text}</p>}
      {r.moderation && <p className="mt-2 text-xs text-danger">Hidden: {r.moderation.reason}</p>}
      {r.flag && <p className="mt-1 text-xs text-amber-900">Restaurant&apos;s report: {r.flag.reason}</p>}
      {r.reply && (
        <div className="mt-3 rounded-xl bg-brand-soft p-3 text-sm">
          <p className="mb-0.5 text-xs font-semibold text-brand-strong">Reply from {r.restaurant.name}</p>
          <p className="whitespace-pre-wrap">{r.reply.text}</p>
        </div>
      )}
      {children}
    </article>
  );
}

export function ReviewsSection({ slug }: { slug: string }) {
  const q = useQuery(`reviews:${slug}`, (api) => api.restaurantReviews(slug));
  if (q.error || !q.data) return null;
  const { summary, reviews } = q.data;
  const total = summary.distribution.reduce((a, b) => a + b, 0);
  return (
    <section id="reviews" className="scroll-mt-24 space-y-3">
      <h2 className="text-lg font-bold">Reviews</h2>
      <div className="flex flex-wrap items-center gap-5 rounded-2xl bg-surface p-4 ring-1 ring-line">
        <div className="text-center">
          <p className="text-3xl font-black">{summary.average.toFixed(1)}</p>
          <Stars value={Math.round(summary.average)} />
          <p className="text-xs text-muted">{summary.count.toLocaleString()} ratings</p>
        </div>
        {total > 0 && (
          <div className="min-w-48 flex-1 space-y-1" aria-label="Feast reviews by stars">
            {[5, 4, 3, 2, 1].map((n) => (
              <div key={n} className="flex items-center gap-2 text-xs">
                <span className="w-3">{n}</span>
                <span className="h-2 flex-1 overflow-hidden rounded-full bg-canvas">
                  <span
                    className="block h-full bg-amber-400"
                    style={{
                      width: `${(summary.distribution[n - 1] / total) * 100}%`,
                    }}
                  />
                </span>
                <span className="w-6 text-right text-muted">{summary.distribution[n - 1]}</span>
              </div>
            ))}
          </div>
        )}
        <p className="flex items-center gap-1 text-xs text-muted">
          <ShieldCheck className="h-4 w-4 text-brand" aria-hidden /> Verified: only diners who booked or paid on Feast can review.
        </p>
      </div>
      {reviews.length === 0 ? (
        <p className="text-sm text-muted">No Feast reviews yet — book a table and be the first.</p>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {reviews.map((r) => (
            <ReviewCard key={r.id} r={r} />
          ))}
        </div>
      )}
    </section>
  );
}

// ================================================================ moderation

/** Owner's view of a hidden item/offer: Feast's reason + "ask for review". */
export function ModerationNotice({
  m,
  restaurantId,
  kind,
  id,
  canAppeal,
  onDone,
}: {
  m: Moderation;
  restaurantId: string;
  kind: ContentKind;
  id: string;
  canAppeal: boolean;
  onDone: () => void;
}) {
  const toast = useToast();
  const appeal = useMutation((api) =>
    api.appealModeration(restaurantId, {
      kind,
      id,
      note: "Fixed — please review again.",
    }),
  );
  return (
    <div className="mt-1 rounded-lg bg-red-50 p-2 text-xs text-danger">
      <p className="flex items-center gap-1 font-semibold">
        <EyeOff className="h-3.5 w-3.5" aria-hidden /> Hidden from diners by Feast
      </p>
      <p>{m.reason}</p>
      {m.reviewRequestedAt ? (
        <p className="mt-1 text-muted">You asked for a review {timeAgo(m.reviewRequestedAt)}.</p>
      ) : (
        canAppeal && (
          <button
            className="mt-1 font-semibold underline underline-offset-2"
            disabled={appeal.pending}
            onClick={async () => {
              await appeal.run();
              toast("Feast will take another look");
              onDone();
            }}
          >
            I&apos;ve fixed it — ask Feast to review
          </button>
        )
      )}
    </div>
  );
}

/** Feast staff: hide (with a reason) or restore one menu item / offer. */
export function ModerateButton({
  restaurantId,
  kind,
  id,
  title,
  hidden,
  onDone,
}: {
  restaurantId: string;
  kind: ContentKind;
  id: string;
  title: string;
  hidden: boolean;
  onDone: () => void;
}) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState("");
  const run = useMutation((api, action: "HIDE" | "RESTORE") =>
    api.moderateContent(restaurantId, {
      kind,
      id,
      action,
      reason: action === "HIDE" ? reason : undefined,
    }),
  );
  if (hidden)
    return (
      <Button
        size="sm"
        variant="secondary"
        loading={run.pending}
        onClick={async () => {
          await run.run("RESTORE");
          toast(`${title} is visible again`);
          onDone();
        }}
      >
        <RotateCcw className="h-4 w-4" aria-hidden /> Restore
      </Button>
    );
  return (
    <>
      <Button size="sm" variant="secondary" onClick={() => setOpen(true)} aria-label={`Hide ${title}`}>
        <EyeOff className="h-4 w-4" aria-hidden /> Hide
      </Button>
      <Sheet
        open={open}
        onClose={() => setOpen(false)}
        title={`Hide “${title}” from diners`}
        footer={
          <Button
            className="w-full"
            disabled={reason.trim().length < 10}
            loading={run.pending}
            onClick={async () => {
              await run.run("HIDE");
              if (!run.error) {
                setOpen(false);
                toast(`${title} hidden — the owner was told why`);
                onDone();
              }
            }}
          >
            Hide and tell the owner
          </Button>
        }
      >
        <label className="block">
          <span className="mb-1 block text-sm font-semibold">Reason the owner will see</span>
          <textarea
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={3}
            className={field}
            placeholder="e.g. The discount shown doesn't match the terms"
          />
        </label>
        <p className="mt-2 text-xs text-muted">The owner keeps control of their content: they fix it and ask Feast to review it again.</p>
        {run.error && (
          <p role="alert" className="mt-2 text-sm text-danger">
            {run.error.message}
          </p>
        )}
      </Sheet>
    </>
  );
}
