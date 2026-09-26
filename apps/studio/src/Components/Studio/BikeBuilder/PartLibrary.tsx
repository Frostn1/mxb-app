import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Box, Loader2, Plus, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import {
  PART_EXTENSIONS,
  ROLES,
  addPart,
  listParts,
  removePart,
  setPartRole,
  setSlot,
  type LibraryPart,
  type Role,
  type Slots,
} from "../../../api/bikebuild";

/** Radix's Select can't carry an empty value, so "no role" / "empty slot" is this. */
const NONE = "none";

function Thumb({ part, className }: { part: LibraryPart | undefined; className: string }) {
  return part?.thumb ? (
    <img src={part.thumb} alt="" className={`${className} object-contain`} draggable={false} />
  ) : (
    <div className={`${className} flex items-center justify-center text-faint`}>
      <Box className="size-5" />
    </div>
  );
}

/**
 * The part tray and the bike's slots. A part is run through Blender once, when it's added:
 * that's where its thumbnail, attach empties and a first guess at its role come from. The
 * rider corrects the role if the guess is wrong, then puts one part in each slot.
 */
export default function PartLibrary({
  ready,
  version,
  onChanged,
}: {
  ready: boolean;
  /** Bumped when another panel changed the library. */
  version: number;
  onChanged: () => void;
}) {
  const t = useT();
  const [parts, setParts] = useState<LibraryPart[] | null>(null);
  const [slots, setSlots] = useState<Slots>({});
  /** What's going through Blender now: "2 of 5". Blender runs one job at a time. */
  const [adding, setAdding] = useState<{ n: number; of: number } | null>(null);
  /** A role, slot or remove change on its way: one at a time, so answers can't cross. */
  const [changing, setChanging] = useState(false);
  /** Only the newest read of the library is shown; an older one landing late is dropped. */
  const readSeq = useRef(0);

  const reload = useCallback(() => {
    const seq = ++readSeq.current;
    listParts()
      .then((lib) => {
        if (seq !== readSeq.current) return;
        setParts(lib.parts);
        setSlots(lib.slots);
      })
      .catch((e) => toast.error(t("bike.libraryFailed"), { description: String(e) }));
  }, [t]);

  /** Make one change to the library, then read it back whole: the backend may have moved
   * other things with it (a role change empties the slot the part no longer fits). */
  async function change(run: () => Promise<unknown>, failed: Parameters<typeof t>[0]) {
    setChanging(true);
    try {
      await run();
    } catch (e) {
      toast.error(t(failed), { description: String(e) });
    } finally {
      setChanging(false);
      onChanged();
    }
  }
  useEffect(() => reload(), [reload, version]);

  async function add(files: string[]) {
    let failed = 0;
    for (const [i, file] of files.entries()) {
      setAdding({ n: i + 1, of: files.length });
      try {
        await addPart(file);
      } catch (e) {
        failed++;
        toast.error(t("bike.addFailed", { name: file.split(/[\\/]/).pop() ?? file }), {
          description: String(e),
        });
      }
    }
    setAdding(null);
    if (failed < files.length) onChanged();
  }

  async function onAdd() {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: t("bike.partFiles"), extensions: PART_EXTENSIONS }],
    });
    const files = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (files.length) await add(files);
  }

  const onRole = (part: LibraryPart, value: string) =>
    change(() => setPartRole(part.id, value === NONE ? null : (value as Role)), "bike.roleFailed");
  const onRemove = (part: LibraryPart) => change(() => removePart(part.id), "bike.removeFailed");
  const onSlot = (role: Role, value: string) =>
    change(() => setSlot(role, value === NONE ? null : value), "bike.slotFailed");

  const byId = new Map((parts ?? []).map((p) => [p.id, p]));
  const busy = adding !== null || changing;

  return (
    <>
      <section className="flex flex-col gap-3 border border-border bg-card p-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("bike.parts")}
          </h2>
          <Button size="sm" className="ml-auto" onClick={onAdd} disabled={!ready || busy}>
            {adding ? <Loader2 className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
            {adding ? t("bike.adding", { n: adding.n, of: adding.of }) : t("bike.addParts")}
          </Button>
        </div>
        {parts === null ? null : parts.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("bike.partsEmpty")}</p>
        ) : (
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-2">
            {parts.map((p) => (
              <li key={p.id} className="flex flex-col gap-1.5 border border-border bg-background p-2">
                <Thumb part={p} className="aspect-square w-full bg-muted/40" />
                <p className="truncate text-sm font-medium" title={p.source}>
                  {p.name}
                </p>
                <p className="truncate text-[11px] text-muted-foreground">
                  {t("bike.partStats", {
                    tris: p.tris.toLocaleString(),
                    empties: p.empties.length,
                  })}
                </p>
                {(p.missing || p.stale) && (
                  <p className="flex items-center gap-1 text-[11px] text-amber-500">
                    <AlertTriangle className="size-3" />
                    {p.missing ? t("bike.partMissing") : t("bike.partStale")}
                  </p>
                )}
                <Select value={p.role ?? NONE} onValueChange={(v) => onRole(p, v)} disabled={busy}>
                  <SelectTrigger className="h-8">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={NONE}>{t("bike.noRole")}</SelectItem>
                    {ROLES.map((r) => (
                      <SelectItem key={r} value={r}>
                        {t(`bike.role.${r}`)}
                        {p.role === r && p.roleGuessed ? ` · ${t("bike.guessed")}` : ""}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <div className="flex gap-1">
                  {!p.missing && (
                    <Button
                      size="sm"
                      variant="ghost"
                      title={t("bike.refreshPart")}
                      onClick={() => add([p.source])}
                      disabled={!ready || busy}
                    >
                      <RefreshCw className="size-3.5" />
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    className="ml-auto"
                    title={t("bike.removePart")}
                    onClick={() => onRemove(p)}
                    disabled={busy}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="flex flex-col gap-3 border border-border bg-card p-4">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
          {t("bike.slots")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("bike.slotsHint")}</p>
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-2">
          {ROLES.map((role) => {
            const filled = slots[role] ? byId.get(slots[role]!) : undefined;
            const fits = (parts ?? []).filter((p) => p.role === role);
            return (
              <li key={role} className="flex items-center gap-2 border border-border bg-background p-2">
                <Thumb part={filled} className="size-12 shrink-0 bg-muted/40" />
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <span className="text-[11px] font-semibold uppercase tracking-[0.06em] text-faint">
                    {t(`bike.role.${role}`)}
                  </span>
                  <Select
                    value={filled?.id ?? NONE}
                    onValueChange={(v) => onSlot(role, v)}
                    disabled={busy || fits.length === 0}
                  >
                    <SelectTrigger className="h-8">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value={NONE}>
                        {fits.length === 0 ? t("bike.noPartsForRole") : t("bike.emptySlot")}
                      </SelectItem>
                      {fits.map((p) => (
                        <SelectItem key={p.id} value={p.id}>
                          {p.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </li>
            );
          })}
        </ul>
      </section>
    </>
  );
}
