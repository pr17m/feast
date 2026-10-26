"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import clsx from "clsx";
import { Bell, CheckCheck } from "lucide-react";
import { useMutation, useQuery } from "@/lib/hooks";
import type { Notification } from "@/lib/api/types";
import { useLive } from "./live";
import { useAuth, useToast } from "./providers";
import { Button, Empty, Sheet, Skeleton } from "./ui";

function ago(iso: string): string {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86_400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** Bell with unread badge; opens the notification list. Hidden when signed out. */
export function NotificationBell({ className }: { className?: string }) {
  const { user } = useAuth();
  const { unread, setUnread, latest } = useLive();
  const toast = useToast();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const list = useQuery(user && open ? `notifications:${user.id}` : null, (api) => api.listNotifications(false, 50));
  const markRead = useMutation((api, ids?: string[]) => api.markNotificationsRead(ids));

  // Pop a toast for notifications that arrive while the app is open.
  useEffect(() => {
    if (latest) toast(latest.title);
  }, [latest, toast]);

  if (!user) return null;

  const openItem = async (n: Notification) => {
    if (!n.readAt) {
      const r = await markRead.run([n.id]);
      if (r) setUnread(r.unread);
    }
    setOpen(false);
    if (n.link) router.push(n.link);
  };

  return (
    <>
      <button
        onClick={() => setOpen(true)}
        className={clsx("relative grid h-9 w-9 place-items-center rounded-lg text-muted hover:bg-canvas hover:text-ink", className)}
        aria-label={unread ? `Notifications, ${unread} unread` : "Notifications"}
      >
        <Bell className="h-5 w-5" aria-hidden />
        {unread > 0 && (
          <span className="absolute -right-0.5 -top-0.5 min-w-5 rounded-full bg-deal px-1 text-center text-[11px] font-bold leading-5 text-white">{unread > 99 ? "99+" : unread}</span>
        )}
      </button>
      <Sheet
        open={open}
        onClose={() => setOpen(false)}
        title="Notifications"
        footer={
          unread > 0 ? (
            <Button
              variant="secondary"
              className="w-full"
              loading={markRead.pending}
              onClick={async () => {
                const r = await markRead.run();
                if (r) setUnread(r.unread);
                list.reload();
              }}
            >
              <CheckCheck className="h-4 w-4" aria-hidden /> Mark all as read
            </Button>
          ) : undefined
        }
      >
        {!list.data && !list.error && <Skeleton className="h-40 w-full" />}
        {list.error && <p className="text-sm text-danger">{list.error.message}</p>}
        {list.data?.items.length === 0 && <Empty title="You're all caught up" body="Bookings, orders, payments and account updates show up here." />}
        <ul className="divide-y divide-line">
          {list.data?.items.map((n) => (
            <li key={n.id}>
              <button onClick={() => openItem(n)} className="flex w-full gap-3 py-3 text-left">
                <span className={clsx("mt-1.5 h-2 w-2 shrink-0 rounded-full", n.readAt ? "bg-transparent" : "bg-brand")} aria-hidden />
                <span className="min-w-0 flex-1">
                  <span className={clsx("block", !n.readAt && "font-semibold")}>{n.title}</span>
                  <span className="block text-sm text-muted">{n.body}</span>
                  <span className="text-xs text-faint">{ago(n.createdAt)}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      </Sheet>
    </>
  );
}
