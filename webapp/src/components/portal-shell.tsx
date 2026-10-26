"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import clsx from "clsx";
import type { ReactNode } from "react";
import { Eye, LogOut } from "lucide-react";
import { Logo } from "./app-shell";
import { NotificationBell } from "./notification-bell";
import { useAuth } from "./providers";

export interface PortalNavItem {
  href: string;
  label: string;
  icon: typeof LogOut;
  badge?: number;
}

/**
 * Shell for the partner and admin portals.
 * Desktop: fixed left sidebar + wide content area for tables.
 * Mobile: compact top bar + horizontally scrolling section tabs.
 */
export function PortalShell({
  title,
  nav,
  switcher,
  preview,
  children,
}: {
  title: string;
  nav: PortalNavItem[];
  switcher?: ReactNode;
  /** "View as customer" link to the selected restaurant's public page. `href: null` = not public yet. */
  preview?: { href: string | null; note?: string };
  children: ReactNode;
}) {
  const path = usePathname();
  const router = useRouter();
  const { user, signOut } = useAuth();
  // The most specific nav entry that matches the current path is active.
  const current = nav
    .filter((n) => path === n.href || path.startsWith(`${n.href}/`))
    .sort((a, b) => b.href.length - a.href.length)[0]?.href;
  const active = (href: string) => href === current;

  const out = async () => {
    await signOut();
    router.replace("/login?as=partner");
  };

  return (
    <div className="min-h-dvh bg-canvas md:flex">
      {/* Desktop sidebar */}
      <aside className="sticky top-0 hidden h-dvh w-64 shrink-0 flex-col border-r border-line bg-surface md:flex">
        <div className="flex h-16 items-center gap-2 px-5">
          <Logo />
          <span className="rounded-md bg-ink px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider text-white">{title}</span>
          <NotificationBell className="ml-auto" />
        </div>
        {switcher && <div className="px-3 pb-3">{switcher}</div>}
        <nav className="flex-1 space-y-0.5 overflow-y-auto px-3" aria-label={title}>
          {nav.map((n) => (
            <Link
              key={n.href}
              href={n.href}
              aria-current={active(n.href) ? "page" : undefined}
              className={clsx(
                "flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-semibold",
                active(n.href) ? "bg-brand-soft text-brand-strong" : "text-muted hover:bg-canvas hover:text-ink",
              )}
            >
              <n.icon className="h-5 w-5" aria-hidden />
              <span className="flex-1">{n.label}</span>
              {!!n.badge && <span className="rounded-full bg-deal px-2 py-0.5 text-xs font-bold text-white">{n.badge}</span>}
            </Link>
          ))}
        </nav>
        <div className="space-y-1 border-t border-line p-3">
          {preview &&
            (preview.href ? (
              <Link href={preview.href} className="flex items-center gap-3 rounded-xl px-3 py-2 text-sm font-semibold text-muted hover:bg-canvas hover:text-ink">
                <Eye className="h-4 w-4" aria-hidden /> View as customer
              </Link>
            ) : (
              <p className="flex items-start gap-3 rounded-xl px-3 py-2 text-xs text-muted">
                <Eye className="mt-0.5 h-4 w-4 shrink-0" aria-hidden /> {preview.note ?? "Not visible to customers yet"}
              </p>
            ))}
          <div className="flex items-center gap-3 rounded-xl px-3 py-2">
            <span className="grid h-8 w-8 place-items-center rounded-full bg-brand-soft text-sm font-bold text-brand-strong">{user?.name.slice(0, 1)}</span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm font-semibold">{user?.name}</span>
              <span className="block text-xs text-muted">{user?.role.toLowerCase()}</span>
            </span>
            <button onClick={out} className="grid h-8 w-8 place-items-center rounded-lg text-muted hover:bg-red-50 hover:text-danger" aria-label="Sign out">
              <LogOut className="h-4 w-4" />
            </button>
          </div>
        </div>
      </aside>

      {/* Mobile header */}
      <div className="sticky top-0 z-40 border-b border-line bg-surface md:hidden">
        <div className="flex h-14 items-center gap-2 px-4">
          <Logo className="text-xl" />
          <span className="rounded-md bg-ink px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider text-white">{title}</span>
          <NotificationBell className="ml-auto" />
          {preview?.href && (
            <Link href={preview.href} className="grid h-9 w-9 place-items-center rounded-lg text-muted" aria-label="View as customer" title="View as customer">
              <Eye className="h-5 w-5" aria-hidden />
            </Link>
          )}
          <button onClick={out} className="grid h-9 w-9 place-items-center rounded-lg text-muted" aria-label="Sign out">
            <LogOut className="h-5 w-5" />
          </button>
        </div>
        {switcher && <div className="px-4 pb-2">{switcher}</div>}
        <nav className="no-scrollbar flex gap-1 overflow-x-auto px-3 pb-2" aria-label={title}>
          {nav.map((n) => (
            <Link
              key={n.href}
              href={n.href}
              aria-current={active(n.href) ? "page" : undefined}
              className={clsx(
                "flex shrink-0 items-center gap-1.5 rounded-full px-3 py-1.5 text-sm font-semibold",
                active(n.href) ? "bg-ink text-white" : "bg-canvas text-muted",
              )}
            >
              <n.icon className="h-4 w-4" aria-hidden />
              {n.label}
              {!!n.badge && <span className="rounded-full bg-deal px-1.5 text-xs font-bold text-white">{n.badge}</span>}
            </Link>
          ))}
        </nav>
      </div>

      <main className="min-w-0 flex-1 px-4 py-5 md:px-8 md:py-8">{children}</main>
    </div>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="mb-5 flex flex-wrap items-end justify-between gap-3">
      <div>
        <h1 className="text-2xl font-black tracking-tight">{title}</h1>
        {subtitle && <p className="text-sm text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap gap-2">{actions}</div>}
    </div>
  );
}

export function Stat({ label, value, hint, tone }: { label: string; value: ReactNode; hint?: string; tone?: "brand" | "deal" }) {
  return (
    <div className="rounded-2xl bg-surface p-4 ring-1 ring-line">
      <p className="text-xs font-semibold uppercase tracking-wide text-muted">{label}</p>
      <p className={clsx("mt-1 text-2xl font-black", tone === "brand" && "text-brand", tone === "deal" && "text-deal")}>{value}</p>
      {hint && <p className="text-xs text-muted">{hint}</p>}
    </div>
  );
}

export function Toggle({ checked, onChange, label, description, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; description?: string; disabled?: boolean }) {
  return (
    <label className={clsx("flex items-start justify-between gap-4 py-3", disabled && "opacity-60")}>
      <span>
        <span className="block font-semibold">{label}</span>
        {description && <span className="block text-sm text-muted">{description}</span>}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={clsx("relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition-colors", checked ? "bg-brand" : "bg-faint")}
      >
        <span className={clsx("absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-transform", checked ? "translate-x-5" : "translate-x-0.5")} />
      </button>
    </label>
  );
}
