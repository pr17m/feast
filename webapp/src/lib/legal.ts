/**
 * Business and legal details for the policy pages, read on the server at
 * request time. One deployment serves one country, so each deployment sets
 * its own values (see deploy/compose/*.env.example):
 *
 *   FEAST_LEGAL_NAME        registered company name, e.g. "Feast Dining Pty Ltd"
 *   FEAST_LEGAL_ID          registration shown with it, e.g. "ABN 12 345 678 901" or "CIN U72900KA2026PTC000000"
 *   FEAST_LEGAL_ADDRESS     registered address
 *   FEAST_SUPPORT_EMAIL     diner and restaurant support
 *   FEAST_SUPPORT_PHONE     optional
 *   FEAST_PRIVACY_EMAIL     privacy requests (defaults to the support email)
 *   FEAST_GRIEVANCE_OFFICER India: "Name, email" of the Grievance Officer
 *   FEAST_GOVERNING_LAW     e.g. "New South Wales, Australia" or "Karnataka, India"
 *   FEAST_LEGAL_UPDATED     date the policies were last updated, YYYY-MM-DD
 *   FEAST_LEGAL_REVIEWED    "true" once a lawyer has approved the text (hides the draft notice)
 */


export type LegalCountry = "AU" | "IN" | "OTHER";

export interface LegalInfo {
  country: LegalCountry;
  countryCode: string;
  name: string;
  registration?: string;
  address?: string;
  supportEmail: string;
  supportPhone?: string;
  privacyEmail: string;
  grievanceOfficer?: string;
  governingLaw: string;
  updated: string;
  reviewed: boolean;
  /** Settings still showing a placeholder, listed in the draft notice. */
  missing: string[];
}

const env = (k: string) => process.env[k]?.trim() || undefined;

export function legalInfo(): LegalInfo {
  const countryCode = (env("FEAST_COUNTRY") ?? "").toUpperCase();
  const country: LegalCountry = countryCode === "AU" ? "AU" : countryCode === "IN" ? "IN" : "OTHER";
  const missing: string[] = [];
  const need = (k: string, placeholder: string) => {
    const v = env(k);
    if (!v) missing.push(k);
    return v ?? placeholder;
  };
  const name = need("FEAST_LEGAL_NAME", "[Feast's registered company name]");
  const supportEmail = need("FEAST_SUPPORT_EMAIL", "[support email]");
  const address = need("FEAST_LEGAL_ADDRESS", "[registered address]");
  const registration = need("FEAST_LEGAL_ID", country === "AU" ? "[ABN]" : country === "IN" ? "[CIN]" : "[company number]");
  const grievanceOfficer = country === "IN" ? need("FEAST_GRIEVANCE_OFFICER", "[Grievance Officer name and email]") : env("FEAST_GRIEVANCE_OFFICER");
  const defaultLaw = country === "AU" ? "New South Wales, Australia" : country === "IN" ? "India" : "[governing law]";
  return {
    country,
    countryCode,
    name,
    registration,
    address,
    supportEmail,
    supportPhone: env("FEAST_SUPPORT_PHONE"),
    privacyEmail: env("FEAST_PRIVACY_EMAIL") ?? supportEmail,
    grievanceOfficer,
    governingLaw: env("FEAST_GOVERNING_LAW") ?? defaultLaw,
    updated: env("FEAST_LEGAL_UPDATED") ?? "2026-10-25",
    reviewed: env("FEAST_LEGAL_REVIEWED") === "true",
    missing,
  };
}
