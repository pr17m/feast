"use client";

import { useEffect, type ReactNode } from "react";
import { usePathname, useRouter } from "next/navigation";
import { ShieldAlert } from "lucide-react";
import type { Role } from "@/lib/api/types";
import { homeFor, useAuth } from "./providers";
import { Button, LinkButton, Skeleton } from "./ui";

/**
 * Client-side guard for UX only — it decides what to render and where to redirect.
 * Real enforcement lives in the backend, which rejects calls by role.
 */
export function RequireRole({ roles, children }: { roles?: Role[]; children: ReactNode }) {
  const { user, ready, signOut } = useAuth();
  const router = useRouter();
  const path = usePathname();

  useEffect(() => {
    if (ready && !user) {
      const next = typeof window !== "undefined" ? window.location.pathname + window.location.search : path;
      router.replace(`/login?next=${encodeURIComponent(next)}`);
    }
  }, [ready, user, router, path]);

  if (!ready || !user)
    return (
      <div className="mx-auto max-w-xl space-y-3 py-6" aria-busy="true">
        <Skeleton className="h-10 w-1/2" />
        <Skeleton className="h-40 w-full" />
      </div>
    );

  if (roles && !roles.includes(user.role)) {
    return (
      <div className="mx-auto flex max-w-md flex-col items-center gap-3 rounded-3xl bg-surface px-6 py-12 text-center ring-1 ring-line">
        <ShieldAlert className="h-10 w-10 text-deal" aria-hidden />
        <h1 className="text-xl font-bold">You don&apos;t have access to this page</h1>
        <p className="text-muted">
          You&apos;re signed in as <b>{user.name}</b> ({user.role.toLowerCase()}). This area is for {roles.map((r) => r.toLowerCase()).join(" or ")} accounts.
        </p>
        <div className="flex gap-2 pt-2">
          <LinkButton href={homeFor(user.role)} variant="secondary">
            Go to my home
          </LinkButton>
          <Button
            onClick={async () => {
              await signOut();
              router.replace(`/login?next=${encodeURIComponent(path)}`);
            }}
          >
            Switch account
          </Button>
        </div>
      </div>
    );
  }

  return <>{children}</>;
}
