"use client";

import Link from "next/link";
import { QrCode, Receipt, Search } from "lucide-react";
import { useCity } from "@/components/providers";
import { HomeSections } from "@/components/home-sections";
import { ScanQrButton } from "@/components/native-app";
import { ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";

export default function HomePage() {
  const { cityId, ready } = useCity();
  const home = useQuery(ready ? `home:${cityId}` : null, (api) => api.getHome(cityId));

  return (
    <div className="space-y-6">
      <Link
        href="/search"
        className="flex h-12 items-center gap-2 rounded-xl bg-surface px-4 text-muted ring-1 ring-line md:hidden"
      >
        <Search className="h-4 w-4" aria-hidden />
        Search restaurants, cuisines, dishes
      </Link>

      <div className="grid grid-cols-2 gap-3">
        <Link href="/dine-in" className="flex flex-col items-start gap-2 rounded-2xl bg-surface p-3 ring-1 ring-line hover:shadow-[var(--shadow-card)] sm:flex-row sm:items-center sm:gap-3 sm:p-4">
          <span className="grid h-10 w-10 place-items-center rounded-xl bg-brand-soft text-brand">
            <QrCode className="h-5 w-5" aria-hidden />
          </span>
          <span>
            <span className="block text-sm font-bold sm:text-base">Arrived?</span>
            <span className="text-xs text-muted sm:text-sm">Tap &ldquo;I&apos;m here&rdquo; for your booking</span>
          </span>
        </Link>
        <Link href="/dine-in" className="flex flex-col items-start gap-2 rounded-2xl bg-surface p-3 ring-1 ring-line hover:shadow-[var(--shadow-card)] sm:flex-row sm:items-center sm:gap-3 sm:p-4">
          <span className="grid h-10 w-10 place-items-center rounded-xl bg-deal-soft text-deal">
            <Receipt className="h-5 w-5" aria-hidden />
          </span>
          <span>
            <span className="block text-sm font-bold sm:text-base">Booked today?</span>
            <span className="text-xs text-muted sm:text-sm">Pay in the app to get your deal</span>
          </span>
        </Link>
      </div>
      <ScanQrButton className="w-full" variant="secondary" />

      {home.error && <ErrorState error={home.error} onRetry={home.reload} />}
      {!home.data && !home.error && (
        <div className="space-y-6" aria-busy="true" aria-label="Loading">
          <Skeleton className="h-48 w-full rounded-3xl" />
          <div className="flex gap-3">
            {Array.from({ length: 4 }, (_, i) => (
              <Skeleton key={i} className="h-28 w-32 shrink-0" />
            ))}
          </div>
          <div className="grid gap-3 md:grid-cols-3">
            {Array.from({ length: 3 }, (_, i) => (
              <Skeleton key={i} className="h-64" />
            ))}
          </div>
        </div>
      )}
      {home.data && <HomeSections sections={home.data.sections} />}
    </div>
  );
}
