"use client";

import { PageHeader } from "@/components/portal-shell";
import { CaseList } from "@/components/trust";
import { Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { usePartner } from "../partner-context";

export default function PartnerCases() {
  const { restaurant, href } = usePartner();
  const q = useQuery(`pCases:${restaurant.id}`, (api) => api.restaurantCases(restaurant.id));
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-64 w-full" />;
  const open = q.data.filter((c) => c.status !== "RESOLVED");
  const yourTurn = open.filter((c) => c.status === "AWAITING_RESTAURANT").length;
  return (
    <div className="max-w-3xl space-y-5">
      <PageHeader
        title="Issues & disputes"
        subtitle={
          yourTurn
            ? `${yourTurn} complaint${yourTurn === 1 ? "" : "s"} waiting for your reply — answer within 48 hours or Feast steps in.`
            : "Diners' complaints come here first, so you can put things right. Report a no-show or a difficult diner from Bookings."
        }
      />
      <CaseList
        cases={q.data}
        viewer="RESTAURANT"
        hrefFor={(c) => href(`/partner/cases/${c.id}`)}
        empty={<Empty title="No issues raised" body="When a diner reports a problem, you'll be notified and can reply here." />}
      />
    </div>
  );
}
