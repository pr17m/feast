import type { Metadata } from "next";
import { AdminFrame } from "./admin-frame";

// The admin area is for Feast staff: not installable as an app (no manifest, no home-screen title).
export const metadata: Metadata = {
  manifest: null,
  appleWebApp: null,
  other: { "apple-mobile-web-app-capable": "no" },
  icons: null,
};

export default function AdminLayout({ children }: { children: React.ReactNode }) {
  return <AdminFrame>{children}</AdminFrame>;
}
