import type { Booking } from "./api/types";

export const STATUS_STYLE: Record<Booking["status"], string> = {
  CONFIRMED: "bg-green-50 text-veg",
  SEATED: "bg-brand-soft text-brand-strong",
  PENDING_PAYMENT: "bg-amber-50 text-amber-800",
  CANCELLED: "bg-red-50 text-danger",
  COMPLETED: "bg-canvas text-muted",
  NO_SHOW: "bg-canvas text-muted",
};
