import type { Metadata, Viewport } from "next";
import Script from "next/script";
import "./globals.css";
import { Providers } from "@/components/providers";
import { AppShell } from "@/components/app-shell";

export const metadata: Metadata = {
  title: { default: "Feast — book tables, get deals, pay bills", template: "%s · Feast" },
  description: "Discover restaurants, pre-book tables with deals and pay your bill to get them.",
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  themeColor: "#0f766e",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body className="antialiased">
        {/* Deploy-time config (API URL per country); must load before the app. */}
        <Script src="/runtime-config.js" strategy="beforeInteractive" />
        <Providers>
          <AppShell>{children}</AppShell>
        </Providers>
      </body>
    </html>
  );
}
