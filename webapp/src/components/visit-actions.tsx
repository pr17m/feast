"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { MessageSquareWarning, Star } from "lucide-react";
import { useToast } from "@/components/providers";
import { RateVisitSheet, ReportProblemSheet, Stars } from "@/components/trust";
import { Button } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { longDate } from "@/lib/format";

/**
 * After a visit: rate it (verified review) or report a problem to the restaurant.
 * Shows the existing review / open case instead when there is one.
 */
export function VisitActions({
  bookingId,
  billPaymentId,
  restaurantName,
  visitDate,
  canReview,
}: {
  bookingId?: string;
  billPaymentId?: string;
  restaurantName: string;
  visitDate: string;
  canReview: boolean;
}) {
  const toast = useToast();
  const router = useRouter();
  const reviews = useQuery("myReviews", (api) => api.myReviews());
  const cases = useQuery("myCases", (api) => api.myCases());
  const [rating, setRating] = useState(false);
  const [reporting, setReporting] = useState(false);
  const mine = (x: { bookingId?: string; billPaymentId?: string }) =>
    (bookingId && x.bookingId === bookingId) || (billPaymentId && x.billPaymentId === billPaymentId);
  const review = reviews.data?.find(mine);
  const openCase = cases.data?.find((c) => mine(c) && c.kind === "COMPLAINT" && c.status !== "RESOLVED");
  if (!reviews.data || !cases.data) return null;

  return (
    <div className="space-y-2">
      {review ? (
        <div className="flex items-center gap-2 rounded-xl bg-canvas p-3 text-sm">
          <Stars value={review.rating} /> You reviewed this visit
          {review.reply ? " · the restaurant replied" : ""}.
        </div>
      ) : (
        canReview && (
          <Button variant="secondary" className="w-full" onClick={() => setRating(true)}>
            <Star className="h-4 w-4" aria-hidden /> Rate your visit
          </Button>
        )
      )}
      {openCase ? (
        <Link href={`/account/cases/${openCase.id}`} className="block rounded-xl bg-amber-50 p-3 text-sm text-amber-900 ring-1 ring-amber-200">
          You reported a problem ({openCase.number}) — see replies
        </Link>
      ) : (
        <Button variant="ghost" className="w-full" onClick={() => setReporting(true)}>
          <MessageSquareWarning className="h-4 w-4" aria-hidden /> Report a problem
        </Button>
      )}
      {rating && (
        <RateVisitSheet
          open
          onClose={() => setRating(false)}
          bookingId={bookingId}
          billPaymentId={billPaymentId}
          restaurantName={restaurantName}
          onDone={() => {
            setRating(false);
            toast("Thanks — your review is live");
            reviews.reload();
          }}
        />
      )}
      {reporting && (
        <ReportProblemSheet
          open
          onClose={() => setReporting(false)}
          kind="COMPLAINT"
          bookingId={bookingId}
          billPaymentId={billPaymentId}
          subject={`${restaurantName} · visit on ${longDate(visitDate)}`}
          onOpened={(c) => {
            setReporting(false);
            toast("Sent to the restaurant — they have 48 hours to reply");
            router.push(`/account/cases/${c.id}`);
          }}
        />
      )}
    </div>
  );
}
