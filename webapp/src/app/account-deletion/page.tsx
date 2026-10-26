import type { Metadata } from "next";
import Link from "next/link";
import { Bullets, LegalPage, Mail, Section } from "@/components/legal";
import { legalInfo } from "@/lib/legal";

export const dynamic = "force-dynamic";
export const metadata: Metadata = { title: "Delete your Feast account" };

/**
 * Public page explaining account deletion (Google Play asks for a web link;
 * Apple for in-app deletion, which the Account / Settings screens provide).
 */
export default async function AccountDeletionPage({ searchParams }: { searchParams: Promise<{ done?: string }> }) {
  const info = legalInfo();
  const { done } = await searchParams;
  return (
    <LegalPage title="Delete your Feast account" info={info} current="/account-deletion">
      {done === "1" && (
        <p role="status" className="rounded-2xl bg-brand-soft p-4 font-semibold text-brand-strong">
          Your account has been deleted. Thanks for dining with Feast.
        </p>
      )}
      <p>
        You can delete your account yourself at any time, in the Feast app, the Feast Partner app or on this website. It takes effect immediately and
        can&apos;t be undone.
      </p>

      <Section title="How to delete it">
        <Bullets
          items={[
            <>
              <b>Diners (Feast app or website):</b> sign in, open{" "}
              <Link href="/account" className="font-semibold text-brand hover:underline">
                Account
              </Link>{" "}
              and choose <b>Delete my account</b>. If you sign in with your phone number we text you a code to confirm it&apos;s you.
            </>,
            <>
              <b>Restaurant owners (Feast Partner app or website):</b> open <b>Settings</b> and choose <b>Delete my account</b>, then enter your password.
              Restaurants you are the only owner of are paused. If one has counter payments waiting or open disputes, finish those first or add another
              owner.
            </>,
            <>
              <b>Can&apos;t sign in?</b> Email <Mail to={info.privacyEmail} /> from the email address or with the phone number on your account and we&apos;ll
              delete it within 30 days.
            </>,
          ]}
        />
      </Section>

      <Section title="What we delete">
        <Bullets
          items={[
            "Your name, phone number and email address.",
            "Your password and any Google, Apple or Facebook sign-in links.",
            "Your phone's push-notification registrations and notification settings.",
            "Your upcoming bookings, which are cancelled (the restaurant is told and the table is released).",
          ]}
        />
      </Section>

      <Section title="What we keep, and why">
        <Bullets
          items={[
            "Receipts and payment records, without your name, for as long as tax and accounting law requires (typically 5 to 8 years).",
            "Past bookings, reviews and dispute threads, shown as “Former Feast diner”, so restaurants' records and ratings stay accurate.",
            "The audit trail of changes (for example that a booking was cancelled), which can't be altered, to resolve disputes and meet legal obligations.",
          ]}
        />
        <p>
          See our{" "}
          <Link href="/legal/privacy" className="font-semibold text-brand hover:underline">
            privacy policy
          </Link>{" "}
          for details.
        </p>
      </Section>
    </LegalPage>
  );
}
