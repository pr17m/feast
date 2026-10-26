"use client";

import clsx from "clsx";
import { Globe, ScanLine } from "lucide-react";
import { useRouter } from "next/navigation";
import { useEffect, useState, useSyncExternalStore } from "react";
import { useToast } from "@/components/providers";
import { Button } from "@/components/ui";
import { changeCountry, feastQrPath, hasNativePlugin, initNativeApp, openFeastLink, scanQrCode } from "@/lib/native";
import { isNativeApp } from "@/lib/platform";

/** Mounted once in the root layout: push, notification taps and Feast links in the iOS/Android apps. */
export function NativeAppBridge() {
  const router = useRouter();
  useEffect(() => initNativeApp((path) => router.push(path)), [router]);
  return null;
}

/** True only inside the iOS/Android apps, after hydration (so server and first client render match). */
export function useIsNativeApp(): boolean {
  return useSyncExternalStore(noSubscribe, isNativeApp, () => false);
}
const noSubscribe = () => () => {};

/** "Scan restaurant QR" — only in the apps (in a browser, the phone's camera app does this). */
export function ScanQrButton({ className, variant = "primary" }: { className?: string; variant?: "primary" | "secondary" }) {
  const native = useIsNativeApp();
  const router = useRouter();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  if (!native || !hasNativePlugin("BarcodeScanner")) return null;
  return (
    <Button
      variant={variant}
      size="lg"
      className={className}
      loading={busy}
      onClick={async () => {
        setBusy(true);
        try {
          const res = await scanQrCode();
          if (res.ok) {
            const path = feastQrPath(res.value);
            if (path) router.push(path);
            else if (!openFeastLink(res.value, (p) => router.push(p))) toast("That isn't a Feast QR code", "error");
          } else if (res.reason === "preparing" || res.reason === "error") {
            toast(res.message ?? "Couldn't open the scanner", "error");
          }
        } finally {
          setBusy(false);
        }
      }}
    >
      <ScanLine className="h-5 w-5" aria-hidden /> Scan restaurant QR
    </Button>
  );
}

/** Back to the app's country picker (only in the apps). */
export function ChangeCountryButton({ className, compact }: { className?: string; compact?: boolean }) {
  const native = useIsNativeApp();
  if (!native) return null;
  return (
    <button type="button" onClick={changeCountry} className={clsx("inline-flex items-center gap-1.5", className)} aria-label="Change country">
      <Globe className={compact ? "h-5 w-5" : "h-4 w-4"} aria-hidden />
      {!compact && "Change country"}
    </button>
  );
}
