"use client";

import clsx from "clsx";
import { Download, PlusSquare, RefreshCw, Share, Smartphone, X } from "lucide-react";
import { useEffect, useState, useSyncExternalStore } from "react";
import { Button, Sheet } from "@/components/ui";
import {
  dismissInstall,
  installDismissed,
  installMode,
  promptInstall,
  registerServiceWorker,
  subscribeInstall,
  type InstallApp,
  type InstallMode,
} from "@/lib/pwa";

const NAME: Record<InstallApp, string> = { diner: "Feast", partner: "Feast Partner" };

/** Mounted once in the root layout: registers the service worker and offers updates. */
export function PwaBootstrap() {
  const [apply, setApply] = useState<(() => void) | null>(null);
  useEffect(() => registerServiceWorker((fn) => setApply(() => fn)), []);
  if (!apply) return null;
  return (
    <div
      role="status"
      className="fixed inset-x-4 bottom-24 z-50 mx-auto flex max-w-md items-center gap-3 rounded-2xl bg-ink px-4 py-3 text-sm text-white shadow-lg md:bottom-6"
    >
      <span className="flex-1">New version of Feast available.</span>
      <button type="button" onClick={apply} className="inline-flex items-center gap-1.5 rounded-lg bg-white/15 px-3 py-1.5 font-semibold hover:bg-white/25">
        <RefreshCw className="h-4 w-4" aria-hidden /> Refresh
      </button>
    </div>
  );
}

/** What we can offer for `app`, after hydration (null on the server and first render). */
function useInstallOffer(app: InstallApp): InstallMode {
  return useSyncExternalStore(
    subscribeInstall,
    () => (installDismissed(app) ? null : installMode()),
    () => null,
  );
}

/** "Install Feast" card: Chrome/Edge/Android get the browser's install dialog, iPhone/iPad get the Add to Home Screen steps. */
export function InstallAppCard({ app, className }: { app: InstallApp; className?: string }) {
  const mode = useInstallOffer(app);
  const [steps, setSteps] = useState(false);
  if (!mode) return null;
  const name = NAME[app];
  const blurb =
    app === "diner"
      ? "Open Feast from your home screen: today's booking and paying the bill are one tap away."
      : "Open Feast Partner from your home screen or desktop to check bookings and confirm counter payments fast.";
  return (
    <div className={clsx("relative flex items-start gap-3 rounded-[var(--radius-card)] bg-brand-soft p-4 ring-1 ring-brand/20", className)}>
      <Smartphone className="mt-0.5 h-6 w-6 shrink-0 text-brand" aria-hidden />
      <div className="min-w-0 flex-1 space-y-2">
        <p className="font-bold">{mode === "ios" ? `Add ${name} to your Home Screen` : `Install the ${name} app`}</p>
        <p className="text-sm text-muted">{blurb} No app store needed.</p>
        <div className="flex flex-wrap gap-2">
          {mode === "prompt" ? (
            <Button size="sm" onClick={() => void promptInstall()}>
              <Download className="h-4 w-4" aria-hidden /> Install
            </Button>
          ) : (
            <Button size="sm" onClick={() => setSteps(true)}>
              <PlusSquare className="h-4 w-4" aria-hidden /> Show me how
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={() => dismissInstall(app)}>
            Not now
          </Button>
        </div>
      </div>
      <button type="button" aria-label="Dismiss" onClick={() => dismissInstall(app)} className="rounded-lg p-1 text-muted hover:text-ink">
        <X className="h-4 w-4" aria-hidden />
      </button>
      <IosSteps open={steps} onClose={() => setSteps(false)} name={name} />
    </div>
  );
}

/** Compact sidebar entry ("Install app"); same behaviour as the card. */
export function InstallAppButton({ app, className }: { app: InstallApp; className?: string }) {
  const mode = useInstallOffer(app);
  const [steps, setSteps] = useState(false);
  if (!mode) return null;
  return (
    <>
      <button
        type="button"
        className={clsx("inline-flex items-center gap-2", className)}
        onClick={() => (mode === "prompt" ? void promptInstall() : setSteps(true))}
      >
        <Download className="h-4 w-4" aria-hidden /> Install app
      </button>
      <IosSteps open={steps} onClose={() => setSteps(false)} name={NAME[app]} />
    </>
  );
}

function IosSteps({ open, onClose, name }: { open: boolean; onClose: () => void; name: string }) {
  return (
    <Sheet open={open} onClose={onClose} title={`Add ${name} to your Home Screen`} footer={<Button onClick={onClose}>Got it</Button>}>
      <ol className="space-y-3 text-sm">
        <li className="flex items-start gap-3">
          <Step n={1} />
          <span>
            Tap <Share className="mx-0.5 inline h-4 w-4 align-text-bottom text-brand" aria-label="Share" /> <b>Share</b> in the browser toolbar (at the bottom
            in Safari on iPhone, at the top on iPad).
          </span>
        </li>
        <li className="flex items-start gap-3">
          <Step n={2} />
          <span>
            Scroll down and tap <b>Add to Home Screen</b> <PlusSquare className="mx-0.5 inline h-4 w-4 align-text-bottom text-brand" aria-hidden />.
          </span>
        </li>
        <li className="flex items-start gap-3">
          <Step n={3} />
          <span>
            Tap <b>Add</b>. {name} opens from your Home Screen like an app, full screen.
          </span>
        </li>
      </ol>
    </Sheet>
  );
}

function Step({ n }: { n: number }) {
  return <span className="grid h-6 w-6 shrink-0 place-items-center rounded-full bg-brand text-xs font-bold text-white">{n}</span>;
}
