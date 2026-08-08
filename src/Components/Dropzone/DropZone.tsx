import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Loader2, PackageOpen } from "lucide-react";
import { toast } from "sonner";
import {
  cancelDrop,
  commitDrop,
  planDrop,
  repreviewDrop,
} from "../../api/mods";
import type { DropCommitItem, DropPlan } from "../../types";
import { useT } from "../../i18n/context";
import DropReview, { type RowState } from "./DropReview";

/**
 * Whole-window drop target.
 *
 * Uses Tauri's `onDragDropEvent` rather than the HTML5 drag events: with the default
 * `dragDropEnabled`, the OS handler consumes the drop and the webview never sees one — and
 * even if it did, an HTML5 `File` carries no filesystem path, which is the only thing the
 * installer can work with.
 *
 * Nothing is written when a drop lands. `planDrop` stages and classifies, the review sheet
 * shows what was found, and only `commitDrop` touches the game folder.
 */
export default function DropZone({ onInstalled }: { onInstalled?: () => void }) {
  const t = useT();
  const [hovering, setHovering] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [plan, setPlan] = useState<DropPlan | null>(null);
  const [rows, setRows] = useState<RowState[]>([]);
  const [installing, setInstalling] = useState(false);

  const onInstalledRef = useRef(onInstalled);
  onInstalledRef.current = onInstalled;
  const tRef = useRef(t);
  tRef.current = t;
  // A plan holds a staging directory; if a second drop lands we must release the first.
  const planRef = useRef<DropPlan | null>(null);
  planRef.current = plan;

  const reset = useCallback(() => {
    setPlan(null);
    setRows([]);
    setInstalling(false);
  }, []);

  const discard = useCallback(() => {
    const current = planRef.current;
    if (current) void cancelDrop(current.id).catch(() => {});
    reset();
  }, [reset]);

  const handlePaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    // Only one plan may be staged at a time — drop the previous one's temp files.
    const previous = planRef.current;
    if (previous) void cancelDrop(previous.id).catch(() => {});

    setScanning(true);
    try {
      const next = await planDrop(paths);
      if (next.items.length === 0) {
        setPlan(null);
        setRows([]);
        const why = next.skipped[0]?.reason;
        toast.error(tRef.current("drop.nothingUsable"), { description: why });
        void cancelDrop(next.id).catch(() => {});
        return;
      }
      setPlan(next);
      setRows(
        next.items.map((item) => ({
          item,
          keep: true,
          subpath: item.subpath,
          destFolder: item.destFolder,
          // A row that already has a destination shows it, rather than inviting a choice it
          // doesn't need; only genuinely undecided rows get the "choose one" placeholder.
          destLabel: item.needsChoice
            ? ""
            : (item.choices.find(
                (c) => c.value === item.destFolder && c.subpath === item.subpath,
              )?.label ??
              item.choices.find(
                (c) => c.value === "" && c.subpath === item.subpath,
              )?.label ??
              ""),
          picked: item.needsChoice ? "" : item.destFolder,
          fileCount: item.fileCount,
          collisions: item.collisions,
          busy: false,
        })),
      );
    } catch (e) {
      toast.error(tRef.current("drop.scanFailed"), { description: String(e) });
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setHovering(true);
        } else if (event.payload.type === "drop") {
          setHovering(false);
          void handlePaths(event.payload.paths);
        } else {
          setHovering(false);
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [handlePaths]);

  const toggle = useCallback((id: string) => {
    setRows((rs) =>
      rs.map((r) => (r.item.id === id ? { ...r, keep: !r.keep } : r)),
    );
  }, []);

  /** Picking a destination re-costs the row on the backend — file count and collisions
   *  both depend on where it lands, and guessing them here would let the sheet promise
   *  something the installer doesn't do. */
  const pick = useCallback(
    (id: string, value: string) => {
      const current = planRef.current;
      if (!current) return;
      const row = rows.find((r) => r.item.id === id);
      if (!row) return;

      // Each offered destination carries its own category. Only a folder the user typed
      // themselves has to be placed by shape, and gear is the one prefix worth honouring.
      const known = row.item.choices.find((c) => c.value === value);
      const subpath =
        known?.subpath ??
        (/^(helmets|boots|protection|riders)(\/|$)/.test(value)
          ? "mods/rider"
          : row.item.subpath || "mods/bikes");

      // The structural folder rides along: filing a bike under "MX2" must still leave it in
      // `mods/bikes/MX2/<Bike>/`, not scatter its configs into `MX2`.
      const keep = row.item.keepFolder;
      const destFolder = [value, keep && value !== keep ? keep : ""]
        .filter(Boolean)
        .join("/");

      setRows((rs) =>
        rs.map((r) =>
          r.item.id === id
            ? {
                ...r,
                subpath,
                destFolder,
                destLabel: known?.label ?? value,
                picked: value,
                busy: true,
              }
            : r,
        ),
      );

      void repreviewDrop(current.id, id, subpath, destFolder)
        .then((p) =>
          setRows((rs) =>
            rs.map((r) =>
              r.item.id === id
                ? {
                    ...r,
                    fileCount: p.fileCount,
                    collisions: p.collisions,
                    busy: false,
                  }
                : r,
            ),
          ),
        )
        .catch((e) => {
          setRows((rs) =>
            rs.map((r) => (r.item.id === id ? { ...r, busy: false } : r)),
          );
          toast.error(tRef.current("drop.previewFailed"), {
            description: String(e),
          });
        });
    },
    [rows],
  );

  const install = useCallback(async () => {
    const current = planRef.current;
    if (!current) return;
    const items: DropCommitItem[] = rows
      .filter((r) => r.keep && !(r.item.needsChoice && !r.subpath))
      .map((r) => ({
        id: r.item.id,
        subpath: r.subpath,
        destFolder: r.destFolder,
      }));
    if (items.length === 0) return;

    setInstalling(true);
    try {
      const outcome = await commitDrop(current.id, items);
      reset();
      if (outcome.installed.length > 0) {
        toast.success(
          tRef.current("drop.installed", {
            count: outcome.installed.length,
          }),
          {
            description: outcome.installed
              .map((i) => `${i.name} → ${i.dest}`)
              .join("\n"),
          },
        );
        onInstalledRef.current?.();
      }
      for (const f of outcome.failed) {
        toast.error(tRef.current("drop.itemFailed", { name: f.name }), {
          description: f.error,
          duration: Infinity,
        });
      }
    } catch (e) {
      setInstalling(false);
      toast.error(tRef.current("drop.installFailed"), {
        description: String(e),
      });
    }
  }, [rows, reset]);

  return (
    <>
      {(hovering || scanning) && !plan && (
        <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
          <div className="flex flex-col items-center gap-3 rounded-2xl border-2 border-dashed border-primary/50 bg-card/80 px-12 py-10">
            {scanning ? (
              <Loader2 className="size-8 animate-spin text-primary" />
            ) : (
              <PackageOpen className="size-8 text-primary" />
            )}
            <div className="text-center">
              <p className="text-[15px] font-bold">
                {scanning ? t("drop.scanning") : t("drop.dropHere")}
              </p>
              {!scanning && (
                <p className="mt-0.5 text-[12px] text-muted-foreground">
                  {t("drop.dropHint")}
                </p>
              )}
            </div>
          </div>
        </div>
      )}

      {plan && (
        <DropReview
          plan={plan}
          rows={rows}
          installing={installing}
          onToggle={toggle}
          onPick={pick}
          onCancel={discard}
          onInstall={() => void install()}
        />
      )}
    </>
  );
}
