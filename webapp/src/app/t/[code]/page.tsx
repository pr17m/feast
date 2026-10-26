"use client";

import { useParams } from "next/navigation";
import { QrLanding } from "@/components/qr-landing";

/** QR codes printed for single tables before table ordering was removed: they open the restaurant. */
export default function OldTableQrPage() {
  const { code } = useParams<{ code: string }>();
  return <QrLanding code={code} />;
}
