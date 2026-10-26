"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { useAuth } from "@/components/providers";
import { CaseThread } from "@/components/trust";
import { LinkButton, Skeleton } from "@/components/ui";

export default function MyCasePage() {
  const { id } = useParams<{ id: string }>();
  const { user, ready } = useAuth();
  if (!ready) return <Skeleton className="mx-auto h-64 max-w-2xl" />;
  if (!user)
    return (
      <div className="mx-auto max-w-md text-center">
        <LinkButton href={`/login?next=/account/cases/${id}`}>Sign in to see this case</LinkButton>
      </div>
    );
  return (
    <div className="mx-auto max-w-2xl space-y-4">
      <Link href="/account/cases" className="inline-flex items-center gap-1 text-sm font-semibold text-brand">
        <ArrowLeft className="h-4 w-4" aria-hidden /> Problems & reports
      </Link>
      <CaseThread caseId={id} viewer="CUSTOMER" />
    </div>
  );
}
