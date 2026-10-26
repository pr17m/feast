"use client";

import Link from "next/link";
import { useState } from "react";
import { EyeOff, RotateCcw } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { ReviewCard } from "@/components/trust";
import { Button, Empty, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import type { Review } from "@/lib/api/types";

const field = "w-full rounded-xl bg-canvas px-3 py-2 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

export default function AdminReviews() {
  const toast = useToast();
  const q = useQuery("adminReviews", (api) => api.adminReviews());
  const [hiding, setHiding] = useState<Review | null>(null);
  const [reason, setReason] = useState("");
  const mod = useMutation((api, id: string, action: "HIDE" | "RESTORE") =>
    api.moderateReview(id, {
      action,
      reason: action === "HIDE" ? reason : undefined,
    }),
  );
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const reported = q.data.filter((r) => r.flag).length;
  return (
    <div className="max-w-3xl space-y-5">
      <PageHeader
        title="Reviews"
        subtitle={`${reported} reported by restaurants · hide only reviews that break the guidelines (abuse, fake visit, wrong restaurant). Honest bad reviews stay.`}
      />
      {q.data.length === 0 && <Empty title="Nothing to check" body="Reviews restaurants report, and reviews Feast has hidden, show up here." />}
      <div className="space-y-3">
        {q.data.map((r) => (
          <ReviewCard key={r.id} r={r}>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <Link href={`/restaurants/${r.restaurant.slug}#reviews`} className="text-sm font-semibold text-brand">
                {r.restaurant.name}
              </Link>
              <span className="flex-1" />
              {r.status === "PUBLISHED" ? (
                <>
                  {r.flag && (
                    <Button
                      size="sm"
                      variant="secondary"
                      loading={mod.pending}
                      onClick={async () => {
                        if (await mod.run(r.id, "RESTORE")) {
                          toast("Kept visible; the report is closed");
                          q.reload();
                        }
                      }}
                    >
                      Keep visible
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      setReason("");
                      setHiding(r);
                    }}
                  >
                    <EyeOff className="h-4 w-4" aria-hidden /> Hide
                  </Button>
                </>
              ) : (
                <Button
                  size="sm"
                  variant="secondary"
                  loading={mod.pending}
                  onClick={async () => {
                    if (await mod.run(r.id, "RESTORE")) {
                      toast("Review restored");
                      q.reload();
                    }
                  }}
                >
                  <RotateCcw className="h-4 w-4" aria-hidden /> Restore
                </Button>
              )}
            </div>
          </ReviewCard>
        ))}
      </div>
      <Sheet
        open={!!hiding}
        onClose={() => setHiding(null)}
        title="Hide this review"
        footer={
          <Button
            className="w-full"
            disabled={reason.trim().length < 10}
            loading={mod.pending}
            onClick={async () => {
              if (hiding && (await mod.run(hiding.id, "HIDE"))) {
                setHiding(null);
                toast("Hidden — the diner and the restaurant were told why");
                q.reload();
              }
            }}
          >
            Hide review
          </Button>
        }
      >
        <label className="block">
          <span className="mb-1 block text-sm font-semibold">Reason (shown to the diner and the restaurant)</span>
          <textarea value={reason} onChange={(e) => setReason(e.target.value)} rows={3} className={field} placeholder="e.g. The visit couldn't be verified" />
        </label>
        {mod.error && (
          <p role="alert" className="mt-2 text-sm text-danger">
            {mod.error.message}
          </p>
        )}
      </Sheet>
    </div>
  );
}
