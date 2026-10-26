"use client";

import { useState } from "react";
import clsx from "clsx";
import { ExternalLink, FileUp, Send, ShieldCheck } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { StateBadge, fileSize, openDocument } from "@/components/compliance";
import { Button, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import type { ComplianceItem, UploadDocumentRequest } from "@/lib/api/types";
import { usePartner } from "../partner-context";

const field = "h-11 w-full rounded-xl bg-canvas px-3 ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

function UploadSheet({ item, restaurantId, onClose, onDone }: { item: ComplianceItem | null; restaurantId: string; onClose: () => void; onDone: () => void }) {
  const [file, setFile] = useState<File | null>(null);
  const [number, setNumber] = useState("");
  const [expiresOn, setExpiresOn] = useState("");
  const upload = useMutation((api, req: UploadDocumentRequest) => api.uploadDocument(restaurantId, req));
  const req = item?.requirement;
  const ok = !!file && (!req?.numberLabel || number.trim()) && (!req?.hasExpiry || expiresOn);
  return (
    <Sheet
      open={!!item}
      onClose={upload.pending ? () => {} : onClose}
      title={req ? `Upload ${req.name}` : ""}
      footer={
        <Button
          className="w-full"
          loading={upload.pending}
          disabled={!ok}
          onClick={async () => {
            if (!file || !req) return;
            const res = await upload.run({ requirementId: req.id, file, number: number.trim() || undefined, expiresOn: expiresOn || undefined });
            if (res) onDone();
          }}
        >
          <FileUp className="h-4 w-4" aria-hidden /> Upload for review
        </Button>
      }
    >
      {req && (
        <div className="space-y-3">
          <p className="text-sm text-muted">
            {req.description}
            {req.issuer && <span className="block">Issued by: {req.issuer}</span>}
          </p>
          <label className="block">
            <span className="mb-1 block text-sm font-medium">File (PDF, JPG, PNG · max 10 MB)</span>
            <input type="file" accept="application/pdf,image/jpeg,image/png,image/webp" onChange={(e) => setFile(e.target.files?.[0] ?? null)} className="block w-full text-sm" aria-label="Document file" />
          </label>
          {req.numberLabel && (
            <label className="block">
              <span className="mb-1 block text-sm font-medium">{req.numberLabel}</span>
              <input value={number} onChange={(e) => setNumber(e.target.value)} className={field} />
            </label>
          )}
          {req.hasExpiry && (
            <label className="block">
              <span className="mb-1 block text-sm font-medium">Valid until</span>
              <input type="date" value={expiresOn} onChange={(e) => setExpiresOn(e.target.value)} className={field} />
            </label>
          )}
          {upload.error && <p className="text-sm text-danger">{upload.error.message}</p>}
        </div>
      )}
    </Sheet>
  );
}

export default function PartnerDocuments() {
  const { restaurant, reloadRestaurants } = usePartner();
  const toast = useToast();
  const q = useQuery(`compliance:${restaurant.id}`, (api) => api.compliance(restaurant.id));
  const [uploading, setUploading] = useState<ComplianceItem | null>(null);
  const submit = useMutation((api) => api.submitForReview(restaurant.id));

  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const c = q.data;
  const required = c.items.filter((i) => i.requirement.required);
  const done = required.filter((i) => i.state === "APPROVED").length;
  const onboarding = c.restaurantStatus === "ONBOARDING";

  return (
    <div className="max-w-3xl space-y-5">
      <PageHeader
        title="Documents & licences"
        subtitle={`What ${c.countryCode === "AU" ? "Australian" : c.countryCode === "IN" ? "Indian" : "local"} law requires to list a restaurant. Feast reviews each one; your listing goes live once every required document is approved.`}
      />

      <div className="rounded-2xl bg-surface p-4 ring-1 ring-line">
        <div className="mb-2 flex items-center justify-between text-sm font-semibold">
          <span>
            {done} of {required.length} required documents approved
          </span>
          <span className="text-muted">{c.restaurantStatus.replace("_", " ").toLowerCase()}</span>
        </div>
        <div className="h-2 overflow-hidden rounded-full bg-canvas">
          <div className="h-full rounded-full bg-brand transition-all" style={{ width: `${required.length ? (done / required.length) * 100 : 0}%` }} />
        </div>
        {c.reviewNotes && onboarding && (
          <p className="mt-3 rounded-xl bg-amber-50 p-3 text-sm text-amber-900">
            <b>Feast asked for changes:</b> {c.reviewNotes}
          </p>
        )}
        {c.restaurantStatus === "PENDING_APPROVAL" && <p className="mt-3 rounded-xl bg-sky-50 p-3 text-sm text-sky-900">Submitted — Feast usually reviews within 2 working days.</p>}
      </div>

      <ul className="space-y-2">
        {c.items.map((item) => (
          <li key={item.requirement.id} className={clsx("rounded-2xl bg-surface p-4 ring-1", item.state === "REJECTED" || item.state === "EXPIRED" ? "ring-danger/40" : "ring-line")}>
            <div className="flex flex-wrap items-start gap-3">
              <div className="min-w-0 flex-1">
                <p className="font-semibold">
                  {item.requirement.name}
                  {!item.requirement.required && <span className="ml-2 text-xs font-normal text-muted">optional</span>}
                </p>
                <p className="text-sm text-muted">{item.requirement.description}</p>
                {item.document && (
                  <p className="mt-1 text-xs text-muted">
                    {item.document.fileName} · {fileSize(item.document.fileSize)}
                    {item.document.number && ` · ${item.document.number}`}
                    {item.document.expiresOn && ` · valid until ${item.document.expiresOn}`}
                  </p>
                )}
                {item.state === "REJECTED" && item.document?.reviewNote && <p className="mt-1 text-sm text-danger">Rejected: {item.document.reviewNote}</p>}
              </div>
              <div className="flex shrink-0 flex-col items-end gap-2">
                <StateBadge item={item} />
                <div className="flex gap-2">
                  {item.document && (
                    <Button size="sm" variant="ghost" onClick={() => openDocument(item.document!.id)} aria-label={`View ${item.requirement.name}`}>
                      <ExternalLink className="h-4 w-4" aria-hidden />
                    </Button>
                  )}
                  <Button size="sm" variant={item.state === "MISSING" || item.state === "REJECTED" || item.state === "EXPIRED" ? "primary" : "secondary"} onClick={() => setUploading(item)}>
                    <FileUp className="h-4 w-4" aria-hidden /> {item.document ? "Replace" : "Upload"}
                  </Button>
                </div>
              </div>
            </div>
          </li>
        ))}
      </ul>

      {onboarding && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl bg-brand-soft p-4">
          <p className="flex items-center gap-2 text-sm text-brand-strong">
            <ShieldCheck className="h-5 w-5" aria-hidden />
            {c.readyToSubmit ? "Everything required is uploaded. Submit for Feast's review." : "Upload every required document to submit."}
          </p>
          <Button
            disabled={!c.readyToSubmit}
            loading={submit.pending}
            onClick={async () => {
              if (await submit.run()) {
                toast("Submitted for review");
                q.reload();
                reloadRestaurants();
              }
            }}
          >
            <Send className="h-4 w-4" aria-hidden /> Submit for review
          </Button>
        </div>
      )}
      {submit.error && <p className="text-sm text-danger">{submit.error.message}</p>}

      <UploadSheet
        key={uploading?.requirement.id ?? "none"}
        item={uploading}
        restaurantId={restaurant.id}
        onClose={() => setUploading(null)}
        onDone={() => {
          setUploading(null);
          toast("Uploaded — Feast will review it");
          q.reload();
        }}
      />
    </div>
  );
}
