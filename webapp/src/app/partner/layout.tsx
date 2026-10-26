import { Suspense } from "react";
import { RequireRole } from "@/components/require-role";
import { Skeleton } from "@/components/ui";
import { PartnerFrame } from "./partner-context";

export const metadata = { title: "Partner portal" };

export default function PartnerLayout({ children }: { children: React.ReactNode }) {
  return (
    <RequireRole roles={["OWNER", "ADMIN"]}>
      <Suspense fallback={<Skeleton className="m-6 h-64" />}>
        <PartnerFrame>{children}</PartnerFrame>
      </Suspense>
    </RequireRole>
  );
}
