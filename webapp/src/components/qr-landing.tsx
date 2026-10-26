"use client";

import Link from "next/link";
import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { Empty, ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { ApiError } from "@/lib/api/types";

/**
 * Where a scanned restaurant QR lands (`/r/{code}`, and old table codes at
 * `/t/{code}`): resolve it and open the restaurant's page, which offers
 * "Pay bill & get your deal" to a diner with a Feast booking there today and
 * "Book a table" to everyone else.
 */
export function QrLanding({ code }: { code: string }) {
  const router = useRouter();
  const qr = useQuery(`qr:${code}`, (api) => api.getRestaurantQr(code));
  const slug = qr.data?.restaurant.slug;
  useEffect(() => {
    if (slug) router.replace(`/restaurants/${slug}?via=qr`);
  }, [slug, router]);

  if (qr.error instanceof ApiError && qr.error.status === 404)
    return (
      <div className="mx-auto max-w-md">
        <Empty
          title="This QR code isn't linked to a restaurant"
          body="It may be old or from a restaurant that's no longer on Feast."
          action={
            <Link href="/" className="font-semibold text-brand">
              Find restaurants
            </Link>
          }
        />
      </div>
    );
  if (qr.error) return <ErrorState error={qr.error} onRetry={qr.reload} />;
  return (
    <div aria-busy="true" aria-label="Opening the restaurant">
      <Skeleton className="mx-auto h-64 max-w-xl" />
    </div>
  );
}
