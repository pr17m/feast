"use client";

import { AuditExplorer } from "@/components/audit";
import { PageHeader } from "@/components/portal-shell";

export default function AdminAudit() {
  return (
    <div className="space-y-5">
      <PageHeader title="Audit trail" subtitle="Every state change across Feast, newest first, with who made it (user, payment provider or background job), from where, and when. Append-only." />
      <AuditExplorer scopeKey="all" admin load={(api, f) => api.adminAudit(f)} />
    </div>
  );
}
