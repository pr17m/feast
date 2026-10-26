"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { CaseThread } from "@/components/trust";
import { usePartner } from "../../partner-context";

export default function PartnerCase() {
  const { id } = useParams<{ id: string }>();
  const { href, isStaff } = usePartner();
  return (
    <div className="max-w-3xl space-y-4">
      <Link href={href("/partner/cases")} className="inline-flex items-center gap-1 text-sm font-semibold text-brand">
        <ArrowLeft className="h-4 w-4" aria-hidden /> Issues & disputes
      </Link>
      <CaseThread caseId={id} viewer={isStaff ? "FEAST" : "RESTAURANT"} />
    </div>
  );
}
