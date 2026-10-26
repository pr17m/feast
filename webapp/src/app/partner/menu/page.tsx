"use client";

import { useRef, useState } from "react";
import { ModerateButton, ModerationNotice } from "@/components/trust";
import { Camera, Check, ImageOff, Pencil, Search, Sparkles, Trash2 } from "lucide-react";
import { DishPhoto } from "@/components/menu-list";
import { getApi } from "@/lib/api/client";
import { PageHeader, Toggle } from "@/components/portal-shell";
import { useToast } from "@/components/providers";
import { Button, Empty, ErrorState, Skeleton, VegMark } from "@/components/ui";
import { useMutation, useQuery } from "@/lib/hooks";
import { currencySymbol, money, toMajor, toMinor } from "@/lib/format";
import type { MenuItem } from "@/lib/api/types";
import { usePartner } from "../partner-context";

function PriceEditor({ item, onSave, readOnly }: { item: MenuItem; onSave: (paise: number) => Promise<boolean>; readOnly?: boolean }) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(toMajor(item.price));
  const [saving, setSaving] = useState(false);
  if (readOnly) return <span className="text-sm font-semibold">{money(item.price)}</span>;
  if (!editing)
    return (
      <button
        onClick={() => setEditing(true)}
        className="inline-flex items-center gap-1 text-sm font-semibold hover:text-brand"
        aria-label={`Edit price of ${item.name}`}
      >
        {money(item.price)} <Pencil className="h-3.5 w-3.5 text-muted" aria-hidden />
      </button>
    );
  return (
    <form
      className="flex items-center gap-1"
      onSubmit={async (e) => {
        e.preventDefault();
        setSaving(true);
        const ok = await onSave(toMinor(value));
        setSaving(false);
        if (ok) setEditing(false);
      }}
    >
      <span className="text-sm text-muted">{currencySymbol()}</span>
      <input
        value={value}
        onChange={(e) => setValue(e.target.value.replace(/[^\d.]/g, ""))}
        inputMode="decimal"
        autoFocus
        className="h-8 w-20 rounded-lg bg-canvas px-2 text-sm ring-1 ring-line outline-none focus:ring-2 focus:ring-brand"
        aria-label="New price"
      />
      <Button size="sm" type="submit" loading={saving} aria-label="Save price" className="!h-8 !px-2">
        <Check className="h-4 w-4" />
      </Button>
    </form>
  );
}

const MAX_PHOTO_BYTES = 5 * 1024 * 1024;

function photoLabel(i: MenuItem) {
  if (!i.image) return i.stockDeclined ? "No photo · diners see a drawing" : "No photo yet";
  if (i.imageSource === "OWNER") return "Your photo";
  return "Stock photo · upload your own";
}

/** Per-dish photo: upload/replace, remove, or ask Feast for a stock photo. */
function PhotoControl({ item, restaurantId, canEdit, onChanged }: { item: MenuItem; restaurantId: string; canEdit: boolean; onChanged: () => void }) {
  const toast = useToast();
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState<"upload" | "remove" | "find" | null>(null);
  const own = item.imageSource === "OWNER";

  const act = async (kind: "upload" | "remove" | "find", file?: File) => {
    setBusy(kind);
    try {
      const api = await getApi();
      if (kind === "upload" && file) await api.uploadMenuPhoto(restaurantId, item.id, file);
      if (kind === "remove") await api.removeMenuPhoto(restaurantId, item.id);
      if (kind === "find") await api.findMenuPhoto(restaurantId, item.id);
      toast(kind === "upload" ? `Photo added to ${item.name}` : kind === "remove" ? `Photo removed from ${item.name}` : `Found a photo for ${item.name}`);
      onChanged();
    } catch (e) {
      toast((e as Error).message || "Couldn't update the photo", "error");
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex items-center gap-2">
      {item.image ? (
        <DishPhoto item={item} className="h-14 w-14 shrink-0 rounded-lg" />
      ) : (
        <span className="grid h-14 w-14 shrink-0 place-items-center rounded-lg bg-canvas text-faint ring-1 ring-line" aria-hidden>
          <ImageOff className="h-5 w-5" />
        </span>
      )}
      <div className="min-w-0">
        <p className="text-xs text-muted">{photoLabel(item)}</p>
        {canEdit && (
          <div className="mt-1 flex flex-wrap gap-1">
            <input
              ref={input}
              type="file"
              accept="image/jpeg,image/png,image/webp"
              className="sr-only"
              aria-label={`Upload a photo of ${item.name}`}
              onChange={(e) => {
                const f = e.target.files?.[0];
                e.target.value = "";
                if (!f) return;
                if (f.size > MAX_PHOTO_BYTES) return toast("Photos must be under 5 MB", "error");
                void act("upload", f);
              }}
            />
            <Button size="sm" variant="secondary" loading={busy === "upload"} onClick={() => input.current?.click()} className="!h-7 !px-2 text-xs">
              <Camera className="h-3.5 w-3.5" aria-hidden /> {own ? "Replace" : "Upload"}
            </Button>
            {!own && !item.image && (
              <Button size="sm" variant="ghost" loading={busy === "find"} onClick={() => act("find")} className="!h-7 !px-2 text-xs">
                <Sparkles className="h-3.5 w-3.5" aria-hidden /> Find a photo
              </Button>
            )}
            {item.image && (
              <Button size="sm" variant="ghost" loading={busy === "remove"} onClick={() => act("remove")} className="!h-7 !px-2 text-xs" aria-label={`Remove the photo of ${item.name}`}>
                <Trash2 className="h-3.5 w-3.5" aria-hidden /> Remove
              </Button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export default function PartnerMenu() {
  const { restaurant, isStaff, canEdit } = usePartner();
  const toast = useToast();
  const [q, setQ] = useState("");
  const menu = useQuery(`pMenu:${restaurant.id}`, (api) => api.partnerMenu(restaurant.id));
  const patch = useMutation((api, itemId: string, p: { available?: boolean; price?: number }) => api.updateMenuItem(restaurant.id, itemId, p));

  const save = async (item: MenuItem, p: { available?: boolean; price?: number }) => {
    const res = await patch.run(item.id, p);
    if (res) {
      toast(p.available === undefined ? `${item.name} price updated` : `${item.name} is ${p.available ? "back in stock" : "out of stock"}`);
      menu.reload();
      return true;
    }
    toast("Couldn't save — check the value and try again", "error");
    return false;
  };

  if (menu.error) return <ErrorState error={menu.error} onRetry={menu.reload} />;
  if (!menu.data) return <Skeleton className="h-96 w-full" />;
  const term = q.trim().toLowerCase();
  const cats = menu.data.categories
    .map((c) => ({
      ...c,
      items: c.items.filter((i) => !term || i.name.toLowerCase().includes(term)),
    }))
    .filter((c) => c.items.length);
  const all = menu.data.categories.flatMap((c) => c.items);
  const out = all.filter((i) => !i.available).length;
  const hidden = all.filter((i) => i.moderation).length;
  const noPhoto = all.filter((i) => i.imageSource !== "OWNER").length;

  return (
    <div className="space-y-5">
      <PageHeader
        title="Menu"
        subtitle={`${out} item${out === 1 ? "" : "s"} out of stock${hidden ? ` · ${hidden} hidden by Feast` : ""}${noPhoto ? ` · ${noPhoto} without your photo` : ""} · changes show up for diners immediately`}
      />
      <label className="flex h-11 max-w-md items-center gap-2 rounded-xl bg-surface px-3 ring-1 ring-line focus-within:ring-2 focus-within:ring-brand">
        <Search className="h-4 w-4 text-muted" aria-hidden />
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Find a dish"
          className="w-full bg-transparent text-sm outline-none"
          aria-label="Find a dish"
        />
      </label>
      {cats.length === 0 && <Empty title="No dishes match" />}
      <div className="grid gap-4 lg:grid-cols-2">
        {cats.map((c) => (
          <section key={c.id} className="rounded-2xl bg-surface p-4 ring-1 ring-line">
            <h2 className="mb-1 font-bold">{c.name}</h2>
            <ul className="divide-y divide-line">
              {c.items.map((i) => (
                <li key={i.id} className="flex flex-wrap items-center gap-3 py-2">
                  <VegMark veg={i.isVeg} />
                  <div className="min-w-0 flex-1">
                    <p className={i.available && !i.moderation ? "font-semibold" : "font-semibold text-faint line-through"}>{i.name}</p>
                    <PriceEditor item={i} readOnly={!canEdit} onSave={(price) => save(i, { price })} />
                    {i.moderation && (
                      <ModerationNotice m={i.moderation} restaurantId={restaurant.id} kind="MENU_ITEM" id={i.id} canAppeal={!isStaff} onDone={menu.reload} />
                    )}
                  </div>
                  <div className="w-40">
                    <Toggle
                      checked={i.available}
                      disabled={!canEdit}
                      onChange={(v) => save(i, { available: v })}
                      label={i.available ? "In stock" : "Out of stock"}
                    />
                  </div>
                  <div className="basis-full">
                    <PhotoControl item={i} restaurantId={restaurant.id} canEdit={canEdit} onChanged={menu.reload} />
                  </div>
                  {isStaff && (
                    <ModerateButton restaurantId={restaurant.id} kind="MENU_ITEM" id={i.id} title={i.name} hidden={!!i.moderation} onDone={menu.reload} />
                  )}
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
