import type { Metadata, Viewport } from "next";
import Script from "next/script";
import "./globals.css";
import { Providers } from "@/components/providers";
import { AppShell } from "@/components/app-shell";
import { NativeAppBridge } from "@/components/native-app";
import { PwaBootstrap } from "@/components/pwa";
import { INSTALL_PROMPT_CAPTURE } from "@/lib/pwa-capture";

export const metadata: Metadata = {
  title: { default: "Feast — book tables, get deals, pay bills", template: "%s · Feast" },
  description: "Discover restaurants, pre-book tables with deals and pay your bill to get them.",
  applicationName: "Feast",
  // Installable web app ("Add to Home Screen"). /partner swaps in the Feast Partner
  // manifest; the admin area links none (not installable).
  manifest: "/manifest.webmanifest",
  appleWebApp: { capable: true, title: "Feast", statusBarStyle: "default" },
  icons: { apple: [{ url: "/icons/diner-apple-touch-icon.png", sizes: "180x180" }] },
  // Next emits only "mobile-web-app-capable"; iOS before 16.4 needs the apple- variant.
  other: { "apple-mobile-web-app-capable": "yes" },
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
        {/* Keep Chrome's install prompt: it can fire before React hydrates. */}
        <Script id="feast-install-capture" strategy="beforeInteractive" dangerouslySetInnerHTML={{ __html: INSTALL_PROMPT_CAPTURE }} />
        <Providers>
          <NativeAppBridge />
          <PwaBootstrap />
          <AppShell>{children}</AppShell>
        </Providers>
      </body>
    </html>
  );
}
