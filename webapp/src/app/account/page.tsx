"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { ChevronRight, LogOut, MapPin, UserRound } from "lucide-react";
import { Avatar, RoleBadge, accountLinks, contactLine } from "@/components/account";
import { DeleteAccountCard, PushSettingsCard } from "@/components/delete-account";
import { useAuth, useCity } from "@/components/providers";
import { Button, LinkButton, Skeleton } from "@/components/ui";
import { useQuery } from "@/lib/hooks";

export default function AccountPage() {
  const { user, ready, signOut } = useAuth();
  const { cityId, setCityId } = useCity();
  const router = useRouter();
  const cities = useQuery("cities", (api) => api.listCities());

  if (!ready) return <Skeleton className="mx-auto h-64 max-w-xl" />;

  if (!user)
    return (
      <div className="mx-auto flex max-w-md flex-col items-center gap-3 rounded-3xl bg-surface px-6 py-12 text-center ring-1 ring-line">
        <span className="grid h-16 w-16 place-items-center rounded-full bg-brand-soft text-brand">
          <UserRound className="h-8 w-8" aria-hidden />
        </span>
        <h1 className="text-xl font-black">Your Feast account</h1>
        <p className="text-muted">Sign in to book tables, pay your bill with your deal and keep your receipts in one place.</p>
        <LinkButton href="/login?next=/account" className="mt-2 w-full">
          Sign in
        </LinkButton>
      </div>
    );

  return (
    <div className="mx-auto max-w-xl space-y-4">
      <section className="flex items-center gap-4 rounded-3xl bg-surface p-5 ring-1 ring-line">
        <Avatar name={user.name} size="lg" />
        <div className="min-w-0">
          <h1 className="truncate text-xl font-black tracking-tight">{user.name}</h1>
          <p className="truncate text-sm text-muted">{contactLine(user)}</p>
          <RoleBadge role={user.role} className="mt-1.5" />
        </div>
      </section>

      <nav aria-label="Account" className="overflow-hidden rounded-3xl bg-surface ring-1 ring-line">
        <ul className="divide-y divide-line">
          {accountLinks(user).map((x) => (
            <li key={x.href}>
              <Link href={x.href} className="flex items-center gap-3 px-5 py-4 hover:bg-canvas">
                <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-brand-soft text-brand">
                  <x.icon className="h-5 w-5" aria-hidden />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block font-semibold">{x.label}</span>
                  <span className="block truncate text-sm text-muted">{x.hint}</span>
                </span>
                <ChevronRight className="h-5 w-5 text-faint" aria-hidden />
              </Link>
            </li>
          ))}
        </ul>
      </nav>

      <section className="rounded-3xl bg-surface p-5 ring-1 ring-line">
        <h2 className="mb-3 flex items-center gap-2 font-bold">
          <MapPin className="h-5 w-5 text-brand" aria-hidden /> City
        </h2>
        <div className="grid grid-cols-2 gap-2" role="radiogroup" aria-label="City">
          {cities.data?.map((c) => (
            <button
              key={c.id}
              role="radio"
              aria-checked={c.id === cityId}
              onClick={() => setCityId(c.id)}
              className={
                c.id === cityId
                  ? "rounded-xl bg-brand-soft px-3 py-2.5 text-sm font-semibold text-brand-strong ring-2 ring-brand"
                  : "rounded-xl px-3 py-2.5 text-sm font-semibold ring-1 ring-line hover:bg-canvas"
              }
            >
              {c.name}
            </button>
          ))}
        </div>
      </section>

      <PushSettingsCard />

      <Button
        variant="secondary"
        size="lg"
        className="w-full text-danger"
        onClick={async () => {
          await signOut();
          router.push("/");
        }}
      >
        <LogOut className="h-5 w-5" aria-hidden /> Sign out
      </Button>

      <DeleteAccountCard />
    </div>
  );
}
