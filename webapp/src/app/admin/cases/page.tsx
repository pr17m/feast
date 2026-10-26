"use client";

import Link from "next/link";
import { useState } from "react";
import clsx from "clsx";
import { AlertTriangle } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { CaseStatusBadge, REASON_LABEL } from "@/components/trust";
import { Chip, Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { longDate, timeAgo } from "@/lib/format";

type Filter = "decide" | "open" | "resolved" | "all";

export default function ResolutionDesk() {
  const q = useQuery("adminCases", (api) => api.adminCases());
  const [filter, setFilter] = useState<Filter>("decide");
  if (q.error) return <ErrorState error={q.error} onRetry={q.reload} />;
  if (!q.data) return <Skeleton className="h-96 w-full" />;
  const items = q.data.filter(({ case: c }) =>
    filter === "decide" ? c.status === "WITH_FEAST" : filter === "open" ? c.status !== "RESOLVED" : filter === "resolved" ? c.status === "RESOLVED" : true,
  );
  const decide = q.data.filter((i) => i.case.status === "WITH_FEAST").length;
  const overdue = q.data.filter((i) => i.overdue).length;
  return (
    <div className="space-y-5">
      <PageHeader
        title="Resolution desk"
        subtitle={`${decide} case${decide === 1 ? "" : "s"} waiting for Feast's decision${overdue ? ` · ${overdue} overdue` : ""} · target: decide within 72 hours`}
      />
      <div className="flex flex-wrap gap-2">
        {(
          [
            ["decide", "Needs decision"],
            ["open", "All open"],
            ["resolved", "Resolved"],
            ["all", "Everything"],
          ] as [Filter, string][]
        ).map(([k, label]) => (
          <Chip key={k} active={filter === k} onClick={() => setFilter(k)}>
            {label}
          </Chip>
        ))}
      </div>
      {items.length === 0 ? (
        <Empty title="Nothing here" body="Cases escalated by diners or restaurants, and reports about diners, land here." />
      ) : (
        <ul className="divide-y divide-line overflow-hidden rounded-2xl bg-surface ring-1 ring-line">
          {items.map(({ case: c, overdue, restaurantComplaints, customerReports }) => (
            <li key={c.id}>
              <Link href={`/admin/cases/${c.id}`} className={clsx("flex flex-wrap items-center gap-3 p-4 hover:bg-canvas", overdue && "bg-red-50/50")}>
                <div className="min-w-0 flex-1">
                  <p className="font-semibold">
                    {c.kind === "COMPLAINT" ? "Complaint" : "Report"} · {REASON_LABEL[c.reason]} <span className="font-normal text-muted">· {c.number}</span>
                  </p>
                  <p className="truncate text-sm text-muted">
                    {c.restaurant.name} ({restaurantComplaints} complaint
                    {restaurantComplaints === 1 ? "" : "s"}) · {c.customer.name} ({customerReports} report
                    {customerReports === 1 ? "" : "s"}) · visit {longDate(c.visitDate)} · updated {timeAgo(c.updatedAt)}
                  </p>
                </div>
                {overdue && (
                  <span className="inline-flex items-center gap-1 text-xs font-semibold text-danger">
                    <AlertTriangle className="h-3.5 w-3.5" aria-hidden /> Overdue
                  </span>
                )}
                <CaseStatusBadge c={c} viewer="FEAST" />
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
