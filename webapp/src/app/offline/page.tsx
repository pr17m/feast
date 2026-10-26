import type { Metadata } from "next";
import { WifiOff } from "lucide-react";

export const metadata: Metadata = { title: "Offline" };

/** Shown by the service worker when a page isn't available offline. Static: no data, no API calls. */
export default function OfflinePage() {
  return (
    <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-16 text-center">
      <WifiOff className="h-12 w-12 text-muted" aria-hidden />
      <h1 className="text-2xl font-black tracking-tight">You&apos;re offline</h1>
      <p className="text-muted">Your bookings will show when you&apos;re back online. Check your connection, then try again.</p>
      {/* eslint-disable-next-line @next/next/no-html-link-for-pages -- a full page load, so the browser retries the network */}
      <a href="/" className="mt-2 rounded-xl bg-brand px-4 py-2.5 font-semibold text-white">
        Try again
      </a>
    </div>
  );
}
