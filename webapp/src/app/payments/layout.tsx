import { RequireRole } from "@/components/require-role";

/** Any signed-in user. The backend scopes data to the caller. */
export default function Layout({ children }: { children: React.ReactNode }) {
  return <RequireRole>{children}</RequireRole>;
}
