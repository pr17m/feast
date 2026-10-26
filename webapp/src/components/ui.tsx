"use client";

import clsx from "clsx";
import Link from "next/link";
import { useEffect, type ButtonHTMLAttributes, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Loader2, Minus, Plus, Star, X, AlertTriangle, BadgePercent } from "lucide-react";

// ---------- Button ----------

type Variant = "primary" | "secondary" | "ghost" | "deal";

const variants: Record<Variant, string> = {
  primary: "bg-brand text-white hover:bg-brand-strong disabled:bg-faint",
  secondary: "bg-surface text-ink ring-1 ring-line hover:bg-canvas disabled:text-faint",
  ghost: "text-brand hover:bg-brand-soft disabled:text-faint",
  deal: "bg-deal text-white hover:brightness-95 disabled:bg-faint",
};

export function Button({
  variant = "primary",
  size = "md",
  loading,
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: "sm" | "md" | "lg"; loading?: boolean }) {
  return (
    <button
      {...rest}
      disabled={rest.disabled || loading}
      className={clsx(
        "inline-flex items-center justify-center gap-2 rounded-xl font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed",
        size === "sm" && "h-9 px-3 text-sm",
        size === "md" && "h-11 px-4 text-sm",
        size === "lg" && "h-12 px-5 text-base",
        variants[variant],
        className,
      )}
    >
      {loading && <Loader2 className="h-4 w-4 animate-spin" aria-hidden />}
      {children}
    </button>
  );
}

export function LinkButton({
  href,
  variant = "primary",
  className,
  children,
}: {
  href: string;
  variant?: Variant;
  className?: string;
  children: ReactNode;
}) {
  return (
    <Link
      href={href}
      className={clsx(
        "inline-flex h-11 items-center justify-center gap-2 rounded-xl px-4 text-sm font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand",
        variants[variant],
        className,
      )}
    >
      {children}
    </Link>
  );
}

// ---------- Chip ----------

export function Chip({
  active,
  onClick,
  children,
  className,
}: {
  active?: boolean;
  onClick?: () => void;
  children: ReactNode;
  className?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={clsx(
        "inline-flex h-9 shrink-0 items-center gap-1.5 rounded-full px-3.5 text-sm font-medium ring-1 transition-colors",
        active ? "bg-brand-soft text-brand-strong ring-brand" : "bg-surface text-ink ring-line hover:ring-faint",
        className,
      )}
    >
      {children}
    </button>
  );
}

// ---------- Stepper ----------

export function Stepper({
  value,
  min = 0,
  max = 99,
  onChange,
  label,
  size = "md",
}: {
  value: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
  label: string;
  size?: "sm" | "md";
}) {
  const btn = clsx(
    "grid place-items-center rounded-lg text-brand hover:bg-brand-soft disabled:text-faint disabled:hover:bg-transparent",
    size === "sm" ? "h-7 w-7" : "h-9 w-9",
  );
  return (
    <div className="inline-flex items-center gap-1 rounded-xl bg-surface p-0.5 ring-1 ring-line" role="group" aria-label={label}>
      <button type="button" className={btn} onClick={() => onChange(value - 1)} disabled={value <= min} aria-label={`Decrease ${label}`}>
        <Minus className="h-4 w-4" />
      </button>
      <span className={clsx("min-w-6 text-center font-semibold tabular-nums", size === "sm" ? "text-sm" : "text-base")} aria-live="polite">
        {value}
      </span>
      <button type="button" className={btn} onClick={() => onChange(value + 1)} disabled={value >= max} aria-label={`Increase ${label}`}>
        <Plus className="h-4 w-4" />
      </button>
    </div>
  );
}

// ---------- Sheet (bottom sheet on mobile, dialog on desktop) ----------

export function Sheet({
  open,
  onClose,
  title,
  children,
  footer,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = prev;
    };
  }, [open, onClose]);

  if (!open || typeof document === "undefined") return null;
  // Portal to <body> so ancestors with transforms/filters (e.g. a blurred sticky header) can't trap or clip the overlay.
  return createPortal(
    <div className="fixed inset-0 z-50 flex items-end justify-center md:items-center" role="dialog" aria-modal="true" aria-label={title}>
      <div className="animate-fade-in absolute inset-0 bg-ink/50" onClick={onClose} />
      <div className="animate-sheet-in relative flex max-h-[88vh] w-full flex-col rounded-t-3xl bg-surface shadow-[var(--shadow-sheet)] md:max-w-lg md:rounded-3xl">
        <div className="flex items-center justify-between gap-4 border-b border-line px-5 py-4">
          <h2 className="text-lg font-bold">{title}</h2>
          <button onClick={onClose} className="grid h-9 w-9 place-items-center rounded-full hover:bg-canvas" aria-label="Close">
            <X className="h-5 w-5" />
          </button>
        </div>
        <div className="flex-1 overflow-y-auto px-5 py-4">{children}</div>
        {footer && <div className="border-t border-line px-5 py-4 pb-[max(1rem,env(safe-area-inset-bottom))]">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}

// ---------- Small display pieces ----------

export function Rating({ value, count, className }: { value: number; count?: number; className?: string }) {
  return (
    <span className={clsx("inline-flex items-center gap-1 text-sm", className)}>
      <span className="inline-flex items-center gap-0.5 rounded-md bg-rating px-1.5 py-0.5 text-xs font-bold text-white">
        {value.toFixed(1)}
        <Star className="h-3 w-3 fill-current" aria-hidden />
      </span>
      {count !== undefined && <span className="text-muted">({count >= 1000 ? `${(count / 1000).toFixed(1)}K` : count})</span>}
      <span className="sr-only">rated {value} out of 5</span>
    </span>
  );
}

export function VegMark({ veg, className }: { veg: boolean; className?: string }) {
  return (
    <span
      className={clsx("inline-grid h-4 w-4 shrink-0 place-items-center rounded-[3px] border-[1.5px]", veg ? "border-veg" : "border-nonveg", className)}
      title={veg ? "Vegetarian" : "Non-vegetarian"}
    >
      <span className={clsx("h-2 w-2 rounded-full", veg ? "bg-veg" : "bg-nonveg")} />
      <span className="sr-only">{veg ? "Vegetarian" : "Non-vegetarian"}</span>
    </span>
  );
}

export function OfferTag({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span className={clsx("inline-flex items-center gap-1 text-sm font-semibold text-deal", className)}>
      <BadgePercent className="h-4 w-4 shrink-0" aria-hidden />
      {children}
    </span>
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={clsx("animate-pulse rounded-xl bg-line/70", className)} />;
}

export function ErrorState({ error, onRetry }: { error: Error; onRetry?: () => void }) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-[var(--radius-card)] bg-surface px-6 py-10 text-center ring-1 ring-line">
      <AlertTriangle className="h-8 w-8 text-deal" aria-hidden />
      <p className="font-semibold">{error.message || "Something went wrong"}</p>
      {onRetry && (
        <Button variant="secondary" size="sm" onClick={onRetry}>
          Try again
        </Button>
      )}
    </div>
  );
}

export function Empty({ title, body, action }: { title: string; body?: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-[var(--radius-card)] bg-surface px-6 py-12 text-center ring-1 ring-line">
      <p className="text-base font-semibold">{title}</p>
      {body && <p className="max-w-sm text-sm text-muted">{body}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function SectionTitle({ title, subtitle, action }: { title: string; subtitle?: string; action?: ReactNode }) {
  return (
    <div className="mb-3 flex items-end justify-between gap-4">
      <div>
        <h2 className="text-lg font-bold tracking-tight md:text-xl">{title}</h2>
        {subtitle && <p className="text-sm text-muted">{subtitle}</p>}
      </div>
      {action}
    </div>
  );
}

export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={clsx("rounded-[var(--radius-card)] bg-surface p-4 ring-1 ring-line", className)}>{children}</div>;
}
