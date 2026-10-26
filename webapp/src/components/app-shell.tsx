"use client";

import { LEGAL_PAGES } from "./legal";
import { ChangeCountryButton } from "./native-app";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import clsx from "clsx";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, CalendarCheck, ChevronDown, Compass, Eye, LocateFixed, MapPin, QrCode, Search, Check, LogOut, UserRound, X } from "lucide-react";
import { NotificationBell } from "./notification-bell";
import { homeFor, useAuth, useCity, useMarkets } from "./providers";
import { Sheet, Skeleton } from "./ui";
import { Avatar, RoleBadge, accountLinks, contactLine } from "./account";

const NAV = [
  { href: "/", label: "Discover", icon: Compass },
  { href: "/search", label: "Search", icon: Search },
  { href: "/dine-in", label: "At the restaurant", short: "Today", icon: QrCode },
  { href: "/bookings", label: "Bookings", icon: CalendarCheck },
];

/** Phones get an Account tab; on desktop the account lives in the header dropdown. */
const MOBILE_NAV = [...NAV, { href: "/account", label: "Account", icon: UserRound }];

function isActive(path: string, href: string) {
  return href === "/" ? path === "/" : path.startsWith(href);
}

export function Logo({ className }: { className?: string }) {
  return (
    <span className={clsx("text-2xl font-black tracking-tight text-brand", className)}>
      feast<span className="text-deal">.</span>
    </span>
  );
}

function CityPicker() {
  const { cityId, setCityId, locateMe } = useCity();
  const { cities: list, marketOf } = useMarkets();
  const [open, setOpen] = useState(false);
  const [locating, setLocating] = useState<"idle" | "busy" | "failed">("idle");
  const cities = { data: list };
  const city = list.find((c) => c.id === cityId);
  return (
    <>
      <button
        onClick={() => setOpen(true)}
        className="flex items-center gap-1 rounded-lg px-2 py-1.5 text-sm font-semibold hover:bg-canvas"
        aria-label={`City: ${city?.name ?? "loading"}. Change city`}
      >
        <MapPin className="h-4 w-4 text-brand" aria-hidden />
        <span>{city?.name ?? "…"}</span>
        <ChevronDown className="h-4 w-4 text-muted" aria-hidden />
      </button>
      <Sheet open={open} onClose={() => setOpen(false)} title="Choose your city">
        <button
          onClick={async () => {
            setLocating("busy");
            const ok = await locateMe();
            setLocating(ok ? "idle" : "failed");
            if (ok) setOpen(false);
          }}
          className="mb-2 flex w-full items-center gap-2 rounded-xl px-3 py-3 text-left font-semibold text-brand hover:bg-canvas"
        >
          <LocateFixed className="h-5 w-5" aria-hidden />
          {locating === "busy" ? "Finding you…" : "Use my location"}
        </button>
        {locating === "failed" && <p className="mb-2 px-3 text-sm text-muted">Couldn&apos;t get your location — pick a city below.</p>}
        <ul className="space-y-1">
          {cities.data?.map((c) => (
            <li key={c.id}>
              <button
                onClick={() => {
                  setCityId(c.id);
                  setOpen(false);
                }}
                className="flex w-full items-center justify-between rounded-xl px-3 py-3 text-left hover:bg-canvas"
              >
                <span>
                  <span className="block font-semibold">{c.name}</span>
                  <span className="text-sm text-muted">
                    {marketOf(c.countryCode)?.name} · {c.areas.slice(0, 3).join(", ")}…
                  </span>
                </span>
                {c.id === cityId && <Check className="h-5 w-5 text-brand" aria-label="Selected" />}
              </button>
            </li>
          ))}
        </ul>
      </Sheet>
    </>
  );
}

/** Desktop account control: avatar + name opening a dropdown anchored under it. Phones use the Account tab instead. */
function AccountMenu() {
  const { user, ready, signOut } = useAuth();
  const [open, setOpen] = useState(false);
  const router = useRouter();
  const path = usePathname();
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click, Escape, or navigation.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  const [lastPath, setLastPath] = useState(path);
  if (path !== lastPath) {
    setLastPath(path);
    setOpen(false);
  }

  if (!ready) return <span className="h-10 w-24" aria-hidden />;
  if (!user)
    return (
      <Link href="/login" className="inline-flex h-10 items-center rounded-xl bg-brand px-4 text-sm font-semibold text-white hover:bg-brand-strong">
        Sign in
      </Link>
    );

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Account: ${user.name}`}
        className={clsx("flex h-10 items-center gap-2 rounded-full py-1 pl-1 pr-3 transition-colors hover:bg-canvas", open && "bg-canvas")}
      >
        <Avatar name={user.name} size="sm" />
        <span className="max-w-28 truncate text-sm font-semibold">{user.name.split(" ")[0]}</span>
        <ChevronDown className={clsx("h-4 w-4 text-muted transition-transform", open && "rotate-180")} aria-hidden />
      </button>

      {open && (
        <div
          role="menu"
          className="animate-fade-in absolute right-0 top-full z-50 mt-2 w-80 overflow-hidden rounded-2xl bg-surface shadow-[0_12px_40px_rgb(28_25_23/0.16)] ring-1 ring-line"
        >
          <div className="flex items-center gap-3 border-b border-line p-4">
            <Avatar name={user.name} />
            <div className="min-w-0">
              <p className="truncate font-bold">{user.name}</p>
              <p className="truncate text-sm text-muted">{contactLine(user)}</p>
              <RoleBadge role={user.role} className="mt-1" />
            </div>
          </div>
          <ul className="p-2">
            {accountLinks(user).map((x) => (
              <li key={x.href}>
                <Link href={x.href} role="menuitem" onClick={() => setOpen(false)} className="flex items-center gap-3 rounded-xl px-3 py-2.5 hover:bg-canvas">
                  <x.icon className="h-5 w-5 shrink-0 text-brand" aria-hidden />
                  <span className="min-w-0">
                    <span className="block text-sm font-semibold">{x.label}</span>
                    <span className="block truncate text-xs text-muted">{x.hint}</span>
                  </span>
                </Link>
              </li>
            ))}
          </ul>
          <div className="border-t border-line p-2">
            <button
              role="menuitem"
              onClick={async () => {
                setOpen(false);
                await signOut();
                router.push("/");
              }}
              className="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-semibold text-danger hover:bg-red-50"
            >
              <LogOut className="h-5 w-5" aria-hidden /> Sign out
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Customer, owner and admin are separate apps that share one backend.
 * Staff (OWNER/ADMIN) never get the customer app: they're sent to their portal.
 * The one exception is a restaurant's public page, which staff open in
 * "customer preview" to see exactly what diners see of their listing.
 */
function staffRoute(path: string): "portal" | "preview" | "allowed" {
  if (path.startsWith("/partner") || path.startsWith("/admin") || path.startsWith("/login") || path.startsWith("/join") || path.startsWith("/legal") || path.startsWith("/account-deletion") || path.startsWith("/offline"))
    return "allowed";
  if (path.startsWith("/restaurants/")) return "preview";
  return "portal";
}

function PreviewShell({ children, home }: { children: ReactNode; home: string }) {
  return (
    <div className="min-h-dvh">
      <div className="sticky top-0 z-40 bg-ink text-white">
        <div className="mx-auto flex h-12 max-w-6xl items-center gap-3 px-4 text-sm">
          <Eye className="h-4 w-4 shrink-0" aria-hidden />
          <span className="min-w-0 flex-1 truncate">
            <b>Customer preview</b>
            <span className="hidden text-white/70 sm:inline"> · this is how diners see this listing</span>
          </span>
          <Link href={home} className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-white/10 px-3 py-1.5 font-semibold hover:bg-white/20">
            <ArrowLeft className="h-4 w-4" aria-hidden /> Back to portal
          </Link>
        </div>
      </div>
      <main className="mx-auto max-w-6xl px-4 py-5 md:py-8">{children}</main>
    </div>
  );
}

/** Shown when we detect a country Feast doesn't serve yet. */
function CountryNotice() {
  const { unsupportedCountry, countryRedirect } = useCity();
  const { markets } = useMarkets();
  const [hidden, setHidden] = useState(false);
  if (!unsupportedCountry || hidden) return null;
  let country = unsupportedCountry;
  try {
    country = new Intl.DisplayNames(["en"], { type: "region" }).of(unsupportedCountry) ?? unsupportedCountry;
  } catch {
    // keep the code
  }
  return (
    <div className="border-b border-amber-200 bg-amber-50 text-amber-900">
      <div className="mx-auto flex max-w-6xl items-center gap-3 px-4 py-2 text-sm">
        {countryRedirect ? (
          <span className="flex-1">
            Looks like you&apos;re in {country}.{" "}
            <a href={countryRedirect} className="font-semibold underline">
              Go to Feast {country} →
            </a>
          </span>
        ) : (
          <span className="flex-1">
            Feast isn&apos;t in {country} yet. You&apos;re browsing Feast {markets[0]?.name ?? ""}.
          </span>
        )}
        <button onClick={() => setHidden(true)} aria-label="Dismiss" className="rounded p-1 hover:bg-amber-100">
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}

export function AppShell({ children }: { children: ReactNode }) {
  const path = usePathname();
  const router = useRouter();
  const { user, ready } = useAuth();
  const staff = ready && !!user && (user.role === "OWNER" || user.role === "ADMIN");
  const access = staff ? staffRoute(path) : "allowed";
  const home = user ? homeFor(user.role) : "/";

  useEffect(() => {
    if (access === "portal") router.replace(home);
  }, [access, home, router]);

  // Partner/admin portals have their own shell.
  const focused = path.startsWith("/partner") || path.startsWith("/admin");
  if (focused) return <>{children}</>;
  if (access === "portal")
    return (
      <div className="mx-auto max-w-xl space-y-3 p-6" aria-busy="true" aria-label="Opening your portal">
        <Skeleton className="h-10 w-1/2" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  if (access === "preview") return <PreviewShell home={home}>{children}</PreviewShell>;

  return (
    <div className="min-h-dvh pb-20 md:pb-0">
      <header className="sticky top-0 z-40 border-b border-line bg-surface/90 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-6xl items-center gap-3 px-4">
          <Link href="/" aria-label="Feast home">
            <Logo />
          </Link>
          <CityPicker />
          <form
            className="ml-auto hidden max-w-sm flex-1 md:block"
            onSubmit={(e) => {
              e.preventDefault();
              const q = new FormData(e.currentTarget).get("q");
              router.push(`/search?q=${encodeURIComponent(String(q ?? ""))}`);
            }}
          >
            <label className="flex h-10 items-center gap-2 rounded-xl bg-canvas px-3 ring-1 ring-line focus-within:ring-brand">
              <Search className="h-4 w-4 text-muted" aria-hidden />
              <input name="q" placeholder="Restaurants, cuisines, dishes" className="w-full bg-transparent text-sm outline-none" />
            </label>
          </form>
          <nav className="ml-auto hidden items-center gap-1 md:ml-2 md:flex" aria-label="Main">
            {NAV.filter((n) => n.href !== "/search").map((n) => (
              <Link
                key={n.href}
                href={n.href}
                className={clsx(
                  "flex items-center gap-1.5 rounded-lg px-3 py-2 text-sm font-semibold",
                  isActive(path, n.href) ? "text-brand" : "text-muted hover:text-ink",
                )}
              >
                <n.icon className="h-4 w-4" aria-hidden />
                {n.label}
              </Link>
            ))}
          </nav>
          <NotificationBell className="ml-auto md:ml-0" />
          <div className="hidden md:block">
            <AccountMenu />
          </div>
        </div>
      </header>

      <CountryNotice />
      <main className="mx-auto max-w-6xl px-4 py-5 md:py-8">{children}</main>

      <footer className="border-t border-line">
        <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-x-5 gap-y-2 px-4 py-6 text-sm text-muted">
          <span>© {new Date().getFullYear()} Feast</span>
          {LEGAL_PAGES.map((p) => (
            <Link key={p.href} href={p.href} className="hover:text-ink">
              {p.label}
            </Link>
          ))}
          <Link href="/join" className="hover:text-ink">
            List your restaurant
          </Link>
          <ChangeCountryButton className="hover:text-ink" />
        </div>
      </footer>

      <nav
        className="fixed inset-x-0 bottom-0 z-40 border-t border-line bg-surface pb-[env(safe-area-inset-bottom)] md:hidden"
        aria-label="Main"
      >
        <ul className="grid grid-cols-5">
          {MOBILE_NAV.map((n) => (
            <li key={n.href}>
              <Link
                href={n.href}
                className={clsx(
                  "flex flex-col items-center gap-0.5 py-2.5 text-xs font-semibold",
                  isActive(path, n.href) ? "text-brand" : "text-muted",
                )}
                aria-current={isActive(path, n.href) ? "page" : undefined}
              >
                <n.icon className="h-5 w-5" aria-hidden />
                {("short" in n && n.short) || n.label}
              </Link>
            </li>
          ))}
        </ul>
      </nav>
    </div>
  );
}
