"use client";

import { useEffect } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Skeleton } from "@/components/ui";

/** Old "Tables & QR" bookmarks: the restaurant now has one QR code. */
export default function TablesMoved() {
  const router = useRouter();
  const params = useSearchParams();
  useEffect(() => {
    const r = params.get("r");
    router.replace(r ? `/partner/qr?r=${encodeURIComponent(r)}` : "/partner/qr");
  }, [params, router]);
  return <Skeleton className="h-40 w-full" />;
}
