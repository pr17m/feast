"use client";

import { Suspense, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import clsx from "clsx";
import { Plus, Search } from "lucide-react";
import { PageHeader } from "@/components/portal-shell";
import { useAuth, useToast } from "@/components/providers";
import { Button, Chip, Empty, ErrorState, Sheet, Skeleton } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import type { RestaurantDetail, Role, User, UserPatch } from "@/lib/api/types";

const ROLES: Role[] = ["CUSTOMER", "OWNER", "ADMIN"];
const ROLE_STYLE: Record<Role, string> = {
  CUSTOMER: "bg-canvas text-muted",
  OWNER: "bg-brand-soft text-brand-strong",
  ADMIN: "bg-ink text-white",
};
const field = "h-10 w-full rounded-lg bg-canvas px-3 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand";

function UserSheet({
  user,
  restaurants,
  onClose,
  onSaved,
  isSelf,
}: {
  user: User | null;
  restaurants: RestaurantDetail[];
  onClose: () => void;
  onSaved: () => void;
  isSelf: boolean;
}) {
  const [role, setRole] = useState<Role>(user?.role ?? "CUSTOMER");
  const [ids, setIds] = useState<string[]>(user?.restaurantIds ?? []);
  const save = useMutation((api, id: string, patch: UserPatch) => api.updateUser(id, patch));
  if (!user) return null;
  return (
    <Sheet
      open
      onClose={onClose}
      title={user.name}
      footer={
        <div className="flex gap-3">
          <Button
            variant="secondary"
            className={clsx("flex-1", user.status === "ACTIVE" && "text-danger")}
            disabled={isSelf}
            loading={save.pending}
            onClick={async () => {
              if (
                await save.run(user.id, {
                  status: user.status === "ACTIVE" ? "BLOCKED" : "ACTIVE",
                })
              )
                onSaved();
            }}
          >
            {user.status === "ACTIVE" ? "Block user" : "Unblock user"}
          </Button>
          <Button
            className="flex-1"
            loading={save.pending}
            onClick={async () => {
              if (
                await save.run(user.id, {
                  role,
                  restaurantIds: role === "OWNER" ? ids : [],
                })
              )
                onSaved();
            }}
          >
            Save access
          </Button>
        </div>
      }
    >
      <div className="space-y-4">
        <p className="text-sm text-muted">
          {user.email ?? "No email"} · {user.phone ?? "No phone"} · joined{" "}
          {new Date(user.createdAt).toLocaleDateString(undefined, {
            month: "short",
            year: "numeric",
          })}
        </p>
        <fieldset>
          <legend className="mb-2 font-semibold">Role</legend>
          <div className="grid grid-cols-3 gap-2">
            {ROLES.map((r) => (
              <label
                key={r}
                className={clsx(
                  "cursor-pointer rounded-xl p-3 text-center text-sm font-semibold ring-1",
                  role === r ? "bg-brand-soft ring-brand" : "ring-line",
                  isSelf && r !== "ADMIN" && "opacity-50",
                )}
              >
                <input type="radio" name="role" className="sr-only" checked={role === r} disabled={isSelf && r !== "ADMIN"} onChange={() => setRole(r)} />
                {r.toLowerCase()}
              </label>
            ))}
          </div>
          <p className="mt-2 text-xs text-muted">
            {role === "CUSTOMER" && "Can browse, book, order and pay."}
            {role === "OWNER" && "Can manage only the restaurants ticked below. Needs an email to sign in."}
            {role === "ADMIN" && "Full control of every restaurant, user, booking and payment."}
          </p>
        </fieldset>
        {role === "OWNER" && (
          <fieldset>
            <legend className="mb-2 font-semibold">Restaurants they manage</legend>
            <ul className="max-h-64 space-y-1 overflow-y-auto">
              {restaurants.map((r) => (
                <li key={r.id}>
                  <label className="flex cursor-pointer items-center gap-2 rounded-lg px-2 py-2 hover:bg-canvas">
                    <input
                      type="checkbox"
                      checked={ids.includes(r.id)}
                      onChange={(e) => setIds((x) => (e.target.checked ? [...x, r.id] : x.filter((i) => i !== r.id)))}
                      className="accent-[var(--color-brand)]"
                    />
                    <span className="text-sm">
                      {r.name} <span className="text-muted">· {r.area}</span>
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          </fieldset>
        )}
        {save.error && (
          <p role="alert" className="rounded-lg bg-red-50 p-3 text-sm text-danger">
            {save.error.message}
          </p>
        )}
      </div>
    </Sheet>
  );
}

function CreateUserSheet({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: () => void }) {
  const [f, setF] = useState({
    name: "",
    email: "",
    phone: "",
    role: "OWNER" as Role,
  });
  const create = useMutation((api, input: Parameters<typeof api.createUser>[0]) => api.createUser(input));
  return (
    <Sheet
      open={open}
      onClose={onClose}
      title="Add a user"
      footer={
        <Button
          className="w-full"
          loading={create.pending}
          onClick={async () => {
            if (
              await create.run({
                name: f.name,
                email: f.email || undefined,
                phone: f.phone || undefined,
                role: f.role,
              })
            )
              onCreated();
          }}
        >
          Create user
        </Button>
      }
    >
      <div className="space-y-3">
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Name</span>
          <input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} className={field} autoFocus />
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Email (required for owners/admins)</span>
          <input type="email" value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} className={field} />
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Mobile (optional)</span>
          <input
            value={f.phone}
            onChange={(e) =>
              setF({
                ...f,
                phone: e.target.value.replace(/\D/g, "").slice(0, 10),
              })
            }
            inputMode="numeric"
            className={field}
          />
        </label>
        <label className="block">
          <span className="mb-1 block text-xs font-semibold text-muted">Role</span>
          <select value={f.role} onChange={(e) => setF({ ...f, role: e.target.value as Role })} className={field}>
            {ROLES.map((r) => (
              <option key={r} value={r}>
                {r.toLowerCase()}
              </option>
            ))}
          </select>
        </label>
        <p className="text-xs text-muted">
          New owners and admins sign in with the initial partner password set on the server, then change it. Assign restaurants after creating.
        </p>
        {create.error && (
          <p role="alert" className="text-sm text-danger">
            {create.error.message}
          </p>
        )}
      </div>
    </Sheet>
  );
}

function AdminUsersInner() {
  const sp = useSearchParams();
  const router = useRouter();
  const toast = useToast();
  const { user: me, refresh } = useAuth();
  const roleFilter = sp.get("role") as Role | null;
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<User | null>(null);
  const [creating, setCreating] = useState(false);
  const users = useQuery("adminUsers", (api) => api.adminUsers());
  const restaurants = useQuery("adminRestaurants", (api) => api.adminRestaurants());

  if (users.error) return <ErrorState error={users.error} onRetry={users.reload} />;
  if (!users.data || !restaurants.data) return <Skeleton className="h-96 w-full" />;
  const byId = new Map(restaurants.data.map((r) => [r.id, r.name]));
  const term = q.trim().toLowerCase();
  const rows = users.data.filter(
    (u) => (!roleFilter || u.role === roleFilter) && (!term || `${u.name} ${u.email ?? ""} ${u.phone ?? ""}`.toLowerCase().includes(term)),
  );

  const managed = (u: User) =>
    u.role === "OWNER" ? u.restaurantIds.map((id) => byId.get(id) ?? id).join(", ") || "None assigned" : u.role === "ADMIN" ? "All restaurants" : "—";

  return (
    <div className="space-y-5">
      <PageHeader
        title="Users & roles"
        subtitle="Grant partner access, link owners to restaurants, or block accounts."
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="h-4 w-4" aria-hidden /> Add user
          </Button>
        }
      />
      <div className="flex flex-col gap-3 md:flex-row md:items-center">
        <label className="flex h-10 items-center gap-2 rounded-xl bg-surface px-3 ring-1 ring-line focus-within:ring-2 focus-within:ring-brand md:w-72">
          <Search className="h-4 w-4 text-muted" aria-hidden />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Name, email or phone"
            className="w-full bg-transparent text-sm outline-none"
            aria-label="Search users"
          />
        </label>
        <div className="flex gap-2">
          <Chip active={!roleFilter} onClick={() => router.replace("/admin/users")}>
            All
          </Chip>
          {ROLES.map((r) => (
            <Chip key={r} active={roleFilter === r} onClick={() => router.replace(`/admin/users?role=${r}`)}>
              {r.toLowerCase()}s
            </Chip>
          ))}
        </div>
      </div>

      {rows.length === 0 && <Empty title="No users match" />}

      <div className="hidden overflow-x-auto rounded-2xl bg-surface ring-1 ring-line md:block">
        <table className="w-full text-left text-sm">
          <thead className="bg-canvas text-xs uppercase tracking-wide text-muted">
            <tr>
              <th className="px-4 py-3">User</th>
              <th className="px-4 py-3">Role</th>
              <th className="px-4 py-3">Manages</th>
              <th className="px-4 py-3">Status</th>
              <th className="px-4 py-3" />
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {rows.map((u) => (
              <tr key={u.id} className={clsx(u.status === "BLOCKED" && "opacity-60")}>
                <td className="px-4 py-3">
                  <p className="font-semibold">
                    {u.name} {u.id === me?.id && <span className="text-xs text-muted">(you)</span>}
                  </p>
                  <p className="text-xs text-muted">{[u.email, u.phone].filter(Boolean).join(" · ")}</p>
                </td>
                <td className="px-4 py-3">
                  <span className={clsx("rounded-md px-2 py-0.5 text-xs font-bold", ROLE_STYLE[u.role])}>{u.role.toLowerCase()}</span>
                </td>
                <td className="max-w-xs px-4 py-3 text-muted">{managed(u)}</td>
                <td className="px-4 py-3">
                  {u.status === "ACTIVE" ? <span className="text-veg">Active</span> : <span className="font-semibold text-danger">Blocked</span>}
                </td>
                <td className="px-4 py-3 text-right">
                  <Button size="sm" variant="secondary" onClick={() => setEditing(u)}>
                    Manage access
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <ul className="space-y-3 md:hidden">
        {rows.map((u) => (
          <li key={u.id}>
            <button
              onClick={() => setEditing(u)}
              className={clsx("w-full rounded-2xl bg-surface p-4 text-left ring-1 ring-line", u.status === "BLOCKED" && "opacity-60")}
            >
              <div className="flex items-center justify-between gap-2">
                <p className="font-bold">{u.name}</p>
                <span className={clsx("rounded-md px-2 py-0.5 text-xs font-bold", ROLE_STYLE[u.role])}>{u.role.toLowerCase()}</span>
              </div>
              <p className="text-xs text-muted">{[u.email, u.phone].filter(Boolean).join(" · ")}</p>
              {u.role !== "CUSTOMER" && <p className="mt-1 text-sm text-muted">{managed(u)}</p>}
              {u.status === "BLOCKED" && <p className="mt-1 text-sm font-semibold text-danger">Blocked</p>}
            </button>
          </li>
        ))}
      </ul>

      <UserSheet
        key={editing?.id ?? "none"}
        user={editing}
        isSelf={editing?.id === me?.id}
        restaurants={restaurants.data}
        onClose={() => setEditing(null)}
        onSaved={() => {
          setEditing(null);
          toast("Access updated");
          users.reload();
          refresh();
        }}
      />
      <CreateUserSheet
        open={creating}
        onClose={() => setCreating(false)}
        onCreated={() => {
          setCreating(false);
          toast("User created");
          users.reload();
        }}
      />
    </div>
  );
}

export default function AdminUsers() {
  return (
    <Suspense fallback={<Skeleton className="h-96 w-full" />}>
      <AdminUsersInner />
    </Suspense>
  );
}
