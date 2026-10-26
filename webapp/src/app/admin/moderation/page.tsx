"use client";

import Link from "next/link";
import { PageHeader } from "@/components/portal-shell";
import { ModerateButton } from "@/components/trust";
import { Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { timeAgo } from "@/lib/format";

export default function ModerationQueue() {
  const q = useQuery("moderationQueue", (api) => api.moderationQueue());
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const appeals = q.data.filter((m) => m.moderation.reviewRequestedAt).length;
  return (
    <div className="max-w-3xl space-y-5">
      <PageHeader
        title="Moderation"
        subtitle={`${q.data.length} item${q.data.length === 1 ? "" : "s"} hidden from diners${appeals ? ` · ${appeals} waiting for your review` : ""}. Restaurants own their content: Feast hides what breaks the rules, the owner fixes it.`}
      />
      {q.data.length === 0 ? (
        <Empty title="Nothing hidden" body="Hide a menu item or offer from the restaurant's Menu or Offers page in the partner portal." />
      ) : (
        <ul className="divide-y divide-line overflow-hidden rounded-2xl bg-surface ring-1 ring-line">
          {q.data.map((m) => (
            <li key={`${m.restaurant.id}:${m.id}`} className="flex flex-wrap items-center gap-3 p-4">
              <div className="min-w-0 flex-1">
                <p className="font-semibold">
                  {m.title} <span className="font-normal text-muted">· {m.kind === "MENU_ITEM" ? "menu item" : "offer"}</span>
                </p>
                <p className="text-sm text-muted">
                  <Link href={`/partner/${m.kind === "MENU_ITEM" ? "menu" : "offers"}?r=${m.restaurant.id}`} className="font-semibold text-brand">
                    {m.restaurant.name}
                  </Link>{" "}
                  · hidden {timeAgo(m.moderation.hiddenAt)}: {m.moderation.reason}
                </p>
                {m.moderation.reviewRequestedAt && (
                  <p className="mt-1 text-sm text-amber-900">
                    Owner asked for a review {timeAgo(m.moderation.reviewRequestedAt)}
                    {m.moderation.reviewNote ? `: “${m.moderation.reviewNote}”` : ""}
                  </p>
                )}
              </div>
              <ModerateButton restaurantId={m.restaurant.id} kind={m.kind} id={m.id} title={m.title} hidden onDone={q.reload} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
