"use client";

import { Banknote, CalendarCheck, CreditCard, EyeOff, FileCheck2, Globe2, History, LayoutDashboard, Receipt, Scale, Star, Store, TrendingUp, Users } from "lucide-react";
import { PortalShell } from "@/components/portal-shell";
import { RequireRole } from "@/components/require-role";

const NAV = [
  { href: "/admin", label: "Overview", icon: LayoutDashboard },
  { href: "/admin/insights", label: "Insights", icon: TrendingUp },
  { href: "/admin/restaurants", label: "Restaurants", icon: Store },
  { href: "/admin/approvals", label: "Approvals", icon: FileCheck2 },
  { href: "/admin/cases", label: "Resolution desk", icon: Scale },
  { href: "/admin/reviews", label: "Reviews", icon: Star },
  { href: "/admin/moderation", label: "Moderation", icon: EyeOff },
  { href: "/admin/subscriptions", label: "Subscriptions", icon: CreditCard },
  {
    href: "/admin/platform-payments",
    label: "Platform payments",
    icon: Receipt,
  },
  { href: "/admin/markets", label: "Markets", icon: Globe2 },
  { href: "/admin/users", label: "Users & roles", icon: Users },
  { href: "/admin/bookings", label: "Bookings", icon: CalendarCheck },
  { href: "/admin/payments", label: "Payments", icon: Banknote },
  { href: "/admin/audit", label: "Audit trail", icon: History },
];

export default function AdminLayout({ children }: { children: React.ReactNode }) {
  return (
    <RequireRole roles={["ADMIN"]}>
      <PortalShell title="Admin" nav={NAV}>
        {children}
      </PortalShell>
    </RequireRole>
  );
}
