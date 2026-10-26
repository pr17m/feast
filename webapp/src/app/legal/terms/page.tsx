import type { Metadata } from "next";
import Link from "next/link";
import { Bullets, LegalPage, Mail, Section } from "@/components/legal";
import { legalInfo } from "@/lib/legal";

export const dynamic = "force-dynamic";
export const metadata: Metadata = { title: "Terms of use" };

export default function TermsPage() {
  const info = legalInfo();
  const consumerLaw =
    info.country === "AU"
      ? "the Australian Consumer Law"
      : info.country === "IN"
        ? "the Consumer Protection Act, 2019 and the Consumer Protection (E-Commerce) Rules, 2020"
        : "the consumer laws that apply where you live";
  return (
    <LegalPage title="Terms of use" info={info} current="/legal/terms">
      <p>
        These terms apply when you use Feast to find restaurants, book tables or pay your bill, and when a restaurant uses Feast to
        list itself. Feast is run by {info.name}
        {info.address ? `, ${info.address}` : ""} (&ldquo;Feast&rdquo;, &ldquo;we&rdquo;). By using Feast you agree to these terms.
      </p>

      <Section title="1. What Feast does">
        <p>
          Feast connects diners with restaurants. The restaurant, not Feast, prepares and serves your meal and is responsible for the food, the service and
          the prices on its menu. Feast runs the app, takes payments on the restaurant&apos;s behalf and helps resolve problems.
        </p>
      </Section>

      <Section title="2. Your account">
        <Bullets
          items={[
            "Diners sign in with a one-time code sent to their phone, or with Google, Apple or Facebook. Restaurant staff sign in with a password.",
            "Keep your sign-in details to yourself. You're responsible for what happens in your account.",
            "Give accurate details. We may suspend accounts used for fraud, abuse or repeated no-shows, and we tell you why.",
            "You can delete your account at any time in the app. Upcoming bookings are cancelled; see the account deletion page for what we keep.",
          ]}
        />
      </Section>

      <Section title="3. Bookings">
        <Bullets
          items={[
            "A booking holds a table for the date, time and number of guests shown. Arrive on time; the restaurant may release the table if you're very late.",
            "Some restaurants ask for an upfront charge to hold the table. The booking screen shows it before you confirm. See our refunds and cancellations policy for what happens if you cancel or don't come.",
            "You can cancel a booking in the app while it is confirmed. Please cancel as early as you can so someone else can have the table.",
            "Restaurants can report diners who repeatedly don't turn up. That can limit future bookings.",
          ]}
        />
      </Section>

      <Section title="4. Feast deals">
        <Bullets
          items={[
            "Feast deals are for diners who book a table on Feast and pay their bill on Feast. A deal is never available at the restaurant's till, and a walk-in without a Feast booking doesn't get one.",
            "One discounted bill per booking, on the booking's date, at the restaurant you booked.",
            "Each deal shows its own terms (for example a minimum bill, a maximum discount or excluded items). Restaurants may also cap the discount on any one bill and limit how many deals they give each day. When today's deals are used up, you can still pay on Feast, without the deal.",
            "Menu prices are the same for every diner. The deal is a discount on your bill, shown before you pay.",
          ]}
        />
      </Section>

      <Section title="5. At the restaurant and paying">
        <Bullets
          items={[
            "You can browse restaurants and menus without an account. You sign in to book or to pay.",
            "You order your food with the restaurant's staff, as usual. The restaurant's Feast QR code opens its page on Feast, where a diner with a Feast booking there today pays the bill.",
            "You enter the total from the restaurant's bill and pay in the app, or choose to pay at the counter and the restaurant confirms it in Feast. The restaurant sees what you paid.",
            "Payments are processed by our payment providers (Stripe in Australia, Razorpay in India). Feast never sees or stores your full card number.",
            "Check the amount before you pay. If something on your bill is wrong, report it in the app (see section 7).",
          ]}
        />
      </Section>

      <Section title="6. Reviews and content">
        <Bullets
          items={[
            "You can review a restaurant after a real visit (a booking you attended or a bill you paid on Feast), once per visit. Your review shows your first name and the first letter of your surname.",
            "Reviews must be honest and about your own visit. No abuse, discrimination, personal information, threats, advertising or content you don't have the right to share.",
            "Restaurants can reply to and report reviews. Feast may hide a review or restaurant content that breaks these rules, and always records the reason; the author can ask us to look again.",
          ]}
        />
      </Section>

      <Section title="7. When something goes wrong">
        <p>
          Report a problem from the booking or receipt in the app within 30 days of your visit. The restaurant has 48 hours to reply. If you&apos;re not happy
          with the reply, ask Feast to step in and we decide within 72 hours, explaining our decision to both sides. Restaurants can report diners in the
          same way. You can also <Link href="/legal/contact" className="font-semibold text-brand hover:underline">contact us</Link>.
        </p>
        <p>
          Some processes are automated: a complaint goes to Feast automatically if the restaurant doesn&apos;t reply in time, and closes automatically if you
          don&apos;t reply to the restaurant within 7 days. Ask us if you want a person to review an automated step.
        </p>
      </Section>

      <Section id="restaurants" title="8. Terms for restaurants">
        <Bullets
          items={[
            "Your restaurant stays hidden from diners until Feast has checked the licences and registrations your country requires. We may ask for updated documents, and may pause a listing if they lapse.",
            "You own your menu, photos, offers and settings, and you're responsible for keeping them accurate. You give Feast permission to show them in the app and to promote your restaurant.",
            "Feast staff can only change your content when you grant time-limited support access, and every change is logged for you to see. Feast can hide content that breaks these terms or the law, always with a reason you can appeal.",
            "Honour the bookings and Feast deals shown in the app. Charge Feast diners the same menu prices as everyone else; don't give the Feast deal at your till.",
            "Subscriptions are billed in advance for the plan you choose, at the price shown when you subscribe (plus tax). New restaurants get the free period shown at sign-up. You can cancel at any time; your plan runs to the end of the period you've paid for.",
            "For bills and upfront charges paid on Feast, card processing costs and Feast's payments fee (shown in the partner app) are deducted before the money is paid to you.",
          ]}
        />
      </Section>

      <Section title="9. Things you mustn't do">
        <Bullets
          items={[
            "Misuse deals, create fake bookings or reviews, or use someone else's account or payment details.",
            "Interfere with Feast's systems, scrape the app, or try to get around its security or limits.",
            "Use Feast for anything unlawful.",
          ]}
        />
      </Section>

      <Section title="10. Our responsibility">
        <p>
          Nothing in these terms limits rights you have under {consumerLaw} that can&apos;t be excluded. Apart from those rights, Feast isn&apos;t responsible
          for the meal or service a restaurant provides, and our liability to you for anything else is limited to the amount you paid through Feast for the
          booking or bill concerned.
        </p>
      </Section>

      <Section title="11. Changes and governing law">
        <p>
          We may update these terms. If a change matters to you, we&apos;ll tell you in the app before it takes effect. These terms are governed by the laws of{" "}
          {info.governingLaw}.
        </p>
        <p>
          Questions: <Mail to={info.supportEmail} />.
        </p>
      </Section>
    </LegalPage>
  );
}
