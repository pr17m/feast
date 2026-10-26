"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { CaseThread } from "@/components/trust";

export default function AdminCase() {
  const { id } = useParams<{ id: string }>();
  return (
    <div className="max-w-3xl space-y-4">
      <Link href="/admin/cases" className="inline-flex items-center gap-1 text-sm font-semibold text-brand">
        <ArrowLeft className="h-4 w-4" aria-hidden /> Resolution desk
      </Link>
      <CaseThread caseId={id} viewer="FEAST" />
    </div>
  );
}
