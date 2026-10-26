import type { Metadata } from "next";
import Link from "next/link";
import { Bullets, LegalPage, Mail, Section } from "@/components/legal";
import { legalInfo } from "@/lib/legal";

export const dynamic = "force-dynamic";
export const metadata: Metadata = { title: "Refunds and cancellations" };

export default function RefundsPage() {
  const info = legalInfo();
  const law =
    info.country === "AU"
      ? "Your rights under the Australian Consumer Law, including refunds for a major problem, aren't affected by this policy."
      : info.country === "IN"
        ? "Your rights under the Consumer Protection Act, 2019 aren't affected by this policy."
        : "Your rights under consumer law aren't affected by this policy.";
  return (
    <LegalPage title="Refunds and cancellations" info={info} current="/legal/refunds">
      <p>{law}</p>

      <Section title="Cancelling a booking">
        <Bullets
          items={[
            "Bookings without an upfront charge: cancel any time before your booking in the app. There's no fee.",
            "Bookings with an upfront charge: cancel at least 2 hours before your booking time and the charge is refunded in full. If you cancel later or don't turn up, the restaurant keeps it.",
            "If the restaurant cancels your booking, any upfront charge is refunded in full.",
            "Refunds go back to the card or payment method you used. Your bank usually shows them within 5 to 10 business days.",
          ]}
        />
      </Section>

      <Section title="Problems with a bill or a meal">
        <p>
          If you were charged the wrong amount, a deal you were entitled to wasn&apos;t applied, or there was a serious problem with your meal, report it from
          the receipt or booking in the app within 30 days. The restaurant replies within 48 hours. If you can&apos;t agree, ask Feast to decide; we decide
          within 72 hours and tell you both. If we decide you should get money back, the refund goes to your original payment method.
        </p>
        <p>
          Bills paid at the restaurant&apos;s till rather than on Feast are between you and the restaurant, but you can still tell us about the problem.
        </p>
      </Section>

      <Section title="Feast deals">
        <p>
          A deal is applied before you pay and shown on your receipt. Deals have no cash value and can&apos;t be claimed after paying at the till, or added to a
          bill afterwards.
        </p>
      </Section>

      <Section title="Restaurant subscriptions">
        <Bullets
          items={[
            "New restaurants get the free period shown at sign-up; you aren't charged during it.",
            "Plans are paid in advance. You can cancel at any time and keep your plan until the end of the period you've paid for. We don't refund part-used periods unless the law requires it or we've made a mistake.",
            "If we charged you incorrectly, tell us and we'll refund the difference.",
          ]}
        />
      </Section>

      <Section title="Contact">
        <p>
          <Link href="/account/cases" className="font-semibold text-brand hover:underline">
            Report a problem in the app
          </Link>{" "}
          or email <Mail to={info.supportEmail} />.
        </p>
      </Section>
    </LegalPage>
  );
}
