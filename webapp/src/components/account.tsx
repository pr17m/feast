"use client";

import clsx from "clsx";
import type { LucideIcon } from "lucide-react";
import { CalendarCheck, MessageSquareWarning, PiggyBank, Receipt, ShieldCheck, Store } from "lucide-react";
import type { Role, User } from "@/lib/api/types";

export const ROLE_LABEL: Record<Role, string> = {
  CUSTOMER: "Diner",
  OWNER: "Restaurant partner",
  ADMIN: "Feast admin",
};

const ROLE_STYLE: Record<Role, string> = {
  CUSTOMER: "bg-canvas text-muted ring-1 ring-line",
  OWNER: "bg-brand-soft text-brand-strong",
  ADMIN: "bg-ink text-white",
};

export function RoleBadge({ role, className }: { role: Role; className?: string }) {
  return (
    <span className={clsx("inline-flex items-center rounded-full px-2 py-0.5 text-[11px] font-bold uppercase tracking-wide", ROLE_STYLE[role], className)}>
      {ROLE_LABEL[role]}
    </span>
  );
}

export function Avatar({ name, size = "md" }: { name: string; size?: "sm" | "md" | "lg" }) {
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join("");
  return (
    <span
      className={clsx(
        "grid shrink-0 place-items-center rounded-full bg-brand-soft font-bold text-brand-strong ring-1 ring-brand/15",
        size === "sm" && "h-8 w-8 text-xs",
        size === "md" && "h-10 w-10 text-sm",
        size === "lg" && "h-16 w-16 text-xl",
      )}
      aria-hidden
    >
      {initials || "?"}
    </span>
  );
}

export interface AccountLink {
  href: string;
  label: string;
  hint: string;
  icon: LucideIcon;
}

/** Links shown in the account menu (desktop) and account page (mobile), by role. */
export function accountLinks(user: User): AccountLink[] {
  const links: AccountLink[] = [
    {
      href: "/bookings",
      label: "My bookings",
      hint: "Upcoming and past tables",
      icon: CalendarCheck,
    },
    {
      href: "/bookings?tab=payments",
      label: "Bill payments",
      hint: "Receipts and savings",
      icon: Receipt,
    },
    ...(user.role === "CUSTOMER"
      ? [
          {
            href: "/account/insights",
            label: "My savings",
            hint: "What you've saved, favourites and visits",
            icon: PiggyBank,
          },
        ]
      : []),
    {
      href: "/account/cases",
      label: "Problems & reviews",
      hint: "Issues you reported, reports about you, your reviews",
      icon: MessageSquareWarning,
    },
  ];
  if (user.role === "OWNER" || user.role === "ADMIN")
    links.push({
      href: "/partner",
      label: "Partner portal",
      hint: user.role === "ADMIN" ? "Manage any restaurant" : "Orders, bookings, menu, offers",
      icon: Store,
    });
  if (user.role === "ADMIN")
    links.push({
      href: "/admin",
      label: "Admin console",
      hint: "Restaurants, users, platform",
      icon: ShieldCheck,
    });
  return links;
}

export function contactLine(user: User): string {
  return user.email ?? user.phone ?? "";
}
