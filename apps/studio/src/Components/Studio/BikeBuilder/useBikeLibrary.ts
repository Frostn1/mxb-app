import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { useT } from "@/i18n";
import {
  PART_EXTENSIONS,
  addPart,
  listParts,
  removePart,
  setPartRole,
  setSlot,
  splitPart,
  type LibraryPart,
  type Role,
  type Slots,
} from "../../../api/bikebuild";

/** Radix's Select can't carry an empty value, so "no role" / "empty slot" is this. */
export const NONE = "none";

/**
 * The part tray and the bike's slots share one library: a part is run through Blender once,
 * when it's added, and both panels read the same list back. Split out of the panels
 * themselves (which used to be one component, `PartLibrary`) so the tray, the slots and the
 * preview can sit side by side instead of stacked, per Sean's "tray | slots | preview" ask.
 */
export function useBikeLibrary(version: number, onChanged: () => void) {
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
  const onSplit = (part: LibraryPart) => change(() => splitPart(part.id), "bike.splitFailed");

  const busy = adding !== null || changing;

  return { parts, slots, adding, busy, onAdd, add, onRole, onRemove, onSlot, onSplit };
}
