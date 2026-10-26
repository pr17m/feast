"use client";

import clsx from "clsx";
import { Flame, Star } from "lucide-react";
import { useState, type ReactNode } from "react";
import type { MenuCategory, MenuItem } from "@/lib/api/types";
import { mediaUrl } from "@/lib/api/client";
import { money } from "@/lib/format";
import { VegMark } from "./ui";

/** Dish photo (owner upload or stock); hides itself if it can't load, so the row falls back cleanly. */
export function DishPhoto({ item, className }: { item: Pick<MenuItem, "image" | "name" | "imageCredit">; className?: string }) {
  const src = mediaUrl(item.image);
  const [failed, setFailed] = useState<string | null>(null);
  if (!src || failed === src) return null;
  return (
    // eslint-disable-next-line @next/next/no-img-element -- remote photos; next/image would need every host allow-listed
    <img
      src={src}
      alt={item.name}
      title={item.imageCredit ? `Photo: ${item.imageCredit.split(" (")[0]}` : undefined}
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      className={clsx("bg-line object-cover", className ?? "h-24 w-24 rounded-xl md:h-28 md:w-28")}
      onError={() => setFailed(src)}
    />
  );
}

export function MenuItemRow({ item, action }: { item: MenuItem; action?: ReactNode }) {
  return (
    <div className={clsx("flex gap-4 py-4", !item.available && "opacity-60")}>
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex items-center gap-2">
          <VegMark veg={item.isVeg} />
          {item.isBestseller && (
            <span className="inline-flex items-center gap-0.5 text-xs font-semibold text-deal">
              <Star className="h-3 w-3 fill-current" aria-hidden /> Bestseller
            </span>
          )}
          {!!item.spiceLevel && (
            <span className="inline-flex text-nonveg" aria-label={`Spice level ${item.spiceLevel} of 3`}>
              {Array.from({ length: item.spiceLevel }, (_, i) => (
                <Flame key={i} className="h-3.5 w-3.5" aria-hidden />
              ))}
            </span>
          )}
        </div>
        <h4 className="font-semibold">{item.name}</h4>
        <p className="text-sm font-semibold">
          {money(item.price)}
          {item.variants?.length ? <span className="font-normal text-muted"> onwards</span> : null}
        </p>
        <p className="line-clamp-2 text-sm text-muted">{item.description}</p>
        {!item.available && <p className="text-xs font-bold uppercase tracking-wide text-danger">Currently unavailable</p>}
      </div>
      {item.image ? (
        <div className="flex shrink-0 flex-col items-center">
          <DishPhoto item={item} />
          {action && <div className="-mt-4 flex justify-center drop-shadow-sm">{action}</div>}
        </div>
      ) : (
        action && <div className="flex shrink-0 items-center">{action}</div>
      )}
    </div>
  );
}

export function MenuList({
  categories,
  vegOnly,
  renderAction,
  stickyOffset = 56,
}: {
  categories: MenuCategory[];
  vegOnly?: boolean;
  renderAction?: (item: MenuItem) => ReactNode;
  stickyOffset?: number;
}) {
  const visible = categories
    .map((c) => ({ ...c, items: c.items.filter((i) => !vegOnly || i.isVeg) }))
    .filter((c) => c.items.length);
  return (
    <div>
      <nav
        className="no-scrollbar sticky z-20 -mx-4 flex gap-2 overflow-x-auto border-b border-line bg-canvas/95 px-4 py-2 backdrop-blur"
        style={{ top: stickyOffset }}
        aria-label="Menu categories"
      >
        {visible.map((c) => (
          <a
            key={c.id}
            href={`#${c.id}`}
            className="shrink-0 rounded-full bg-surface px-3 py-1.5 text-sm font-medium ring-1 ring-line hover:ring-brand"
          >
            {c.name}
          </a>
        ))}
      </nav>
      {visible.map((c) => (
        <section key={c.id} id={c.id} className="scroll-mt-32 pt-4">
          <h3 className="text-lg font-bold">
            {c.name} <span className="text-sm font-normal text-muted">({c.items.length})</span>
          </h3>
          <div className={clsx("divide-y divide-line")}>
            {c.items.map((i) => (
              <MenuItemRow key={i.id} item={i} action={renderAction?.(i)} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
