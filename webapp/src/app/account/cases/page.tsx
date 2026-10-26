"use client";

import Link from "next/link";
import { ArrowLeft } from "lucide-react";
import { useAuth } from "@/components/providers";
import { CaseList, ReviewCard } from "@/components/trust";
import { Empty, ErrorState, LinkButton, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";

export default function MyCasesPage() {
  const { user, ready } = useAuth();
  const cases = useQuery(user ? "myCases" : null, (api) => api.myCases());
  const reviews = useQuery(user ? "myReviews" : null, (api) => api.myReviews());
  if (!ready) return <Skeleton className="mx-auto h-64 max-w-2xl" />;
  if (!user)
    return (
      <div className="mx-auto max-w-md text-center">
        <LinkButton href="/login?next=/account/cases">Sign in to see your reports and reviews</LinkButton>
      </div>
    );
  if (cases.error) return <ErrorState error={cases.error} onRetry={cases.reload} />;
  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <Link href="/account" className="inline-flex items-center gap-1 text-sm font-semibold text-brand">
        <ArrowLeft className="h-4 w-4" aria-hidden /> Account
      </Link>
      <section className="space-y-3">
        <h1 className="text-2xl font-black">Problems & reports</h1>
        <p className="text-sm text-muted">
          Report a problem from any booking or receipt. The restaurant replies within 48 hours; if not, or you&apos;re not happy, Feast steps in.
        </p>
        {!cases.data ? (
          <Skeleton className="h-32 w-full" />
        ) : (
          <CaseList
            cases={cases.data}
            viewer="CUSTOMER"
            hrefFor={(c) => `/account/cases/${c.id}`}
            empty={<Empty title="Nothing reported" body="Hopefully it stays that way!" />}
          />
        )}
      </section>
      <section className="space-y-3">
        <h2 className="text-xl font-black">Your reviews</h2>
        {!reviews.data ? (
          <Skeleton className="h-32 w-full" />
        ) : reviews.data.length === 0 ? (
          <Empty title="No reviews yet" body="After a visit, open the booking or receipt and tap Rate your visit." />
        ) : (
          <div className="space-y-3">
            {reviews.data.map((r) => (
              <ReviewCard key={r.id} r={r}>
                <p className="mt-2 text-xs text-muted">
                  <Link href={`/restaurants/${r.restaurant.slug}#reviews`} className="font-semibold text-brand">
                    {r.restaurant.name}
                  </Link>
                </p>
              </ReviewCard>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
