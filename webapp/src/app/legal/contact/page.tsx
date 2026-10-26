import type { Metadata } from "next";
import Link from "next/link";
import { LegalPage, Mail, Section } from "@/components/legal";
import { legalInfo } from "@/lib/legal";

export const dynamic = "force-dynamic";
export const metadata: Metadata = { title: "Contact us" };

export default function ContactPage() {
  const info = legalInfo();
  return (
    <LegalPage title="Contact us" info={info} current="/legal/contact">
      <Section title="Something wrong with a visit?">
        <p>
          The fastest way is the app: open the booking or receipt and choose <b>Report a problem</b>. The restaurant replies within 48 hours and Feast steps in
          if needed. See{" "}
          <Link href="/account/cases" className="font-semibold text-brand hover:underline">
            your problems and reviews
          </Link>
          .
        </p>
      </Section>

      <Section title="Diners and restaurants">
        <dl className="grid gap-x-6 gap-y-2 sm:grid-cols-[10rem_1fr]">
          <dt className="font-semibold">Email</dt>
          <dd>
            <Mail to={info.supportEmail} />
          </dd>
          {info.supportPhone && (
            <>
              <dt className="font-semibold">Phone</dt>
              <dd>
                <a href={`tel:${info.supportPhone.replace(/\s/g, "")}`} className="font-semibold text-brand hover:underline">
                  {info.supportPhone}
                </a>
              </dd>
            </>
          )}
          <dt className="font-semibold">Privacy requests</dt>
          <dd>
            <Mail to={info.privacyEmail} />
          </dd>
          <dt className="font-semibold">Company</dt>
          <dd>
            {info.name}
            {info.registration ? ` · ${info.registration}` : ""}
          </dd>
          <dt className="font-semibold">Address</dt>
          <dd>{info.address}</dd>
        </dl>
      </Section>

      {info.country === "IN" && (
        <Section title="Grievance Officer">
          <p>
            {info.grievanceOfficer}. We acknowledge complaints within 24 hours and resolve them within 15 days, in line with the Consumer Protection
            (E-Commerce) Rules, 2020 and the Information Technology (Intermediary Guidelines and Digital Media Ethics Code) Rules, 2021.
          </p>
        </Section>
      )}

      <Section title="Restaurants who want to join">
        <p>
          <Link href="/join" className="font-semibold text-brand hover:underline">
            List your restaurant on Feast
          </Link>
          . Your listing goes live once your documents are approved.
        </p>
      </Section>
    </LegalPage>
  );
}
