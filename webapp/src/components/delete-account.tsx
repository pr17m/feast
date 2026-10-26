"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Bell, BellOff, Trash2 } from "lucide-react";
import { useAuth, useToast } from "@/components/providers";
import { Button, Sheet } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";

const INPUT = "h-11 w-full rounded-xl bg-canvas px-3 text-base ring-1 ring-line focus:outline-none focus:ring-2 focus:ring-brand";

/**
 * "Delete my account" (App Store / Google Play requirement): explains what
 * happens, re-confirms identity (a fresh code for phone sign-in, the password
 * for restaurant accounts) and the word DELETE, then signs out.
 */
export function DeleteAccountCard({ className }: { className?: string }) {
  const { user, signOut } = useAuth();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [word, setWord] = useState("");
  const [otp, setOtp] = useState("");
  const [password, setPassword] = useState("");
  const [codeSent, setCodeSent] = useState(false);
  const send = useMutation((api, phone: string) => api.requestOtp(phone));
  const del = useMutation((api) => api.deleteAccount({ confirm: word, otp: otp || undefined, password: password || undefined }));

  if (!user || user.role === "ADMIN") return null;
  const owner = user.role === "OWNER";
  const phoneOtp = !owner && !!user.phone;
  const ready = word.trim().toUpperCase() === "DELETE" && (owner ? password.length > 0 : !phoneOtp || otp.trim().length >= 4);

  const submit = async () => {
    const res = await del.run();
    if (!res) return;
    setOpen(false);
    // The account no longer exists: drop the local session (the server already refuses the token).
    await signOut().catch(() => {});
    router.replace("/account-deletion?done=1");
  };

  return (
    <section className={className ?? "rounded-3xl bg-surface p-5 ring-1 ring-line"} aria-labelledby="delete-account">
      <h2 id="delete-account" className="mb-2 flex items-center gap-2 font-bold">
        <Trash2 className="h-5 w-5 text-danger" aria-hidden /> Delete my account
      </h2>
      <ul className="mb-4 list-disc space-y-1 pl-5 text-sm text-muted">
        <li>Your upcoming bookings are cancelled{owner ? "" : " and the restaurants are told"}.</li>
        <li>Your name, phone number, email and sign-in links are removed.</li>
        <li>Receipts, reviews and dispute records are kept without your name, because the law requires them.</li>
        {owner && <li>Restaurants you are the only owner of are paused and hidden from diners.</li>}
        <li>This can&apos;t be undone.</li>
      </ul>
      <p className="mb-4 text-sm text-muted">
        <Link href="/account-deletion" className="font-semibold text-brand hover:underline">
          What we delete and what we keep
        </Link>
      </p>
      <Button variant="secondary" className="text-danger" onClick={() => setOpen(true)}>
        <Trash2 className="h-4 w-4" aria-hidden /> Delete my account…
      </Button>

      <Sheet
        open={open}
        onClose={del.pending ? () => {} : () => setOpen(false)}
        title="Delete your Feast account"
        footer={
          <Button size="lg" className="w-full bg-danger hover:bg-danger" loading={del.pending} disabled={!ready} onClick={submit}>
            Delete my account
          </Button>
        }
      >
        <div className="space-y-4">
          {phoneOtp && (
            <div className="space-y-2">
              <p className="text-sm">
                First, confirm it&apos;s you: we&apos;ll text a code to <b>{user.phone}</b>.
              </p>
              <Button
                variant="secondary"
                size="sm"
                loading={send.pending}
                onClick={async () => {
                  if (await send.run(user.phone!)) setCodeSent(true);
                }}
              >
                {codeSent ? "Send the code again" : "Send me a code"}
              </Button>
              {send.error && <p className="text-sm text-danger">{send.error.message}</p>}
              <label className="block text-sm font-semibold">
                Code
                <input className={INPUT} inputMode="numeric" autoComplete="one-time-code" value={otp} onChange={(e) => setOtp(e.target.value)} />
              </label>
            </div>
          )}
          {owner && (
            <label className="block text-sm font-semibold">
              Your password
              <input className={INPUT} type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} />
            </label>
          )}
          <label className="block text-sm font-semibold">
            Type DELETE to confirm
            <input className={INPUT} autoCapitalize="characters" value={word} onChange={(e) => setWord(e.target.value)} />
          </label>
          {del.error && (
            <p role="alert" className="rounded-xl bg-red-50 p-3 text-sm text-danger">
              {del.error.message}
            </p>
          )}
        </div>
      </Sheet>
    </section>
  );
}

/** Phone notifications on/off for this account (all the user's phones). */
export function PushSettingsCard({ className }: { className?: string }) {
  const { user } = useAuth();
  const toast = useToast();
  const q = useQuery(user ? `push-settings:${user.id}` : null, (api) => api.getPushSettings());
  const save = useMutation((api, enabled: boolean) => api.setPushSettings({ enabled }));
  if (!user || !q.data) return null;
  const on = q.data.enabled;
  return (
    <section className={className ?? "flex items-center gap-3 rounded-3xl bg-surface p-5 ring-1 ring-line"}>
      <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-brand-soft text-brand">
        {on ? <Bell className="h-5 w-5" aria-hidden /> : <BellOff className="h-5 w-5" aria-hidden />}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block font-semibold">Phone notifications</span>
        <span className="block text-sm text-muted">{on ? "Bookings, payments and replies arrive on your phone." : "Off — you'll still see them in the app."}</span>
      </span>
      <Button
        variant="secondary"
        size="sm"
        loading={save.pending}
        onClick={async () => {
          const res = await save.run(!on);
          if (res) {
            q.reload();
            toast(res.enabled ? "Phone notifications on" : "Phone notifications off");
          }
        }}
      >
        {on ? "Turn off" : "Turn on"}
      </Button>
    </section>
  );
}
