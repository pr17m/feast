import type { Metadata } from "next";
import { Suspense } from "react";
import { RequireRole } from "@/components/require-role";
import { Skeleton } from "@/components/ui";
import { PartnerFrame } from "./partner-context";

export const metadata: Metadata = {
  title: "Partner portal",
  // Installs as its own app, "Feast Partner" (scope /partner).
  manifest: "/partner/manifest.webmanifest",
  appleWebApp: { capable: true, title: "Feast Partner", statusBarStyle: "default" },
  icons: { apple: [{ url: "/icons/partner-apple-touch-icon.png", sizes: "180x180" }] },
};

export default function PartnerLayout({ children }: { children: React.ReactNode }) {
  return (
    <RequireRole roles={["OWNER", "ADMIN"]}>
      <Suspense fallback={<Skeleton className="m-6 h-64" />}>
        <PartnerFrame>{children}</PartnerFrame>
      </Suspense>
    </RequireRole>
  );
}
