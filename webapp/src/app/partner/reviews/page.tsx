"use client";

import { useState } from "react";
import { Flag, MessageCircleReply } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { ReviewCard, Stars } from "@/components/trust";
import { Button, Empty, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import type { Review } from "@/lib/api/types";
import { usePartner } from "../partner-context";

const field = "w-full rounded-xl bg-canvas px-3 py-2 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

export default function PartnerReviews() {
  const { restaurant, canEdit, isStaff } = usePartner();
  const toast = useToast();
  const q = useQuery(`pReviews:${restaurant.id}`, (api) => api.partnerReviews(restaurant.id));
  const [replying, setReplying] = useState<Review | null>(null);
  const [reporting, setReporting] = useState<Review | null>(null);
  const [text, setText] = useState("");
  const reply = useMutation((api, id: string) => api.replyReview(id, text));
  const report = useMutation((api, id: string) => api.reportReview(id, text));

  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-64 w-full" />;
  const { summary, reviews } = q.data;
  const unanswered = reviews.filter((r) => !r.reply && r.status === "PUBLISHED").length;

  return (
    <div className="max-w-3xl space-y-5">
      <PageHeader
        title="Reviews"
        subtitle={`${summary.average.toFixed(1)}★ from ${summary.count.toLocaleString()} ratings${unanswered ? ` · ${unanswered} without a reply` : ""} · only diners who visited through Feast can review`}
      />
      {reviews.length === 0 && <Empty title="No Feast reviews yet" body="Diners can review after a completed booking or a bill paid on Feast." />}
      <div className="space-y-3">
        {reviews.map((r) => (
          <ReviewCard key={r.id} r={r}>
            <div className="mt-3 flex flex-wrap gap-2">
              {canEdit && (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => {
                    setText(r.reply?.text ?? "");
                    setReplying(r);
                  }}
                >
                  <MessageCircleReply className="h-4 w-4" aria-hidden /> {r.reply ? "Edit reply" : "Reply"}
                </Button>
              )}
              {!isStaff && !r.flag && r.status === "PUBLISHED" && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setText("");
                    setReporting(r);
                  }}
                >
                  <Flag className="h-4 w-4" aria-hidden /> Report to Feast
                </Button>
              )}
            </div>
          </ReviewCard>
        ))}
      </div>

      <Sheet
        open={!!replying}
        onClose={() => setReplying(null)}
        title="Reply publicly"
        footer={
          <Button
            className="w-full"
            disabled={text.trim().length < 2}
            loading={reply.pending}
            onClick={async () => {
              if (replying && (await reply.run(replying.id))) {
                setReplying(null);
                toast("Reply posted");
                q.reload();
              }
            }}
          >
            Post reply
          </Button>
        }
      >
        {replying && (
          <div className="space-y-3">
            <div className="flex items-center gap-2 text-sm">
              <Stars value={replying.rating} /> <b>{replying.authorName}</b>
            </div>
            <p className="text-sm text-muted">{replying.text || "No written review."}</p>
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={4}
              maxLength={600}
              className={field}
              placeholder="Thank them, or explain what you've changed. Everyone can read this."
            />
            {reply.error && (
              <p role="alert" className="text-sm text-danger">
                {reply.error.message}
              </p>
            )}
          </div>
        )}
      </Sheet>

      <Sheet
        open={!!reporting}
        onClose={() => setReporting(null)}
        title="Report this review to Feast"
        footer={
          <Button
            className="w-full"
            disabled={text.trim().length < 10}
            loading={report.pending}
            onClick={async () => {
              if (reporting && (await report.run(reporting.id))) {
                setReporting(null);
                toast("Feast will check this review");
                q.reload();
              }
            }}
          >
            Send report
          </Button>
        }
      >
        <div className="space-y-3">
          <p className="text-sm text-muted">
            Feast hides reviews that are abusive, about the wrong restaurant, or from a visit that didn&apos;t happen. Bad but honest reviews stay — reply to
            them instead.
          </p>
          <textarea value={text} onChange={(e) => setText(e.target.value)} rows={3} className={field} placeholder="What's wrong with this review?" />
          {report.error && (
            <p role="alert" className="text-sm text-danger">
              {report.error.message}
            </p>
          )}
        </div>
      </Sheet>
    </div>
  );
}
