"use client";

import { AuditExplorer } from "@/components/audit";
import { PageHeader } from "@/components/portal-shell";
import { usePartner } from "../partner-context";

export default function PartnerActivity() {
  const { restaurant } = usePartner();
  return (
    <div className="space-y-5">
      <PageHeader
        title="Activity log"
        subtitle="Every booking, payment, cancellation, order, dispute and change at your restaurant: who did it and when. Records can't be edited or deleted."
      />
      <AuditExplorer scopeKey={restaurant.id} load={(api, f) => api.restaurantAudit(restaurant.id, f)} />
    </div>
  );
}
