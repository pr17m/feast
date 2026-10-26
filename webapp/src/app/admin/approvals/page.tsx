"use client";

import { Suspense, useState } from "react";
import { useSearchParams } from "next/navigation";
import clsx from "clsx";
import { Check, ExternalLink, MessageSquareWarning, X } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { StateBadge, fileSize, openDocument } from "@/components/compliance";
import { Button, Empty, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import type { ApprovalQueueItem, ComplianceItem } from "@/lib/api/types";

function ReviewRow({ item, onChanged }: { item: ComplianceItem; onChanged: () => void }) {
  const [rejecting, setRejecting] = useState(false);
  const [note, setNote] = useState("");
  const review = useMutation((api, id: string, d: "APPROVED" | "REJECTED", n?: string) => api.reviewDocument(id, d, n));
  const doc = item.document;
  return (
    <li className="flex flex-wrap items-start gap-3 py-3">
      <div className="min-w-0 flex-1">
        <p className="font-semibold">
          {item.requirement.name}
          {!item.requirement.required && <span className="ml-2 text-xs font-normal text-muted">optional</span>}
        </p>
        {doc ? (
          <p className="text-xs text-muted">
            {doc.fileName} · {fileSize(doc.fileSize)}
            {doc.number && ` · ${item.requirement.numberLabel ?? "No."} ${doc.number}`}
            {doc.expiresOn && ` · valid until ${doc.expiresOn}`}
          </p>
        ) : (
          <p className="text-xs text-muted">Not uploaded yet</p>
        )}
        {doc?.reviewNote && <p className="text-xs text-danger">Note: {doc.reviewNote}</p>}
        {rejecting && (
          <div className="mt-2 flex gap-2">
            <input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Reason (shown to the owner)" aria-label="Rejection reason" className="h-9 flex-1 rounded-lg bg-canvas px-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand" autoFocus />
            <Button
              size="sm"
              className="bg-danger hover:bg-red-800"
              disabled={!note.trim()}
              loading={review.pending}
              onClick={async () => {
                if (doc && (await review.run(doc.id, "REJECTED", note))) {
                  setRejecting(false);
                  onChanged();
                }
              }}
            >
              Reject
            </Button>
          </div>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <StateBadge item={item} />
        {doc && (
          <>
            <Button size="sm" variant="ghost" onClick={() => openDocument(doc.id)} aria-label={`Open ${item.requirement.name}`}>
              <ExternalLink className="h-4 w-4" aria-hidden />
            </Button>
            {item.state !== "APPROVED" && (
              <Button
                size="sm"
                variant="secondary"
                loading={review.pending && !rejecting}
                onClick={async () => (await review.run(doc.id, "APPROVED")) && onChanged()}
                aria-label={`Approve ${item.requirement.name}`}
              >
                <Check className="h-4 w-4 text-veg" aria-hidden /> Approve
              </Button>
            )}
            {item.state !== "REJECTED" && (
              <Button size="sm" variant="ghost" className="text-danger hover:bg-red-50" onClick={() => setRejecting((r) => !r)} aria-label={`Reject ${item.requirement.name}`}>
                <X className="h-4 w-4" aria-hidden />
              </Button>
            )}
          </>
        )}
      </div>
    </li>
  );
}

function Application({ a, open, onToggle, onChanged }: { a: ApprovalQueueItem; open: boolean; onToggle: () => void; onChanged: () => void }) {
  const toast = useToast();
  const [asking, setAsking] = useState(false);
  const [note, setNote] = useState("");
  const approve = useMutation((api) => api.setRestaurantStatus(a.restaurant.id, "LIVE"));
  const changes = useMutation((api, n: string) => api.requestChanges(a.restaurant.id, n));
  const c = a.compliance;
  const req = c.items.filter((i) => i.requirement.required);
  const approved = req.filter((i) => i.state === "APPROVED").length;
  return (
    <section className="rounded-2xl bg-surface ring-1 ring-line">
      <button onClick={onToggle} className="flex w-full flex-wrap items-center gap-3 p-4 text-left" aria-expanded={open}>
        <span className="min-w-0 flex-1">
          <span className="block font-bold">{a.restaurant.name}</span>
          <span className="text-sm text-muted">
            {a.restaurant.area} · {a.restaurant.countryCode} · {a.owners.map((o) => o.name).join(", ") || "no owner yet"}
          </span>
        </span>
        <span className={clsx("rounded-full px-2 py-0.5 text-xs font-bold", a.restaurant.status === "PENDING_APPROVAL" ? "bg-sky-50 text-sky-800" : "bg-stone-100 text-stone-700")}>
          {a.restaurant.status === "PENDING_APPROVAL" ? "Submitted" : a.restaurant.status.replace("_", " ").toLowerCase()}
        </span>
        <span className="text-sm font-semibold">
          {approved}/{req.length} approved
        </span>
      </button>
      {open && (
        <div className="border-t border-line px-4 pb-4">
          <ul className="divide-y divide-line">
            {c.items.map((i) => (
              <ReviewRow key={i.requirement.id} item={i} onChanged={onChanged} />
            ))}
          </ul>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              disabled={!c.complete || a.restaurant.status === "LIVE"}
              loading={approve.pending}
              onClick={async () => {
                if (await approve.run()) {
                  toast(`${a.restaurant.name} is live — free trial started`);
                  onChanged();
                }
              }}
            >
              <Check className="h-4 w-4" aria-hidden /> Approve & go live
            </Button>
            <Button variant="secondary" onClick={() => setAsking(true)}>
              <MessageSquareWarning className="h-4 w-4" aria-hidden /> Request changes
            </Button>
          </div>
          {!c.complete && <p className="mt-2 text-xs text-muted">Every required document must be approved (and unexpired) before the restaurant can go live.</p>}
          {approve.error && <p className="mt-2 text-sm text-danger">{approve.error.message}</p>}
        </div>
      )}
      <Sheet
        open={asking}
        onClose={() => setAsking(false)}
        title="Request changes"
        footer={
          <Button
            className="w-full"
            disabled={!note.trim()}
            loading={changes.pending}
            onClick={async () => {
              if (await changes.run(note)) {
                setAsking(false);
                toast("Sent back to the owner");
                onChanged();
              }
            }}
          >
            Send to owner
          </Button>
        }
      >
        <textarea value={note} onChange={(e) => setNote(e.target.value)} rows={4} placeholder="What does the owner need to fix?" aria-label="Notes for the owner" className="w-full rounded-xl bg-canvas p-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand" />
      </Sheet>
    </section>
  );
}

function ApprovalsInner() {
  const focus = useSearchParams().get("r");
  const q = useQuery("approvals", (api) => api.approvalQueue());
  const [openId, setOpenId] = useState<string | null>(focus);
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const submitted = q.data.filter((a) => a.restaurant.status === "PENDING_APPROVAL" || a.compliance.items.some((i) => i.state === "SUBMITTED"));
  const drafts = q.data.filter((a) => !submitted.includes(a));
  return (
    <div className="max-w-4xl space-y-5">
      <PageHeader title="Approvals" subtitle="Check each licence against the issuing authority, then approve. Restaurants go live only with every required document approved." />
      {q.data.length === 0 && <Empty title="Nothing to review" body="New restaurant applications and renewed licences show up here." />}
      {submitted.length > 0 && (
        <div className="space-y-3">
          <h2 className="text-sm font-bold uppercase tracking-wide text-muted">Waiting for review</h2>
          {submitted.map((a) => (
            <Application key={a.restaurant.id} a={a} open={openId === a.restaurant.id} onToggle={() => setOpenId(openId === a.restaurant.id ? null : a.restaurant.id)} onChanged={q.reload} />
          ))}
        </div>
      )}
      {drafts.length > 0 && (
        <div className="space-y-3">
          <h2 className="text-sm font-bold uppercase tracking-wide text-muted">Still onboarding</h2>
          {drafts.map((a) => (
            <Application key={a.restaurant.id} a={a} open={openId === a.restaurant.id} onToggle={() => setOpenId(openId === a.restaurant.id ? null : a.restaurant.id)} onChanged={q.reload} />
          ))}
        </div>
      )}
    </div>
  );
}

export default function AdminApprovals() {
  return (
    <Suspense fallback={<Skeleton className="h-96 w-full" />}>
      <ApprovalsInner />
    </Suspense>
  );
}
