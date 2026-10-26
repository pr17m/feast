"use client";

import clsx from "clsx";
import type { ReactNode } from "react";
import { Chip } from "./ui";

/** 0.256 → "26%". */
export function pct(x: number | undefined | null, digits = 0): string {
  return x === undefined || x === null ? "—" : `${(x * 100).toFixed(digits)}%`;
}

export function PeriodPicker<T extends number>({
  value,
  options,
  onChange,
  label = "Period",
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label?: string;
}) {
  return (
    <div role="group" aria-label={label} className="no-scrollbar flex gap-2 overflow-x-auto">
      {options.map((o) => (
        <Chip key={o.value} active={o.value === value} onClick={() => onChange(o.value)}>
          {o.label}
        </Chip>
      ))}
    </div>
  );
}

export function Panel({ title, subtitle, children, className }: { title: string; subtitle?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={clsx("rounded-2xl bg-surface p-4 ring-1 ring-line md:p-5", className)}>
      <h2 className="font-bold">{title}</h2>
      {subtitle && <p className="text-sm text-muted">{subtitle}</p>}
      <div className="mt-4">{children}</div>
    </section>
  );
}

/** Horizontal bars: one row per item, value text on the right. */
export function BarList({ rows, empty = "Nothing yet" }: { rows: { label: ReactNode; value: number; display: ReactNode; key?: string }[]; empty?: string }) {
  if (!rows.length) return <p className="text-sm text-muted">{empty}</p>;
  const max = Math.max(1, ...rows.map((r) => r.value));
  return (
    <ul className="space-y-2.5">
      {rows.map((r, i) => (
        <li key={r.key ?? i} className="grid grid-cols-[minmax(0,9rem)_1fr_auto] items-center gap-3 text-sm">
          <span className="truncate">{r.label}</span>
          <span className="h-2.5 overflow-hidden rounded-full bg-canvas" aria-hidden>
            <span className="block h-full rounded-full bg-brand" style={{ width: `${Math.max(2, (r.value / max) * 100)}%` }} />
          </span>
          <span className="tabular-nums font-semibold">{r.display}</span>
        </li>
      ))}
    </ul>
  );
}

/** Vertical columns with a screen-reader table of the same numbers. */
export function Columns({
  data,
  label,
  format,
  highlight = "max",
}: {
  data: { label: string; value: number }[];
  label: string;
  format: (v: number) => string;
  highlight?: "max" | "none";
}) {
  const max = Math.max(1, ...data.map((d) => d.value));
  const top = highlight === "max" ? Math.max(...data.map((d) => d.value)) : -1;
  return (
    <figure>
      <div className="flex h-36 items-end gap-1.5 md:gap-3" aria-hidden>
        {data.map((d) => (
          <div key={d.label} className="flex h-full min-w-0 flex-1 flex-col items-center justify-end gap-1">
            <span className="text-[10px] tabular-nums text-muted md:text-xs">{d.value ? format(d.value) : ""}</span>
            <div className={clsx("w-full rounded-t-md", d.value === top && top > 0 ? "bg-brand" : "bg-brand/35")} style={{ height: `${Math.max(d.value ? 4 : 1, (d.value / max) * 100)}%` }} />
            <span className="truncate text-xs text-muted">{d.label}</span>
          </div>
        ))}
      </div>
      <table className="sr-only">
        <caption>{label}</caption>
        <tbody>
          {data.map((d) => (
            <tr key={d.label}>
              <th scope="row">{d.label}</th>
              <td>{format(d.value)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}

/** A labelled progress meter (0–1). */
export function Meter({ value, label, hint }: { value: number; label: string; hint?: string }) {
  const v = Math.min(1, Math.max(0, value));
  return (
    <div>
      <div className="flex justify-between text-sm">
        <span className="font-semibold">{label}</span>
        <span className="tabular-nums">{pct(value)}</span>
      </div>
      <div className="mt-1.5 h-2.5 overflow-hidden rounded-full bg-canvas" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v * 100)} aria-label={label}>
        <div className={clsx("h-full rounded-full", v >= 0.9 ? "bg-deal" : "bg-brand")} style={{ width: `${v * 100}%` }} />
      </div>
      {hint && <p className="mt-1 text-xs text-muted">{hint}</p>}
    </div>
  );
}

/** "2026-10" → "Oct". */
export function monthLabel(ym: string): string {
  const [y, m] = ym.split("-").map(Number);
  if (!y || !m) return ym;
  return new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString(undefined, { month: "short", timeZone: "UTC" });
}

/** 19 → "7pm". */
export function hourLabel(h: number): string {
  const suffix = h < 12 ? "am" : "pm";
  const n = h % 12 === 0 ? 12 : h % 12;
  return `${n}${suffix}`;
}
