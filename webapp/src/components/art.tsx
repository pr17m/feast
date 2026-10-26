"use client";

import clsx from "clsx";
import { useState } from "react";
import { Beer, Cake, Coffee, CookingPot, Fish, Flame, Pizza, Soup } from "lucide-react";
import type { CuisineMotif } from "@/lib/api/types";

const ICONS: Record<CuisineMotif, typeof Pizza> = {
  pizza: Pizza,
  noodles: Soup,
  curry: CookingPot,
  grill: Flame,
  cafe: Coffee,
  dessert: Cake,
  seafood: Fish,
  bar: Beer,
};

/**
 * Restaurant imagery. Shows the photo when there is one; the generated art
 * (deterministic per restaurant: hue + motif) sits underneath as the
 * placeholder while it loads, and stays if the photo is missing or fails.
 */
export function Art({
  hue,
  motif,
  className,
  iconClassName,
  label,
  image,
  credit,
}: {
  hue: number;
  motif?: CuisineMotif;
  className?: string;
  iconClassName?: string;
  label?: string;
  image?: string | null;
  credit?: string | null;
}) {
  const Icon = motif ? ICONS[motif] : null;
  const [failed, setFailed] = useState<string | null>(null);
  const showPhoto = !!image && failed !== image;
  return (
    <div
      // Callers may position it absolutely (as a background); otherwise it's a positioned box for the icon.
      className={clsx("overflow-hidden", !/\b(absolute|fixed)\b/.test(className ?? "") && "relative", className)}
      style={{
        background: `radial-gradient(120% 90% at 85% 10%, hsl(${hue} 70% 70% / 0.9), transparent 55%),
          linear-gradient(135deg, hsl(${hue} 55% 38%), hsl(${(hue + 30) % 360} 60% 22%))`,
      }}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      title={showPhoto && credit ? `Photo: ${credit}` : undefined}
    >
      <div
        className="absolute inset-0 opacity-[0.12]"
        style={{
          backgroundImage: "radial-gradient(circle at 1px 1px, white 1px, transparent 0)",
          backgroundSize: "14px 14px",
        }}
      />
      {Icon && (
        <Icon
          className={clsx("absolute -bottom-3 -right-3 h-24 w-24 rotate-[-12deg] text-white/25", iconClassName)}
          strokeWidth={1.25}
        />
      )}
      {showPhoto && (
        // eslint-disable-next-line @next/next/no-img-element -- remote photos of any size; next/image would need every host allow-listed
        <img
          src={image}
          alt=""
          loading="lazy"
          decoding="async"
          referrerPolicy="no-referrer"
          className="absolute inset-0 h-full w-full object-cover"
          onError={() => setFailed(image)}
        />
      )}
    </div>
  );
}
