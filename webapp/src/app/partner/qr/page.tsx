"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { Download, ExternalLink, Printer } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { ErrorState, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";
import { usePartner } from "../partner-context";

/** The link printed in the restaurant's QR code. */
function qrUrl(code: string) {
  const path = `/r/${code.toLowerCase()}`;
  return typeof window !== "undefined" ? `${window.location.origin}${path}` : path;
}

function escapeHtml(s: string) {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c);
}

/** A printable A4/Letter poster: the QR, the restaurant's name and what scanning it does. */
async function printPoster(code: string, name: string, deal?: string) {
  const qr = await QRCode.toDataURL(qrUrl(code), { width: 900, margin: 1, color: { dark: "#1c1917", light: "#ffffff" } });
  const w = window.open("", "_blank", "noopener=no,width=800,height=1000");
  if (!w) return;
  w.document.write(`<!doctype html><html><head><meta charset="utf-8"><title>${escapeHtml(name)} · Feast QR</title>
<style>
  @page { size: A4; margin: 18mm; }
  * { box-sizing: border-box; }
  body { font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, Arial, sans-serif; color: #1c1917; margin: 0; text-align: center; }
  .wrap { display: flex; flex-direction: column; align-items: center; gap: 18px; padding-top: 6mm; }
  .brand { font-size: 28px; font-weight: 900; color: #0f766e; letter-spacing: -0.5px; }
  h1 { font-size: 40px; margin: 0; line-height: 1.1; }
  .lead { font-size: 22px; margin: 0; max-width: 150mm; line-height: 1.35; }
  .deal { font-size: 26px; font-weight: 800; color: #d9480f; margin: 0; }
  img { width: 110mm; height: 110mm; }
  .small { font-size: 15px; color: #78716c; margin: 0; max-width: 150mm; line-height: 1.4; }
  .url { font-family: ui-monospace, Menlo, monospace; font-size: 16px; }
</style></head><body><div class="wrap">
  <div class="brand">feast.</div>
  <h1>${escapeHtml(name)}</h1>
  ${deal ? `<p class="deal">${escapeHtml(deal)}</p>` : ""}
  <img src="${qr}" alt="QR code">
  <p class="lead"><b>Booked on Feast?</b> Scan to pay your bill and get your deal.</p>
  <p class="lead"><b>Not booked?</b> Scan to book your next visit with Feast.</p>
  <p class="url">${escapeHtml(qrUrl(code))}</p>
  <p class="small">Feast deals apply only to diners who booked on Feast and pay through Feast.</p>
</div><script>window.onload = () => { window.focus(); window.print(); };</script></body></html>`);
  w.document.close();
}

function QrImage({ code, size = 240 }: { code: string; size?: number }) {
  const [src, setSrc] = useState<string>();
  useEffect(() => {
    let alive = true;
    QRCode.toDataURL(qrUrl(code), { width: size, margin: 1, color: { dark: "#1c1917", light: "#ffffff" } })
      .then((url) => alive && setSrc(url))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [code, size]);
  // eslint-disable-next-line @next/next/no-img-element
  return src ? <img src={src} width={size} height={size} alt={`QR code linking to ${qrUrl(code)}`} className="rounded-xl" /> : <Skeleton className="h-60 w-60" />;
}

export default function PartnerQr() {
  const { restaurant } = usePartner();
  const qr = useQuery(`pQr:${restaurant.id}`, (api) => api.partnerQr(restaurant.id));
  const code = qr.data?.code;
  const deal = restaurant.headlineOffer;

  return (
    <div className="space-y-5">
      <PageHeader title="QR code" subtitle="One code for the whole restaurant: print it for the tables, the counter or the door." />

      {qr.error && !qr.data && <ErrorState error={qr.error} onRetry={qr.reload} />}
      {!qr.data && !qr.error && <Skeleton className="h-80 w-full" />}

      {code && (
        <div className="grid gap-5 lg:grid-cols-[auto_1fr]">
          <div className="flex flex-col items-center gap-3 rounded-2xl bg-surface p-5 text-center ring-1 ring-line">
            <QrImage code={code} />
            <p className="max-w-full break-all font-mono text-sm">{qrUrl(code)}</p>
            <div className="flex flex-wrap justify-center gap-2">
              <button
                onClick={() => printPoster(code, restaurant.name, deal)}
                className="inline-flex h-10 items-center gap-1.5 rounded-xl bg-brand px-3 text-sm font-semibold text-white"
              >
                <Printer className="h-4 w-4" aria-hidden /> Print poster
              </button>
              <button
                onClick={async () => {
                  const url = await QRCode.toDataURL(qrUrl(code), { width: 1024, margin: 2 });
                  const a = document.createElement("a");
                  a.href = url;
                  a.download = `${restaurant.slug}-feast-qr.png`;
                  a.click();
                }}
                className="inline-flex h-10 items-center gap-1.5 rounded-xl px-3 text-sm font-semibold ring-1 ring-line"
              >
                <Download className="h-4 w-4" aria-hidden /> Download PNG
              </button>
              <Link href={`/r/${code.toLowerCase()}`} target="_blank" className="inline-flex h-10 items-center gap-1.5 rounded-xl px-3 text-sm font-semibold ring-1 ring-line">
                <ExternalLink className="h-4 w-4" aria-hidden /> Open as diner
              </Link>
            </div>
          </div>

          <div className="space-y-3 rounded-2xl bg-surface p-5 text-sm leading-relaxed ring-1 ring-line">
            <h2 className="text-base font-bold">What diners see when they scan it</h2>
            <ul className="list-disc space-y-1.5 pl-5">
              <li>
                <b>Booked on Feast for today:</b> &ldquo;Pay bill &amp; get your deal&rdquo;. They type the total from your bill and pay in the app, or choose
                to pay at the counter and you confirm it under Payments.
              </li>
              <li>
                <b>No booking:</b> your page with menu, photos and reviews, and &ldquo;Book a table&rdquo; for their next visit. They pay you as usual today.
              </li>
              <li>Before arriving, booked diners can tap &ldquo;I&apos;m here&rdquo; so you see them as arrived on the Bookings page.</li>
            </ul>
            <p className="text-muted">
              Feast deals only apply to diners who booked on Feast and pay through Feast. Don&apos;t give the Feast deal at your till.
            </p>
            <p className="text-muted">QR codes you printed for individual tables before still work: they open this same page.</p>
          </div>
        </div>
      )}
    </div>
  );
}
