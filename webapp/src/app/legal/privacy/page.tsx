import type { Metadata } from "next";
import { Bullets, LegalPage, Mail, Section } from "@/components/legal";
import { legalInfo } from "@/lib/legal";

export const dynamic = "force-dynamic";
export const metadata: Metadata = { title: "Privacy policy" };

export default function PrivacyPage() {
  const info = legalInfo();
  const au = info.country === "AU";
  const india = info.country === "IN";
  return (
    <LegalPage title="Privacy policy" info={info} current="/legal/privacy">
      <p>
        This policy explains what personal information {info.name} (&ldquo;Feast&rdquo;) collects, why, who we share it with and the choices you have.
        {au && " We handle personal information under the Privacy Act 1988 (Cth) and the Australian Privacy Principles."}
        {india &&
          " We handle personal data under Indian law, including the Information Technology Act, 2000 and the Digital Personal Data Protection Act, 2023 and its rules as they come into force."}
      </p>

      <Section title="What we collect">
        <Bullets
          items={[
            <>
              <b>Account details:</b> your name, phone number and email. If you sign in with Google, Apple or Facebook, we receive your name and email from
              them (never your password).
            </>,
            <>
              <b>Bookings and payments:</b> restaurants, dates, guests, bill amounts, deals used and payment status. Card details go
              straight to our payment provider; we only keep a reference and the result.
            </>,
            <>
              <b>Reviews and complaints:</b> what you write, ratings, photos you attach and messages with restaurants and Feast staff.
            </>,
            <>
              <b>Restaurant partners:</b> owner and staff contact details, business details, and the licences, registrations and bank documents submitted for
              verification.
            </>,
            <>
              <b>Device and usage:</b> your IP address and approximate country (to show the right country&apos;s app), browser type, and logs of how the app is
              used, kept for security and to fix problems. The app stores your sign-in session and a few preferences in your browser&apos;s local storage; we
              don&apos;t use advertising cookies.
            </>,
          ]}
        />
      </Section>

      <Section title="Why we use it">
        <Bullets
          items={[
            "To run your account, bookings and payments, and to apply the deals you're entitled to.",
            "To send you notifications about your bookings, payments, reviews and complaints.",
            "To verify restaurants before they're shown to diners, and to keep Feast safe from fraud and abuse.",
            "To resolve complaints between diners and restaurants.",
            "To show restaurants and Feast anonymous or summary insights (for example how many diners came through Feast).",
            "To meet legal, tax and accounting obligations.",
          ]}
        />
        <p>
          We use automated rules for some steps, such as moving an unanswered complaint to Feast, closing a complaint the diner stops replying to, and
          ordering search results. You can ask for a person to review any automated step that affects you.
        </p>
      </Section>

      <Section title="Who we share it with">
        <Bullets
          items={[
            "The restaurant you book or pay: your name, phone number, booking and payment details, and any complaint or review about that visit.",
            "Payment providers (Stripe in Australia, Razorpay in India) to process payments and refunds.",
            "Service providers who host and run Feast for us (cloud hosting, messaging and email, stock photo search), under contracts that require them to protect your information.",
            "Professional advisers, and authorities or courts when the law requires it.",
          ]}
        />
        <p>We don&apos;t sell your personal information.</p>
        {au && (
          <p>
            Some of these providers store or process information outside Australia, including in the United States. We take reasonable steps to make sure
            they handle it in line with the Australian Privacy Principles.
          </p>
        )}
        {india && <p>Some of these providers process data outside India, as permitted by Indian law.</p>}
      </Section>

      <Section title="How long we keep it">
        <p>
          We keep your information while your account is active and for as long as we need it for the purposes above. Payment and tax records are kept for
          the period the law requires. When we no longer need information, we delete or anonymise it.
        </p>
      </Section>

      <Section title="Your choices and rights">
        <Bullets
          items={[
            "See and update most of your details in the app.",
            "Ask us for a copy of your information, to correct it, or to delete your account.",
            "Turn off notifications you don't want.",
            india ? "Withdraw consent, nominate someone to act for you, and complain to the Data Protection Board of India." : "Complain to us, and if you're not satisfied, to the regulator.",
          ]}
        />
        <p>
          Write to <Mail to={info.privacyEmail} />. We reply within 30 days.
          {au && (
            <>
              {" "}
              If you&apos;re not happy with our response, you can complain to the Office of the Australian Information Commissioner at{" "}
              <a href="https://www.oaic.gov.au" className="font-semibold text-brand hover:underline">
                oaic.gov.au
              </a>
              .
            </>
          )}
        </p>
        {india && info.grievanceOfficer && <p>Grievance Officer: {info.grievanceOfficer}.</p>}
      </Section>

      <Section title="Security">
        <p>
          We use encryption in transit, access controls and audit logs. Feast staff can only see what they need for their job, and staff access to a
          restaurant&apos;s content is time-limited and logged. If a data breach is likely to cause you serious harm, we&apos;ll tell you
          {au ? " and the OAIC" : india ? " and the Data Protection Board" : ""} as the law requires.
        </p>
      </Section>

      <Section title="Changes">
        <p>We&apos;ll update this policy when our practices change and show the new date at the top.</p>
      </Section>
    </LegalPage>
  );
}
