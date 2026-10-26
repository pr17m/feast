import Link from "next/link";
import type { ReactNode } from "react";
import type { LegalInfo } from "@/lib/legal";

export const LEGAL_PAGES = [
  { href: "/legal/terms", label: "Terms of use" },
  { href: "/legal/privacy", label: "Privacy policy" },
  { href: "/legal/refunds", label: "Refunds and cancellations" },
  { href: "/legal/contact", label: "Contact us" },
  { href: "/account-deletion", label: "Delete your account" },
] as const;

function fmtDate(iso: string) {
  const d = new Date(`${iso}T00:00:00Z`);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
}

/** Shared layout for the policy pages: title, last-updated date, draft notice and links between the pages. */
export function LegalPage({ title, info, current, children }: { title: string; info: LegalInfo; current: string; children: ReactNode }) {
  return (
    <article className="mx-auto max-w-3xl space-y-6 pb-10">
      <header className="space-y-2">
        <h1 className="text-3xl font-bold tracking-tight">{title}</h1>
        <p className="text-sm text-muted">
          {info.name}
          {info.registration ? ` · ${info.registration}` : ""} · Last updated {fmtDate(info.updated)}
        </p>
      </header>
      {!info.reviewed && (
        <div role="note" className="rounded-2xl bg-deal-soft p-4 text-sm text-ink ring-1 ring-deal/30">
          <b>Draft for legal review.</b> This text describes how Feast works today and must be checked by a lawyer before launch.
          {info.missing.length > 0 && <> Missing settings: {info.missing.join(", ")}.</>}
        </div>
      )}
      <div className="legal-prose space-y-6 text-[15px] leading-relaxed">{children}</div>
      <nav aria-label="Policies" className="flex flex-wrap gap-2 border-t border-line pt-5">
        {LEGAL_PAGES.map((p) => (
          <Link
            key={p.href}
            href={p.href}
            aria-current={p.href === current ? "page" : undefined}
            className={
              p.href === current
                ? "rounded-full bg-ink px-3 py-1.5 text-sm font-semibold text-white"
                : "rounded-full bg-surface px-3 py-1.5 text-sm font-medium ring-1 ring-line hover:ring-brand"
            }
          >
            {p.label}
          </Link>
        ))}
      </nav>
    </article>
  );
}

export function Section({ id, title, children }: { id?: string; title: string; children: ReactNode }) {
  return (
    <section id={id} className="space-y-3">
      <h2 className="text-xl font-bold">{title}</h2>
      {children}
    </section>
  );
}

export function Bullets({ items }: { items: ReactNode[] }) {
  return (
    <ul className="list-disc space-y-1.5 pl-5">
      {items.map((it, i) => (
        <li key={i}>{it}</li>
      ))}
    </ul>
  );
}

export function Mail({ to }: { to: string }) {
  return to.startsWith("[") ? <span>{to}</span> : <a href={`mailto:${to}`} className="font-semibold text-brand underline-offset-2 hover:underline">{to}</a>;
}
