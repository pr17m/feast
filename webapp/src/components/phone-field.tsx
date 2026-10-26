"use client";

import clsx from "clsx";
import { useId } from "react";
import { useMarkets } from "./providers";
import { marketForPhone, toE164 } from "@/lib/api/market";
import type { Market } from "@/lib/api/types";

/** A phone number being typed: country (market) + national digits. */
export interface PhoneValue {
  country: string;
  national: string;
}

/** E.164 if the number is valid for the chosen country, else null. */
export function phoneE164(markets: Market[], v: PhoneValue): string | null {
  const m = markets.find((x) => x.countryCode === v.country);
  return m ? toE164(m, v.national) : null;
}

/** Split a stored E.164 number back into country + national digits. */
export function phoneFromE164(markets: Market[], e164: string | undefined, fallbackCountry: string): PhoneValue {
  const m = e164 ? marketForPhone(markets, e164) : undefined;
  return m && e164 ? { country: m.countryCode, national: e164.slice(m.phone.dialCode.length) } : { country: fallbackCountry, national: "" };
}

/**
 * Country code picker + national number. Countries come from live markets, so
 * launching a market adds its dial code here automatically.
 */
export function PhoneField({
  value,
  onChange,
  label = "Mobile number",
  tall,
  autoFocus,
  showError = true,
}: {
  value: PhoneValue;
  onChange: (v: PhoneValue) => void;
  label?: string;
  tall?: boolean;
  autoFocus?: boolean;
  showError?: boolean;
}) {
  const { markets } = useMarkets();
  const id = useId();
  const live = markets.filter((m) => m.status === "LIVE");
  const market = markets.find((m) => m.countryCode === value.country) ?? live[0];
  const valid = !!market && !!toE164(market, value.national);
  const invalid = showError && value.national.replace(/\D/g, "").length >= 6 && !valid;
  return (
    <div>
      <label htmlFor={id} className="mb-1 block text-sm font-medium">
        {label}
      </label>
      <div
        className={clsx(
          "flex items-center rounded-xl bg-canvas ring-1 focus-within:ring-2 focus-within:ring-brand",
          tall ? "h-12" : "h-11",
          invalid ? "ring-danger" : "ring-line",
        )}
      >
        <select
          aria-label="Country code"
          value={market?.countryCode ?? ""}
          onChange={(e) => onChange({ ...value, country: e.target.value })}
          className="h-full rounded-l-xl bg-transparent pl-3 pr-1 text-muted outline-none"
        >
          {live.map((m) => (
            <option key={m.countryCode} value={m.countryCode}>
              {m.phone.dialCode} {m.countryCode}
            </option>
          ))}
        </select>
        <input
          id={id}
          value={value.national}
          onChange={(e) => onChange({ ...value, national: e.target.value.replace(/[^\d ]/g, "").slice(0, 16) })}
          inputMode="tel"
          autoComplete="tel-national"
          placeholder={market?.phone.example}
          aria-invalid={invalid}
          className="h-full w-full bg-transparent pl-2 pr-3 outline-none"
          autoFocus={autoFocus}
        />
      </div>
      {invalid && <span className="mt-1 block text-sm text-danger">Enter a valid mobile number for {market?.name}</span>}
    </div>
  );
}
