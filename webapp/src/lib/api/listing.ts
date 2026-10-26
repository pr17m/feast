/**
 * Limits for owner-entered listing data, mirrored from the backend
 * (`feast_domain::listing`), which cleans the data and builds search filters
 * from live listings. The UI uses these only to guide input.
 */
export const LISTING_LIMITS = { cuisines: 6, tags: 8, label: 30, about: 600, highlights: 6 };
