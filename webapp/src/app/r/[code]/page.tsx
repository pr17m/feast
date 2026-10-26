"use client";

import { useParams } from "next/navigation";
import { QrLanding } from "@/components/qr-landing";

/** The restaurant's printed QR code. */
export default function RestaurantQrPage() {
  const { code } = useParams<{ code: string }>();
  return <QrLanding code={code} />;
}
