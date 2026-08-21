import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { toast } from "sonner";
import { cancelDrop, commitDrop, repreviewDrop } from "../api/mods";
import type { DropCommitItem, DropPlan, NewDownload } from "../types";
import { useDownloads } from "./Downloads";
import { useT } from "../i18n/context";
import DropReview, { type RowState } from "../Components/Dropzone/DropReview";

/**
 * The staged-install review sheet, and the one place that owns it.
 *
 * A plan is a *staged* install: the backend has extracted and classified something into a
 * temp directory and written nothing under `mods/`. The sheet shows what was found and where
 * each piece would land, and only `commitDrop` touches the game folder.
 *
 * This lived inside `DropZone` while drag-and-drop was the only way to produce a plan. It is
 * a provider now because purchases produce one too — `shop_stage` downloads a bought file and
 * returns the same `DropPlan` — and both paths deserve the same review, the same collision
 * warnings, the same destination override and the same commit. Hoisting the state was the
 * alternative to a second, parallel install path that would drift.
 *
 * One plan at a time: each holds a staging directory, so staging a second releases the first.
 */
interface DropReviewContextValue {
  /**
   * Put a staged plan up for review. Handles the empty plan itself (says so and releases the
   * staging directory), so callers only have to produce one.
   */
  reviewPlan: (plan: DropPlan) => void;
  /** Whether a sheet is currently up — lets a caller suppress its own overlay behind it. */
  reviewing: boolean;
}

const DropReviewContext = createContext<DropReviewContextValue | null>(null);

export function DropReviewProvider({
  onInstalled,
  children,
}: {
  /** Fired once per commit that installed something, so library views can re-scan. */
  onInstalled?: () => void;
  children: ReactNode;
}) {
  const t = useT();
  const [plan, setPlan] = useState<DropPlan | null>(null);
  const [rows, setRows] = useState<RowState[]>([]);
  const [installing, setInstalling] = useState(false);

  const onInstalledRef = useRef(onInstalled);
  onInstalledRef.current = onInstalled;
  const tRef = useRef(t);
  tRef.current = t;
  const { note } = useDownloads();
  // A plan holds a staging directory; if a second one arrives we must release the first.
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

  const reviewPlan = useCallback<DropReviewContextValue["reviewPlan"]>((next) => {
    // Only one plan may be staged at a time — drop the previous one's temp files.
    const previous = planRef.current;
    if (previous && previous.id !== next.id) {
      void cancelDrop(previous.id).catch(() => {});
    }

    if (next.items.length === 0) {
      setPlan(null);
      setRows([]);
      toast.error(tRef.current("drop.nothingUsable"), {
        description: next.skipped[0]?.reason,
      });
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
            item.choices.find((c) => c.value === "" && c.subpath === item.subpath)
              ?.label ??
            ""),
        picked: item.needsChoice ? "" : item.destFolder,
        fileCount: item.fileCount,
        collisions: item.collisions,
        busy: false,
      })),
    );
  }, []);

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

      // One history row per item, from the committed request rather than the receipt: the
      // receipt shows a display path, and the page needs the real subpath to route back to
      // the library. A dropped file has no mod page, hence the empty slug.
      const asked = new Map(items.map((i) => [i.id, i]));
      const forHistory = (id: string, name: string): NewDownload => ({
        title: name,
        slug: "",
        subpath: asked.get(id)?.subpath ?? "",
        destFolder: asked.get(id)?.destFolder ?? "",
        categoryId: null,
        source: "file",
        host: null,
        url: null,
        bytes: rows.find((r) => r.item.id === id)?.item.bytes ?? null,
        status: "installed",
        error: null,
      });
      for (const i of outcome.installed) note(forHistory(i.id, i.name));
      for (const f of outcome.failed) {
        note({ ...forHistory(f.id, f.name), status: "failed", error: f.error });
      }

      reset();
      if (outcome.installed.length > 0) {
        toast.success(
          tRef.current("drop.installed", { count: outcome.installed.length }),
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
  }, [rows, reset, note]);

  const value = useMemo(
    () => ({ reviewPlan, reviewing: plan !== null }),
    [reviewPlan, plan],
  );

  return (
    <DropReviewContext.Provider value={value}>
      {children}
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
    </DropReviewContext.Provider>
  );
}

export function useDropReview(): DropReviewContextValue {
  const ctx = useContext(DropReviewContext);
  if (!ctx) {
    throw new Error("useDropReview must be used inside a DropReviewProvider");
  }
  return ctx;
}
